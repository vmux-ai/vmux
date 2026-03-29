//! Input action types and root entity marker.

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

#[derive(Component)]
pub struct AppInputRoot;

#[derive(Actionlike, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppAction {
    Quit,
}
