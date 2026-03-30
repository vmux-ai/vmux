//! Focus marker shared across layout, input, and webview.

use bevy::prelude::*;

/// Marks the focused pane (e.g. paired with `vmux_layout::Pane` in the tiling UI).
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct Active;
