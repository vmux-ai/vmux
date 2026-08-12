//! The terminal types other crates are allowed to depend on.
//!
//! [`TerminalPlugin`](crate::TerminalPlugin) starts the background service and opens a PTY
//! connection as it builds. A crate that only wants to push input into a terminal that already
//! exists must not pay for that, so the registrations live here on their own.

use bevy::prelude::*;

use crate::plugin::{
    RunShellRequest, TerminalFontSizeCommand, TerminalReinputRequest, TerminalSendRequest,
};

/// Registers every terminal message that crates outside `vmux_terminal` send or read.
///
/// [`TerminalPlugin`](crate::TerminalPlugin) adds this, so a full app is unaffected. Add it
/// directly from a plugin that drives a terminal without hosting the service —
/// `AgentSessionPlugin` does — instead of restating the registrations locally.
///
/// Adding it more than once is deliberate and safe: `add_message` skips a type that is already
/// present, and [`Plugin::is_unique`] is `false` so repeated composition does not trip Bevy's
/// duplicate-plugin check.
pub struct TerminalContractPlugin;

impl Plugin for TerminalContractPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RunShellRequest>()
            .add_message::<TerminalFontSizeCommand>()
            .add_message::<TerminalReinputRequest>()
            .add_message::<TerminalSendRequest>();
    }

    fn is_unique(&self) -> bool {
        false
    }
}
