//! Projects ACP `session/update` notifications into the vmux [`Message`] transcript that
//! the chat UI already renders (the same shape the provider-direct path produces).

use crate::message::{AssistantBlock, Message, PlanStep};
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

/// Accumulates ACP updates into a `Vec<Message>`. Pure and synchronous so it is fully
/// unit-testable without an ACP connection.
#[derive(Default)]
pub struct AcpProjector {
    messages: Vec<Message>,
}

impl AcpProjector {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current transcript, serialized into `AgentMessagesSnapshot` by the driver.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Record the user's prompt as its own turn. ACP never echoes the prompt back as a
    /// `session/update`, so without this the transcript the chat renders would omit it and
    /// the optimistic user bubble would vanish on the next snapshot.
    pub fn push_user(&mut self, text: String) {
        self.messages.push(Message::User { text });
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
            Some(Message::User { text: existing }) => existing.push_str(&text),
            _ => self.messages.push(Message::User { text }),
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

    fn apply_tool_call(&mut self, tc: ToolCall) -> Vec<Intent> {
        let call_id = tc.tool_call_id.to_string();
        self.upsert_tool_use(&call_id, &tc.title, &raw_input_json(tc.raw_input.as_ref()));
        let mut intents = vec![Intent::Snapshot];
        intents.extend(file_touch_intents(tc.kind, &tc.locations));
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
        if let (Some(kind), Some(locations)) = (update.fields.kind, &update.fields.locations) {
            intents.extend(file_touch_intents(kind, locations));
        }
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
        for item in content {
            if let ToolCallContent::Diff(diff) = item {
                let path = diff.path.to_string_lossy().into_owned();
                self.upsert_diff(call_id, &path, diff.old_text.clone(), diff.new_text.clone());
                intents.push(Intent::ProposedDiff {
                    call_id: call_id.to_string(),
                    path,
                    old_text: diff.old_text.clone(),
                    new_text: diff.new_text.clone(),
                });
            }
        }
        let output = tool_output_text(content);
        if !output.is_empty() {
            self.upsert_tool_result(call_id, output, failed);
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
        ContentChunk, Diff, SessionUpdate, TextContent, ToolCall, ToolCallContent,
    };

    fn chunk(text: &str) -> SessionUpdate {
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            text,
        ))))
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
        p.push_user("hi".to_string());
        p.apply(chunk("hello"));
        assert_eq!(p.messages().len(), 2);
        assert_eq!(
            p.messages()[0],
            Message::User {
                text: "hi".to_string()
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
}
