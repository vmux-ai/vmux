use bevy_app::{App, Plugin};
use bevy_ecs::prelude::{ChildOf, Children};
use moonshine_save::prelude::Save;
use vmux_ecs::store::SchemaPlugin;

use crate::{Pane, PaneId, PaneSize, PaneSplit, PaneSplitDirection, SpawnSeq};

pub struct StorePlugin;

impl Plugin for StorePlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<Pane, Save>()
            .register_required_components::<PaneId, Save>()
            .register_required_components::<PaneSplit, Save>()
            .register_required_components::<PaneSize, Save>()
            .register_required_components::<SpawnSeq, Save>()
            .add_plugins(SchemaPlugin::<Schema>::default());
    }
}

#[vmux_ecs::store(
    Pane,
    PaneId,
    PaneSize,
    PaneSplit,
    PaneSplitDirection,
    SpawnSeq,
    ChildOf,
    Children
)]
pub struct Schema;
