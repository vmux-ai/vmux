//! Whether a laid-out node is shown.

use bevy::prelude::*;

/// Whether a node's surface is shown.
///
/// Two states, not Bevy's three. `bevy_camera::Visibility` carries `Inherited` because a
/// propagation system resolves it against the parent — but that system belongs to the render
/// stack, which this app does not have, so nothing ever propagated and `Inherited` only ever
/// meant "not hidden". Hiding a subtree is done through [`crate::Display::None`] instead, which
/// layout genuinely does propagate.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
}

impl Visibility {
    /// `Hidden` when the caller says to hide, which is how every writer here decides.
    pub fn hidden(hidden: bool) -> Self {
        if hidden { Self::Hidden } else { Self::Visible }
    }

    pub fn is_hidden(self) -> bool {
        matches!(self, Self::Hidden)
    }
}
