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
    RemoteDriver, RemoteFuture, RemoteOperationStore, ServiceProtocolConnection,
    ServiceProtocolDriver,
};

use crate::acp::{AcpInput, AcpSessions};
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
        }
    }

    pub(crate) fn remote_driver(
        &self,
        operations: Arc<dyn RemoteOperationStore>,
    ) -> Arc<dyn RemoteDriver> {
        Arc::new(AgentRemoteDriver::new(
            self.sessions.clone(),
            self.broker.clone(),
            operations,
        ))
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
}

impl AcpServiceConnection {
    async fn dispatch(&mut self, message: ClientMessage) -> Result<(), ClientMessage> {
        match message {
            ClientMessage::SubscribeAgentCommands => {
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
            ClientMessage::AgentRequest {
                request_id,
                anchor,
                request,
            } => {
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
            ClientMessage::AgentQuery { request_id, query } => {
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
            ClientMessage::AgentQueryResult(result) => {
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
            ClientMessage::AgentCommandResponse { request_id, result } => {
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
            ClientMessage::Shared(
                SharedMessage::ListSessions
                | SharedMessage::AgentNewChat { .. }
                | SharedMessage::AgentListAgents
                | SharedMessage::AgentListTeam
                | SharedMessage::AgentListModels { .. }
                | SharedMessage::AgentSelectModel { .. }
                | SharedMessage::AgentSetEffort { .. }
                | SharedMessage::AgentListMedia { .. },
            ) => {
                tracing::warn!("local socket: ignoring a remote-only request");
            }
            ClientMessage::Shared(SharedMessage::AgentAttach { sid }) => {
                self.attach(sid).await;
            }
            ClientMessage::DetachAgentSession { sid } => {
                self.detach(&sid);
            }
            ClientMessage::Shared(SharedMessage::AgentInput {
                sid,
                text,
                context,
                attachments,
                preferred_mode,
            }) => {
                self.service
                    .sessions
                    .input(
                        sid,
                        AcpInput::User {
                            text,
                            context,
                            attachments,
                            preferred_mode,
                        },
                    )
                    .await;
            }
            ClientMessage::RebindAcpWorkspace { sid, cwd } => {
                if let Err(message) = self
                    .service
                    .sessions
                    .rebind_cwd(sid, PathBuf::from(cwd))
                    .await
                {
                    let _ = self.outbound.send(ServiceMessage::Error { message });
                }
            }
            ClientMessage::AcpSetSessionConfig {
                sid,
                request_id,
                config_id,
                value,
            } => {
                self.service
                    .sessions
                    .input(
                        sid,
                        AcpInput::SetConfig {
                            request_id,
                            config_id,
                            value,
                        },
                    )
                    .await;
            }
            ClientMessage::Shared(SharedMessage::AgentCancel { sid }) => {
                self.service.sessions.input(sid, AcpInput::Cancel).await;
            }
            ClientMessage::Shared(SharedMessage::AgentApprove {
                sid,
                call_id,
                decision,
            }) => {
                self.service
                    .sessions
                    .input(sid, AcpInput::Approve { call_id, decision })
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
                managed_mcp_servers,
            } => {
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
                        managed_mcp_servers,
                        resume_acp_session_id,
                    )
                    .await
                {
                    let _ = self.outbound.send(ServiceMessage::Error { message });
                    return Ok(());
                }
                self.attach(sid).await;
            }
            other => return Err(other),
        }
        Ok(())
    }

    async fn attach(&mut self, sid: String) {
        let Some(mut messages) = self.service.sessions.subscribe(sid.clone()).await else {
            return;
        };
        for message in [
            self.service.sessions.snapshot(sid.clone()).await,
            self.service.sessions.agent_info(sid.clone()).await,
            self.service.sessions.config_state(sid.clone()).await,
            self.service.sessions.status(sid.clone()).await,
        ]
        .into_iter()
        .flatten()
        {
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
        Box::pin(AcpServiceConnection::dispatch(self, message))
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
