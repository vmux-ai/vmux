//! Pages that cover the window instead of sitting in a pane.

use bevy::prelude::*;
use bevy_cef::prelude::CefKeyboardTarget;

/// A page that covers the window rather than sitting in a pane, and owns the keyboard while it is
/// showing.
///
/// The shell lays these out against the window instead of a pane, hands them first-responder ahead
/// of the active page, and keeps its own keybindings off them. It asks for the capability rather
/// than for a particular page, so a page crate can declare itself an overlay without the shell
/// being taught its name.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowOverlay;

/// Authoritative lifecycle state of an overlay surface.
///
/// Two different questions get asked, and for the two-to-ten frames the page spends painting its
/// first frame they have different answers: *does it own input?* and *is its surface on screen?*
/// Answering the first one with a visibility test hands the user's keystrokes to whichever page was
/// focused before, so both are resolved here once instead of being re-derived at each call site.
///
/// Ordered least to most revealed, so the frontmost of several overlays is the `max`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum OverlayState {
    /// No keyboard target, or the node is `display: none`.
    #[default]
    Closed,
    /// Owns input; surface still hidden while the page paints its first frame.
    Revealing,
    /// Owns input and the surface is on screen.
    Shown,
}

impl OverlayState {
    pub fn of(display: Display, visibility: Visibility, has_keyboard_target: bool) -> Self {
        if display == Display::None || !has_keyboard_target {
            Self::Closed
        } else if visibility == Visibility::Hidden {
            Self::Revealing
        } else {
            Self::Shown
        }
    }

    /// The state of the frontmost overlay, or `Closed` when none is up.
    pub fn of_any(overlay_q: &OverlayStateQuery) -> Self {
        let mut state = Self::Closed;
        for (node, visibility, has_keyboard_target) in overlay_q.iter() {
            state = state.max(Self::of(node.display, *visibility, has_keyboard_target));
        }
        state
    }

    /// The frontmost of the overlays matching `overlay_q`, whatever the query is filtered to.
    pub fn of_each<'a>(surfaces: impl Iterator<Item = (&'a Node, &'a Visibility, bool)>) -> Self {
        let mut state = Self::Closed;
        for (node, visibility, has_keyboard_target) in surfaces {
            state = state.max(Self::of(node.display, *visibility, has_keyboard_target));
        }
        state
    }

    /// Keyboard target, first-responder handoff, toggle, and dismiss all key off this. True from
    /// the moment the overlay takes the keyboard target, before its surface is revealed.
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

pub type OverlayStateQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Node, &'static Visibility, Has<CefKeyboardTarget>),
    With<WindowOverlay>,
>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keyboard_target_is_closed() {
        assert_eq!(
            OverlayState::of(Display::Flex, Visibility::Inherited, false),
            OverlayState::Closed
        );
    }

    #[test]
    fn display_none_is_closed_even_with_keyboard_target() {
        assert_eq!(
            OverlayState::of(Display::None, Visibility::Inherited, true),
            OverlayState::Closed
        );
    }

    #[test]
    fn hidden_surface_with_keyboard_target_still_owns_input() {
        let state = OverlayState::of(Display::Flex, Visibility::Hidden, true);

        assert_eq!(state, OverlayState::Revealing);
        assert!(state.owns_input());
        assert!(!state.is_shown());
    }

    #[test]
    fn visible_surface_with_keyboard_target_is_shown() {
        let state = OverlayState::of(Display::Flex, Visibility::Inherited, true);

        assert_eq!(state, OverlayState::Shown);
        assert!(state.owns_input());
        assert!(state.is_shown());
    }

    #[test]
    fn closed_owns_nothing() {
        assert!(!OverlayState::Closed.owns_input());
        assert!(!OverlayState::Closed.is_shown());
        assert!(!OverlayState::Closed.is_revealing());
    }

    /// The frontmost overlay wins, so one still painting cannot mask another already up.
    #[test]
    fn the_furthest_along_overlay_is_the_greatest() {
        assert!(OverlayState::Revealing > OverlayState::Closed);
        assert!(OverlayState::Shown > OverlayState::Revealing);
    }
}
