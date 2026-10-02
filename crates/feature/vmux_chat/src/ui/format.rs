#[cfg(ui)]
pub(crate) use vmux_ui::prompt_recall::{PromptHistoryDirection, prompt_history_direction};

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

pub(crate) struct ImportedMessages(u32);

impl ImportedMessages {
    pub(crate) fn new(count: u32) -> Self {
        Self(count)
    }

    pub(crate) fn boundary(&self, message_index: usize) -> bool {
        self.0 != 0 && message_index + 1 == self.0 as usize
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
