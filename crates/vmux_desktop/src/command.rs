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

    #[menu(label = "Camera")]
    Camera(CameraCommand),
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
    #[menu(id = "tab_next", label = "Select Next Tab")]
    #[bind(direct = "Ctrl+Tab")]
    Next,

    #[menu(id = "tab_previous", label = "Select Previous Tab")]
    #[bind(direct = "Shift+Ctrl+Tab")]
    Previous,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneCommand {
    #[default]
    #[menu(id = "split_v", label = "Split Vertically")]
    #[bind(chord = "Ctrl+b, %")]
    SplitV,
    #[menu(id = "split_h", label = "Split Horizontally")]
    #[bind(chord = "Ctrl+b, \"")]
    SplitH,
    #[menu(id = "toggle_pane", label = "Toggle Pane")]
    #[bind(chord = "Ctrl+b, o")]
    Toggle,
    #[menu(id = "close_pane", label = "Close Pane")]
    #[bind(chord = "Ctrl+b, x")]
    Close,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraCommand {
    #[default]
    #[menu(id = "reset_camera", label = "Reset Camera")]
    Reset,

    #[menu(id = "toggle_free_camera", label = "Toggle Free Camera")]
    ToggleFreeCamera,
}
