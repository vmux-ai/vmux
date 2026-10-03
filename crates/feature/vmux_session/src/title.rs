use unicode_segmentation::UnicodeSegmentation;
use vmux_api::conversation::Message;

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
