//! Observers for pane lifecycle (diagnostics when layout invariants break).

use bevy::ecs::lifecycle::Despawn;
use bevy::log::warn;
use bevy::prelude::*;

use crate::{LayoutTree, Pane, Root};

/// [`try_kill_active_pane`](crate::pane_ops::try_kill_active_pane) updates [`LayoutTree`](crate::LayoutTree)
/// before despawning. If a pane entity is despawned while still listed as a leaf, the tree is stale.
pub fn warn_if_pane_despawn_still_in_layout(
    trigger: On<Despawn, Pane>,
    layout_q: Query<&LayoutTree, With<Root>>,
) {
    let entity = trigger.entity;
    let Ok(tree) = layout_q.single() else {
        return;
    };
    if tree.root.contains_leaf(entity) {
        warn!(
            "Pane {entity:?} despawned while still referenced by LayoutTree; update the tree before despawning"
        );
    }
}
