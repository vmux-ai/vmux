//! Input actions and prefix-routing markers (re-exported from [`vmux_core`] for layout sharing).

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

pub use vmux_core::input_root::{
    AppInputRoot, PREFIX_TIMEOUT_SECS, VmuxPrefixChordSet, VmuxPrefixState,
};

#[derive(Actionlike, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppAction {
    Quit,
    /// Centered command palette (⌘T on macOS, Ctrl+T elsewhere).
    ToggleCommandPalette,
    /// Command palette with the active pane’s current URL in the field (⌘L on macOS, Ctrl+L elsewhere).
    FocusCommandPaletteUrl,
    /// History overlay (⌘Y on macOS, Ctrl+Shift+H elsewhere).
    ToggleHistory,
}
