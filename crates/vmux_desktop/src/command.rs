use bevy::prelude::*;
use vmux_macro::{OsMenu, OsSubMenu};

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

#[derive(Message, OsMenu, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    #[menu(label = "Space")]
    Space(SpaceCommand),

    #[menu(label = "Pane")]
    Pane(PaneCommand),

    #[menu(label = "Camera")]
    Camera(CameraCommand),
}

#[derive(OsSubMenu, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpaceCommand {
    #[default]
    #[menu(id = "new_space", label = "New Space")]
    New,
}

#[derive(OsSubMenu, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneCommand {
    #[default]
    #[menu(id = "split_v", label = "Split Vertically")]
    SplitV,
    #[menu(id = "split_h", label = "Split Horizontally")]
    SplitH,
}

#[derive(OsSubMenu, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraCommand {
    #[default]
    #[menu(id = "reset_camera", label = "Reset Camera")]
    Reset,

    #[menu(id = "toggle_free_camera", label = "Toggle Free Camera")]
    ToggleFreeCamera,
}
