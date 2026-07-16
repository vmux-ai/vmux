use super::event::{
    ChatBlock, ChatItem, ModelOptionEntry, ResumableSessionEntry, SlashCommandEntry,
};
use unicode_segmentation::UnicodeSegmentation;

const CHAT_PAGE_TITLE_MAX_GRAPHEMES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectorMode<'a> {
    None,
    Commands(&'a str),
    Resume(&'a str),
    Models(&'a str),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolActivity {
    Guardian,
    ReadFile,
    Image,
    Browser,
    Search,
    Command,
    Other,
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
    if let Some(rest) = token.strip_prefix("model")
        && rest.chars().next().is_some_and(char::is_whitespace)
    {
        return SelectorMode::Models(rest.trim_start_matches(char::is_whitespace));
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
        SelectorMode::Models(_) => false,
    }
}

pub(crate) fn filter_models(models: &[ModelOptionEntry], query: &str) -> Vec<ModelOptionEntry> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return models.to_vec();
    }
    models
        .iter()
        .filter(|model| {
            model.id.to_lowercase().contains(&query)
                || model.name.to_lowercase().contains(&query)
                || model.description.to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
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

pub(crate) fn should_clear_draft_on_escape(
    streaming: bool,
    queue_empty: bool,
    draft_empty: bool,
) -> bool {
    !streaming && queue_empty && !draft_empty
}

pub(crate) fn chat_page_title(items: &[ChatItem], status: &str, agent_name: &str) -> String {
    let topic = items
        .iter()
        .filter_map(|item| match item {
            ChatItem::User { text } => Some(text.as_str()),
            ChatItem::Turn(_) => None,
        })
        .map(normalize_chat_page_title)
        .find(|title| !title.is_empty())
        .unwrap_or_else(|| normalize_chat_page_title(agent_name));

    match status {
        "streaming" => format!("{} {topic}", streaming_title_emoji(items)),
        "installing" => format!("📦 {topic}"),
        "awaiting" => format!("✋ {topic}"),
        "errored" => format!("❌ {topic}"),
        _ => topic,
    }
}

fn streaming_title_emoji(items: &[ChatItem]) -> &'static str {
    let block = items
        .iter()
        .rev()
        .find_map(|item| match item {
            ChatItem::Turn(turn) if turn.running => Some(turn.blocks.last()),
            _ => None,
        })
        .flatten();

    match block {
        Some(ChatBlock::Text(_)) => "✍️",
        Some(ChatBlock::Thinking(_)) | Some(ChatBlock::ToolResult { .. }) | None => "🧠",
        Some(ChatBlock::ToolUse { name, .. }) => tool_activity_emoji(tool_activity(name)),
        Some(ChatBlock::Diff { .. }) => "✏️",
        Some(ChatBlock::Plan { .. }) => "📋",
        Some(ChatBlock::Reconnect { .. }) => "🛜",
    }
}

pub(crate) fn tool_activity(name: &str) -> ToolActivity {
    let lower = name.to_ascii_lowercase();
    if lower.contains("guardian")
        || lower.contains("approval")
        || lower == "review"
        || lower.ends_with("_review")
        || lower.ends_with(".review")
        || lower.ends_with(":review")
    {
        ToolActivity::Guardian
    } else if lower.contains("read_file") || lower.contains("read file") {
        ToolActivity::ReadFile
    } else if lower.contains("view_image") || lower.contains("view image") {
        ToolActivity::Image
    } else if lower.contains("browser") || lower.contains("navigate") || lower.contains("web_") {
        ToolActivity::Browser
    } else if lower.contains("grep") || lower.contains("search") || lower.contains("find") {
        ToolActivity::Search
    } else if lower.contains("run")
        || lower.contains("exec")
        || lower.contains("command")
        || lower.contains("shell")
        || lower.contains("terminal")
    {
        ToolActivity::Command
    } else {
        ToolActivity::Other
    }
}

fn tool_activity_emoji(activity: ToolActivity) -> &'static str {
    match activity {
        ToolActivity::Guardian => "🛡️",
        ToolActivity::ReadFile => "📄",
        ToolActivity::Image => "🖼️",
        ToolActivity::Browser => "🌐",
        ToolActivity::Search => "🔎",
        ToolActivity::Command => "💻",
        ToolActivity::Other => "🛠️",
    }
}

fn normalize_chat_page_title(value: &str) -> String {
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
            .filter(|character| !is_disallowed_chat_page_title_char(*character))
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

fn is_disallowed_chat_page_title_char(character: char) -> bool {
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
    use crate::chat_page::event::{ChatTurn, SlashCommandEntry};

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
        assert_eq!(selector_mode("/model"), SelectorMode::Commands("model"));
        assert_eq!(selector_mode("/model son"), SelectorMode::Models("son"));
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
        assert_eq!(filter_models(&models, "son")[0].id, "claude-sonnet");
        assert_eq!(filter_models(&models, "capable")[0].id, "claude-opus");
        assert_eq!(filter_models(&models, "claude-opus")[0].name, "Opus");
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
    fn escape_clears_only_idle_unqueued_draft() {
        assert!(should_clear_draft_on_escape(false, true, false));
        assert!(!should_clear_draft_on_escape(true, true, false));
        assert!(!should_clear_draft_on_escape(false, false, false));
        assert!(!should_clear_draft_on_escape(false, true, true));
    }

    #[test]
    fn chat_page_title_uses_first_prompt_as_stable_topic() {
        let items = vec![
            ChatItem::User {
                text: "  Fix the\ndynamic   agent page title  ".into(),
            },
            ChatItem::Turn(ChatTurn {
                running: true,
                ..Default::default()
            }),
            ChatItem::User {
                text: "continue".into(),
            },
        ];

        assert_eq!(
            chat_page_title(&items, "streaming", "Codex"),
            "🧠 Fix the dynamic agent page title"
        );
        assert_eq!(
            chat_page_title(&items, "awaiting", "Codex"),
            "✋ Fix the dynamic agent page title"
        );
    }

    #[test]
    fn chat_page_title_tracks_live_timeline_activity() {
        fn title(block: ChatBlock) -> String {
            chat_page_title(
                &[
                    ChatItem::User {
                        text: "Ship dynamic titles".into(),
                    },
                    ChatItem::Turn(ChatTurn {
                        blocks: vec![block],
                        running: true,
                        ..Default::default()
                    }),
                ],
                "streaming",
                "Codex",
            )
        }

        assert_eq!(
            title(ChatBlock::Thinking("hmm".into())),
            "🧠 Ship dynamic titles"
        );
        assert_eq!(
            title(ChatBlock::ToolUse {
                call_id: "1".into(),
                name: "functions.exec_command".into(),
                args: "{}".into(),
            }),
            "💻 Ship dynamic titles"
        );
        assert_eq!(
            title(ChatBlock::ToolUse {
                call_id: "2".into(),
                name: "search_files".into(),
                args: "{}".into(),
            }),
            "🔎 Ship dynamic titles"
        );
        assert_eq!(
            title(ChatBlock::Plan { steps: Vec::new() }),
            "📋 Ship dynamic titles"
        );
        assert_eq!(
            title(ChatBlock::Diff {
                call_id: "3".into(),
                path: "page.rs".into(),
                old_text: None,
                new_text: String::new(),
            }),
            "✏️ Ship dynamic titles"
        );
        assert_eq!(
            title(ChatBlock::Text("done soon".into())),
            "✍️ Ship dynamic titles"
        );
        assert_eq!(
            title(ChatBlock::Reconnect {
                attempt: 2,
                total: 5,
            }),
            "🛜 Ship dynamic titles"
        );
    }

    #[test]
    fn chat_page_title_falls_back_to_agent_and_truncates_topic() {
        assert_eq!(chat_page_title(&[], "idle", "Codex"), "Codex");

        let items = vec![ChatItem::User {
            text: "a".repeat(CHAT_PAGE_TITLE_MAX_GRAPHEMES + 10),
        }];
        let title = chat_page_title(&items, "idle", "Codex");
        assert_eq!(title.graphemes(true).count(), CHAT_PAGE_TITLE_MAX_GRAPHEMES);
        assert!(title.ends_with('…'));
        assert_eq!(
            chat_page_title(
                &[ChatItem::User {
                    text: "Fix \u{202E}\x1b title".into(),
                }],
                "idle",
                "Codex"
            ),
            "Fix title"
        );
        assert_eq!(
            chat_page_title(
                &[
                    ChatItem::User {
                        text: "\u{202E}".into(),
                    },
                    ChatItem::User {
                        text: "Keep 👩‍💻 and فارسی\u{200C}".into(),
                    },
                ],
                "errored",
                "Codex"
            ),
            "❌ Keep 👩‍💻 and فارسی\u{200C}"
        );
        assert_eq!(chat_page_title(&[], "installing", "Codex"), "📦 Codex");
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
