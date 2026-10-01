use bevy::prelude::*;
use vmux_ecs::overlay::WindowOverlay;
use vmux_flex::prelude::*;

use crate::host::bundle::CommandBar;

pub use controller::{
    CommandBarNativeSize, CommandBarOpenRequest, PendingCommandBarReveal, WriteCommandBarRequests,
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
        ))
        .add_systems(Startup, spawn);
    }
}

impl CommandBar {
    fn surface() -> impl Bundle {
        (
            CommandBar,
            WindowOverlay,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                display: Display::None,
                ..default()
            },
            Transform::default(),
            Visibility::Hidden,
        )
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn(CommandBar::surface());
}
