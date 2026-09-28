use std::collections::HashMap;

use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_api::protocol::{ClientMessage, ProcessId};
use vmux_core::service::ServiceConnected;
use vmux_core::service::ServiceRequest;

use super::plugin::{
    AwaitingProcessCreated, PendingServiceCreate, ServiceMessageSet, ShellOutputSeen,
    TerminalReinputRequest,
};
use crate::{ProcessExited, Terminal};

pub(super) struct InputQueuePlugin;

impl Plugin for InputQueuePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
            .add_message::<TerminalReinputRequest>()
            .add_message::<QueueTerminalInput>()
            .add_systems(
                Startup,
                (spawn_terminal_process_index, spawn_terminal_input_sequence),
            )
            .add_systems(PreUpdate, sync_terminal_process_index)
            .add_systems(Update, enqueue_terminal_reinput.after(ServiceMessageSet))
            .add_systems(
                PostUpdate,
                (
                    queue_terminal_input,
                    bevy::ecs::schedule::ApplyDeferred,
                    flush_terminal_input,
                )
                    .chain(),
            );
    }
}

#[derive(Component, Default, Debug)]
pub(crate) struct TerminalProcessIndex {
    by_process: HashMap<ProcessId, Entity>,
    by_entity: EntityHashMap<ProcessId>,
}

impl TerminalProcessIndex {
    pub fn get(&self, process_id: &ProcessId) -> Option<Entity> {
        self.by_process.get(process_id).copied()
    }
}

#[derive(Component, Default)]
struct NextTerminalInputSequence(u64);

#[derive(Message)]
pub(crate) struct QueueTerminalInput {
    pub terminal: Entity,
    pub data: Vec<u8>,
}

#[derive(Component)]
#[relationship(relationship_target = TerminalInputs)]
pub(crate) struct TerminalInputTarget {
    #[relationship]
    terminal: Entity,
}

#[derive(Component)]
#[relationship_target(relationship = TerminalInputTarget)]
pub(crate) struct TerminalInputs(Vec<Entity>);

#[derive(Component)]
pub(crate) struct TerminalInput {
    sequence: u64,
    data: Vec<u8>,
}

fn spawn_terminal_process_index(mut commands: Commands) {
    commands.spawn((
        Name::new("Terminal process index"),
        TerminalProcessIndex::default(),
    ));
}

fn sync_terminal_process_index(
    mut index: Single<&mut TerminalProcessIndex>,
    changed: Query<
        (Entity, &ProcessId),
        (With<Terminal>, Or<(Changed<ProcessId>, Added<Terminal>)>),
    >,
    mut removed_process_ids: RemovedComponents<ProcessId>,
    mut removed_terminals: RemovedComponents<Terminal>,
) {
    for entity in removed_process_ids.read() {
        let Some(process_id) = index.by_entity.remove(&entity) else {
            continue;
        };
        if index.by_process.get(&process_id) == Some(&entity) {
            index.by_process.remove(&process_id);
        }
    }
    for entity in removed_terminals.read() {
        let Some(process_id) = index.by_entity.remove(&entity) else {
            continue;
        };
        if index.by_process.get(&process_id) == Some(&entity) {
            index.by_process.remove(&process_id);
        }
    }
    for (entity, process_id) in &changed {
        if let Some(previous_process_id) = index.by_entity.insert(entity, *process_id) {
            index.by_process.remove(&previous_process_id);
        }
        if let Some(previous_entity) = index.by_process.insert(*process_id, entity)
            && previous_entity != entity
        {
            index.by_entity.remove(&previous_entity);
        }
    }
}

fn spawn_terminal_input_sequence(mut commands: Commands) {
    commands.spawn((
        Name::new("Terminal input sequence"),
        NextTerminalInputSequence::default(),
    ));
}

fn queue_terminal_input(
    mut requests: MessageReader<QueueTerminalInput>,
    mut next: Single<&mut NextTerminalInputSequence>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let sequence = next.0;
        next.0 = next.0.wrapping_add(1);
        commands.spawn((
            TerminalInput {
                sequence,
                data: request.data.clone(),
            },
            TerminalInputTarget {
                terminal: request.terminal,
            },
        ));
    }
}

#[cfg(test)]
pub(crate) fn pending_terminal_input(world: &mut World, terminal: Entity) -> Vec<Vec<u8>> {
    let mut query = world.query::<(&TerminalInput, &TerminalInputTarget)>();
    let mut pending = query
        .iter(world)
        .filter(|(_, target)| target.get() == terminal)
        .map(|(input, _)| (input.sequence, input.data.clone()))
        .collect::<Vec<_>>();
    pending.sort_by_key(|(sequence, _)| *sequence);
    pending.into_iter().map(|(_, data)| data).collect()
}

fn enqueue_terminal_reinput(
    mut requests: MessageReader<TerminalReinputRequest>,
    process_index: Single<&TerminalProcessIndex>,
    terminals: Query<(), With<Terminal>>,
    mut terminal_inputs: MessageWriter<QueueTerminalInput>,
) {
    for request in requests.read() {
        let Some(terminal) = process_index.get(&request.process_id) else {
            continue;
        };
        if !terminals.contains(terminal) {
            continue;
        }
        terminal_inputs.write(QueueTerminalInput {
            terminal,
            data: request.data.clone(),
        });
    }
}

fn flush_terminal_input(
    inputs: Query<(Entity, &TerminalInput, &TerminalInputTarget)>,
    terminals: Query<
        (
            &vmux_api::protocol::ProcessId,
            Has<ShellOutputSeen>,
            Has<PendingServiceCreate>,
            Has<AwaitingProcessCreated>,
            Has<ProcessExited>,
        ),
        With<Terminal>,
    >,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    if connected.is_none() {
        return;
    }
    let mut pending = inputs.iter().collect::<Vec<_>>();
    pending.sort_by_key(|(_, input, _)| input.sequence);

    for (entity, input, target) in pending {
        let Ok((process_id, shell_output_seen, creating, restarting, exited)) =
            terminals.get(target.get())
        else {
            commands.entity(entity).despawn();
            continue;
        };
        if !shell_output_seen || creating || restarting || exited {
            continue;
        }
        service_requests.write(ServiceRequest(ClientMessage::ProcessInput {
            process_id: *process_id,
            data: input.data.clone(),
        }));
        commands.entity(entity).despawn();
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
        app.add_plugins(InputQueuePlugin);
        app
    }

    fn indexed_entity(app: &mut App, process_id: &ProcessId) -> Option<Entity> {
        let mut query = app.world_mut().query::<&TerminalProcessIndex>();
        query.single(app.world()).unwrap().get(process_id)
    }

    #[test]
    fn indexes_terminal_processes() {
        let mut app = app();
        let process_id = process_id(1);
        let entity = app.world_mut().spawn((Terminal, process_id)).id();

        app.update();

        assert_eq!(indexed_entity(&mut app, &process_id), Some(entity));
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

        assert_eq!(indexed_entity(&mut app, &previous), None);
        assert_eq!(indexed_entity(&mut app, &current), Some(entity));
    }

    #[test]
    fn removes_despawned_terminals() {
        let mut app = app();
        let process_id = process_id(1);
        let entity = app.world_mut().spawn((Terminal, process_id)).id();
        app.update();

        app.world_mut().entity_mut(entity).despawn();
        app.update();

        assert_eq!(indexed_entity(&mut app, &process_id), None);
    }
}
