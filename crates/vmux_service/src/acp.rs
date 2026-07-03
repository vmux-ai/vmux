//! ACP host (Agent Client Protocol). vmux implements the `Client` role: external coding
//! agents run as spawned subprocesses driven over JSON-RPC, surfaced through vmux's native
//! panes. Mirrors [`crate::agent::AgentSessionManager`]: one broadcast channel per session
//! that the server forwards to the connected client.

mod driver;
mod projector;

pub use driver::{AcpInput, AcpShared};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};
use vmux_core::ProcessId;

use crate::protocol::ServiceMessage;

struct AcpHandle {
    input_tx: mpsc::UnboundedSender<AcpInput>,
    shared: Arc<AcpShared>,
}

/// Tracks live ACP sessions by sid. Constructed once in the daemon and shared like
/// [`crate::agent::AgentSessionManager`].
#[derive(Default)]
pub struct AcpSessionManager {
    sessions: HashMap<String, AcpHandle>,
}

impl AcpSessionManager {
    pub fn spawn(
        &mut self,
        sid: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: PathBuf,
        mcp_servers: Vec<agent_client_protocol::schema::v1::McpServer>,
    ) {
        if self.sessions.contains_key(&sid) {
            return;
        }
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let (stream_tx, _) = broadcast::channel(256);
        let shared = Arc::new(AcpShared {
            sid: sid.clone(),
            cwd,
            anchor: ProcessId::new(),
            stream_tx,
            projector: Mutex::new(projector::AcpProjector::new()),
            pending_perms: Mutex::new(HashMap::new()),
            terminals: Mutex::new(HashMap::new()),
            cancel_requested: std::sync::atomic::AtomicBool::new(false),
        });
        tokio::spawn(driver::run(
            command,
            args,
            env,
            mcp_servers,
            shared.clone(),
            input_rx,
        ));
        self.sessions.insert(sid, AcpHandle { input_tx, shared });
    }

    pub fn input(&self, sid: &str, input: AcpInput) -> bool {
        match self.sessions.get(sid) {
            Some(handle) => handle.input_tx.send(input).is_ok(),
            None => false,
        }
    }

    pub fn subscribe(&self, sid: &str) -> Option<broadcast::Receiver<ServiceMessage>> {
        self.sessions
            .get(sid)
            .map(|handle| handle.shared.stream_tx.subscribe())
    }

    pub fn snapshot(&self, sid: &str) -> Option<ServiceMessage> {
        self.sessions
            .get(sid)
            .map(|handle| handle.shared.snapshot_message())
    }

    pub fn contains(&self, sid: &str) -> bool {
        self.sessions.contains_key(sid)
    }

    /// Ask the session's driver to shut down: it observes `Close`, sends the ACP cancel
    /// notification, and kills its child on the way out. Dropping the handle (no `abort`) lets the
    /// task finish that cleanup instead of being killed mid-flight.
    pub fn close(&mut self, sid: &str) {
        if let Some(handle) = self.sessions.remove(sid) {
            let _ = handle.input_tx.send(AcpInput::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_routes_input_by_sid() {
        let mgr = AcpSessionManager::default();
        assert!(!mgr.contains("nope"));
        assert!(!mgr.input("nope", AcpInput::User("x".to_string())));
        assert!(mgr.subscribe("nope").is_none());
        assert!(mgr.snapshot("nope").is_none());
    }
}
