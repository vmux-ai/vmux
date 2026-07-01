//! Projects ACP `session/update` notifications into the vmux [`Message`] transcript that
//! the chat UI already renders (the same shape the provider-direct path produces).

use crate::message::{AssistantBlock, Message};
use agent_client_protocol::schema::v1::{
    ContentBlock, SessionUpdate, ToolCall, ToolCallContent, ToolCallUpdate,
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

    /// Feed one update; returns the side effects the driver should emit.
    pub fn apply(&mut self, update: SessionUpdate) -> Vec<Intent> {
        match update {
            SessionUpdate::AgentMessageChunk(chunk) => self.append_assistant_text(chunk.content),
            SessionUpdate::ToolCall(tc) => self.apply_tool_call(tc),
            SessionUpdate::ToolCallUpdate(update) => self.apply_tool_call_update(update),
            _ => Vec::new(),
        }
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
        for content in &tc.content {
            if let ToolCallContent::Diff(diff) = content {
                intents.push(Intent::ProposedDiff {
                    call_id: call_id.clone(),
                    path: diff.path.to_string_lossy().into_owned(),
                    old_text: diff.old_text.clone(),
                    new_text: diff.new_text.clone(),
                });
            }
        }
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
        vec![Intent::Snapshot]
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
}

fn raw_input_json(raw: Option<&serde_json::Value>) -> String {
    raw.map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string())
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
}
