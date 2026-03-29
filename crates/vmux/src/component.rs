//! Scene and input components.

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

#[derive(Component)]
pub struct VmuxWebview;

#[derive(Component)]
pub struct VmuxWorldCamera;

#[derive(Component)]
pub struct AppInputRoot;

#[derive(Actionlike, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppAction {
    Quit,
}
