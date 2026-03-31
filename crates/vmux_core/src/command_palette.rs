//! Global command-palette state (⌘T / Ctrl+T, ⌘L / Ctrl+L) shared with input/CEF suppression.

use bevy::prelude::*;

/// Open/closed state, query string, and list selection for the centered command palette.
#[derive(Resource, Default, Clone)]
pub struct VmuxCommandPaletteState {
    pub open: bool,
    pub query: String,
    /// Row index in one list: open-tab slots first, then omnibox / web / history or GitHub / commands / layout / close.
    pub selection: usize,
    /// [`Time::elapsed_secs`] when the palette was last opened; resets caret blink phase.
    pub caret_blink_t0: f32,
}
