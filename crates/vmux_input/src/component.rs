//! Input action types and root entity marker.

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

#[derive(Component)]
pub struct AppInputRoot;

/// Tmux-style prefix mode: after **Ctrl+B**, the next key within [`PREFIX_TIMEOUT_SECS`] is a window-manager command.
#[derive(Component, Debug, Clone, Copy)]
pub struct VmuxPrefixState {
    /// Waiting for a command key after the prefix chord.
    pub awaiting: bool,
    /// `Time::elapsed_secs()` value when the prefix expires.
    pub deadline_secs: f32,
}

impl Default for VmuxPrefixState {
    fn default() -> Self {
        Self {
            awaiting: false,
            deadline_secs: 0.0,
        }
    }
}

/// Seconds to wait for a key after **Ctrl+B** (tmux-like).
pub const PREFIX_TIMEOUT_SECS: f32 = 1.5;

#[derive(Actionlike, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppAction {
    Quit,
}
