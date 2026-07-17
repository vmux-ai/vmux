//! Projects ACP `session/update` notifications into the vmux [`Message`] transcript that
//! the chat UI already renders (the same shape the provider-direct path produces).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use crate::message::{AssistantBlock, Message, PlanStep};
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
                } = block
                else {
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
        self.upsert_tool_use(&call_id, &tc.title, &raw_input_json(tc.raw_input.as_ref()));
        let mut intents = vec![Intent::Snapshot];
        intents.extend(self.project_tool_file_touches(
            &call_id,
            Some(tc.kind),
            Some(&tc.locations),
            Some(tc.status),
            true,
        ));
        intents.extend(self.record_tool_content(
            &call_id,
            &tc.content,
            matches!(tc.status, ToolCallStatus::Failed),
        ));
        intents
    }

    fn apply_tool_call_update(&mut self, update: ToolCallUpdate) -> Vec<Intent> {
        let call_id = update.tool_call_id.to_string();
        let title = update.fields.title.clone().unwrap_or_default();
        self.upsert_tool_use(
            &call_id,
            &title,
            &raw_input_json(update.fields.raw_input.as_ref()),
        );
        let mut intents = vec![Intent::Snapshot];
        intents.extend(self.project_tool_file_touches(
            &call_id,
            update.fields.kind,
            update.fields.locations.as_deref(),
            update.fields.status,
            false,
        ));
        if let Some(content) = &update.fields.content {
            let failed = matches!(update.fields.status, Some(ToolCallStatus::Failed));
            intents.extend(self.record_tool_content(&call_id, content, failed));
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

    fn upsert_tool_use(&mut self, call_id: &str, name: &str, args: &str) {
        for message in self.messages.iter_mut() {
            if let Message::Assistant { blocks } = message {
                for block in blocks.iter_mut() {
                    if let AssistantBlock::ToolUse {
                        call_id: existing,
                        name: existing_name,
                        args: existing_args,
                    } = block
                        && existing == call_id
                    {
                        if !name.is_empty() {
                            *existing_name = name.to_string();
                        }
                        if args != "{}" {
                            *existing_args = args.to_string();
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
