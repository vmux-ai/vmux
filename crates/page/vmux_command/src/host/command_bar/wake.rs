use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};

use crate::command_bar::handler::PendingCommandBarReveal;

pub(crate) struct CommandBarWakePlugin;

impl Plugin for CommandBarWakePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            keep_awake_while_command_bar_opening.after(crate::ReadAppCommands),
        );
    }
}

fn keep_awake_while_command_bar_opening(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    pending: Query<&PendingCommandBarReveal>,
) {
    if !pending.iter().any(PendingCommandBarReveal::is_active) {
        return;
    }
    if let Some(proxy) = proxy {
        let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
    }
}
