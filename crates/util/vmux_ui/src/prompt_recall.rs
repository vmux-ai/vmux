#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptHistoryDirection {
    Older,
    Newer,
}

impl PromptHistoryDirection {
    pub fn from_menu(next: Option<bool>) -> Option<Self> {
        Some(if next? { Self::Newer } else { Self::Older })
    }
}

impl PromptHistoryDirection {
    pub fn from_key(
        key: &str,
        ctrl: bool,
        value: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<Self> {
        if selection_start != selection_end {
            return None;
        }
        if ctrl {
            return match key {
                "n" | "N" => Some(Self::Newer),
                "p" | "P" => Some(Self::Older),
                _ => None,
            };
        }
        let caret = selection_start.min(value.len());
        match key {
            "ArrowUp" if !value[..caret].contains('\n') => Some(Self::Older),
            "ArrowDown" if !value[caret..].contains('\n') => Some(Self::Newer),
            _ => None,
        }
    }

    pub fn move_in(
        self,
        history: &[String],
        cursor: Option<usize>,
        scratch: &str,
        current: &str,
    ) -> (String, Option<usize>, String) {
        if history.is_empty() {
            return (current.to_string(), cursor, scratch.to_string());
        }
        match self {
            Self::Older => {
                let next = cursor.map_or(history.len() - 1, |index| index.saturating_sub(1));
                let scratch = if cursor.is_none() { current } else { scratch };
                (history[next].clone(), Some(next), scratch.to_string())
            }
            Self::Newer => match cursor {
                Some(index) if index + 1 < history.len() => (
                    history[index + 1].clone(),
                    Some(index + 1),
                    scratch.to_string(),
                ),
                Some(_) => (scratch.to_string(), None, scratch.to_string()),
                None => (current.to_string(), None, scratch.to_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_history_uses_arrows_at_text_boundaries_and_ctrl_np_anywhere() {
        assert_eq!(
            PromptHistoryDirection::from_key("ArrowUp", false, "first\nsecond", 2, 2),
            Some(PromptHistoryDirection::Older)
        );
        assert_eq!(
            PromptHistoryDirection::from_key("ArrowUp", false, "first\nsecond", 8, 8),
            None
        );
        assert_eq!(
            PromptHistoryDirection::from_key("ArrowDown", false, "first\nsecond", 8, 8),
            Some(PromptHistoryDirection::Newer)
        );
        assert_eq!(
            PromptHistoryDirection::from_key("p", true, "first\nsecond", 8, 8),
            Some(PromptHistoryDirection::Older)
        );
        assert_eq!(
            PromptHistoryDirection::from_key("n", true, "first\nsecond", 2, 4),
            None
        );
    }

    #[test]
    fn prompt_history_restores_unsent_scratch_after_newest_entry() {
        let history = vec!["first".to_string(), "second".to_string()];
        let (value, cursor, scratch) =
            PromptHistoryDirection::Older.move_in(&history, None, "", "unfinished");
        assert_eq!(
            (value.as_str(), cursor, scratch.as_str()),
            ("second", Some(1), "unfinished")
        );

        let (value, cursor, scratch) =
            PromptHistoryDirection::Older.move_in(&history, cursor, &scratch, &value);
        assert_eq!((value.as_str(), cursor), ("first", Some(0)));

        let (value, cursor, scratch) =
            PromptHistoryDirection::Newer.move_in(&history, cursor, &scratch, &value);
        assert_eq!((value.as_str(), cursor), ("second", Some(1)));

        let (value, cursor, _) =
            PromptHistoryDirection::Newer.move_in(&history, cursor, &scratch, &value);
        assert_eq!(
            (value.as_str(), cursor),
            ("unfinished", None),
            "walking back past the newest entry has to return what the reader had typed"
        );
    }
}
