use super::event::{ResumableSessionEntry, SlashCommandEntry};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectorMode<'a> {
    None,
    Commands(&'a str),
    Resume(&'a str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuDirection {
    Next,
    Previous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PromptEdit<'a> {
    Insert(&'a str),
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

pub(crate) fn selector_mode(draft: &str) -> SelectorMode<'_> {
    let Some(token) = draft.strip_prefix('/') else {
        return SelectorMode::None;
    };
    if let Some(rest) = token.strip_prefix("resume")
        && rest.chars().next().is_some_and(char::is_whitespace)
    {
        return SelectorMode::Resume(rest.trim_start_matches(char::is_whitespace));
    }
    if token.chars().any(char::is_whitespace) {
        SelectorMode::None
    } else {
        SelectorMode::Commands(token)
    }
}

pub(crate) fn should_fetch_resume(draft: &str, commands: &[SlashCommandEntry]) -> bool {
    match selector_mode(draft) {
        SelectorMode::Resume(_) => true,
        SelectorMode::Commands(query) => {
            let query = query.to_lowercase();
            let mut matches = commands
                .iter()
                .filter(|command| command.name.starts_with(&query));
            matches
                .next()
                .is_some_and(|command| command.name == "resume")
                && matches.next().is_none()
        }
        SelectorMode::None => false,
    }
}

pub(crate) fn filter_sessions(
    sessions: &[ResumableSessionEntry],
    query: &str,
) -> Vec<ResumableSessionEntry> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return sessions.to_vec();
    }
    sessions
        .iter()
        .filter(|session| {
            session.sid.to_lowercase().contains(&query)
                || session.title.to_lowercase().contains(&query)
                || session.cwd.to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
}

pub(crate) fn resume_menu_state(
    requested: bool,
    loading: bool,
    session_count: usize,
    filtered_count: usize,
) -> ResumeMenuState {
    if !requested || loading {
        ResumeMenuState::Loading
    } else if session_count == 0 {
        ResumeMenuState::Empty
    } else if filtered_count == 0 {
        ResumeMenuState::NoMatch
    } else {
        ResumeMenuState::Results
    }
}

pub(crate) fn menu_direction(key: &str, ctrl: bool) -> Option<MenuDirection> {
    match key {
        "ArrowDown" if !ctrl => Some(MenuDirection::Next),
        "ArrowUp" if !ctrl => Some(MenuDirection::Previous),
        "n" | "N" if ctrl => Some(MenuDirection::Next),
        "p" | "P" if ctrl => Some(MenuDirection::Previous),
        _ => None,
    }
}

pub(crate) fn move_selection(current: usize, len: usize, direction: MenuDirection) -> usize {
    if len == 0 {
        return 0;
    }
    match direction {
        MenuDirection::Next => (current + 1) % len,
        MenuDirection::Previous => (current + len - 1) % len,
    }
}

pub(crate) fn is_handoff_boundary(message_index: usize, imported_message_count: u32) -> bool {
    imported_message_count != 0 && message_index + 1 == imported_message_count as usize
}

pub(crate) fn edit_prompt(
    value: &str,
    selection_start: u32,
    selection_end: u32,
    edit: PromptEdit<'_>,
) -> (String, u32) {
    let start = utf16_to_byte(value, selection_start);
    let end = utf16_to_byte(value, selection_end);
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let (replace_start, replace_end, replacement) = match edit {
        PromptEdit::Insert(text) => (start, end, text),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_page::event::SlashCommandEntry;

    fn session(sid: &str, title: &str, cwd: &str) -> ResumableSessionEntry {
        ResumableSessionEntry {
            sid: sid.into(),
            title: title.into(),
            cwd: cwd.into(),
            ..Default::default()
        }
    }

    #[test]
    fn selector_mode_distinguishes_commands_and_resume_arguments() {
        assert_eq!(selector_mode("hello"), SelectorMode::None);
        assert_eq!(selector_mode("/res"), SelectorMode::Commands("res"));
        assert_eq!(selector_mode("/resume"), SelectorMode::Commands("resume"));
        assert_eq!(selector_mode("/resume "), SelectorMode::Resume(""));
        assert_eq!(
            selector_mode("/resume  SID-9"),
            SelectorMode::Resume("SID-9")
        );
        assert_eq!(selector_mode("/unknown arg"), SelectorMode::None);
    }

    #[test]
    fn resume_filter_matches_sid_title_and_cwd_case_insensitively() {
        let sessions = vec![
            session("SID-ABC", "Fix auth", "/work/api"),
            session("sid-def", "Docs", "/work/site"),
        ];
        assert_eq!(filter_sessions(&sessions, "abc")[0].sid, "SID-ABC");
        assert_eq!(filter_sessions(&sessions, "AUTH")[0].sid, "SID-ABC");
        assert_eq!(filter_sessions(&sessions, "SITE")[0].sid, "sid-def");
        assert!(filter_sessions(&sessions, "missing").is_empty());
    }

    #[test]
    fn resume_menu_distinguishes_loading_from_loaded_empty() {
        assert_eq!(
            resume_menu_state(false, false, 0, 0),
            ResumeMenuState::Loading
        );
        assert_eq!(
            resume_menu_state(true, true, 0, 0),
            ResumeMenuState::Loading
        );
        assert_eq!(resume_menu_state(true, false, 0, 0), ResumeMenuState::Empty);
        assert_eq!(
            resume_menu_state(true, false, 2, 0),
            ResumeMenuState::NoMatch
        );
        assert_eq!(
            resume_menu_state(true, false, 2, 1),
            ResumeMenuState::Results
        );
    }

    #[test]
    fn resume_prefetch_starts_only_for_resume_as_the_sole_match() {
        let commands = vec![
            SlashCommandEntry {
                name: "resume".into(),
                ..Default::default()
            },
            SlashCommandEntry {
                name: "cli".into(),
                ..Default::default()
            },
        ];
        assert!(should_fetch_resume("/r", &commands));
        assert!(should_fetch_resume("/resume", &commands));
        assert!(should_fetch_resume("/resume ", &commands));
        assert!(!should_fetch_resume("/", &commands));
        assert!(!should_fetch_resume("/c", &commands));
        assert!(!should_fetch_resume("hello", &commands));
    }

    #[test]
    fn menu_navigation_wraps_and_empty_stays_zero() {
        assert_eq!(move_selection(0, 3, MenuDirection::Previous), 2);
        assert_eq!(move_selection(2, 3, MenuDirection::Next), 0);
        assert_eq!(move_selection(7, 0, MenuDirection::Next), 0);
        assert_eq!(menu_direction("n", true), Some(MenuDirection::Next));
        assert_eq!(menu_direction("p", true), Some(MenuDirection::Previous));
        assert_eq!(menu_direction("n", false), None);
        assert_eq!(menu_direction("ArrowDown", true), None);
    }

    #[test]
    fn prompt_edits_preserve_utf16_caret_semantics() {
        assert_eq!(
            edit_prompt("abcd", 1, 3, PromptEdit::Insert("X")),
            ("aXd".into(), 2)
        );
        assert_eq!(
            edit_prompt("a🙂b", 3, 3, PromptEdit::Backspace),
            ("ab".into(), 1)
        );
        assert_eq!(
            edit_prompt("a🙂b", 1, 1, PromptEdit::Delete),
            ("ab".into(), 1)
        );
    }

    #[test]
    fn handoff_divider_appears_after_last_imported_message() {
        assert!(!is_handoff_boundary(0, 2));
        assert!(is_handoff_boundary(1, 2));
        assert!(!is_handoff_boundary(2, 2));
        assert!(!is_handoff_boundary(0, 0));
    }
}
