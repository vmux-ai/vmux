use bevy::prelude::*;
pub use controller::{
    ApplyCommandBarRequests, CommandBarNativeSize, CommandBarOpenRequest, PendingCommandBarReveal,
    WriteCommandBarRequests,
};
pub use model::ResumeRows;
pub use panel::CommandBarPanelActive;

mod completion;
mod controller;
mod model;
mod palette;
mod panel;
mod project_files;
mod work_snapshot;

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
            controller::Plugin,
            palette::PalettePlugin,
            panel::PanelPlugin,
        ));
    }
}
