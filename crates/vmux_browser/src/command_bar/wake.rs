//! Keeping the loop awake while the bar opens.
//!
//! Opening spans several reactive frames, and the desktop app used to own this because it
//! owns the event loop proxy. But nothing here is about the app: the condition is entirely the
//! bar's own — a deferred open and a reveal waiting on its ack — so it belongs with the bar,
//! and the proxy is reachable from any crate that links `bevy_winit`.

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};

use crate::command_bar::handler::PendingCommandBarReveal;

pub(crate) struct CommandBarWakePlugin;

impl Plugin for CommandBarWakePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            keep_awake_while_command_bar_opening.after(vmux_command::ReadAppCommands),
        );
    }
}

fn command_bar_should_wake(needs_open: bool, has_active_reveal: bool) -> bool {
    needs_open || has_active_reveal
}

/// The bar opens across several reactive frames: the first shortcut may defer
/// (`NewStackContext::needs_open`) until the webview is ready, then a reveal
/// (`PendingCommandBarReveal`) waits for the rendered/sized ack. Without an explicit wake the loop
/// idles after the keystroke and the open stalls until the next input — the user has to press
/// Cmd+K/Cmd+L twice. Runs after `ReadAppCommands` so a `needs_open` set this frame is observed.
/// Self-terminating: once revealed, `needs_open` clears and the placeholder reveal is
/// `open_id == 0` (inactive), so we stop waking.
fn keep_awake_while_command_bar_opening(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    new_stack_ctx: Option<Res<vmux_layout::NewStackContext>>,
    pending: Query<&PendingCommandBarReveal>,
) {
    let needs_open = new_stack_ctx.map(|ctx| ctx.needs_open).unwrap_or(false);
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
