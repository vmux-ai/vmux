//! Sync `CefKeyboardTarget` with [`Active`] after splits / focus cycling, and in [`PreUpdate`]
//! before CEF key delivery. Pointer handlers in `pane_spawn` set both immediately on hover/press.

use bevy::prelude::*;
use bevy_cef::prelude::CefKeyboardTarget;
use vmux_core::Active;
use vmux_layout::Pane;

pub fn sync_cef_keyboard_target(
    mut commands: Commands,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    panes: Query<Entity, With<Pane>>,
) {
    let Ok(active_ent) = active.single() else {
        return;
    };
    for e in panes.iter() {
        if e == active_ent {
            commands.entity(e).insert(CefKeyboardTarget);
        } else {
            commands.entity(e).remove::<CefKeyboardTarget>();
        }
    }
}
