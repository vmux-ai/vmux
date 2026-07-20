use serde::{Deserialize, Serialize};

use crate::protocol::AgentAttachment;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Message {
    User {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<AgentAttachment>,
    },
    Assistant {
        blocks: Vec<AssistantBlock>,
    },
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            text: text.into(),
            attachments: Vec::new(),
        }
    }

    pub fn user_with_attachments(
        text: impl Into<String>,
        attachments: Vec<AgentAttachment>,
    ) -> Self {
        Self::User {
            text: text.into(),
            attachments,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AssistantBlock {
    Text(String),
    /// The agent's streamed internal reasoning (ACP `AgentThoughtChunk`), shown as a
    /// collapsible "Thinking" section.
    Thinking(String),
    ToolUse {
        call_id: String,
        name: String,
        args: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_call_id: Option<String>,
    },
    Subagent(Box<SubagentBlock>),
    /// A proposed file edit (ACP `ToolCallContent::Diff`), rendered as an inline diff in the chat.
    Diff {
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    /// The agent's execution plan / task tree (ACP `SessionUpdate::Plan`). Re-sent in full on each
    /// update, so the projector replaces the single plan block in place.
    Plan {
        steps: Vec<PlanStep>,
    },
}

/// A delegated agent operation surfaced by an ACP adapter.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SubagentBlock {
    pub call_id: String,
    pub provider: String,
    pub title: String,
    pub status: String,
    pub action: String,
    pub agent_name: Option<String>,
    pub thread_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub child_thread_ids: Vec<String>,
    pub parent_call_id: Option<String>,
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub raw_input: String,
}

/// One entry in an agent [`AssistantBlock::Plan`]. `status` is `pending` | `in_progress` |
/// `completed`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanStep {
    pub content: String,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_roundtrip() {
        let m = Message::user("hi");
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
        assert!(!json.contains("attachments"));
    }

    #[test]
    fn user_deserializes_legacy_message_without_attachments() {
        let message: Message = serde_json::from_str(r#"{"User":{"text":"hi"}}"#).unwrap();
        assert_eq!(message, Message::user("hi"));
    }

    #[test]
    fn assistant_blocks_roundtrip() {
        let m = Message::Assistant {
            blocks: vec![
                AssistantBlock::Text("hello".into()),
                AssistantBlock::ToolUse {
                    call_id: "abc".into(),
                    name: "list_spaces".into(),
                    args: "{}".to_string(),
                    parent_call_id: None,
                },
                AssistantBlock::Subagent(Box::new(SubagentBlock {
                    call_id: "agent-1".into(),
                    provider: "Codex".into(),
                    title: "Start subagent explorer".into(),
                    status: "in_progress".into(),
                    action: "started".into(),
                    agent_name: Some("explorer".into()),
                    thread_id: Some("thread-1".into()),
                    parent_thread_id: Some("thread-root".into()),
                    child_thread_ids: vec!["thread-1".into()],
                    parent_call_id: None,
                    prompt: Some("Inspect ACP support".into()),
                    model: Some("gpt-5.4".into()),
                    reasoning_effort: Some("high".into()),
                    raw_input: "{}".into(),
                })),
            ],
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn tool_use_deserializes_without_parent_call_id() {
        let block: AssistantBlock =
            serde_json::from_str(r#"{"ToolUse":{"call_id":"abc","name":"run","args":"{}"}}"#)
                .unwrap();
        assert!(matches!(
            block,
            AssistantBlock::ToolUse {
                parent_call_id: None,
                ..
            }
        ));
    }

    #[test]
    fn tool_result_roundtrip() {
        let m = Message::ToolResult {
            call_id: "abc".into(),
            content: "ok".into(),
            is_error: false,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }
}
