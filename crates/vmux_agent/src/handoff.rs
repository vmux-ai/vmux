use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{AgentKind, AssistantBlock, Message};

pub const HANDOFF_PROMPT_PREFIX: &str = vmux_service::protocol::PRIVATE_CONTEXT_PREFIX;
pub const OMITTED_MARKER: &str = "[Older source turns omitted]";
pub const DEFAULT_CONTEXT_LIMIT: usize = 64 * 1024;

const CONTEXT_INTRO: &str = "Conversation imported from another agent:\n";

#[derive(Component, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImportedConversation {
    pub source_agent: String,
    pub source_kind: AgentKind,
    pub source_sid: String,
    pub messages: Vec<Message>,
    pub truncated: bool,
    pub first_prompt: Option<String>,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PendingHandoff {
    pub context: String,
    pub sent: bool,
}

impl PendingHandoff {
    pub fn context_for_send(&mut self) -> Option<String> {
        if self.sent {
            return None;
        }
        self.sent = true;
        Some(self.context.clone())
    }

    pub fn retry(&mut self) {
        self.sent = false;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltContext {
    pub text: String,
    pub truncated: bool,
}

pub fn build_context(messages: &[Message], limit: usize) -> BuiltContext {
    let segments: Vec<String> = messages.iter().filter_map(context_segment).collect();
    let full = format!("{CONTEXT_INTRO}{}", segments.join("\n"));
    if full.chars().count() <= limit {
        return BuiltContext {
            text: full,
            truncated: false,
        };
    }

    let reserved = CONTEXT_INTRO.chars().count() + OMITTED_MARKER.chars().count() + 2;
    let mut remaining = limit.saturating_sub(reserved);
    let mut kept = Vec::new();
    for segment in segments.iter().rev() {
        let len = segment.chars().count() + usize::from(!kept.is_empty());
        if len > remaining {
            continue;
        }
        remaining -= len;
        kept.push(segment.clone());
    }
    kept.reverse();

    BuiltContext {
        text: format!("{CONTEXT_INTRO}{OMITTED_MARKER}\n\n{}", kept.join("\n")),
        truncated: true,
    }
}

fn context_segment(message: &Message) -> Option<String> {
    match message {
        Message::User { text } if !text.trim().is_empty() => Some(format!("User:\n{text}")),
        Message::Assistant { blocks } => {
            let text = blocks
                .iter()
                .filter_map(|block| match block {
                    AssistantBlock::Text(text) if !text.trim().is_empty() => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            (!text.is_empty()).then(|| format!("Assistant:\n{text}"))
        }
        _ => None,
    }
}

pub fn wire_prompt(context: &str, display_text: &str) -> String {
    vmux_service::protocol::compose_agent_prompt(display_text, Some(context))
}

pub fn sanitize_replayed_messages(messages: &mut [Message], first_prompt: Option<&str>) {
    let Some(first_prompt) = first_prompt else {
        return;
    };
    let Some(Message::User { text }) = messages
        .iter_mut()
        .find(|message| matches!(message, Message::User { .. }))
    else {
        return;
    };
    if text.starts_with(HANDOFF_PROMPT_PREFIX) {
        *text = first_prompt.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssistantBlock, Message};

    fn user(text: &str) -> Message {
        Message::User {
            text: text.to_string(),
        }
    }

    fn assistant(text: &str) -> Message {
        Message::Assistant {
            blocks: vec![AssistantBlock::Text(text.to_string())],
        }
    }

    #[test]
    fn context_budget_keeps_newest_complete_messages() {
        let messages = vec![
            user("old message that should not fit"),
            assistant("middle message"),
            user("new message"),
        ];

        let built = build_context(&messages, 100);

        assert!(built.truncated);
        assert!(built.text.contains(OMITTED_MARKER));
        assert!(built.text.contains("new message"));
        assert!(!built.text.contains("old message"));
    }

    #[test]
    fn context_budget_preserves_chronological_order() {
        let messages = vec![user("first"), assistant("second"), user("third")];

        let built = build_context(&messages, 1_000);

        let first = built.text.find("first").unwrap();
        let second = built.text.find("second").unwrap();
        let third = built.text.find("third").unwrap();
        assert!(first < second && second < third);
        assert!(!built.truncated);
    }

    #[test]
    fn context_ignores_non_text_assistant_blocks_and_tool_results() {
        let messages = vec![
            Message::Assistant {
                blocks: vec![
                    AssistantBlock::Thinking("secret".into()),
                    AssistantBlock::Text("visible".into()),
                    AssistantBlock::ToolUse {
                        call_id: "c".into(),
                        name: "run".into(),
                        args: "{}".into(),
                    },
                ],
            },
            Message::ToolResult {
                call_id: "c".into(),
                content: "tool output".into(),
                is_error: false,
            },
        ];

        let built = build_context(&messages, 1_000);

        assert!(built.text.contains("visible"));
        assert!(!built.text.contains("secret"));
        assert!(!built.text.contains("tool output"));
    }

    #[test]
    fn private_wire_prompt_keeps_display_prompt_separate() {
        let prompt = wire_prompt("prior conversation", "continue here");

        assert!(prompt.starts_with(HANDOFF_PROMPT_PREFIX));
        assert!(prompt.contains("prior conversation"));
        assert!(prompt.ends_with("continue here"));
    }

    #[test]
    fn replay_private_prompt_is_replaced_with_display_prompt() {
        let mut messages = vec![
            user(&wire_prompt("prior conversation", "continue here")),
            assistant("done"),
        ];

        sanitize_replayed_messages(&mut messages, Some("continue here"));

        assert_eq!(messages[0], user("continue here"));
        assert_eq!(messages[1], assistant("done"));
    }

    #[test]
    fn pending_context_sends_once_and_can_retry_after_error() {
        let mut pending = PendingHandoff {
            context: "prior conversation".into(),
            sent: false,
        };

        assert_eq!(
            pending.context_for_send().as_deref(),
            Some("prior conversation")
        );
        assert!(pending.context_for_send().is_none());
        pending.retry();
        assert_eq!(
            pending.context_for_send().as_deref(),
            Some("prior conversation")
        );
    }
}
