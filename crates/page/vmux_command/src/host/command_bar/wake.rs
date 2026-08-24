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

fn command_bar_should_wake(needs_open: bool, has_active_reveal: bool) -> bool {
    needs_open || has_active_reveal
}

fn keep_awake_while_command_bar_opening(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    pending_launch: Option<Res<vmux_core::launcher::PendingLaunch>>,
    pending: Query<&PendingCommandBarReveal>,
) {
    let needs_open = pending_launch.map(|ctx| ctx.needs_open).unwrap_or(false);
    let has_active_reveal = pending.iter().any(PendingCommandBarReveal::is_active);
    if !command_bar_should_wake(needs_open, has_active_reveal) {
        return;
    }
    if let Some(proxy) = proxy {
        let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_bar_wake_covers_defer_and_active_reveal() {
        assert!(command_bar_should_wake(true, false));
        assert!(command_bar_should_wake(false, true));
        assert!(command_bar_should_wake(true, true));
        assert!(!command_bar_should_wake(false, false));
    }
}
