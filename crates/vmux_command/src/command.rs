use bevy::prelude::*;
use vmux_macro::{CommandBar, DefaultShortcuts, McpTool, OsMenu, OsSubMenu, OsSubMenuGroup};

use crate::open::OpenCommand;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WriteAppCommands;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReadAppCommands;

pub fn build_native_root_menu(menu: &mut muda::Menu) -> Result<(), muda::Error> {
    AppCommand::build_native_root_menu(menu)
}

#[derive(Message, OsMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, PartialEq, Eq)]
pub enum AppCommand {
    #[menu(label = "Scene")]
    Scene(SceneCommand),

    #[menu(label = "Layout")]
    #[mcp(skip)]
    Layout(LayoutCommand),

    #[menu(label = "Terminal")]
    Terminal(TerminalCommand),

    #[menu(label = "Browser")]
    Browser(BrowserCommand),

    #[menu(label = "Service")]
    Service(ServiceCommand),

    #[menu(label = "Bookmark")]
    #[mcp(skip)]
    Bookmark(BookmarkCommand),

    #[menu(label = "Agent")]
    #[mcp(skip)]
    Agent(AgentCommand),
}

#[allow(dead_code)]
#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentCommand {
    #[default]
    #[menu(
        id = "agent_add_selection",
        label = "Add Selection to Agent",
        accel = "super+shift+l"
    )]
    #[shortcut(direct = "Super+Shift+L")]
    AddSelection,
}

#[derive(OsSubMenuGroup, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutCommand {
    #[menu(label = "Window")]
    Window(WindowCommand),

    #[menu(label = "Layout")]
    ToggleLayout(ToggleLayoutCommand),

    #[menu(label = "Tab")]
    Tab(TabCommand),

    #[menu(label = "Pane")]
    Pane(PaneCommand),

    #[menu(label = "Stack")]
    Stack(StackCommand),

    #[menu(label = "Space")]
    Space(SpaceCommand),
}

#[allow(dead_code)]
#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StackCommand {
    #[default]
    #[menu(id = "stack_close", label = "Close Stack", accel = "super+w")]
    #[shortcut(chord = "Ctrl+g, x")]
    Close,
    #[menu(id = "stack_next", label = "Next Stack", accel = "super+shift+n")]
    #[shortcut(direct = "Super+Shift+J")]
    Next,
    #[menu(
        id = "stack_previous",
        label = "Previous Stack",
        accel = "super+shift+p"
    )]
    #[shortcut(direct = "Super+Shift+K")]
    Previous,
    #[menu(
        id = "stack_reopen",
        label = "Reopen Closed Page",
        accel = "super+shift+t"
    )]
    #[shortcut(direct = "Ctrl+Shift+T")]
    Reopen,
    #[menu(id = "stack_duplicate", label = "Duplicate Stack\t<leader> d", hidden)]
    #[shortcut(chord = "Ctrl+g, d")]
    Duplicate,

    #[menu(
        id = "stack_move_to_pane",
        label = "Move Stack to Pane\t<leader> !",
        hidden
    )]
    #[shortcut(chord = "Ctrl+g, !")]
    MoveToPane,
    #[menu(id = "stack_swap_prev", label = "Move Stack Left\t<leader> <")]
    #[shortcut(chord = "Ctrl+g, <")]
    SwapPrev,
    #[menu(id = "stack_swap_next", label = "Move Stack Right\t<leader> >")]
    #[shortcut(chord = "Ctrl+g, >")]
    SwapNext,
}

#[allow(dead_code)]
#[derive(
    OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, Copy, PartialEq, Eq, Default,
)]
pub enum TerminalCommand {
    #[default]
    #[menu(id = "terminal_close", label = "Close Terminal")]
    Close,
    #[menu(id = "terminal_next", label = "Next Terminal")]
    Next,
    #[menu(id = "terminal_prev", label = "Previous Terminal")]
    Previous,
    #[menu(id = "terminal_clear", label = "Clear Terminal")]
    Clear,
    #[menu(id = "terminal_copy_mode", label = "Visual Mode\t<leader> [", hidden)]
    #[shortcut(chord = "Ctrl+g, [")]
    CopyMode,
}

#[derive(OsSubMenuGroup, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, PartialEq, Eq)]
pub enum BrowserCommand {
    #[menu(label = "Navigation")]
    Navigation(BrowserNavigationCommand),

    #[menu(label = "Open")]
    Open(OpenCommand),

    #[menu(label = "View")]
    View(BrowserViewCommand),

    #[menu(label = "Bar")]
    Bar(BrowserBarCommand),
}

#[allow(dead_code)]
#[derive(
    OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, Copy, PartialEq, Eq, Default,
)]
pub enum BrowserNavigationCommand {
    #[default]
    #[menu(id = "browser_prev_page", label = "Back", accel = "super+[")]
    PrevPage,
    #[menu(id = "browser_next_page", label = "Forward", accel = "super+]")]
    NextPage,
    #[menu(id = "browser_reload", label = "Reload", accel = "super+r")]
    #[shortcut(direct = "Super+r")]
    Reload,
    #[menu(
        id = "browser_hard_reload",
        label = "Hard Reload",
        accel = "super+shift+r"
    )]
    #[shortcut(direct = "Super+Shift+R")]
    HardReload,
    #[menu(id = "browser_stop", label = "Stop Loading", accel = "super+.", hidden)]
    Stop,
}

#[allow(dead_code)]
#[derive(
    OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, Copy, PartialEq, Eq, Default,
)]
pub enum BrowserViewCommand {
    #[default]
    #[menu(id = "browser_zoom_in", label = "Zoom In", accel = "super+=")]
    ZoomIn,
    #[menu(id = "browser_zoom_out", label = "Zoom Out", accel = "super+-")]
    ZoomOut,
    #[menu(id = "browser_zoom_reset", label = "Actual Size", accel = "super+0")]
    ZoomReset,
    #[menu(
        id = "browser_dev_tools",
        label = "Developer Tools",
        accel = "super+alt+i"
    )]
    DevTools,
    #[menu(
        id = "browser_view_source",
        label = "View Source",
        accel = "super+alt+u",
        hidden
    )]
    ViewSource,
    #[menu(id = "browser_print", label = "Print", accel = "super+p", hidden)]
    Print,
}

#[allow(dead_code)]
#[derive(
    OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, Copy, PartialEq, Eq, Default,
)]
pub enum BrowserBarCommand {
    #[default]
    #[menu(
        id = "browser_open_command_bar",
        label = "Command Bar",
        accel = "super+k"
    )]
    #[shortcut(direct = "Super+k")]
    OpenCommandBar,
    #[menu(
        id = "browser_open_page_in_command_bar",
        label = "Edit Page",
        accel = "super+l"
    )]
    #[shortcut(direct = "Super+l")]
    OpenPageInCommandBar,
    #[menu(
        id = "browser_open_path_bar",
        label = "Path Navigator",
        accel = "super+/"
    )]
    #[shortcut(direct = "Super+/")]
    OpenPathBar,
    #[menu(id = "browser_open_commands", label = "Commands")]
    #[shortcut(direct = ">")]
    OpenCommands,
    #[menu(id = "browser_open_history", label = "History", accel = "super+y")]
    OpenHistory,
    #[menu(id = "browser_find", label = "Find", accel = "super+f", hidden)]
    Find,
}

#[allow(dead_code)]
#[derive(
    OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, Copy, PartialEq, Eq, Default,
)]
pub enum ServiceCommand {
    #[default]
    #[menu(id = "service_open", label = "Open Service Monitor")]
    Open,
}

#[allow(dead_code)]
#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BookmarkCommand {
    #[default]
    #[menu(
        id = "bookmark_toggle_active",
        label = "Bookmark Page",
        accel = "super+d"
    )]
    #[shortcut(direct = "Super+d")]
    ToggleActive,
    #[menu(id = "bookmark_pin_active", label = "Pin Page")]
    PinActive,
    #[menu(id = "bookmark_new_folder", label = "New Folder", hidden)]
    NewFolder,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpaceCommand {
    #[default]
    #[menu(id = "space_open", label = "Spaces\t<leader> s")]
    #[shortcut(chord = "Ctrl+g, s")]
    Open,
}

#[allow(dead_code)]
#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneCommand {
    #[default]
    #[menu(id = "toggle_pane", label = "Next Pane\t<leader> o", hidden)]
    #[shortcut(chord = "Ctrl+g, o")]
    Toggle,
    #[menu(id = "close_pane", label = "Close Pane")]
    Close,
    #[menu(id = "zoom_pane", label = "Zoom Pane\t<leader> z", hidden)]
    #[shortcut(chord = "Ctrl+g, z")]
    Zoom,
    #[menu(id = "select_pane_left", label = "Select Left Pane\t<leader> h")]
    #[shortcut(chord = "Ctrl+g, h")]
    SelectLeft,
    #[menu(id = "select_pane_right", label = "Select Right Pane\t<leader> l")]
    #[shortcut(chord = "Ctrl+g, l")]
    SelectRight,
    #[menu(id = "select_pane_up", label = "Select Up Pane\t<leader> k")]
    #[shortcut(chord = "Ctrl+g, k")]
    SelectUp,
    #[menu(id = "select_pane_down", label = "Select Down Pane\t<leader> j")]
    #[shortcut(chord = "Ctrl+g, j")]
    SelectDown,
    #[menu(id = "swap_pane_prev", label = "Swap Pane Previous\t<leader> {")]
    #[shortcut(chord = "Ctrl+g, {")]
    SwapPrev,
    #[menu(id = "swap_pane_next", label = "Swap Pane Next\t<leader> }")]
    #[shortcut(chord = "Ctrl+g, }")]
    SwapNext,
    #[menu(
        id = "rotate_forward",
        label = "Rotate Forward\t<leader> ctrl+o",
        hidden
    )]
    #[shortcut(chord = "Ctrl+g, Ctrl+o")]
    RotateForward,
    #[menu(
        id = "rotate_backward",
        label = "Rotate Backward\t<leader> alt+o",
        hidden
    )]
    #[shortcut(chord = "Ctrl+g, Alt+o")]
    RotateBackward,
    #[menu(id = "equalize_pane_size", label = "Equalize Pane Size\t<leader> =")]
    #[shortcut(chord = "Ctrl+g, =")]
    EqualizeSize,
    #[menu(id = "resize_pane_left", label = "Resize Pane Left\t<leader> alt+left")]
    #[shortcut(chord = "Ctrl+g, Alt+ArrowLeft")]
    ResizeLeft,
    #[menu(
        id = "resize_pane_right",
        label = "Resize Pane Right\t<leader> alt+right"
    )]
    #[shortcut(chord = "Ctrl+g, Alt+ArrowRight")]
    ResizeRight,
    #[menu(id = "resize_pane_up", label = "Resize Pane Up\t<leader> alt+up")]
    #[shortcut(chord = "Ctrl+g, Alt+ArrowUp")]
    ResizeUp,
    #[menu(id = "resize_pane_down", label = "Resize Pane Down\t<leader> alt+down")]
    #[shortcut(chord = "Ctrl+g, Alt+ArrowDown")]
    ResizeDown,
}

#[allow(dead_code)]
#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabCommand {
    #[default]
    #[menu(id = "close_tab", label = "Close Tab")]
    Close,
    #[menu(id = "new_task", label = "New Task…")]
    New,
    #[menu(id = "next_tab", label = "Next Tab", accel = "super+shift+]")]
    #[shortcut(direct = "Super+Shift+L")]
    #[shortcut(direct = "Super+Alt+ArrowRight")]
    #[shortcut(direct = "Super+Shift+BracketRight")]
    Next,
    #[menu(id = "prev_tab", label = "Previous Tab", accel = "super+shift+[")]
    #[shortcut(direct = "Super+Shift+H")]
    #[shortcut(direct = "Super+Alt+ArrowLeft")]
    #[shortcut(direct = "Super+Shift+BracketLeft")]
    Previous,
    #[menu(id = "rename_tab", label = "Rename Tab")]
    Rename,
    #[menu(id = "tab_select_1", label = "Select Tab 1", accel = "super+1")]
    SelectIndex1,
    #[menu(id = "tab_select_2", label = "Select Tab 2", accel = "super+2")]
    SelectIndex2,
    #[menu(id = "tab_select_3", label = "Select Tab 3", accel = "super+3")]
    SelectIndex3,
    #[menu(id = "tab_select_4", label = "Select Tab 4", accel = "super+4")]
    SelectIndex4,
    #[menu(id = "tab_select_5", label = "Select Tab 5", accel = "super+5")]
    SelectIndex5,
    #[menu(id = "tab_select_6", label = "Select Tab 6", accel = "super+6")]
    SelectIndex6,
    #[menu(id = "tab_select_7", label = "Select Tab 7", accel = "super+7")]
    SelectIndex7,
    #[menu(id = "tab_select_8", label = "Select Tab 8", accel = "super+8")]
    SelectIndex8,
    #[menu(id = "tab_select_last", label = "Select Last Tab", accel = "super+9")]
    SelectLast,
    #[menu(id = "swap_tab_prev", label = "Move Tab Left", hidden)]
    SwapPrev,
    #[menu(id = "swap_tab_next", label = "Move Tab Right", hidden)]
    SwapNext,
}

#[allow(dead_code)]
#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToggleLayoutCommand {
    #[default]
    #[menu(id = "toggle_layout", label = "Toggle Layout", accel = "super+shift+s")]
    #[shortcut(direct = "Super+Shift+S")]
    Toggle,
}

#[derive(
    OsSubMenuGroup, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, Copy, PartialEq, Eq,
)]
pub enum SceneCommand {
    #[menu(label = "Interactive Mode")]
    InteractiveMode(SceneInteractiveModeCommand),
}

#[derive(
    OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, Copy, PartialEq, Eq, Default,
)]
pub enum SceneInteractiveModeCommand {
    #[default]
    #[menu(id = "interactive_mode_user", label = "User")]
    User,
    #[menu(id = "interactive_mode_player", label = "Player")]
    Player,
    #[menu(id = "toggle_player_mode", label = "Toggle Player Mode", hidden)]
    #[shortcut(chord = "Ctrl+g, Enter")]
    #[mcp(skip)]
    Toggle,
}

#[allow(dead_code)]
#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowCommand {
    #[default]
    #[menu(id = "new_window", label = "New Window", hidden)]
    NewWindow,
    #[menu(
        id = "close_window",
        label = "Close Window",
        accel = "super+shift+w",
        hidden
    )]
    CloseWindow,
    #[menu(id = "minimize_window", label = "Minimize", accel = "super+m")]
    Minimize,
    #[menu(
        id = "toggle_fullscreen",
        label = "Toggle Fullscreen",
        accel = "ctrl+super+f",
        hidden
    )]
    ToggleFullscreen,
    #[menu(id = "open_settings", label = "Settings", accel = "super+,", hidden)]
    #[shortcut(direct = "Super+,")]
    Settings,
}

#[cfg(test)]
mod tests {
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
                let resolved_by_call =
                    AppCommand::from_mcp_call(bare, serde_json::json!({})).is_some();
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
    fn add_selection_uses_cursor_shortcut() {
        let add_selection = Shortcut::Direct(KeyCombo {
            key: KeyCode::KeyL,
            modifiers: Modifiers {
                shift: true,
                super_key: true,
                ..Default::default()
            },
        });

        assert!(
            AppCommand::default_shortcuts()
                .iter()
                .any(|(shortcut, id)| {
                    shortcut == &add_selection && id == "agent_add_selection"
                })
        );
        assert_eq!(
            AppCommand::from_menu_id("agent_add_selection"),
            Some(AppCommand::Agent(AgentCommand::AddSelection))
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
}
