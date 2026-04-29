use crate::protocol::{ClientMessage, DaemonMessage, SessionId};
use crate::session::SessionManager;
use crate::{read_message, write_message};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::sync::{Mutex, broadcast};

// rkyv is used directly in the attach forwarder (can't use write_message! macro
// inside a spawned task that doesn't return Result).

/// Run the IPC server loop, accepting connections and dispatching messages.
pub async fn run_server(listener: UnixListener) {
    let manager = Arc::new(Mutex::new(SessionManager::new()));

    // Poll sessions at ~60Hz in background
    let poll_mgr = Arc::clone(&manager);
    tokio::spawn(async move {
        loop {
            {
                let mut mgr = poll_mgr.lock().await;
                let exited = mgr.poll_all();
                for id in exited {
                    mgr.remove_session(&id);
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
    manager: Arc<Mutex<SessionManager>>,
) -> std::io::Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let writer = Arc::new(tokio::sync::Mutex::new(writer));

    // Track which sessions this client is attached to, so we can forward patches.
    let attached: Arc<tokio::sync::Mutex<HashMap<SessionId, tokio::task::JoinHandle<()>>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    loop {
        let msg: Option<ClientMessage> = read_message!(&mut reader, ClientMessage)?;
        let Some(msg) = msg else {
            break; // client disconnected
        };

        match msg {
            ClientMessage::CreateSession {
                shell,
                cwd,
                env,
                cols,
                rows,
            } => {
                let mut mgr = manager.lock().await;
                match mgr.create_session(shell, cwd, env, cols, rows) {
                    Ok(id) => {
                        let resp = DaemonMessage::SessionCreated { session_id: id };
                        let w = writer.clone();
                        let mut w = w.lock().await;
                        write_message!(&mut *w, &resp)?;
                    }
                    Err(e) => {
                        let resp = DaemonMessage::Error { message: e };
                        let w = writer.clone();
                        let mut w = w.lock().await;
                        write_message!(&mut *w, &resp)?;
                    }
                }
            }

            ClientMessage::AttachSession { session_id } => {
                let mgr = manager.lock().await;
                if let Some(session) = mgr.sessions.get(&session_id) {
                    let mut rx = session.subscribe();
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
                    attached.lock().await.insert(session_id, handle);
                } else {
                    let resp = DaemonMessage::Error {
                        message: format!("session not found: {session_id}"),
                    };
                    let mut w = writer.lock().await;
                    write_message!(&mut *w, &resp)?;
                }
            }

            ClientMessage::DetachSession { session_id } => {
                if let Some(handle) = attached.lock().await.remove(&session_id) {
                    handle.abort();
                }
            }

            ClientMessage::SessionInput { session_id, data } => {
                let mgr = manager.lock().await;
                if let Some(session) = mgr.sessions.get(&session_id) {
                    session.write_input(&data);
                }
            }

            ClientMessage::ResizeSession {
                session_id,
                cols,
                rows,
            } => {
                let mut mgr = manager.lock().await;
                if let Some(session) = mgr.sessions.get_mut(&session_id) {
                    session.resize(cols, rows);
                }
            }

            ClientMessage::ListSessions => {
                let mgr = manager.lock().await;
                let sessions = mgr.sessions.values().map(|s| s.info()).collect::<Vec<_>>();
                let resp = DaemonMessage::SessionList { sessions };
                let mut w = writer.lock().await;
                write_message!(&mut *w, &resp)?;
            }

            ClientMessage::KillSession { session_id } => {
                let mut mgr = manager.lock().await;
                mgr.remove_session(&session_id);
                if let Some(handle) = attached.lock().await.remove(&session_id) {
                    handle.abort();
                }
            }

            ClientMessage::RequestSnapshot { session_id } => {
                let mgr = manager.lock().await;
                if let Some(session) = mgr.sessions.get(&session_id) {
                    let snap = session.snapshot();
                    let mut w = writer.lock().await;
                    write_message!(&mut *w, &snap)?;
                } else {
                    let resp = DaemonMessage::Error {
                        message: format!("session not found: {session_id}"),
                    };
                    let mut w = writer.lock().await;
                    write_message!(&mut *w, &resp)?;
                }
            }

            ClientMessage::Shutdown => {
                let mut mgr = manager.lock().await;
                mgr.shutdown();
                let resp = DaemonMessage::SessionList {
                    sessions: Vec::new(),
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
