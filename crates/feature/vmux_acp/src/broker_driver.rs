use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::sync::{Mutex, mpsc, oneshot};
use vmux_api::browser::AgentOpenBrowser;
use vmux_api::protocol::{
    AGENT_QUERY_TIMEOUT, AGENT_REQUEST_TIMEOUT, AgentCommandResult, AgentRequest, AgentRequestId,
    ProcessId, ServiceMessage,
};

pub type AgentCommandResponses = PendingResponses<AgentRequestId, AgentCommandResult>;
pub type AgentQueryResponses = PendingResponses<AgentRequestId, ServiceMessage>;
pub type AgentToolResponses = PendingResponses<AgentRequestId, (String, bool)>;

const NO_AGENT_SUBSCRIBER: &str = "no desktop subscribed to agent commands";

#[derive(Default)]
struct Recipients(Vec<mpsc::WeakUnboundedSender<ServiceMessage>>);

impl Recipients {
    fn add(&mut self, target: &mpsc::UnboundedSender<ServiceMessage>) {
        self.0
            .retain(|client| client.upgrade().is_some_and(|client| !client.is_closed()));
        for client in &self.0 {
            if client
                .upgrade()
                .is_some_and(|client| client.same_channel(target))
            {
                return;
            }
        }
        self.0.push(target.downgrade());
    }

    fn target(&mut self) -> Option<mpsc::UnboundedSender<ServiceMessage>> {
        self.0
            .retain(|client| client.upgrade().is_some_and(|client| !client.is_closed()));
        self.0.first()?.upgrade()
    }
}

#[derive(Default)]
struct BrowserClients {
    all: Recipients,
    sources: HashMap<String, Recipients>,
}

impl BrowserClients {
    fn target(
        &mut self,
        source: Option<&str>,
    ) -> Result<mpsc::UnboundedSender<ServiceMessage>, String> {
        if let Some(source) = source {
            return self
                .sources
                .get_mut(source)
                .and_then(Recipients::target)
                .ok_or_else(|| {
                    "browser source conversation is not attached to a desktop".to_string()
                });
        }
        self.all
            .target()
            .ok_or_else(|| NO_AGENT_SUBSCRIBER.to_string())
    }
}

pub struct PendingResponses<K, V> {
    entries: Arc<Mutex<HashMap<K, oneshot::Sender<V>>>>,
}

impl<K, V> Clone for PendingResponses<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: Arc::clone(&self.entries),
        }
    }
}

impl<K, V> Default for PendingResponses<K, V> {
    fn default() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<K, V> PendingResponses<K, V>
where
    K: Copy + Eq + Hash,
{
    async fn request(
        &self,
        id: K,
        timeout: Duration,
        publish: impl FnOnce() -> bool,
        unavailable: &'static str,
        timed_out: &'static str,
    ) -> Result<V, String> {
        let (sender, receiver) = oneshot::channel();
        self.entries.lock().await.insert(id, sender);
        if !publish() {
            self.entries.lock().await.remove(&id);
            return Err(unavailable.to_string());
        }
        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(response)) => Ok(response),
            _ => {
                self.entries.lock().await.remove(&id);
                Err(timed_out.to_string())
            }
        }
    }

    async fn resolve(&self, id: K, response: V) -> bool {
        let Some(sender) = self.entries.lock().await.remove(&id) else {
            return false;
        };
        sender.send(response).is_ok()
    }
}

#[derive(Clone)]
pub struct AgentBroker {
    outbound: broadcast::Sender<ServiceMessage>,
    commands: AgentCommandResponses,
    queries: AgentQueryResponses,
    tools: AgentToolResponses,
    browsers: Arc<Mutex<BrowserClients>>,
}

impl AgentBroker {
    pub fn new(
        outbound: broadcast::Sender<ServiceMessage>,
        commands: AgentCommandResponses,
        queries: AgentQueryResponses,
        tools: AgentToolResponses,
    ) -> Self {
        Self {
            outbound,
            commands,
            queries,
            tools,
            browsers: Default::default(),
        }
    }

    pub async fn subscribe_browser(&self, outbound: &mpsc::UnboundedSender<ServiceMessage>) {
        let mut clients = self.browsers.lock().await;
        clients.all.add(outbound);
        clients
            .sources
            .retain(|_, recipients| recipients.target().is_some());
    }

    pub async fn attach_browser(
        &self,
        sid: &str,
        outbound: &mpsc::UnboundedSender<ServiceMessage>,
    ) {
        self.browsers
            .lock()
            .await
            .sources
            .entry(sid.to_string())
            .or_default()
            .add(outbound);
    }

    pub async fn command(
        &self,
        request_id: AgentRequestId,
        anchor: Option<ProcessId>,
        request: AgentRequest,
    ) -> Result<AgentCommandResult, String> {
        if let Some(browser) = request.decode::<AgentOpenBrowser>()? {
            let target = self
                .browsers
                .lock()
                .await
                .target(browser.source.as_deref())?;
            return self
                .commands
                .request(
                    request_id,
                    AGENT_REQUEST_TIMEOUT,
                    || {
                        target
                            .send(ServiceMessage::AgentRequest {
                                request_id,
                                anchor,
                                request,
                            })
                            .is_ok()
                    },
                    "browser client disconnected",
                    "browser opening timed out",
                )
                .await;
        }
        if self.outbound.receiver_count() == 0 {
            return Err(NO_AGENT_SUBSCRIBER.to_string());
        }
        self.commands
            .request(
                request_id,
                AGENT_REQUEST_TIMEOUT,
                || {
                    self.outbound
                        .send(ServiceMessage::AgentRequest {
                            request_id,
                            anchor,
                            request,
                        })
                        .is_ok()
                },
                NO_AGENT_SUBSCRIBER,
                "agent command timed out",
            )
            .await
    }

    pub async fn query(
        &self,
        request_id: AgentRequestId,
        query: AgentRequest,
    ) -> Result<ServiceMessage, String> {
        if self.outbound.receiver_count() == 0 {
            return Err(NO_AGENT_SUBSCRIBER.to_string());
        }
        self.queries
            .request(
                request_id,
                AGENT_QUERY_TIMEOUT,
                || {
                    self.outbound
                        .send(ServiceMessage::AgentQuery { request_id, query })
                        .is_ok()
                },
                NO_AGENT_SUBSCRIBER,
                "agent query timed out",
            )
            .await
    }

    pub async fn resolve_command(
        &self,
        request_id: AgentRequestId,
        result: AgentCommandResult,
    ) -> bool {
        self.commands.resolve(request_id, result).await
    }

    pub async fn resolve_query(&self, request_id: AgentRequestId, result: ServiceMessage) -> bool {
        self.queries.resolve(request_id, result).await
    }

    pub async fn resolve_tool(&self, request_id: AgentRequestId, content: String, is_error: bool) {
        self.tools.resolve(request_id, (content, is_error)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl AgentBroker {
        fn start() -> Self {
            let (outbound, _) = broadcast::channel(16);
            Self::new(
                outbound,
                Default::default(),
                Default::default(),
                Default::default(),
            )
        }
    }

    #[tokio::test]
    async fn browser_request_goes_only_to_a_source_owner_and_preserves_its_error() {
        let broker = AgentBroker::start();
        let (other, mut other_receiver) = mpsc::unbounded_channel();
        let (owner, mut owner_receiver) = mpsc::unbounded_channel();
        broker.subscribe_browser(&other).await;
        broker.subscribe_browser(&owner).await;
        broker.attach_browser("source", &owner).await;
        let request_id = AgentRequestId::new();
        let request = AgentRequest::encode(&AgentOpenBrowser {
            url: "https://example.test/auth".into(),
            source: Some("source".into()),
        })
        .unwrap();
        let sender = broker.clone();
        let task = tokio::spawn(async move { sender.command(request_id, None, request).await });
        let received = owner_receiver.recv().await.unwrap();
        assert!(
            matches!(received, ServiceMessage::AgentRequest { request_id: received, .. } if received == request_id)
        );
        assert!(other_receiver.try_recv().is_err());
        assert!(
            broker
                .resolve_command(
                    request_id,
                    AgentCommandResult::Error("owner rejected the page".into())
                )
                .await
        );
        assert!(
            matches!(task.await.unwrap(), Ok(AgentCommandResult::Error(error)) if error == "owner rejected the page")
        );
    }

    #[tokio::test]
    async fn browser_source_without_a_desktop_fails_without_broadcasting() {
        let broker = AgentBroker::start();
        let (other, mut receiver) = mpsc::unbounded_channel();
        broker.subscribe_browser(&other).await;
        let request = AgentRequest::encode(&AgentOpenBrowser {
            url: "https://example.test/auth".into(),
            source: Some("missing".into()),
        })
        .unwrap();
        let result = broker.command(AgentRequestId::new(), None, request).await;
        assert!(
            matches!(result, Err(error) if error == "browser source conversation is not attached to a desktop")
        );
        assert!(receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn browser_source_uses_another_attached_desktop_after_disconnect() {
        let broker = AgentBroker::start();
        let (first, first_receiver) = mpsc::unbounded_channel();
        let (second, mut second_receiver) = mpsc::unbounded_channel();
        broker.subscribe_browser(&first).await;
        broker.subscribe_browser(&second).await;
        broker.attach_browser("source", &first).await;
        broker.attach_browser("source", &second).await;
        drop(first_receiver);
        let request_id = AgentRequestId::new();
        let request = AgentRequest::encode(&AgentOpenBrowser {
            url: "https://example.test/auth".into(),
            source: Some("source".into()),
        })
        .unwrap();
        let sender = broker.clone();
        let task = tokio::spawn(async move { sender.command(request_id, None, request).await });
        assert!(
            matches!(second_receiver.recv().await.unwrap(), ServiceMessage::AgentRequest { request_id: received, .. } if received == request_id)
        );
        broker
            .resolve_command(request_id, AgentCommandResult::Ok)
            .await;
        assert!(matches!(task.await.unwrap(), Ok(AgentCommandResult::Ok)));
    }
}
