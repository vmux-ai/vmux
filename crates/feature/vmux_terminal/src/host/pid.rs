use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use std::collections::HashMap;

pub struct PidPlugin;

impl Plugin for PidPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_index).add_systems(
            Update,
            (track_inserts, track_removals).chain().in_set(PidIndexSet),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PidIndexSet;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub u32);

impl Pid {
    pub fn page_url(&self) -> String {
        format!("{}{}", crate::TerminalPlugin::URL, self.0)
    }
}

#[derive(Component, Default, Debug)]
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
}

impl FromIterator<(u32, Entity)> for PidToEntity {
    fn from_iter<T: IntoIterator<Item = (u32, Entity)>>(iter: T) -> Self {
        let mut index = Self::default();
        for (pid, entity) in iter {
            index.by_pid.insert(pid, entity);
            index.by_entity.insert(entity, pid);
        }
        index
    }
}

fn spawn_index(mut commands: Commands) {
    commands.spawn((Name::new("Terminal PID index"), PidToEntity::default()));
}

fn track_inserts(mut map: Single<&mut PidToEntity>, inserted: Query<(Entity, &Pid), Changed<Pid>>) {
    for (entity, Pid(pid)) in &inserted {
        if let Some(previous_pid) = map.by_entity.insert(entity, *pid) {
            map.by_pid.remove(&previous_pid);
        }
        if let Some(previous_entity) = map.by_pid.insert(*pid, entity)
            && previous_entity != entity
        {
            map.by_entity.remove(&previous_entity);
        }
    }
}

fn track_removals(
    mut map: Single<&mut PidToEntity>,
    mut removed: RemovedComponents<Pid>,
    survivors: Query<&Pid>,
) {
    for entity in removed.read() {
        if survivors.get(entity).is_ok() {
            continue;
        }
        let Some(pid) = map.by_entity.remove(&entity) else {
            continue;
        };
        if map.by_pid.get(&pid) == Some(&entity) {
            map.by_pid.remove(&pid);
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
        let mut query = app.world_mut().query::<&PidToEntity>();
        let map = query.single(app.world()).unwrap();
        assert_eq!(map.get(7777), Some(e));
    }

    #[test]
    fn entity_despawn_removes_pid_from_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn(Pid(8888)).id();
        app.update();
        app.world_mut().despawn(e);
        app.update();
        let mut query = app.world_mut().query::<&PidToEntity>();
        let map = query.single(app.world()).unwrap();
        assert_eq!(map.get(8888), None);
    }

    #[test]
    fn changing_pid_updates_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn(Pid(9000)).id();
        app.update();
        app.world_mut().entity_mut(e).insert(Pid(9001));
        app.update();
        let mut query = app.world_mut().query::<&PidToEntity>();
        let map = query.single(app.world()).unwrap();
        assert_eq!(map.get(9001), Some(e));
        assert_eq!(map.get(9000), None);
    }
}
