use bevy::prelude::*;
use bevy_cef::prelude::CefKeyboardTarget;

use crate::window::Modal;

/// Authoritative command bar lifecycle state.
///
/// Two different questions get asked about the command bar, and for the two-to-ten frames the CEF
/// page spends painting its first frame they have different answers: *does it own input?* and *is
/// its surface on screen?* Answering the first one with a visibility test hands the user's
/// keystrokes to whichever page was focused before, so both questions are resolved here once
/// instead of being re-derived at each call site.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommandBarState {
    /// No keyboard target, or the node is `display: none`.
    #[default]
    Closed,
    /// Owns input; surface still hidden while the page paints its first frame.
    Revealing,
    /// Owns input and the surface is on screen.
    Shown,
}

impl CommandBarState {
    pub fn from_modal(display: Display, visibility: Visibility, has_keyboard_target: bool) -> Self {
        if display == Display::None || !has_keyboard_target {
            Self::Closed
        } else if visibility == Visibility::Hidden {
            Self::Revealing
        } else {
            Self::Shown
        }
    }

    /// Keyboard target, first-responder handoff, toggle, and dismiss all key off this. True from
    /// the moment the bar takes the keyboard target, before its surface is revealed.
    pub fn owns_input(self) -> bool {
        !matches!(self, Self::Closed)
    }

    /// Native view placement, overlay compositing, and pointer hit testing key off this.
    pub fn is_shown(self) -> bool {
        matches!(self, Self::Shown)
    }

    /// Owns input but is not yet on screen — the native view is parked offscreen so it can hold
    /// first responder without being visible.
    pub fn is_revealing(self) -> bool {
        matches!(self, Self::Revealing)
    }
}

pub type CommandBarStateQuery<'w, 's> =
    Query<'w, 's, (&'static Node, &'static Visibility, Has<CefKeyboardTarget>), With<Modal>>;

pub fn command_bar_state(modal_q: &CommandBarStateQuery) -> CommandBarState {
    modal_q
        .iter()
        .map(|(node, visibility, has_keyboard_target)| {
            CommandBarState::from_modal(node.display, *visibility, has_keyboard_target)
        })
        .max_by_key(|state| match state {
            CommandBarState::Closed => 0,
            CommandBarState::Revealing => 1,
            CommandBarState::Shown => 2,
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "state.test.rs"]
mod tests;
