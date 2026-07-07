use crate::process::{Process, ProcessManager, PtyInputWriter};
use crate::protocol::{ClientMessage, ProcessId, ServiceMessage, validate_agent_command};
use crate::{read_message, write_message};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Instant;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio::time::MissedTickBehavior;

static SERVICE_STARTED: OnceLock<Instant> = OnceLock::new();

pub(crate) fn init_started_at() {
    SERVICE_STARTED.get_or_init(Instant::now);
}

const MAX_WAKE_EVENTS_PER_TICK: usize = 1024;
type InputWriters = Arc<Mutex<HashMap<ProcessId, PtyInputWriter>>>;
type PendingQueries = Arc<
    Mutex<
        HashMap<
            crate::protocol::AgentRequestId,
            tokio::sync::oneshot::Sender<crate::protocol::AgentQueryResult>,
        >,
    >,
>;
type PendingCommands = Arc<
    Mutex<
        HashMap<
            crate::protocol::AgentRequestId,
            tokio::sync::oneshot::Sender<crate::protocol::AgentCommandResult>,
        >,
    >,
>;

/// Acquire the manager lock and run `f` against the process if it exists.
/// Returns Some(result) when the process was found, None otherwise.
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

// rkyv is used directly in the attach forwarder (can't use write_message! macro
// inside a spawned task that doesn't return Result).

/// Run the IPC server loop, accepting connections and dispatching messages.
pub async fn run_server(listener: UnixListener) {
    let (wake_tx, mut wake_rx) = mpsc::unbounded_channel();
    let manager = Arc::new(Mutex::new(ProcessManager::new(wake_tx)));
    let input_writers = Arc::new(Mutex::new(HashMap::new()));
    let (agent_tx, _) = broadcast::channel::<ServiceMessage>(128);
    let pending_queries: PendingQueries = Arc::new(Mutex::new(HashMap::new()));
    let pending_commands: PendingCommands = Arc::new(Mutex::new(HashMap::new()));
    let pending_tool_calls: crate::agent_broker::PendingToolCalls =
        Arc::new(Mutex::new(HashMap::new()));
    let agent_manager = Arc::new(Mutex::new(crate::agent::AgentSessionManager::default()));
    let acp_manager = Arc::new(Mutex::new(crate::acp::AcpSessionManager::default()));
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

    init_started_at();

    let poll_mgr = Arc::clone(&manager);
    let poll_input_writers = Arc::clone(&input_writers);
    let poll_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(16));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                Some(_) = wake_rx.recv() => {
                    drain_pending_wakes(&mut wake_rx);
                }
            }

            let exited = {
                let mut mgr = poll_mgr.lock().await;
                let exited = mgr.poll_all();
                for id in &exited {
                    mgr.remove_process(id);
                }
                exited
            };
            if !exited.is_empty() {
                let mut writers = poll_input_writers.lock().await;
                for id in exited {
                    writers.remove(&id);
                }
            }
        }
    });

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
                let input_writers = Arc::clone(&input_writers);
                let agent_tx = agent_tx.clone();
                let pending_queries = Arc::clone(&pending_queries);
                let pending_commands = Arc::clone(&pending_commands);
                let pending_tool_calls = Arc::clone(&pending_tool_calls);
                let agent_manager = Arc::clone(&agent_manager);
                let acp_manager = Arc::clone(&acp_manager);
                let shutdown_tx = shutdown_tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(
                        stream,
                        mgr,
                        input_writers,
                        agent_tx,
                        pending_queries,
                        pending_commands,
                        pending_tool_calls,
                        agent_manager,
                        acp_manager,
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
    poll_handle.abort();
    tracing::info!("server: drain complete, exiting");
}

fn drain_pending_wakes(wake_rx: &mut mpsc::UnboundedReceiver<ProcessId>) {
    for _ in 0..MAX_WAKE_EVENTS_PER_TICK {
        if wake_rx.try_recv().is_err() {
            break;
        }
    }
}

fn command_result_to_content(result: crate::protocol::AgentCommandResult) -> (String, bool) {
    use crate::protocol::AgentCommandResult;
    match result {
        AgentCommandResult::Ok => ("ok".to_string(), false),
        AgentCommandResult::Text(text) => (text, false),
        AgentCommandResult::Layout(snapshot) => {
            (serde_json::to_string(&snapshot).unwrap_or_default(), false)
        }
        AgentCommandResult::Error(message) => (message, true),
    }
}

fn query_result_to_content(result: crate::protocol::AgentQueryResult) -> (String, bool) {
    use crate::protocol::AgentQueryResult;
    match result {
        AgentQueryResult::Layout(snapshot) => {
            (serde_json::to_string(&snapshot).unwrap_or_default(), false)
        }
        AgentQueryResult::Text(text) => (text, false),
        AgentQueryResult::Settings(json) => (json, false),
        AgentQueryResult::Spaces(json) => (json, false),
        AgentQueryResult::CommandExit { seq, exit } => {
            let exit = exit.map_or_else(|| "null".to_string(), |code| code.to_string());
            (format!("{{\"seq\":{seq},\"exit\":{exit}}}"), false)
        }
        AgentQueryResult::RunCompletion { token, exit } => {
            let token = token.map_or_else(|| "null".to_string(), |t| format!("\"{t}\""));
            let exit = exit.map_or_else(|| "null".to_string(), |code| code.to_string());
            (format!("{{\"token\":{token},\"exit\":{exit}}}"), false)
        }
        AgentQueryResult::Image {
            path,
            width,
            height,
            ..
        } => (format!("saved {path} ({width}×{height})"), false),
        AgentQueryResult::Recording {
            mp4_path,
            gif_path,
            duration_ms,
            bytes,
            auto_stopped,
        } => {
            let secs = duration_ms as f64 / 1000.0;
            let gif = gif_path.map(|g| format!(" + {g}")).unwrap_or_default();
            let auto = if auto_stopped { " (auto-stopped)" } else { "" };
            (
                format!("recorded {secs:.1}s -> {mp4_path} ({bytes} bytes){gif}{auto}"),
                false,
            )
        }
        AgentQueryResult::Error(message) => (message, true),
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_client(
    stream: tokio::net::UnixStream,
    manager: Arc<Mutex<ProcessManager>>,
    input_writers: InputWriters,
    agent_tx: broadcast::Sender<ServiceMessage>,
    pending_queries: PendingQueries,
    pending_commands: PendingCommands,
    pending_tool_calls: crate::agent_broker::PendingToolCalls,
    agent_manager: Arc<Mutex<crate::agent::AgentSessionManager>>,
    acp_manager: Arc<Mutex<crate::acp::AcpSessionManager>>,
    shutdown_tx: mpsc::Sender<()>,
) -> std::io::Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let writer = Arc::new(tokio::sync::Mutex::new(writer));

    // Track which processes this client is attached to, so we can forward patches.
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

    // Processes created by this client. The desktop is the sole owner of its
    // terminals, so when it disconnects (including via a crash) these are reaped
    // below — otherwise PTY children outlive the GUI and leak PTYs across runs.
    let mut created_processes: Vec<ProcessId> = Vec::new();

    loop {
        let msg: Option<ClientMessage> = read_message!(&mut reader, ClientMessage)?;
        let Some(msg) = msg else {
            break; // client disconnected
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
                        .map(|(id, pid)| (id, pid, mgr.input_writer(&id)))
                };
                match created {
                    Ok((id, pid, input_writer)) => {
                        created_processes.push(id);
                        if let Some(input_writer) = input_writer {
                            input_writers.lock().await.insert(id, input_writer);
                        }
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
                                Err(broadcast::error::RecvError::Lagged(_)) => continue,
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
                let can_write = {
                    let mgr = manager.lock().await;
                    mgr.processes
                        .get(&process_id)
                        .is_some_and(|process| !process.is_copy_mode())
                };
                let writer = if can_write {
                    input_writers.lock().await.get(&process_id).cloned()
                } else {
                    None
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
                input_writers.lock().await.remove(&process_id);
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
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
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
                input_writers.lock().await.clear();
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
                // ReadTerminal is answered by the service directly (it owns the
                // terminal state); other queries relay to the GUI.
                let query = match query {
                    crate::protocol::AgentQuery::ReadTerminal { process_id } => {
                        let result = {
                            let mgr = manager.lock().await;
                            match mgr.processes.get(&process_id) {
                                Some(process) => {
                                    let text = match process.snapshot() {
                                        ServiceMessage::Snapshot { lines, .. } => lines
                                            .iter()
                                            .map(|line| {
                                                line.spans
                                                    .iter()
                                                    .map(|span| span.text.as_str())
                                                    .collect::<String>()
                                            })
                                            .collect::<Vec<_>>()
                                            .join("\n"),
                                        _ => String::new(),
                                    };
                                    crate::protocol::AgentQueryResult::Text(text)
                                }
                                None => crate::protocol::AgentQueryResult::Error(format!(
                                    "process not found: {process_id}"
                                )),
                            }
                        };
                        let resp = ServiceMessage::AgentQueryResult { request_id, result };
                        let mut w = writer.lock().await;
                        write_message!(&mut *w, &resp)?;
                        continue;
                    }
                    crate::protocol::AgentQuery::ReadTerminalFull { process_id } => {
                        let result = {
                            let mgr = manager.lock().await;
                            match mgr.processes.get(&process_id) {
                                Some(process) => {
                                    crate::protocol::AgentQueryResult::Text(process.full_text())
                                }
                                None => crate::protocol::AgentQueryResult::Error(format!(
                                    "process not found: {process_id}"
                                )),
                            }
                        };
                        let resp = ServiceMessage::AgentQueryResult { request_id, result };
                        let mut w = writer.lock().await;
                        write_message!(&mut *w, &resp)?;
                        continue;
                    }
                    crate::protocol::AgentQuery::CommandExit { process_id } => {
                        let result = {
                            let mgr = manager.lock().await;
                            match mgr.processes.get(&process_id) {
                                Some(process) => {
                                    let (seq, exit) = process.command_status();
                                    crate::protocol::AgentQueryResult::CommandExit { seq, exit }
                                }
                                None => crate::protocol::AgentQueryResult::Error(format!(
                                    "process not found: {process_id}"
                                )),
                            }
                        };
                        let resp = ServiceMessage::AgentQueryResult { request_id, result };
                        let mut w = writer.lock().await;
                        write_message!(&mut *w, &resp)?;
                        continue;
                    }
                    crate::protocol::AgentQuery::RunCompletion { process_id } => {
                        let result = {
                            let mgr = manager.lock().await;
                            match mgr.processes.get(&process_id) {
                                Some(process) => {
                                    let (token, exit) = match process.run_completion() {
                                        Some((token, exit)) => (Some(token), Some(exit)),
                                        None => (None, None),
                                    };
                                    crate::protocol::AgentQueryResult::RunCompletion { token, exit }
                                }
                                None => crate::protocol::AgentQueryResult::Error(format!(
                                    "process not found: {process_id}"
                                )),
                            }
                        };
                        let resp = ServiceMessage::AgentQueryResult { request_id, result };
                        let mut w = writer.lock().await;
                        write_message!(&mut *w, &resp)?;
                        continue;
                    }
                    other => other,
                };

                let broker = broker.clone();
                let writer = writer.clone();
                tokio::spawn(async move {
                    let resp = match broker.query(request_id, query).await {
                        Ok(result) => ServiceMessage::AgentQueryResult { request_id, result },
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

            ClientMessage::AgentQueryResponse { request_id, result } => {
                let pending = pending_queries.lock().await.remove(&request_id);
                if let Some(tx) = pending {
                    let _ = tx.send(result);
                } else {
                    let (content, is_error) = query_result_to_content(result);
                    broker.resolve_tool(request_id, content, is_error).await;
                }
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
                cwd: _,
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

            ClientMessage::AttachPageAgent { sid } => {
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
                                Err(broadcast::error::RecvError::Lagged(_)) => continue,
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

            ClientMessage::AgentInput { sid, text } => {
                if acp_manager.lock().await.contains(&sid) {
                    acp_manager
                        .lock()
                        .await
                        .input(&sid, crate::acp::AcpInput::User(text));
                } else {
                    agent_manager
                        .lock()
                        .await
                        .input(&sid, crate::agent::SessionInput::User(text));
                }
            }

            ClientMessage::AgentCancel { sid } => {
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

            ClientMessage::AgentApprove {
                sid,
                call_id,
                decision,
            } => {
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
                agent_id: _,
                command,
                args,
                env,
                cwd,
                anchor,
                mcp_command,
                mcp_args,
                resume_acp_session_id,
            } => {
                let mcp_servers = mcp_command
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
                acp_manager.lock().await.spawn(
                    sid.clone(),
                    command,
                    args,
                    env,
                    std::path::PathBuf::from(cwd),
                    anchor,
                    mcp_servers,
                    resume_acp_session_id,
                );
                // ACP has no separate Attach message; forward this session's stream now.
                let rx = acp_manager.lock().await.subscribe(&sid);
                if let Some(mut rx) = rx {
                    if let Some(snapshot) = acp_manager.lock().await.snapshot(&sid) {
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
                                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                                Err(broadcast::error::RecvError::Closed) => break,
                            }
                        }
                    });
                    page_agent_forwarders.insert(sid, handle);
                }
            }
        }
    }

    // Client disconnected — abort all patch forwarders
    for (_, handle) in attached.lock().await.drain() {
        handle.abort();
    }
    if let Some(handle) = agent_subscription.take() {
        handle.abort();
    }
    for (_, handle) in page_agent_forwarders.drain() {
        handle.abort();
    }

    // Reap the processes this client created so a disconnected/crashed desktop
    // never orphans PTY children (which would exhaust the system PTY pool).
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
    use crate::protocol::{AgentCommandResult, AgentQuery, AgentQueryResult, AgentRequestId};
    use tokio::sync::oneshot;

    #[test]
    fn wake_drain_leaves_excess_events_for_later_ticks() {
        let (wake_tx, mut wake_rx) = mpsc::unbounded_channel();
        for _ in 0..=MAX_WAKE_EVENTS_PER_TICK {
            wake_tx
                .send(ProcessId::new())
                .expect("wake event should queue");
        }

        drain_pending_wakes(&mut wake_rx);

        assert!(wake_rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn pending_queries_roundtrips_oneshot() {
        let pending: PendingQueries = Arc::new(Mutex::new(HashMap::new()));
        let request_id = AgentRequestId::new();
        let (tx, rx) = oneshot::channel::<AgentQueryResult>();
        pending.lock().await.insert(request_id, tx);

        let result = AgentQueryResult::Settings("{}".into());
        let resp_tx = pending.lock().await.remove(&request_id).expect("entry");
        resp_tx.send(result.clone()).expect("send");

        let received = rx.await.expect("recv");
        assert_eq!(received, result);
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
        use crate::protocol::ClientMessage;

        let dir = std::env::temp_dir().join(format!("vmux-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("test.sock");
        let _ = std::fs::remove_file(&sock);
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();

        let server = tokio::spawn(super::run_server(listener));

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
        use crate::protocol::ClientMessage;

        let dir = std::env::temp_dir().join(format!("vmux-reap-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("reap.sock");
        let pidfile = dir.join("child.pid");
        let _ = std::fs::remove_file(&sock);
        let _ = std::fs::remove_file(&pidfile);
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();

        let server = tokio::spawn(super::run_server(listener));

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

        // Simulate a desktop crash: drop the client connection without Shutdown.
        drop(w);
        drop(r);

        let reaped = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while process_alive(pid, &identity) {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;

        // Hygiene: ensure no leaked child regardless of outcome.
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
}
