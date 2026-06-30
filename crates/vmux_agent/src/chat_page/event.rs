//! Shared bin-ipc payloads for the `vmux://agent` chat page. Compiled for both native
//! (emit/receive in the Bevy host) and wasm (the Dioxus page). rkyv for the bin-ipc wire;
//! serde for the JSON-encoded message list.

/// Bin-event id: native → page conversation/run-state snapshot.
pub const CHAT_SNAPSHOT_EVENT: &str = "chat_snapshot";

/// Native → page: the full conversation plus run-state, pushed on every change.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatSnapshot {
    /// `serde_json` of `Vec<ChatMessage>` (shape matches `vmux_service::message::Message`).
    pub messages_json: String,
    /// `idle` | `streaming` | `awaiting` | `errored`.
    pub status: String,
    /// Populated when `status == "errored"`.
    pub error: String,
    /// Populated when `status == "awaiting"`.
    pub approval_call_id: String,
    pub approval_name: String,
}

/// Page → native: the user submitted a prompt.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatSubmit {
    pub text: String,
}

/// Page → native: the user answered a permission prompt. `decision`: 0 = deny, 1 = allow,
/// 2 = allow-always.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatApproval {
    pub call_id: String,
    pub decision: u8,
}

/// Page-side mirror of `vmux_service::message::Message` (which is native-only). The JSON
/// representation is identical, so the page deserializes `messages_json` into this.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ChatMessage {
    User {
        text: String,
    },
    Assistant {
        blocks: Vec<ChatBlock>,
    },
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
}

/// Mirror of `vmux_service::message::AssistantBlock`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ChatBlock {
    Text(String),
    ToolUse {
        call_id: String,
        name: String,
        args: String,
    },
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn chat_snapshot_rkyv_roundtrip() {
        let v = ChatSnapshot {
            messages_json: "[]".to_string(),
            status: "streaming".to_string(),
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
        let back = rkyv::from_bytes::<ChatSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.status, "streaming");
    }

    #[test]
    fn chat_message_mirror_matches_service_message_json() {
        // The page deserializes the native Message JSON into ChatMessage; the shapes must match.
        let json = r#"[{"Assistant":{"blocks":[{"Text":"hi"},{"ToolUse":{"call_id":"c","name":"run","args":"{}"}}]}}]"#;
        let parsed: Vec<ChatMessage> = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.len(), 1);
        match &parsed[0] {
            ChatMessage::Assistant { blocks } => assert_eq!(blocks.len(), 2),
            _ => panic!("expected assistant"),
        }
    }
}
