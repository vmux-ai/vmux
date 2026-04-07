use bevy::prelude::*;
use vmux_macro::{OsMenu, OsSubMenu};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, _app: &mut App) {}
}

#[derive(Event, OsMenu, Debug, Clone, Copy, PartialEq, Eq)]
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
}
