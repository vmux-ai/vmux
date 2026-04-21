use bevy::prelude::*;
use vmux_macro::{CommandPalette, DefaultKeyBindings, OsMenu, OsSubMenu};

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

#[derive(Message, OsMenu, DefaultKeyBindings, CommandPalette, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    #[menu(label = "Scene")]
    Scene(SceneCommand),

    #[menu(label = "Window")]
    Window(WindowCommand),

    #[menu(label = "Side Sheet")]
    SideSheet(SideSheetCommand),

    #[menu(label = "Space")]
    Space(SpaceCommand),

    #[menu(label = "Pane")]
    Pane(PaneCommand),

    #[menu(label = "Tab")]
    Tab(TabCommand),

    #[menu(label = "Browser")]
    Browser(BrowserCommand),
}

#[derive(OsSubMenu, DefaultKeyBindings, CommandPalette, Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    #[menu(id = "tab_duplicate", label = "Duplicate Tab")]
    Duplicate,
    #[menu(id = "tab_pin", label = "Pin Tab")]
    Pin,
    #[menu(id = "tab_mute", label = "Mute Tab")]
    Mute,
    #[menu(id = "tab_move_to_pane", label = "Move Tab to Pane")]
    MoveToPane,
    #[menu(id = "tab_new_terminal", label = "New Terminal Tab\tCtrl+B, `")]
    #[bind(chord = "Ctrl+b, `")]
    NewTerminal,
}

#[derive(OsSubMenu, DefaultKeyBindings, CommandPalette, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserCommand {
    #[default]
    #[menu(id = "browser_prev_page", label = "Back", accel = "super+[")]
    PrevPage,
    #[menu(id = "browser_next_page", label = "Forward", accel = "super+]")]
    NextPage,
    #[menu(id = "browser_reload", label = "Reload", accel = "super+r")]
    Reload,
    #[menu(id = "browser_hard_reload", label = "Hard Reload", accel = "super+shift+r")]
    #[bind(direct = "Super+Shift+r")]
    HardReload,
    #[menu(id = "browser_stop", label = "Stop Loading")]
    Stop,
    #[menu(id = "browser_focus_address_bar", label = "Open Location", accel = "super+l")]
    FocusAddressBar,
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

#[derive(OsSubMenu, DefaultKeyBindings, CommandPalette, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneCommand {
    #[default]
    #[menu(id = "split_v", label = "Split Vertically")]
    SplitV,
    #[menu(id = "split_h", label = "Split Horizontally")]
    SplitH,
    #[menu(id = "toggle_pane", label = "Toggle Pane\tCtrl+B, T")]
    #[bind(chord = "Ctrl+b, t")]
    Toggle,
    #[menu(id = "close_pane", label = "Close Pane\tCtrl+B, X")]
    #[bind(chord = "Ctrl+b, x")]
    Close,
    #[menu(id = "zoom_pane", label = "Zoom Pane\tCtrl+B, Z")]
    #[bind(chord = "Ctrl+b, z")]
    Zoom,
    #[menu(id = "select_pane_left", label = "Select Left Pane\tCtrl+B, H")]
    #[bind(chord = "Ctrl+b, h")]
    SelectLeft,
    #[menu(id = "select_pane_right", label = "Select Right Pane\tCtrl+B, L")]
    #[bind(chord = "Ctrl+b, l")]
    SelectRight,
    #[menu(id = "select_pane_up", label = "Select Up Pane\tCtrl+B, K")]
    #[bind(chord = "Ctrl+b, k")]
    SelectUp,
    #[menu(id = "select_pane_down", label = "Select Down Pane\tCtrl+B, J")]
    #[bind(chord = "Ctrl+b, j")]
    SelectDown,
    #[menu(id = "swap_pane_prev", label = "Swap Pane Previous\tCtrl+B, {")]
    #[bind(chord = "Ctrl+b, {")]
    SwapPrev,
    #[menu(id = "swap_pane_next", label = "Swap Pane Next\tCtrl+B, }")]
    #[bind(chord = "Ctrl+b, }")]
    SwapNext,
    #[menu(id = "rotate_forward", label = "Rotate Forward\tCtrl+B, Ctrl+O")]
    #[bind(chord = "Ctrl+b, Ctrl+o")]
    RotateForward,
    #[menu(id = "rotate_backward", label = "Rotate Backward\tCtrl+B, Alt+O")]
    #[bind(chord = "Ctrl+b, Alt+o")]
    RotateBackward,
    #[menu(id = "equalize_pane_size", label = "Equalize Pane Size\tCtrl+B, =")]
    #[bind(chord = "Ctrl+b, =")]
    EqualizeSize,
    #[menu(id = "resize_pane_left", label = "Resize Pane Left\tCtrl+B, Alt+Left")]
    #[bind(chord = "Ctrl+b, Alt+ArrowLeft")]
    ResizeLeft,
    #[menu(id = "resize_pane_right", label = "Resize Pane Right\tCtrl+B, Alt+Right")]
    #[bind(chord = "Ctrl+b, Alt+ArrowRight")]
    ResizeRight,
    #[menu(id = "resize_pane_up", label = "Resize Pane Up\tCtrl+B, Alt+Up")]
    #[bind(chord = "Ctrl+b, Alt+ArrowUp")]
    ResizeUp,
    #[menu(id = "resize_pane_down", label = "Resize Pane Down\tCtrl+B, Alt+Down")]
    #[bind(chord = "Ctrl+b, Alt+ArrowDown")]
    ResizeDown,
}

#[derive(OsSubMenu, DefaultKeyBindings, CommandPalette, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpaceCommand {
    #[default]
    #[menu(id = "new_space", label = "New Space\tCtrl+B, C")]
    #[bind(chord = "Ctrl+b, c")]
    New,
    #[menu(id = "close_space", label = "Close Space\tCtrl+B, &")]
    #[bind(chord = "Ctrl+b, &")]
    Close,
    #[menu(id = "next_space", label = "Next Space", accel = "ctrl+tab")]
    Next,
    #[menu(id = "prev_space", label = "Previous Space", accel = "ctrl+shift+tab")]
    Previous,
    #[menu(id = "rename_space", label = "Rename Space")]
    Rename,
}

#[derive(OsSubMenu, DefaultKeyBindings, CommandPalette, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SideSheetCommand {
    #[default]
    #[menu(id = "toggle_side_sheet", label = "Toggle Side Sheet\tCtrl+B, S")]
    #[bind(chord = "Ctrl+b, s")]
    Toggle,
    #[menu(id = "toggle_side_sheet_right", label = "Toggle Right Sheet")]
    ToggleRight,
    #[menu(id = "toggle_side_sheet_bottom", label = "Toggle Bottom Sheet")]
    ToggleBottom,
}

#[derive(OsSubMenu, DefaultKeyBindings, CommandPalette, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SceneCommand {
    #[default]
    #[menu(id = "toggle_free_camera", label = "Toggle Camera Mode")]
    #[bind(chord = "Ctrl+b, Enter")]
    ToggleFreeCamera,
}

#[derive(OsSubMenu, DefaultKeyBindings, CommandPalette, Debug, Clone, Copy, PartialEq, Eq, Default)]
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
