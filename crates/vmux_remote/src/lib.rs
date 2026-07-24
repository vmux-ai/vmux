use serde::{Deserialize, Serialize};
use vmux_wire::protocol::{AgentAttachment, AgentRunStatus};

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
    /// The agent's streamed internal reasoning.
    Thinking(String),
    ToolUse {
        call_id: String,
        name: String,
        args: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_call_id: Option<String>,
    },
    Subagent(Box<SubagentBlock>),
    /// A proposed file edit rendered as an inline diff.
    Diff {
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    /// The agent's execution plan.
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

/// One entry in an agent [`AssistantBlock::Plan`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanStep {
    pub content: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteStatus {
    Idle,
    Streaming,
    Interrupted,
    Errored(String),
}

impl From<&AgentRunStatus> for RemoteStatus {
    fn from(status: &AgentRunStatus) -> Self {
        match status {
            AgentRunStatus::Idle => Self::Idle,
            AgentRunStatus::Streaming => Self::Streaming,
            AgentRunStatus::Interrupted => Self::Interrupted,
            AgentRunStatus::Errored(message) => Self::Errored(message.clone()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RemoteApproval {
    pub call_id: String,
    pub name: String,
    pub args_json: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RemoteSession {
    pub sid: String,
    pub name: String,
    pub runtime: String,
    pub model: Option<String>,
    pub cwd: String,
    pub status: RemoteStatus,
    pub approval: Option<RemoteApproval>,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RemoteEvent {
    Session { session: RemoteSession },
    Snapshot { messages: Vec<Message> },
    Delta { text: String },
    Status { status: RemoteStatus },
    Approval { approval: Option<RemoteApproval> },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PromptRequest {
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApprovalRequest {
    pub call_id: String,
    pub allow: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_roundtrip() {
        let message = Message::user("hi");
        let json = serde_json::to_string(&message).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(message, back);
        assert!(!json.contains("attachments"));
    }

    #[test]
    fn user_deserializes_legacy_message_without_attachments() {
        let message: Message = serde_json::from_str(r#"{"User":{"text":"hi"}}"#).unwrap();
        assert_eq!(message, Message::user("hi"));
    }

    #[test]
    fn assistant_blocks_roundtrip() {
        let message = Message::Assistant {
            blocks: vec![
                AssistantBlock::Text("hello".into()),
                AssistantBlock::ToolUse {
                    call_id: "abc".into(),
                    name: "list_spaces".into(),
                    args: "{}".to_string(),
                    parent_call_id: None,
                },
            ],
        };
        let json = serde_json::to_string(&message).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(message, back);
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
}
