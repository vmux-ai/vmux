//! Observers for pane lifecycle (diagnostics when layout invariants break).

use bevy::ecs::lifecycle::Despawn;
use bevy::log::warn;
use bevy::prelude::*;

use crate::{Layout, Pane, Window};

/// [`try_kill_active_pane`](crate::pane_ops::try_kill_active_pane) updates [`Layout`](crate::Layout)
/// before despawning. If a pane entity is despawned while still listed as a leaf, the tree is stale.
pub fn warn_if_pane_despawn_still_in_layout(
    trigger: On<Despawn, Pane>,
    layout_q: Query<&Layout, With<Window>>,
) {
    let entity = trigger.entity;
    let Ok(tree) = layout_q.single() else {
        return;
    };
    if tree.root.contains_leaf(entity) {
        warn!(
            "Pane {entity:?} despawned while still referenced by Layout; update the tree before despawning"
        );
    }
}
