use crate::event::ModelOptionEntry;
pub(crate) use crate::selector::{SelectorMode, selector_mode};
use unicode_segmentation::UnicodeSegmentation;
#[cfg(ui)]
pub(crate) use vmux_ui::prompt_recall::{
    PromptHistoryDirection, move_prompt_history, prompt_history_direction,
};

const CHAT_PAGE_TITLE_MAX_GRAPHEMES: usize = 64;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PromptEdit {
    Insert(String),
    Backspace,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResumeMenuState {
    Loading,
    Empty,
    NoMatch,
    Results,
}

impl ResumeMenuState {
    pub(crate) fn resolve(active: bool, loading: bool, query: &str, result_count: usize) -> Self {
        if !active || loading {
            Self::Loading
        } else if result_count == 0 && !query.trim().is_empty() {
            Self::NoMatch
        } else if result_count == 0 {
            Self::Empty
        } else {
            Self::Results
        }
    }
}

pub(crate) struct ModelOptions(Vec<ModelOptionEntry>);

impl ModelOptions {
    pub(crate) fn new(models: Vec<ModelOptionEntry>) -> Self {
        Self(models)
    }

    pub(crate) fn filtered(&self, query: &str) -> Vec<ModelOptionEntry> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return self.0.clone();
        }
        let mut matching = Vec::new();
        for model in &self.0 {
            if model.id.to_lowercase().contains(&query)
                || model.name.to_lowercase().contains(&query)
                || model.description.to_lowercase().contains(&query)
            {
                matching.push(model.clone());
            }
        }
        matching
    }
}

pub(crate) struct ImportedMessages(u32);

impl ImportedMessages {
    pub(crate) fn new(count: u32) -> Self {
        Self(count)
    }

    pub(crate) fn boundary(&self, message_index: usize) -> bool {
        self.0 != 0 && message_index + 1 == self.0 as usize
    }
}

pub(crate) struct ChatPageTitle;

impl ChatPageTitle {
    pub(crate) fn resolve(generated_title: &str, agent_name: &str) -> String {
        let title = Self::normalize(generated_title);
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
                if graphemes_written >= CHAT_PAGE_TITLE_MAX_GRAPHEMES {
                    truncated = true;
                    break;
                }
                title.push(' ');
                graphemes_written += 1;
                pending_space = false;
            }
            if graphemes_written >= CHAT_PAGE_TITLE_MAX_GRAPHEMES {
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

impl PromptEdit {
    pub(crate) fn apply(
        self,
        value: &str,
        selection_start: u32,
        selection_end: u32,
    ) -> (String, u32) {
        let start = Self::utf16_to_byte(value, selection_start);
        let end = Self::utf16_to_byte(value, selection_end);
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let (replace_start, replace_end, replacement) = match self {
            PromptEdit::Insert(ref text) => (start, end, text.as_str()),
            PromptEdit::Backspace if start != end => (start, end, ""),
            PromptEdit::Backspace => {
                let previous = value[..start]
                    .char_indices()
                    .next_back()
                    .map(|(index, _)| index)
                    .unwrap_or(start);
                (previous, start, "")
            }
            PromptEdit::Delete if start != end => (start, end, ""),
            PromptEdit::Delete => {
                let next = value[end..]
                    .chars()
                    .next()
                    .map(|character| end + character.len_utf8())
                    .unwrap_or(end);
                (end, next, "")
            }
        };
        let mut updated =
            String::with_capacity(value.len() - (replace_end - replace_start) + replacement.len());
        updated.push_str(&value[..replace_start]);
        updated.push_str(replacement);
        updated.push_str(&value[replace_end..]);
        let caret_byte = replace_start + replacement.len();
        let caret_utf16 = updated[..caret_byte].encode_utf16().count() as u32;
        (updated, caret_utf16)
    }

    fn utf16_to_byte(value: &str, offset: u32) -> usize {
        let mut units = 0u32;
        for (byte, character) in value.char_indices() {
            if units >= offset {
                return byte;
            }
            units += character.len_utf16() as u32;
        }
        value.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_mode_distinguishes_mcp_and_other_selector_arguments() {
        assert_eq!(selector_mode("hello"), SelectorMode::None);
        assert_eq!(selector_mode("/res"), SelectorMode::Commands("res"));
        assert_eq!(selector_mode("/resume"), SelectorMode::Commands("resume"));
        assert_eq!(selector_mode("/resume "), SelectorMode::Resume(""));
        assert_eq!(selector_mode("/model"), SelectorMode::Commands("model"));
        assert_eq!(selector_mode("/model son"), SelectorMode::Models("son"));
        assert_eq!(selector_mode("/mcp"), SelectorMode::Mcp(""));
        assert_eq!(selector_mode("/mcp lin"), SelectorMode::Mcp("lin"));
        assert_eq!(
            selector_mode("/resume  SID-9"),
            SelectorMode::Resume("SID-9")
        );
        assert_eq!(selector_mode("/unknown arg"), SelectorMode::None);
    }

    #[test]
    fn models_filter_by_name_id_and_description() {
        let models = vec![
            ModelOptionEntry {
                id: "claude-sonnet".into(),
                name: "Sonnet".into(),
                description: "Balanced".into(),
            },
            ModelOptionEntry {
                id: "claude-opus".into(),
                name: "Opus".into(),
                description: "Most capable".into(),
            },
        ];
        let models = ModelOptions::new(models);
        assert_eq!(models.filtered("son")[0].id, "claude-sonnet");
        assert_eq!(models.filtered("capable")[0].id, "claude-opus");
        assert_eq!(models.filtered("claude-opus")[0].name, "Opus");
    }

    #[test]
    fn resume_menu_distinguishes_loading_from_loaded_empty() {
        assert_eq!(
            ResumeMenuState::resolve(false, false, "", 0),
            ResumeMenuState::Loading
        );
        assert_eq!(
            ResumeMenuState::resolve(true, true, "", 0),
            ResumeMenuState::Loading
        );
        assert_eq!(
            ResumeMenuState::resolve(true, false, "", 0),
            ResumeMenuState::Empty
        );
        assert_eq!(
            ResumeMenuState::resolve(true, false, "missing", 0),
            ResumeMenuState::NoMatch
        );
        assert_eq!(
            ResumeMenuState::resolve(true, false, "match", 1),
            ResumeMenuState::Results
        );
    }

    #[test]
    fn chat_page_title_uses_model_written_summary() {
        assert_eq!(
            ChatPageTitle::resolve("  Refine model-generated\n summaries  ", "Codex"),
            "Refine model-generated summaries"
        );
        assert_eq!(ChatPageTitle::resolve("", "Codex"), "Codex");
    }

    #[test]
    fn chat_page_title_falls_back_to_agent_and_truncates_topic() {
        assert_eq!(ChatPageTitle::resolve("", "Codex"), "Codex");

        let generated = "a".repeat(CHAT_PAGE_TITLE_MAX_GRAPHEMES + 10);
        let title = ChatPageTitle::resolve(&generated, "Codex");
        assert_eq!(title.graphemes(true).count(), CHAT_PAGE_TITLE_MAX_GRAPHEMES);
        assert!(title.ends_with('…'));
        assert_eq!(
            ChatPageTitle::resolve("Fix \u{202E}\x1b title", "Codex"),
            "Fix title"
        );
        assert_eq!(
            ChatPageTitle::resolve("Keep 👩‍💻 and فارسی\u{200C}", "Codex"),
            "Keep 👩‍💻 and فارسی\u{200C}"
        );
    }

    #[test]
    fn prompt_edits_preserve_utf16_caret_semantics() {
        assert_eq!(
            PromptEdit::Insert("X".into()).apply("abcd", 1, 3),
            ("aXd".into(), 2)
        );
        assert_eq!(PromptEdit::Backspace.apply("a🙂b", 3, 3), ("ab".into(), 1));
        assert_eq!(PromptEdit::Delete.apply("a🙂b", 1, 1), ("ab".into(), 1));
    }

    #[test]
    fn handoff_divider_appears_after_last_imported_message() {
        let imported = ImportedMessages::new(2);
        assert!(!imported.boundary(0));
        assert!(imported.boundary(1));
        assert!(!imported.boundary(2));
        assert!(!ImportedMessages::new(0).boundary(0));
    }
}
