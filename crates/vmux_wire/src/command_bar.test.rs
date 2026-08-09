use super::*;

#[test]
fn looks_like_path_absolute() {
    assert!(looks_like_path("/usr/bin"));
    assert!(looks_like_path("/"));
}

#[test]
fn looks_like_path_home() {
    assert!(looks_like_path("~/projects"));
    assert!(looks_like_path("~/"));
}

#[test]
fn looks_like_path_relative() {
    assert!(looks_like_path("./src"));
    assert!(looks_like_path("../parent"));
}

#[test]
fn looks_like_path_with_slash() {
    assert!(looks_like_path("src/main.rs"));
    assert!(looks_like_path("foo/bar"));
}

#[test]
fn looks_like_path_rejects_urls() {
    assert!(!looks_like_path("http://example.com/path"));
    assert!(!looks_like_path("https://example.com/path"));
    assert!(!looks_like_path("google.com/maps"));
    assert!(!looks_like_path("example.com"));
}

#[test]
fn looks_like_url_protocols() {
    assert!(looks_like_url("http://example.com"));
    assert!(looks_like_url("https://example.com/path"));
    assert!(looks_like_url("file:///Users/me/main.rs"));
}

#[test]
fn looks_like_url_domain_like() {
    assert!(looks_like_url("google.com"));
    assert!(looks_like_url("google.com/maps"));
    assert!(looks_like_url("example.co.uk/page"));
}

#[test]
fn looks_like_url_data_scheme() {
    assert!(looks_like_url("data:text/html,<h1>hi</h1>"));
    assert!(looks_like_url(
        "data:text/html,<style>body{background:white}</style>"
    ));
    assert!(looks_like_url("DATA:text/html,<h1>hi</h1>"));
    assert!(looks_like_url("Data:text/html,<h1>hi</h1>"));
    assert!(!looks_like_path("data:text/html,<h1>hi</h1>"));
    assert!(!looks_like_path("DATA:text/html,<h1>hi</h1>"));
}

#[test]
fn looks_like_url_rejects_file_paths() {
    assert!(!looks_like_url("src/main.rs"));
    assert!(!looks_like_url("/usr/bin"));
    assert!(!looks_like_url("foo/bar"));
}

#[test]
fn looks_like_url_rejects_spaces() {
    assert!(!looks_like_url("search query"));
    assert!(!looks_like_url("hello world.txt"));
}

#[test]
fn multiline_prompt_with_embedded_url_is_not_a_url() {
    let prompt = "Continue DSK-627 in:\n\nWorktree:\n  /tmp/dashboard\n\nPR:\n  https://github.com/mistralai/dashboard/pull/39364";

    assert!(!looks_like_url(prompt));
    assert!(is_start_prompt_query(prompt));
}

#[test]
fn looks_like_path_rejects_bare_words() {
    assert!(!looks_like_path("mistral"));
    assert!(!looks_like_path("hello world"));
    assert!(!looks_like_path("google.com"));
}

#[test]
fn looks_like_path_rejects_spaces_with_slash() {
    assert!(!looks_like_path("some query / thing"));
}

#[test]
fn explicit_path_only_prefixed() {
    assert!(looks_like_explicit_path("/usr"));
    assert!(looks_like_explicit_path("~/foo"));
    assert!(looks_like_explicit_path("./bar"));
    assert!(looks_like_explicit_path("../baz"));
}

#[test]
fn explicit_path_rejects_bare_words() {
    assert!(!looks_like_explicit_path("mistral"));
    assert!(!looks_like_explicit_path("foo/bar"));
    assert!(!looks_like_explicit_path("google.com"));
    assert!(!looks_like_explicit_path("search query"));
}

#[test]
fn explicit_path_rejects_urls() {
    assert!(!looks_like_explicit_path("http://example.com"));
    assert!(!looks_like_explicit_path("https://example.com"));
}

#[test]
fn command_bar_open_event_carries_space_name() {
    let event = CommandBarOpenEvent {
        space_name: "Work".to_string(),
        ..Default::default()
    };

    assert_eq!(event.space_name, "Work");
}

#[test]
fn command_bar_open_event_carries_open_id() {
    let event = CommandBarOpenEvent {
        open_id: 7,
        ..Default::default()
    };

    assert_eq!(event.open_id, 7);
}

#[test]
fn command_bar_open_event_defaults_to_osr_layout() {
    let event = CommandBarOpenEvent::default();

    assert!(!event.native_windowed);
}

#[test]
fn command_bar_open_event_carries_native_windowed() {
    let event = CommandBarOpenEvent {
        native_windowed: true,
        ..Default::default()
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
    let recovered =
        rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");

    assert!(recovered.native_windowed);
}

#[test]
fn command_bar_duplicate_open_id_does_not_reset_input() {
    assert!(!command_bar_open_should_reset_input(7, 7));
    assert!(command_bar_open_should_reset_input(7, 8));
    assert!(command_bar_open_should_reset_input(0, 8));
    assert!(command_bar_open_should_reset_input(0, 0));
}

#[test]
fn command_bar_refocus_only_on_open_id_change() {
    // Fresh open (open_id changed) → focus + select-all.
    assert!(command_bar_should_refocus(u64::MAX, 0));
    assert!(command_bar_should_refocus(7, 8));
    // Live refresh reuses the same open_id → must NOT refocus (else it
    // select-alls and clobbers in-progress typing on vmux://start).
    assert!(!command_bar_should_refocus(0, 0));
    assert!(!command_bar_should_refocus(7, 7));
}

#[test]
fn command_bar_retried_open_payload_still_gets_ack() {
    assert!(command_bar_open_should_ack(7));
    assert!(!command_bar_open_should_ack(0));
}

#[test]
fn in_place_enter_opens_typed_query_without_nav_selection() {
    assert!(should_open_typed_query_on_enter(
        Some(crate::open_target::OpenTarget::InPlace),
        false,
        "https://example.com"
    ));
}

#[test]
fn in_place_enter_keeps_explicit_nav_selection() {
    assert!(!should_open_typed_query_on_enter(
        Some(crate::open_target::OpenTarget::InPlace),
        true,
        "https://example.com"
    ));
}

#[test]
fn command_query_enter_keeps_command_selection() {
    assert!(!should_open_typed_query_on_enter(
        Some(crate::open_target::OpenTarget::InPlace),
        false,
        "> close"
    ));
}

#[test]
fn in_place_enter_keeps_highlighted_suggestion_for_plain_text_query() {
    assert!(!should_open_typed_query_on_enter(
        Some(crate::open_target::OpenTarget::InPlace),
        false,
        "terminal"
    ));
}

#[test]
fn in_place_enter_opens_typed_domain_query() {
    assert!(should_open_typed_query_on_enter(
        Some(crate::open_target::OpenTarget::InPlace),
        false,
        "google.com"
    ));
}

#[test]
fn start_plain_text_is_prompt_query() {
    assert!(is_start_prompt_query("fix the failing test"));
}

#[test]
fn search_engines_build_encoded_urls() {
    assert_eq!(
        SearchEngine::Google.search_url("hello world"),
        "https://www.google.com/search?q=hello+world"
    );
    assert_eq!(
        SearchEngine::Bing.search_url("hello world"),
        "https://www.bing.com/search?q=hello+world"
    );
    assert_eq!(
        SearchEngine::DuckDuckGo.search_url("hello world"),
        "https://duckduckgo.com/?q=hello+world"
    );
    assert_eq!(
        SearchEngine::Brave.search_url("hello world"),
        "https://search.brave.com/search?q=hello+world"
    );
    assert_eq!(
        SearchEngine::Kagi.search_url("hello world"),
        "https://kagi.com/search?q=hello+world"
    );
}

#[test]
fn start_agent_name_is_still_prompt_query() {
    assert!(is_start_prompt_query("codex"));
}

#[test]
fn start_explicit_navigation_inputs_are_not_prompts() {
    for query in [
        "https://example.com",
        "example.com",
        "vmux://settings/",
        "/tmp/file",
        "~/project",
        "./src",
        "../repo",
        "> close tab",
    ] {
        assert!(!is_start_prompt_query(query), "{query}");
    }
}

#[test]
fn command_bar_open_event_carries_target_enum() {
    let event = CommandBarOpenEvent {
        target: Some(crate::open_target::OpenTarget::InNewStack),
        ..Default::default()
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
    let recovered =
        rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(
        recovered.target,
        Some(crate::open_target::OpenTarget::InNewStack)
    );
}

#[test]
fn command_bar_open_event_target_none_round_trips() {
    let event = CommandBarOpenEvent::default();
    assert_eq!(event.target, None);
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
    let recovered =
        rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(recovered.target, None);
}

#[test]
fn command_bar_open_event_carries_spaces() {
    let event = CommandBarOpenEvent {
        spaces: vec![CommandBarSpace {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: "Personal".to_string(),
            is_active: true,
            tab_count: 2,
        }],
        ..Default::default()
    };

    assert_eq!(event.spaces[0].id, "work");
    assert!(event.spaces[0].is_active);
}

#[test]
fn command_bar_open_event_carries_pages() {
    let event = CommandBarOpenEvent {
        pages: vec![CommandBarPage {
            host: "settings".to_string(),
            url: "vmux://settings/".to_string(),
            title: "Settings".to_string(),
            keywords: vec!["preferences".to_string()],
            icon: crate::icon::PageIcon::Builtin(crate::icon::BuiltinIcon::Settings),
            shortcut: String::new(),
            prompt_target: false,
        }],
        ..Default::default()
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
    let recovered =
        rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(recovered.pages.len(), 1);
    assert_eq!(recovered.pages[0].title, "Settings");
}

#[test]
fn command_bar_open_event_carries_work_and_recent() {
    let event = CommandBarOpenEvent {
        work_dirs: vec![CommandBarWorkDir {
            path: "/work/proj/main.rs".into(),
            is_dir: false,
        }],
        recent_files: vec![CommandBarRecentFile {
            url: "file:///work/proj/main.rs".into(),
            title: "main.rs".into(),
        }],
        ..Default::default()
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
    let recovered =
        rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(recovered.work_dirs.len(), 1);
    assert_eq!(recovered.work_dirs[0].path, "/work/proj/main.rs");
    assert!(!recovered.work_dirs[0].is_dir);
    assert_eq!(recovered.recent_files[0].title, "main.rs");
}
