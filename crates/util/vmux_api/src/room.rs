use serde::{Deserialize, Serialize};

pub use crate::prompt_media::{InlineMediaQuery, inline_media_query, replace_inline_media_query};
pub use crate::protocol::AgentAttachment;
use crate::protocol::AgentRunStatus;

use vmux_macro::string_id;

#[string_id]
pub struct RoomId(pub String);

impl RoomId {
    pub fn for_session(sid: &str) -> Self {
        Self::new(format!("session:{sid}"))
    }
}

#[string_id]
pub struct MemberId(pub String);

impl MemberId {
    pub fn local(room_id: &RoomId) -> Self {
        Self::new(format!("{}:member:local", room_id.as_str()))
    }

    pub fn agent(room_id: &RoomId) -> Self {
        Self::new(format!("{}:member:agent", room_id.as_str()))
    }
}

#[string_id]
pub struct EventId(pub String);

#[string_id]
pub struct ClientOpId(pub String);
#[vmux_api::contract(Copy, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoomRole {
    Owner,
    Participant,
    Observer,
}

#[vmux_api::contract(Copy, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemberKind {
    Human,
    Agent,
    System,
}

#[vmux_api::contract(Eq)]
pub struct RoomMember {
    pub room_id: RoomId,
    pub member_id: MemberId,
    pub display_name: String,
    pub role: RoomRole,
    pub kind: MemberKind,
}

#[vmux_api::contract]
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

#[vmux_api::contract]
pub struct RoomEvent {
    pub event_id: EventId,
    pub room_id: RoomId,
    pub actor_id: MemberId,
    pub client_op_id: Option<ClientOpId>,
    pub server_seq: u64,
    pub created_at_ms: u64,
    pub reply_to: Option<EventId>,
    pub message: Message,
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

#[vmux_api::contract]
pub enum AssistantBlock {
    Text(String),
    Thinking(String),
    ToolUse {
        call_id: String,
        name: String,
        args: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_call_id: Option<String>,
    },
    Subagent(Box<SubagentBlock>),
    Diff {
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    Plan {
        steps: Vec<PlanStep>,
    },
}

#[vmux_api::contract]
pub struct SubagentBlock {
    pub call_id: String,
    pub provider: String,
    pub title: String,
    pub status: String,
    pub activity: String,
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

#[vmux_api::contract]
pub struct PlanStep {
    pub content: String,
    pub status: String,
}

#[vmux_api::contract]
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

#[vmux_api::contract]
pub struct RemoteApproval {
    pub call_id: String,
    pub name: String,
    pub args: crate::json::JsonValue,
}

#[vmux_api::contract]
pub struct RemoteMediaEntry {
    pub path: String,
    pub name: String,
    pub parent: String,
    pub mime_type: String,
    pub size: u64,
    pub is_dir: bool,
    pub preview_data_url: String,
}

#[vmux_api::contract]
pub struct RemoteSession {
    pub sid: String,
    #[serde(default)]
    pub url: String,
    pub room_id: RoomId,
    #[serde(default)]
    pub title: String,
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
    Session {
        session: RemoteSession,
    },
    Snapshot {
        room_id: RoomId,
        through_seq: u64,
        events: Vec<RoomEvent>,
    },
    Delta {
        room_id: RoomId,
        text: String,
    },
    Status {
        status: RemoteStatus,
    },
    Approval {
        approval: Option<RemoteApproval>,
    },
}

#[vmux_api::contract(Default, Eq)]
pub struct ModelOptionEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RemoteModelState {
    pub models: Vec<ModelOptionEntry>,
    pub selected_id: String,
    pub effort_levels: Vec<String>,
    pub effort: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PromptRequest {
    pub client_op_id: ClientOpId,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AgentAttachment>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NewChatRequest {
    pub client_op_id: ClientOpId,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_url: Option<String>,
}

#[vmux_api::contract]
pub struct RemoteAgent {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub icon: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApprovalRequest {
    pub call_id: String,
    pub decision: crate::protocol::ApprovalDecision,
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

    #[test]
    fn new_chat_request_roundtrips() {
        let request = NewChatRequest {
            client_op_id: ClientOpId::new("op-1"),
            text: "start here".to_string(),
            agent_url: Some("vmux://sessions/claude".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let back: NewChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.text, request.text);
        assert_eq!(back.agent_url, request.agent_url);
    }

    #[test]
    fn prompt_request_deserializes_without_attachments() {
        let request: PromptRequest =
            serde_json::from_str(r#"{"client_op_id":"op-1","text":"hello"}"#).unwrap();
        assert_eq!(request.text, "hello");
        assert!(request.attachments.is_empty());
    }

    #[test]
    fn inline_media_query_requires_an_open_token() {
        assert_eq!(
            inline_media_query("inspect @Pictures/scr"),
            Some(InlineMediaQuery {
                start: 8,
                query: "Pictures/scr",
            })
        );
        assert_eq!(inline_media_query("mail@example.com"), None);
        assert_eq!(inline_media_query("inspect @image.png next"), None);
    }
}
