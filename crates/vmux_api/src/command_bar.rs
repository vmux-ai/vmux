pub use crate::history::{HistoryEntry, HistorySuggestionsRequest, HistorySuggestionsResponse};

mod input;
mod open;
mod picker;
mod query;
mod request;

pub use input::*;
pub use open::*;
pub use picker::*;
pub use query::*;
pub use request::*;

#[cfg(test)]
mod tests {
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
    fn a_slash_token_is_a_bare_name_and_never_a_path() {
        assert_eq!(
            CommandBarQuery("/resume").slash_token(),
            Some(("resume", ""))
        );
        assert_eq!(
            CommandBarQuery("/resume yesterday").slash_token(),
            Some(("resume", "yesterday"))
        );
        assert_eq!(
            CommandBarQuery("  /model").slash_token(),
            Some(("model", ""))
        );
        assert_eq!(
            CommandBarQuery("/Users/jun/notes.md").slash_token(),
            None,
            "a path keeps its slashes, so it can never be read as a command"
        );
        assert_eq!(CommandBarQuery("/").slash_token(), None);
        assert_eq!(CommandBarQuery("resume").slash_token(), None);
    }

    #[test]
    fn multiline_prompt_with_embedded_url_is_not_a_url() {
        let prompt = "Continue DSK-627 in:\n\nWorktree:\n  /tmp/dashboard\n\nPR:\n  https://github.com/mistralai/dashboard/pull/39364";

        assert!(!looks_like_url(prompt));
        assert!(CommandBarQuery(prompt).is_start_prompt());
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
            open_id: OpenId(7),
            ..Default::default()
        };

        assert_eq!(event.open_id, OpenId(7));
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
        assert!(!OpenId(7).should_reset_input(OpenId(7)));
        assert!(OpenId(8).should_reset_input(OpenId(7)));
        assert!(OpenId(8).should_reset_input(OpenId::NONE));
        assert!(OpenId::NONE.should_reset_input(OpenId::NONE));
    }

    #[test]
    fn command_bar_refocus_only_on_open_id_change() {
        assert!(OpenId::NONE.should_refocus(OpenId(u64::MAX)));
        assert!(OpenId(8).should_refocus(OpenId(7)));
        assert!(!OpenId::NONE.should_refocus(OpenId::NONE));
        assert!(!OpenId(7).should_refocus(OpenId(7)));
    }

    #[test]
    fn only_a_real_open_is_acked_and_revealed() {
        assert!(OpenId(7).is_open());
        assert!(!OpenId::NONE.is_open());
    }

    #[test]
    fn in_place_enter_opens_typed_query_without_nav_selection() {
        assert!(
            CommandBarQuery("https://example.com")
                .opens_typed_url_on_enter(Some(crate::open_target::OpenTarget::InPlace), false)
        );
    }

    #[test]
    fn in_place_enter_keeps_explicit_nav_selection() {
        assert!(
            !CommandBarQuery("https://example.com")
                .opens_typed_url_on_enter(Some(crate::open_target::OpenTarget::InPlace), true)
        );
    }

    #[test]
    fn command_query_enter_keeps_command_selection() {
        assert!(
            !CommandBarQuery("> close")
                .opens_typed_url_on_enter(Some(crate::open_target::OpenTarget::InPlace), false)
        );
    }

    #[test]
    fn in_place_enter_keeps_highlighted_suggestion_for_plain_text_query() {
        assert!(
            !CommandBarQuery("terminal")
                .opens_typed_url_on_enter(Some(crate::open_target::OpenTarget::InPlace), false)
        );
    }

    #[test]
    fn in_place_enter_opens_typed_domain_query() {
        assert!(
            CommandBarQuery("google.com")
                .opens_typed_url_on_enter(Some(crate::open_target::OpenTarget::InPlace), false)
        );
    }

    #[test]
    fn start_plain_text_is_prompt_query() {
        assert!(CommandBarQuery("fix the failing test").is_start_prompt());
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
        assert!(CommandBarQuery("codex").is_start_prompt());
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
            assert!(!CommandBarQuery(query).is_start_prompt(), "{query}");
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
    fn a_typed_line_number_is_one_based_and_anything_else_is_refused() {
        assert_eq!(
            CommandBarPick::goto_line("42"),
            Some(CommandBarPick::GotoLine { line: 41 })
        );
        assert_eq!(
            CommandBarPick::goto_line("  7  "),
            Some(CommandBarPick::GotoLine { line: 6 })
        );
        assert_eq!(
            CommandBarPick::goto_line("12:5"),
            Some(CommandBarPick::GotoLine { line: 11 }),
            "a pasted line:column lands on the line"
        );
        assert_eq!(
            CommandBarPick::goto_line("0"),
            Some(CommandBarPick::GotoLine { line: 0 })
        );
        for refused in ["", "abc", "-3", "3.5"] {
            assert_eq!(CommandBarPick::goto_line(refused), None, "{refused}");
        }
    }

    #[test]
    fn an_asserted_picker_survives_the_wire() {
        let event = CommandBarOpenEvent {
            picker: Some(CommandBarPicker::EncodingReopen),
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
        let recovered =
            rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");

        assert_eq!(recovered.picker, Some(CommandBarPicker::EncodingReopen));
        assert_eq!(CommandBarOpenEvent::default().picker, None);
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
}
