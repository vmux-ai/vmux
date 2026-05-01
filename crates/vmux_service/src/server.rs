use crate::process::{Process, ProcessManager, PtyInputWriter};
use crate::protocol::{ClientMessage, ProcessId, ServiceMessage};
use crate::{read_message, write_message};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio::time::MissedTickBehavior;

const MAX_WAKE_EVENTS_PER_TICK: usize = 1024;
type InputWriters = Arc<Mutex<HashMap<ProcessId, PtyInputWriter>>>;

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
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, mgr, input_writers).await {
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
) -> std::io::Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let writer = Arc::new(tokio::sync::Mutex::new(writer));

    // Track which processes this client is attached to, so we can forward patches.
    let attached: Arc<tokio::sync::Mutex<HashMap<ProcessId, tokio::task::JoinHandle<()>>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

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
        }
    }

    // Client disconnected — abort all patch forwarders
    for (_, handle) in attached.lock().await.drain() {
        handle.abort();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
