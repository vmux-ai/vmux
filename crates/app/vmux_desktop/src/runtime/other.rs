use bevy::prelude::*;

pub(super) struct RuntimePlatformPlugin;

impl Plugin for RuntimePlatformPlugin {
    fn build(&self, _: &mut App) {}
}

pub(super) fn live_resize_active() -> bool {
    false
}

pub(super) fn native_pointer_inside() -> bool {
    false
}
