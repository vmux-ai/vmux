use std::collections::HashMap;

use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use vmux_core::ProcessId;

use crate::Terminal;

pub struct TerminalProcessIndexPlugin;

impl Plugin for TerminalProcessIndexPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerminalProcessIndex>()
            .add_systems(PreUpdate, sync_terminal_process_index);
    }
}

#[derive(Resource, Default, Debug)]
pub(crate) struct TerminalProcessIndex {
    by_process: HashMap<ProcessId, Entity>,
    by_entity: EntityHashMap<ProcessId>,
}

impl TerminalProcessIndex {
    pub fn get(&self, process_id: &ProcessId) -> Option<Entity> {
        self.by_process.get(process_id).copied()
    }

    fn insert(&mut self, entity: Entity, process_id: ProcessId) {
        if let Some(previous_process_id) = self.by_entity.insert(entity, process_id) {
            self.by_process.remove(&previous_process_id);
        }
        if let Some(previous_entity) = self.by_process.insert(process_id, entity)
            && previous_entity != entity
        {
            self.by_entity.remove(&previous_entity);
        }
    }

    fn remove(&mut self, entity: Entity) {
        let Some(process_id) = self.by_entity.remove(&entity) else {
            return;
        };
        if self.by_process.get(&process_id) == Some(&entity) {
            self.by_process.remove(&process_id);
        }
    }
}

fn sync_terminal_process_index(
    mut index: ResMut<TerminalProcessIndex>,
    changed: Query<
        (Entity, &ProcessId),
        (With<Terminal>, Or<(Changed<ProcessId>, Added<Terminal>)>),
    >,
    mut removed_process_ids: RemovedComponents<ProcessId>,
    mut removed_terminals: RemovedComponents<Terminal>,
) {
    for entity in removed_process_ids.read() {
        index.remove(entity);
    }
    for entity in removed_terminals.read() {
        index.remove(entity);
    }
    for (entity, process_id) in &changed {
        index.insert(entity, *process_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_id(byte: u8) -> ProcessId {
        ProcessId([byte; 16])
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(TerminalProcessIndexPlugin);
        app
    }

    #[test]
    fn indexes_terminal_processes() {
        let mut app = app();
        let process_id = process_id(1);
        let entity = app.world_mut().spawn((Terminal, process_id)).id();

        app.update();

        assert_eq!(
            app.world()
                .resource::<TerminalProcessIndex>()
                .get(&process_id),
            Some(entity)
        );
    }

    #[test]
    fn replaces_changed_process_ids() {
        let mut app = app();
        let previous = process_id(1);
        let current = process_id(2);
        let entity = app.world_mut().spawn((Terminal, previous)).id();
        app.update();

        app.world_mut().entity_mut(entity).insert(current);
        app.update();

        let index = app.world().resource::<TerminalProcessIndex>();
        assert_eq!(index.get(&previous), None);
        assert_eq!(index.get(&current), Some(entity));
    }

    #[test]
    fn removes_despawned_terminals() {
        let mut app = app();
        let process_id = process_id(1);
        let entity = app.world_mut().spawn((Terminal, process_id)).id();
        app.update();

        app.world_mut().entity_mut(entity).despawn();
        app.update();

        assert_eq!(
            app.world()
                .resource::<TerminalProcessIndex>()
                .get(&process_id),
            None
        );
    }
}
