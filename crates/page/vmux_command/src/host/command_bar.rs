#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::prelude::*;

mod completion;
pub mod handler;
pub mod key;
pub mod panel;
pub mod project_files;
pub mod state;
pub mod wake;
pub mod work_snapshot;

pub struct CommandBarPlugin;

impl Plugin for CommandBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            completion::CompletionPlugin,
            handler::InputPlugin,
            key::KeyPlugin,
            panel::PanelPlugin,
            wake::WakePlugin,
        ));
    }
}
