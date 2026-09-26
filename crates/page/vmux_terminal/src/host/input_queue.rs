use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::ClientMessage;

use super::plugin::{
    AwaitingProcessCreated, PendingServiceCreate, ServiceMessageSet, ShellOutputSeen,
    TerminalReinputRequest,
};
use super::process_index::TerminalProcessIndex;
use crate::{ProcessExited, Terminal};

pub(super) struct InputQueuePlugin;

impl Plugin for InputQueuePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<super::process_index::TerminalProcessIndexPlugin>() {
            app.add_plugins(super::process_index::TerminalProcessIndexPlugin);
        }
        app.init_resource::<NextTerminalInputSequence>()
            .add_message::<TerminalReinputRequest>()
            .add_systems(
                Update,
                (
                    enqueue_terminal_reinput,
                    bevy::ecs::schedule::ApplyDeferred,
                    flush_terminal_input,
                )
                    .chain()
                    .after(ServiceMessageSet),
            );
    }
}

#[derive(Resource, Default)]
pub(crate) struct NextTerminalInputSequence(u64);

impl NextTerminalInputSequence {
    fn take(&mut self) -> u64 {
        let sequence = self.0;
        self.0 = self.0.wrapping_add(1);
        sequence
    }
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

pub(crate) fn enqueue_terminal_input(
    commands: &mut Commands,
    sequence: &mut NextTerminalInputSequence,
    terminal: Entity,
    data: Vec<u8>,
) {
    commands.spawn((
        TerminalInput {
            sequence: sequence.take(),
            data,
        },
        TerminalInputTarget { terminal },
    ));
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
    process_index: Res<TerminalProcessIndex>,
    terminals: Query<(), With<Terminal>>,
    mut sequence: ResMut<NextTerminalInputSequence>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Some(terminal) = process_index.get(&request.process_id) else {
            continue;
        };
        if !terminals.contains(terminal) {
            continue;
        }
        enqueue_terminal_input(&mut commands, &mut sequence, terminal, request.data.clone());
    }
}

fn flush_terminal_input(
    inputs: Query<(Entity, &TerminalInput, &TerminalInputTarget)>,
    terminals: Query<
        (
            &vmux_service::protocol::ProcessId,
            Has<ShellOutputSeen>,
            Has<PendingServiceCreate>,
            Has<AwaitingProcessCreated>,
            Has<ProcessExited>,
        ),
        With<Terminal>,
    >,
    service: Option<Single<&ServiceClient>>,
    mut commands: Commands,
) {
    let Some(service) = service else { return };
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
        service.0.send(ClientMessage::ProcessInput {
            process_id: *process_id,
            data: input.data.clone(),
        });
        commands.entity(entity).despawn();
    }
}
