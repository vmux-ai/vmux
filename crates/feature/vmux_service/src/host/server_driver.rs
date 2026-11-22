use std::sync::Arc;
use std::time::Instant;

use tokio::net::UnixListener;
use tokio::sync::{mpsc, watch};
use vmux_api::protocol::{ClientMessage, ServiceMessage};
use vmux_process::ProcessRuntime;
use vmux_transport::service::{LocalProtocol, ServiceCodec, ServiceProtocolDriver};
use vmux_transport::service_peer::LocalPeer;

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
    observations: Option<watch::Receiver<vmux_api::remote::Observation>>,
}

impl ConnectionRuntime {
    pub(super) fn add_local(
        &mut self,
        client: u64,
        protocol: Arc<dyn ServiceProtocolDriver>,
        peer: LocalPeer,
        admitted: bool,
    ) {
        self.protocols.push(Arc::new(LocalProtocol::new(
            client, protocol, peer, admitted,
        )));
    }

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
            observations: None,
        }
    }

    pub(super) fn observations(
        mut self,
        observations: Option<watch::Receiver<vmux_api::remote::Observation>>,
    ) -> Self {
        self.observations = observations;
        self
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
            mut observations,
            ..
        } = runtime;
        let (mut reader, writer) = stream.into_split();
        let writer = Arc::new(tokio::sync::Mutex::new(writer));

        let (outbound, inbound) = mpsc::unbounded_channel();
        let mut protocols = protocols
            .iter()
            .map(|protocol| protocol.connect(outbound.clone()))
            .collect::<Vec<_>>();
        let writer_task = Self::writer(writer.clone(), inbound);
        let mut observation_task: Option<tokio::task::JoinHandle<()>> = None;

        let outcome = loop {
            let message: Option<ClientMessage> = match ServiceCodec::read_client(&mut reader).await
            {
                Ok(message) => message,
                Err(error) => {
                    tracing::warn!(%error, "client stream ended mid-frame");
                    break Err(error);
                }
            };
            let Some(message) = message else {
                break Ok(());
            };

            match message {
                ClientMessage::SubscribeRemoteObservations => {
                    if observation_task.is_none()
                        && let Some(observations) = observations.take()
                    {
                        observation_task = Some(
                            ObservationStream {
                                observations,
                                outbound: outbound.clone(),
                            }
                            .start(),
                        );
                    }
                }
                ClientMessage::Shutdown => {
                    tracing::info!("shutdown requested by client; draining");
                    let _ = processes.shutdown().await;
                    let response = ServiceMessage::ProcessList {
                        processes: Vec::new(),
                    };
                    let mut writer = writer.lock().await;
                    let response = ServiceCodec::write_service(&mut *writer, &response).await;
                    shutdown_tx.send(()).await.ok();
                    break response;
                }
                ClientMessage::Status => {
                    let response = ServiceMessage::StatusResponse {
                        uptime_secs: started_at.elapsed().as_secs(),
                        process_count: processes.count().await.unwrap_or_default(),
                    };
                    let mut writer = writer.lock().await;
                    if let Err(error) = ServiceCodec::write_service(&mut *writer, &response).await {
                        break Err(error);
                    }
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
        };

        for protocol in &mut protocols {
            protocol.disconnect().await;
        }
        drop(protocols);
        if let Some(task) = observation_task {
            task.abort();
            let _ = task.await;
        }
        writer_task.abort();
        let _ = writer_task.await;
        outcome
    }

    fn writer(
        writer: Arc<tokio::sync::Mutex<tokio::net::unix::OwnedWriteHalf>>,
        mut inbound: mpsc::UnboundedReceiver<ServiceMessage>,
    ) -> tokio::task::JoinHandle<()> {
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
    }
}

struct ObservationStream {
    observations: watch::Receiver<vmux_api::remote::Observation>,
    outbound: mpsc::UnboundedSender<ServiceMessage>,
}

impl ObservationStream {
    fn start(mut self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let observation = self.observations.borrow_and_update().clone();
                if self
                    .outbound
                    .send(ServiceMessage::RemoteObserved(observation))
                    .is_err()
                {
                    return;
                }
                if self.observations.changed().await.is_err() {
                    return;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn failed_status_write_releases_the_observation_stream_before_returning() {
        let (mut client, stream) = tokio::net::UnixStream::pair().unwrap();
        let (wake, _) = mpsc::unbounded_channel();
        let (processes, owner) = ProcessRuntime::new(wake);
        let mut app = bevy::prelude::App::new();
        app.add_plugins(vmux_process::ProcessPlugin);
        app.world_mut().spawn(owner);
        let (shutdown, _) = mpsc::channel(1);
        let (observations, receiver) = watch::channel(vmux_api::remote::Observation {
            epoch: uuid::Uuid::new_v4(),
            revision: 1,
            phase: vmux_api::remote::RemotePhase::Disabled,
        });
        let runtime = ConnectionRuntime::new(processes, Vec::new(), shutdown, Instant::now())
            .observations(Some(receiver));
        let task = tokio::spawn(Connection::new(stream, runtime).run());
        ServiceCodec::write_client(&mut client, &ClientMessage::SubscribeRemoteObservations)
            .await
            .unwrap();
        assert!(matches!(
            ServiceCodec::read_service(&mut client).await.unwrap(),
            Some(ServiceMessage::RemoteObserved(_))
        ));
        ServiceCodec::write_client(&mut client, &ClientMessage::Status)
            .await
            .unwrap();
        drop(client);
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        while !task.is_finished() {
            app.update();
            assert!(Instant::now() < deadline);
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        assert!(task.await.unwrap().is_err());
        assert_eq!(
            observations.receiver_count(),
            0,
            "failed connection must drain its owned observation task"
        );
    }
}
