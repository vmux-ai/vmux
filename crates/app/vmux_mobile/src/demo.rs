//! A canned session an App Store reviewer can open without a Mac.
//!
//! Unpaired, the app offers a QR code that does not exist and a pairing link the reviewer cannot
//! obtain, which reads as a dead end under review guidelines 2.1 and 4.2. This is the way through:
//! a session list and a transcript, assembled from the same types the real ones use.
//!
//! Nothing here talks to a relay, so it survives the Mac being off and the relay being down
//! mid-review. `RoomEvent::from_messages` derives ids, sequence numbers and reply links from
//! position, so a hand-written `Vec<Message>` becomes a well-formed log rather than a special case
//! the transcript has to know about.

use vmux_ui::i18n::translate;
use vmux_wire::room::{AssistantBlock, Message, RemoteSession, RemoteStatus, RoomEvent, RoomId};

use crate::MobileRoomProjection;

const SID: &str = "demo-session";

/// Fixed rather than `SystemTime::now()`, so the list does not read as "just now" forever.
const CREATED_AT_MS: u64 = 1_754_000_000_000;

pub fn sessions() -> Vec<RemoteSession> {
    vec![RemoteSession {
        sid: SID.to_string(),
        room_id: RoomId::for_session(SID),
        title: translate("mobile-demo-session-title"),
        name: translate("mobile-demo-session-title"),
        runtime: "claude".to_string(),
        model: Some("claude-opus-4-8".to_string()),
        cwd: "~/Projects/acme-api".to_string(),
        status: RemoteStatus::Idle,
        approval: None,
        created_at_ms: CREATED_AT_MS,
    }]
}

/// The transcript behind that session.
///
/// Deliberately more than prose: a reviewer should see that this is an agent doing work, so it
/// carries reasoning, a tool call and a diff rather than two chat bubbles.
pub fn room() -> MobileRoomProjection {
    let events = RoomEvent::from_messages(
        SID,
        CREATED_AT_MS,
        &[
            Message::user(translate("mobile-demo-prompt")),
            Message::Assistant {
                blocks: vec![AssistantBlock::Thinking(translate("mobile-demo-thinking"))],
            },
            Message::Assistant {
                blocks: vec![AssistantBlock::ToolUse {
                    call_id: "demo-tool-1".to_string(),
                    name: "read_file".to_string(),
                    args: "{\"path\":\"src/handlers/orders.rs\"}".to_string(),
                    parent_call_id: None,
                }],
            },
            Message::ToolResult {
                call_id: "demo-tool-1".to_string(),
                content: "src/handlers/orders.rs (84 lines)".to_string(),
                is_error: false,
            },
            Message::Assistant {
                blocks: vec![AssistantBlock::Diff {
                    call_id: "demo-tool-2".to_string(),
                    path: "src/handlers/orders.rs".to_string(),
                    old_text: Some(
                        "    let total = items.iter().map(|i| i.price).sum();\n".to_string(),
                    ),
                    new_text: "    let total: Cents = items.iter().map(|i| i.price).sum();\n"
                        .to_string(),
                }],
            },
            Message::Assistant {
                blocks: vec![AssistantBlock::Text(translate("mobile-demo-answer"))],
            },
        ],
    );

    MobileRoomProjection {
        room_id: Some(RoomId::for_session(SID)),
        through_seq: events.len() as u64,
        events,
    }
}
