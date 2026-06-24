use bevy::prelude::*;

pub mod client;
pub mod framing;
pub mod manager;
pub mod registry;

pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, _app: &mut App) {}
}
