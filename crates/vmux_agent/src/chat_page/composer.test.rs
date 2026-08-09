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
    assert_eq!(selector_mode("/model"), SelectorMode::Commands("model"));
    assert_eq!(selector_mode("/model son"), SelectorMode::Models("son"));
    assert_eq!(
        selector_mode("/resume  SID-9"),
        SelectorMode::Resume("SID-9")
    );
    assert_eq!(selector_mode("/unknown arg"), SelectorMode::None);
}

#[test]
fn approval_selector_maps_allow_always_and_deny() {
    assert_eq!(approval_decision_for_index(0), Some(1));
    assert_eq!(approval_decision_for_index(1), Some(2));
    assert_eq!(approval_decision_for_index(2), Some(0));
    assert_eq!(approval_decision_for_index(3), None);
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
    assert_eq!((value.as_str(), cursor), ("unfinished", None));
}

#[test]
fn escape_clears_only_idle_unqueued_draft() {
    assert!(should_clear_draft_on_escape(false, true, false));
    assert!(!should_clear_draft_on_escape(true, true, false));
    assert!(!should_clear_draft_on_escape(false, false, false));
    assert!(!should_clear_draft_on_escape(false, true, true));
}

#[test]
fn chat_page_title_uses_model_written_summary() {
    assert_eq!(
        chat_page_title("  Refine model-generated\n summaries  ", "Codex"),
        "Refine model-generated summaries"
    );
    assert_eq!(chat_page_title("", "Codex"), "Codex");
}

#[test]
fn chat_page_title_falls_back_to_agent_and_truncates_topic() {
    assert_eq!(chat_page_title("", "Codex"), "Codex");

    let generated = "a".repeat(CHAT_PAGE_TITLE_MAX_GRAPHEMES + 10);
    let title = chat_page_title(&generated, "Codex");
    assert_eq!(title.graphemes(true).count(), CHAT_PAGE_TITLE_MAX_GRAPHEMES);
    assert!(title.ends_with('…'));
    assert_eq!(
        chat_page_title("Fix \u{202E}\x1b title", "Codex"),
        "Fix title"
    );
    assert_eq!(
        chat_page_title("Keep 👩‍💻 and فارسی\u{200C}", "Codex"),
        "Keep 👩‍💻 and فارسی\u{200C}"
    );
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
