use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::sync::{broadcast, mpsc};
use vmux_api::protocol::{ClientMessage, ProcessId, ServiceMessage, SharedMessage};
use vmux_process::{ProcessLaunch, ProcessRuntime};
use vmux_transport::service::ServiceCodec;

use crate::remote::authorization::RemoteAuthorizations;
use crate::remote::client_operation::ClientOperations;
use vmux_agent::acp::AcpSessions;
use vmux_agent::broker::AgentBroker;

pub struct ServiceDaemonPlugin;

impl ServiceDaemonPlugin {
    pub fn runtime(
        listener: UnixListener,
        wake: mpsc::UnboundedSender<()>,
        runtime: tokio::runtime::Handle,
        exit: mpsc::Sender<()>,
    ) -> impl Bundle {
        let (processes, process_runtime) = ProcessRuntime::new(wake.clone());
        let (client_operations, client_operation_runtime) = ClientOperations::new(wake.clone());
        let (authorizations, authorization_runtime) = RemoteAuthorizations::new(wake.clone());
        let (acp_sessions, acp_session_runtime) = AcpSessions::new(runtime.clone(), wake.clone());
        let (agent_tx, _) = broadcast::channel::<ServiceMessage>(128);
        let broker = AgentBroker::new(
            agent_tx.clone(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let remote_runtime = crate::remote::server::RemoteRuntimeStartup::new(
            runtime.clone(),
            authorizations,
            acp_sessions.clone(),
            broker.clone(),
            client_operations,
        );
        (
            Name::new("vmux service runtime"),
            ServiceDaemonStartup(Some(ServiceDaemonStart {
                listener,
                processes,
                acp_sessions,
                agent_tx,
                broker,
                wake: wake.clone(),
                runtime,
                exit,
            })),
            process_runtime,
            client_operation_runtime,
            authorization_runtime,
            acp_session_runtime,
            remote_runtime,
        )
    }
}

impl Plugin for ServiceDaemonPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_process::ProcessPlugin,
            crate::remote::RemotePlugin,
            vmux_agent::acp::AcpSessionPlugin,
        ))
        .add_systems(Startup, start_daemon)
        .add_systems(Update, (start_clients, reap_clients).chain());
    }
}

#[derive(Component)]
struct ServiceDaemonStartup(Option<ServiceDaemonStart>);

struct ServiceDaemonStart {
    listener: UnixListener,
    processes: ProcessRuntime,
    acp_sessions: AcpSessions,
    agent_tx: broadcast::Sender<ServiceMessage>,
    broker: AgentBroker,
    wake: mpsc::UnboundedSender<()>,
    runtime: tokio::runtime::Handle,
    exit: mpsc::Sender<()>,
}

#[derive(Component)]
struct ServiceDaemon;

#[derive(Component, Clone, Copy)]
struct ServiceStartedAt(Instant);

#[derive(Component)]
struct ServiceListenerTask(tokio::task::JoinHandle<()>);

impl Drop for ServiceListenerTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Component, Clone)]
struct ServiceClientRuntime {
    runtime: tokio::runtime::Handle,
    processes: ProcessRuntime,
    agent_tx: broadcast::Sender<ServiceMessage>,
    broker: AgentBroker,
    acp_sessions: AcpSessions,
    shutdown: mpsc::Sender<()>,
    wake: mpsc::UnboundedSender<()>,
    started_at: ServiceStartedAt,
}

#[derive(Component)]
struct ServiceConnectionInbox(mpsc::UnboundedReceiver<tokio::net::UnixStream>);

#[derive(Component)]
struct ServiceClient;

#[derive(Component)]
struct ServiceClientTask(tokio::task::JoinHandle<()>);

impl Drop for ServiceClientTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn start_daemon(mut startups: Query<(Entity, &mut ServiceDaemonStartup)>, mut commands: Commands) {
    for (entity, mut startup) in &mut startups {
        let Some(start) = startup.0.take() else {
            continue;
        };
        let started_at = ServiceStartedAt(Instant::now());
        let (connections, connection_inbox) = mpsc::unbounded_channel();
        let (shutdown, shutdown_inbox) = mpsc::channel(1);
        let listener = ServiceListener::new(
            start.listener,
            connections,
            start.wake.clone(),
            shutdown_inbox,
            start.exit,
        );
        let task = start.runtime.spawn(listener.run());
        commands
            .entity(entity)
            .remove::<ServiceDaemonStartup>()
            .insert((
                ServiceDaemon,
                started_at,
                ServiceClientRuntime {
                    runtime: start.runtime,
                    processes: start.processes,
                    agent_tx: start.agent_tx,
                    broker: start.broker,
                    acp_sessions: start.acp_sessions,
                    shutdown,
                    wake: start.wake,
                    started_at,
                },
                ServiceConnectionInbox(connection_inbox),
                ServiceListenerTask(task),
            ));
    }
}

struct ServiceListener {
    listener: UnixListener,
    connections: mpsc::UnboundedSender<tokio::net::UnixStream>,
    wake: mpsc::UnboundedSender<()>,
    shutdown: mpsc::Receiver<()>,
    exit: mpsc::Sender<()>,
}

impl ServiceListener {
    fn new(
        listener: UnixListener,
        connections: mpsc::UnboundedSender<tokio::net::UnixStream>,
        wake: mpsc::UnboundedSender<()>,
        shutdown: mpsc::Receiver<()>,
        exit: mpsc::Sender<()>,
    ) -> Self {
        Self {
            listener,
            connections,
            wake,
            shutdown,
            exit,
        }
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                accepted = self.listener.accept() => {
                    let (stream, _) = match accepted {
                        Ok(connection) => connection,
                        Err(error) => {
                            tracing::error!(%error, "accept error");
                            continue;
                        }
                    };
                    if self.connections.send(stream).is_err() {
                        break;
                    }
                    let _ = self.wake.send(());
                }
                _ = self.shutdown.recv() => {
                    tracing::info!("server: drain signaled, closing listener");
                    break;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        tracing::info!("server: drain complete, exiting");
        let _ = self.exit.send(()).await;
    }
}

fn start_clients(
    runtime: Single<&ServiceClientRuntime, With<ServiceDaemon>>,
    mut inbox: Single<&mut ServiceConnectionInbox, With<ServiceDaemon>>,
    mut commands: Commands,
) {
    while let Ok(stream) = inbox.0.try_recv() {
        let client = (*runtime).clone();
        let executor = client.runtime.clone();
        let wake = client.wake.clone();
        let connection = ServiceConnection::new(stream, client);
        let task = executor.spawn(async move {
            if let Err(error) = connection.run().await {
                tracing::error!(%error, "client error");
            }
            let _ = wake.send(());
        });
        commands.spawn((
            Name::new("service client"),
            ServiceClient,
            ServiceClientTask(task),
        ));
    }
}

fn reap_clients(
    clients: Query<(Entity, &ServiceClientTask), With<ServiceClient>>,
    mut commands: Commands,
) {
    for (entity, task) in &clients {
        if task.0.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

struct AgentContent {
    content: String,
    is_error: bool,
}

impl From<vmux_api::protocol::AgentCommandResult> for AgentContent {
    fn from(result: vmux_api::protocol::AgentCommandResult) -> Self {
        use vmux_api::protocol::AgentCommandResult;
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

    fn try_from(response: ServiceMessage) -> Result<Self, Self::Error> {
        let ServiceMessage::AgentQueryResult(result) = response else {
            return Err(());
        };
        Ok(Self {
            content: result.content,
            is_error: result.is_error,
        })
    }
}

struct AgentQueryResponse {
    request_id: vmux_api::protocol::AgentRequestId,
    response: ServiceMessage,
}

impl AgentQueryResponse {
    async fn resolve(self, broker: &AgentBroker) {
        if broker
            .resolve_query(self.request_id, self.response.clone())
            .await
        {
            return;
        }
        if let Ok(content) = AgentContent::try_from(self.response) {
            broker
                .resolve_tool(self.request_id, content.content, content.is_error)
                .await;
        }
    }
}

struct ServiceConnection {
    stream: tokio::net::UnixStream,
    runtime: ServiceClientRuntime,
}

impl ServiceConnection {
    fn new(stream: tokio::net::UnixStream, runtime: ServiceClientRuntime) -> Self {
        Self { stream, runtime }
    }

    async fn run(self) -> std::io::Result<()> {
        let Self { stream, runtime } = self;
        let ServiceClientRuntime {
            processes,
            agent_tx,
            broker,
            acp_sessions,
            shutdown: shutdown_tx,
            started_at,
            ..
        } = runtime;
        let (reader, writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let writer = Arc::new(tokio::sync::Mutex::new(writer));

        let attached: Arc<tokio::sync::Mutex<HashMap<ProcessId, tokio::task::JoinHandle<()>>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let mut agent_subscription: Option<tokio::task::JoinHandle<()>> = None;
        let mut agent_forwarders: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();

        let mut created_processes: Vec<ProcessId> = Vec::new();

        loop {
            let msg: Option<ClientMessage> = match ServiceCodec::read_client(&mut reader).await {
                Ok(msg) => msg,
                Err(error) => {
                    tracing::warn!(%error, "client stream ended mid-frame");
                    break;
                }
            };
            let Some(msg) = msg else {
                break;
            };

            match msg {
                ClientMessage::CreateProcess {
                    process_id,
                    command,
                    args,
                    cwd,
                    env,
                    cols,
                    rows,
                } => {
                    let created = processes
                        .create(ProcessLaunch {
                            id: process_id,
                            command,
                            args,
                            cwd,
                            env,
                            cols,
                            rows,
                            keep_after_exit: false,
                        })
                        .await;
                    match created {
                        Ok(created) => {
                            created_processes.push(created.id);
                            let resp = ServiceMessage::ProcessCreated {
                                process_id: created.id,
                                pid: created.pid,
                            };
                            let w = writer.clone();
                            let mut w = w.lock().await;
                            ServiceCodec::write_service(&mut *w, &resp).await?;
                        }
                        Err(reason) => {
                            let resp = ServiceMessage::ProcessCreateFailed { process_id, reason };
                            let w = writer.clone();
                            let mut w = w.lock().await;
                            ServiceCodec::write_service(&mut *w, &resp).await?;
                        }
                    }
                }

                ClientMessage::AttachProcess { process_id } => {
                    if let Ok(mut rx) = processes.subscribe(process_id).await {
                        let w = writer.clone();
                        let handle = tokio::spawn(async move {
                            loop {
                                match rx.recv().await {
                                    Ok(update) => {
                                        let msg = update.into_service_message(process_id);
                                        let bytes =
                                            match rkyv::to_bytes::<rkyv::rancor::Error>(&msg) {
                                                Ok(b) => b,
                                                Err(_) => break,
                                            };
                                        let mut w = w.lock().await;
                                        if ServiceCodec::write_raw(&mut *w, &bytes).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                                        tracing::warn!(
                                            dropped,
                                            "service stream lagged; frames were dropped"
                                        );
                                        continue;
                                    }
                                    Err(broadcast::error::RecvError::Closed) => break,
                                }
                            }
                        });
                        attached.lock().await.insert(process_id, handle);
                    } else {
                        let resp = ServiceMessage::Error {
                            message: format!("process not found: {process_id}"),
                        };
                        let mut w = writer.lock().await;
                        ServiceCodec::write_service(&mut *w, &resp).await?;
                    }
                }

                ClientMessage::DetachProcess { process_id } => {
                    if let Some(handle) = attached.lock().await.remove(&process_id) {
                        handle.abort();
                    }
                }

                ClientMessage::ProcessInput { process_id, data } => {
                    let _ = processes.input(process_id, data).await;
                }

                ClientMessage::MouseWheel {
                    process_id,
                    up,
                    col,
                    row,
                    modifiers,
                } => {
                    let _ = processes
                        .mouse_wheel(process_id, up, col, row, modifiers)
                        .await;
                }

                ClientMessage::ScrollWindow {
                    process_id,
                    top_row,
                    follow,
                } => {
                    let _ = processes.scroll_window(process_id, top_row, follow).await;
                }

                ClientMessage::ResizeProcess {
                    process_id,
                    cols,
                    rows,
                } => {
                    let _ = processes.resize(process_id, cols, rows).await;
                }

                ClientMessage::ListProcesses => {
                    let process_list = processes.list().await.unwrap_or_default();
                    let resp = ServiceMessage::ProcessList {
                        processes: process_list,
                    };
                    let mut w = writer.lock().await;
                    ServiceCodec::write_service(&mut *w, &resp).await?;
                }

                ClientMessage::KillProcess { process_id } => {
                    let _ = processes.remove(process_id).await;
                    if let Some(handle) = attached.lock().await.remove(&process_id) {
                        handle.abort();
                    }
                }

                ClientMessage::RequestSnapshot { process_id } => {
                    if let Ok(snapshot) = processes.snapshot(process_id).await {
                        let snap = snapshot.into_service_message(process_id);
                        let mut w = writer.lock().await;
                        ServiceCodec::write_service(&mut *w, &snap).await?;
                    } else {
                        let resp = ServiceMessage::Error {
                            message: format!("process not found: {process_id}"),
                        };
                        let mut w = writer.lock().await;
                        ServiceCodec::write_service(&mut *w, &resp).await?;
                    }
                }

                ClientMessage::SetSelection { process_id, range } => {
                    let _ = processes.set_selection(process_id, range).await;
                }

                ClientMessage::ExtendSelectionTo {
                    process_id,
                    col,
                    row,
                } => {
                    let _ = processes.extend_selection(process_id, col, row).await;
                }

                ClientMessage::SelectWordAt {
                    process_id,
                    col,
                    row,
                } => {
                    let _ = processes.select_word(process_id, col, row).await;
                }

                ClientMessage::SelectLineAt { process_id, row } => {
                    let _ = processes.select_line(process_id, row).await;
                }

                ClientMessage::GetSelectionText { process_id } => {
                    let text = processes
                        .selection_text(process_id)
                        .await
                        .unwrap_or_default();
                    let resp = ServiceMessage::SelectionText { process_id, text };
                    let mut w = writer.lock().await;
                    ServiceCodec::write_service(&mut *w, &resp).await?;
                }

                ClientMessage::EnterCopyMode { process_id } => {
                    let _ = processes.enter_copy_mode(process_id).await;
                }

                ClientMessage::ExitCopyMode { process_id } => {
                    let _ = processes.exit_copy_mode(process_id).await;
                }

                ClientMessage::CopyModeKey { process_id, key } => {
                    if let Ok(Some(text)) = processes.copy_mode_key(process_id, key).await {
                        let resp = ServiceMessage::SelectionText { process_id, text };
                        let mut w = writer.lock().await;
                        ServiceCodec::write_service(&mut *w, &resp).await?;
                    }
                }

                ClientMessage::SubscribeAgentCommands => {
                    if let Some(handle) = agent_subscription.take() {
                        handle.abort();
                    }
                    let mut rx = agent_tx.subscribe();
                    let w = writer.clone();
                    agent_subscription = Some(tokio::spawn(async move {
                        loop {
                            match rx.recv().await {
                                Ok(msg) => {
                                    let bytes = match rkyv::to_bytes::<rkyv::rancor::Error>(&msg) {
                                        Ok(b) => b,
                                        Err(_) => break,
                                    };
                                    let mut w = w.lock().await;
                                    if ServiceCodec::write_raw(&mut *w, &bytes).await.is_err() {
                                        break;
                                    }
                                }
                                Err(broadcast::error::RecvError::Lagged(dropped)) => {
                                    tracing::warn!(
                                        dropped,
                                        "agent stream lagged; frames were dropped"
                                    );
                                    continue;
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
                    let broker = broker.clone();
                    let writer = writer.clone();
                    tokio::spawn(async move {
                        let resp = match broker.command(request_id, anchor, request).await {
                            Ok(result) => ServiceMessage::AgentCommandResult { request_id, result },
                            Err(message) => ServiceMessage::Error { message },
                        };
                        let bytes = match rkyv::to_bytes::<rkyv::rancor::Error>(&resp) {
                            Ok(b) => b,
                            Err(_) => return,
                        };
                        let mut w = writer.lock().await;
                        let _ = ServiceCodec::write_raw(&mut *w, &bytes).await;
                    });
                }

                ClientMessage::Shutdown => {
                    tracing::info!("shutdown requested by client; draining");
                    let _ = processes.shutdown().await;
                    let resp = ServiceMessage::ProcessList {
                        processes: Vec::new(),
                    };
                    let mut w = writer.lock().await;
                    ServiceCodec::write_service(&mut *w, &resp).await?;
                    shutdown_tx.send(()).await.ok();
                    break;
                }

                ClientMessage::Status => {
                    let uptime_secs = started_at.0.elapsed().as_secs();
                    let process_count = processes.count().await.unwrap_or_default();
                    let resp = ServiceMessage::StatusResponse {
                        uptime_secs,
                        process_count,
                    };
                    let mut w = writer.lock().await;
                    ServiceCodec::write_service(&mut *w, &resp).await?;
                }

                ClientMessage::AgentQuery { request_id, query } => {
                    let response = match processes.response(request_id, &query).await {
                        Ok(Some(response)) => response,
                        Ok(None) => {
                            let broker = broker.clone();
                            let writer = writer.clone();
                            tokio::spawn(async move {
                                let response = match broker.query(request_id, query).await {
                                    Ok(response) => response,
                                    Err(message) => ServiceMessage::Error { message },
                                };
                                let bytes = match rkyv::to_bytes::<rkyv::rancor::Error>(&response) {
                                    Ok(bytes) => bytes,
                                    Err(_) => return,
                                };
                                let mut writer = writer.lock().await;
                                let _ = ServiceCodec::write_raw(&mut *writer, &bytes).await;
                            });
                            continue;
                        }
                        Err(message) => ServiceMessage::Error { message },
                    };
                    let mut writer = writer.lock().await;
                    ServiceCodec::write_service(&mut *writer, &response).await?;
                }

                ClientMessage::AgentQueryResult(result) => {
                    let request_id = result.request_id;
                    AgentQueryResponse {
                        request_id,
                        response: ServiceMessage::AgentQueryResult(result),
                    }
                    .resolve(&broker)
                    .await;
                }
                ClientMessage::AgentCommandResponse { request_id, result } => {
                    if !broker.resolve_command(request_id, result.clone()).await {
                        let content = AgentContent::from(result);
                        broker
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
                    let rx = acp_sessions.subscribe(sid.clone()).await;
                    if let Some(mut rx) = rx {
                        if let Some(snapshot) = acp_sessions.snapshot(sid.clone()).await {
                            let mut w = writer.lock().await;
                            ServiceCodec::write_service(&mut *w, &snapshot).await?;
                        }
                        if let Some(agent_info) = acp_sessions.agent_info(sid.clone()).await {
                            let mut w = writer.lock().await;
                            ServiceCodec::write_service(&mut *w, &agent_info).await?;
                        }
                        if let Some(config_state) = acp_sessions.config_state(sid.clone()).await {
                            let mut w = writer.lock().await;
                            ServiceCodec::write_service(&mut *w, &config_state).await?;
                        }
                        if let Some(old) = agent_forwarders.remove(&sid) {
                            old.abort();
                        }
                        let w = writer.clone();
                        let handle = tokio::spawn(async move {
                            loop {
                                match rx.recv().await {
                                    Ok(msg) => {
                                        let bytes =
                                            match rkyv::to_bytes::<rkyv::rancor::Error>(&msg) {
                                                Ok(b) => b,
                                                Err(_) => break,
                                            };
                                        let mut w = w.lock().await;
                                        if ServiceCodec::write_raw(&mut *w, &bytes).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                                        tracing::warn!(
                                            dropped,
                                            "service stream lagged; frames were dropped"
                                        );
                                        continue;
                                    }
                                    Err(broadcast::error::RecvError::Closed) => break,
                                }
                            }
                        });
                        agent_forwarders.insert(sid, handle);
                    }
                }

                ClientMessage::DetachAgentSession { sid } => {
                    if let Some(handle) = agent_forwarders.remove(&sid) {
                        handle.abort();
                    }
                }

                ClientMessage::Shared(SharedMessage::AgentInput {
                    sid,
                    text,
                    context,
                    attachments,
                    preferred_mode,
                }) => {
                    acp_sessions
                        .input(
                            sid,
                            vmux_agent::acp::AcpInput::User {
                                text,
                                context,
                                attachments,
                                preferred_mode,
                            },
                        )
                        .await;
                }

                ClientMessage::RebindAcpWorkspace { sid, cwd } => {
                    if let Err(message) = acp_sessions
                        .rebind_cwd(sid, std::path::PathBuf::from(cwd))
                        .await
                    {
                        let resp = ServiceMessage::Error { message };
                        let mut w = writer.lock().await;
                        ServiceCodec::write_service(&mut *w, &resp).await?;
                    }
                }

                ClientMessage::AcpSetSessionConfig {
                    sid,
                    request_id,
                    config_id,
                    value,
                } => {
                    acp_sessions
                        .input(
                            sid,
                            vmux_agent::acp::AcpInput::SetConfig {
                                request_id,
                                config_id,
                                value,
                            },
                        )
                        .await;
                }

                ClientMessage::Shared(SharedMessage::AgentCancel { sid }) => {
                    acp_sessions
                        .input(sid, vmux_agent::acp::AcpInput::Cancel)
                        .await;
                }

                ClientMessage::Shared(SharedMessage::AgentApprove {
                    sid,
                    call_id,
                    decision,
                }) => {
                    acp_sessions
                        .input(
                            sid,
                            vmux_agent::acp::AcpInput::Approve { call_id, decision },
                        )
                        .await;
                }

                ClientMessage::CloseAgentSession { sid } => {
                    acp_sessions.close(sid.clone()).await;
                    if let Some(handle) = agent_forwarders.remove(&sid) {
                        handle.abort();
                    }
                }

                ClientMessage::AgentToolResult {
                    request_id,
                    content,
                    is_error,
                } => {
                    broker.resolve_tool(request_id, content, is_error).await;
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
                    if let Err(message) = acp_sessions
                        .spawn(
                            sid.clone(),
                            agent_id,
                            command,
                            args,
                            env,
                            std::path::PathBuf::from(cwd),
                            anchor,
                            processes.clone(),
                            mcp_command,
                            mcp_args,
                            managed_mcp_servers,
                            resume_acp_session_id,
                        )
                        .await
                    {
                        let response = ServiceMessage::Error { message };
                        let mut writer = writer.lock().await;
                        ServiceCodec::write_service(&mut *writer, &response).await?;
                        continue;
                    }
                    let rx = acp_sessions.subscribe(sid.clone()).await;
                    if let Some(mut rx) = rx {
                        if let Some(snapshot) = acp_sessions.snapshot(sid.clone()).await {
                            let mut w = writer.lock().await;
                            ServiceCodec::write_service(&mut *w, &snapshot).await?;
                        }
                        if let Some(agent_info) = acp_sessions.agent_info(sid.clone()).await {
                            let mut w = writer.lock().await;
                            ServiceCodec::write_service(&mut *w, &agent_info).await?;
                        }
                        if let Some(config_state) = acp_sessions.config_state(sid.clone()).await {
                            let mut w = writer.lock().await;
                            ServiceCodec::write_service(&mut *w, &config_state).await?;
                        }
                        if let Some(old) = agent_forwarders.remove(&sid) {
                            old.abort();
                        }
                        let w = writer.clone();
                        let handle = tokio::spawn(async move {
                            loop {
                                match rx.recv().await {
                                    Ok(msg) => {
                                        let bytes =
                                            match rkyv::to_bytes::<rkyv::rancor::Error>(&msg) {
                                                Ok(b) => b,
                                                Err(_) => break,
                                            };
                                        let mut w = w.lock().await;
                                        if ServiceCodec::write_raw(&mut *w, &bytes).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                                        tracing::warn!(
                                            dropped,
                                            "service stream lagged; frames were dropped"
                                        );
                                        continue;
                                    }
                                    Err(broadcast::error::RecvError::Closed) => break,
                                }
                            }
                        });
                        agent_forwarders.insert(sid, handle);
                    }
                }
            }
        }

        for (_, handle) in attached.lock().await.drain() {
            handle.abort();
        }
        if let Some(handle) = agent_subscription.take() {
            handle.abort();
        }
        for (_, handle) in agent_forwarders.drain() {
            handle.abort();
        }

        if !created_processes.is_empty() {
            for id in &created_processes {
                let _ = processes.remove(*id).await;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ProcessAppThread {
        stop: Option<std::sync::mpsc::Sender<()>>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl ProcessAppThread {
        fn start(wake: mpsc::UnboundedSender<()>) -> (Self, ProcessRuntime) {
            let (processes, runtime) = ProcessRuntime::new(wake);
            let (stop, stopped) = std::sync::mpsc::channel();
            let handle = std::thread::spawn(move || {
                let mut app = App::new();
                app.add_plugins((MinimalPlugins, vmux_process::ProcessPlugin));
                app.world_mut()
                    .spawn((Name::new("vmux process runtime"), runtime));
                loop {
                    app.update();
                    match stopped.recv_timeout(std::time::Duration::from_millis(1)) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            });
            (
                Self {
                    stop: Some(stop),
                    handle: Some(handle),
                },
                processes,
            )
        }
    }

    impl Drop for ProcessAppThread {
        fn drop(&mut self) {
            if let Some(stop) = self.stop.take() {
                let _ = stop.send(());
            }
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    async fn run_test_server(listener: UnixListener, wake: mpsc::UnboundedSender<()>) {
        let (_process_app, processes) = ProcessAppThread::start(wake.clone());
        let (acp_sessions, acp_runtime) =
            vmux_agent::acp::AcpSessions::new(tokio::runtime::Handle::current(), wake.clone());
        drop(acp_runtime);
        let (agent_tx, _) = broadcast::channel(8);
        let broker = AgentBroker::new(
            agent_tx.clone(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let (connections, mut connection_inbox) = mpsc::unbounded_channel();
        let (shutdown, shutdown_inbox) = mpsc::channel(1);
        let (exit, mut exit_inbox) = mpsc::channel(1);
        let started_at = ServiceStartedAt(Instant::now());
        let runtime = ServiceClientRuntime {
            runtime: tokio::runtime::Handle::current(),
            processes,
            agent_tx,
            broker,
            acp_sessions,
            shutdown,
            wake: wake.clone(),
            started_at,
        };
        let listener = tokio::spawn(
            ServiceListener::new(listener, connections, wake, shutdown_inbox, exit).run(),
        );
        let mut clients = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                Some(stream) = connection_inbox.recv() => {
                    clients.spawn(ServiceConnection::new(stream, runtime.clone()).run());
                }
                _ = exit_inbox.recv() => break,
            }
        }
        clients.abort_all();
        let _ = listener.await;
    }

    #[tokio::test]
    async fn shutdown_message_breaks_run_server() {
        use vmux_api::protocol::ClientMessage;

        let dir = std::env::temp_dir().join(format!("vmux-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("test.sock");
        let _ = std::fs::remove_file(&sock);
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();

        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let server = tokio::spawn(run_test_server(listener, wake_tx));

        let stream = tokio::net::UnixStream::connect(&sock).await.unwrap();
        let (_r, mut w) = stream.into_split();
        let bytes =
            rkyv::to_bytes::<rkyv::rancor::Error>(&ClientMessage::Shutdown).expect("serialize");
        ServiceCodec::write_raw(&mut w, &bytes)
            .await
            .expect("write shutdown");

        let res = tokio::time::timeout(std::time::Duration::from_secs(3), server).await;
        assert!(res.is_ok(), "run_server did not exit after Shutdown");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    fn process_alive(pid: u32, identity: &Option<String>) -> bool {
        if unsafe { libc::kill(pid as i32, 0) } != 0 {
            return false;
        }
        #[cfg(target_os = "linux")]
        {
            if linux_proc_state(pid) == Some('Z') {
                return false;
            }
            if linux_proc_starttime(pid) != *identity {
                return false;
            }
        }
        true
    }

    fn proc_identity(pid: u32) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            linux_proc_starttime(pid)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_proc_state(pid: u32) -> Option<char> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(')')?.1.trim_start().chars().next()
    }

    #[cfg(target_os = "linux")]
    fn linux_proc_starttime(pid: u32) -> Option<String> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(')')?
            .1
            .split_whitespace()
            .nth(19)
            .map(str::to_string)
    }

    fn proc_state_label(pid: u32) -> String {
        #[cfg(target_os = "linux")]
        {
            linux_proc_state(pid)
                .map(|c| c.to_string())
                .unwrap_or_else(|| "gone".to_string())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            "n/a".to_string()
        }
    }

    async fn await_child_pid(pidfile: &std::path::Path) -> Option<u32> {
        for _ in 0..200 {
            if let Ok(s) = std::fs::read_to_string(pidfile)
                && let Ok(pid) = s.trim().parse::<u32>()
                && pid > 0
            {
                return Some(pid);
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        None
    }

    #[tokio::test]
    async fn client_disconnect_reaps_created_processes() {
        use vmux_api::protocol::ClientMessage;

        let dir = std::env::temp_dir().join(format!("vmux-reap-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("reap.sock");
        let pidfile = dir.join("child.pid");
        let _ = std::fs::remove_file(&sock);
        let _ = std::fs::remove_file(&pidfile);
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();

        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let server = tokio::spawn(run_test_server(listener, wake_tx));

        let stream = tokio::net::UnixStream::connect(&sock).await.unwrap();
        let (r, mut w) = stream.into_split();

        let create = ClientMessage::CreateProcess {
            process_id: ProcessId::new(),
            command: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                format!("echo $$ > {}; exec sleep 30", pidfile.display()),
            ],
            cwd: dir.display().to_string(),
            env: vec![],
            cols: 80,
            rows: 24,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&create).expect("serialize");
        ServiceCodec::write_raw(&mut w, &bytes)
            .await
            .expect("write create");

        let pid = await_child_pid(&pidfile)
            .await
            .expect("child process should report its pid");
        let identity = proc_identity(pid);
        assert!(
            process_alive(pid, &identity),
            "child should be alive after CreateProcess"
        );

        drop(w);
        drop(r);

        let reaped = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while process_alive(pid, &identity) {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;

        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        server.abort();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            reaped.is_ok(),
            "child pid {pid} still alive after client disconnect — service did not reap it (state: {})",
            proc_state_label(pid)
        );
    }

    #[tokio::test]
    async fn a_client_that_dies_mid_frame_still_has_its_processes_reaped() {
        use vmux_api::protocol::ClientMessage;

        let dir = std::env::temp_dir().join(format!("vmux-torn-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("torn.sock");
        let pidfile = dir.join("child.pid");
        let _ = std::fs::remove_file(&sock);
        let _ = std::fs::remove_file(&pidfile);
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();

        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let server = tokio::spawn(run_test_server(listener, wake_tx));

        let stream = tokio::net::UnixStream::connect(&sock).await.unwrap();
        let (r, mut w) = stream.into_split();

        let create = ClientMessage::CreateProcess {
            process_id: ProcessId::new(),
            command: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                format!("echo $$ > {}; exec sleep 30", pidfile.display()),
            ],
            cwd: dir.display().to_string(),
            env: vec![],
            cols: 80,
            rows: 24,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&create).expect("serialize");
        ServiceCodec::write_raw(&mut w, &bytes)
            .await
            .expect("write create");

        let pid = await_child_pid(&pidfile)
            .await
            .expect("child process should report its pid");
        let identity = proc_identity(pid);

        tokio::io::AsyncWriteExt::write_all(&mut w, &1024u32.to_le_bytes())
            .await
            .expect("write prefix");
        tokio::io::AsyncWriteExt::write_all(&mut w, b"only a few bytes")
            .await
            .expect("write partial body");
        drop(w);
        drop(r);

        let reaped = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while process_alive(pid, &identity) {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;

        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        server.abort();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            reaped.is_ok(),
            "child pid {pid} survived a torn frame — the read loop propagated instead of reaping (state: {})",
            proc_state_label(pid)
        );
    }
}
