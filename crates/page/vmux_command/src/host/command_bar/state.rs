use crate::CommandBar;
use bevy::prelude::*;
use bevy_cef::prelude::CefKeyboardTarget;
use vmux_core::overlay::OverlayState;

/// The bar's own view of [`OverlayState`], filtered to its entity rather than to any overlay.
pub type CommandBarStateQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Node,
        &'static Visibility,
        Has<CefKeyboardTarget>,
        Has<vmux_core::overlay::OverlayShownInline>,
    ),
    With<CommandBar>,
>;

pub fn command_bar_state(modal_q: &CommandBarStateQuery) -> OverlayState {
    OverlayState::of_each(modal_q.iter())
}
