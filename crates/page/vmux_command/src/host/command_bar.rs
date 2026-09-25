#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::prelude::*;

mod completion;
pub mod handler;
mod palette;
pub mod panel;
pub mod project_files;
pub mod state;
pub mod wake;
pub mod work_snapshot;

#[derive(EntityEvent)]
struct CloseCommandBar {
    #[event_target]
    webview: Entity,
    restore_keyboard: bool,
}

impl CloseCommandBar {
    fn after(webview: Entity, custom_keyboard_restore: bool) -> Self {
        Self {
            webview,
            restore_keyboard: !custom_keyboard_restore,
        }
    }
}

pub struct CommandBarPlugin;

impl Plugin for CommandBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            completion::CompletionPlugin,
            handler::InputPlugin,
            palette::PalettePlugin,
            panel::PanelPlugin,
            wake::WakePlugin,
        ));
    }
}
