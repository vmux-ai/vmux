use crate::process::{
    Process, ProcessCommandLifecycle, ProcessManager, ProcessSnapshot, ProcessUpdate,
};
use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::sync::{Mutex, broadcast, mpsc};
use vmux_api::protocol::{
    AgentAttachment, ClientMessage, ManagedMcpServer, ManagedMcpTransport, ProcessId,
    ServiceMessage, SharedMessage, compose_agent_prompt,
};

use super::query::ProcessQueries;
use crate::acp::AcpSessions;
use crate::agent::AgentSessions;
use crate::remote::authorization::RemoteAuthorizations;
use crate::remote::client_operation::ClientOperations;

type PendingQueries = crate::agent::AgentQueryResponses;

impl ProcessUpdate {
    fn into_service_message(self, process_id: ProcessId) -> ServiceMessage {
        match self {
            Self::Viewport(patch) => ServiceMessage::ViewportPatch {
                process_id,
                changed_lines: patch.changed_lines,
                cursor: patch.cursor,
                cols: patch.cols,
                rows: patch.rows,
                selection: patch.selection,
                copy_mode: patch.copy_mode,
                full: patch.full,
                first_row: patch.first_row,
                total_rows: patch.total_rows,
                alt: patch.alt,
                mouse: patch.mouse,
                evicted_total: patch.evicted_total,
            },
            Self::Exited { exit_code } => ServiceMessage::ProcessExited {
                process_id,
                exit_code,
            },
            Self::Title(title) => ServiceMessage::ProcessTitle { process_id, title },
            Self::Bell => ServiceMessage::Bell { process_id },
            Self::CommandLifecycle(lifecycle) => ServiceMessage::CommandLifecycle {
                process_id,
                kind: match lifecycle {
                    ProcessCommandLifecycle::Started => {
                        vmux_api::protocol::CommandLifecycleKind::Started
                    }
                    ProcessCommandLifecycle::Ended { exit_code } => {
                        vmux_api::protocol::CommandLifecycleKind::Ended { exit_code }
                    }
                },
            },
            Self::Mode(mode) => ServiceMessage::TerminalMode {
                process_id,
                mouse_capture: mode.mouse_capture,
                copy_mode: mode.copy_mode,
                alt_screen: mode.alt_screen,
                focus_reporting: mode.focus_reporting,
            },
        }
    }
}

impl ProcessSnapshot {
    fn into_service_message(self, process_id: ProcessId) -> ServiceMessage {
        ServiceMessage::Snapshot {
            process_id,
            lines: self.lines,
            cursor: self.cursor,
            cols: self.cols,
            rows: self.rows,
        }
    }
}

pub struct ServiceDaemonPlugin;

impl ServiceDaemonPlugin {
    pub fn runtime(
        listener: UnixListener,
        wake: mpsc::UnboundedSender<()>,
        runtime: tokio::runtime::Handle,
        exit: mpsc::Sender<()>,
    ) -> impl Bundle {
        let manager = Arc::new(Mutex::new(ProcessManager::new(wake.clone())));
        let (queries, process_runtime) = ProcessQueries::new(Arc::clone(&manager), wake.clone());
        let (client_operations, client_operation_runtime) = ClientOperations::new(wake.clone());
        let (authorizations, authorization_runtime) = RemoteAuthorizations::new(wake.clone());
        let (agent_sessions, agent_session_runtime) =
            AgentSessions::new(runtime.clone(), wake.clone());
        let (acp_sessions, acp_session_runtime) = AcpSessions::new(runtime.clone(), wake);
        (
            Name::new("vmux service runtime"),
            ServiceDaemonStartup(Some(ServiceDaemonStart {
                listener,
                manager,
                process_queries: queries,
                client_operations,
                authorizations,
                agent_sessions,
                acp_sessions,
                runtime,
                exit,
            })),
            process_runtime,
            client_operation_runtime,
            authorization_runtime,
            agent_session_runtime,
            acp_session_runtime,
        )
    }
}

impl Plugin for ServiceDaemonPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, start_service_daemon);
    }
}

#[derive(Component)]
struct ServiceDaemonStartup(Option<ServiceDaemonStart>);

struct ServiceDaemonStart {
    listener: UnixListener,
    manager: Arc<Mutex<ProcessManager>>,
    process_queries: ProcessQueries,
    client_operations: ClientOperations,
    authorizations: RemoteAuthorizations,
    agent_sessions: AgentSessions,
    acp_sessions: AcpSessions,
    runtime: tokio::runtime::Handle,
    exit: mpsc::Sender<()>,
}

#[derive(Component)]
struct ServiceDaemon;

#[derive(Component, Clone, Copy)]
struct ServiceStartedAt(Instant);

#[derive(Component)]
struct ServiceServerTask(tokio::task::JoinHandle<()>);

impl Drop for ServiceServerTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn start_service_daemon(
    mut startups: Query<(Entity, &mut ServiceDaemonStartup)>,
    mut commands: Commands,
) {
    for (entity, mut startup) in &mut startups {
        let Some(start) = startup.0.take() else {
            continue;
        };
        let started_at = ServiceStartedAt(Instant::now());
        let task = start.runtime.spawn(async move {
            ServiceServer {
                listener: start.listener,
                manager: start.manager,
                process_queries: start.process_queries,
                client_operations: start.client_operations,
                authorizations: start.authorizations,
                agent_sessions: start.agent_sessions,
                acp_sessions: start.acp_sessions,
                started_at,
            }
            .run()
            .await;
            let _ = start.exit.send(()).await;
        });
        commands
            .entity(entity)
            .remove::<ServiceDaemonStartup>()
            .insert((ServiceDaemon, started_at, ServiceServerTask(task)));
    }
}

type PendingCommands = crate::agent::AgentCommandResponses;

fn to_acp_mcp_server(
    server: ManagedMcpServer,
) -> Option<agent_client_protocol::schema::v1::McpServer> {
    use agent_client_protocol::schema::v1::{
        EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
    };

    if server.transport == ManagedMcpTransport::Stdio && server.cwd.is_some() {
        tracing::warn!(
            "managed MCP server {} skipped for ACP because ACP v1 does not support stdio cwd",
            server.name
        );
        return None;
    }
    let headers = server
        .headers
        .into_iter()
        .map(|(name, value)| HttpHeader::new(name, value))
        .collect();
    match server.transport {
        ManagedMcpTransport::Stdio => server.command.map(|command| {
            McpServer::Stdio(
                McpServerStdio::new(server.name, command)
                    .args(server.args)
                    .env(
                        server
                            .env
                            .into_iter()
                            .map(|(name, value)| EnvVariable::new(name, value))
                            .collect(),
                    ),
            )
        }),
        ManagedMcpTransport::Http => server
            .url
            .map(|url| McpServer::Http(McpServerHttp::new(server.name, url).headers(headers))),
        ManagedMcpTransport::Sse => server
            .url
            .map(|url| McpServer::Sse(McpServerSse::new(server.name, url).headers(headers))),
    }
}

fn page_agent_prompt(text: String, attachments: &[AgentAttachment]) -> String {
    if attachments.is_empty() {
        return text;
    }
    let mut prompt = text;
    if !prompt.is_empty() {
        prompt.push_str("\n\n");
    }
    prompt.push_str("Attached files:\n");
    for attachment in attachments {
        prompt.push_str("- ");
        prompt.push_str(&attachment.path);
        prompt.push('\n');
    }
    prompt.pop();
    prompt
}

async fn route_agent_input(
    acp_sessions: &AcpSessions,
    agent_sessions: &AgentSessions,
    sid: String,
    text: String,
    context: Option<String>,
    attachments: Vec<AgentAttachment>,
    preferred_mode: Option<String>,
) {
    if acp_sessions
        .input(
            sid.clone(),
            crate::acp::AcpInput::User {
                text: text.clone(),
                context: context.clone(),
                attachments: attachments.clone(),
                preferred_mode,
            },
        )
        .await
    {
        return;
    }
    let text = compose_agent_prompt(&page_agent_prompt(text, &attachments), context.as_deref());
    agent_sessions
        .input(sid, crate::agent::SessionInput::User { text, attachments })
        .await;
}

async fn with_process_mut<F, R>(
    manager: &Arc<Mutex<ProcessManager>>,
    id: ProcessId,
    f: F,
) -> Option<R>
where
    F: FnOnce(&mut Process) -> R,
{
    let mut mgr = manager.lock().await;
    mgr.processes.get_mut(&id).map(f)
}

struct ServiceServer {
    listener: UnixListener,
    manager: Arc<Mutex<ProcessManager>>,
    process_queries: ProcessQueries,
    client_operations: ClientOperations,
    authorizations: RemoteAuthorizations,
    agent_sessions: AgentSessions,
    acp_sessions: AcpSessions,
    started_at: ServiceStartedAt,
}

impl ServiceServer {
    async fn run(self) {
        let Self {
            listener,
            manager,
            process_queries,
            client_operations,
            authorizations,
            agent_sessions,
            acp_sessions,
            started_at,
        } = self;
        let (agent_tx, _) = broadcast::channel::<ServiceMessage>(128);
        let pending_queries = PendingQueries::default();
        let pending_commands = PendingCommands::default();
        let pending_tool_calls = crate::agent::AgentToolResponses::default();
        let remote_broker = crate::agent::AgentBroker::new(
            agent_tx.clone(),
            pending_commands.clone(),
            pending_queries.clone(),
            pending_tool_calls.clone(),
        );
        let remote_handle = crate::remote::server::spawn(
            agent_sessions.clone(),
            acp_sessions.clone(),
            remote_broker,
            client_operations,
            authorizations,
        );
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        loop {
            tokio::select! {
                accept = listener.accept() => {
                    let (stream, _) = match accept {
                        Ok(conn) => conn,
                        Err(e) => {
                            tracing::error!(error = %e, "accept error");
                            continue;
                        }
                    };
                    let mgr = Arc::clone(&manager);
                    let agent_tx = agent_tx.clone();
                    let pending_queries = pending_queries.clone();
                    let pending_commands = pending_commands.clone();
                    let pending_tool_calls = pending_tool_calls.clone();
                    let agent_sessions = agent_sessions.clone();
                    let acp_sessions = acp_sessions.clone();
                    let process_queries = process_queries.clone();
                    let shutdown_tx = shutdown_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(
                            stream,
                            mgr,
                            agent_tx,
                            pending_queries,
                            pending_commands,
                            pending_tool_calls,
                            agent_sessions,
                            acp_sessions,
                            process_queries,
                            shutdown_tx,
                            started_at,
                        )
                        .await
                        {
                            tracing::error!(error = %e, "client error");
                        }
                    });
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("server: drain signaled, closing listener");
                    break;
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        remote_handle.abort();
        tracing::info!("server: drain complete, exiting");
    }
}

fn command_result_to_content(result: vmux_api::protocol::AgentCommandResult) -> (String, bool) {
    use vmux_api::protocol::AgentCommandResult;
    match result {
        AgentCommandResult::Ok => ("ok".to_string(), false),
        AgentCommandResult::Text(text) => (text, false),
        AgentCommandResult::Layout(snapshot) => {
            (serde_json::to_string(&snapshot).unwrap_or_default(), false)
        }
        AgentCommandResult::Error(message) => (message, true),
    }
}

fn query_response_to_content(response: ServiceMessage) -> Option<(String, bool)> {
    let content = match response {
        ServiceMessage::AgentQueryError { message, .. } => (message, true),
        ServiceMessage::AgentLayoutResult { result, .. } => match result {
            Ok(snapshot) => (serde_json::to_string(&snapshot).unwrap_or_default(), false),
            Err(message) => (message, true),
        },
        ServiceMessage::AgentVaultStatusResult { result, .. } => match result {
            Ok(snapshot) => (
                serde_json::to_string_pretty(&snapshot).unwrap_or_default(),
                false,
            ),
            Err(message) => (message, true),
        },
        ServiceMessage::ProcessOutputResult { result, .. }
        | ServiceMessage::ProcessTranscriptResult { result, .. }
        | ServiceMessage::AgentBrowserSnapshotResult { result, .. }
        | ServiceMessage::AgentBrowserScrollResult { result, .. }
        | ServiceMessage::AgentSimulatorControlResult { result, .. }
        | ServiceMessage::AgentWorkingDirectoryResult { result, .. } => match result {
            Ok(text) => (text, false),
            Err(message) => (message, true),
        },
        ServiceMessage::AgentSettingsResult { result, .. } => match result {
            Ok(settings) => {
                let value =
                    serde_json::Value::try_from(&settings).unwrap_or(serde_json::Value::Null);
                (serde_json::to_string(&value).unwrap_or_default(), false)
            }
            Err(message) => (message, true),
        },
        ServiceMessage::AgentSpacesResult { result, .. } => match result {
            Ok(spaces) => (serde_json::to_string(&spaces).unwrap_or_default(), false),
            Err(message) => (message, true),
        },
        ServiceMessage::AgentBookmarksResult { result, .. } => match result {
            Ok(bookmarks) => (serde_json::to_string(&bookmarks).unwrap_or_default(), false),
            Err(message) => (message, true),
        },
        ServiceMessage::AgentCommandsResult { result, .. } => match result {
            Ok(commands) => (serde_json::to_string(&commands).unwrap_or_default(), false),
            Err(message) => (message, true),
        },
        ServiceMessage::ProcessCommandExitResult { result, .. } => match result {
            Ok(result) => {
                let exit = result
                    .exit
                    .map_or_else(|| "null".to_string(), |code| code.to_string());
                (
                    format!("{{\"seq\":{},\"exit\":{exit}}}", result.sequence),
                    false,
                )
            }
            Err(message) => (message, true),
        },
        ServiceMessage::ProcessRunCompletionResult { result, .. } => match result {
            Ok(result) => {
                let token = result
                    .token
                    .map_or_else(|| "null".to_string(), |token| format!("\"{token}\""));
                let exit = result
                    .exit
                    .map_or_else(|| "null".to_string(), |code| code.to_string());
                (format!("{{\"token\":{token},\"exit\":{exit}}}"), false)
            }
            Err(message) => (message, true),
        },
        ServiceMessage::AgentScreenshotResult { result, .. }
        | ServiceMessage::AgentSimulatorScreenshotResult { result, .. } => match result {
            Ok(image) => (
                format!("saved {} ({}×{})", image.path, image.width, image.height),
                false,
            ),
            Err(message) => (message, true),
        },
        ServiceMessage::AgentRecordStartResult { result, .. } => match result {
            Ok(max_secs) => (format!("recording started, max {max_secs}s"), false),
            Err(message) => (message, true),
        },
        ServiceMessage::AgentRecordStopResult { result, .. } => match result {
            Ok(recording) => {
                let secs = recording.duration_ms as f64 / 1000.0;
                let gif = recording
                    .gif_path
                    .map(|path| format!(" + {path}"))
                    .unwrap_or_default();
                let auto = if recording.auto_stopped {
                    " (auto-stopped)"
                } else {
                    ""
                };
                (
                    format!(
                        "recorded {secs:.1}s -> {} ({} bytes){gif}{auto}",
                        recording.mp4_path, recording.bytes
                    ),
                    false,
                )
            }
            Err(message) => (message, true),
        },
        _ => return None,
    };
    Some(content)
}

async fn route_agent_query_response(
    request_id: vmux_api::protocol::AgentRequestId,
    response: ServiceMessage,
    pending_queries: &PendingQueries,
    broker: &crate::agent::AgentBroker,
) {
    if pending_queries.resolve(request_id, response.clone()).await {
        return;
    }
    if let Some((content, is_error)) = query_response_to_content(response) {
        broker.resolve_tool(request_id, content, is_error).await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_client(
    stream: tokio::net::UnixStream,
    manager: Arc<Mutex<ProcessManager>>,
    agent_tx: broadcast::Sender<ServiceMessage>,
    pending_queries: PendingQueries,
    pending_commands: PendingCommands,
    pending_tool_calls: crate::agent::AgentToolResponses,
    agent_sessions: AgentSessions,
    acp_sessions: AcpSessions,
    process_queries: ProcessQueries,
    shutdown_tx: mpsc::Sender<()>,
    started_at: ServiceStartedAt,
) -> std::io::Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let writer = Arc::new(tokio::sync::Mutex::new(writer));

    let attached: Arc<tokio::sync::Mutex<HashMap<ProcessId, tokio::task::JoinHandle<()>>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let mut agent_subscription: Option<tokio::task::JoinHandle<()>> = None;
    let mut page_agent_forwarders: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();
    let broker = crate::agent::AgentBroker::new(
        agent_tx.clone(),
        pending_commands.clone(),
        pending_queries.clone(),
        pending_tool_calls.clone(),
    );

    let mut created_processes: Vec<ProcessId> = Vec::new();

    loop {
        let msg: Option<ClientMessage> =
            match vmux_core::service::read_client_message(&mut reader).await {
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
                let created = {
                    let mut mgr = manager.lock().await;
                    mgr.create_process(process_id, command, args, cwd, env, cols, rows)
                };
                match created {
                    Ok((id, pid)) => {
                        created_processes.push(id);
                        let resp = ServiceMessage::ProcessCreated {
                            process_id: id,
                            pid,
                        };
                        let w = writer.clone();
                        let mut w = w.lock().await;
                        vmux_core::service::write_service_message(&mut *w, &resp).await?;
                    }
                    Err(reason) => {
                        let resp = ServiceMessage::ProcessCreateFailed { process_id, reason };
                        let w = writer.clone();
                        let mut w = w.lock().await;
                        vmux_core::service::write_service_message(&mut *w, &resp).await?;
                    }
                }
            }

            ClientMessage::AttachProcess { process_id } => {
                let mgr = manager.lock().await;
                if let Some(process) = mgr.processes.get(&process_id) {
                    let mut rx = process.subscribe();
                    let w = writer.clone();
                    let handle = tokio::spawn(async move {
                        loop {
                            match rx.recv().await {
                                Ok(update) => {
                                    let msg = update.into_service_message(process_id);
                                    let bytes = match rkyv::to_bytes::<rkyv::rancor::Error>(&msg) {
                                        Ok(b) => b,
                                        Err(_) => break,
                                    };
                                    let mut w = w.lock().await;
                                    if vmux_core::service::write_raw_frame(&mut *w, &bytes)
                                        .await
                                        .is_err()
                                    {
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
                    vmux_core::service::write_service_message(&mut *w, &resp).await?;
                }
            }

            ClientMessage::DetachProcess { process_id } => {
                if let Some(handle) = attached.lock().await.remove(&process_id) {
                    handle.abort();
                }
            }

            ClientMessage::ProcessInput { process_id, data } => {
                let writer = {
                    let mgr = manager.lock().await;
                    mgr.processes
                        .get(&process_id)
                        .filter(|process| !process.is_copy_mode())
                        .map(Process::input_writer)
                };
                if let Some(writer) = writer {
                    Process::write_input_to_writer(&writer, &data);
                }
            }

            ClientMessage::MouseWheel {
                process_id,
                up,
                col,
                row,
                modifiers,
            } => {
                with_process_mut(&manager, process_id, |process| {
                    process.handle_mouse_wheel(up, col, row, modifiers)
                })
                .await;
            }

            ClientMessage::ScrollWindow {
                process_id,
                top_row,
                follow,
            } => {
                with_process_mut(&manager, process_id, |process| {
                    process.handle_scroll_window(top_row, follow)
                })
                .await;
            }

            ClientMessage::ResizeProcess {
                process_id,
                cols,
                rows,
            } => {
                let mut mgr = manager.lock().await;
                if let Some(process) = mgr.processes.get_mut(&process_id) {
                    process.resize(cols, rows);
                }
            }

            ClientMessage::ListProcesses => {
                let mgr = manager.lock().await;
                let processes = mgr.processes.values().map(|p| p.info()).collect::<Vec<_>>();
                let resp = ServiceMessage::ProcessList { processes };
                let mut w = writer.lock().await;
                vmux_core::service::write_service_message(&mut *w, &resp).await?;
            }

            ClientMessage::KillProcess { process_id } => {
                let mut mgr = manager.lock().await;
                mgr.remove_process(&process_id);
                if let Some(handle) = attached.lock().await.remove(&process_id) {
                    handle.abort();
                }
            }

            ClientMessage::RequestSnapshot { process_id } => {
                let mgr = manager.lock().await;
                if let Some(process) = mgr.processes.get(&process_id) {
                    let snap = process.snapshot().into_service_message(process_id);
                    let mut w = writer.lock().await;
                    vmux_core::service::write_service_message(&mut *w, &snap).await?;
                } else {
                    let resp = ServiceMessage::Error {
                        message: format!("process not found: {process_id}"),
                    };
                    let mut w = writer.lock().await;
                    vmux_core::service::write_service_message(&mut *w, &resp).await?;
                }
            }

            ClientMessage::SetSelection { process_id, range } => {
                with_process_mut(&manager, process_id, |process| process.set_selection(range))
                    .await;
            }

            ClientMessage::ExtendSelectionTo {
                process_id,
                col,
                row,
            } => {
                with_process_mut(&manager, process_id, |process| {
                    process.extend_selection_to(col, row)
                })
                .await;
            }

            ClientMessage::SelectWordAt {
                process_id,
                col,
                row,
            } => {
                with_process_mut(&manager, process_id, |process| {
                    process.select_word_at(col, row)
                })
                .await;
            }

            ClientMessage::SelectLineAt { process_id, row } => {
                with_process_mut(&manager, process_id, |process| process.select_line_at(row)).await;
            }

            ClientMessage::GetSelectionText { process_id } => {
                let text =
                    with_process_mut(&manager, process_id, |process| process.selection_text())
                        .await
                        .flatten()
                        .unwrap_or_default();
                let resp = ServiceMessage::SelectionText { process_id, text };
                let mut w = writer.lock().await;
                vmux_core::service::write_service_message(&mut *w, &resp).await?;
            }

            ClientMessage::EnterCopyMode { process_id } => {
                with_process_mut(&manager, process_id, |process| process.enter_copy_mode()).await;
            }

            ClientMessage::ExitCopyMode { process_id } => {
                with_process_mut(&manager, process_id, |process| process.exit_copy_mode()).await;
            }

            ClientMessage::CopyModeKey { process_id, key } => {
                if let Some(Some(text)) =
                    with_process_mut(&manager, process_id, |process| process.copy_mode_key(key))
                        .await
                {
                    let resp = ServiceMessage::SelectionText { process_id, text };
                    let mut w = writer.lock().await;
                    vmux_core::service::write_service_message(&mut *w, &resp).await?;
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
                                if vmux_core::service::write_raw_frame(&mut *w, &bytes)
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(dropped)) => {
                                tracing::warn!(dropped, "agent stream lagged; frames were dropped");
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
                    let _ = vmux_core::service::write_raw_frame(&mut *w, &bytes).await;
                });
            }

            ClientMessage::Shutdown => {
                tracing::info!("shutdown requested by client; draining");
                {
                    let mut mgr = manager.lock().await;
                    mgr.shutdown();
                }
                let resp = ServiceMessage::ProcessList {
                    processes: Vec::new(),
                };
                let mut w = writer.lock().await;
                vmux_core::service::write_service_message(&mut *w, &resp).await?;
                shutdown_tx.send(()).await.ok();
                break;
            }

            ClientMessage::Status => {
                let uptime_secs = started_at.0.elapsed().as_secs();
                let process_count = {
                    let mgr = manager.lock().await;
                    mgr.processes.len() as u32
                };
                let resp = ServiceMessage::StatusResponse {
                    uptime_secs,
                    process_count,
                };
                let mut w = writer.lock().await;
                vmux_core::service::write_service_message(&mut *w, &resp).await?;
            }

            ClientMessage::AgentQuery { request_id, query } => {
                let response = match process_queries.response(request_id, &query).await {
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
                            let _ = vmux_core::service::write_raw_frame(&mut *writer, &bytes).await;
                        });
                        continue;
                    }
                    Err(message) => ServiceMessage::Error { message },
                };
                let mut writer = writer.lock().await;
                vmux_core::service::write_service_message(&mut *writer, &response).await?;
            }

            ClientMessage::AgentQueryError {
                request_id,
                message,
            } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentQueryError {
                        request_id,
                        message,
                    },
                    &pending_queries,
                    &broker,
                )
                .await;
            }

            ClientMessage::AgentLayoutResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentLayoutResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::ProcessOutputResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::ProcessOutputResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::ProcessTranscriptResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::ProcessTranscriptResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::ProcessCommandExitResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::ProcessCommandExitResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::ProcessRunCompletionResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::ProcessRunCompletionResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentSettingsResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentSettingsResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentSpacesResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentSpacesResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentScreenshotResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentScreenshotResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentBrowserSnapshotResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentBrowserSnapshotResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentBrowserScrollResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentBrowserScrollResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentRecordStartResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentRecordStartResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentRecordStopResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentRecordStopResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentBookmarksResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentBookmarksResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentSimulatorScreenshotResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentSimulatorScreenshotResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentSimulatorControlResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentSimulatorControlResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentWorkingDirectoryResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentWorkingDirectoryResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentVaultStatusResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentVaultStatusResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }
            ClientMessage::AgentCommandsResult { request_id, result } => {
                route_agent_query_response(
                    request_id,
                    ServiceMessage::AgentCommandsResult { request_id, result },
                    &pending_queries,
                    &broker,
                )
                .await;
            }

            ClientMessage::AgentCommandResponse { request_id, result } => {
                if !broker.resolve_command(request_id, result.clone()).await {
                    let (content, is_error) = command_result_to_content(result);
                    broker.resolve_tool(request_id, content, is_error).await;
                }
            }

            ClientMessage::SpawnPageAgent {
                sid,
                provider,
                model,
                cwd,
                auto_tools,
                tools_json,
            } => {
                let tools: Vec<crate::stream::ToolDef> =
                    serde_json::from_str(&tools_json).unwrap_or_default();
                let auto: std::collections::HashSet<String> = auto_tools.into_iter().collect();
                let result = agent_sessions
                    .spawn(sid, provider, model, cwd, tools, auto, broker.clone())
                    .await;
                if let Err(message) = result {
                    let resp = ServiceMessage::Error { message };
                    let mut w = writer.lock().await;
                    vmux_core::service::write_service_message(&mut *w, &resp).await?;
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
                let rx = agent_sessions.subscribe(sid.clone()).await;
                if let Some(mut rx) = rx {
                    if let Some(snapshot) = agent_sessions.snapshot(sid.clone()).await {
                        let mut w = writer.lock().await;
                        vmux_core::service::write_service_message(&mut *w, &snapshot).await?;
                    }
                    if let Some(old) = page_agent_forwarders.remove(&sid) {
                        old.abort();
                    }
                    let w = writer.clone();
                    let handle = tokio::spawn(async move {
                        loop {
                            match rx.recv().await {
                                Ok(msg) => {
                                    let bytes = match rkyv::to_bytes::<rkyv::rancor::Error>(&msg) {
                                        Ok(b) => b,
                                        Err(_) => break,
                                    };
                                    let mut w = w.lock().await;
                                    if vmux_core::service::write_raw_frame(&mut *w, &bytes)
                                        .await
                                        .is_err()
                                    {
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
                    page_agent_forwarders.insert(sid, handle);
                }
            }

            ClientMessage::DetachPageAgent { sid } => {
                if let Some(handle) = page_agent_forwarders.remove(&sid) {
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
                route_agent_input(
                    &acp_sessions,
                    &agent_sessions,
                    sid,
                    text,
                    context,
                    attachments,
                    preferred_mode,
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
                    vmux_core::service::write_service_message(&mut *w, &resp).await?;
                }
            }

            ClientMessage::AcpSetModel {
                sid,
                request_id,
                config_id,
                model_id,
            } => {
                acp_sessions
                    .input(
                        sid,
                        crate::acp::AcpInput::SetModel {
                            request_id,
                            config_id,
                            model_id,
                        },
                    )
                    .await;
            }

            ClientMessage::AcpSetMode {
                sid,
                request_id,
                config_id,
                mode_id,
            } => {
                acp_sessions
                    .input(
                        sid,
                        crate::acp::AcpInput::SetMode {
                            request_id,
                            config_id,
                            mode_id,
                        },
                    )
                    .await;
            }

            ClientMessage::Shared(SharedMessage::AgentCancel { sid }) => {
                if !acp_sessions
                    .input(sid.clone(), crate::acp::AcpInput::Cancel)
                    .await
                {
                    agent_sessions
                        .input(sid, crate::agent::SessionInput::Cancel)
                        .await;
                }
            }

            ClientMessage::Shared(SharedMessage::AgentApprove {
                sid,
                call_id,
                decision,
            }) => {
                if !acp_sessions
                    .input(
                        sid.clone(),
                        crate::acp::AcpInput::Approve {
                            call_id: call_id.clone(),
                            decision,
                        },
                    )
                    .await
                {
                    agent_sessions
                        .input(
                            sid,
                            crate::agent::SessionInput::Approve { call_id, decision },
                        )
                        .await;
                }
            }

            ClientMessage::ClosePageAgent { sid } => {
                if !acp_sessions.close(sid.clone()).await {
                    agent_sessions.close(sid.clone()).await;
                }
                if let Some(handle) = page_agent_forwarders.remove(&sid) {
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
                effort,
            } => {
                let mut mcp_servers = mcp_command
                    .map(|cmd| {
                        vec![agent_client_protocol::schema::v1::McpServer::Stdio(
                            agent_client_protocol::schema::v1::McpServerStdio::new(
                                "vmux",
                                std::path::PathBuf::from(cmd),
                            )
                            .args(mcp_args),
                        )]
                    })
                    .unwrap_or_default();
                mcp_servers.extend(
                    managed_mcp_servers
                        .into_iter()
                        .filter_map(to_acp_mcp_server),
                );
                if let Err(message) = acp_sessions
                    .spawn(
                        sid.clone(),
                        agent_id,
                        command,
                        args,
                        env,
                        std::path::PathBuf::from(cwd),
                        anchor,
                        Arc::clone(&manager),
                        mcp_servers,
                        resume_acp_session_id,
                        effort,
                    )
                    .await
                {
                    let response = ServiceMessage::Error { message };
                    let mut writer = writer.lock().await;
                    vmux_core::service::write_service_message(&mut *writer, &response).await?;
                    continue;
                }
                let rx = acp_sessions.subscribe(sid.clone()).await;
                if let Some(mut rx) = rx {
                    if let Some(snapshot) = acp_sessions.snapshot(sid.clone()).await {
                        let mut w = writer.lock().await;
                        vmux_core::service::write_service_message(&mut *w, &snapshot).await?;
                    }
                    if let Some(agent_info) = acp_sessions.agent_info(sid.clone()).await {
                        let mut w = writer.lock().await;
                        vmux_core::service::write_service_message(&mut *w, &agent_info).await?;
                    }
                    if let Some(model_info) = acp_sessions.model_info(sid.clone()).await {
                        let mut w = writer.lock().await;
                        vmux_core::service::write_service_message(&mut *w, &model_info).await?;
                    }
                    if let Some(mode_info) = acp_sessions.mode_info(sid.clone()).await {
                        let mut w = writer.lock().await;
                        vmux_core::service::write_service_message(&mut *w, &mode_info).await?;
                    }
                    if let Some(old) = page_agent_forwarders.remove(&sid) {
                        old.abort();
                    }
                    let w = writer.clone();
                    let handle = tokio::spawn(async move {
                        loop {
                            match rx.recv().await {
                                Ok(msg) => {
                                    let bytes = match rkyv::to_bytes::<rkyv::rancor::Error>(&msg) {
                                        Ok(b) => b,
                                        Err(_) => break,
                                    };
                                    let mut w = w.lock().await;
                                    if vmux_core::service::write_raw_frame(&mut *w, &bytes)
                                        .await
                                        .is_err()
                                    {
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
                    page_agent_forwarders.insert(sid, handle);
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
    for (_, handle) in page_agent_forwarders.drain() {
        handle.abort();
    }

    if !created_processes.is_empty() {
        let mut mgr = manager.lock().await;
        for id in &created_processes {
            mgr.remove_process(id);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run_test_server(listener: UnixListener, wake: mpsc::UnboundedSender<()>) {
        let manager = Arc::new(Mutex::new(ProcessManager::new(wake.clone())));
        let (process_queries, _process_runtime) =
            ProcessQueries::new(Arc::clone(&manager), wake.clone());
        let (agent_sessions, _agent_runtime) =
            crate::agent::AgentSessions::new(tokio::runtime::Handle::current(), wake);
        let acp_sessions = crate::acp::AcpSessions::closed();
        let mut server = Box::pin(
            super::ServiceServer {
                listener,
                manager: Arc::clone(&manager),
                process_queries,
                client_operations: ClientOperations::closed(),
                authorizations: RemoteAuthorizations::closed(),
                agent_sessions,
                acp_sessions,
                started_at: ServiceStartedAt(Instant::now()),
            }
            .run(),
        );
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(16));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = &mut server => return,
                _ = interval.tick() => manager.lock().await.reap_exited(),
            }
        }
    }

    #[test]
    fn page_agent_prompt_appends_attachment_paths() {
        let attachments = vec![AgentAttachment {
            path: "/tmp/report.txt".into(),
            name: "report.txt".into(),
            mime_type: "text/plain".into(),
            size: 12,
        }];
        assert_eq!(
            page_agent_prompt("review".into(), &attachments),
            "review\n\nAttached files:\n- /tmp/report.txt"
        );
    }

    #[test]
    fn page_agent_private_context_keeps_empty_display_prompt() {
        let prompt = compose_agent_prompt(&page_agent_prompt(String::new(), &[]), Some("resume"));

        assert!(prompt.contains("resume"));
        assert_eq!(
            vmux_api::protocol::extract_display_prompt(&prompt),
            Some("")
        );
    }

    #[test]
    fn acp_rejects_stdio_mcp_server_with_working_directory() {
        assert!(
            to_acp_mcp_server(ManagedMcpServer {
                name: "local".into(),
                transport: ManagedMcpTransport::Stdio,
                command: Some("server".into()),
                args: Vec::new(),
                env: Vec::new(),
                cwd: Some("/tmp/project".into()),
                url: None,
                headers: Vec::new(),
            })
            .is_none()
        );
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
        vmux_core::service::write_raw_frame(&mut w, &bytes)
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
        vmux_core::service::write_raw_frame(&mut w, &bytes)
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
        vmux_core::service::write_raw_frame(&mut w, &bytes)
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
