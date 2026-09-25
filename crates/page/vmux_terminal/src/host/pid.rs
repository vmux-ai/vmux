use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use std::collections::HashMap;

pub use vmux_core::focus_pane_entity;

pub struct PidPlugin;

impl Plugin for PidPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PidToEntity>()
            .add_systems(Update, (track_pid_inserts, track_pid_removals).chain());
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub u32);

impl Pid {
    pub fn page_url(&self) -> String {
        format!("{}{}", vmux_layout::event::TERMINAL_PAGE_URL, self.0)
    }
}

#[derive(Resource, Default, Debug)]
pub struct PidToEntity {
    by_pid: HashMap<u32, Entity>,
    by_entity: EntityHashMap<u32>,
}

impl PidToEntity {
    pub fn get(&self, pid: u32) -> Option<Entity> {
        self.by_pid.get(&pid).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, Entity)> + '_ {
        self.by_pid.iter().map(|(pid, entity)| (*pid, *entity))
    }

    fn insert(&mut self, entity: Entity, pid: u32) {
        if let Some(previous_pid) = self.by_entity.insert(entity, pid) {
            self.by_pid.remove(&previous_pid);
        }
        if let Some(previous_entity) = self.by_pid.insert(pid, entity)
            && previous_entity != entity
        {
            self.by_entity.remove(&previous_entity);
        }
    }

    fn remove(&mut self, entity: Entity) {
        let Some(pid) = self.by_entity.remove(&entity) else {
            return;
        };
        if self.by_pid.get(&pid) == Some(&entity) {
            self.by_pid.remove(&pid);
        }
    }
}

impl FromIterator<(u32, Entity)> for PidToEntity {
    fn from_iter<T: IntoIterator<Item = (u32, Entity)>>(iter: T) -> Self {
        let mut index = Self::default();
        for (pid, entity) in iter {
            index.insert(entity, pid);
        }
        index
    }
}

pub(crate) fn track_pid_inserts(
    mut map: ResMut<PidToEntity>,
    inserted: Query<(Entity, &Pid), Changed<Pid>>,
) {
    for (entity, Pid(pid)) in &inserted {
        map.insert(entity, *pid);
    }
}

fn track_pid_removals(
    mut map: ResMut<PidToEntity>,
    mut removed: RemovedComponents<Pid>,
    survivors: Query<&Pid>,
) {
    for entity in removed.read() {
        if survivors.get(entity).is_err() {
            map.remove(entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(PidPlugin);
        app
    }

    #[test]
    fn pid_insert_populates_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn(Pid(7777)).id();
        app.update();
        let map = app.world().resource::<PidToEntity>();
        assert_eq!(map.get(7777), Some(e));
    }

    #[test]
    fn entity_despawn_removes_pid_from_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn(Pid(8888)).id();
        app.update();
        app.world_mut().despawn(e);
        app.update();
        let map = app.world().resource::<PidToEntity>();
        assert_eq!(map.get(8888), None);
    }

    #[test]
    fn changing_pid_updates_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn(Pid(9000)).id();
        app.update();
        app.world_mut().entity_mut(e).insert(Pid(9001));
        app.update();
        let map = app.world().resource::<PidToEntity>();
        assert_eq!(map.get(9001), Some(e));
        assert_eq!(map.get(9000), None);
    }
}
