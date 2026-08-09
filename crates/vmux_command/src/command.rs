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
#[path = "command.test.rs"]
mod tests;
