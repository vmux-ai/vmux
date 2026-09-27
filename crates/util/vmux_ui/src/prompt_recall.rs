use crate::caret::utf16_offset_to_byte;
use crate::list_nav::MenuDirection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptHistoryDirection {
    Older,
    Newer,
}

impl PromptHistoryDirection {
    pub fn from_menu(direction: Option<MenuDirection>) -> Option<Self> {
        match direction? {
            MenuDirection::Previous => Some(Self::Older),
            MenuDirection::Next => Some(Self::Newer),
        }
    }
}

pub fn prompt_history_direction(
    key: &str,
    ctrl: bool,
    value: &str,
    selection_start: u32,
    selection_end: u32,
) -> Option<PromptHistoryDirection> {
    if selection_start != selection_end {
        return None;
    }
    if ctrl {
        return match key {
            "n" | "N" => Some(PromptHistoryDirection::Newer),
            "p" | "P" => Some(PromptHistoryDirection::Older),
            _ => None,
        };
    }
    let caret = utf16_offset_to_byte(value, selection_start);
    match key {
        "ArrowUp" if !value[..caret].contains('\n') => Some(PromptHistoryDirection::Older),
        "ArrowDown" if !value[caret..].contains('\n') => Some(PromptHistoryDirection::Newer),
        _ => None,
    }
}

pub fn move_prompt_history(
    history: &[String],
    cursor: Option<usize>,
    scratch: &str,
    current: &str,
    direction: PromptHistoryDirection,
) -> (String, Option<usize>, String) {
    if history.is_empty() {
        return (current.to_string(), cursor, scratch.to_string());
    }
    match direction {
        PromptHistoryDirection::Older => {
            let next = cursor.map_or(history.len() - 1, |index| index.saturating_sub(1));
            let scratch = if cursor.is_none() { current } else { scratch };
            (history[next].clone(), Some(next), scratch.to_string())
        }
        PromptHistoryDirection::Newer => match cursor {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_history_uses_arrows_at_text_boundaries_and_ctrl_np_anywhere() {
        assert_eq!(
            prompt_history_direction("ArrowUp", false, "first\nsecond", 2, 2),
            Some(PromptHistoryDirection::Older)
        );
        assert_eq!(
            prompt_history_direction("ArrowUp", false, "first\nsecond", 8, 8),
            None
        );
        assert_eq!(
            prompt_history_direction("ArrowDown", false, "first\nsecond", 8, 8),
            Some(PromptHistoryDirection::Newer)
        );
        assert_eq!(
            prompt_history_direction("p", true, "first\nsecond", 8, 8),
            Some(PromptHistoryDirection::Older)
        );
        assert_eq!(
            prompt_history_direction("n", true, "first\nsecond", 2, 4),
            None
        );
    }

    #[test]
    fn prompt_history_restores_unsent_scratch_after_newest_entry() {
        let history = vec!["first".to_string(), "second".to_string()];
        let (value, cursor, scratch) = move_prompt_history(
            &history,
            None,
            "",
            "unfinished",
            PromptHistoryDirection::Older,
        );
        assert_eq!(
            (value.as_str(), cursor, scratch.as_str()),
            ("second", Some(1), "unfinished")
        );

        let (value, cursor, scratch) = move_prompt_history(
            &history,
            cursor,
            &scratch,
            &value,
            PromptHistoryDirection::Older,
        );
        assert_eq!((value.as_str(), cursor), ("first", Some(0)));

        let (value, cursor, scratch) = move_prompt_history(
            &history,
            cursor,
            &scratch,
            &value,
            PromptHistoryDirection::Newer,
        );
        assert_eq!((value.as_str(), cursor), ("second", Some(1)));

        let (value, cursor, _) = move_prompt_history(
            &history,
            cursor,
            &scratch,
            &value,
            PromptHistoryDirection::Newer,
        );
        assert_eq!(
            (value.as_str(), cursor),
            ("unfinished", None),
            "walking back past the newest entry has to return what the reader had typed"
        );
    }
}
