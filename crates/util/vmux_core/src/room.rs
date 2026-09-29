use unicode_segmentation::UnicodeSegmentation;
use vmux_api::room::{EventId, MemberId, Message, RoomEvent, RoomId};

pub struct RoomEvents;

impl RoomEvents {
    pub fn from_messages(sid: &str, created_at_ms: u64, messages: &[Message]) -> Vec<RoomEvent> {
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

pub struct ConversationTitle;

impl ConversationTitle {
    const MAX_GRAPHEMES: usize = 64;

    pub fn from_messages(messages: &[Message], fallback: &str) -> String {
        for message in messages {
            let Message::User { text, .. } = message else {
                continue;
            };
            let title = Self::normalize(text);
            if !title.is_empty() {
                return title;
            }
        }
        Self::normalize(fallback)
    }

    fn normalize(value: &str) -> String {
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
                .filter(|character| !Self::disallowed(*character))
                .collect::<String>();
            if grapheme.is_empty() {
                continue;
            }
            if pending_space {
                if graphemes_written >= Self::MAX_GRAPHEMES {
                    truncated = true;
                    break;
                }
                title.push(' ');
                graphemes_written += 1;
                pending_space = false;
            }
            if graphemes_written >= Self::MAX_GRAPHEMES {
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

    fn disallowed(character: char) -> bool {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::room::AssistantBlock;

    #[test]
    fn message_projection_has_stable_order_and_reply_links() {
        let events = RoomEvents::from_messages(
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
    fn title_uses_the_first_user_prompt() {
        let messages = vec![
            Message::user("  Show me something fun.\n in terminal  "),
            Message::Assistant { blocks: Vec::new() },
            Message::user("later"),
        ];
        assert_eq!(
            ConversationTitle::from_messages(&messages, "Codex"),
            "Show me something fun. in terminal"
        );
    }

    #[test]
    fn title_falls_back_and_sanitizes() {
        assert_eq!(ConversationTitle::from_messages(&[], "Codex"), "Codex");
        assert_eq!(
            ConversationTitle::from_messages(&[Message::user("Fix \u{202e}\x1b title")], "Codex"),
            "Fix title"
        );
    }
}
