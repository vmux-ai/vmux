use unicode_segmentation::UnicodeSegmentation;

use crate::event::ChatSnapshot;
use crate::tab::Accent;

pub(super) struct ChatPresentation;

impl ChatPresentation {
    const MAX_TITLE_GRAPHEMES: usize = 64;

    pub(super) fn apply(snapshot: &mut ChatSnapshot, transcript_empty: bool) {
        snapshot.header_name = if snapshot.agent_name.is_empty() {
            snapshot.agent_id.clone()
        } else {
            snapshot.agent_name.clone()
        };
        snapshot.page_title = Self::title(&snapshot.conversation_title, &snapshot.header_name);
        let accent = Accent::for_agent(&snapshot.accent_color, &snapshot.agent_id);
        snapshot.accent_color = accent.css;
        snapshot.accent_rgb = accent.rgb;
        snapshot.installing = snapshot.status == "installing";
        snapshot.installing_splash = snapshot.installing && transcript_empty;
        snapshot.streaming = matches!(snapshot.status.as_str(), "streaming" | "awaiting");
        snapshot.choice_pending =
            !snapshot.choice_options.is_empty() || snapshot.approval.is_some();
    }

    fn title(generated: &str, agent_name: &str) -> String {
        let title = Self::normalize(generated);
        if title.is_empty() {
            return Self::normalize(agent_name);
        }
        title
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
                if graphemes_written >= Self::MAX_TITLE_GRAPHEMES {
                    truncated = true;
                    break;
                }
                title.push(' ');
                graphemes_written += 1;
                pending_space = false;
            }
            if graphemes_written >= Self::MAX_TITLE_GRAPHEMES {
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
                    | '\u{2060}'..='\u{206F}'
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
    fn presentation_resolves_title_accent_and_run_flags() {
        let mut snapshot = ChatSnapshot {
            status: "awaiting".into(),
            agent_id: "codex".into(),
            conversation_title: "  Refine model-generated\n summaries  ".into(),
            choice_options: vec!["Continue".into()],
            ..Default::default()
        };

        ChatPresentation::apply(&mut snapshot, true);

        assert_eq!(snapshot.header_name, "codex");
        assert_eq!(snapshot.page_title, "Refine model-generated summaries");
        assert!(snapshot.accent_color.starts_with("rgb("));
        assert!(!snapshot.accent_rgb.is_empty());
        assert!(snapshot.streaming);
        assert!(snapshot.choice_pending);
        assert!(!snapshot.installing_splash);
    }

    #[test]
    fn presentation_sanitizes_and_truncates_generated_titles() {
        let generated = "a".repeat(ChatPresentation::MAX_TITLE_GRAPHEMES + 10);
        assert!(ChatPresentation::title(&generated, "Codex").ends_with('…'));
        assert_eq!(
            ChatPresentation::title("Fix \u{202E}\x1b title", "Codex"),
            "Fix title"
        );
        assert_eq!(
            ChatPresentation::title("Keep 👩‍💻 and فارسی\u{200C}", "Codex"),
            "Keep 👩‍💻 and فارسی\u{200C}"
        );
    }
}
