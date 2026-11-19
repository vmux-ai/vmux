use tokio::sync::{mpsc, oneshot};
use vmux_api::protocol::{
    ServiceMessage, SharedEvent, SharedFailure, SharedMessage, SharedResponse,
};
use vmux_transport::service::{RemoteDriver, RemoteFuture};

use crate::acp::AcpSessions;
use crate::remote::{Inbox, Received};

pub struct AgentRemoteDriver {
    sessions: AcpSessions,
    requests: mpsc::UnboundedSender<Received>,
    wake: mpsc::UnboundedSender<()>,
}

impl AgentRemoteDriver {
    pub(crate) fn new(sessions: AcpSessions, wake: mpsc::UnboundedSender<()>) -> (Self, Inbox) {
        let (requests, receiver) = mpsc::unbounded_channel();
        (
            Self {
                sessions,
                requests,
                wake,
            },
            Inbox(receiver),
        )
    }
}

impl RemoteDriver for AgentRemoteDriver {
    fn dispatch(&self, request: SharedMessage) -> RemoteFuture<'_, SharedResponse> {
        Box::pin(async move {
            let (reply, receiver) = oneshot::channel();
            if self.requests.send(Received { request, reply }).is_err() {
                return SharedResponse::Failed(SharedFailure::NoDesktop);
            }
            let _ = self.wake.send(());
            receiver
                .await
                .unwrap_or(SharedResponse::Failed(SharedFailure::NoDesktop))
        })
    }

    fn subscription(&self, request: &SharedMessage) -> Option<String> {
        match request {
            SharedMessage::AgentAttach { sid } => Some(sid.clone()),
            _ => None,
        }
    }

    fn subscribe(
        &self,
        sid: String,
    ) -> RemoteFuture<'_, Option<tokio::sync::broadcast::Receiver<ServiceMessage>>> {
        Box::pin(self.sessions.subscribe(sid))
    }

    fn resolve(&self, sid: String, event: SharedEvent) -> RemoteFuture<'_, Option<SharedEvent>> {
        Box::pin(async move {
            match event {
                SharedEvent::AcpAgentInfo { .. } | SharedEvent::AcpWorkspaceChanged { .. } => {
                    let session = self.sessions.remote_session(sid).await?;
                    Some(SharedEvent::Session { session })
                }
                event => Some(event),
            }
        })
    }

    fn snapshot(&self, sid: String) -> RemoteFuture<'_, Option<SharedEvent>> {
        Box::pin(async move {
            match self.sessions.snapshot(sid).await? {
                ServiceMessage::Shared(event) => Some(event),
                _ => None,
            }
        })
    }
}
