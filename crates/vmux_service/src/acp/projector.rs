//! Projects ACP `session/update` notifications into the vmux [`Message`] transcript that
//! the chat UI already renders (the same shape the provider-direct path produces).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use crate::message::{AssistantBlock, Message, PlanStep, SubagentBlock};
use crate::protocol::AgentAttachment;
use agent_client_protocol::schema::v1::{
    ContentBlock, Plan, PlanEntryStatus, SessionUpdate, ToolCall, ToolCallContent,
    ToolCallLocation, ToolCallStatus, ToolCallUpdate, ToolKind,
};

/// A side effect the driver performs after feeding a `SessionUpdate` to the projector.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// Incremental assistant text → `ServiceMessage::AgentDelta`.
    Delta(String),
    /// The transcript changed structurally → `ServiceMessage::AgentMessagesSnapshot`.
    Snapshot,
    /// A tool call carries a proposed edit → `ServiceMessage::AcpProposedDiff`.
    ProposedDiff {
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    /// A tool call read or edited a file (from its ACP `locations`) → open that file in the
    /// agent's `file://` follow-pane, mirroring the vibe CLI `vmux-file-follow` hook.
    FileTouched {
        path: String,
        line: Option<u32>,
        kind: crate::protocol::FileTouchKind,
    },
    WorkspaceChanged {
        name: String,
        branch: String,
        cwd: String,
        workspace_cwd: String,
    },
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcpWorktreeMetadata {
    name: String,
    branch: String,
    cwd: String,
    workspace_cwd: String,
}

fn workspace_changed_intent(
    update: agent_client_protocol::schema::v1::SessionInfoUpdate,
) -> Vec<Intent> {
    let Some(value) = update
        .meta
        .as_ref()
        .and_then(|meta| meta.get("worktree"))
        .cloned()
    else {
        return Vec::new();
    };
    let Ok(worktree) = serde_json::from_value::<AcpWorktreeMetadata>(value) else {
        return Vec::new();
    };
    if worktree.name.trim().is_empty()
        || worktree.branch.trim().is_empty()
        || !Path::new(&worktree.cwd).is_absolute()
        || !Path::new(&worktree.workspace_cwd).is_absolute()
    {
        return Vec::new();
    }
    vec![Intent::WorkspaceChanged {
        name: worktree.name,
        branch: worktree.branch,
        cwd: worktree.cwd,
        workspace_cwd: worktree.workspace_cwd,
    }]
}

/// Map an ACP `ToolKind` to a follow-pane touch kind. Only file-affecting kinds open a
/// preview; search/execute/think/etc. are ignored.
fn file_touch_kind(kind: ToolKind) -> Option<crate::protocol::FileTouchKind> {
    use crate::protocol::FileTouchKind;
    match kind {
        ToolKind::Read => Some(FileTouchKind::Read),
        ToolKind::Edit | ToolKind::Delete | ToolKind::Move => Some(FileTouchKind::Edit),
        _ => None,
    }
}

/// Build `FileTouched` intents for each file location of a file-affecting tool call.
fn file_touch_intents(kind: ToolKind, locations: &[ToolCallLocation]) -> Vec<Intent> {
    let Some(kind) = file_touch_kind(kind) else {
        return Vec::new();
    };
    locations
        .iter()
        .map(|loc| Intent::FileTouched {
            path: loc.path.to_string_lossy().into_owned(),
            line: loc.line,
            kind,
        })
        .collect()
}

fn locations_from_diffs(content: &[ToolCallContent]) -> Vec<ToolCallLocation> {
    let mut paths = HashSet::new();
    let mut locations = Vec::new();
    for item in content {
        if let ToolCallContent::Diff(diff) = item
            && paths.insert(diff.path.clone())
        {
            locations.push(ToolCallLocation::new(diff.path.clone()));
        }
    }
    locations
}

fn edit_tool_kind(kind: ToolKind) -> bool {
    matches!(kind, ToolKind::Edit | ToolKind::Delete | ToolKind::Move)
}

const ACTIVE_FILE_TOUCH_LIMIT: usize = 1024;
const FINALIZED_FILE_TOUCH_LIMIT: usize = 1024;

#[derive(Default)]
struct FileTouchState {
    kind: ToolKind,
    locations: Vec<ToolCallLocation>,
    pending_edits: Vec<Intent>,
}

fn project_file_touches(
    state: &mut FileTouchState,
    kind: Option<ToolKind>,
    locations: Option<&[ToolCallLocation]>,
    status: Option<ToolCallStatus>,
    full_update: bool,
) -> (Vec<Intent>, bool) {
    let identity_changed = full_update
        || kind.is_some_and(|kind| kind != state.kind)
        || locations.is_some_and(|locations| locations != state.locations);
    if let Some(kind) = kind {
        state.kind = kind;
    }
    if let Some(locations) = locations {
        state.locations = locations.to_vec();
    }
    let current = file_touch_intents(state.kind, &state.locations);
    match status {
        Some(ToolCallStatus::Failed) => {
            state.pending_edits.clear();
            (Vec::new(), true)
        }
        Some(ToolCallStatus::Completed) => {
            let intents = if identity_changed {
                current
            } else {
                std::mem::take(&mut state.pending_edits)
            };
            (intents, true)
        }
        _ if identity_changed => {
            if edit_tool_kind(state.kind) {
                state.pending_edits.clone_from(&current);
            } else {
                state.pending_edits.clear();
            }
            (current, false)
        }
        _ => (Vec::new(), false),
    }
}

/// Accumulates ACP updates into a `Vec<Message>`. Pure and synchronous so it is fully
/// unit-testable without an ACP connection.
#[derive(Default)]
pub struct AcpProjector {
    messages: Vec<Message>,
    file_touches: HashMap<String, FileTouchState>,
    file_touch_order: VecDeque<String>,
    finalized_file_touches: HashSet<String>,
    finalized_file_touch_order: VecDeque<String>,
}

impl AcpProjector {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current transcript, serialized into `AgentMessagesSnapshot` by the driver.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Returns the projected title and raw input for a tool call.
    pub fn tool_call_details(&self, call_id: &str) -> Option<(String, String)> {
        self.messages.iter().find_map(|message| {
            let Message::Assistant { blocks } = message else {
                return None;
            };
            blocks.iter().find_map(|block| {
                let AssistantBlock::ToolUse {
                    call_id: existing,
                    name,
                    args,
                    ..
                } = block
                else {
                    if let AssistantBlock::Subagent(subagent) = block {
                        return (subagent.call_id == call_id)
                            .then(|| (subagent.title.clone(), subagent.raw_input.clone()));
                    }
                    return None;
                };
                (existing == call_id).then(|| (name.clone(), args.clone()))
            })
        })
    }

    /// Record the user's prompt as its own turn. ACP never echoes the prompt back as a
    /// `session/update`, so without this the transcript the chat renders would omit it and
    /// the optimistic user bubble would vanish on the next snapshot.
    pub fn push_user(&mut self, text: String, attachments: Vec<AgentAttachment>) {
        self.messages
            .push(Message::user_with_attachments(text, attachments));
    }

    /// Feed one update; returns the side effects the driver should emit.
    pub fn apply(&mut self, update: SessionUpdate) -> Vec<Intent> {
        match update {
            SessionUpdate::AgentMessageChunk(chunk) => self.append_assistant_text(chunk.content),
            // Replayed during `session/load`: the agent re-emits prior user turns (live prompting
            // never echoes them — those go through `push_user`).
            SessionUpdate::UserMessageChunk(chunk) => self.append_user_chunk(chunk.content),
            SessionUpdate::AgentThoughtChunk(chunk) => self.append_thinking(chunk.content),
            SessionUpdate::ToolCall(tc) => self.apply_tool_call(tc),
            SessionUpdate::ToolCallUpdate(update) => self.apply_tool_call_update(update),
            SessionUpdate::Plan(plan) => self.upsert_plan(plan),
            SessionUpdate::SessionInfoUpdate(update) => workspace_changed_intent(update),
            _ => Vec::new(),
        }
    }

    fn append_thinking(&mut self, content: ContentBlock) -> Vec<Intent> {
        let ContentBlock::Text(text) = content else {
            return Vec::new();
        };
        let text = text.text;
        match self.messages.last_mut() {
            Some(Message::Assistant { blocks }) => match blocks.last_mut() {
                Some(AssistantBlock::Thinking(existing)) => existing.push_str(&text),
                _ => blocks.push(AssistantBlock::Thinking(text)),
            },
            _ => self.messages.push(Message::Assistant {
                blocks: vec![AssistantBlock::Thinking(text)],
            }),
        }
        vec![Intent::Snapshot]
    }

    fn append_user_chunk(&mut self, content: ContentBlock) -> Vec<Intent> {
        let ContentBlock::Text(text) = content else {
            return Vec::new();
        };
        let text = text.text;
        match self.messages.last_mut() {
            Some(Message::User { text: existing, .. }) => existing.push_str(&text),
            _ => self.messages.push(Message::user(text)),
        }
        vec![Intent::Snapshot]
    }

    fn upsert_plan(&mut self, plan: Plan) -> Vec<Intent> {
        let steps: Vec<PlanStep> = plan
            .entries
            .iter()
            .map(|entry| PlanStep {
                content: entry.content.clone(),
                status: match entry.status {
                    PlanEntryStatus::InProgress => "in_progress",
                    PlanEntryStatus::Completed => "completed",
                    _ => "pending",
                }
                .to_string(),
            })
            .collect();
        for message in self.messages.iter_mut() {
            if let Message::Assistant { blocks } = message {
                for block in blocks.iter_mut() {
                    if let AssistantBlock::Plan { steps: existing } = block {
                        *existing = steps;
                        return vec![Intent::Snapshot];
                    }
                }
            }
        }
        let block = AssistantBlock::Plan { steps };
        match self.messages.last_mut() {
            Some(Message::Assistant { blocks }) => blocks.push(block),
            _ => self.messages.push(Message::Assistant {
                blocks: vec![block],
            }),
        }
        vec![Intent::Snapshot]
    }

    fn append_assistant_text(&mut self, content: ContentBlock) -> Vec<Intent> {
        let ContentBlock::Text(text) = content else {
            return Vec::new();
        };
        let text = text.text;
        match self.messages.last_mut() {
            Some(Message::Assistant { blocks }) => match blocks.last_mut() {
                Some(AssistantBlock::Text(existing)) => existing.push_str(&text),
                _ => blocks.push(AssistantBlock::Text(text.clone())),
            },
            _ => self.messages.push(Message::Assistant {
                blocks: vec![AssistantBlock::Text(text.clone())],
            }),
        }
        // `Delta` is the incremental hint; `Snapshot` keeps `AgentMessages` (what the chat page
        // renders) in sync, since nothing applies `AgentDelta` to it.
        vec![Intent::Delta(text), Intent::Snapshot]
    }

    fn project_tool_file_touches(
        &mut self,
        call_id: &str,
        kind: Option<ToolKind>,
        locations: Option<&[ToolCallLocation]>,
        status: Option<ToolCallStatus>,
        full_update: bool,
    ) -> Vec<Intent> {
        if self.finalized_file_touches.contains(call_id) {
            return Vec::new();
        }
        let should_track = self.file_touches.contains_key(call_id)
            || kind.is_some_and(|kind| file_touch_kind(kind).is_some())
            || locations.is_some_and(|locations| !locations.is_empty());
        if !should_track {
            return Vec::new();
        }
        self.track_file_touch(call_id);
        let (intents, finalized) = project_file_touches(
            self.file_touches
                .get_mut(call_id)
                .expect("tracked file touch state"),
            kind,
            locations,
            status,
            full_update,
        );
        if finalized {
            self.remove_file_touch(call_id);
            self.mark_file_touch_finalized(call_id);
        }
        intents
    }

    fn track_file_touch(&mut self, call_id: &str) {
        if self.file_touches.contains_key(call_id) {
            return;
        }
        self.file_touches
            .insert(call_id.to_string(), FileTouchState::default());
        self.file_touch_order.push_back(call_id.to_string());
        while self.file_touch_order.len() > ACTIVE_FILE_TOUCH_LIMIT {
            if let Some(expired) = self.file_touch_order.pop_front() {
                self.file_touches.remove(&expired);
            }
        }
    }

    fn remove_file_touch(&mut self, call_id: &str) {
        self.file_touches.remove(call_id);
        if let Some(index) = self
            .file_touch_order
            .iter()
            .position(|tracked| tracked == call_id)
        {
            self.file_touch_order.remove(index);
        }
    }

    fn mark_file_touch_finalized(&mut self, call_id: &str) {
        if !self.finalized_file_touches.insert(call_id.to_string()) {
            return;
        }
        self.finalized_file_touch_order
            .push_back(call_id.to_string());
        while self.finalized_file_touch_order.len() > FINALIZED_FILE_TOUCH_LIMIT {
            if let Some(expired) = self.finalized_file_touch_order.pop_front() {
                self.finalized_file_touches.remove(&expired);
            }
        }
    }

    fn apply_tool_call(&mut self, tc: ToolCall) -> Vec<Intent> {
        let call_id = tc.tool_call_id.to_string();
        let subagent = subagent_block(
            &call_id,
            &tc.title,
            Some(&tc.status),
            tc.raw_input.as_ref(),
            tc.meta.as_ref(),
        );
        let is_subagent = subagent.is_some();
        if let Some(subagent) = subagent {
            self.upsert_subagent(subagent);
        } else {
            self.upsert_tool_use(
                &call_id,
                &tc.title,
                &raw_input_json(tc.raw_input.as_ref()),
                parent_call_id(tc.meta.as_ref()),
            );
        }
        let diff_locations = tc
            .locations
            .is_empty()
            .then(|| locations_from_diffs(&tc.content));
        let locations = diff_locations.as_deref().unwrap_or(&tc.locations);
        let mut intents = vec![Intent::Snapshot];
        intents.extend(self.project_tool_file_touches(
            &call_id,
            Some(tc.kind),
            Some(locations),
            Some(tc.status),
            true,
        ));
        if !is_subagent
            || matches!(
                tc.status,
                ToolCallStatus::Completed | ToolCallStatus::Failed
            )
        {
            intents.extend(self.record_tool_content(
                &call_id,
                &tc.content,
                matches!(tc.status, ToolCallStatus::Failed),
            ));
            if is_subagent && tool_output_text(&tc.content).is_empty() {
                self.record_raw_output(
                    &call_id,
                    tc.raw_output.as_ref(),
                    matches!(tc.status, ToolCallStatus::Failed),
                );
            }
        }
        intents
    }

    fn apply_tool_call_update(&mut self, update: ToolCallUpdate) -> Vec<Intent> {
        let call_id = update.tool_call_id.to_string();
        let title = update.fields.title.clone().unwrap_or_default();
        let subagent = subagent_block(
            &call_id,
            &title,
            update.fields.status.as_ref(),
            update.fields.raw_input.as_ref(),
            update.meta.as_ref(),
        );
        let is_subagent = if let Some(subagent) = subagent {
            self.upsert_subagent(subagent);
            true
        } else if self.update_subagent(
            &call_id,
            &title,
            update.fields.status.as_ref(),
            update.fields.raw_input.as_ref(),
        ) {
            true
        } else {
            self.upsert_tool_use(
                &call_id,
                &title,
                &raw_input_json(update.fields.raw_input.as_ref()),
                parent_call_id(update.meta.as_ref()),
            );
            false
        };
        let diff_locations = update
            .fields
            .content
            .as_deref()
            .map(locations_from_diffs)
            .unwrap_or_default();
        let locations = update.fields.locations.as_deref();
        let locations = if locations.is_none_or(|locations| locations.is_empty())
            && !diff_locations.is_empty()
        {
            Some(diff_locations.as_slice())
        } else {
            locations
        };
        let mut intents = vec![Intent::Snapshot];
        intents.extend(self.project_tool_file_touches(
            &call_id,
            update.fields.kind,
            locations,
            update.fields.status,
            false,
        ));
        let terminal_status = matches!(
            update.fields.status,
            Some(ToolCallStatus::Completed | ToolCallStatus::Failed)
        );
        if let Some(content) = &update.fields.content
            && (!is_subagent || terminal_status)
        {
            let failed = matches!(update.fields.status, Some(ToolCallStatus::Failed));
            intents.extend(self.record_tool_content(&call_id, content, failed));
            if is_subagent && tool_output_text(content).is_empty() {
                self.record_raw_output(&call_id, update.fields.raw_output.as_ref(), failed);
            }
        } else if is_subagent && terminal_status {
            self.record_raw_output(
                &call_id,
                update.fields.raw_output.as_ref(),
                matches!(update.fields.status, Some(ToolCallStatus::Failed)),
            );
        }
        intents
    }

    /// Fold a tool call's content into the transcript: proposed diffs become inline diff blocks
    /// (and a `ProposedDiff` intent), textual output becomes a `ToolResult` message. Returns the
    /// `ProposedDiff` intents to emit.
    fn record_tool_content(
        &mut self,
        call_id: &str,
        content: &[ToolCallContent],
        failed: bool,
    ) -> Vec<Intent> {
        let mut intents = Vec::new();
        let mut has_terminal = false;
        for item in content {
            match item {
                ToolCallContent::Diff(diff) => {
                    let path = diff.path.to_string_lossy().into_owned();
                    self.upsert_diff(call_id, &path, diff.old_text.clone(), diff.new_text.clone());
                    intents.push(Intent::ProposedDiff {
                        call_id: call_id.to_string(),
                        path,
                        old_text: diff.old_text.clone(),
                        new_text: diff.new_text.clone(),
                    });
                }
                // An embedded ACP terminal renders as a live pane; point the transcript card at it.
                // Real captured text (if the agent also sends `Content`) overwrites this below.
                ToolCallContent::Terminal(_) => has_terminal = true,
                _ => {}
            }
        }
        let output = tool_output_text(content);
        if !output.is_empty() {
            self.upsert_tool_result(call_id, output, failed);
        } else if has_terminal {
            self.upsert_tool_result(
                call_id,
                "[terminal output shown in the attached pane]".to_string(),
                failed,
            );
        }
        intents
    }

    fn upsert_tool_result(&mut self, call_id: &str, content: String, is_error: bool) {
        for message in self.messages.iter_mut() {
            if let Message::ToolResult {
                call_id: existing,
                content: existing_content,
                is_error: existing_error,
            } = message
                && existing == call_id
            {
                *existing_content = content;
                *existing_error = is_error;
                return;
            }
        }
        self.messages.push(Message::ToolResult {
            call_id: call_id.to_string(),
            content,
            is_error,
        });
    }

    fn record_raw_output(
        &mut self,
        call_id: &str,
        raw_output: Option<&serde_json::Value>,
        is_error: bool,
    ) {
        let Some(raw_output) = raw_output.filter(|value| !value.is_null()) else {
            return;
        };
        self.upsert_tool_result(call_id, pretty_json(raw_output), is_error);
    }

    fn update_subagent(
        &mut self,
        call_id: &str,
        title: &str,
        status: Option<&ToolCallStatus>,
        raw_input: Option<&serde_json::Value>,
    ) -> bool {
        for message in self.messages.iter_mut() {
            let Message::Assistant { blocks } = message else {
                continue;
            };
            for block in blocks.iter_mut() {
                let AssistantBlock::Subagent(subagent) = block else {
                    continue;
                };
                if subagent.call_id != call_id {
                    continue;
                }
                if !title.is_empty() {
                    subagent.title = title.to_string();
                }
                if let Some(status) = status {
                    subagent.status = tool_call_status(status).to_string();
                }
                if let Some(raw_input) = raw_input {
                    subagent.raw_input = pretty_json(raw_input);
                    merge_subagent_input(subagent, raw_input);
                }
                return true;
            }
        }
        false
    }

    fn upsert_subagent(&mut self, subagent: SubagentBlock) {
        for message in self.messages.iter_mut() {
            let Message::Assistant { blocks } = message else {
                continue;
            };
            for block in blocks.iter_mut() {
                let same_call = match block {
                    AssistantBlock::ToolUse {
                        call_id: existing, ..
                    } => existing == &subagent.call_id,
                    AssistantBlock::Subagent(existing) => existing.call_id == subagent.call_id,
                    _ => false,
                };
                if !same_call {
                    continue;
                }
                match block {
                    AssistantBlock::Subagent(existing) => merge_subagent(existing, subagent),
                    _ => *block = AssistantBlock::Subagent(Box::new(subagent)),
                }
                return;
            }
        }
        let block = AssistantBlock::Subagent(Box::new(subagent));
        match self.messages.last_mut() {
            Some(Message::Assistant { blocks }) => blocks.push(block),
            _ => self.messages.push(Message::Assistant {
                blocks: vec![block],
            }),
        }
    }

    fn upsert_tool_use(
        &mut self,
        call_id: &str,
        name: &str,
        args: &str,
        parent_call_id: Option<String>,
    ) {
        for message in self.messages.iter_mut() {
            if let Message::Assistant { blocks } = message {
                for block in blocks.iter_mut() {
                    if let AssistantBlock::ToolUse {
                        call_id: existing,
                        name: existing_name,
                        args: existing_args,
                        parent_call_id: existing_parent_call_id,
                    } = block
                        && existing == call_id
                    {
                        if !name.is_empty() {
                            *existing_name = name.to_string();
                        }
                        if args != "{}" {
                            *existing_args = args.to_string();
                        }
                        if parent_call_id.is_some() {
                            *existing_parent_call_id = parent_call_id;
                        }
                        return;
                    }
                }
            }
        }
        let block = AssistantBlock::ToolUse {
            call_id: call_id.to_string(),
            name: name.to_string(),
            args: args.to_string(),
            parent_call_id,
        };
        match self.messages.last_mut() {
            Some(Message::Assistant { blocks }) => blocks.push(block),
            _ => self.messages.push(Message::Assistant {
                blocks: vec![block],
            }),
        }
    }

    fn upsert_diff(
        &mut self,
        call_id: &str,
        path: &str,
        old_text: Option<String>,
        new_text: String,
    ) {
        for message in self.messages.iter_mut() {
            if let Message::Assistant { blocks } = message {
                for block in blocks.iter_mut() {
                    if let AssistantBlock::Diff {
                        call_id: existing,
                        old_text: eo,
                        new_text: en,
                        ..
                    } = block
                        && existing == call_id
                    {
                        *eo = old_text;
                        *en = new_text;
                        return;
                    }
                }
            }
        }
        let block = AssistantBlock::Diff {
            call_id: call_id.to_string(),
            path: path.to_string(),
            old_text,
            new_text,
        };
        match self.messages.last_mut() {
            Some(Message::Assistant { blocks }) => blocks.push(block),
            _ => self.messages.push(Message::Assistant {
                blocks: vec![block],
            }),
        }
    }
}

fn subagent_block(
    call_id: &str,
    title: &str,
    status: Option<&ToolCallStatus>,
    raw_input: Option<&serde_json::Value>,
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<SubagentBlock> {
    codex_subagent_block(call_id, title, status, raw_input, meta)
        .or_else(|| claude_subagent_block(call_id, title, status, raw_input, meta))
}

fn codex_subagent_block(
    call_id: &str,
    title: &str,
    status: Option<&ToolCallStatus>,
    raw_input: Option<&serde_json::Value>,
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<SubagentBlock> {
    let codex = meta?.get("codex")?.as_object()?;
    let input = raw_input.and_then(serde_json::Value::as_object);
    if let Some(subagent) = codex.get("subagent").and_then(serde_json::Value::as_object) {
        let path = string_field(subagent, "path");
        return Some(SubagentBlock {
            call_id: call_id.to_string(),
            provider: "Codex".to_string(),
            title: title.to_string(),
            status: status.map(tool_call_status).unwrap_or_default().to_string(),
            action: string_field(subagent, "activity").unwrap_or_default(),
            agent_name: path.as_deref().and_then(agent_name_from_path),
            thread_id: string_field(subagent, "threadId")
                .or_else(|| input.and_then(|input| string_field(input, "agentThreadId"))),
            parent_thread_id: None,
            child_thread_ids: Vec::new(),
            parent_call_id: None,
            prompt: input.and_then(|input| string_field(input, "prompt")),
            model: input.and_then(|input| string_field(input, "model")),
            reasoning_effort: input.and_then(|input| string_field(input, "reasoningEffort")),
            raw_input: raw_input.map(pretty_json).unwrap_or_default(),
        });
    }
    let collaboration = codex
        .get("collaboration")
        .and_then(serde_json::Value::as_object)?;
    let child_thread_ids = string_list_field(collaboration, "receiverThreadIds");
    Some(SubagentBlock {
        call_id: call_id.to_string(),
        provider: "Codex".to_string(),
        title: title.to_string(),
        status: status.map(tool_call_status).unwrap_or_default().to_string(),
        action: string_field(collaboration, "tool").unwrap_or_default(),
        agent_name: None,
        thread_id: None,
        parent_thread_id: string_field(collaboration, "senderThreadId")
            .or_else(|| input.and_then(|input| string_field(input, "senderThreadId"))),
        child_thread_ids: if child_thread_ids.is_empty() {
            input
                .map(|input| string_list_field(input, "receiverThreadIds"))
                .unwrap_or_default()
        } else {
            child_thread_ids
        },
        parent_call_id: None,
        prompt: input.and_then(|input| string_field(input, "prompt")),
        model: input.and_then(|input| string_field(input, "model")),
        reasoning_effort: input.and_then(|input| string_field(input, "reasoningEffort")),
        raw_input: raw_input.map(pretty_json).unwrap_or_default(),
    })
}

fn claude_subagent_block(
    call_id: &str,
    title: &str,
    status: Option<&ToolCallStatus>,
    raw_input: Option<&serde_json::Value>,
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<SubagentBlock> {
    let claude = meta?.get("claudeCode")?.as_object()?;
    let tool_name = claude.get("toolName")?.as_str()?;
    if !matches!(tool_name, "Agent" | "Task") {
        return None;
    }
    let input = raw_input.and_then(serde_json::Value::as_object);
    let action = if input.is_some_and(|input| input.get("resume").is_some()) {
        "resume"
    } else if input
        .and_then(|input| input.get("run_in_background"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        "background"
    } else {
        "delegate"
    };
    Some(SubagentBlock {
        call_id: call_id.to_string(),
        provider: "Claude".to_string(),
        title: title.to_string(),
        status: status.map(tool_call_status).unwrap_or_default().to_string(),
        action: action.to_string(),
        agent_name: input.and_then(|input| {
            string_field(input, "name").or_else(|| string_field(input, "subagent_type"))
        }),
        thread_id: input.and_then(|input| string_field(input, "resume")),
        parent_thread_id: None,
        child_thread_ids: Vec::new(),
        parent_call_id: string_field(claude, "parentToolUseId"),
        prompt: input.and_then(|input| string_field(input, "prompt")),
        model: input.and_then(|input| string_field(input, "model")),
        reasoning_effort: None,
        raw_input: raw_input.map(pretty_json).unwrap_or_default(),
    })
}

fn parent_call_id(meta: Option<&serde_json::Map<String, serde_json::Value>>) -> Option<String> {
    meta?
        .get("claudeCode")?
        .as_object()
        .and_then(|claude| string_field(claude, "parentToolUseId"))
}

fn merge_subagent(existing: &mut SubagentBlock, update: SubagentBlock) {
    if !update.provider.is_empty() {
        existing.provider = update.provider;
    }
    if !update.title.is_empty() {
        existing.title = update.title;
    }
    if !update.status.is_empty() {
        existing.status = update.status;
    }
    if !update.action.is_empty() {
        existing.action = update.action;
    }
    existing.agent_name = update.agent_name.or(existing.agent_name.take());
    existing.thread_id = update.thread_id.or(existing.thread_id.take());
    existing.parent_thread_id = update.parent_thread_id.or(existing.parent_thread_id.take());
    if !update.child_thread_ids.is_empty() {
        existing.child_thread_ids = update.child_thread_ids;
    }
    existing.parent_call_id = update.parent_call_id.or(existing.parent_call_id.take());
    existing.prompt = update.prompt.or(existing.prompt.take());
    existing.model = update.model.or(existing.model.take());
    existing.reasoning_effort = update.reasoning_effort.or(existing.reasoning_effort.take());
    if !update.raw_input.is_empty() {
        existing.raw_input = update.raw_input;
    }
}

fn merge_subagent_input(subagent: &mut SubagentBlock, raw_input: &serde_json::Value) {
    let Some(input) = raw_input.as_object() else {
        return;
    };
    subagent.prompt = string_field(input, "prompt").or(subagent.prompt.take());
    subagent.model = string_field(input, "model").or(subagent.model.take());
    subagent.reasoning_effort =
        string_field(input, "reasoningEffort").or(subagent.reasoning_effort.take());
    let child_thread_ids = string_list_field(input, "receiverThreadIds");
    if !child_thread_ids.is_empty() {
        subagent.child_thread_ids = child_thread_ids;
    }
}

fn string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Option<String> {
    let value = object.get(field)?;
    value.as_str().map(ToString::to_string).or_else(|| {
        (!value.is_null() && !value.is_object() && !value.is_array()).then(|| value.to_string())
    })
}

fn string_list_field(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Vec<String> {
    object
        .get(field)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(ToString::to_string))
        .collect()
}

fn agent_name_from_path(path: &str) -> Option<String> {
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .map(|part| part.trim_end_matches(".toml").to_string())
}

fn tool_call_status(status: &ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Pending => "pending",
        ToolCallStatus::InProgress => "in_progress",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed => "failed",
        _ => "pending",
    }
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn raw_input_json(raw: Option<&serde_json::Value>) -> String {
    raw.map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string())
}

/// Concatenate the textual (non-diff) content of a tool call into a single output string.
fn tool_output_text(content: &[ToolCallContent]) -> String {
    let mut out = String::new();
    for item in content {
        if let ToolCallContent::Content(inner) = item
            && let ContentBlock::Text(text) = &inner.content
        {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&text.text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        ContentChunk, Diff, SessionInfoUpdate, SessionUpdate, Terminal, TextContent, ToolCall,
        ToolCallContent,
    };

    fn chunk(text: &str) -> SessionUpdate {
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            text,
        ))))
    }

    fn meta(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        let serde_json::Value::Object(meta) = value else {
            panic!("expected metadata object")
        };
        meta
    }

    #[test]
    fn session_info_worktree_metadata_emits_workspace_change() {
        let mut meta = serde_json::Map::new();
        meta.insert(
            "worktree".to_string(),
            serde_json::json!({
                "name": "quiet-amber-wolf",
                "branch": "vibe/quiet-amber-wolf",
                "cwd": "/worktrees/quiet-amber-wolf/subdir",
                "workspaceCwd": "/repo/subdir"
            }),
        );
        let mut projector = AcpProjector::new();

        let intents = projector.apply(SessionUpdate::SessionInfoUpdate(
            SessionInfoUpdate::new().meta(meta),
        ));

        assert_eq!(
            intents,
            vec![Intent::WorkspaceChanged {
                name: "quiet-amber-wolf".to_string(),
                branch: "vibe/quiet-amber-wolf".to_string(),
                cwd: "/worktrees/quiet-amber-wolf/subdir".to_string(),
                workspace_cwd: "/repo/subdir".to_string(),
            }]
        );
    }

    #[test]
    fn session_info_rejects_relative_worktree_paths() {
        let mut meta = serde_json::Map::new();
        meta.insert(
            "worktree".to_string(),
            serde_json::json!({
                "name": "quiet-amber-wolf",
                "branch": "vibe/quiet-amber-wolf",
                "cwd": "worktrees/quiet-amber-wolf",
                "workspaceCwd": "/repo"
            }),
        );
        let mut projector = AcpProjector::new();

        let intents = projector.apply(SessionUpdate::SessionInfoUpdate(
            SessionInfoUpdate::new().meta(meta),
        ));

        assert!(intents.is_empty());
    }

    #[test]
    fn message_chunks_accumulate_into_one_assistant_message() {
        let mut p = AcpProjector::new();
        let first = p.apply(chunk("Hel"));
        let second = p.apply(chunk("lo"));
        assert_eq!(
            first,
            vec![Intent::Delta("Hel".to_string()), Intent::Snapshot]
        );
        assert_eq!(
            second,
            vec![Intent::Delta("lo".to_string()), Intent::Snapshot]
        );
        assert_eq!(p.messages().len(), 1);
        assert_eq!(
            p.messages()[0],
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("Hello".to_string())],
            }
        );
    }

    #[test]
    fn push_user_records_a_turn_before_following_assistant_text() {
        let mut p = AcpProjector::new();
        let attachment = AgentAttachment {
            path: "/tmp/image.png".into(),
            name: "image.png".into(),
            mime_type: "image/png".into(),
            size: 3,
        };
        p.push_user("hi".to_string(), vec![attachment.clone()]);
        p.apply(chunk("hello"));
        assert_eq!(p.messages().len(), 2);
        assert_eq!(
            p.messages()[0],
            Message::User {
                text: "hi".to_string(),
                attachments: vec![attachment],
            }
        );
        assert_eq!(
            p.messages()[1],
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("hello".to_string())],
            }
        );
    }

    #[test]
    fn tool_call_with_diff_emits_proposed_diff_and_records_block() {
        let mut p = AcpProjector::new();
        let tc = ToolCall::new("c1", "Edit file").content(vec![ToolCallContent::Diff(
            Diff::new("/tmp/a.rs", "b").old_text("a"),
        )]);
        let intents = p.apply(SessionUpdate::ToolCall(tc));
        assert!(intents.contains(&Intent::Snapshot));
        assert!(intents.iter().any(|i| matches!(
            i,
            Intent::ProposedDiff { call_id, path, old_text, new_text }
                if call_id == "c1"
                    && path == "/tmp/a.rs"
                    && old_text.as_deref() == Some("a")
                    && new_text == "b"
        )));
        assert_eq!(p.messages().len(), 1);
        match &p.messages()[0] {
            Message::Assistant { blocks } => assert!(matches!(
                blocks.first(),
                Some(AssistantBlock::ToolUse { call_id, .. }) if call_id == "c1"
            )),
            other => panic!("expected assistant message, got {other:?}"),
        }
    }

    #[test]
    fn codex_subagent_metadata_projects_first_class_block() {
        let mut p = AcpProjector::new();
        let tc = ToolCall::new("sub-1", "Start subagent explorer")
            .status(ToolCallStatus::InProgress)
            .raw_input(serde_json::json!({
                "agentThreadId": "thread-child",
                "agentPath": ".codex/agents/explorer.toml",
                "activityKind": "started",
                "prompt": "Inspect ACP projection",
                "model": "gpt-5.4",
                "reasoningEffort": "high"
            }))
            .meta(meta(serde_json::json!({
                "codex": {
                    "subagent": {
                        "threadId": "thread-child",
                        "path": ".codex/agents/explorer.toml",
                        "activity": "started"
                    }
                }
            })));

        p.apply(SessionUpdate::ToolCall(tc));

        let Message::Assistant { blocks } = &p.messages()[0] else {
            panic!("expected assistant message")
        };
        let AssistantBlock::Subagent(subagent) = &blocks[0] else {
            panic!("expected subagent block")
        };
        assert_eq!(subagent.provider, "Codex");
        assert_eq!(subagent.status, "in_progress");
        assert_eq!(subagent.action, "started");
        assert_eq!(subagent.agent_name.as_deref(), Some("explorer"));
        assert_eq!(subagent.thread_id.as_deref(), Some("thread-child"));
        assert_eq!(subagent.prompt.as_deref(), Some("Inspect ACP projection"));
        assert_eq!(subagent.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(subagent.reasoning_effort.as_deref(), Some("high"));
    }

    #[test]
    fn codex_collaboration_metadata_projects_child_threads() {
        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("spawn-1", "spawn_agent")
                .status(ToolCallStatus::InProgress)
                .raw_input(serde_json::json!({
                    "prompt": "Inspect two subsystems",
                    "receiverThreadIds": ["thread-a", "thread-b"],
                    "model": "gpt-5.4",
                    "reasoningEffort": "medium"
                }))
                .meta(meta(serde_json::json!({
                    "codex": {
                        "collaboration": {
                            "tool": "spawn_agent",
                            "senderThreadId": "thread-root",
                            "receiverThreadIds": ["thread-a", "thread-b"]
                        }
                    }
                }))),
        ));

        let Message::Assistant { blocks } = &p.messages()[0] else {
            panic!("expected assistant message")
        };
        let AssistantBlock::Subagent(subagent) = &blocks[0] else {
            panic!("expected subagent block")
        };
        assert_eq!(subagent.action, "spawn_agent");
        assert_eq!(subagent.parent_thread_id.as_deref(), Some("thread-root"));
        assert_eq!(subagent.child_thread_ids, ["thread-a", "thread-b"]);
        assert_eq!(subagent.prompt.as_deref(), Some("Inspect two subsystems"));
    }

    #[test]
    fn subagent_update_without_metadata_preserves_identity_and_records_output() {
        use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("sub-1", "Start subagent explorer")
                .status(ToolCallStatus::InProgress)
                .meta(meta(serde_json::json!({
                    "codex": {
                        "subagent": {
                            "threadId": "thread-child",
                            "path": "explorer",
                            "activity": "started"
                        }
                    }
                }))),
        ));

        p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "sub-1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .raw_output(serde_json::json!({"summary": "inspection complete"})),
        )));

        let Message::Assistant { blocks } = &p.messages()[0] else {
            panic!("expected assistant message")
        };
        assert!(matches!(
            &blocks[0],
            AssistantBlock::Subagent(subagent) if subagent.status == "completed"
        ));
        assert!(p.messages().iter().any(|message| matches!(
            message,
            Message::ToolResult { call_id, content, is_error: false }
                if call_id == "sub-1" && content.contains("inspection complete")
        )));
    }

    #[test]
    fn claude_agent_and_child_tool_preserve_parent_relationship() {
        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("agent-1", "Inspect ACP support")
                .status(ToolCallStatus::InProgress)
                .raw_input(serde_json::json!({
                    "description": "Inspect ACP support",
                    "prompt": "Trace subagent metadata",
                    "subagent_type": "Explore",
                    "model": "sonnet"
                }))
                .meta(meta(serde_json::json!({
                    "claudeCode": {"toolName": "Agent"}
                }))),
        ));
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("read-1", "Read files").meta(meta(serde_json::json!({
                "claudeCode": {
                    "toolName": "Read",
                    "parentToolUseId": "agent-1"
                }
            }))),
        ));

        let Message::Assistant { blocks } = &p.messages()[0] else {
            panic!("expected assistant message")
        };
        assert!(matches!(
            &blocks[0],
            AssistantBlock::Subagent(subagent)
                if subagent.provider == "Claude"
                    && subagent.agent_name.as_deref() == Some("Explore")
                    && subagent.prompt.as_deref() == Some("Trace subagent metadata")
        ));
        assert!(matches!(
            &blocks[1],
            AssistantBlock::ToolUse { parent_call_id, .. }
                if parent_call_id.as_deref() == Some("agent-1")
        ));
    }

    #[test]
    fn edit_diff_without_locations_emits_and_retries_file_touch() {
        use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

        let mut p = AcpProjector::new();
        let started = p.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Editing files")
                .kind(ToolKind::Edit)
                .content(vec![ToolCallContent::Diff(Diff::new(
                    "/repo/src/main.rs",
                    "new",
                ))]),
        ));
        let completed = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )));

        for intents in [started, completed] {
            assert!(intents.iter().any(|intent| matches!(
                intent,
                Intent::FileTouched { path, line: None, kind }
                    if path == "/repo/src/main.rs"
                        && *kind == crate::protocol::FileTouchKind::Edit
            )));
        }
    }

    #[test]
    fn tool_call_details_returns_projected_title_and_input() {
        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "vmux.run")
                .raw_input(serde_json::json!({"command": "echo hi", "focus": true})),
        ));

        assert_eq!(
            p.tool_call_details("c1"),
            Some((
                "vmux.run".to_string(),
                r#"{"command":"echo hi","focus":true}"#.to_string(),
            ))
        );
    }

    #[test]
    fn read_tool_call_locations_emit_file_touched() {
        let mut p = AcpProjector::new();
        let tc = ToolCall::new("c1", "Read file")
            .kind(ToolKind::Read)
            .locations(vec![ToolCallLocation::new("/repo/src/main.rs")]);
        let intents = p.apply(SessionUpdate::ToolCall(tc));
        assert!(intents.iter().any(|i| matches!(
            i,
            Intent::FileTouched { path, line: None, kind }
                if path == "/repo/src/main.rs" && *kind == crate::protocol::FileTouchKind::Read
        )));
    }

    #[test]
    fn completed_edit_retries_file_touch_from_initial_tool_call() {
        use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

        let mut p = AcpProjector::new();
        let tc = ToolCall::new("c1", "Write file")
            .kind(ToolKind::Edit)
            .locations(vec![ToolCallLocation::new("/repo/new.rs")]);
        p.apply(SessionUpdate::ToolCall(tc));

        let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )));

        assert!(intents.iter().any(|intent| matches!(
            intent,
            Intent::FileTouched { path, line: None, kind }
                if path == "/repo/new.rs" && *kind == crate::protocol::FileTouchKind::Edit
        )));
    }

    #[test]
    fn failed_edit_clears_pending_and_suppresses_future_touches() {
        use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write file")
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        ));

        let failed = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Failed),
        )));
        let completed = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        )));

        assert!(
            failed
                .iter()
                .chain(&completed)
                .all(|intent| !matches!(intent, Intent::FileTouched { .. }))
        );
        assert!(!p.file_touches.contains_key("c1"));
        assert!(!p.file_touch_order.iter().any(|call_id| call_id == "c1"));
    }

    #[test]
    fn locations_only_update_uses_initial_edit_kind() {
        use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write file").kind(ToolKind::Edit),
        ));

        let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::InProgress)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        )));

        assert!(intents.iter().any(|intent| matches!(
            intent,
            Intent::FileTouched { path, line: None, kind }
                if path == "/repo/new.rs" && *kind == crate::protocol::FileTouchKind::Edit
        )));
    }

    #[test]
    fn kind_only_update_uses_initial_locations() {
        use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write file")
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        ));

        let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::InProgress)
                .kind(ToolKind::Edit),
        )));

        assert!(intents.iter().any(|intent| matches!(
            intent,
            Intent::FileTouched { path, line: None, kind }
                if path == "/repo/new.rs" && *kind == crate::protocol::FileTouchKind::Edit
        )));
    }

    #[test]
    fn completion_with_explicit_locations_uses_replacement() {
        use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write files")
                .kind(ToolKind::Edit)
                .locations(vec![
                    ToolCallLocation::new("/repo/a.rs"),
                    ToolCallLocation::new("/repo/b.rs"),
                ]),
        ));

        let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/b.rs")]),
        )));
        let touches: Vec<_> = intents
            .iter()
            .filter_map(|intent| match intent {
                Intent::FileTouched { path, .. } => Some(path.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(touches, vec!["/repo/b.rs"]);
    }

    #[test]
    fn completion_reclassification_does_not_replay_initial_edit() {
        use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write file")
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/old.rs")]),
        ));

        let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .kind(ToolKind::Read)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        )));

        assert_eq!(
            intents
                .iter()
                .filter(|intent| matches!(intent, Intent::FileTouched { .. }))
                .collect::<Vec<_>>(),
            vec![&Intent::FileTouched {
                path: "/repo/new.rs".to_string(),
                line: None,
                kind: crate::protocol::FileTouchKind::Read,
            }]
        );
    }

    #[test]
    fn repeated_completion_emits_no_duplicate_file_touch() {
        use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write file")
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        ));
        p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )));

        let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        )));

        assert!(
            !intents
                .iter()
                .any(|intent| matches!(intent, Intent::FileTouched { .. }))
        );
        assert!(!p.file_touches.contains_key("c1"));
    }

    #[test]
    fn read_completion_with_unchanged_identity_emits_no_duplicate_touch() {
        use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Read file")
                .kind(ToolKind::Read)
                .locations(vec![ToolCallLocation::new("/repo/file.rs")]),
        ));

        let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .kind(ToolKind::Read)
                .locations(vec![ToolCallLocation::new("/repo/file.rs")]),
        )));

        assert!(
            !intents
                .iter()
                .any(|intent| matches!(intent, Intent::FileTouched { .. }))
        );
    }

    #[test]
    fn finalized_file_touch_tombstones_are_bounded() {
        let mut p = AcpProjector::new();
        for index in 0..1025 {
            p.apply(SessionUpdate::ToolCall(
                ToolCall::new(format!("c{index}"), "Read file")
                    .kind(ToolKind::Read)
                    .status(ToolCallStatus::Completed)
                    .locations(vec![ToolCallLocation::new(format!("/repo/{index}.rs"))]),
            ));
        }

        assert!(p.finalized_file_touches.len() <= 1024);
    }

    #[test]
    fn in_progress_file_touches_are_bounded() {
        let mut p = AcpProjector::new();
        for index in 0..1025 {
            p.apply(SessionUpdate::ToolCall(
                ToolCall::new(format!("c{index}"), "Read file")
                    .kind(ToolKind::Read)
                    .locations(vec![ToolCallLocation::new(format!("/repo/{index}.rs"))]),
            ));
        }

        assert!(p.file_touches.len() <= 1024);
        assert!(!p.file_touches.contains_key("c0"));
        assert!(p.file_touches.contains_key("c1024"));
    }

    #[test]
    fn non_file_tool_call_emits_no_file_touched() {
        let mut p = AcpProjector::new();
        let tc = ToolCall::new("c1", "run a command")
            .kind(ToolKind::Execute)
            .locations(vec![ToolCallLocation::new("/repo/x")]);
        let intents = p.apply(SessionUpdate::ToolCall(tc));
        assert!(
            !intents
                .iter()
                .any(|i| matches!(i, Intent::FileTouched { .. }))
        );
    }

    fn thought(text: &str) -> SessionUpdate {
        SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            text,
        ))))
    }

    #[test]
    fn thought_chunks_accumulate_into_a_thinking_block() {
        let mut p = AcpProjector::new();
        p.apply(thought("plan"));
        p.apply(thought("ning"));
        assert_eq!(p.messages().len(), 1);
        assert_eq!(
            p.messages()[0],
            Message::Assistant {
                blocks: vec![AssistantBlock::Thinking("planning".to_string())],
            }
        );
    }

    #[test]
    fn plan_update_replaces_the_single_plan_block() {
        use agent_client_protocol::schema::v1::{Plan, PlanEntry, PlanEntryPriority};
        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::Plan(Plan::new(vec![PlanEntry::new(
            "step one",
            PlanEntryPriority::High,
            PlanEntryStatus::Pending,
        )])));
        p.apply(SessionUpdate::Plan(Plan::new(vec![PlanEntry::new(
            "step one",
            PlanEntryPriority::High,
            PlanEntryStatus::Completed,
        )])));
        let blocks = match &p.messages()[0] {
            Message::Assistant { blocks } => blocks,
            other => panic!("expected assistant, got {other:?}"),
        };
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            AssistantBlock::Plan { steps } => {
                assert_eq!(steps.len(), 1);
                assert_eq!(steps[0].content, "step one");
                assert_eq!(steps[0].status, "completed");
            }
            other => panic!("expected plan, got {other:?}"),
        }
    }

    #[test]
    fn tool_call_update_content_becomes_a_tool_result() {
        use agent_client_protocol::schema::v1::{Content, ToolCallUpdate, ToolCallUpdateFields};
        let mut p = AcpProjector::new();
        p.apply(SessionUpdate::ToolCall(ToolCall::new("c1", "run")));
        let fields = ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .content(vec![ToolCallContent::Content(Content::new(
                ContentBlock::Text(TextContent::new("hello output")),
            ))]);
        p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1", fields,
        )));
        assert!(p.messages().iter().any(|m| matches!(
            m,
            Message::ToolResult { call_id, content, is_error: false }
                if call_id == "c1" && content == "hello output"
        )));
    }

    #[test]
    fn tool_call_with_terminal_folds_to_pane_pointer_result() {
        let mut p = AcpProjector::new();
        let tc = ToolCall::new("c1", "Run")
            .content(vec![ToolCallContent::Terminal(Terminal::new("t1"))]);
        p.apply(SessionUpdate::ToolCall(tc));
        assert!(p.messages().iter().any(|m| matches!(
            m,
            Message::ToolResult { call_id, content, .. }
                if call_id == "c1" && content.contains("pane")
        )));
    }

    #[test]
    fn tool_call_with_terminal_and_text_prefers_text_output() {
        use agent_client_protocol::schema::v1::Content;
        let mut p = AcpProjector::new();
        let tc = ToolCall::new("c1", "Run").content(vec![
            ToolCallContent::Terminal(Terminal::new("t1")),
            ToolCallContent::Content(Content::new(ContentBlock::Text(TextContent::new(
                "real output",
            )))),
        ]);
        p.apply(SessionUpdate::ToolCall(tc));
        assert!(p.messages().iter().any(|m| matches!(
            m,
            Message::ToolResult { call_id, content, .. }
                if call_id == "c1" && content == "real output"
        )));
    }
}
