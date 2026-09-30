#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};

mod completion;
pub mod handler;
mod palette;
pub mod panel;
pub mod project_files;
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
        ))
        .add_systems(Update, keep_awake.after(crate::ReadCommandRequests));
    }
}

fn keep_awake(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    pending: Query<&handler::PendingCommandBarReveal>,
) {
    if !pending
        .iter()
        .any(handler::PendingCommandBarReveal::is_active)
    {
        return;
    }
    if let Some(proxy) = proxy {
        let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
    }
}
