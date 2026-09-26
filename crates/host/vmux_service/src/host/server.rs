use crate::process::{Process, ProcessManager};
use crate::{read_message, write_message};
use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Instant;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::sync::{Mutex, broadcast, mpsc};
use vmux_api::protocol::{
    AgentAttachment, ClientMessage, ManagedMcpServer, ManagedMcpTransport, ProcessId,
    ServiceMessage, SharedMessage, compose_agent_prompt, validate_agent_command,
};

use super::query::{ProcessQueries, ProcessQueryPlugin};

static SERVICE_STARTED: OnceLock<Instant> = OnceLock::new();

pub(crate) fn init_started_at() {
    SERVICE_STARTED.get_or_init(Instant::now);
}

type PendingQueries = Arc<
    Mutex<
        HashMap<vmux_api::protocol::AgentRequestId, tokio::sync::oneshot::Sender<ServiceMessage>>,
    >,
>;

pub(crate) struct ServiceDaemonPlugin {
    listener: std::sync::Mutex<Option<UnixListener>>,
    manager: Arc<Mutex<ProcessManager>>,
    runtime: tokio::runtime::Handle,
    exit: mpsc::Sender<()>,
    queries: ProcessQueries,
    query_plugin: std::sync::Mutex<Option<ProcessQueryPlugin>>,
}

impl ServiceDaemonPlugin {
    pub(crate) fn new(
        listener: UnixListener,
        wake: mpsc::UnboundedSender<ProcessId>,
        runtime: tokio::runtime::Handle,
        exit: mpsc::Sender<()>,
    ) -> Self {
        let manager = Arc::new(Mutex::new(ProcessManager::new(wake.clone())));
        let (query_plugin, queries) = ProcessQueryPlugin::new(Arc::clone(&manager), wake);
        Self {
            listener: std::sync::Mutex::new(Some(listener)),
            manager,
            runtime,
            exit,
            queries,
            query_plugin: std::sync::Mutex::new(Some(query_plugin)),
        }
    }
}

impl Plugin for ServiceDaemonPlugin {
    fn build(&self, app: &mut App) {
        let listener = self
            .listener
            .lock()
            .unwrap()
            .take()
            .expect("service daemon plugin can only be built once");
        let manager = Arc::clone(&self.manager);
        let server_manager = Arc::clone(&manager);
        let queries = self.queries.clone();
        let exit = self.exit.clone();
        let query_plugin = self
            .query_plugin
            .lock()
            .unwrap()
            .take()
            .expect("service daemon plugin can only be built once");
        app.add_plugins(query_plugin);
        let task = self.runtime.spawn(async move {
            run_server(listener, server_manager, queries).await;
            let _ = exit.send(()).await;
        });
        app.world_mut().spawn((
            Name::new("vmux service daemon"),
            ServiceDaemon,
            ServiceServerTask(task),
        ));
    }
}

#[derive(Component)]
struct ServiceDaemon;

#[derive(Component)]
struct ServiceServerTask(tokio::task::JoinHandle<()>);

impl Drop for ServiceServerTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

type PendingCommands = Arc<
    Mutex<
        HashMap<
            vmux_api::protocol::AgentRequestId,
            tokio::sync::oneshot::Sender<vmux_api::protocol::AgentCommandResult>,
        >,
    >,
>;

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
    acp_manager: &Arc<Mutex<crate::acp::AcpSessionManager>>,
    agent_manager: &Arc<Mutex<crate::agent::AgentSessionManager>>,
    sid: String,
    text: String,
    context: Option<String>,
    attachments: Vec<AgentAttachment>,
    preferred_mode: Option<String>,
) {
    let acp = acp_manager.lock().await;
    if acp.contains(&sid) {
        acp.input(
            &sid,
            crate::acp::AcpInput::User {
                text,
                context,
                attachments,
                preferred_mode,
            },
        );
        return;
    }
    drop(acp);
    let text = compose_agent_prompt(&page_agent_prompt(text, &attachments), context.as_deref());
    agent_manager
        .lock()
        .await
        .input(&sid, crate::agent::SessionInput::User { text, attachments });
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

async fn run_server(
    listener: UnixListener,
    manager: Arc<Mutex<ProcessManager>>,
    process_queries: ProcessQueries,
) {
    let (agent_tx, _) = broadcast::channel::<ServiceMessage>(128);
    let pending_queries: PendingQueries = Arc::new(Mutex::new(HashMap::new()));
    let pending_commands: PendingCommands = Arc::new(Mutex::new(HashMap::new()));
    let pending_tool_calls: crate::agent_broker::PendingToolCalls =
        Arc::new(Mutex::new(HashMap::new()));
    let agent_manager = Arc::new(Mutex::new(crate::agent::AgentSessionManager::default()));
    let acp_manager = Arc::new(Mutex::new(crate::acp::AcpSessionManager::default()));
    let remote_broker = crate::agent_broker::AgentBroker::new(
        agent_tx.clone(),
        Arc::clone(&pending_commands),
        Arc::clone(&pending_queries),
        Arc::clone(&pending_tool_calls),
    );
    let remote_handle = crate::remote::server::spawn(
        Arc::clone(&agent_manager),
        Arc::clone(&acp_manager),
        remote_broker,
    );
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

    init_started_at();

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
                let pending_queries = Arc::clone(&pending_queries);
                let pending_commands = Arc::clone(&pending_commands);
                let pending_tool_calls = Arc::clone(&pending_tool_calls);
                let agent_manager = Arc::clone(&agent_manager);
                let acp_manager = Arc::clone(&acp_manager);
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
                        agent_manager,
                        acp_manager,
                        process_queries,
                        shutdown_tx,
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
    broker: &crate::agent_broker::AgentBroker,
) {
    let pending = pending_queries.lock().await.remove(&request_id);
    if let Some(tx) = pending {
        let _ = tx.send(response);
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
    pending_tool_calls: crate::agent_broker::PendingToolCalls,
    agent_manager: Arc<Mutex<crate::agent::AgentSessionManager>>,
    acp_manager: Arc<Mutex<crate::acp::AcpSessionManager>>,
    process_queries: ProcessQueries,
    shutdown_tx: mpsc::Sender<()>,
) -> std::io::Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let writer = Arc::new(tokio::sync::Mutex::new(writer));

    let attached: Arc<tokio::sync::Mutex<HashMap<ProcessId, tokio::task::JoinHandle<()>>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let mut agent_subscription: Option<tokio::task::JoinHandle<()>> = None;
    let mut page_agent_forwarders: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();
    let broker = crate::agent_broker::AgentBroker::new(
        agent_tx.clone(),
        Arc::clone(&pending_commands),
        Arc::clone(&pending_queries),
        Arc::clone(&pending_tool_calls),
    );

    let mut created_processes: Vec<ProcessId> = Vec::new();

    loop {
        let msg: Option<ClientMessage> = match read_message!(&mut reader, ClientMessage) {
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
                        write_message!(&mut *w, &resp)?;
                    }
                    Err(reason) => {
                        let resp = ServiceMessage::ProcessCreateFailed { process_id, reason };
                        let w = writer.clone();
                        let mut w = w.lock().await;
                        write_message!(&mut *w, &resp)?;
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
                                Ok(msg) => {
                                    let bytes = match rkyv::to_bytes::<rkyv::rancor::Error>(&msg) {
                                        Ok(b) => b,
                                        Err(_) => break,
                                    };
                                    let mut w = w.lock().await;
                                    if crate::framing::write_raw_frame(&mut *w, &bytes)
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
                    write_message!(&mut *w, &resp)?;
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
                write_message!(&mut *w, &resp)?;
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
                    let snap = process.snapshot();
                    let mut w = writer.lock().await;
                    write_message!(&mut *w, &snap)?;
                } else {
                    let resp = ServiceMessage::Error {
                        message: format!("process not found: {process_id}"),
                    };
                    let mut w = writer.lock().await;
                    write_message!(&mut *w, &resp)?;
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
                write_message!(&mut *w, &resp)?;
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
                    write_message!(&mut *w, &resp)?;
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
                                if crate::framing::write_raw_frame(&mut *w, &bytes)
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

            ClientMessage::AgentCommand {
                request_id,
                anchor,
                command,
            } => {
                if let Err(message) = validate_agent_command(&command) {
                    let resp = ServiceMessage::Error {
                        message: message.to_string(),
                    };
                    let mut w = writer.lock().await;
                    write_message!(&mut *w, &resp)?;
                    continue;
                }

                let broker = broker.clone();
                let writer = writer.clone();
                tokio::spawn(async move {
                    let resp = match broker.command(request_id, anchor, command).await {
                        Ok(result) => ServiceMessage::AgentCommandResult { request_id, result },
                        Err(message) => ServiceMessage::Error { message },
                    };
                    let bytes = match rkyv::to_bytes::<rkyv::rancor::Error>(&resp) {
                        Ok(b) => b,
                        Err(_) => return,
                    };
                    let mut w = writer.lock().await;
                    let _ = crate::framing::write_raw_frame(&mut *w, &bytes).await;
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
                write_message!(&mut *w, &resp)?;
                shutdown_tx.send(()).await.ok();
                break;
            }

            ClientMessage::Status => {
                let uptime_secs = SERVICE_STARTED
                    .get()
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                let process_count = {
                    let mgr = manager.lock().await;
                    mgr.processes.len() as u32
                };
                let resp = ServiceMessage::StatusResponse {
                    uptime_secs,
                    process_count,
                };
                let mut w = writer.lock().await;
                write_message!(&mut *w, &resp)?;
            }

            ClientMessage::AgentQuery { request_id, query } => {
                let response = match query {
                    vmux_api::protocol::AgentQuery::ReadProcessOutput { process_id } => {
                        ServiceMessage::ProcessOutputResult {
                            request_id,
                            result: process_queries.output(process_id).await,
                        }
                    }
                    vmux_api::protocol::AgentQuery::ReadProcessTranscript { process_id } => {
                        ServiceMessage::ProcessTranscriptResult {
                            request_id,
                            result: process_queries.transcript(process_id).await,
                        }
                    }
                    vmux_api::protocol::AgentQuery::ProcessCommandExit { process_id } => {
                        let result = process_queries.command_exit(process_id).await;
                        ServiceMessage::ProcessCommandExitResult { request_id, result }
                    }
                    vmux_api::protocol::AgentQuery::ProcessRunCompletion { process_id } => {
                        let result = process_queries.run_completion(process_id).await;
                        ServiceMessage::ProcessRunCompletionResult { request_id, result }
                    }
                    query => {
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
                            let _ = crate::framing::write_raw_frame(&mut *writer, &bytes).await;
                        });
                        continue;
                    }
                };
                let mut writer = writer.lock().await;
                write_message!(&mut *writer, &response)?;
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
                let pending = pending_commands.lock().await.remove(&request_id);
                if let Some(tx) = pending {
                    let _ = tx.send(result);
                } else {
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
                let result = agent_manager.lock().await.spawn(
                    sid,
                    &provider,
                    model,
                    cwd,
                    tools,
                    auto,
                    broker.clone(),
                );
                if let Err(message) = result {
                    let resp = ServiceMessage::Error { message };
                    let mut w = writer.lock().await;
                    write_message!(&mut *w, &resp)?;
                }
            }

            ClientMessage::Shared(
                SharedMessage::ListSessions
                | SharedMessage::AgentCommand(_)
                | SharedMessage::AgentListMedia { .. },
            ) => {
                tracing::warn!("local socket: ignoring a remote-only request");
            }

            ClientMessage::Shared(SharedMessage::AgentAttach { sid }) => {
                let rx = agent_manager.lock().await.subscribe(&sid);
                if let Some(mut rx) = rx {
                    if let Some(snapshot) = agent_manager.lock().await.snapshot(&sid).await {
                        let mut w = writer.lock().await;
                        write_message!(&mut *w, &snapshot)?;
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
                                    if crate::framing::write_raw_frame(&mut *w, &bytes)
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
                    &acp_manager,
                    &agent_manager,
                    sid,
                    text,
                    context,
                    attachments,
                    preferred_mode,
                )
                .await;
            }

            ClientMessage::RebindAcpWorkspace { sid, cwd } => {
                if let Err(message) = acp_manager
                    .lock()
                    .await
                    .rebind_cwd(&sid, std::path::PathBuf::from(cwd))
                {
                    let resp = ServiceMessage::Error { message };
                    let mut w = writer.lock().await;
                    write_message!(&mut *w, &resp)?;
                }
            }

            ClientMessage::AcpSetModel {
                sid,
                request_id,
                config_id,
                model_id,
            } => {
                acp_manager.lock().await.input(
                    &sid,
                    crate::acp::AcpInput::SetModel {
                        request_id,
                        config_id,
                        model_id,
                    },
                );
            }

            ClientMessage::AcpSetMode {
                sid,
                request_id,
                config_id,
                mode_id,
            } => {
                acp_manager.lock().await.input(
                    &sid,
                    crate::acp::AcpInput::SetMode {
                        request_id,
                        config_id,
                        mode_id,
                    },
                );
            }

            ClientMessage::Shared(SharedMessage::AgentCancel { sid }) => {
                if acp_manager.lock().await.contains(&sid) {
                    acp_manager
                        .lock()
                        .await
                        .input(&sid, crate::acp::AcpInput::Cancel);
                } else {
                    agent_manager
                        .lock()
                        .await
                        .input(&sid, crate::agent::SessionInput::Cancel);
                }
            }

            ClientMessage::Shared(SharedMessage::AgentApprove {
                sid,
                call_id,
                decision,
            }) => {
                if acp_manager.lock().await.contains(&sid) {
                    acp_manager
                        .lock()
                        .await
                        .input(&sid, crate::acp::AcpInput::Approve { call_id, decision });
                } else {
                    agent_manager.lock().await.input(
                        &sid,
                        crate::agent::SessionInput::Approve { call_id, decision },
                    );
                }
            }

            ClientMessage::ClosePageAgent { sid } => {
                if acp_manager.lock().await.contains(&sid) {
                    acp_manager.lock().await.close(&sid);
                } else {
                    agent_manager.lock().await.close(&sid);
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
                acp_manager.lock().await.spawn(
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
                );
                let rx = acp_manager.lock().await.subscribe(&sid);
                if let Some(mut rx) = rx {
                    if let Some(snapshot) = acp_manager.lock().await.snapshot(&sid) {
                        let mut w = writer.lock().await;
                        write_message!(&mut *w, &snapshot)?;
                    }
                    if let Some(agent_info) = acp_manager.lock().await.agent_info(&sid) {
                        let mut w = writer.lock().await;
                        write_message!(&mut *w, &agent_info)?;
                    }
                    if let Some(model_info) = acp_manager.lock().await.model_info(&sid) {
                        let mut w = writer.lock().await;
                        write_message!(&mut *w, &model_info)?;
                    }
                    if let Some(mode_info) = acp_manager.lock().await.mode_info(&sid) {
                        let mut w = writer.lock().await;
                        write_message!(&mut *w, &mode_info)?;
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
                                    if crate::framing::write_raw_frame(&mut *w, &bytes)
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
    use tokio::sync::oneshot;
    use vmux_api::protocol::{AgentCommandResult, AgentQuery, AgentRequestId};

    async fn run_test_server(listener: UnixListener, wake: mpsc::UnboundedSender<ProcessId>) {
        let manager = Arc::new(Mutex::new(ProcessManager::new(wake.clone())));
        let (_query_plugin, process_queries) = ProcessQueryPlugin::new(Arc::clone(&manager), wake);
        let mut server = Box::pin(super::run_server(
            listener,
            Arc::clone(&manager),
            process_queries,
        ));
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
    async fn pending_queries_roundtrips_oneshot() {
        let pending: PendingQueries = Arc::new(Mutex::new(HashMap::new()));
        let request_id = AgentRequestId::new();
        let (tx, rx) = oneshot::channel::<ServiceMessage>();
        pending.lock().await.insert(request_id, tx);

        let response = ServiceMessage::AgentSettingsResult {
            request_id,
            result: Ok(vmux_api::protocol::JsonValue::Object(Vec::new())),
        };
        let resp_tx = pending.lock().await.remove(&request_id).expect("entry");
        resp_tx.send(response).expect("send");

        let received = rx.await.expect("recv");
        assert!(matches!(
            received,
            ServiceMessage::AgentSettingsResult {
                request_id: received_id,
                result: Ok(_),
            } if received_id == request_id
        ));
    }

    #[tokio::test]
    async fn pending_queries_returns_none_for_unknown_request_id() {
        let pending: PendingQueries = Arc::new(Mutex::new(HashMap::new()));
        let request_id = AgentRequestId::new();
        assert!(pending.lock().await.remove(&request_id).is_none());

        let _ = AgentQuery::ReadLayout { anchor: None };
    }

    #[tokio::test]
    async fn pending_commands_roundtrips_oneshot() {
        let pending: PendingCommands = Arc::new(Mutex::new(HashMap::new()));
        let request_id = AgentRequestId::new();
        let (tx, rx) = oneshot::channel::<AgentCommandResult>();
        pending.lock().await.insert(request_id, tx);

        let result = AgentCommandResult::Ok;
        let resp_tx = pending.lock().await.remove(&request_id).expect("entry");
        resp_tx.send(result.clone()).expect("send");

        let received = rx.await.expect("recv");
        assert_eq!(received, result);
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
        crate::framing::write_raw_frame(&mut w, &bytes)
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
        crate::framing::write_raw_frame(&mut w, &bytes)
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
        crate::framing::write_raw_frame(&mut w, &bytes)
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
