use std::time::{Duration, Instant};

use bevy_app::{App, Plugin as BevyPlugin, PreUpdate};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::hierarchy::ChildOf;
use bevy_ecs::message::MessageWriter;
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Commands, Query};
use vmux_api::session::{RemoteEvent, SessionId};
use vmux_ecs::{Operation, OperationRetained, OperationState};
use vmux_session::host::Reported;

use crate::catalog::UpdateSessionItem;
use crate::connection::{
    ActiveConnectionHandle, ConnectionSource, MobileRoot, SourceEpoch, SourceUnauthorized,
};
use crate::io::{
    OpenStreamJob, OpenStreamReceipt, ReadStreamEventJob, ReadStreamEventReceipt,
    ResetConnectionJob, ResetConnectionReceipt, StreamCancel, StreamHandle, StreamReadOutcome,
};
use crate::remote::ApiError;
use crate::session::{SelectedSession, SessionGeneration, StreamConnected, StreamRestartPending};
use crate::{DecideSet, InvalidateSet, MaterializeSet};

pub(crate) struct MobileStreamPlugin;

impl BevyPlugin for MobileStreamPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, cancel_stale.in_set(InvalidateSet))
            .add_systems(
                PreUpdate,
                (adopt_reset, adopt_open, adopt_read)
                    .chain()
                    .in_set(MaterializeSet),
            )
            .add_systems(
                PreUpdate,
                (ensure_stream, advance_stream).chain().in_set(DecideSet),
            );
    }
}

#[derive(Component)]
pub(crate) struct StreamSession {
    source: Entity,
    source_epoch: u64,
    session_id: SessionId,
    generation: u64,
    needs_reset: bool,
}

#[derive(Component, Default)]
struct ActiveStreamHandle(Option<StreamHandle>);

#[derive(Component)]
pub(crate) struct StreamReconnectDeadline(pub(crate) Instant);

impl StreamReconnectDeadline {
    const DELAY: Duration = Duration::from_secs(2);
}

#[derive(Component)]
pub(crate) struct StreamTerminalRefusal;

#[derive(Component)]
struct StreamResetOp {
    owner: Entity,
    source_epoch: u64,
    generation: u64,
}

#[derive(Component)]
struct StreamOpenOp {
    owner: Entity,
    source_epoch: u64,
    generation: u64,
}

#[derive(Component)]
struct StreamReadOp {
    owner: Entity,
    source_epoch: u64,
    generation: u64,
}

fn cancel_stale(
    roots: Query<(&SessionGeneration, Option<&SelectedSession>), With<MobileRoot>>,
    sources: Query<(Entity, &SourceEpoch), With<ConnectionSource>>,
    mut streams: Query<(Entity, &StreamSession, &mut ActiveStreamHandle)>,
    reads: Query<(&StreamReadOp, &StreamCancel)>,
    mut commands: Commands,
) {
    let Ok((generation, selected)) = roots.single() else {
        return;
    };
    let Ok((active_source, active_epoch)) = sources.single() else {
        return;
    };
    for (entity, stream, mut handle) in &mut streams {
        let current = selected.is_some_and(|sel| sel.session_id == stream.session_id)
            && stream.source == active_source
            && stream.source_epoch == active_epoch.0
            && stream.generation == generation.0;
        if current {
            continue;
        }
        if let Some(mut open_handle) = handle.0.take() {
            open_handle.stop();
        }
        for (read, cancel) in &reads {
            if read.owner == entity {
                cancel.cancel();
            }
        }
        commands.entity(entity).despawn();
    }
}

#[allow(clippy::type_complexity)]
fn ensure_stream(
    roots: Query<
        (
            Entity,
            &SessionGeneration,
            Option<&SelectedSession>,
            Option<&StreamRestartPending>,
        ),
        With<MobileRoot>,
    >,
    sources: Query<(Entity, &SourceEpoch, &ActiveConnectionHandle), With<ConnectionSource>>,
    streams: Query<&StreamSession>,
    mut commands: Commands,
) {
    let Ok((root, generation, selected, restart_pending)) = roots.single() else {
        return;
    };
    let Some(selected) = selected else {
        return;
    };
    let Ok((source, epoch, active_handle)) = sources.single() else {
        return;
    };
    if active_handle.0.is_none() {
        return;
    }
    let already_exists = streams.iter().any(|stream| {
        stream.source == source
            && stream.source_epoch == epoch.0
            && stream.generation == generation.0
            && stream.session_id == selected.session_id
    });
    if already_exists {
        return;
    }
    let needs_reset = restart_pending.is_some();
    if needs_reset {
        commands.entity(root).remove::<StreamRestartPending>();
    }
    commands.spawn((
        ChildOf(root),
        StreamSession {
            source,
            source_epoch: epoch.0,
            session_id: selected.session_id.clone(),
            generation: generation.0,
            needs_reset,
        },
        ActiveStreamHandle::default(),
    ));
}

type AdvanceStreamQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static StreamSession,
        &'static mut ActiveStreamHandle,
        Option<&'static StreamReconnectDeadline>,
        Option<&'static StreamTerminalRefusal>,
    ),
>;

fn advance_stream(
    mut streams: AdvanceStreamQuery,
    sources: Query<(&SourceEpoch, &ActiveConnectionHandle), With<ConnectionSource>>,
    resets: Query<&StreamResetOp>,
    opens: Query<&StreamOpenOp>,
    reads: Query<&StreamReadOp>,
    mut commands: Commands,
) {
    let now = Instant::now();
    for (owner, stream, mut handle, deadline, terminal) in &mut streams {
        if terminal.is_some() {
            continue;
        }
        let Ok((epoch, active_handle)) = sources.get(stream.source) else {
            continue;
        };
        if epoch.0 != stream.source_epoch {
            continue;
        }
        let Some(conn_handle) = active_handle.0.clone() else {
            continue;
        };
        if stream.needs_reset {
            if resets.iter().any(|op| op.owner == owner) {
                continue;
            }
            commands.spawn((
                Operation::bundle("mobile.stream.reset").unwrap().running(),
                OperationRetained,
                StreamResetOp {
                    owner,
                    source_epoch: stream.source_epoch,
                    generation: stream.generation,
                },
                ResetConnectionJob {
                    handle: conn_handle,
                },
            ));
            continue;
        }
        if let Some(stream_handle) = handle.0.take() {
            if reads.iter().any(|op| op.owner == owner) {
                handle.0 = Some(stream_handle);
                continue;
            }
            let cancel = StreamCancel::new();
            commands.spawn((
                Operation::bundle("mobile.stream.read").unwrap().running(),
                OperationRetained,
                StreamReadOp {
                    owner,
                    source_epoch: stream.source_epoch,
                    generation: stream.generation,
                },
                cancel.clone(),
                ReadStreamEventJob {
                    stream: Some(stream_handle),
                    cancel,
                },
            ));
            continue;
        }
        if reads.iter().any(|op| op.owner == owner) || opens.iter().any(|op| op.owner == owner) {
            continue;
        }
        if let Some(wait_until) = deadline {
            if now < wait_until.0 {
                continue;
            }
            commands.entity(owner).remove::<StreamReconnectDeadline>();
        }
        commands.spawn((
            Operation::bundle("mobile.stream.open").unwrap().running(),
            OperationRetained,
            StreamOpenOp {
                owner,
                source_epoch: stream.source_epoch,
                generation: stream.generation,
            },
            OpenStreamJob {
                handle: conn_handle,
                sid: stream.session_id.clone(),
            },
        ));
    }
}

fn adopt_reset(
    ops: Query<(Entity, &StreamResetOp), With<ResetConnectionReceipt>>,
    mut streams: Query<&mut StreamSession>,
    mut commands: Commands,
) {
    for (entity, op) in &ops {
        commands
            .entity(entity)
            .remove::<ResetConnectionReceipt>()
            .remove::<StreamResetOp>()
            .remove::<OperationRetained>()
            .insert(OperationState::Succeeded);
        if let Ok(mut stream) = streams.get_mut(op.owner)
            && stream.source_epoch == op.source_epoch
            && stream.generation == op.generation
        {
            stream.needs_reset = false;
        }
    }
}

fn adopt_open(
    mut ops: Query<(Entity, &StreamOpenOp, &mut OpenStreamReceipt)>,
    mut streams: Query<(&StreamSession, &mut ActiveStreamHandle)>,
    mut roots: Query<(&SessionGeneration, &mut StreamConnected), With<MobileRoot>>,
    sources: Query<&SourceEpoch, With<ConnectionSource>>,
    mut unauthorized: MessageWriter<SourceUnauthorized>,
    mut commands: Commands,
) {
    for (entity, op, mut receipt) in &mut ops {
        commands
            .entity(entity)
            .remove::<OpenStreamReceipt>()
            .remove::<StreamOpenOp>()
            .remove::<OperationRetained>();
        let outcome = std::mem::replace(
            &mut receipt.0,
            Err(ApiError::Message("consumed".to_string())),
        );
        let Ok((stream, mut active_handle)) = streams.get_mut(op.owner) else {
            if let Ok(mut handle) = outcome {
                handle.stop();
            }
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        };
        let valid_source = sources
            .get(stream.source)
            .is_ok_and(|epoch| epoch.0 == op.source_epoch);
        let Ok((generation, mut connected)) = roots.single_mut() else {
            if let Ok(mut handle) = outcome {
                handle.stop();
            }
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        };
        if !valid_source || generation.0 != op.generation || stream.generation != op.generation {
            if let Ok(mut handle) = outcome {
                handle.stop();
            }
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        }
        match outcome {
            Ok(handle) => {
                connected.0 = true;
                active_handle.0 = Some(handle);
                commands.entity(entity).insert(OperationState::Succeeded);
            }
            Err(ApiError::Unauthorized) => {
                connected.0 = false;
                commands.entity(op.owner).insert(StreamTerminalRefusal);
                unauthorized.write(SourceUnauthorized {
                    source: stream.source,
                    epoch: op.source_epoch,
                });
                commands.entity(entity).insert(OperationState::Failed);
            }
            Err(ApiError::NotFound) => {
                connected.0 = false;
                commands.entity(op.owner).insert(StreamTerminalRefusal);
                commands.entity(entity).insert(OperationState::Failed);
            }
            Err(_) => {
                connected.0 = false;
                commands.entity(op.owner).insert(StreamReconnectDeadline(
                    Instant::now() + StreamReconnectDeadline::DELAY,
                ));
                commands.entity(entity).insert(OperationState::Failed);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn adopt_read(
    mut ops: Query<(Entity, &StreamReadOp, &mut ReadStreamEventReceipt)>,
    mut streams: Query<(&StreamSession, &mut ActiveStreamHandle)>,
    mut roots: Query<
        (
            &SessionGeneration,
            Option<&SelectedSession>,
            &mut StreamConnected,
        ),
        With<MobileRoot>,
    >,
    sources: Query<&SourceEpoch, With<ConnectionSource>>,
    mut reported: MessageWriter<Reported>,
    mut updates: MessageWriter<UpdateSessionItem>,
    mut unauthorized: MessageWriter<SourceUnauthorized>,
    mut commands: Commands,
) {
    for (entity, op, mut receipt) in &mut ops {
        commands
            .entity(entity)
            .remove::<ReadStreamEventReceipt>()
            .remove::<StreamReadOp>()
            .remove::<StreamCancel>()
            .remove::<OperationRetained>();
        let returned_stream = receipt.stream.take();
        let outcome = std::mem::replace(&mut receipt.outcome, StreamReadOutcome::Canceled);
        let Ok((stream, mut active_handle)) = streams.get_mut(op.owner) else {
            if let Some(mut handle) = returned_stream {
                handle.stop();
            }
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        };
        let valid_source = sources
            .get(stream.source)
            .is_ok_and(|epoch| epoch.0 == op.source_epoch);
        let Ok((generation, selected, mut connected)) = roots.single_mut() else {
            if let Some(mut handle) = returned_stream {
                handle.stop();
            }
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        };
        let matches_selection = selected.is_some_and(|sel| sel.session_id == stream.session_id)
            && valid_source
            && generation.0 == op.generation
            && stream.generation == op.generation;
        if !matches_selection {
            if let Some(mut handle) = returned_stream {
                handle.stop();
            }
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        }
        match outcome {
            StreamReadOutcome::Event(event) => {
                if let RemoteEvent::Session { session } = &event {
                    if session.id == stream.session_id {
                        updates.write(UpdateSessionItem {
                            source: stream.source,
                            epoch: op.source_epoch,
                            session: session.as_ref().clone(),
                        });
                        reported.write(Reported(event));
                    }
                } else {
                    reported.write(Reported(event));
                }
                active_handle.0 = returned_stream;
                commands.entity(entity).insert(OperationState::Succeeded);
            }
            StreamReadOutcome::Malformed => {
                tracing::warn!("session event frame malformed");
                active_handle.0 = returned_stream;
                commands.entity(entity).insert(OperationState::Succeeded);
            }
            StreamReadOutcome::Ended | StreamReadOutcome::Refused(ApiError::Message(_)) => {
                if let Some(mut handle) = returned_stream {
                    handle.stop();
                }
                connected.0 = false;
                commands.entity(op.owner).insert(StreamReconnectDeadline(
                    Instant::now() + StreamReconnectDeadline::DELAY,
                ));
                commands.entity(entity).insert(OperationState::Failed);
            }
            StreamReadOutcome::Refused(ApiError::Unauthorized) => {
                if let Some(mut handle) = returned_stream {
                    handle.stop();
                }
                connected.0 = false;
                commands.entity(op.owner).insert(StreamTerminalRefusal);
                unauthorized.write(SourceUnauthorized {
                    source: stream.source,
                    epoch: op.source_epoch,
                });
                commands.entity(entity).insert(OperationState::Failed);
            }
            StreamReadOutcome::Refused(ApiError::NotFound) => {
                if let Some(mut handle) = returned_stream {
                    handle.stop();
                }
                connected.0 = false;
                commands.entity(op.owner).insert(StreamTerminalRefusal);
                commands.entity(entity).insert(OperationState::Failed);
            }
            StreamReadOutcome::Canceled => {
                if let Some(mut handle) = returned_stream {
                    handle.stop();
                }
                commands.entity(entity).insert(OperationState::Canceled);
            }
        }
    }
}
