use crate::CommandBar;
use bevy::prelude::*;
use vmux_core::KeyboardOwner;
use vmux_core::overlay::OverlayState;
use vmux_flex::prelude::*;

pub type CommandBarStateQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Node,
        &'static Visibility,
        Has<KeyboardOwner>,
        Has<vmux_core::overlay::OverlayShownInline>,
    ),
    With<CommandBar>,
>;

pub fn command_bar_state(modal_q: &CommandBarStateQuery) -> OverlayState {
    OverlayState::of_each(modal_q.iter())
}
