use bevy::prelude::*;
use vmux_macro::{CommandBar, DefaultShortcuts, OsMenu, OsSubMenu};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WriteAppCommands;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ReadAppCommands;

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppCommand>()
            .configure_sets(Update, ReadAppCommands.after(WriteAppCommands));
    }
}

#[derive(Message, OsMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    #[menu(label = "Scene")]
    Scene(SceneCommand),

    #[menu(label = "Window")]
    Window(WindowCommand),

    #[menu(label = "Header")]
    Header(HeaderCommand),

    #[menu(label = "Side Sheet")]
    SideSheet(SideSheetCommand),

    #[menu(label = "Space")]
    Space(SpaceCommand),

    #[menu(label = "Pane")]
    Pane(PaneCommand),

    #[menu(label = "Tab")]
    Tab(TabCommand),

    #[menu(label = "Terminal")]
    Terminal(TerminalCommand),

    #[menu(label = "Browser")]
    Browser(BrowserCommand),
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabCommand {
    #[default]
    #[menu(id = "tab_new", label = "New Tab", accel = "super+t")]
    New,
    #[menu(id = "tab_close", label = "Close Tab", accel = "super+w")]
    Close,
    #[menu(id = "tab_next", label = "Next Tab", accel = "super+shift+]")]
    Next,
    #[menu(id = "tab_previous", label = "Previous Tab", accel = "super+shift+[")]
    Previous,
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
    #[menu(id = "tab_reopen", label = "Reopen Closed Tab", accel = "super+shift+t")]
    Reopen,
    #[menu(id = "tab_duplicate", label = "Duplicate Tab\t<leader> d")]
    #[shortcut(chord = "Ctrl+g, d")]
    Duplicate,
    #[menu(id = "tab_pin", label = "Pin Tab")]
    Pin,
    #[menu(id = "tab_mute", label = "Mute Tab\t<leader> m")]
    #[shortcut(chord = "Ctrl+g, m")]
    Mute,
    #[menu(id = "tab_move_to_pane", label = "Move Tab to Pane\t<leader> !")]
    #[shortcut(chord = "Ctrl+g, !")]
    MoveToPane,
    #[menu(id = "tab_swap_prev", label = "Move Tab Left\t<leader> <")]
    #[shortcut(chord = "Ctrl+g, <")]
    SwapPrev,
    #[menu(id = "tab_swap_next", label = "Move Tab Right\t<leader> >")]
    #[shortcut(chord = "Ctrl+g, >")]
    SwapNext,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalCommand {
    #[default]
    #[menu(id = "terminal_new", label = "New Terminal", accel = "ctrl+`")]
    #[shortcut(direct = "Ctrl+`")]
    New,
    #[menu(id = "terminal_close", label = "Close Terminal")]
    Close,
    #[menu(id = "terminal_next", label = "Next Terminal")]
    Next,
    #[menu(id = "terminal_prev", label = "Previous Terminal")]
    Previous,
    #[menu(id = "terminal_clear", label = "Clear Terminal")]
    Clear,
    #[menu(id = "terminal_reset", label = "Reset Terminal")]
    Reset,
    #[menu(id = "terminal_split_v", label = "Split Terminal Vertically")]
    SplitV,
    #[menu(id = "terminal_split_h", label = "Split Terminal Horizontally")]
    SplitH,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserCommand {
    #[default]
    #[menu(id = "browser_prev_page", label = "Back", accel = "super+[")]
    PrevPage,
    #[menu(id = "browser_next_page", label = "Forward", accel = "super+]")]
    NextPage,
    #[menu(id = "browser_reload", label = "Reload", accel = "super+r")]
    Reload,
    #[menu(id = "browser_hard_reload", label = "Hard Reload", accel = "super+shift+r")]
    HardReload,
    #[menu(id = "browser_stop", label = "Stop Loading", accel = "super+.")]
    Stop,
    #[menu(id = "browser_focus_address_bar", label = "Open Location", accel = "super+l")]
    FocusAddressBar,
    #[menu(id = "browser_open_command_bar", label = "Command Bar", accel = "super+k")]
    OpenCommandBar,
    #[menu(id = "browser_open_path_bar", label = "Path Navigator", accel = "super+/")]
    OpenPathBar,
    #[menu(id = "browser_open_commands", label = "Commands")]
    #[shortcut(direct = ">")]
    OpenCommands,
    #[menu(id = "browser_find", label = "Find", accel = "super+f")]
    Find,
    #[menu(id = "browser_zoom_in", label = "Zoom In", accel = "super+=")]
    ZoomIn,
    #[menu(id = "browser_zoom_out", label = "Zoom Out", accel = "super+-")]
    ZoomOut,
    #[menu(id = "browser_zoom_reset", label = "Actual Size", accel = "super+0")]
    ZoomReset,
    #[menu(id = "browser_dev_tools", label = "Developer Tools", accel = "super+alt+i")]
    DevTools,
    #[menu(id = "browser_view_source", label = "View Source", accel = "super+alt+u")]
    ViewSource,
    #[menu(id = "browser_print", label = "Print", accel = "super+p")]
    Print,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneCommand {
    #[default]
    #[menu(id = "split_v", label = "Split Vertically\t<leader> %")]
    #[shortcut(chord = "Ctrl+g, %")]
    SplitV,
    #[menu(id = "split_h", label = "Split Horizontally\t<leader> \"")]
    #[shortcut(chord = "Ctrl+g, \"")]
    SplitH,
    #[menu(id = "toggle_pane", label = "Next Pane\t<leader> o")]
    #[shortcut(chord = "Ctrl+g, o")]
    Toggle,
    #[menu(id = "close_pane", label = "Close Pane\t<leader> x")]
    #[shortcut(chord = "Ctrl+g, x")]
    Close,
    #[menu(id = "zoom_pane", label = "Zoom Pane\t<leader> z")]
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
    #[menu(id = "rotate_forward", label = "Rotate Forward\t<leader> ctrl+o")]
    #[shortcut(chord = "Ctrl+g, Ctrl+o")]
    RotateForward,
    #[menu(id = "rotate_backward", label = "Rotate Backward\t<leader> alt+o")]
    #[shortcut(chord = "Ctrl+g, Alt+o")]
    RotateBackward,
    #[menu(id = "equalize_pane_size", label = "Equalize Pane Size\t<leader> =")]
    #[shortcut(chord = "Ctrl+g, =")]
    EqualizeSize,
    #[menu(id = "resize_pane_left", label = "Resize Pane Left\t<leader> alt+left")]
    #[shortcut(chord = "Ctrl+g, Alt+ArrowLeft")]
    ResizeLeft,
    #[menu(id = "resize_pane_right", label = "Resize Pane Right\t<leader> alt+right")]
    #[shortcut(chord = "Ctrl+g, Alt+ArrowRight")]
    ResizeRight,
    #[menu(id = "resize_pane_up", label = "Resize Pane Up\t<leader> alt+up")]
    #[shortcut(chord = "Ctrl+g, Alt+ArrowUp")]
    ResizeUp,
    #[menu(id = "resize_pane_down", label = "Resize Pane Down\t<leader> alt+down")]
    #[shortcut(chord = "Ctrl+g, Alt+ArrowDown")]
    ResizeDown,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpaceCommand {
    #[default]
    #[menu(id = "new_space", label = "New Space\t<leader> c")]
    #[shortcut(chord = "Ctrl+g, c")]
    New,
    #[menu(id = "close_space", label = "Close Space\t<leader> &")]
    #[shortcut(chord = "Ctrl+g, &")]
    Close,
    #[menu(id = "next_space", label = "Next Space\t<leader> n")]
    #[shortcut(chord = "Ctrl+g, n")]
    Next,
    #[menu(id = "prev_space", label = "Previous Space\t<leader> p")]
    #[shortcut(chord = "Ctrl+g, p")]
    Previous,
    #[menu(id = "rename_space", label = "Rename Space\t<leader> ,")]
    #[shortcut(chord = "Ctrl+g, Comma")]
    Rename,
    #[menu(id = "swap_space_prev", label = "Move Space Left")]
    SwapPrev,
    #[menu(id = "swap_space_next", label = "Move Space Right")]
    SwapNext,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SideSheetCommand {
    #[default]
    #[menu(id = "toggle_side_sheet", label = "Toggle Side Sheet", accel = "super+s")]
    #[shortcut(direct = "Super+s")]
    Toggle,
    #[menu(id = "toggle_side_sheet_right", label = "Toggle Right Sheet\t<leader> r")]
    #[shortcut(chord = "Ctrl+g, r")]
    ToggleRight,
    #[menu(id = "toggle_side_sheet_bottom", label = "Toggle Bottom Sheet\t<leader> b")]
    #[shortcut(chord = "Ctrl+g, b")]
    ToggleBottom,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SceneCommand {
    #[default]
    #[menu(id = "toggle_player_mode", label = "Toggle Player Mode")]
    #[shortcut(chord = "Ctrl+g, Enter")]
    TogglePlayerMode,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeaderCommand {
    #[default]
    #[menu(id = "toggle_header", label = "Toggle Header", accel = "super+shift+h")]
    Toggle,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowCommand {
    #[default]
    #[menu(id = "new_window", label = "New Window", accel = "super+n")]
    NewWindow,
    #[menu(id = "close_window", label = "Close Window", accel = "super+shift+w")]
    CloseWindow,
    #[menu(id = "minimize_window", label = "Minimize", accel = "super+m")]
    Minimize,
    #[menu(id = "toggle_fullscreen", label = "Toggle Fullscreen", accel = "ctrl+super+f")]
    ToggleFullscreen,
    #[menu(id = "open_settings", label = "Settings", accel = "super+,")]
    Settings,
}
