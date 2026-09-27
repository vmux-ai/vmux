pub use crate::history::{HistoryEntry, HistorySuggestionsRequest, HistorySuggestionsResponse};

mod input;
mod open;
mod palette;
mod path;
mod picker;
mod request;
mod state;

pub use input::*;
pub use open::*;
pub use palette::*;
pub use path::*;
pub use picker::*;
pub use request::*;
pub use state::*;

#[cfg(test)]
mod tests {
    use super::*;

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
