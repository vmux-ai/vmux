use super::*;
use crate::shortcut::{KeyCombo, Modifiers, Shortcut};
use bevy::input::keyboard::KeyCode;

#[test]
fn menu_accelerators_are_registered_as_global_shortcuts() {
    let shortcuts = AppCommand::default_shortcuts();
    let has_super = |k: KeyCode| {
        shortcuts.iter().any(|(s, _)| {
            matches!(s, Shortcut::Direct(c) if c.key == k && c.modifiers.super_key
                    && !c.modifiers.shift && !c.modifiers.ctrl && !c.modifiers.alt)
        })
    };
    // Accelerator-only menu commands must also reach the universal shortcut layer so they fire
    // when a terminal/layout holds focus (winit swallows menu key-equivalents there).
    assert!(
        has_super(KeyCode::KeyT),
        "cmd+T (new tab) must be a global shortcut"
    );
    assert!(
        has_super(KeyCode::KeyN),
        "cmd+N (new stack) must be a global shortcut"
    );
    assert!(
        has_super(KeyCode::KeyW),
        "cmd+W (close stack) must be a global shortcut"
    );
    assert!(
        has_super(KeyCode::KeyD),
        "cmd+D (bookmark page) must be a global shortcut"
    );
    assert_eq!(
        AppCommand::from_menu_id("open_in_new_tab"),
        Some(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InNewTab { url: None }
        )))
    );
}

#[test]
fn hidden_commands_can_have_default_shortcuts() {
    assert_eq!(
        AppCommand::from_menu_id("terminal_copy_mode"),
        Some(AppCommand::Terminal(TerminalCommand::CopyMode))
    );

    let copy_mode = AppCommand::default_shortcuts()
        .into_iter()
        .find(|(_, id)| id == "terminal_copy_mode")
        .map(|(shortcut, _)| shortcut);

    assert_eq!(
        copy_mode,
        Some(Shortcut::Chord(
            KeyCombo {
                key: KeyCode::KeyG,
                modifiers: Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            },
            KeyCombo {
                key: KeyCode::BracketLeft,
                modifiers: Modifiers::default(),
            },
        ))
    );
}

#[test]
fn leader_x_closes_stack_like_command_w() {
    let leader_x = Shortcut::Chord(
        KeyCombo {
            key: KeyCode::KeyG,
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        },
        KeyCombo {
            key: KeyCode::KeyX,
            modifiers: Modifiers::default(),
        },
    );
    let ids: Vec<String> = AppCommand::default_shortcuts()
        .into_iter()
        .filter(|(shortcut, _)| shortcut == &leader_x)
        .map(|(_, id)| id)
        .collect();

    assert_eq!(ids, vec!["stack_close".to_string()]);
    assert_eq!(
        AppCommand::from_menu_id("stack_close"),
        Some(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close
        )))
    );
}

#[test]
fn mcp_lookup_resolves_every_command_id() {
    let entries = AppCommand::mcp_tool_entries();
    assert!(!entries.is_empty(), "mcp_tool_entries should not be empty");

    for (id, _description, schema) in &entries {
        assert!(
            !id.starts_with("vmux_"),
            "advertised MCP tool name must not be vmux_-prefixed (server is already named vmux): {id}"
        );
        let bare = *id;
        let has_required_params = schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        if has_required_params {
            assert!(
                AppCommand::from_mcp_call(bare, serde_json::json!({})).is_some(),
                "from_mcp_call failed to resolve {id}"
            );
        } else {
            let resolved_by_id = AppCommand::from_mcp_id(bare).is_some();
            let resolved_by_call = AppCommand::from_mcp_call(bare, serde_json::json!({})).is_some();
            assert!(
                resolved_by_id || resolved_by_call,
                "neither from_mcp_id nor from_mcp_call resolved {id}"
            );
        }
    }

    assert_eq!(
        AppCommand::from_mcp_id("terminal_clear"),
        Some(AppCommand::Terminal(TerminalCommand::Clear))
    );
    assert_eq!(
        AppCommand::from_mcp_id("browser_reload"),
        Some(AppCommand::Browser(BrowserCommand::Navigation(
            BrowserNavigationCommand::Reload
        )))
    );
}

#[test]
fn browser_open_in_new_stack_resolves_through_nested_chain() {
    assert!(matches!(
        AppCommand::from_menu_id("open_in_new_stack"),
        Some(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InNewStack { url: None }
        )))
    ));
}

#[test]
fn command_bar_names_are_hierarchical() {
    let entries = AppCommand::command_bar_entries();
    let back = entries
        .iter()
        .find(|(id, _, _)| *id == "browser_prev_page")
        .map(|(_, name, _)| name.as_str());
    assert_eq!(back, Some("Browser > Navigation > Back"));
}

#[test]
fn browser_navigation_back_still_resolves() {
    assert!(matches!(
        AppCommand::from_menu_id("browser_prev_page"),
        Some(AppCommand::Browser(BrowserCommand::Navigation(
            BrowserNavigationCommand::PrevPage
        )))
    ));
}

#[test]
fn browser_reload_has_direct_shortcut_for_native_webviews() {
    let reload = Shortcut::Direct(KeyCombo {
        key: KeyCode::KeyR,
        modifiers: Modifiers {
            super_key: true,
            ..Default::default()
        },
    });
    let hard_reload = Shortcut::Direct(KeyCombo {
        key: KeyCode::KeyR,
        modifiers: Modifiers {
            shift: true,
            super_key: true,
            ..Default::default()
        },
    });
    let shortcuts = AppCommand::default_shortcuts();

    assert!(
        shortcuts
            .iter()
            .any(|(shortcut, id)| shortcut == &reload && id == "browser_reload")
    );
    assert!(
        shortcuts
            .iter()
            .any(|(shortcut, id)| shortcut == &hard_reload && id == "browser_hard_reload")
    );
}

#[test]
fn tab_nav_brackets_are_global_shortcuts() {
    let next = Shortcut::Direct(KeyCombo {
        key: KeyCode::BracketRight,
        modifiers: Modifiers {
            shift: true,
            super_key: true,
            ..Default::default()
        },
    });
    let prev = Shortcut::Direct(KeyCombo {
        key: KeyCode::BracketLeft,
        modifiers: Modifiers {
            shift: true,
            super_key: true,
            ..Default::default()
        },
    });
    let shortcuts = AppCommand::default_shortcuts();

    assert!(
        shortcuts
            .iter()
            .any(|(shortcut, id)| shortcut == &next && id == "next_tab"),
        "cmd+shift+] must be a global shortcut so it fires under terminal/layout focus"
    );
    assert!(
        shortcuts
            .iter()
            .any(|(shortcut, id)| shortcut == &prev && id == "prev_tab"),
        "cmd+shift+[ must be a global shortcut so it fires under terminal/layout focus"
    );
}

#[test]
fn browser_view_zoom_still_resolves() {
    assert!(matches!(
        AppCommand::from_menu_id("browser_zoom_in"),
        Some(AppCommand::Browser(BrowserCommand::View(
            BrowserViewCommand::ZoomIn
        )))
    ));
}

#[test]
fn browser_bar_command_bar_still_resolves() {
    assert!(matches!(
        AppCommand::from_menu_id("browser_open_command_bar"),
        Some(AppCommand::Browser(BrowserCommand::Bar(
            BrowserBarCommand::OpenCommandBar
        )))
    ));
}

#[test]
fn layout_command_ids_no_longer_exposed_via_mcp() {
    for id in [
        "split_v",
        "split_h",
        "close_pane",
        "select_pane_left",
        "new_tab",
        "tab_select_1",
        "stack_new",
    ] {
        assert!(
            AppCommand::from_mcp_id(id).is_none(),
            "{id} should not be exposed via MCP after the derive strip"
        );
    }
}

#[test]
fn non_layout_command_ids_still_exposed_via_mcp() {
    for id in ["terminal_clear", "browser_reload"] {
        assert!(
            AppCommand::from_mcp_id(id).is_some(),
            "{id} should still be exposed via MCP"
        );
    }
}

#[test]
fn layout_menu_id_resolves_through_nested_chain() {
    assert_eq!(
        AppCommand::from_menu_id("toggle_pane"),
        Some(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Toggle)))
    );
    assert_eq!(
        AppCommand::from_menu_id("toggle_layout"),
        Some(AppCommand::Layout(LayoutCommand::ToggleLayout(
            ToggleLayoutCommand::Toggle
        )))
    );
    assert_eq!(
        AppCommand::from_menu_id("space_open"),
        Some(AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open)))
    );
}

#[test]
fn scene_interactive_mode_menu_ids_resolve() {
    assert_eq!(
        AppCommand::from_menu_id("interactive_mode_user").map(|cmd| format!("{cmd:?}")),
        Some("Scene(InteractiveMode(User))".to_string())
    );
    assert_eq!(
        AppCommand::from_menu_id("interactive_mode_player").map(|cmd| format!("{cmd:?}")),
        Some("Scene(InteractiveMode(Player))".to_string())
    );
}

#[test]
fn scene_menu_nests_interactive_mode_selector() {
    let source = include_str!("command.rs")
        .split("#[cfg(test)]")
        .next()
        .expect("production source");

    assert!(source.contains("#[menu(label = \"Interactive Mode\")]"));
    assert!(source.contains("interactive_mode_user"));
    assert!(source.contains("interactive_mode_player"));
}
