#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::prelude::*;
mod completion;
mod controller;
mod model;
mod palette;
pub mod panel;
pub mod project_files;
pub mod work_snapshot;

pub use controller::{
    ApplyCommandBarRequests, CommandBarNativeSize, CommandBarOpenRequest, PendingCommandBarReveal,
    WriteCommandBarRequests,
};

#[derive(EntityEvent)]
pub struct CommandBarDismiss {
    #[event_target]
    webview: Entity,
    restore_keyboard: bool,
}

impl CommandBarDismiss {
    pub fn new(webview: Entity, restore_keyboard: bool) -> Self {
        Self {
            webview,
            restore_keyboard,
        }
    }
}

pub struct CommandBarPlugin;

impl Plugin for CommandBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            completion::CompletionPlugin,
            controller::CommandBarControllerPlugin,
            palette::PalettePlugin,
            panel::PanelPlugin,
        ));
    }
}
