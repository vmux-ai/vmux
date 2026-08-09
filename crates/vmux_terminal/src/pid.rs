use bevy::prelude::*;
use std::collections::HashMap;

pub use vmux_core::focus_pane_entity;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub u32);

#[derive(Resource, Default, Debug)]
pub struct PidToEntity(pub HashMap<u32, Entity>);

pub fn track_pid_inserts(
    mut map: ResMut<PidToEntity>,
    inserted: Query<(Entity, &Pid), Added<Pid>>,
) {
    for (entity, Pid(pid)) in &inserted {
        map.0.insert(*pid, entity);
    }
}

pub fn track_pid_removals(
    mut map: ResMut<PidToEntity>,
    mut removed: RemovedComponents<Pid>,
    survivors: Query<&Pid>,
) {
    for entity in removed.read() {
        if let Ok(Pid(pid)) = survivors.get(entity) {
            map.0.remove(pid);
        } else {
            map.0.retain(|_, &mut e| e != entity);
        }
    }
}

#[cfg(test)]
#[path = "pid.test.rs"]
mod tests;
