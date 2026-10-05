use std::sync::Arc;
use std::time::Instant;

use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use vmux_api::protocol::{ClientMessage, ServiceMessage};
use vmux_process::ProcessRuntime;
use vmux_transport::service::{ServiceCodec, ServiceProtocolDriver};

pub(super) struct Listener {
    listener: UnixListener,
    connections: mpsc::UnboundedSender<tokio::net::UnixStream>,
    wake: mpsc::UnboundedSender<()>,
    shutdown: mpsc::Receiver<()>,
    exit: mpsc::Sender<()>,
}

impl Listener {
    pub(super) fn new(
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

    pub(super) async fn run(mut self) {
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

#[derive(Clone)]
pub(super) struct ConnectionRuntime {
    processes: ProcessRuntime,
    protocols: Vec<Arc<dyn ServiceProtocolDriver>>,
    shutdown: mpsc::Sender<()>,
    started_at: Instant,
}

impl ConnectionRuntime {
    pub(super) fn new(
        processes: ProcessRuntime,
        protocols: Vec<Arc<dyn ServiceProtocolDriver>>,
        shutdown: mpsc::Sender<()>,
        started_at: Instant,
    ) -> Self {
        Self {
            processes,
            protocols,
            shutdown,
            started_at,
        }
    }
}

pub(super) struct Connection {
    stream: tokio::net::UnixStream,
    runtime: ConnectionRuntime,
}

impl Connection {
    pub(super) fn new(stream: tokio::net::UnixStream, runtime: ConnectionRuntime) -> Self {
        Self { stream, runtime }
    }

    pub(super) async fn run(self) -> std::io::Result<()> {
        let Self { stream, runtime } = self;
        let ConnectionRuntime {
            processes,
            protocols,
            shutdown: shutdown_tx,
            started_at,
            ..
        } = runtime;
        let (reader, writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let writer = Arc::new(tokio::sync::Mutex::new(writer));

        let (outbound, mut inbound) = mpsc::unbounded_channel();
        let mut protocols = protocols
            .iter()
            .map(|protocol| protocol.connect(outbound.clone()))
            .collect::<Vec<_>>();
        let writer_task = {
            let writer = writer.clone();
            tokio::spawn(async move {
                while let Some(message) = inbound.recv().await {
                    let mut writer = writer.lock().await;
                    if ServiceCodec::write_service(&mut *writer, &message)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            })
        };

        loop {
            let message: Option<ClientMessage> = match ServiceCodec::read_client(&mut reader).await
            {
                Ok(message) => message,
                Err(error) => {
                    tracing::warn!(%error, "client stream ended mid-frame");
                    break;
                }
            };
            let Some(message) = message else {
                break;
            };

            match message {
                ClientMessage::Shutdown => {
                    tracing::info!("shutdown requested by client; draining");
                    let _ = processes.shutdown().await;
                    let response = ServiceMessage::ProcessList {
                        processes: Vec::new(),
                    };
                    let mut writer = writer.lock().await;
                    ServiceCodec::write_service(&mut *writer, &response).await?;
                    shutdown_tx.send(()).await.ok();
                    break;
                }
                ClientMessage::Status => {
                    let response = ServiceMessage::StatusResponse {
                        uptime_secs: started_at.elapsed().as_secs(),
                        process_count: processes.count().await.unwrap_or_default(),
                    };
                    let mut writer = writer.lock().await;
                    ServiceCodec::write_service(&mut *writer, &response).await?;
                }
                message => {
                    let mut pending = message;
                    let mut handled = false;
                    for protocol in &mut protocols {
                        match protocol.dispatch(pending).await {
                            Ok(()) => {
                                handled = true;
                                break;
                            }
                            Err(message) => pending = message,
                        }
                    }
                    if !handled {
                        tracing::warn!("unsupported local service request");
                    }
                }
            }
        }

        for protocol in &mut protocols {
            protocol.disconnect().await;
        }
        drop(protocols);
        writer_task.abort();
        Ok(())
    }
}
