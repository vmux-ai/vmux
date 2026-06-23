use vmux_command::open::{OpenCommand, OpenTarget, PaneDirection, PaneOpenMode, PaneTarget};

#[test]
fn default_pane_target_is_new_split() {
    assert_eq!(PaneTarget::default(), PaneTarget::NewSplit);
}

#[test]
fn default_pane_open_mode_is_new_stack() {
    assert_eq!(PaneOpenMode::default(), PaneOpenMode::NewStack);
}

#[test]
fn open_command_in_place_has_none_url_default() {
    let cmd = OpenCommand::InPlace { url: None };
    assert!(matches!(cmd, OpenCommand::InPlace { url: None }));
}

#[test]
fn open_command_in_pane_carries_all_four_fields() {
    let cmd = OpenCommand::InPane {
        direction: PaneDirection::Right,
        target: PaneTarget::Existing,
        mode: PaneOpenMode::InPlace,
        url: Some("https://example.com".to_string()),
    };
    let OpenCommand::InPane {
        direction,
        target,
        mode,
        url,
    } = cmd
    else {
        panic!("expected InPane variant");
    };
    assert_eq!(direction, PaneDirection::Right);
    assert_eq!(target, PaneTarget::Existing);
    assert_eq!(mode, PaneOpenMode::InPlace);
    assert_eq!(url.as_deref(), Some("https://example.com"));
}

#[test]
fn from_menu_id_resolves_all_expanded_pane_directions() {
    let top = OpenCommand::from_menu_id("open_in_pane_top");
    assert_eq!(
        top,
        Some(OpenCommand::InPane {
            direction: PaneDirection::Top,
            target: PaneTarget::NewSplit,
            mode: PaneOpenMode::NewStack,
            url: None,
        })
    );
    assert!(OpenCommand::from_menu_id("open_in_pane_right").is_some());
    assert!(OpenCommand::from_menu_id("open_in_pane_bottom").is_some());
    assert!(OpenCommand::from_menu_id("open_in_pane_left").is_some());
}

#[test]
fn from_menu_id_resolves_non_expanded_variants() {
    assert!(OpenCommand::from_menu_id("open_in_place").is_some());
    assert!(OpenCommand::from_menu_id("open_in_new_stack").is_some());
    assert!(OpenCommand::from_menu_id("open_in_new_tab").is_some());
    assert!(OpenCommand::from_menu_id("open_in_new_space").is_some());
}

#[test]
fn default_shortcuts_contains_expected_ids() {
    let shortcuts = OpenCommand::default_shortcuts();
    let ids: Vec<_> = shortcuts.iter().map(|(_, id)| id.as_str()).collect();
    assert!(ids.contains(&"open_in_pane_top"));
    assert!(ids.contains(&"open_in_pane_right"));
    assert!(ids.contains(&"open_in_pane_bottom"));
    assert!(ids.contains(&"open_in_pane_left"));
}

#[test]
fn extra_chord_bindings_has_two_tmux_chords() {
    let extras = OpenCommand::extra_chord_bindings();
    assert_eq!(extras.len(), 2);
    assert_eq!(
        extras[0].1,
        OpenCommand::InPane {
            direction: PaneDirection::Right,
            target: PaneTarget::NewSplit,
            mode: PaneOpenMode::NewStack,
            url: None,
        }
    );
    assert_eq!(
        extras[1].1,
        OpenCommand::InPane {
            direction: PaneDirection::Bottom,
            target: PaneTarget::NewSplit,
            mode: PaneOpenMode::NewStack,
            url: None,
        }
    );
}

#[test]
fn command_bar_entries_has_eight_entries() {
    let entries = OpenCommand::command_bar_entries();
    assert_eq!(entries.len(), 8);
    let ids: Vec<_> = entries.iter().map(|(id, _, _)| *id).collect();
    assert!(ids.contains(&"open_in_place"));
    assert!(ids.contains(&"open_in_new_stack"));
    assert!(ids.contains(&"open_in_pane_top"));
    assert!(ids.contains(&"open_in_pane_right"));
    assert!(ids.contains(&"open_in_pane_bottom"));
    assert!(ids.contains(&"open_in_pane_left"));
    assert!(ids.contains(&"open_in_new_tab"));
    assert!(ids.contains(&"open_in_new_space"));
}

#[test]
fn mcp_tool_entries_has_all_variants() {
    let entries = OpenCommand::mcp_tool_entries();
    assert!(!entries.is_empty());
    let names: Vec<_> = entries.iter().map(|(name, _, _)| *name).collect();
    assert!(names.contains(&"vmux_in_place"));
    assert!(names.contains(&"vmux_in_new_stack"));
    assert!(names.contains(&"vmux_in_new_tab"));
    assert!(names.contains(&"vmux_in_new_space"));
    // in_pane is #[mcp(skip)] (superseded by the self-relative vmux_open_page tool).
    assert!(!names.contains(&"vmux_in_pane"));
}

#[test]
fn open_target_default_is_in_place() {
    assert_eq!(OpenTarget::default(), OpenTarget::InPlace);
}

#[test]
fn open_target_in_pane_variant() {
    let t = OpenTarget::InPane {
        direction: PaneDirection::Right,
        target: PaneTarget::Existing,
        mode: PaneOpenMode::InPlace,
    };
    assert!(matches!(
        t,
        OpenTarget::InPane {
            direction: PaneDirection::Right,
            ..
        }
    ));
}
