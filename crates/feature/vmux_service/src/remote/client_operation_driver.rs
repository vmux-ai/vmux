use bevy::prelude::{Bundle, Component};
use tokio::sync::{mpsc, oneshot};
use vmux_api::room::ClientOpId;
use vmux_transport::service::{RemoteFuture, RemoteOperationStore};

#[derive(Clone)]
pub(crate) struct ClientOperations {
    claims: mpsc::UnboundedSender<ClaimClientOperation>,
    releases: mpsc::UnboundedSender<ReleaseClientOperation>,
    wake: mpsc::UnboundedSender<()>,
}

impl ClientOperations {
    pub(crate) fn new(wake: mpsc::UnboundedSender<()>) -> (Self, impl Bundle) {
        let (claims, claim_inbox) = mpsc::unbounded_channel();
        let (releases, release_inbox) = mpsc::unbounded_channel();
        (
            Self {
                claims,
                releases,
                wake,
            },
            ClientOperationInbox(ClientOperationReceivers {
                claims: claim_inbox,
                releases: release_inbox,
            }),
        )
    }

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
}

impl RemoteOperationStore for ClientOperations {
    fn claim(&self, id: ClientOpId) -> RemoteFuture<'_, bool> {
        Box::pin(ClientOperations::claim(self, id))
    }

    fn release(&self, id: ClientOpId) -> RemoteFuture<'_, ()> {
        Box::pin(ClientOperations::release(self, id))
    }
}

pub(super) struct ClientOperationReceivers {
    pub(super) claims: mpsc::UnboundedReceiver<ClaimClientOperation>,
    pub(super) releases: mpsc::UnboundedReceiver<ReleaseClientOperation>,
}

#[derive(Component)]
pub(super) struct ClientOperationInbox(pub(super) ClientOperationReceivers);

#[derive(Component)]
pub(super) struct ClaimClientOperation {
    pub(super) id: ClientOpId,
    pub(super) response: Option<oneshot::Sender<bool>>,
}

#[derive(Component)]
pub(super) struct ReleaseClientOperation {
    pub(super) id: ClientOpId,
    pub(super) response: Option<oneshot::Sender<()>>,
}
