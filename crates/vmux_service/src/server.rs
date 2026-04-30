use crate::process::{Process, ProcessManager};
use crate::protocol::{ClientMessage, ProcessId, ServiceMessage};
use crate::{read_message, write_message};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::sync::{Mutex, broadcast};

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
    let manager = Arc::new(Mutex::new(ProcessManager::new()));

    // Poll processes at ~60Hz in background
    let poll_mgr = Arc::clone(&manager);
    tokio::spawn(async move {
        loop {
            {
                let mut mgr = poll_mgr.lock().await;
                let exited = mgr.poll_all();
                for id in exited {
                    mgr.remove_process(&id);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
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
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, mgr).await {
                eprintln!("client error: {e}");
            }
        });
    }
}

async fn handle_client(
    stream: tokio::net::UnixStream,
    manager: Arc<Mutex<ProcessManager>>,
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
                let mut mgr = manager.lock().await;
                match mgr.create_process(shell, cwd, env, cols, rows) {
                    Ok(id) => {
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
                let mgr = manager.lock().await;
                if let Some(process) = mgr.processes.get(&process_id)
                    && !process.is_copy_mode()
                {
                    process.write_input(&data);
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
                with_process_mut(&manager, process_id, |process| {
                    process.set_selection(range)
                })
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
                with_process_mut(&manager, process_id, |process| {
                    process.select_line_at(row)
                })
                .await;
            }

            ClientMessage::GetSelectionText { process_id } => {
                let text = with_process_mut(&manager, process_id, |process| process.selection_text())
                    .await
                    .flatten()
                    .unwrap_or_default();
                let resp = ServiceMessage::SelectionText { process_id, text };
                let mut w = writer.lock().await;
                write_message!(&mut *w, &resp)?;
            }

            ClientMessage::EnterCopyMode { process_id } => {
                with_process_mut(&manager, process_id, |process| {
                    process.enter_copy_mode()
                })
                .await;
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
                let mut mgr = manager.lock().await;
                mgr.shutdown();
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
