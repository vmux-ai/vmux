use std::sync::Arc;

use bevy::prelude::*;
use vmux_api::protocol::{
    AgentCommandResult, AgentNewChat, AgentRequest, AgentRequestId, SharedFailure, SharedResponse,
};
use vmux_api::session::ClientOpId;
use vmux_ecs::service::Operations;
use vmux_ecs::{EntityTarget, OperationTarget, Uuid};
use vmux_transport::service::RemoteOperationStore;

use super::remote::{Live, RemoteSet, Response, Waiting};
use super::remote_io::{Accepted, Joined, Loan, Output};
use super::service_driver::AcpService;

pub(super) struct RemoteBrokerPlugin;

impl Plugin for RemoteBrokerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (collect_claims, collect_broker, collect_release)
                .chain()
                .in_set(RemoteSet::Collect),
        )
        .add_systems(
            Update,
            (
                claim,
                submit::<AgentNewChat>,
                submit::<vmux_api::protocol::AgentListModels>,
                submit::<vmux_api::protocol::AgentSelectModel>,
                submit::<vmux_api::protocol::AgentSetEffort>,
                submit::<vmux_api::protocol::AgentListAgents>,
                submit::<vmux_api::protocol::AgentListTeam>,
                release,
            )
                .in_set(RemoteSet::Submit),
        )
        .add_systems(
            Update,
            (
                discard::<AgentNewChat>,
                discard::<vmux_api::protocol::AgentListModels>,
                discard::<vmux_api::protocol::AgentSelectModel>,
                discard::<vmux_api::protocol::AgentSetEffort>,
                discard::<vmux_api::protocol::AgentListAgents>,
                discard::<vmux_api::protocol::AgentListTeam>,
            )
                .in_set(RemoteSet::Clean),
        );
    }
}

#[derive(Component)]
#[component(immutable)]
pub(super) struct Broker<T: Send + Sync + 'static>(pub T);

#[derive(Component)]
pub(super) struct NeedsClaim;

#[derive(Component, Clone)]
pub(super) struct ClaimLease {
    store: Arc<dyn RemoteOperationStore>,
    id: ClientOpId,
}

#[derive(Component)]
struct ClaimIo;

#[derive(Component)]
struct BrokerIo;

#[derive(Component)]
struct ReleaseIo;

fn discard<T: Send + Sync + 'static>(
    requests: Query<
        Entity,
        (
            With<Response>,
            Without<Waiting>,
            Without<ClaimLease>,
            With<Broker<T>>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in &requests {
        commands.entity(entity).remove::<Broker<T>>();
    }
}

fn claim(
    requests: Query<
        (
            Entity,
            &Broker<AgentNewChat>,
            &EntityTarget<AcpService>,
            &Uuid,
            &Loan,
        ),
        (With<NeedsClaim>, Without<Waiting>, Without<Response>),
    >,
    stores: Query<&Operations, With<AcpService>>,
    mut commands: Commands,
) {
    for (entity, request, owner, uuid, loan) in &requests {
        let id = &request.0.client_op_id;
        if id.as_str().trim().is_empty() || id.as_str().len() > 256 {
            commands
                .entity(entity)
                .insert(Response(SharedResponse::Failed(SharedFailure::Invalid)));
            continue;
        }
        let Ok(store) = stores.get(owner.entity()) else {
            continue;
        };
        let lease = ClaimLease {
            store: store.0.clone(),
            id: id.clone(),
        };
        let store = lease.store.clone();
        let id = lease.id.clone();
        let native = Output::spawn(
            &mut commands,
            loan,
            Some(entity),
            Accepted {
                owner: owner.entity(),
                uuid: uuid.clone(),
            },
            async move { store.claim(id).await },
        );
        commands.entity(native).insert((ClaimIo, lease));
        commands.entity(entity).insert(Waiting(native));
    }
}

fn submit<T>(
    requests: Query<
        (
            Entity,
            &Broker<T>,
            &EntityTarget<AcpService>,
            &Uuid,
            &Loan,
            Option<&ClaimLease>,
        ),
        (Without<NeedsClaim>, Without<Waiting>, Without<Response>),
    >,
    services: Query<&AcpService>,
    mut commands: Commands,
) where
    T: vmux_api::AgentRequestContract + serde::Serialize + Send + Sync + 'static,
{
    for (entity, request, owner, uuid, loan, lease) in &requests {
        let Ok(service) = services.get(owner.entity()) else {
            continue;
        };
        let request = match AgentRequest::encode(&request.0) {
            Ok(request) => request,
            Err(_) => {
                commands
                    .entity(entity)
                    .insert(Response(SharedResponse::Failed(SharedFailure::Invalid)));
                continue;
            }
        };
        let broker = service.broker().clone();
        let native = Output::spawn(
            &mut commands,
            loan,
            Some(entity),
            Accepted {
                owner: owner.entity(),
                uuid: uuid.clone(),
            },
            async move { broker.command(AgentRequestId::new(), None, request).await },
        );
        commands.entity(native).insert(BrokerIo);
        if let Some(lease) = lease {
            commands.entity(native).insert(lease.clone());
        }
        commands.entity(entity).insert(Waiting(native));
    }
}

fn collect_claims(
    mut native: Query<
        (
            Entity,
            &mut Output<bool>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
            &ClaimLease,
            &Loan,
        ),
        With<ClaimIo>,
    >,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target, lease, loan) in &mut native {
        let claimed = match output.take(joined) {
            Ok(claimed) => claimed,
            Err(_) => {
                if let Some(target) = live.matching(entity, accepted, target) {
                    commands
                        .entity(target)
                        .remove::<Waiting>()
                        .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
                }
                commands.entity(entity).despawn();
                continue;
            }
        };
        if let Some(target) = live.accepts(entity, accepted, target) {
            let mut target = commands.entity(target);
            target.remove::<Waiting>().remove::<NeedsClaim>();
            if claimed {
                target.insert(lease.clone());
            } else {
                target.insert(Response(SharedResponse::AlreadyApplied));
            }
        } else if claimed {
            let target = live.matching(entity, accepted, target);
            let store = lease.store.clone();
            let id = lease.id.clone();
            let release =
                Output::spawn(&mut commands, loan, target, accepted.clone(), async move {
                    store.release(id).await
                });
            commands.entity(release).insert(ReleaseIo);
            if let Some(target) = target {
                commands.entity(target).remove::<ClaimLease>().insert((
                    Waiting(release),
                    Response(SharedResponse::Failed(SharedFailure::NoDesktop)),
                ));
            }
        } else if let Some(target) = live.matching(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
        commands.entity(entity).despawn();
    }
}

fn collect_broker(
    mut native: Query<
        (
            Entity,
            &mut Output<Result<AgentCommandResult, String>>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
            Option<&ClaimLease>,
            &Loan,
        ),
        With<BrokerIo>,
    >,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target, lease, loan) in &mut native {
        let response = match output.take(joined) {
            Ok(Ok(AgentCommandResult::Text(json))) => SharedResponse::BrokerJson(json),
            Ok(Ok(AgentCommandResult::Ok)) => SharedResponse::Ok,
            Ok(Ok(AgentCommandResult::Error(message))) => {
                tracing::warn!(%message, "remote quic: the GUI refused a brokered command");
                SharedResponse::Failed(SharedFailure::Invalid)
            }
            _ => SharedResponse::Failed(SharedFailure::NoDesktop),
        };
        if let Some(target) = live.accepts(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(response));
        } else {
            let target = live.matching(entity, accepted, target);
            if matches!(response, SharedResponse::Failed(_))
                && let Some(lease) = lease
            {
                let store = lease.store.clone();
                let id = lease.id.clone();
                let release =
                    Output::spawn(&mut commands, loan, target, accepted.clone(), async move {
                        store.release(id).await
                    });
                commands.entity(release).insert(ReleaseIo);
                if let Some(target) = target {
                    commands.entity(target).remove::<ClaimLease>().insert((
                        Waiting(release),
                        Response(SharedResponse::Failed(SharedFailure::NoDesktop)),
                    ));
                }
            } else if let Some(target) = target {
                commands
                    .entity(target)
                    .remove::<Waiting>()
                    .remove::<ClaimLease>()
                    .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
            }
        }
        commands.entity(entity).despawn();
    }
}

fn release(
    operations: Query<
        (
            Entity,
            &Response,
            &ClaimLease,
            &EntityTarget<AcpService>,
            &Uuid,
            &Loan,
        ),
        Without<Waiting>,
    >,
    mut commands: Commands,
) {
    for (entity, response, lease, owner, uuid, loan) in &operations {
        if !matches!(response.0, SharedResponse::Failed(_)) {
            commands.entity(entity).remove::<ClaimLease>();
            continue;
        }
        let store = lease.store.clone();
        let id = lease.id.clone();
        let native = Output::spawn(
            &mut commands,
            loan,
            Some(entity),
            Accepted {
                owner: owner.entity(),
                uuid: uuid.clone(),
            },
            async move { store.release(id).await },
        );
        commands.entity(native).insert(ReleaseIo);
        commands
            .entity(entity)
            .remove::<ClaimLease>()
            .insert(Waiting(native));
    }
}

fn collect_release(
    mut native: Query<
        (
            Entity,
            &mut Output<()>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
        ),
        With<ReleaseIo>,
    >,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target) in &mut native {
        let result = output.take(joined);
        if let Some(target) = live.matching(entity, accepted, target) {
            let mut target = commands.entity(target);
            target.remove::<Waiting>();
            if result.is_err() {
                target.insert(Response(SharedResponse::Failed(SharedFailure::Internal)));
            }
        }
        commands.entity(entity).despawn();
    }
}
