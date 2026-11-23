use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::{Bundle, Component};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use vmux_api::protocol::{
    AgentCommandResult, AgentProcessCommandExit, AgentProcessRunCompletion, AgentQueryResult,
    AgentReadProcessOutput, AgentReadProcessTranscript, AgentRequest, AgentRequestId,
    ClientMessage, ServiceMessage, SharedMessage,
};
use vmux_process::ProcessRuntime;
use vmux_transport::service::{
    RemoteDriver, RemoteFuture, ServiceProtocolConnection, ServiceProtocolDriver,
};

use crate::acp::{AcpSessions, CatalogDriver, CatalogRequest};
use crate::broker_driver::AgentBroker;
use crate::remote_driver::AgentRemoteDriver;

#[derive(Clone, Component)]
pub(crate) struct AcpService {
    sessions: AcpSessions,
    broker: AgentBroker,
    processes: ProcessRuntime,
    outbound: broadcast::Sender<ServiceMessage>,
}

impl AcpService {
    pub(crate) fn new(
        runtime: Handle,
        wake: mpsc::UnboundedSender<()>,
        processes: ProcessRuntime,
    ) -> (Self, impl Bundle) {
        let (sessions, session_runtime) = AcpSessions::new(runtime, wake);
        let (outbound, _) = broadcast::channel(128);
        let broker = AgentBroker::new(
            outbound.clone(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        (
            Self {
                sessions,
                broker,
                processes,
                outbound,
            },
            session_runtime,
        )
    }

    fn connection(&self, outbound: mpsc::UnboundedSender<ServiceMessage>) -> AcpServiceConnection {
        AcpServiceConnection {
            service: self.clone(),
            outbound,
            command_subscription: None,
            session_forwarders: HashMap::new(),
            catalog_tasks: HashMap::new(),
        }
    }

    pub(crate) fn remote_driver(
        &self,
        wake: mpsc::UnboundedSender<()>,
    ) -> (Arc<dyn RemoteDriver>, crate::remote::Inbox) {
        let (driver, inbox) = AgentRemoteDriver::new(self.sessions.clone(), wake);
        (Arc::new(driver), inbox)
    }

    pub(crate) fn sessions(&self) -> &AcpSessions {
        &self.sessions
    }

    pub(crate) fn broker(&self) -> &AgentBroker {
        &self.broker
    }

    async fn process_response(
        &self,
        request_id: AgentRequestId,
        request: &AgentRequest,
    ) -> Result<Option<ServiceMessage>, String> {
        if let Some(request) = request.decode::<AgentReadProcessOutput>()? {
            return Ok(Some(ServiceMessage::AgentQueryResult(
                AgentQueryResult::text(request_id, self.processes.output(request.process_id).await),
            )));
        }
        if let Some(request) = request.decode::<AgentReadProcessTranscript>()? {
            return Ok(Some(ServiceMessage::AgentQueryResult(
                AgentQueryResult::text(
                    request_id,
                    self.processes.transcript(request.process_id).await,
                ),
            )));
        }
        if let Some(request) = request.decode::<AgentProcessCommandExit>()? {
            let result = self
                .processes
                .command_exit(request.process_id)
                .await
                .map(|result| {
                    let exit = result
                        .exit
                        .map_or_else(|| "null".to_string(), |code| code.to_string());
                    format!("{{\"seq\":{},\"exit\":{exit}}}", result.sequence)
                });
            return Ok(Some(ServiceMessage::AgentQueryResult(
                AgentQueryResult::text(request_id, result),
            )));
        }
        if let Some(request) = request.decode::<AgentProcessRunCompletion>()? {
            let result = self
                .processes
                .run_completion(request.process_id)
                .await
                .map(|result| {
                    let token = result
                        .token
                        .map_or_else(|| "null".to_string(), |token| format!("\"{token}\""));
                    let exit = result
                        .exit
                        .map_or_else(|| "null".to_string(), |code| code.to_string());
                    format!("{{\"token\":{token},\"exit\":{exit}}}")
                });
            return Ok(Some(ServiceMessage::AgentQueryResult(
                AgentQueryResult::text(request_id, result),
            )));
        }
        Ok(None)
    }
}

impl ServiceProtocolDriver for AcpService {
    fn connect(
        &self,
        outbound: mpsc::UnboundedSender<ServiceMessage>,
    ) -> Box<dyn ServiceProtocolConnection> {
        Box::new(self.connection(outbound))
    }
}

struct AcpServiceConnection {
    service: AcpService,
    outbound: mpsc::UnboundedSender<ServiceMessage>,
    command_subscription: Option<JoinHandle<()>>,
    session_forwarders: HashMap<String, JoinHandle<()>>,
    catalog_tasks: HashMap<u64, JoinHandle<()>>,
}

impl AcpServiceConnection {
    fn subscribe_agent_commands(&mut self) {
        if let Some(task) = self.command_subscription.take() {
            task.abort();
        }
        let mut messages = self.service.outbound.subscribe();
        let outbound = self.outbound.clone();
        self.command_subscription = Some(tokio::spawn(async move {
            loop {
                match messages.recv().await {
                    Ok(message) => {
                        if outbound.send(message).is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                        tracing::warn!(dropped, "agent stream lagged; frames were dropped");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }));
    }

    fn forward_agent_request(
        &self,
        request_id: AgentRequestId,
        anchor: Option<vmux_ecs::ProcessId>,
        request: AgentRequest,
    ) {
        let broker = self.service.broker.clone();
        let outbound = self.outbound.clone();
        tokio::spawn(async move {
            let message = match broker.command(request_id, anchor, request).await {
                Ok(result) => ServiceMessage::AgentCommandResult { request_id, result },
                Err(message) => ServiceMessage::Error { message },
            };
            let _ = outbound.send(message);
        });
    }

    fn forward_agent_query(&self, request_id: AgentRequestId, query: AgentRequest) {
        let service = self.service.clone();
        let broker = self.service.broker.clone();
        let outbound = self.outbound.clone();
        tokio::spawn(async move {
            let message = match service.process_response(request_id, &query).await {
                Ok(Some(message)) => message,
                Ok(None) => match broker.query(request_id, query).await {
                    Ok(message) => message,
                    Err(message) => ServiceMessage::Error { message },
                },
                Err(message) => ServiceMessage::Error { message },
            };
            let _ = outbound.send(message);
        });
    }

    async fn resolve_query_result(&self, result: AgentQueryResult) {
        let request_id = result.request_id;
        let message = ServiceMessage::AgentQueryResult(result);
        if !self
            .service
            .broker
            .resolve_query(request_id, message.clone())
            .await
            && let Ok(content) = AgentContent::try_from(message)
        {
            self.service
                .broker
                .resolve_tool(request_id, content.content, content.is_error)
                .await;
        }
    }

    async fn resolve_command_response(
        &self,
        request_id: AgentRequestId,
        result: AgentCommandResult,
    ) {
        if !self
            .service
            .broker
            .resolve_command(request_id, result.clone())
            .await
        {
            let content = AgentContent::from(result);
            self.service
                .broker
                .resolve_tool(request_id, content.content, content.is_error)
                .await;
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn spawn_acp_agent(
        &mut self,
        sid: String,
        agent_id: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: String,
        anchor: vmux_ecs::ProcessId,
        mcp_command: Option<String>,
        mcp_args: Vec<String>,
        resume_acp_session_id: Option<String>,
    ) {
        if let Err(message) = self
            .service
            .sessions
            .spawn(
                sid.clone(),
                agent_id,
                command,
                args,
                env,
                PathBuf::from(cwd),
                anchor,
                self.service.processes.clone(),
                mcp_command,
                mcp_args,
                resume_acp_session_id,
            )
            .await
        {
            let _ = self.outbound.send(ServiceMessage::Error { message });
            return;
        }
        self.attach(sid, false).await;
    }

    #[allow(clippy::too_many_arguments)]
    fn list_acp_sessions(
        &mut self,
        request_id: u64,
        agent_id: String,
        agent_name: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: String,
    ) {
        if let Some(task) = self.catalog_tasks.remove(&request_id) {
            task.abort();
        }
        let outbound = self.outbound.clone();
        let task = tokio::spawn(async move {
            let result = CatalogDriver::list(CatalogRequest {
                command,
                args,
                env,
                cwd: PathBuf::from(cwd),
            })
            .await;
            let (supported, sessions, error) = match result {
                Ok(result) => (result.supported, result.sessions, None),
                Err(error) => (false, Vec::new(), Some(error)),
            };
            let _ = outbound.send(ServiceMessage::AcpSessionCatalog {
                request_id,
                agent_id,
                agent_name,
                supported,
                sessions,
                error,
            });
        });
        self.catalog_tasks.insert(request_id, task);
    }

    async fn dispatch_shared(&mut self, message: SharedMessage) {
        match message {
            SharedMessage::ListSessions
            | SharedMessage::AgentNewChat { .. }
            | SharedMessage::AgentListAgents
            | SharedMessage::AgentListTeam
            | SharedMessage::AgentListModels { .. }
            | SharedMessage::AgentSelectModel { .. }
            | SharedMessage::AgentSetEffort { .. }
            | SharedMessage::AgentListMedia { .. } => {
                tracing::warn!("local socket: ignoring a remote-only request");
            }
            SharedMessage::AgentAttach { sid } => self.attach(sid, true).await,
            SharedMessage::AgentInput {
                sid,
                text,
                context,
                attachments,
                preferred_mode,
            } => {
                self.service
                    .sessions
                    .send_prompt(sid, text, context, attachments, preferred_mode)
                    .await;
            }
            SharedMessage::AgentCancel { sid } => {
                self.service.sessions.cancel(sid).await;
            }
            SharedMessage::AgentApprove {
                sid,
                call_id,
                decision,
            } => {
                self.service.sessions.approve(sid, call_id, decision).await;
            }
        }
    }

    async fn rebind_acp_workspace(&self, sid: String, cwd: String) {
        if let Err(message) = self
            .service
            .sessions
            .rebind_cwd(sid, PathBuf::from(cwd))
            .await
        {
            let _ = self.outbound.send(ServiceMessage::Error { message });
        }
    }

    async fn dispatch(&mut self, message: ClientMessage) -> Result<(), ClientMessage> {
        self.catalog_tasks.retain(|_, task| !task.is_finished());
        match message {
            ClientMessage::SubscribeAgentCommands => self.subscribe_agent_commands(),
            ClientMessage::AgentRequest {
                request_id,
                anchor,
                request,
            } => self.forward_agent_request(request_id, anchor, request),
            ClientMessage::AgentQuery { request_id, query } => {
                self.forward_agent_query(request_id, query);
            }
            ClientMessage::AgentQueryResult(result) => self.resolve_query_result(result).await,
            ClientMessage::AgentCommandResponse { request_id, result } => {
                self.resolve_command_response(request_id, result).await;
            }
            ClientMessage::Shared(shared) => self.dispatch_shared(shared).await,
            ClientMessage::DetachAgentSession { sid } => self.detach(&sid),
            ClientMessage::RebindAcpWorkspace { sid, cwd } => {
                self.rebind_acp_workspace(sid, cwd).await;
            }
            ClientMessage::AcpSetSessionConfig {
                sid,
                request_id,
                config_id,
                value,
            } => {
                self.service
                    .sessions
                    .set_config(sid, request_id, config_id, value)
                    .await;
            }
            ClientMessage::CloseAgentSession { sid } => {
                self.service.sessions.close(sid.clone()).await;
                self.detach(&sid);
            }
            ClientMessage::AgentToolResult {
                request_id,
                content,
                is_error,
            } => {
                self.service
                    .broker
                    .resolve_tool(request_id, content, is_error)
                    .await;
            }
            ClientMessage::SpawnAcpAgent {
                sid,
                agent_id,
                command,
                args,
                env,
                cwd,
                anchor,
                mcp_command,
                mcp_args,
                resume_acp_session_id,
            } => {
                self.spawn_acp_agent(
                    sid,
                    agent_id,
                    command,
                    args,
                    env,
                    cwd,
                    anchor,
                    mcp_command,
                    mcp_args,
                    resume_acp_session_id,
                )
                .await;
            }
            ClientMessage::ListAcpSessions {
                request_id,
                agent_id,
                agent_name,
                command,
                args,
                env,
                cwd,
            } => self.list_acp_sessions(request_id, agent_id, agent_name, command, args, env, cwd),
            ClientMessage::CancelAcpSessionCatalog { request_id } => {
                if let Some(task) = self.catalog_tasks.remove(&request_id) {
                    task.abort();
                }
            }
            other => return Err(other),
        }
        Ok(())
    }

    async fn attach(&mut self, sid: String, include_status: bool) {
        let Some(mut messages) = self.service.sessions.subscribe(sid.clone()).await else {
            return;
        };
        let mut snapshots = vec![
            self.service.sessions.snapshot(sid.clone()).await,
            self.service.sessions.agent_info(sid.clone()).await,
            self.service.sessions.config_state(sid.clone()).await,
        ];
        if include_status {
            snapshots.push(self.service.sessions.status(sid.clone()).await);
        }
        for message in snapshots.into_iter().flatten() {
            if self.outbound.send(message).is_err() {
                return;
            }
        }
        self.detach(&sid);
        let outbound = self.outbound.clone();
        let task = tokio::spawn(async move {
            loop {
                match messages.recv().await {
                    Ok(message) => {
                        if outbound.send(message).is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                        tracing::warn!(dropped, "service stream lagged; frames were dropped");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        self.session_forwarders.insert(sid, task);
    }

    fn detach(&mut self, sid: &str) {
        if let Some(task) = self.session_forwarders.remove(sid) {
            task.abort();
        }
    }
}

impl ServiceProtocolConnection for AcpServiceConnection {
    fn dispatch(&mut self, message: ClientMessage) -> RemoteFuture<'_, Result<(), ClientMessage>> {
        match message {
            ClientMessage::SpawnAcpAgentWithMcp(spawn) => Box::pin(async move {
                let vmux_api::protocol::AcpSpawnWithMcp {
                    sid,
                    agent_id,
                    command,
                    args,
                    env,
                    cwd,
                    anchor,
                    mcp_command,
                    mcp_args,
                    managed_mcp_servers,
                    resume_acp_session_id,
                } = *spawn;
                if let Err(message) = self
                    .service
                    .sessions
                    .spawn_with_mcp(
                        sid.clone(),
                        agent_id,
                        command,
                        args,
                        env,
                        PathBuf::from(cwd),
                        anchor,
                        self.service.processes.clone(),
                        mcp_command,
                        mcp_args,
                        managed_mcp_servers,
                        resume_acp_session_id,
                    )
                    .await
                {
                    let _ = self.outbound.send(ServiceMessage::Error { message });
                    return Ok(());
                }
                self.attach(sid, false).await;
                Ok(())
            }),
            ClientMessage::RebindAcpWorkspaceWithReply {
                request_id,
                sid,
                cwd,
            } => Box::pin(async move {
                let result = self
                    .service
                    .sessions
                    .rebind_cwd(sid.clone(), PathBuf::from(&cwd))
                    .await;
                let (cwd, error) = match result {
                    Ok(cwd) => (cwd.to_string_lossy().into_owned(), None),
                    Err(error) => (cwd, Some(error)),
                };
                let _ = self
                    .outbound
                    .send(ServiceMessage::AcpWorkspaceRebindResult {
                        request_id,
                        sid,
                        cwd,
                        error,
                    });
                Ok(())
            }),
            other => Box::pin(AcpServiceConnection::dispatch(self, other)),
        }
    }
}

impl Drop for AcpServiceConnection {
    fn drop(&mut self) {
        if let Some(task) = self.command_subscription.take() {
            task.abort();
        }
        for (_, task) in self.session_forwarders.drain() {
            task.abort();
        }
        for (_, task) in self.catalog_tasks.drain() {
            task.abort();
        }
    }
}

struct AgentContent {
    content: String,
    is_error: bool,
}

impl From<AgentCommandResult> for AgentContent {
    fn from(result: AgentCommandResult) -> Self {
        match result {
            AgentCommandResult::Ok => Self {
                content: "ok".to_string(),
                is_error: false,
            },
            AgentCommandResult::Text(content) => Self {
                content,
                is_error: false,
            },
            AgentCommandResult::Error(content) => Self {
                content,
                is_error: true,
            },
        }
    }
}

impl TryFrom<ServiceMessage> for AgentContent {
    type Error = ();

    fn try_from(message: ServiceMessage) -> Result<Self, Self::Error> {
        let ServiceMessage::AgentQueryResult(result) = message else {
            return Err(());
        };
        Ok(Self {
            content: result.content,
            is_error: result.is_error,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, MinimalPlugins};

    #[tokio::test]
    async fn checked_rebind_reports_a_correlated_failure_from_the_native_runtime() {
        let (wake, _wake_receiver) = mpsc::unbounded_channel();
        let (processes, _process_runtime) = ProcessRuntime::new(wake.clone());
        let (service, native) = AcpService::new(Handle::current(), wake, processes);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::service::AcpServicePlugin));
        app.world_mut().spawn((service.clone(), native));
        let (outbound, mut received) = mpsc::unbounded_channel();
        let mut connection = service.connection(outbound);
        let request_id = AgentRequestId::new();
        let (mut client, mut server) = tokio::io::duplex(4096);
        vmux_transport::service::ServiceCodec::write_client(
            &mut client,
            &ClientMessage::RebindAcpWorkspaceWithReply {
                request_id,
                sid: "missing-session".into(),
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            },
        )
        .await
        .unwrap();
        let request = vmux_transport::service::ServiceCodec::read_client(&mut server)
            .await
            .unwrap()
            .unwrap();
        let task = tokio::spawn(async move {
            ServiceProtocolConnection::dispatch(&mut connection, request)
                .await
                .unwrap();
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            app.update();
            if let Ok(message) = received.try_recv() {
                vmux_transport::service::ServiceCodec::write_service(&mut server, &message)
                    .await
                    .unwrap();
                let message = vmux_transport::service::ServiceCodec::read_service(&mut client)
                    .await
                    .unwrap()
                    .unwrap();
                let ServiceMessage::AcpWorkspaceRebindResult {
                    request_id: actual,
                    sid,
                    error,
                    ..
                } = message
                else {
                    panic!("checked rebind returned an uncorrelated response");
                };
                assert_eq!(actual, request_id);
                assert_eq!(sid, "missing-session");
                assert!(error.is_some_and(|error| error.contains("not found")));
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "native rebind did not return its failure"
            );
            tokio::task::yield_now().await;
        }
        task.await.unwrap();
    }
}
