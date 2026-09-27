use vmux_api::chat::{
    ChatActivityKind, ChatBlock, ChatDiff, ChatDiffLine, ChatDiffLineKind, ChatItem, ChatPlanItem,
    ChatPlanStatus, ChatSubagent, ChatSubagentState, ChatSubagentStatus, ChatSubagentSummary,
    ChatToolArgument, ChatToolArgumentValue, ChatToolArguments, ChatToolCall, ChatToolChild,
    ChatToolChildCall, ChatToolKind, ChatTurn, ChatTurnRow,
};

pub fn project_turn(turn: &mut ChatTurn) {
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
            .map(activity_for_block)
            .unwrap_or(ChatActivityKind::Thinking)
    } else {
        ChatActivityKind::None
    };

    let latest_tool = turn
        .running
        .then(|| latest_top_level_tool_index(turn))
        .flatten();
    let block_count = turn.blocks.len();
    let mut rows = Vec::new();
    for (index, block) in turn.blocks.iter().enumerate() {
        if parent_tool_index(turn, index).is_some() {
            continue;
        }
        let Some(row) = project_row(turn, index, block, block_count, latest_tool) else {
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

pub fn activity_counts(items: &[ChatItem]) -> (u32, u32) {
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

pub fn current_activity(items: &[ChatItem], status: &str) -> Option<ChatActivityKind> {
    match status {
        "installing" => Some(ChatActivityKind::Installing),
        "awaiting" => Some(ChatActivityKind::Awaiting),
        "errored" => Some(ChatActivityKind::Error),
        "streaming" => items.iter().rev().find_map(|item| match item {
            ChatItem::Turn(turn) if turn.running => Some(turn.activity),
            _ => None,
        }),
        _ => None,
    }
}

fn project_row(
    turn: &ChatTurn,
    index: usize,
    block: &ChatBlock,
    block_count: usize,
    latest_tool: Option<usize>,
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
        ChatBlock::ToolUse { name, args, .. } => Some(ChatTurnRow::Tool(project_tool(
            turn,
            index,
            name,
            args,
            latest_tool == Some(index as usize),
        ))),
        ChatBlock::Subagent(subagent) => Some(ChatTurnRow::Subagent(project_subagent(
            turn, index, subagent,
        ))),
        ChatBlock::Diff {
            path,
            old_text,
            new_text,
            ..
        } => Some(ChatTurnRow::Diff(project_diff(
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
                    status: plan_status(&step.status),
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

fn project_tool(turn: &ChatTurn, index: u32, name: &str, args: &str, live: bool) -> ChatToolCall {
    ChatToolCall {
        index,
        name: name.to_string(),
        kind: tool_kind(name, args),
        activity: activity_for_tool(name, args),
        fallback_label: fallback_tool_label(name),
        file_path: tool_file_path(args),
        arguments: tool_arguments(args),
        children: tool_children(turn, index as usize),
        live,
    }
}

fn project_child_tool(index: usize, name: &str, args: &str) -> ChatToolChildCall {
    ChatToolChildCall {
        index: u32::try_from(index).unwrap_or(u32::MAX),
        name: name.to_string(),
        kind: tool_kind(name, args),
        activity: activity_for_tool(name, args),
        fallback_label: fallback_tool_label(name),
        file_path: tool_file_path(args),
        arguments: tool_arguments(args),
    }
}

fn tool_children(turn: &ChatTurn, parent: usize) -> Vec<ChatToolChild> {
    let mut children = Vec::new();
    for (index, block) in turn.blocks.iter().enumerate() {
        if parent_tool_index(turn, index) != Some(parent) {
            continue;
        }
        match block {
            ChatBlock::ToolUse { name, args, .. } => {
                children.push(ChatToolChild::Tool(project_child_tool(index, name, args)));
            }
            ChatBlock::Subagent(subagent) => {
                children.push(ChatToolChild::Subagent(project_subagent_summary(
                    index, subagent,
                )));
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

fn project_subagent(turn: &ChatTurn, index: u32, subagent: &ChatSubagent) -> ChatSubagentState {
    ChatSubagentState {
        index,
        call_id: subagent.call_id.clone(),
        provider: subagent.provider.clone(),
        title: subagent.title.replace('_', " "),
        status: subagent_status(&subagent.status),
        activity: subagent.activity.replace('_', " "),
        agent_name: subagent.agent_name.clone(),
        thread_id: subagent.thread_id.clone(),
        parent_thread_id: subagent.parent_thread_id.clone(),
        child_threads: subagent.child_thread_ids.join(", "),
        prompt: subagent.prompt.clone(),
        model: subagent.model.clone(),
        reasoning_effort: subagent.reasoning_effort.clone(),
        raw_input: subagent.raw_input.clone(),
        children: tool_children(turn, index as usize),
    }
}

fn project_subagent_summary(index: usize, subagent: &ChatSubagent) -> ChatSubagentSummary {
    ChatSubagentSummary {
        index: u32::try_from(index).unwrap_or(u32::MAX),
        title: subagent.title.replace('_', " "),
        status: subagent_status(&subagent.status),
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
                .map(|(name, value)| tool_argument(name, value))
                .collect(),
        ),
        value => ChatToolArguments::Value(tool_argument_value("", value)),
    }
}

fn tool_argument(name: String, value: serde_json::Value) -> ChatToolArgument {
    ChatToolArgument {
        label: tool_argument_label(&name),
        value: tool_argument_value(&name, value),
        name,
    }
}

fn tool_argument_value(name: &str, value: serde_json::Value) -> ChatToolArgumentValue {
    match value {
        serde_json::Value::String(value) if tool_argument_is_path(name, &value) => {
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
                .map(|(index, value)| tool_argument((index + 1).to_string(), value))
                .collect(),
        ),
        serde_json::Value::Object(values) => ChatToolArgumentValue::Object(
            values
                .into_iter()
                .map(|(name, value)| tool_argument(name, value))
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

fn tool_kind(name: &str, args: &str) -> ChatToolKind {
    let lower = name.to_ascii_lowercase();
    if is_guardian(name) {
        ChatToolKind::Guardian
    } else if lower.contains("read_file")
        || lower.contains("read file")
        || lower.contains("open_file")
        || lower.contains("open file")
    {
        if arguments_read_skill(args) {
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
    } else if lower.contains("browser") || lower.contains("navigate") || lower.contains("web_") {
        ChatToolKind::Browser
    } else if lower.contains("grep") || lower.contains("search") || lower.contains("find") {
        ChatToolKind::Search
    } else if lower.contains("run")
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
    if is_python(args) || is_python(name) {
        return ChatActivityKind::Python;
    }
    match tool_kind(name, args) {
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
        ChatBlock::ToolUse { name, args, .. } => activity_for_tool(name, args),
        ChatBlock::Subagent(_) => ChatActivityKind::Subagent,
        ChatBlock::Diff { path, .. } => {
            if is_python(path) {
                ChatActivityKind::Python
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
    contains_skill_path(&value)
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
                || contains_skill_path(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(contains_skill_path),
        _ => false,
    }
}

fn tool_file_path(args: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str(args)
        && let Some(path) = file_path_from_value(&value)
    {
        return Some(path);
    }
    file_path_from_text(args)
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
            map.values().find_map(file_path_from_value)
        }
        serde_json::Value::Array(values) => values.iter().find_map(file_path_from_value),
        serde_json::Value::String(text) => file_path_from_text(text),
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

fn is_python(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains(".py") || lower == "py" || lower.contains("python")
}

pub(crate) fn parent_tool_index(turn: &ChatTurn, index: usize) -> Option<usize> {
    let mut parent = direct_parent_index(turn, index)?;
    for _ in 0..turn.blocks.len() {
        let Some(next) = direct_parent_index(turn, parent) else {
            break;
        };
        if next == parent {
            break;
        }
        parent = next;
    }
    Some(parent)
}

fn latest_top_level_tool_index(turn: &ChatTurn) -> Option<usize> {
    for (index, block) in turn.blocks.iter().enumerate().rev() {
        if matches!(block, ChatBlock::ToolUse { .. }) && parent_tool_index(turn, index).is_none() {
            return Some(index);
        }
    }
    None
}

fn direct_parent_index(turn: &ChatTurn, index: usize) -> Option<usize> {
    match turn.blocks.get(index)? {
        ChatBlock::ToolUse {
            parent_call_id: Some(parent_call_id),
            ..
        } => call_index(turn, parent_call_id),
        ChatBlock::Subagent(subagent) => subagent
            .parent_call_id
            .as_deref()
            .and_then(|parent_call_id| call_index(turn, parent_call_id)),
        ChatBlock::ToolUse { name, .. } if is_guardian(name) => guardian_parent_index(turn, index),
        ChatBlock::ToolResult { call_id, .. } if !call_id.is_empty() => call_index(turn, call_id),
        _ => None,
    }
}

fn call_index(turn: &ChatTurn, call_id: &str) -> Option<usize> {
    turn.blocks.iter().position(|block| match block {
        ChatBlock::ToolUse {
            call_id: block_call_id,
            ..
        } => block_call_id == call_id,
        ChatBlock::Subagent(subagent) => subagent.call_id == call_id,
        _ => false,
    })
}

fn guardian_parent_index(turn: &ChatTurn, index: usize) -> Option<usize> {
    for (candidate, block) in turn.blocks[..index].iter().enumerate().rev() {
        match block {
            ChatBlock::ToolUse { name, .. } if is_guardian(name) => {}
            ChatBlock::ToolUse { .. } | ChatBlock::Subagent(_) => return Some(candidate),
            _ => return None,
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

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
                },
            ],
            ..Default::default()
        };

        project_turn(&mut turn);

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
                if matches!(fields[0].value, ChatToolArgumentValue::Path(_))
        ));
        assert!(matches!(
            &calls[1].arguments,
            ChatToolArguments::Fields(fields)
                if matches!(fields[0].value, ChatToolArgumentValue::Code(_))
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

        project_turn(&mut turn);

        assert_eq!(turn.activity, ChatActivityKind::Python);
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
            tool_arguments("not json"),
            ChatToolArguments::Raw("not json".into())
        );
    }

    #[test]
    fn hierarchy_is_owned_by_host_projection() {
        let turn = ChatTurn {
            blocks: vec![
                ChatBlock::ToolUse {
                    call_id: "read".into(),
                    name: "read_file".into(),
                    args: "{}".into(),
                    parent_call_id: None,
                },
                ChatBlock::ToolUse {
                    call_id: "review".into(),
                    name: "guardian_review".into(),
                    args: "{}".into(),
                    parent_call_id: None,
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
                },
            ],
            ..Default::default()
        };

        assert_eq!(parent_tool_index(&turn, 0), None);
        assert_eq!(parent_tool_index(&turn, 1), Some(0));
        assert_eq!(parent_tool_index(&turn, 2), Some(0));
        assert_eq!(parent_tool_index(&turn, 3), None);
        assert_eq!(latest_top_level_tool_index(&turn), Some(4));
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
        project_turn(&mut turn);

        assert_eq!(activity_counts(&[ChatItem::Turn(turn)]), (1, 1));
    }
}
