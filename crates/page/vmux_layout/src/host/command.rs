use bevy::prelude::*;
use vmux_command::ReadCommandRequests;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum LayoutRequestSet {
    Dispatch,
    Prepare,
    Handle,
}

pub(super) struct LayoutRequestPlugin;

impl Plugin for LayoutRequestPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                LayoutRequestSet::Dispatch,
                LayoutRequestSet::Prepare,
                LayoutRequestSet::Handle,
            )
                .chain()
                .in_set(ReadCommandRequests),
        );
    }
}
