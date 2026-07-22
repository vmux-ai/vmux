//! ACP host (Agent Client Protocol). vmux implements the `Client` role: external coding
//! agents run as spawned subprocesses driven over JSON-RPC, surfaced through vmux's native
//! panes. Mirrors [`crate::agent::AgentSessionManager`]: one broadcast channel per session
//! that the server forwards to the connected client.

mod driver;
mod projector;

pub use driver::{AcpInput, AcpShared};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{broadcast, mpsc};
use vmux_core::ProcessId;

use crate::process::{ProcessManager, PtyInputWriter};
use crate::protocol::ServiceMessage;
use crate::remote::RemoteSession;

struct AcpHandle {
    input_tx: mpsc::UnboundedSender<AcpInput>,
    shared: Arc<AcpShared>,
    agent_id: String,
    created_at_ms: u64,
}

/// Tracks live ACP sessions by sid. Constructed once in the daemon and shared like
/// [`crate::agent::AgentSessionManager`].
#[derive(Default)]
pub struct AcpSessionManager {
    sessions: HashMap<String, AcpHandle>,
}

impl AcpSessionManager {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &mut self,
        sid: String,
        agent_id: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: PathBuf,
        anchor: ProcessId,
        manager: Arc<tokio::sync::Mutex<ProcessManager>>,
        input_writers: Arc<tokio::sync::Mutex<HashMap<ProcessId, PtyInputWriter>>>,
        mcp_servers: Vec<agent_client_protocol::schema::v1::McpServer>,
        resume: Option<String>,
    ) {
        if self.sessions.contains_key(&sid) {
            return;
        }
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let (stream_tx, _) = broadcast::channel(256);
        let shared = Arc::new(AcpShared::new(
            sid.clone(),
            cwd,
            anchor,
            stream_tx,
            manager,
            input_writers,
        ));
        tokio::spawn(driver::run(
            command,
            args,
            env,
            agent_id.clone(),
            mcp_servers,
            resume,
            shared.clone(),
            input_rx,
        ));
        self.sessions.insert(
            sid,
            AcpHandle {
                input_tx,
                shared,
                agent_id,
                created_at_ms: now_ms(),
            },
        );
    }

    pub fn input(&self, sid: &str, input: AcpInput) -> bool {
        match self.sessions.get(sid) {
            Some(handle) => {
                if let AcpInput::Approve { call_id, .. } = &input {
                    handle.shared.resolve_approval(call_id);
                }
                handle.input_tx.send(input).is_ok()
            }
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

    pub fn agent_info(&self, sid: &str) -> Option<ServiceMessage> {
        self.sessions
            .get(sid)
            .and_then(|handle| handle.shared.agent_info_message())
    }

    pub fn model_info(&self, sid: &str) -> Option<ServiceMessage> {
        self.sessions
            .get(sid)
            .and_then(|handle| handle.shared.model_info_message())
    }

    pub fn remote_messages(&self, sid: &str) -> Option<Vec<crate::message::Message>> {
        self.sessions
            .get(sid)
            .map(|handle| handle.shared.remote_messages())
    }

    pub fn remote_sessions(&self) -> Vec<RemoteSession> {
        self.sessions
            .values()
            .map(|handle| {
                handle
                    .shared
                    .remote_session(&handle.agent_id, handle.created_at_ms)
            })
            .collect()
    }

    pub fn remote_session(&self, sid: &str) -> Option<RemoteSession> {
        self.sessions.get(sid).map(|handle| {
            handle
                .shared
                .remote_session(&handle.agent_id, handle.created_at_ms)
        })
    }

    pub fn contains(&self, sid: &str) -> bool {
        self.sessions.contains_key(sid)
    }

    pub fn rebind_cwd(&self, sid: &str, cwd: PathBuf) -> Result<(), String> {
        self.sessions
            .get(sid)
            .ok_or_else(|| "ACP session not found".to_string())?
            .shared
            .rebind_cwd(cwd)
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_routes_input_by_sid() {
        let mgr = AcpSessionManager::default();
        assert!(!mgr.contains("nope"));
        assert!(!mgr.input(
            "nope",
            AcpInput::User {
                text: "x".to_string(),
                context: None,
                attachments: Vec::new(),
            }
        ));
        assert!(mgr.subscribe("nope").is_none());
        assert!(mgr.snapshot("nope").is_none());
        assert!(mgr.agent_info("nope").is_none());
        assert!(mgr.model_info("nope").is_none());
    }
}
