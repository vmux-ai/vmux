//! Global command-palette state (⌘T / Ctrl+T, ⌘L / Ctrl+L) shared with input/CEF suppression.

use bevy::prelude::*;

/// Open/closed state, query string, and list selection for the centered command palette.
#[derive(Resource, Default, Clone)]
pub struct VmuxCommandPaletteState {
    pub open: bool,
    pub input: CommandPaletteInputState,
    /// Row index in one list: open-tab slots first, then omnibox / web / history or GitHub / commands / layout / close.
    pub selection: usize,
}

/// Query text-edit model (string + caret/selection) for browser-like palette editing.
#[derive(Default, Clone)]
pub struct CommandPaletteInputState {
    pub query: String,
    /// Caret position in `query` (character index).
    pub caret: usize,
    /// Optional selection anchor (character index). Selection is active when this is `Some` and differs from `caret`.
    pub selection_anchor: Option<usize>,
    /// [`Time::elapsed_secs`] when the palette was last opened; resets caret blink phase.
    pub caret_blink_t0: f32,
}
