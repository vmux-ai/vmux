use crate::process::{Process, ProcessManager, PtyInputWriter};
use crate::protocol::{ClientMessage, ProcessId, ServiceMessage, validate_agent_command};
use crate::{read_message, write_message};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio::time::MissedTickBehavior;

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

    // Poll at ~60Hz for exits, and immediately when PTY output arrives.
    let poll_mgr = Arc::clone(&manager);
    let poll_input_writers = Arc::clone(&input_writers);
    tokio::spawn(async move {
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
        let (stream, _) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("accept error: {e}");
                continue;
            }
        };
        let mgr = Arc::clone(&manager);
        let input_writers = Arc::clone(&input_writers);
        let agent_tx = agent_tx.clone();
        let pending_queries = Arc::clone(&pending_queries);
        tokio::spawn(async move {
            if let Err(e) =
                handle_client(stream, mgr, input_writers, agent_tx, pending_queries).await
            {
                eprintln!("client error: {e}");
            }
        });
    }
}

fn drain_pending_wakes(wake_rx: &mut mpsc::UnboundedReceiver<ProcessId>) {
    for _ in 0..MAX_WAKE_EVENTS_PER_TICK {
        if wake_rx.try_recv().is_err() {
            break;
        }
    }
}

async fn handle_client(
    stream: tokio::net::UnixStream,
    manager: Arc<Mutex<ProcessManager>>,
    input_writers: InputWriters,
    agent_tx: broadcast::Sender<ServiceMessage>,
    pending_queries: PendingQueries,
) -> std::io::Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let writer = Arc::new(tokio::sync::Mutex::new(writer));

    // Track which processes this client is attached to, so we can forward patches.
    let attached: Arc<tokio::sync::Mutex<HashMap<ProcessId, tokio::task::JoinHandle<()>>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let mut agent_subscription: Option<tokio::task::JoinHandle<()>> = None;
    let mut in_flight_query_ids: std::collections::HashSet<crate::protocol::AgentRequestId> =
        std::collections::HashSet::new();

    loop {
        let msg: Option<ClientMessage> = read_message!(&mut reader, ClientMessage)?;
        let Some(msg) = msg else {
            break; // client disconnected
        };

        match msg {
            ClientMessage::CreateProcess {
                shell,
                cwd,
                env,
                cols,
                rows,
            } => {
                let created = {
                    let mut mgr = manager.lock().await;
                    mgr.create_process(shell, cwd, env, cols, rows)
                        .map(|id| (id, mgr.input_writer(&id)))
                };
                match created {
                    Ok((id, input_writer)) => {
                        if let Some(input_writer) = input_writer {
                            input_writers.lock().await.insert(id, input_writer);
                        }
                        let resp = ServiceMessage::ProcessCreated { process_id: id };
                        let w = writer.clone();
                        let mut w = w.lock().await;
                        write_message!(&mut *w, &resp)?;
                    }
                    Err(e) => {
                        let resp = ServiceMessage::Error { message: e };
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
                command,
            } => {
                let resp = if let Err(message) = validate_agent_command(&command) {
                    ServiceMessage::Error {
                        message: message.to_string(),
                    }
                } else if agent_tx.receiver_count() == 0 {
                    ServiceMessage::Error {
                        message: "no desktop subscribed to agent commands".to_string(),
                    }
                } else {
                    match agent_tx.send(ServiceMessage::AgentCommand {
                        request_id,
                        command,
                    }) {
                        Ok(_) => ServiceMessage::AgentCommandAccepted { request_id },
                        Err(_) => ServiceMessage::Error {
                            message: "no desktop subscribed to agent commands".to_string(),
                        },
                    }
                };
                let mut w = writer.lock().await;
                write_message!(&mut *w, &resp)?;
            }

            ClientMessage::Shutdown => {
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
                // After responding, exit
                std::process::exit(0);
            }
            ClientMessage::AgentQuery { request_id, query } => {
                if agent_tx.receiver_count() == 0 {
                    let resp = ServiceMessage::Error {
                        message: "no desktop subscribed to agent commands".to_string(),
                    };
                    let mut w = writer.lock().await;
                    write_message!(&mut *w, &resp)?;
                    continue;
                }

                let (tx, rx) = tokio::sync::oneshot::channel::<crate::protocol::AgentQueryResult>();
                pending_queries.lock().await.insert(request_id, tx);
                in_flight_query_ids.insert(request_id);

                if agent_tx
                    .send(ServiceMessage::AgentQuery { request_id, query })
                    .is_err()
                {
                    pending_queries.lock().await.remove(&request_id);
                    in_flight_query_ids.remove(&request_id);
                    let resp = ServiceMessage::Error {
                        message: "no desktop subscribed to agent commands".to_string(),
                    };
                    let mut w = writer.lock().await;
                    write_message!(&mut *w, &resp)?;
                    continue;
                }

                let writer = writer.clone();
                let pending_queries = Arc::clone(&pending_queries);
                tokio::spawn(async move {
                    let resp = match tokio::time::timeout(crate::protocol::AGENT_QUERY_TIMEOUT, rx)
                        .await
                    {
                        Ok(Ok(result)) => ServiceMessage::AgentQueryResult { request_id, result },
                        _ => {
                            pending_queries.lock().await.remove(&request_id);
                            ServiceMessage::Error {
                                message: "agent query timed out".to_string(),
                            }
                        }
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
                if let Some(tx) = pending_queries.lock().await.remove(&request_id) {
                    let _ = tx.send(result);
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

    {
        let mut pending = pending_queries.lock().await;
        for id in &in_flight_query_ids {
            pending.remove(id);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AgentQuery, AgentQueryResult, AgentRequestId, FocusedInfo};
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

        let result = AgentQueryResult::Focused(FocusedInfo {
            space: None,
            pane: None,
            tab: None,
        });
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

        let _ = AgentQuery::ListTabs;
    }
}
