use bevy::prelude::*;
use vmux_macro::{DefaultKeyBindings, OsMenu, OsSubMenu};

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

#[derive(Message, OsMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    #[menu(label = "Space")]
    Space(SpaceCommand),

    #[menu(label = "Pane")]
    Pane(PaneCommand),

    #[menu(label = "Tab")]
    Tab(TabCommand),

    #[menu(label = "Side Sheet")]
    SideSheet(SideSheetCommand),

    #[menu(label = "Camera")]
    Camera(CameraCommand),

    #[menu(label = "Browser")]
    Browser(BrowserCommand),
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SideSheetCommand {
    #[default]
    #[menu(id = "toggle_side_sheet", label = "Toggle Side Sheet")]
    Toggle,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpaceCommand {
    #[default]
    #[menu(id = "new_space", label = "New Space")]
    New,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabCommand {
    #[default]
    #[menu(id = "tab_new", label = "New Tab", accel = "super+t")]
    New,

    #[menu(id = "tab_close", label = "Close Tab", accel = "super+w")]
    Close,

    #[menu(id = "tab_next", label = "Select Next Tab")]
    Next,

    #[menu(id = "tab_previous", label = "Select Previous Tab")]
    Previous,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneCommand {
    #[default]
    #[menu(id = "split_v", label = "Split Vertically")]
    SplitV,
    #[menu(id = "split_h", label = "Split Horizontally")]
    SplitH,
    #[menu(id = "toggle_pane", label = "Toggle Pane")]
    Toggle,
    #[menu(id = "close_pane", label = "Close Pane")]
    Close,
    #[menu(id = "zoom_pane", label = "Zoom Pane")]
    Zoom,
    #[menu(id = "select_pane_left", label = "Select Left Pane")]
    SelectLeft,
    #[menu(id = "select_pane_right", label = "Select Right Pane")]
    SelectRight,
    #[menu(id = "select_pane_up", label = "Select Up Pane")]
    SelectUp,
    #[menu(id = "select_pane_down", label = "Select Down Pane")]
    SelectDown,
    #[menu(id = "swap_pane_prev", label = "Swap Pane Previous")]
    SwapPrev,
    #[menu(id = "swap_pane_next", label = "Swap Pane Next")]
    SwapNext,
    #[menu(id = "rotate_forward", label = "Rotate Forward")]
    RotateForward,
    #[menu(id = "rotate_backward", label = "Rotate Backward")]
    RotateBackward,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserCommand {
    #[default]
    #[menu(id = "browser_prev_page", label = "Previous Page")]
    PrevPage,

    #[menu(id = "browser_next_page", label = "Next Page")]
    NextPage,

    #[menu(id = "browser_reload", label = "Reload")]
    Reload,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraCommand {
    #[default]
    #[menu(id = "reset_camera", label = "Reset Camera")]
    Reset,

    #[menu(id = "toggle_free_camera", label = "Toggle Free Camera")]
    ToggleFreeCamera,
}
