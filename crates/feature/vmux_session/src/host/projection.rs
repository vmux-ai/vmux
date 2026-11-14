use std::collections::HashMap;
use vmux_api::chat::{
    ChatActivityKind, ChatBlock, ChatDiff, ChatDiffLine, ChatDiffLineKind, ChatItem, ChatPlanItem,
    ChatPlanStatus, ChatSubagent, ChatSubagentState, ChatSubagentStatus, ChatSubagentSummary,
    ChatToolArgument, ChatToolArgumentValue, ChatToolArguments, ChatToolCall, ChatToolChild,
    ChatToolChildCall, ChatToolKind, ChatTurn, ChatTurnRow,
};
use vmux_ui::LanguageIconPath;

pub(super) struct ChatTurnProjection;

impl ChatTurnProjection {
    pub(super) fn apply(turn: &mut ChatTurn) {
        let mut copy_text = Vec::new();
        let mut active_subagents = 0u32;
        let mut active_tasks = 0u32;
        for block in &turn.blocks {
            match block {
                ChatBlock::Text(text) if !text.is_empty() => copy_text.push(text.as_str()),
                ChatBlock::Subagent(subagent) if subagent.status == "in_progress" => {
                    active_subagents = active_subagents.saturating_add(1);
                }
                ChatBlock::Plan { steps } => {
                    for step in steps {
                        if step.status != "completed" {
                            active_tasks = active_tasks.saturating_add(1);
                        }
                    }
                }
                _ => {}
            }
        }
        turn.copy_text = copy_text.join("\n\n");
        turn.active_subagents = active_subagents;
        turn.active_tasks = active_tasks;
        turn.activity = if turn.running {
            turn.blocks
                .last()
                .map(ChatTurnProjection::activity_for_block)
                .unwrap_or(ChatActivityKind::Thinking)
        } else {
            ChatActivityKind::None
        };

        let parents = ToolParents::new(&turn.blocks);
        turn.step_count = 0;
        for (index, block) in turn.blocks.iter().enumerate() {
            if !matches!(block, ChatBlock::Text(_)) && parents.roots[index].is_none() {
                turn.step_count = turn.step_count.saturating_add(1);
            }
        }
        let latest_tool = if turn.running {
            turn.blocks
                .iter()
                .enumerate()
                .rev()
                .find(|(index, block)| {
                    matches!(block, ChatBlock::ToolUse { .. }) && parents.roots[*index].is_none()
                })
                .map(|(index, _)| index)
        } else {
            None
        };
        let block_count = turn.blocks.len();
        let mut rows = Vec::new();
        for (index, block) in turn.blocks.iter().enumerate() {
            if parents.roots[index].is_some() {
                continue;
            }
            let Some(row) = ChatTurnProjection::project_row(
                turn,
                index,
                block,
                block_count,
                latest_tool,
                &parents.children[index],
            ) else {
                continue;
            };
            let ChatTurnRow::Tool(call) = row else {
                rows.push(row);
                continue;
            };
            if call.live {
                rows.push(ChatTurnRow::Tool(call));
                continue;
            }
            match rows.last_mut() {
                Some(ChatTurnRow::FinishedTools { calls, .. }) => calls.push(call),
                _ => rows.push(ChatTurnRow::FinishedTools {
                    index: call.index,
                    calls: vec![call],
                }),
            }
        }
        turn.rows = rows;
    }

    pub(super) fn activity_counts(items: &[ChatItem]) -> (u32, u32) {
        let mut subagents = 0u32;
        let mut tasks = 0u32;
        for item in items {
            let ChatItem::Turn(turn) = item else {
                continue;
            };
            subagents = subagents.saturating_add(turn.active_subagents);
            tasks = tasks.saturating_add(turn.active_tasks);
        }
        (subagents, tasks)
    }

    fn project_row(
        turn: &ChatTurn,
        index: usize,
        block: &ChatBlock,
        block_count: usize,
        latest_tool: Option<usize>,
        children: &[usize],
    ) -> Option<ChatTurnRow> {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        match block {
            ChatBlock::Text(text) => Some(ChatTurnRow::Text {
                index,
                text: text.clone(),
            }),
            ChatBlock::Thinking(text) => Some(ChatTurnRow::Thinking {
                index,
                text: text.clone(),
                latest: index as usize + 1 == block_count,
            }),
            ChatBlock::ToolUse {
                name, args, kind, ..
            } => Some(ChatTurnRow::Tool(ChatTurnProjection::project_tool(
                turn,
                index,
                name,
                args,
                *kind,
                latest_tool == Some(index as usize),
                children,
            ))),
            ChatBlock::Subagent(subagent) => Some(ChatTurnRow::Subagent(
                ChatTurnProjection::project_subagent(turn, index, subagent, children),
            )),
            ChatBlock::Diff {
                path,
                old_text,
                new_text,
                ..
            } => Some(ChatTurnRow::Diff(ChatTurnProjection::project_diff(
                index,
                path,
                old_text.as_deref().unwrap_or_default(),
                new_text,
            ))),
            ChatBlock::Plan { steps } => Some(ChatTurnRow::Plan {
                index,
                steps: steps
                    .iter()
                    .map(|step| ChatPlanItem {
                        content: step.content.clone(),
                        status: ChatTurnProjection::plan_status(&step.status),
                    })
                    .collect(),
            }),
            ChatBlock::ToolResult {
                content, is_error, ..
            } => Some(ChatTurnRow::ToolResult {
                index,
                content: content.clone(),
                is_error: *is_error,
            }),
            ChatBlock::Reconnect { attempt, total } => Some(ChatTurnRow::Reconnect {
                index,
                attempt: *attempt,
                total: *total,
            }),
        }
    }

    fn project_tool(
        turn: &ChatTurn,
        index: u32,
        name: &str,
        args: &str,
        kind: Option<vmux_api::session::ToolKind>,
        live: bool,
        children: &[usize],
    ) -> ChatToolCall {
        ChatToolCall {
            index,
            name: name.to_string(),
            kind: ChatTurnProjection::classified_kind(kind, name, args),
            activity: kind
                .and_then(|kind| Self::classified_activity(kind, name))
                .unwrap_or_else(|| Self::activity_for_tool(name, args)),
            fallback_label: ChatTurnProjection::fallback_tool_label(name),
            file_path: ChatTurnProjection::tool_file_path(args),
            arguments: ChatTurnProjection::tool_arguments(args),
            children: ChatTurnProjection::tool_children(turn, children),
            live,
        }
    }

    fn project_child_tool(
        index: usize,
        name: &str,
        args: &str,
        kind: Option<vmux_api::session::ToolKind>,
    ) -> ChatToolChildCall {
        ChatToolChildCall {
            index: u32::try_from(index).unwrap_or(u32::MAX),
            name: name.to_string(),
            kind: ChatTurnProjection::classified_kind(kind, name, args),
            activity: kind
                .and_then(|kind| Self::classified_activity(kind, name))
                .unwrap_or_else(|| Self::activity_for_tool(name, args)),
            fallback_label: ChatTurnProjection::fallback_tool_label(name),
            file_path: ChatTurnProjection::tool_file_path(args),
            arguments: ChatTurnProjection::tool_arguments(args),
        }
    }

    fn tool_children(turn: &ChatTurn, indices: &[usize]) -> Vec<ChatToolChild> {
        let mut children = Vec::new();
        for &index in indices {
            let block = &turn.blocks[index];
            match block {
                ChatBlock::ToolUse {
                    name, args, kind, ..
                } => {
                    children.push(ChatToolChild::Tool(ChatTurnProjection::project_child_tool(
                        index, name, args, *kind,
                    )));
                }
                ChatBlock::Subagent(subagent) => {
                    children.push(ChatToolChild::Subagent(
                        ChatTurnProjection::project_subagent_summary(index, subagent),
                    ));
                }
                ChatBlock::ToolResult {
                    content, is_error, ..
                } => children.push(ChatToolChild::Result {
                    index: u32::try_from(index).unwrap_or(u32::MAX),
                    content: content.clone(),
                    is_error: *is_error,
                }),
                _ => {}
            }
        }
        children
    }

    fn project_subagent(
        turn: &ChatTurn,
        index: u32,
        subagent: &ChatSubagent,
        children: &[usize],
    ) -> ChatSubagentState {
        ChatSubagentState {
            index,
            call_id: subagent.call_id.clone(),
            provider: subagent.provider.clone(),
            title: subagent.title.replace('_', " "),
            status: ChatTurnProjection::subagent_status(&subagent.status),
            activity: subagent.activity.replace('_', " "),
            agent_name: subagent.agent_name.clone(),
            thread_id: subagent.thread_id.clone(),
            parent_thread_id: subagent.parent_thread_id.clone(),
            child_threads: subagent.child_thread_ids.join(", "),
            prompt: subagent.prompt.clone(),
            model: subagent.model.clone(),
            reasoning_effort: subagent.reasoning_effort.clone(),
            raw_input: subagent.raw_input.clone(),
            children: ChatTurnProjection::tool_children(turn, children),
        }
    }

    fn project_subagent_summary(index: usize, subagent: &ChatSubagent) -> ChatSubagentSummary {
        ChatSubagentSummary {
            index: u32::try_from(index).unwrap_or(u32::MAX),
            title: subagent.title.replace('_', " "),
            status: ChatTurnProjection::subagent_status(&subagent.status),
            provider: subagent.provider.clone(),
            agent_name: subagent.agent_name.clone(),
            prompt: subagent.prompt.clone(),
        }
    }

    fn project_diff(index: u32, path: &str, old_text: &str, new_text: &str) -> ChatDiff {
        let mut lines = Vec::new();
        for change in similar::TextDiff::from_lines(old_text, new_text).iter_all_changes() {
            let kind = match change.tag() {
                similar::ChangeTag::Delete => ChatDiffLineKind::Removed,
                similar::ChangeTag::Insert => ChatDiffLineKind::Added,
                similar::ChangeTag::Equal => continue,
            };
            lines.push(ChatDiffLine {
                kind,
                text: change.value().trim_end_matches('\n').to_string(),
            });
        }
        ChatDiff {
            index,
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            lines,
        }
    }

    fn tool_arguments(args: &str) -> ChatToolArguments {
        if args.is_empty() || args == "{}" {
            return ChatToolArguments::None;
        }
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(args) else {
            return ChatToolArguments::Raw(args.to_string());
        };
        while let serde_json::Value::Object(map) = &value {
            let Some(arguments) = map.get("arguments") else {
                break;
            };
            if map.contains_key("server") || map.contains_key("tool") || map.contains_key("name") {
                value = arguments.clone();
            } else {
                break;
            }
        }
        match value {
            serde_json::Value::Object(map) if map.is_empty() => ChatToolArguments::None,
            serde_json::Value::Object(map) => ChatToolArguments::Fields(
                map.into_iter()
                    .map(|(name, value)| ChatTurnProjection::tool_argument(name, value))
                    .collect(),
            ),
            value => ChatToolArguments::Value(ChatTurnProjection::tool_argument_value("", value)),
        }
    }

    fn tool_argument(name: String, value: serde_json::Value) -> ChatToolArgument {
        ChatToolArgument {
            label: ChatTurnProjection::tool_argument_label(&name),
            value: ChatTurnProjection::tool_argument_value(&name, value),
            name,
        }
    }

    fn tool_argument_value(name: &str, value: serde_json::Value) -> ChatToolArgumentValue {
        match value {
            serde_json::Value::String(value)
                if ChatTurnProjection::tool_argument_is_path(name, &value) =>
            {
                ChatToolArgumentValue::Path(value)
            }
            serde_json::Value::String(value)
                if matches!(
                    name,
                    "cmd" | "command" | "script" | "patch" | "text" | "content"
                ) || value.contains('\n') =>
            {
                ChatToolArgumentValue::Code(value)
            }
            serde_json::Value::String(value) => ChatToolArgumentValue::Text(value),
            serde_json::Value::Bool(value) => ChatToolArgumentValue::Bool(value),
            serde_json::Value::Number(value) => ChatToolArgumentValue::Number(value.to_string()),
            serde_json::Value::Array(values) => ChatToolArgumentValue::List(
                values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        ChatTurnProjection::tool_argument((index + 1).to_string(), value)
                    })
                    .collect(),
            ),
            serde_json::Value::Object(values) => ChatToolArgumentValue::Object(
                values
                    .into_iter()
                    .map(|(name, value)| ChatTurnProjection::tool_argument(name, value))
                    .collect(),
            ),
            serde_json::Value::Null => ChatToolArgumentValue::Null,
        }
    }

    fn tool_argument_label(name: &str) -> String {
        let mut label = name.replace('_', " ");
        if let Some(first) = label.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        label
    }

    fn tool_argument_is_path(name: &str, value: &str) -> bool {
        matches!(
            name,
            "path" | "file" | "file_path" | "cwd" | "dir" | "directory" | "workdir"
        ) || value.starts_with('/')
    }

    fn classified_kind(
        kind: Option<vmux_api::session::ToolKind>,
        name: &str,
        args: &str,
    ) -> ChatToolKind {
        match kind {
            Some(vmux_api::session::ToolKind::Think) if Self::is_guardian(name) => {
                ChatToolKind::Guardian
            }
            Some(vmux_api::session::ToolKind::Read) => {
                if Self::arguments_read_skill(args) {
                    ChatToolKind::ReadSkill
                } else {
                    ChatToolKind::ReadFile
                }
            }
            Some(
                vmux_api::session::ToolKind::Edit
                | vmux_api::session::ToolKind::Delete
                | vmux_api::session::ToolKind::Move,
            ) => ChatToolKind::WriteFile,
            Some(vmux_api::session::ToolKind::Search) => ChatToolKind::Search,
            Some(vmux_api::session::ToolKind::Execute) => ChatToolKind::Command,
            Some(vmux_api::session::ToolKind::Fetch) => ChatToolKind::Browser,
            Some(vmux_api::session::ToolKind::Think | vmux_api::session::ToolKind::SwitchMode) => {
                ChatToolKind::Other
            }
            _ => Self::tool_kind(name, args),
        }
    }

    fn classified_activity(
        kind: vmux_api::session::ToolKind,
        name: &str,
    ) -> Option<ChatActivityKind> {
        Some(match kind {
            vmux_api::session::ToolKind::Read => ChatActivityKind::ReadFile,
            vmux_api::session::ToolKind::Edit
            | vmux_api::session::ToolKind::Delete
            | vmux_api::session::ToolKind::Move => ChatActivityKind::WriteFile,
            vmux_api::session::ToolKind::Search => ChatActivityKind::Search,
            vmux_api::session::ToolKind::Execute => ChatActivityKind::Command,
            vmux_api::session::ToolKind::Think if Self::is_guardian(name) => {
                ChatActivityKind::Guardian
            }
            vmux_api::session::ToolKind::Think => ChatActivityKind::Thinking,
            vmux_api::session::ToolKind::Fetch => ChatActivityKind::Browser,
            _ => return None,
        })
    }

    fn tool_kind(name: &str, args: &str) -> ChatToolKind {
        let lower = name.to_ascii_lowercase();
        if ChatTurnProjection::is_guardian(name) {
            ChatToolKind::Guardian
        } else if lower.contains("read_file")
            || lower == "read"
            || lower.starts_with("read ")
            || lower.ends_with("readfile")
            || lower.contains("read file")
            || lower.contains("open_file")
            || lower.contains("open file")
        {
            if ChatTurnProjection::arguments_read_skill(args) {
                ChatToolKind::ReadSkill
            } else {
                ChatToolKind::ReadFile
            }
        } else if matches!(lower.as_str(), "edit" | "write")
            || lower.contains("editing file")
            || lower.contains("edited file")
            || lower.contains("write file")
            || lower.contains("apply_patch")
            || lower.contains("edit_file")
            || lower.contains("write_file")
            || lower.contains("multi_edit")
        {
            ChatToolKind::WriteFile
        } else if lower.contains("worktree")
            || lower.contains("workspace")
            || lower == "select_project"
            || lower.contains("repository")
        {
            ChatToolKind::Worktree
        } else if lower.contains("layout")
            || lower.contains("list_spaces")
            || lower.contains("create_space")
            || lower.contains("rename_space")
            || lower.contains("delete_space")
        {
            ChatToolKind::Layout
        } else if lower.contains("screenshot") {
            ChatToolKind::Screenshot
        } else if lower.contains("open_page") || lower.contains("open page") {
            ChatToolKind::OpenPage
        } else if lower.contains("view_image") || lower.contains("view image") {
            ChatToolKind::Image
        } else if lower.contains("browser") || lower.contains("navigate") || lower.contains("web_")
        {
            ChatToolKind::Browser
        } else if lower.contains("grep")
            || lower.contains("search")
            || lower.contains("find")
            || matches!(lower.as_str(), "glob" | "ls")
            || lower.contains("list_files")
            || lower.contains("list_directory")
        {
            ChatToolKind::Search
        } else if lower == "bash"
            || lower.ends_with(".bash")
            || lower.contains("run")
            || lower.contains("exec")
            || lower.contains("command")
            || lower.contains("shell")
            || lower.contains("terminal")
        {
            ChatToolKind::Command
        } else {
            ChatToolKind::Other
        }
    }

    fn activity_for_tool(name: &str, args: &str) -> ChatActivityKind {
        if let Some(path) = ChatTurnProjection::tool_file_path(args)
            .and_then(|path| LanguageIconPath::from_hint(&path))
        {
            return ChatActivityKind::Language(path);
        }
        if let Some(path) =
            LanguageIconPath::from_text(name).or_else(|| LanguageIconPath::from_text(args))
        {
            return ChatActivityKind::Language(path);
        }
        match ChatTurnProjection::tool_kind(name, args) {
            ChatToolKind::Guardian => ChatActivityKind::Guardian,
            ChatToolKind::ReadFile | ChatToolKind::ReadSkill => ChatActivityKind::ReadFile,
            ChatToolKind::WriteFile => ChatActivityKind::WriteFile,
            ChatToolKind::Layout => ChatActivityKind::Layout,
            ChatToolKind::Worktree => ChatActivityKind::Worktree,
            ChatToolKind::Image => ChatActivityKind::Image,
            ChatToolKind::Screenshot => ChatActivityKind::Screenshot,
            ChatToolKind::OpenPage => ChatActivityKind::OpenPage,
            ChatToolKind::Browser => ChatActivityKind::Browser,
            ChatToolKind::Search => ChatActivityKind::Search,
            ChatToolKind::Command => ChatActivityKind::Command,
            ChatToolKind::Other => ChatActivityKind::Tool,
        }
    }

    fn activity_for_block(block: &ChatBlock) -> ChatActivityKind {
        match block {
            ChatBlock::Text(_) => ChatActivityKind::Writing,
            ChatBlock::Thinking(_) => ChatActivityKind::Thinking,
            ChatBlock::ToolUse {
                name, args, kind, ..
            } => kind
                .and_then(|kind| Self::classified_activity(kind, name))
                .unwrap_or_else(|| Self::activity_for_tool(name, args)),
            ChatBlock::Subagent(_) => ChatActivityKind::Subagent,
            ChatBlock::Diff { path, .. } => {
                if let Some(path) = LanguageIconPath::from_hint(path) {
                    ChatActivityKind::Language(path)
                } else {
                    ChatActivityKind::Diff
                }
            }
            ChatBlock::Plan { .. } => ChatActivityKind::Plan,
            ChatBlock::ToolResult { is_error: true, .. } => ChatActivityKind::Error,
            ChatBlock::ToolResult { .. } => ChatActivityKind::Output,
            ChatBlock::Reconnect { .. } => ChatActivityKind::Reconnect,
        }
    }

    fn subagent_status(status: &str) -> ChatSubagentStatus {
        match status {
            "in_progress" => ChatSubagentStatus::Running,
            "completed" => ChatSubagentStatus::Complete,
            "failed" => ChatSubagentStatus::Failed,
            _ => ChatSubagentStatus::Pending,
        }
    }

    fn plan_status(status: &str) -> ChatPlanStatus {
        match status {
            "completed" => ChatPlanStatus::Complete,
            "in_progress" => ChatPlanStatus::Active,
            _ => ChatPlanStatus::Pending,
        }
    }

    fn fallback_tool_label(name: &str) -> String {
        name.rsplit(['.', ':'])
            .next()
            .unwrap_or(name)
            .replace('_', " ")
    }

    fn arguments_read_skill(args: &str) -> bool {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(args) else {
            return false;
        };
        while let serde_json::Value::Object(map) = &value {
            let Some(arguments) = map.get("arguments") else {
                break;
            };
            if map.contains_key("server") || map.contains_key("tool") || map.contains_key("name") {
                value = arguments.clone();
            } else {
                break;
            }
        }
        ChatTurnProjection::contains_skill_path(&value)
    }

    fn contains_skill_path(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(map) => map.iter().any(|(key, value)| {
                matches!(key.as_str(), "path" | "file" | "file_path" | "filename")
                    && value.as_str().is_some_and(|path| {
                        std::path::Path::new(path)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
                    })
                    || ChatTurnProjection::contains_skill_path(value)
            }),
            serde_json::Value::Array(values) => {
                values.iter().any(ChatTurnProjection::contains_skill_path)
            }
            _ => false,
        }
    }

    fn tool_file_path(args: &str) -> Option<String> {
        if let Ok(value) = serde_json::from_str(args)
            && let Some(path) = ChatTurnProjection::file_path_from_value(&value)
        {
            return Some(path);
        }
        ChatTurnProjection::file_path_from_text(args)
    }

    fn file_path_from_value(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::Object(map) => {
                for key in ["path", "file_path", "filename", "file"] {
                    if let Some(path) = map.get(key).and_then(serde_json::Value::as_str)
                        && !path.trim().is_empty()
                    {
                        return Some(path.to_string());
                    }
                }
                map.values()
                    .find_map(ChatTurnProjection::file_path_from_value)
            }
            serde_json::Value::Array(values) => values
                .iter()
                .find_map(ChatTurnProjection::file_path_from_value),
            serde_json::Value::String(text) => ChatTurnProjection::file_path_from_text(text),
            _ => None,
        }
    }

    fn file_path_from_text(text: &str) -> Option<String> {
        for marker in ["*** Update File: ", "*** Add File: ", "*** Delete File: "] {
            if let Some(path) = text.lines().find_map(|line| line.strip_prefix(marker)) {
                return Some(path.trim().to_string());
            }
        }
        text.split_whitespace()
            .map(|token| token.trim_matches(['"', '\'', ',', ':', ';', '(', ')']))
            .find(|token| {
                if token.contains("://") {
                    return false;
                }
                let name = token.rsplit('/').next().unwrap_or(token);
                name.rsplit_once('.')
                    .is_some_and(|(_, extension)| !extension.is_empty() && extension.len() <= 12)
            })
            .map(ToOwned::to_owned)
    }

    fn is_guardian(name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        lower.contains("guardian")
            || lower.contains("approval")
            || lower == "review"
            || lower.ends_with("_review")
            || lower.ends_with(".review")
            || lower.ends_with(":review")
    }
}

struct ToolParents {
    roots: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
}

impl ToolParents {
    fn new(blocks: &[ChatBlock]) -> Self {
        let mut calls = HashMap::new();
        for (index, block) in blocks.iter().enumerate() {
            let call_id = match block {
                ChatBlock::ToolUse { call_id, .. } => Some(call_id.as_str()),
                ChatBlock::Subagent(subagent) => Some(subagent.call_id.as_str()),
                _ => None,
            };
            if let Some(call_id) = call_id {
                calls.entry(call_id).or_insert(index);
            }
        }
        let mut direct = Vec::with_capacity(blocks.len());
        let mut adjacent = None;
        for (index, block) in blocks.iter().enumerate() {
            let guardian = matches!(block, ChatBlock::ToolUse { name, .. } if ChatTurnProjection::is_guardian(name));
            let parent = match block {
                ChatBlock::ToolUse {
                    parent_call_id: Some(parent),
                    ..
                } => calls.get(parent.as_str()).copied(),
                ChatBlock::Subagent(subagent) => subagent
                    .parent_call_id
                    .as_deref()
                    .and_then(|parent| calls.get(parent).copied()),
                ChatBlock::ToolUse { .. } if guardian => adjacent,
                ChatBlock::ToolResult { call_id, .. } if !call_id.is_empty() => {
                    calls.get(call_id.as_str()).copied()
                }
                _ => None,
            };
            direct.push(parent);
            match block {
                ChatBlock::ToolUse { .. } if guardian => {}
                ChatBlock::ToolUse { .. } | ChatBlock::Subagent(_) => adjacent = Some(index),
                _ => adjacent = None,
            }
        }
        let mut roots = vec![None; blocks.len()];
        let mut children = vec![Vec::new(); blocks.len()];
        for index in 0..blocks.len() {
            let Some(mut parent) = direct[index] else {
                continue;
            };
            for _ in 0..blocks.len() {
                let Some(next) = direct[parent] else {
                    break;
                };
                if next == parent {
                    break;
                }
                parent = next;
            }
            roots[index] = Some(parent);
            children[parent].push(index);
        }
        Self { roots, children }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guardian_authorization_uses_its_own_activity_instead_of_general_thinking() {
        for (name, protocol, kind, activity) in [
            (
                "Guardian Review",
                vmux_api::session::ToolKind::Think,
                ChatToolKind::Guardian,
                ChatActivityKind::Guardian,
            ),
            (
                "Consider the next step",
                vmux_api::session::ToolKind::Think,
                ChatToolKind::Other,
                ChatActivityKind::Thinking,
            ),
            (
                "guardian_read_policy",
                vmux_api::session::ToolKind::Read,
                ChatToolKind::ReadFile,
                ChatActivityKind::ReadFile,
            ),
        ] {
            let mut turn = ChatTurn {
                running: true,
                blocks: vec![ChatBlock::ToolUse {
                    call_id: "call".into(),
                    name: name.into(),
                    args: "{}".into(),
                    parent_call_id: None,
                    kind: Some(protocol),
                }],
                ..Default::default()
            };
            ChatTurnProjection::apply(&mut turn);
            let ChatTurnRow::Tool(call) = &turn.rows[0] else {
                panic!("tool row");
            };
            assert_eq!(call.kind, kind);
            assert_eq!(call.activity, activity);
        }
    }

    #[test]
    fn typed_tool_kind_takes_precedence_over_an_agent_specific_title() {
        let mut turn = ChatTurn {
            running: true,
            blocks: vec![ChatBlock::ToolUse {
                call_id: "call".into(),
                name: "execute command".into(),
                args: "{}".into(),
                parent_call_id: None,
                kind: Some(vmux_api::session::ToolKind::Read),
            }],
            ..Default::default()
        };
        ChatTurnProjection::apply(&mut turn);
        let ChatTurnRow::Tool(call) = &turn.rows[0] else {
            panic!("tool row");
        };
        assert_eq!(call.kind, ChatToolKind::ReadFile);
        assert_eq!(call.activity, ChatActivityKind::ReadFile);
    }

    #[test]
    fn claude_and_vibe_tool_names_share_command_read_and_search_classification() {
        for name in ["Bash", "run_command", "exec_command"] {
            assert_eq!(
                ChatTurnProjection::tool_kind(name, r#"{"command":"rg foo src"}"#),
                ChatToolKind::Command
            );
        }
        for name in ["Read", "read_file", "Read src/main.rs"] {
            assert_eq!(
                ChatTurnProjection::tool_kind(name, r#"{"file_path":"src/main.rs"}"#),
                ChatToolKind::ReadFile
            );
        }
        for name in ["Glob", "Grep", "list_directory"] {
            assert_eq!(
                ChatTurnProjection::tool_kind(name, "{}"),
                ChatToolKind::Search
            );
        }
    }

    #[test]
    fn projects_nested_tools_typed_arguments_and_finished_groups() {
        let mut turn = ChatTurn {
            blocks: vec![
                ChatBlock::ToolUse {
                    call_id: "read".into(),
                    name: "read_file".into(),
                    args: r#"{"arguments":{"path":"/tmp/SKILL.md","limit":3},"server":"vmux"}"#
                        .into(),
                    parent_call_id: None,

                    kind: None,
                },
                ChatBlock::ToolResult {
                    call_id: "read".into(),
                    content: "done".into(),
                    is_error: false,
                },
                ChatBlock::ToolUse {
                    call_id: "run".into(),
                    name: "run".into(),
                    args: r#"{"command":"cargo test"}"#.into(),
                    parent_call_id: None,

                    kind: None,
                },
            ],
            ..Default::default()
        };

        ChatTurnProjection::apply(&mut turn);

        let ChatTurnRow::FinishedTools { calls, .. } = &turn.rows[0] else {
            panic!("expected folded tools");
        };
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].kind, ChatToolKind::ReadSkill);
        assert_eq!(calls[0].file_path.as_deref(), Some("/tmp/SKILL.md"));
        assert_eq!(calls[0].children.len(), 1);
        assert!(matches!(
            &calls[0].arguments,
            ChatToolArguments::Fields(fields)
                if fields
                    .iter()
                    .any(|field| matches!(field.value, ChatToolArgumentValue::Path(_)))
        ));
        assert!(matches!(
            &calls[1].arguments,
            ChatToolArguments::Fields(fields)
                if fields
                    .iter()
                    .any(|field| matches!(field.value, ChatToolArgumentValue::Code(_)))
        ));
    }

    #[test]
    fn live_tool_stays_expanded_and_diff_lines_are_precomputed() {
        let mut turn = ChatTurn {
            blocks: vec![
                ChatBlock::ToolUse {
                    call_id: "run".into(),
                    name: "python".into(),
                    args: r#"{"script":"print(1)"}"#.into(),
                    parent_call_id: None,

                    kind: None,
                },
                ChatBlock::Diff {
                    call_id: "diff".into(),
                    path: "src/main.py".into(),
                    old_text: Some("old\n".into()),
                    new_text: "new\n".into(),
                },
            ],
            running: true,
            ..Default::default()
        };

        ChatTurnProjection::apply(&mut turn);

        assert_eq!(
            turn.activity,
            ChatActivityKind::Language("language.py".into())
        );
        let ChatTurnRow::Tool(call) = &turn.rows[0] else {
            panic!("expected live tool");
        };
        assert!(call.live);
        let ChatTurnRow::Diff(diff) = &turn.rows[1] else {
            panic!("expected diff");
        };
        assert_eq!(diff.name, "main.py");
        assert_eq!(diff.lines.len(), 2);
        assert_eq!(diff.lines[0].kind, ChatDiffLineKind::Removed);
        assert_eq!(diff.lines[1].kind, ChatDiffLineKind::Added);
    }

    #[test]
    fn malformed_arguments_remain_visible() {
        assert_eq!(
            ChatTurnProjection::tool_arguments("not json"),
            ChatToolArguments::Raw("not json".into())
        );
    }

    #[test]
    fn hierarchy_is_owned_by_host_projection() {
        let mut turn = ChatTurn {
            blocks: vec![
                ChatBlock::ToolUse {
                    call_id: "read".into(),
                    name: "read_file".into(),
                    args: "{}".into(),
                    parent_call_id: None,

                    kind: None,
                },
                ChatBlock::ToolUse {
                    call_id: "review".into(),
                    name: "guardian_review".into(),
                    args: "{}".into(),
                    parent_call_id: None,

                    kind: None,
                },
                ChatBlock::ToolResult {
                    call_id: "review".into(),
                    content: "done".into(),
                    is_error: false,
                },
                ChatBlock::ToolResult {
                    call_id: String::new(),
                    content: "standalone".into(),
                    is_error: false,
                },
                ChatBlock::ToolUse {
                    call_id: "run".into(),
                    name: "run".into(),
                    args: "{}".into(),
                    parent_call_id: None,

                    kind: None,
                },
            ],
            running: true,
            ..Default::default()
        };

        ChatTurnProjection::apply(&mut turn);
        let ChatTurnRow::FinishedTools { calls, .. } = &turn.rows[0] else {
            panic!("expected read tool");
        };
        let read = &calls[0];
        assert_eq!(read.children.len(), 2);
        assert!(matches!(&read.children[0], ChatToolChild::Tool(call) if call.index == 1));
        assert!(matches!(
            &read.children[1],
            ChatToolChild::Result { index: 2, .. }
        ));
        assert!(!read.live);
        assert_eq!(turn.step_count, 3);
        assert!(matches!(
            &turn.rows[1],
            ChatTurnRow::ToolResult { index: 3, .. }
        ));
        assert!(matches!(&turn.rows[2], ChatTurnRow::Tool(call) if call.index == 4 && call.live));
    }

    #[test]
    fn activity_counts_use_projected_turn_state() {
        let mut turn = ChatTurn {
            blocks: vec![
                ChatBlock::Subagent(Box::new(ChatSubagent {
                    call_id: "agent".into(),
                    provider: "codex".into(),
                    title: "worker".into(),
                    status: "in_progress".into(),
                    activity: String::new(),
                    agent_name: None,
                    thread_id: None,
                    parent_thread_id: None,
                    child_thread_ids: Vec::new(),
                    parent_call_id: None,
                    prompt: None,
                    model: None,
                    reasoning_effort: None,
                    raw_input: String::new(),
                })),
                ChatBlock::Plan {
                    steps: vec![
                        vmux_api::chat::ChatPlanStep {
                            content: "done".into(),
                            status: "completed".into(),
                        },
                        vmux_api::chat::ChatPlanStep {
                            content: "next".into(),
                            status: "pending".into(),
                        },
                    ],
                },
            ],
            ..Default::default()
        };
        ChatTurnProjection::apply(&mut turn);

        assert_eq!(
            ChatTurnProjection::activity_counts(&[ChatItem::Turn(turn)]),
            (1, 1)
        );
    }
}
