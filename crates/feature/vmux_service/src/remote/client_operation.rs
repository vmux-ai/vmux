use std::sync::Mutex;

use bevy::prelude::*;
use tokio::sync::{mpsc, oneshot};
use vmux_api::room::ClientOpId;

const MAX_CLIENT_OPERATIONS: usize = 4096;

pub(crate) struct ClientOperationPlugin {
    inbox: Mutex<Option<ClientOperationReceivers>>,
}

impl ClientOperationPlugin {
    pub(crate) fn new(wake: mpsc::UnboundedSender<()>) -> (Self, ClientOperations) {
        let (claims, claim_inbox) = mpsc::unbounded_channel();
        let (releases, release_inbox) = mpsc::unbounded_channel();
        (
            Self {
                inbox: Mutex::new(Some(ClientOperationReceivers {
                    claims: claim_inbox,
                    releases: release_inbox,
                })),
            },
            ClientOperations {
                claims,
                releases,
                wake,
            },
        )
    }
}

impl Plugin for ClientOperationPlugin {
    fn build(&self, app: &mut App) {
        let inbox = self
            .inbox
            .lock()
            .unwrap()
            .take()
            .expect("client operation plugin can only be built once");
        app.world_mut().spawn((
            Name::new("remote client operations"),
            ClientOperationInbox(inbox),
        ));
        app.add_systems(
            Update,
            (
                receive_client_operation_requests,
                ApplyDeferred,
                release_client_operations,
                ApplyDeferred,
                claim_client_operations,
            )
                .chain(),
        );
    }
}

#[derive(Clone)]
pub(crate) struct ClientOperations {
    claims: mpsc::UnboundedSender<ClaimClientOperation>,
    releases: mpsc::UnboundedSender<ReleaseClientOperation>,
    wake: mpsc::UnboundedSender<()>,
}

impl ClientOperations {
    pub(crate) async fn claim(&self, id: ClientOpId) -> bool {
        let (response, receiver) = oneshot::channel();
        if self
            .claims
            .send(ClaimClientOperation {
                id,
                response: Some(response),
            })
            .is_err()
        {
            return false;
        }
        if self.wake.send(()).is_err() {
            return false;
        }
        receiver.await.unwrap_or(false)
    }

    pub(crate) async fn release(&self, id: ClientOpId) {
        let (response, receiver) = oneshot::channel();
        if self
            .releases
            .send(ReleaseClientOperation {
                id,
                response: Some(response),
            })
            .is_err()
        {
            return;
        }
        if self.wake.send(()).is_err() {
            return;
        }
        let _ = receiver.await;
    }

    #[cfg(test)]
    pub(crate) fn closed() -> Self {
        let (claims, claim_inbox) = mpsc::unbounded_channel();
        let (releases, release_inbox) = mpsc::unbounded_channel();
        let (wake, wake_inbox) = mpsc::unbounded_channel();
        drop((claim_inbox, release_inbox, wake_inbox));
        Self {
            claims,
            releases,
            wake,
        }
    }
}

struct ClientOperationReceivers {
    claims: mpsc::UnboundedReceiver<ClaimClientOperation>,
    releases: mpsc::UnboundedReceiver<ReleaseClientOperation>,
}

#[derive(Component)]
struct ClientOperationInbox(ClientOperationReceivers);

#[derive(Component)]
struct ClaimClientOperation {
    id: ClientOpId,
    response: Option<oneshot::Sender<bool>>,
}

#[derive(Component)]
struct ReleaseClientOperation {
    id: ClientOpId,
    response: Option<oneshot::Sender<()>>,
}

#[derive(Component, Clone, Eq, PartialEq)]
struct ClientOperationId(ClientOpId);

#[derive(Component, Clone, Copy)]
struct ClientOperationSequence(u64);

fn receive_client_operation_requests(
    mut inbox: Single<&mut ClientOperationInbox>,
    mut commands: Commands,
) {
    while let Ok(request) = inbox.0.releases.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.claims.try_recv() {
        commands.spawn(request);
    }
}

fn release_client_operations(
    operations: Query<(Entity, &ClientOperationId)>,
    mut requests: Query<(Entity, &mut ReleaseClientOperation)>,
    mut commands: Commands,
) {
    let mut removed = Vec::new();
    for (request_entity, mut request) in &mut requests {
        for (operation_entity, operation) in &operations {
            if operation.0 != request.id || removed.contains(&operation_entity) {
                continue;
            }
            commands.entity(operation_entity).despawn();
            removed.push(operation_entity);
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(());
        }
        commands.entity(request_entity).despawn();
    }
}

fn claim_client_operations(
    operations: Query<(Entity, &ClientOperationId, &ClientOperationSequence)>,
    mut requests: Query<(Entity, &mut ClaimClientOperation)>,
    mut sequence: Local<u64>,
    mut commands: Commands,
) {
    let mut claimed = Vec::new();
    for (entity, id, order) in &operations {
        claimed.push((entity, id.0.clone(), order.0));
    }

    for (request_entity, mut request) in &mut requests {
        let accepted = !claimed.iter().any(|(_, id, _)| *id == request.id);
        if accepted {
            if claimed.len() >= MAX_CLIENT_OPERATIONS {
                let oldest = claimed
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, (_, _, order))| *order)
                    .map(|(index, _)| index)
                    .unwrap();
                let (entity, _, _) = claimed.swap_remove(oldest);
                commands.entity(entity).despawn();
            }
            let order = *sequence;
            *sequence = order.wrapping_add(1);
            let entity = commands
                .spawn((
                    Name::new(format!("client operation {}", request.id.as_str())),
                    ClientOperationId(request.id.clone()),
                    ClientOperationSequence(order),
                ))
                .id();
            claimed.push((entity, request.id.clone(), order));
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(accepted);
        }
        commands.entity(request_entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn claim(app: &mut App, operations: ClientOperations, id: ClientOpId) -> bool {
        let task = tokio::spawn(async move { operations.claim(id).await });
        while !task.is_finished() {
            tokio::task::yield_now().await;
            app.update();
        }
        task.await.unwrap()
    }

    #[tokio::test]
    async fn client_operations_are_entities_with_bounded_claims() {
        let (wake, _wake_inbox) = mpsc::unbounded_channel();
        let (plugin, operations) = ClientOperationPlugin::new(wake);
        let mut app = App::new();
        app.add_plugins(plugin);

        let first = ClientOpId::new("first");
        assert!(claim(&mut app, operations.clone(), first.clone()).await);
        assert!(!claim(&mut app, operations.clone(), first.clone()).await);

        let release = operations.clone();
        let release_task = tokio::spawn(async move { release.release(first.clone()).await });
        while !release_task.is_finished() {
            tokio::task::yield_now().await;
            app.update();
        }
        release_task.await.unwrap();
        assert!(claim(&mut app, operations.clone(), ClientOpId::new("first")).await);

        for index in 0..MAX_CLIENT_OPERATIONS {
            assert!(
                claim(
                    &mut app,
                    operations.clone(),
                    ClientOpId::new(format!("op-{index}")),
                )
                .await
            );
        }
        let world = app.world_mut();
        let mut query = world.query::<&ClientOperationId>();
        let count = query.iter(world).count();
        assert_eq!(count, MAX_CLIENT_OPERATIONS);
    }
}
