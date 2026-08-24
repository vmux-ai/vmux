use crate::KeyboardOwner;
use bevy::prelude::*;
use vmux_flex::prelude::*;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowOverlay;

#[derive(Component, Clone, Copy, Debug)]
pub struct OverlayShownInline;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum OverlayState {
    #[default]
    Closed,
    Revealing,
    Shown,
}

impl OverlayState {
    pub fn of(
        display: Display,
        visibility: Visibility,
        has_keyboard_target: bool,
        shown_inline: bool,
    ) -> Self {
        if shown_inline {
            return Self::Shown;
        }
        if display == Display::None || !has_keyboard_target {
            Self::Closed
        } else if visibility == Visibility::Hidden {
            Self::Revealing
        } else {
            Self::Shown
        }
    }

    pub fn of_any(overlay_q: &OverlayStateQuery) -> Self {
        let mut state = Self::Closed;
        for (node, visibility, has_keyboard_target, shown_inline) in overlay_q.iter() {
            state = state.max(Self::of(
                node.display,
                *visibility,
                has_keyboard_target,
                shown_inline,
            ));
        }
        state
    }

    pub fn of_each<'a>(
        surfaces: impl Iterator<Item = (&'a Node, &'a Visibility, bool, bool)>,
    ) -> Self {
        let mut state = Self::Closed;
        for (node, visibility, has_keyboard_target, shown_inline) in surfaces {
            state = state.max(Self::of(
                node.display,
                *visibility,
                has_keyboard_target,
                shown_inline,
            ));
        }
        state
    }

    pub fn owns_input(self) -> bool {
        !matches!(self, Self::Closed)
    }

    pub fn is_shown(self) -> bool {
        matches!(self, Self::Shown)
    }

    pub fn is_revealing(self) -> bool {
        matches!(self, Self::Revealing)
    }
}

pub type OverlayStateQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Node,
        &'static Visibility,
        Has<KeyboardOwner>,
        Has<OverlayShownInline>,
    ),
    With<WindowOverlay>,
>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keyboard_target_is_closed() {
        assert_eq!(
            OverlayState::of(Display::Flex, Visibility::Visible, false, false),
            OverlayState::Closed
        );
    }

    #[test]
    fn display_none_is_closed_even_with_keyboard_target() {
        assert_eq!(
            OverlayState::of(Display::None, Visibility::Visible, true, false),
            OverlayState::Closed
        );
    }

    #[test]
    fn hidden_surface_with_keyboard_target_still_owns_input() {
        let state = OverlayState::of(Display::Flex, Visibility::Hidden, true, false);

        assert_eq!(state, OverlayState::Revealing);
        assert!(state.owns_input());
        assert!(!state.is_shown());
    }

    #[test]
    fn visible_surface_with_keyboard_target_is_shown() {
        let state = OverlayState::of(Display::Flex, Visibility::Visible, true, false);

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

    #[test]
    fn an_inline_surface_is_shown_however_the_overlay_node_looks() {
        assert_eq!(
            OverlayState::of(Display::None, Visibility::Hidden, false, true),
            OverlayState::Shown
        );
    }

    #[test]
    fn the_furthest_along_overlay_is_the_greatest() {
        assert!(OverlayState::Revealing > OverlayState::Closed);
        assert!(OverlayState::Shown > OverlayState::Revealing);
    }
}
