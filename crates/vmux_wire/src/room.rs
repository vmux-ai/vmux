//! Rooms, transcripts and the session views a client renders.
//!
//! Lives here rather than in `vmux_remote` so that crate stays transport-only: the relay links it
//! to move bytes, and a relay that cannot name a `Message` cannot decode one.

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

pub use crate::prompt_media::{InlineMediaQuery, inline_media_query, replace_inline_media_query};
pub use crate::protocol::AgentAttachment;
use crate::protocol::AgentRunStatus;

pub const CONVERSATION_TITLE_MAX_GRAPHEMES: usize = 64;
use vmux_macro::string_id;

/// Identifies one conversation. Derived from a session id, so a client can address a room
/// without having seen it before.
#[string_id]
pub struct RoomId(pub String);

impl RoomId {
    pub fn for_session(sid: &str) -> Self {
        Self::new(format!("session:{sid}"))
    }
}

/// Identifies one participant in a room — the user, or an agent.
#[string_id]
pub struct MemberId(pub String);

impl MemberId {
    /// The human on this desktop.
    pub fn local(room_id: &RoomId) -> Self {
        Self::new(format!("{}:member:local", room_id.as_str()))
    }

    /// The agent answering in this room.
    pub fn agent(room_id: &RoomId) -> Self {
        Self::new(format!("{}:member:agent", room_id.as_str()))
    }
}

/// Identifies one event in a room's append-only log.
#[string_id]
pub struct EventId(pub String);

/// Idempotency key for an operation a client may retry after a dropped connection.
#[string_id]
pub struct ClientOpId(pub String);
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    Eq,
    PartialEq,
    Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RoomRole {
    Owner,
    Participant,
    Observer,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    Eq,
    PartialEq,
    Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum MemberKind {
    Human,
    Agent,
    System,
}

#[derive(
    Clone,
    Debug,
    Deserialize,
    Eq,
    PartialEq,
    Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RoomMember {
    pub room_id: RoomId,
    pub member_id: MemberId,
    pub display_name: String,
    pub role: RoomRole,
    pub kind: MemberKind,
}

#[derive(
    Clone,
    Debug,
    Serialize,
    Deserialize,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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

#[derive(
    Clone,
    Debug,
    Deserialize,
    PartialEq,
    Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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

impl RoomEvent {
    /// Project a transcript into the append-only log a client replays.
    ///
    /// Sequence numbers and event ids come from the position in `messages`, so the same transcript
    /// always projects to the same log — a client that reconnects sees the ids it already has.
    /// Each assistant event points back at the user message it answers.
    pub fn from_messages(sid: &str, created_at_ms: u64, messages: &[Message]) -> Vec<Self> {
        let room_id = RoomId::for_session(sid);
        let local_member = MemberId::local(&room_id);
        let agent_member = MemberId::agent(&room_id);
        let mut events = Vec::with_capacity(messages.len());
        let mut reply_to = None;
        for (index, message) in messages.iter().enumerate() {
            let server_seq = index as u64 + 1;
            let event_id = EventId::new(format!("{}:event:{server_seq}", room_id.as_str()));
            let is_user = matches!(message, Message::User { .. });
            events.push(RoomEvent {
                event_id: event_id.clone(),
                room_id: room_id.clone(),
                actor_id: if is_user {
                    local_member.clone()
                } else {
                    agent_member.clone()
                },
                client_op_id: None,
                server_seq,
                created_at_ms: created_at_ms.saturating_add(index as u64),
                reply_to: if is_user { None } else { reply_to.clone() },
                message: message.clone(),
            });
            if is_user {
                reply_to = Some(event_id);
            }
        }
        events
    }
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

    /// Name a conversation after its first user prompt, or `fallback` when it has none.
    pub fn conversation_title(messages: &[Self], fallback: &str) -> String {
        for message in messages {
            let Self::User { text, .. } = message else {
                continue;
            };
            let title = normalize_conversation_title(text);
            if !title.is_empty() {
                return title;
            }
        }
        normalize_conversation_title(fallback)
    }
}

/// Trim a prompt to a title: collapse whitespace, drop anything invisible, cap the length.
///
/// The character filter is a spoofing defence, not tidiness. Titles are rendered next to names the
/// user trusts, and a bidi override smuggled into a prompt would let a title reorder what it sits
/// beside.
fn normalize_conversation_title(value: &str) -> String {
    let mut title = String::new();
    let mut graphemes_written = 0;
    let mut pending_space = false;
    let mut truncated = false;

    for grapheme in value.graphemes(true) {
        if grapheme.chars().all(char::is_whitespace) {
            pending_space = !title.is_empty();
            continue;
        }
        let grapheme = grapheme
            .chars()
            .filter(|character| !is_disallowed_title_char(*character))
            .collect::<String>();
        if grapheme.is_empty() {
            continue;
        }
        if pending_space {
            if graphemes_written >= CONVERSATION_TITLE_MAX_GRAPHEMES {
                truncated = true;
                break;
            }
            title.push(' ');
            graphemes_written += 1;
            pending_space = false;
        }
        if graphemes_written >= CONVERSATION_TITLE_MAX_GRAPHEMES {
            truncated = true;
            break;
        }
        title.push_str(&grapheme);
        graphemes_written += 1;
    }

    if truncated {
        if let Some((start, _)) = title.grapheme_indices(true).next_back() {
            title.truncate(start);
        }
        title.push('…');
    }
    title
}

fn is_disallowed_title_char(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{061C}'
                | '\u{180E}'
                | '\u{200B}'
                | '\u{200E}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{206F}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
                | '\u{1BCA0}'..='\u{1BCA3}'
        )
}

#[derive(
    Clone,
    Debug,
    Serialize,
    Deserialize,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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
#[derive(
    Clone,
    Debug,
    Serialize,
    Deserialize,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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
#[derive(
    Clone,
    Debug,
    Serialize,
    Deserialize,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PlanStep {
    pub content: String,
    pub status: String,
}

#[derive(
    Clone,
    Debug,
    Deserialize,
    PartialEq,
    Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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

#[derive(
    Clone,
    Debug,
    Deserialize,
    PartialEq,
    Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RemoteApproval {
    pub call_id: String,
    pub name: String,
    pub args_json: String,
}

#[derive(
    Clone,
    Debug,
    Deserialize,
    PartialEq,
    Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RemoteMediaEntry {
    pub path: String,
    pub name: String,
    pub parent: String,
    pub mime_type: String,
    pub size: u64,
    pub is_dir: bool,
    pub preview_data_url: String,
}

impl RemoteMediaEntry {
    /// How this entry is written into a prompt after an `@`.
    ///
    /// Percent-encoded, because a space would otherwise end the token the composer is matching.
    pub fn reference(&self) -> String {
        let encode = |value: &str| value.replace('%', "%25").replace(' ', "%20");
        if self.parent == "~" {
            format!("~/{name}", name = encode(&self.name))
        } else {
            format!(
                "{parent}/{name}",
                parent = encode(&self.parent),
                name = encode(&self.name)
            )
        }
    }

    /// How this entry is shown to a reader — the same path, unencoded.
    pub fn display_path(&self) -> String {
        if self.parent == "~" {
            format!("~/{}", self.name)
        } else {
            format!("{}/{}", self.parent.trim_end_matches('/'), self.name)
        }
    }
}

#[derive(
    Clone,
    Debug,
    Deserialize,
    PartialEq,
    Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RemoteSession {
    pub sid: String,
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

/// One model a session can be switched to.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RemoteModel {
    pub id: String,
    pub name: String,
}

/// The models a session can run and how hard its agent is asked to think.
///
/// Both live in the GUI's ECS rather than the daemon, so this crosses the wire as JSON in answer
/// to [`ListModels`](crate::protocol::SharedAgentCommand::ListModels) rather than as a typed
/// response.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RemoteModelState {
    pub models: Vec<RemoteModel>,
    /// The model in effect, including one selected but not yet acknowledged by the agent.
    pub selected_id: String,
    /// Empty for agents that have no effort setting, which is most of them.
    pub effort_levels: Vec<String>,
    /// Empty when the agent is left at its own default.
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
    /// Launch URL of the agent to start; omitted means the desktop default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_url: Option<String>,
}

/// An installed agent the phone can start a chat with.
#[derive(
    Clone,
    Debug,
    Deserialize,
    PartialEq,
    Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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
            agent_url: Some("vmux://agent/claude".to_string()),
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
    fn message_projection_has_stable_order_and_reply_links() {
        let events = RoomEvent::from_messages(
            "session-1",
            100,
            &[
                Message::user("hello"),
                Message::Assistant {
                    blocks: vec![AssistantBlock::Text("hi".to_string())],
                },
            ],
        );

        assert_eq!(
            events[0].event_id,
            EventId::new("session:session-1:event:1")
        );
        assert_eq!(events[1].server_seq, 2);
        assert_eq!(events[1].reply_to, Some(events[0].event_id.clone()));
        assert_eq!(events[1].created_at_ms, 101);
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

    #[test]
    fn conversation_title_uses_first_user_prompt() {
        let messages = vec![
            Message::user("  Show me something fun.\n in terminal  "),
            Message::Assistant { blocks: Vec::new() },
            Message::user("later"),
        ];
        assert_eq!(
            Message::conversation_title(&messages, "Codex"),
            "Show me something fun. in terminal"
        );
    }

    #[test]
    fn conversation_title_falls_back_and_sanitizes() {
        assert_eq!(Message::conversation_title(&[], "Codex"), "Codex");
        assert_eq!(
            Message::conversation_title(&[Message::user("Fix \u{202e}\x1b title")], "Codex"),
            "Fix title"
        );
    }
}
