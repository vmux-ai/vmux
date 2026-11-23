use std::time::{Duration, Instant};

use bevy_app::{App, Plugin as BevyPlugin, PreUpdate};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::hierarchy::ChildOf;
use bevy_ecs::message::{Message, MessageReader, MessageWriter};
use bevy_ecs::name::Name;
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Commands, Query, SystemParam};
use vmux_api::session::SessionId;
use vmux_ecs::{
    CreatedAt, Cwd, Operation, OperationError, OperationRetained, OperationState, Order,
};
use vmux_session::host::{ChatRuntime, Conversation, LiveTurn, Log};
use vmux_ui::i18n::translate;

use crate::catalog::{ReconcileSessions, RemoteSessionDetails, RemoteSessionItem};
use crate::connection::{
    ActiveConnectionHandle, ClientOperationSequence, ConnectionNotice, ConnectionSource,
    MobileRoot, SourceEpoch, SourceInvalidated, SourceUnauthorized,
};
use crate::io::{CreateChatJob, CreateChatReceipt, ReadSessionsJob, ReadSessionsReceipt};
use crate::remote::ApiError;
use crate::{AdmitSet, DecideSet, InvalidateSet, MaterializeSet};

pub(crate) struct MobileSessionPlugin;

impl BevyPlugin for MobileSessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenSession>()
            .add_message::<LeaveSession>()
            .add_message::<RestartSession>()
            .add_message::<OpenTeam>()
            .add_message::<CloseTeam>()
            .add_message::<LeaveScreen>()
            .add_message::<SwitchTabIntent>()
            .add_message::<StartChatRequest>()
            .add_systems(
                PreUpdate,
                (
                    admit_switch_tab,
                    admit_team,
                    admit_leave_screen,
                    admit_start_chat,
                )
                    .chain()
                    .in_set(AdmitSet),
            )
            .add_systems(
                PreUpdate,
                (leave, open, restart).chain().in_set(InvalidateSet),
            )
            .add_systems(
                PreUpdate,
                (adopt_create_chat, adopt_start_chat_poll)
                    .chain()
                    .in_set(MaterializeSet),
            )
            .add_systems(PreUpdate, poll_created.in_set(DecideSet));
    }
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum MobileRoute {
    #[default]
    Start,
    Session,
    Team,
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub(crate) struct SelectedSession {
    pub(crate) entity: Entity,
    pub(crate) session_id: SessionId,
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SessionGeneration(pub(crate) u64);

impl SessionGeneration {
    pub(crate) fn advance(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }
}

#[derive(Component, Default)]
pub(crate) struct StreamConnected(pub(crate) bool);

#[derive(Component)]
pub(crate) struct StreamRestartPending;

#[derive(Message)]
pub(crate) struct OpenSession(pub(crate) SessionId);

#[derive(Message)]
pub(crate) struct LeaveSession;

#[derive(Message)]
pub(crate) struct RestartSession;

#[derive(Message)]
pub(crate) struct OpenTeam;

#[derive(Message)]
pub(crate) struct CloseTeam;

#[derive(Message)]
pub(crate) struct LeaveScreen;

#[derive(Message)]
pub(crate) struct SwitchTabIntent {
    pub(crate) index: usize,
}

#[derive(Message)]
pub(crate) struct StartChatRequest {
    pub(crate) text: String,
    pub(crate) agent_url: Option<String>,
}

#[derive(Component)]
struct StartChatOperation {
    source: Entity,
    epoch: u64,
    created_session_id: Option<SessionId>,
    attempts_remaining: u8,
    next_poll_at: Instant,
    polling: bool,
}

impl StartChatOperation {
    const POLL_ATTEMPTS: u8 = 40;
    const POLL_INTERVAL: Duration = Duration::from_millis(250);
}

#[derive(SystemParam)]
struct StartChatCommands<'w, 's> {
    commands: Commands<'w, 's>,
}

impl StartChatCommands<'_, '_> {
    fn finish(&mut self, entity: Entity, state: OperationState, error: Option<String>) {
        let mut entity_cmd = self.commands.entity(entity);
        entity_cmd
            .remove::<StartChatOperation>()
            .remove::<OperationRetained>()
            .insert(state);
        if let Some(message) = error
            && let Ok(op_error) = OperationError::new(message)
        {
            entity_cmd.insert(op_error);
        }
    }
}

fn admit_switch_tab(
    mut requests: MessageReader<SwitchTabIntent>,
    sources: Query<Entity, With<ConnectionSource>>,
    sessions: Query<(&ChildOf, &Order, &SessionId), With<RemoteSessionItem>>,
    mut openings: MessageWriter<OpenSession>,
) {
    let Ok(source) = sources.single() else {
        return;
    };
    for request in requests.read() {
        let Ok(target_order) = u32::try_from(request.index) else {
            continue;
        };
        for (parent, order, sid) in &sessions {
            if parent.parent() == source && order.0 == target_order {
                openings.write(OpenSession(sid.clone()));
                break;
            }
        }
    }
}

fn admit_team(
    mut open_team: MessageReader<OpenTeam>,
    mut close_team: MessageReader<CloseTeam>,
    mut roots: Query<(&mut MobileRoute, Option<&SelectedSession>), With<MobileRoot>>,
) {
    let Ok((mut route, selected)) = roots.single_mut() else {
        return;
    };
    if open_team.read().count() > 0 {
        *route = MobileRoute::Team;
    }
    if close_team.read().count() > 0 && *route == MobileRoute::Team {
        *route = if selected.is_some() {
            MobileRoute::Session
        } else {
            MobileRoute::Start
        };
    }
}

fn admit_leave_screen(
    mut requests: MessageReader<LeaveScreen>,
    mut roots: Query<&mut MobileRoute, With<MobileRoot>>,
    mut leaves: MessageWriter<LeaveSession>,
) {
    if requests.read().count() == 0 {
        return;
    }
    let Ok(mut route) = roots.single_mut() else {
        return;
    };
    *route = MobileRoute::Start;
    leaves.write(LeaveSession);
}

fn admit_start_chat(
    mut requests: MessageReader<StartChatRequest>,
    mut roots: Query<&mut ClientOperationSequence, With<MobileRoot>>,
    sources: Query<(Entity, &SourceEpoch, &ActiveConnectionHandle), With<ConnectionSource>>,
    mut commands: Commands,
) {
    let Ok(mut sequence) = roots.single_mut() else {
        return;
    };
    let Ok((source, epoch, active_handle)) = sources.single() else {
        return;
    };
    let Some(handle) = active_handle.0.clone() else {
        return;
    };
    for request in requests.read() {
        let text = request.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let client_op_id = sequence.next();
        commands.spawn((
            Operation::bundle("mobile.session.create")
                .unwrap()
                .running(),
            OperationRetained,
            StartChatOperation {
                source,
                epoch: epoch.0,
                created_session_id: None,
                attempts_remaining: StartChatOperation::POLL_ATTEMPTS,
                next_poll_at: Instant::now(),
                polling: false,
            },
            CreateChatJob {
                handle: handle.clone(),
                client_op_id,
                text,
                agent_url: request.agent_url.clone(),
            },
        ));
    }
}

type SessionCatalogLookup<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static SessionId,
        &'static Name,
        &'static Cwd,
        &'static CreatedAt,
        &'static RemoteSessionDetails,
    ),
    With<RemoteSessionItem>,
>;

fn open(
    mut requests: MessageReader<OpenSession>,
    mut roots: Query<
        (
            Entity,
            &mut MobileRoute,
            &mut SessionGeneration,
            &mut StreamConnected,
        ),
        With<MobileRoot>,
    >,
    sources: Query<(Entity, &ActiveConnectionHandle), With<ConnectionSource>>,
    catalog: SessionCatalogLookup,
    mut chats: Query<(&mut Conversation, &mut Log, &mut LiveTurn), With<ChatRuntime>>,
    mut commands: Commands,
) {
    let Ok((root, mut route, mut generation, mut connected)) = roots.single_mut() else {
        return;
    };
    let Ok((source, active_handle)) = sources.single() else {
        return;
    };
    if active_handle.0.is_none() {
        return;
    }
    let Ok((mut conversation, mut log, mut live)) = chats.single_mut() else {
        return;
    };
    for request in requests.read() {
        let target_sid = &request.0;
        let mut matched = None;
        for (entity, parent, id, name, cwd, created, details) in &catalog {
            if parent.parent() == source && id == target_sid {
                matched = Some((entity, details.to_wire(id, name, cwd, created)));
                break;
            }
        }
        let Some((session_entity, wire)) = matched else {
            continue;
        };
        request.open_sheet();
        generation.advance();
        connected.0 = false;
        *route = MobileRoute::Session;
        commands
            .entity(root)
            .remove::<StreamRestartPending>()
            .insert(SelectedSession {
                entity: session_entity,
                session_id: target_sid.clone(),
            });
        *log = Log {
            session_id: Some(target_sid.clone()),
            ..Log::default()
        };
        *live = LiveTurn::default();
        *conversation = Conversation {
            status: wire.status.clone(),
            approval: wire.approval.clone(),
            session: Some(wire),
        };
    }
}

#[allow(clippy::type_complexity)]
fn leave(
    mut requests: MessageReader<LeaveSession>,
    mut invalidated: MessageReader<SourceInvalidated>,
    mut roots: Query<
        (
            Entity,
            &mut MobileRoute,
            &mut SessionGeneration,
            &mut StreamConnected,
            Option<&SelectedSession>,
        ),
        With<MobileRoot>,
    >,
    mut chats: Query<(&mut Conversation, &mut Log, &mut LiveTurn), With<ChatRuntime>>,
    mut commands: Commands,
) {
    let should_leave = requests.read().count() > 0 || invalidated.read().count() > 0;
    if !should_leave {
        return;
    }
    let Ok((root, mut route, mut generation, mut connected, selected)) = roots.single_mut() else {
        return;
    };
    let had_selected = selected.is_some();
    generation.advance();
    connected.0 = false;
    *route = MobileRoute::Start;
    commands
        .entity(root)
        .remove::<SelectedSession>()
        .remove::<StreamRestartPending>();
    if let Ok((mut conversation, mut log, mut live)) = chats.single_mut() {
        *conversation = Conversation::default();
        *log = Log::default();
        *live = LiveTurn::default();
    }
    if had_selected {
        LeaveSession.close_sheet();
    }
}

fn restart(
    mut requests: MessageReader<RestartSession>,
    mut roots: Query<
        (
            Entity,
            &mut SessionGeneration,
            &mut StreamConnected,
            Option<&SelectedSession>,
        ),
        With<MobileRoot>,
    >,
    sources: Query<&ActiveConnectionHandle, With<ConnectionSource>>,
    mut commands: Commands,
) {
    if requests.read().count() == 0 {
        return;
    }
    let Ok((root, mut generation, mut connected, selected)) = roots.single_mut() else {
        return;
    };
    if selected.is_none() {
        return;
    }
    let Ok(active_handle) = sources.single() else {
        return;
    };
    if active_handle.0.is_none() {
        return;
    }
    generation.advance();
    connected.0 = false;
    commands.entity(root).insert(StreamRestartPending);
}

fn adopt_create_chat(
    mut ops: Query<(Entity, &mut StartChatOperation, &CreateChatReceipt)>,
    mut sources: Query<(&SourceEpoch, &mut ConnectionNotice), With<ConnectionSource>>,
    catalog: Query<(&ChildOf, &SessionId), With<RemoteSessionItem>>,
    mut openings: MessageWriter<OpenSession>,
    mut unauthorized: MessageWriter<SourceUnauthorized>,
    mut ops_cmd: StartChatCommands,
) {
    for (entity, mut op, receipt) in &mut ops {
        ops_cmd
            .commands
            .entity(entity)
            .remove::<CreateChatReceipt>();
        let Ok((epoch, mut notice)) = sources.get_mut(op.source) else {
            ops_cmd.finish(entity, OperationState::Canceled, None);
            continue;
        };
        if epoch.0 != op.epoch {
            ops_cmd.finish(entity, OperationState::Canceled, None);
            continue;
        }
        match &receipt.0 {
            Ok(created_id) => {
                let exists = catalog
                    .iter()
                    .any(|(parent, sid)| parent.parent() == op.source && sid == created_id);
                if exists {
                    openings.write(OpenSession(created_id.clone()));
                    ops_cmd.finish(entity, OperationState::Succeeded, None);
                } else {
                    op.created_session_id = Some(created_id.clone());
                    op.next_poll_at = Instant::now();
                }
            }
            Err(ApiError::Unauthorized) => {
                unauthorized.write(SourceUnauthorized {
                    source: op.source,
                    epoch: op.epoch,
                });
                ops_cmd.finish(entity, OperationState::Failed, None);
            }
            Err(error) => {
                let message = error.to_string();
                notice.0 = message.clone();
                ops_cmd.finish(entity, OperationState::Failed, Some(message));
            }
        }
    }
}

fn poll_created(
    mut ops: Query<(Entity, &mut StartChatOperation)>,
    sources: Query<(&SourceEpoch, &ActiveConnectionHandle), With<ConnectionSource>>,
    mut ops_cmd: StartChatCommands,
) {
    let now = Instant::now();
    for (entity, mut op) in &mut ops {
        let Some(_) = op.created_session_id.as_ref() else {
            continue;
        };
        if op.polling || now < op.next_poll_at {
            continue;
        }
        let Ok((epoch, active_handle)) = sources.get(op.source) else {
            ops_cmd.finish(entity, OperationState::Canceled, None);
            continue;
        };
        if epoch.0 != op.epoch {
            ops_cmd.finish(entity, OperationState::Canceled, None);
            continue;
        }
        let Some(handle) = active_handle.0.clone() else {
            ops_cmd.finish(entity, OperationState::Canceled, None);
            continue;
        };
        if op.attempts_remaining == 0 {
            let message = translate("mobile-error-stack-missing");
            ops_cmd.finish(entity, OperationState::Failed, Some(message));
            continue;
        }
        op.attempts_remaining -= 1;
        op.polling = true;
        ops_cmd
            .commands
            .entity(entity)
            .insert(ReadSessionsJob { handle });
    }
}

fn adopt_start_chat_poll(
    mut ops: Query<(Entity, &mut StartChatOperation, &ReadSessionsReceipt)>,
    mut sources: Query<(&SourceEpoch, &mut ConnectionNotice), With<ConnectionSource>>,
    mut reconcile: MessageWriter<ReconcileSessions>,
    mut openings: MessageWriter<OpenSession>,
    mut unauthorized: MessageWriter<SourceUnauthorized>,
    mut ops_cmd: StartChatCommands,
) {
    for (entity, mut op, receipt) in &mut ops {
        ops_cmd
            .commands
            .entity(entity)
            .remove::<ReadSessionsReceipt>();
        op.polling = false;
        let Ok((epoch, mut notice)) = sources.get_mut(op.source) else {
            ops_cmd.finish(entity, OperationState::Canceled, None);
            continue;
        };
        if epoch.0 != op.epoch {
            ops_cmd.finish(entity, OperationState::Canceled, None);
            continue;
        }
        let Some(target_id) = op.created_session_id.clone() else {
            ops_cmd.finish(entity, OperationState::Canceled, None);
            continue;
        };
        match &receipt.0 {
            Ok(sessions) => {
                reconcile.write(ReconcileSessions {
                    source: op.source,
                    epoch: op.epoch,
                    sessions: sessions.clone(),
                });
                let found = sessions.iter().any(|candidate| candidate.id == target_id);
                if found {
                    openings.write(OpenSession(target_id));
                    ops_cmd.finish(entity, OperationState::Succeeded, None);
                } else if op.attempts_remaining == 0 {
                    let message = translate("mobile-error-stack-missing");
                    notice.0 = message.clone();
                    ops_cmd.finish(entity, OperationState::Failed, Some(message));
                } else {
                    op.next_poll_at = Instant::now() + StartChatOperation::POLL_INTERVAL;
                }
            }
            Err(ApiError::Unauthorized) => {
                unauthorized.write(SourceUnauthorized {
                    source: op.source,
                    epoch: op.epoch,
                });
                ops_cmd.finish(entity, OperationState::Failed, None);
            }
            Err(_) => {
                if op.attempts_remaining == 0 {
                    let message = translate("mobile-error-stack-missing");
                    notice.0 = message.clone();
                    ops_cmd.finish(entity, OperationState::Failed, Some(message));
                } else {
                    op.next_poll_at = Instant::now() + StartChatOperation::POLL_INTERVAL;
                }
            }
        }
    }
}
