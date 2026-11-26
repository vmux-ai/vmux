use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use tokio::sync::{mpsc, oneshot};
use vmux_api::protocol::{
    AgentListAgents, AgentListTeam, SharedFailure, SharedMessage, SharedResponse,
};
use vmux_ecs::service::{Shutdown, ShutdownComplete, ShutdownPending};
use vmux_ecs::{
    EntityTarget, Operation, OperationCompleted, OperationRetained, OperationState,
    OperationTarget, TokioTask, Uuid,
};
use vmux_session::SessionId;

use super::remote_broker::{Broker, NeedsClaim, RemoteBrokerPlugin};
use super::remote_io::{Accepted, Joined, Loan, RemoteIoPlugin};
use super::remote_media::{Approve, Attach, Cancel, Media, Prompt, RemoteMediaPlugin, Sessions};
use super::service_driver::AcpService;

pub(super) struct RemoteRequestPlugin;

impl Plugin for RemoteRequestPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.add_plugins((RemoteIoPlugin, RemoteBrokerPlugin, RemoteMediaPlugin))
            .configure_sets(
                Update,
                (
                    RemoteSet::Receive,
                    RemoteSet::Retire,
                    RemoteSet::Collect,
                    RemoteSet::Submit,
                    RemoteSet::Clean,
                    RemoteSet::Finish,
                    RemoteSet::Poll,
                )
                    .chain()
                    .after(vmux_ecs::service::Register),
            )
            .add_systems(
                Update,
                (receive, ApplyDeferred).chain().in_set(RemoteSet::Receive),
            )
            .add_systems(Update, retire.in_set(RemoteSet::Retire))
            .add_systems(
                Update,
                (finish, release_retention, acknowledge, ApplyDeferred)
                    .chain()
                    .in_set(RemoteSet::Finish),
            )
            .add_systems(Shutdown, stop)
            .add_systems(ShutdownComplete, drain);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum RemoteSet {
    Receive,
    Retire,
    Collect,
    Submit,
    Clean,
    Finish,
    Poll,
}

pub(super) struct Received {
    pub request: SharedMessage,
    pub reply: oneshot::Sender<SharedResponse>,
}

#[derive(Component)]
pub(super) struct Inbox(pub mpsc::UnboundedReceiver<Received>);

#[derive(Component)]
struct Reply(Option<oneshot::Sender<SharedResponse>>);

#[derive(Component)]
pub(super) struct Waiting(pub Entity);

#[derive(Component)]
pub(super) struct Response(pub SharedResponse);

#[derive(Component)]
struct Finished;

#[derive(Component)]
struct Stopping;

#[derive(Component)]
struct RemoteDrain(Option<oneshot::Sender<()>>);

#[derive(SystemParam)]
pub(super) struct Live<'w, 's> {
    operations: Query<
        'w,
        's,
        (
            &'static Uuid,
            &'static OperationState,
            &'static EntityTarget<AcpService>,
            &'static Waiting,
        ),
    >,
    services: Query<'w, 's, (), (With<AcpService>, Without<Stopping>)>,
}

impl Live<'_, '_> {
    pub(super) fn matching(
        &self,
        native: Entity,
        accepted: &Accepted,
        target: Option<&OperationTarget>,
    ) -> Option<Entity> {
        let target = target?.entity();
        let Ok((uuid, _, owner, waiting)) = self.operations.get(target) else {
            return None;
        };
        if uuid != &accepted.uuid || owner.entity() != accepted.owner || waiting.0 != native {
            return None;
        }
        Some(target)
    }

    pub(super) fn accepts(
        &self,
        native: Entity,
        accepted: &Accepted,
        target: Option<&OperationTarget>,
    ) -> Option<Entity> {
        let target = self.matching(native, accepted, target)?;
        let (_, state, owner, _) = self.operations.get(target).ok()?;
        if !matches!(state, OperationState::Pending | OperationState::Running)
            || !self.services.contains(owner.entity())
        {
            return None;
        }
        Some(target)
    }
}

fn receive(
    mut inboxes: Query<
        (
            Entity,
            &mut Inbox,
            &vmux_ecs::service::Executor,
            &vmux_ecs::service::Wake,
        ),
        Without<Stopping>,
    >,
    mut commands: Commands,
) {
    for (owner, mut inbox, executor, wake) in &mut inboxes {
        while let Ok(input) = inbox.0.try_recv() {
            if input.reply.is_closed() {
                continue;
            }
            let kind = match &input.request {
                SharedMessage::AgentNewChat { .. } => "remote.new-chat",
                SharedMessage::AgentListModels { .. } => "remote.list-models",
                SharedMessage::AgentSelectModel { .. } => "remote.select-model",
                SharedMessage::AgentSetEffort { .. } => "remote.set-effort",
                SharedMessage::AgentListAgents => "remote.list-agents",
                SharedMessage::AgentListTeam => "remote.list-team",
                SharedMessage::ListSessions => "remote.list-sessions",
                SharedMessage::AgentAttach { .. } => "remote.attach",
                SharedMessage::AgentCancel { .. } => "remote.cancel",
                SharedMessage::AgentApprove { .. } => "remote.approve",
                SharedMessage::AgentListMedia { .. } => "remote.media",
                SharedMessage::AgentInput { .. } => "remote.prompt",
            };
            let mut entity = commands.spawn((
                Name::new("Accepted remote request"),
                Operation::bundle(kind).unwrap().running(),
                OperationRetained,
                EntityTarget::<AcpService>::new(owner),
                Loan {
                    executor: executor.0.clone(),
                    wake: wake.clone(),
                },
                Reply(Some(input.reply)),
            ));
            match input.request {
                SharedMessage::AgentNewChat { request } => {
                    entity.insert((Broker(request), NeedsClaim));
                }
                SharedMessage::AgentListModels { request } => {
                    entity.insert(Broker(request));
                }
                SharedMessage::AgentSelectModel { request } => {
                    entity.insert(Broker(request));
                }
                SharedMessage::AgentSetEffort { request } => {
                    entity.insert(Broker(request));
                }
                SharedMessage::AgentListAgents => {
                    entity.insert(Broker(AgentListAgents));
                }
                SharedMessage::AgentListTeam => {
                    entity.insert(Broker(AgentListTeam));
                }
                SharedMessage::ListSessions => {
                    entity.insert(Sessions);
                }
                SharedMessage::AgentAttach { sid } => {
                    entity.insert((SessionId(sid), Attach));
                }
                SharedMessage::AgentCancel { sid } => {
                    entity.insert((SessionId(sid), Cancel));
                }
                SharedMessage::AgentApprove {
                    sid,
                    call_id,
                    decision,
                } => {
                    entity.insert((SessionId(sid), Approve { call_id, decision }));
                }
                SharedMessage::AgentListMedia { sid, query } => {
                    entity.insert((SessionId(sid), Media::new(query)));
                }
                SharedMessage::AgentInput {
                    sid,
                    text,
                    context,
                    attachments,
                    preferred_mode,
                } => {
                    entity.insert((
                        SessionId(sid),
                        Prompt::new(text, context, attachments, preferred_mode),
                    ));
                }
            }
        }
    }
}

fn retire(
    operations: Query<
        (Entity, &EntityTarget<AcpService>),
        (With<Reply>, Without<Response>, Without<Waiting>),
    >,
    services: Query<(), (With<AcpService>, Without<Stopping>)>,
    mut commands: Commands,
) {
    for (entity, owner) in &operations {
        if !services.contains(owner.entity()) {
            commands
                .entity(entity)
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
    }
}

fn finish(
    mut operations: Query<
        (Entity, &mut Response, &mut Reply),
        (
            Without<Waiting>,
            Without<Finished>,
            Without<super::remote_broker::ClaimLease>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, mut response, mut reply) in &mut operations {
        let response = std::mem::replace(&mut response.0, SharedResponse::Ok);
        let result = match &response {
            SharedResponse::Failed(failure) => Err(format!("Remote request failed: {failure:?}")),
            _ => Ok(()),
        };
        if let Some(reply) = reply.0.take() {
            let _ = reply.send(response);
        }
        commands.trigger(OperationCompleted { entity, result });
        commands
            .entity(entity)
            .remove::<(Reply, Response, Loan)>()
            .insert(Finished);
    }
}

fn release_retention(
    operations: Query<(Entity, &Uuid), (With<Finished>, With<OperationRetained>)>,
    native: Query<&Accepted>,
    mut commands: Commands,
) {
    for (entity, uuid) in &operations {
        if !native.iter().any(|accepted| &accepted.uuid == uuid) {
            commands.entity(entity).remove::<OperationRetained>();
        }
    }
}

fn stop(
    mut inboxes: Query<(
        Entity,
        &mut Inbox,
        &vmux_ecs::service::Executor,
        &vmux_ecs::service::Wake,
    )>,
    mut commands: Commands,
) {
    for (entity, mut inbox, executor, wake) in &mut inboxes {
        inbox.0.close();
        while let Ok(input) = inbox.0.try_recv() {
            let _ = input
                .reply
                .send(SharedResponse::Failed(SharedFailure::NoDesktop));
        }
        commands.entity(entity).insert(Stopping);
        let (sender, receiver) = oneshot::channel();
        let task = executor.0.spawn(async move {
            let _ = receiver.await;
        });
        commands.spawn((
            Name::new("Remote drain acknowledgment"),
            RemoteDrain(Some(sender)),
            TokioTask::new(task),
            ShutdownPending,
            Loan {
                executor: executor.0.clone(),
                wake: wake.clone(),
            },
        ));
    }
}

fn acknowledge(
    operations: Query<(), With<Reply>>,
    native: Query<(), With<Accepted>>,
    mut drains: Query<&mut RemoteDrain>,
) {
    if !operations.is_empty() || !native.is_empty() {
        return;
    }
    for mut drain in &mut drains {
        if let Some(sender) = drain.0.take() {
            let _ = sender.send(());
        }
    }
}

fn drain(
    loans: Query<&Loan>,
    mut tasks: Query<
        (Entity, &mut TokioTask, Option<&Joined>),
        Or<(With<Accepted>, With<RemoteDrain>)>,
    >,
    replies: Query<Entity, With<Reply>>,
    mut commands: Commands,
) {
    let Some(loan) = loans.iter().next() else {
        return;
    };
    for (_, task, joined) in &tasks {
        if joined.is_none() {
            task.abort();
        }
    }
    loan.executor.block_on(async {
        for (entity, mut task, joined) in &mut tasks {
            if joined.is_none() {
                let _ = task.join().await;
            }
            commands.entity(entity).despawn();
        }
    });
    for entity in &replies {
        commands.entity(entity).despawn();
    }
}
