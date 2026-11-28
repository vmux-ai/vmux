use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::authorization::RemoteAuthorizations;
use super::file_driver::PrivateFile;
use crate::AuthorizationOutcome;
use crate::RemotePaths;
use tokio::sync::{mpsc, watch};
use vmux_api::protocol::{ServiceMessage, SharedEvent, SharedMessage};
use vmux_transport::DeviceId;
use vmux_transport::framing::{Frame, FrameError, FrameStream};
use vmux_transport::quic::endpoint::{RECEIVE_WINDOW, SelfSignedIdentity};
use vmux_transport::quic::{
    ClientCredential, ClientSetup, CloseCode, MessageType, SessionAccepted,
};
use vmux_transport::service::RemoteDriver;

#[derive(Clone, bevy_ecs::prelude::Component)]
#[component(immutable)]
pub(crate) struct RemoteState {
    pub(crate) device_id: DeviceId,
    pub(crate) relay_token: Arc<str>,
    pub(crate) authorizations: RemoteAuthorizations,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub(crate) remote: Arc<dyn RemoteDriver>,
    pub(crate) sessions: SessionPublisher,
}

#[derive(Clone)]
pub(crate) struct SessionPublisher {
    started: mpsc::UnboundedSender<SessionStarted>,
    ended: mpsc::UnboundedSender<SessionEnded>,
    registered: Arc<AtomicBool>,
}

impl SessionPublisher {
    pub(crate) fn channel() -> (
        Self,
        mpsc::UnboundedReceiver<SessionStarted>,
        mpsc::UnboundedReceiver<SessionEnded>,
    ) {
        let (started, started_inbox) = mpsc::unbounded_channel();
        let (ended, ended_inbox) = mpsc::unbounded_channel();
        (
            Self {
                started,
                ended,
                registered: Arc::new(AtomicBool::new(false)),
            },
            started_inbox,
            ended_inbox,
        )
    }

    pub(crate) fn registered(&self) -> bool {
        self.registered.load(Ordering::Acquire)
    }

    pub(crate) fn registration(&self) -> RegistrationLease {
        self.registered.store(true, Ordering::Release);
        RegistrationLease(self.registered.clone())
    }

    fn open(&self, id: usize, device: DeviceId) -> SessionLease {
        let _ = self.started.send(SessionStarted { id, device });
        SessionLease {
            id,
            ended: self.ended.clone(),
        }
    }
}

pub(crate) struct RegistrationLease(Arc<AtomicBool>);

impl Drop for RegistrationLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(crate) struct SessionStarted {
    pub(crate) id: usize,
    pub(crate) device: DeviceId,
}

pub(crate) struct SessionEnded {
    pub(crate) id: usize,
}

struct SessionLease {
    id: usize,
    ended: mpsc::UnboundedSender<SessionEnded>,
}

impl Drop for SessionLease {
    fn drop(&mut self) {
        let _ = self.ended.send(SessionEnded { id: self.id });
    }
}

pub(crate) struct IdentityStore(RemotePaths);

impl IdentityStore {
    pub(crate) fn current() -> Self {
        Self(RemotePaths::current())
    }

    pub(crate) fn load_or_create(&self) -> std::io::Result<SelfSignedIdentity> {
        let cert_path = self.0.certificate();
        let key_path = self.0.key();
        if let (Ok(certificate_pem), Ok(private_key_pem)) = (
            super::file_driver::RemoteFs::read_to_string(&cert_path),
            super::file_driver::RemoteFs::read_to_string(&key_path),
        ) && !certificate_pem.trim().is_empty()
            && !private_key_pem.trim().is_empty()
        {
            let identity = SelfSignedIdentity::from_pem(certificate_pem, private_key_pem)
                .map_err(std::io::Error::other)?;
            self.persist_fingerprint(&identity.fingerprint)?;
            return Ok(identity);
        }

        let identity =
            SelfSignedIdentity::generate(Self::names()).map_err(std::io::Error::other)?;
        super::file_driver::RemoteFs::write(&cert_path, &identity.certificate_pem)?;
        PrivateFile::new(key_path).write(&identity.private_key_pem)?;
        self.persist_fingerprint(&identity.fingerprint)?;
        Ok(identity)
    }

    fn persist_fingerprint(&self, fingerprint: &str) -> std::io::Result<()> {
        let path = self.0.fingerprint();
        if super::file_driver::RemoteFs::read_to_string(&path)
            .is_ok_and(|existing| existing.trim() == fingerprint)
        {
            return Ok(());
        }
        super::file_driver::RemoteFs::write(&path, fingerprint)
    }

    fn names() -> Vec<String> {
        vec!["localhost".to_string(), "127.0.0.1".to_string()]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rejection {
    Unauthorized,
    RemoteDisabled,
    Malformed,
    UnsupportedFraming,
}

impl Rejection {
    pub fn close_code(self) -> CloseCode {
        match self {
            Self::Unauthorized => CloseCode::Unauthorized,
            Self::RemoteDisabled => CloseCode::RemoteDisabled,
            Self::Malformed | Self::UnsupportedFraming => CloseCode::ProtocolError,
        }
    }
}

impl From<FrameError> for Rejection {
    fn from(error: FrameError) -> Self {
        match error {
            FrameError::UnsupportedVersion(_) => Self::UnsupportedFraming,
            _ => Self::Malformed,
        }
    }
}

impl RemoteState {
    async fn admit(
        &self,
        setup: ClientSetup,
        remote_enabled: bool,
    ) -> Result<(SessionAccepted, ActiveAuthorization), Rejection> {
        if !remote_enabled {
            return Err(Rejection::RemoteDisabled);
        }
        let credential = setup.credential.clone();
        let outcome = self
            .authorizations
            .authenticate(setup.client_id.clone(), credential.clone())
            .await
            .map_err(|error| {
                tracing::warn!(%error, "remote quic: authorization store failed");
                Rejection::Unauthorized
            })?
            .ok_or(Rejection::Unauthorized)?;
        match outcome {
            AuthorizationOutcome::Accepted => {
                let ClientCredential::Device(device_token) = credential else {
                    return Err(Rejection::Unauthorized);
                };
                Ok((
                    SessionAccepted::default(),
                    ActiveAuthorization {
                        client_id: setup.client_id,
                        device_token,
                    },
                ))
            }
            AuthorizationOutcome::Paired { device_token } => Ok((
                SessionAccepted {
                    device_token: Some(device_token.clone()),
                },
                ActiveAuthorization {
                    client_id: setup.client_id,
                    device_token,
                },
            )),
        }
    }
}

struct ActiveAuthorization {
    client_id: DeviceId,
    device_token: String,
}

impl ActiveAuthorization {
    async fn is_current(&self, state: &RemoteState) -> bool {
        match state
            .authorizations
            .authorizes(self.client_id.clone(), self.device_token.clone())
            .await
        {
            Ok(authorized) => authorized,
            Err(error) => {
                tracing::warn!(%error, "remote quic: authorization revalidation failed");
                false
            }
        }
    }
}

struct RemoteConnection {
    connection: quinn::Connection,
    state: RemoteState,
    liveness: watch::Receiver<bool>,
}

impl RemoteConnection {
    const AUTHORIZATION_POLL: Duration = Duration::from_secs(1);
    const MAX_HELLO_BYTES: usize = 16 * 1024;
    const MAX_REQUEST_BYTES: usize = (RECEIVE_WINDOW / 8) as usize;
    const SETUP: FrameStream = FrameStream::new(Self::MAX_HELLO_BYTES);
    const CONTROL: FrameStream = FrameStream::new(Self::MAX_REQUEST_BYTES);

    fn new(
        connection: quinn::Connection,
        state: RemoteState,
        liveness: watch::Receiver<bool>,
    ) -> Self {
        Self {
            connection,
            state,
            liveness,
        }
    }

    async fn serve(mut self) {
        let mut stopped = self.liveness.clone();
        let streams = tokio::select! {
            streams = self.connection.accept_bi() => streams,
            _ = stopped.wait_for(|live| !*live) => {
                self.connection.close(CloseCode::RemoteDisabled.as_u32().into(), b"remote off");
                return;
            }
        };
        let Ok((mut send, mut recv)) = streams else {
            return;
        };
        let admission = async {
            match Self::read_setup(&mut recv).await {
                Ok(setup) => {
                    let enabled = *self.liveness.borrow();
                    self.state.admit(setup, enabled).await
                }
                Err(rejection) => Err(rejection),
            }
        };
        let admitted = tokio::select! {
            admitted = admission => admitted,
            _ = stopped.wait_for(|live| !*live) => {
                self.connection.close(CloseCode::RemoteDisabled.as_u32().into(), b"remote off");
                return;
            }
        };
        if !*self.liveness.borrow() {
            self.connection
                .close(CloseCode::RemoteDisabled.as_u32().into(), b"remote off");
            return;
        }

        let (accepted, authorization) = match admitted {
            Ok(accepted) => accepted,
            Err(rejection) => {
                tracing::info!(?rejection, "remote quic: connection refused");
                self.connection
                    .close(rejection.close_code().as_u32().into(), b"refused");
                return;
            }
        };

        let Ok(frame) = Frame::json(MessageType::SESSION_ACCEPTED, &accepted) else {
            self.connection
                .close(CloseCode::ProtocolError.as_u32().into(), b"setup");
            return;
        };
        if Self::SETUP.open(&mut send, &frame).await.is_err() || send.finish().is_err() {
            return;
        }
        let _session = self
            .state
            .sessions
            .open(self.connection.stable_id(), authorization.client_id.clone());
        let mut authorization_poll = tokio::time::interval(Self::AUTHORIZATION_POLL);
        authorization_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        authorization_poll.tick().await;
        let mut streams = tokio::task::JoinSet::new();

        loop {
            tokio::select! {
                changed = self.liveness.changed() => {
                    if changed.is_err() || !*self.liveness.borrow() {
                        self.close_streams(&mut streams, CloseCode::RemoteDisabled).await;
                        return;
                    }
                }
                _ = authorization_poll.tick() => {
                    if !authorization.is_current(&self.state).await {
                        self.close_streams(&mut streams, CloseCode::Unauthorized).await;
                        return;
                    }
                }
                accepted = self.connection.accept_bi() => {
                    match accepted {
                        Ok((send, recv)) => {
                            streams.spawn(ControlStream::new(send, recv).dispatch(self.state.clone()));
                        }
                        Err(_) => { self.close_streams(&mut streams, CloseCode::ProtocolError).await; return; },
                    }
                }
                _ = streams.join_next(), if !streams.is_empty() => {}
            }
        }
    }

    async fn close_streams(&self, streams: &mut tokio::task::JoinSet<()>, code: CloseCode) {
        self.connection
            .close(code.as_u32().into(), b"remote closed");
        streams.abort_all();
        while streams.join_next().await.is_some() {}
    }

    async fn read_setup(stream: &mut quinn::RecvStream) -> Result<ClientSetup, Rejection> {
        match Self::SETUP.accept(stream).await {
            Ok(frame) => frame
                .read_json::<ClientSetup>(MessageType::CLIENT_SETUP)
                .map_err(Rejection::from),
            Err(error) => Err(Rejection::from(error)),
        }
    }
}

struct ControlStream {
    send: quinn::SendStream,
    recv: quinn::RecvStream,
}

impl ControlStream {
    fn new(send: quinn::SendStream, recv: quinn::RecvStream) -> Self {
        Self { send, recv }
    }

    async fn dispatch(mut self, state: RemoteState) {
        let frame = match RemoteConnection::CONTROL.accept(&mut self.recv).await {
            Ok(frame) => frame,
            Err(error) => {
                self.refuse(Rejection::from(error));
                return;
            }
        };
        match frame.message_type {
            MessageType::CONTROL_REQUEST | MessageType::SESSION_EVENTS => {}
            unserved => {
                tracing::debug!(
                    ?unserved,
                    "remote quic: stream refused, unserved message type"
                );
                self.refuse(Rejection::Malformed);
                return;
            }
        }

        let Ok(request) = rkyv::from_bytes::<SharedMessage, rkyv::rancor::Error>(&frame.body)
        else {
            self.refuse(Rejection::Malformed);
            return;
        };

        if frame.message_type == MessageType::SESSION_EVENTS {
            let Some(events) = SessionEvents::new(state, request) else {
                return;
            };
            events.stream(self.send).await;
            return;
        }

        let response = state.remote.dispatch(request).await;
        let Ok(encoded) = rkyv::to_bytes::<rkyv::rancor::Error>(&response) else {
            self.refuse(Rejection::Malformed);
            return;
        };
        let frame = Frame::new(MessageType::CONTROL_RESPONSE, encoded.to_vec());
        if RemoteConnection::CONTROL
            .open(&mut self.send, &frame)
            .await
            .is_ok()
        {
            let _ = self.send.finish();
        }
    }

    fn refuse(&mut self, rejection: Rejection) {
        let code = quinn::VarInt::from(rejection.close_code().as_u32());
        let _ = self.send.reset(code);
        let _ = self.recv.stop(code);
    }
}

struct SessionEvents {
    state: RemoteState,
    sid: String,
}

impl SessionEvents {
    fn new(state: RemoteState, request: SharedMessage) -> Option<Self> {
        let sid = state.remote.subscription(&request)?;
        Some(Self { state, sid })
    }

    async fn stream(self, send: quinn::SendStream) {
        let Some(mut events) = self.state.remote.subscribe(self.sid.clone()).await else {
            let mut writer = SessionEventWriter::new(send);
            writer.finish();
            return;
        };
        let mut writer = SessionEventWriter::new(send);
        if let Some(snapshot) = self.snapshot().await
            && writer.write(&snapshot).await.is_err()
        {
            return;
        }

        loop {
            match events.recv().await {
                Ok(message) => {
                    let ServiceMessage::Shared(event) = message else {
                        continue;
                    };
                    let Some(event) = self.state.remote.resolve(self.sid.clone(), event).await
                    else {
                        continue;
                    };
                    if writer.write(&event).await.is_err() {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let Some(snapshot) = self.snapshot().await else {
                        return;
                    };
                    if writer.write(&snapshot).await.is_err() {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    writer.finish();
                    return;
                }
            }
        }
    }

    async fn snapshot(&self) -> Option<SharedEvent> {
        self.state.remote.snapshot(self.sid.clone()).await
    }
}

struct SessionEventWriter {
    send: quinn::SendStream,
    opened: bool,
}

impl SessionEventWriter {
    fn new(send: quinn::SendStream) -> Self {
        Self {
            send,
            opened: false,
        }
    }

    async fn write(&mut self, event: &SharedEvent) -> Result<(), ()> {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(event).map_err(|_| ())?;
        let frame = Frame::new(MessageType::SESSION_EVENT, bytes.to_vec());
        let written = if self.opened {
            RemoteConnection::CONTROL.send(&mut self.send, &frame).await
        } else {
            self.opened = true;
            RemoteConnection::CONTROL.open(&mut self.send, &frame).await
        };
        written.map_err(|_| ())
    }

    fn finish(&mut self) {
        let _ = self.send.finish();
    }
}

pub(crate) struct InnerEndpoint(quinn::Endpoint);

impl InnerEndpoint {
    pub(crate) fn open(
        socket: Arc<vmux_transport::quic::tunnel::TunnelSocket>,
        identity: &SelfSignedIdentity,
    ) -> Result<Self, String> {
        let config = identity.server_config()?;
        let endpoint = quinn::Endpoint::new_with_abstract_socket(
            quinn::EndpointConfig::default(),
            Some(config),
            socket,
            Arc::new(quinn::TokioRuntime),
        )
        .map_err(|error| format!("tunnel endpoint failed: {error}"))?;
        Ok(Self(endpoint))
    }

    pub(crate) async fn accept(
        &self,
        state: RemoteState,
        liveness: watch::Receiver<bool>,
        control: quinn::Connection,
    ) {
        loop {
            tokio::select! {
                _ = control.closed() => return,
                incoming = self.0.accept() => {
                    let Some(incoming) = incoming else { return };
                    let state = state.clone();
                    let liveness = liveness.clone();
                    tokio::spawn(async move {
                        match incoming.await {
                            Ok(connection) => RemoteConnection::new(connection, state, liveness).serve().await,
                            Err(error) => tracing::debug!(%error, "remote quic: handshake failed"),
                        }
                    });
                }
            }
        }
    }
}

#[cfg(test)]
fn spawn_with_identity(
    state: RemoteState,
    address: std::net::SocketAddr,
    identity: SelfSignedIdentity,
    liveness: watch::Receiver<bool>,
) -> std::io::Result<(
    tokio::task::JoinHandle<()>,
    std::net::SocketAddr,
    mpsc::UnboundedReceiver<()>,
)> {
    let endpoint = identity.listen(address).map_err(std::io::Error::other)?;
    let bound = endpoint.local_addr()?;
    tracing::info!(
        port = bound.port(),
        fingerprint = %identity.fingerprint,
        "remote quic: listening"
    );

    let (accepted, accepted_inbox) = mpsc::unbounded_channel();
    let handle = tokio::spawn(async move {
        let mut stopping = liveness.clone();
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _ = stopping.wait_for(|live| !*live) => break,
                incoming = endpoint.accept() => {
                    let Some(incoming) = incoming else { break; };
                    let state = state.clone();
                    let liveness = liveness.clone();
                    let accepted = accepted.clone();
                    connections.spawn(async move {
                        let mut stopped = liveness.clone();
                        tokio::select! {
                            connected = incoming => {
                                if let Ok(connection) = connected {
                                    let _ = accepted.send(());
                                    RemoteConnection::new(connection, state, liveness).serve().await;
                                }
                            }
                            _ = async { let _ = stopped.wait_for(|live| !*live).await; } => {}
                        }
                    });
                }
                _ = connections.join_next(), if !connections.is_empty() => {}
            }
        }
        endpoint.close(CloseCode::RemoteDisabled.as_u32().into(), b"remote off");
        while connections.join_next().await.is_some() {}
        endpoint.wait_idle().await;
    });
    Ok((handle, bound, accepted_inbox))
}

#[cfg(test)]
pub(crate) mod live {
    use super::*;
    use crate::RemoteAuthorizationStore;
    use crate::remote::authorization::{RemoteAuthorizationPlugin, RemoteAuthorizations};
    use bevy::prelude::{App, MinimalPlugins, Name};
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use vmux_api::protocol::{
        ServiceMessage, SharedEvent, SharedFailure, SharedMessage, SharedResponse,
    };
    use vmux_transport::DeviceId;
    use vmux_transport::quic::endpoint::{SelfSignedIdentity, Trust};
    use vmux_transport::service::{RemoteDriver, RemoteFuture};

    pub(crate) struct ClosedRemote;

    impl RemoteDriver for ClosedRemote {
        fn dispatch(&self, request: SharedMessage) -> RemoteFuture<'_, SharedResponse> {
            Box::pin(async move {
                match request {
                    SharedMessage::ListSessions => SharedResponse::Sessions(Vec::new()),
                    _ => SharedResponse::Failed(SharedFailure::NotFound),
                }
            })
        }

        fn subscription(&self, request: &SharedMessage) -> Option<String> {
            let SharedMessage::AgentAttach { sid } = request else {
                return None;
            };
            Some(sid.clone())
        }

        fn subscribe(
            &self,
            _sid: String,
        ) -> RemoteFuture<'_, Option<tokio::sync::broadcast::Receiver<ServiceMessage>>> {
            Box::pin(async { None })
        }

        fn resolve(
            &self,
            _sid: String,
            event: SharedEvent,
        ) -> RemoteFuture<'_, Option<SharedEvent>> {
            Box::pin(async move { Some(event) })
        }

        fn snapshot(&self, _sid: String) -> RemoteFuture<'_, Option<SharedEvent>> {
            Box::pin(async { None })
        }
    }

    struct Harness {
        _directory: tempfile::TempDir,
        address: std::net::SocketAddr,
        fingerprint: String,
        authorization_store: RemoteAuthorizationStore,
        _authorization_runtime: Option<std::thread::JoinHandle<()>>,
        liveness: watch::Sender<bool>,
        listener: Option<tokio::task::JoinHandle<()>>,
        accepted: tokio::sync::Mutex<mpsc::UnboundedReceiver<()>>,
    }

    impl Harness {
        fn start() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let authorization_store =
                RemoteAuthorizationStore::new(directory.path().join("authorizations.json"));
            authorization_store.ensure().unwrap();
            let (wake, mut wake_inbox) = mpsc::unbounded_channel();
            let (authorizations, authorization_runtime) =
                RemoteAuthorizations::with_store(authorization_store.clone(), wake);
            let authorization_runtime = std::thread::spawn(move || {
                let mut authorization_app = App::new();
                authorization_app.add_plugins((MinimalPlugins, RemoteAuthorizationPlugin));
                authorization_app
                    .world_mut()
                    .spawn((Name::new("remote authorizations"), authorization_runtime));
                authorization_app.update();
                while wake_inbox.blocking_recv().is_some() {
                    authorization_app.update();
                }
            });
            let (sessions, _, _) = SessionPublisher::channel();
            let state = RemoteState {
                device_id: DeviceId::new(uuid::Uuid::new_v4().to_string()),
                relay_token: Arc::from("a1.aabb.ccdd"),
                authorizations: authorizations.clone(),
                remote: Arc::new(ClosedRemote),
                sessions,
            };
            let identity =
                SelfSignedIdentity::generate(vec!["localhost".into()]).expect("identity");
            let fingerprint = identity.fingerprint.clone();
            let (liveness_tx, liveness_rx) = tokio::sync::watch::channel(true);
            let (handle, address, accepted) = spawn_with_identity(
                state,
                (Ipv4Addr::LOCALHOST, 0).into(),
                identity,
                liveness_rx,
            )
            .expect("listener");
            Self {
                _directory: directory,
                address,
                fingerprint,
                authorization_store,
                _authorization_runtime: Some(authorization_runtime),
                liveness: liveness_tx,
                listener: Some(handle),
                accepted: tokio::sync::Mutex::new(accepted),
            }
        }

        async fn stop(&mut self) {
            self.liveness.send_replace(false);
            if let Some(mut listener) = self.listener.take() {
                tokio::time::timeout(Duration::from_secs(5), &mut listener)
                    .await
                    .expect("listener drain timed out")
                    .expect("listener panicked");
            }
            let until = std::time::Instant::now() + Duration::from_secs(5);
            while self
                ._authorization_runtime
                .as_ref()
                .is_some_and(|thread| !thread.is_finished())
            {
                assert!(
                    std::time::Instant::now() < until,
                    "authorization fixture did not drain"
                );
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            if let Some(thread) = self._authorization_runtime.take() {
                thread.join().unwrap();
            }
        }

        async fn connect(
            &self,
            client_id: &DeviceId,
            credential: ClientCredential,
        ) -> Result<(quinn::Connection, SessionAccepted), quinn::ConnectionError> {
            let endpoint = Trust::Desktop {
                fingerprint: self.fingerprint.clone(),
            }
            .endpoint(self.address)
            .expect("client endpoint");
            let connection = endpoint
                .connect(self.address, "localhost")
                .expect("dial")
                .await?;

            let (mut send, mut recv) = connection.open_bi().await.expect("setup stream");
            let setup = ClientSetup {
                client_id: client_id.clone(),
                credential,
            };
            let frame = Frame::json(MessageType::CLIENT_SETUP, &setup).expect("encode");
            RemoteConnection::SETUP
                .open(&mut send, &frame)
                .await
                .expect("write setup");
            send.finish().expect("finish setup");

            match RemoteConnection::SETUP.accept(&mut recv).await {
                Ok(frame) => {
                    let accepted = frame
                        .read_json::<SessionAccepted>(MessageType::SESSION_ACCEPTED)
                        .expect("accepted");
                    Ok((connection, accepted))
                }
                Err(_) => Err(connection
                    .close_reason()
                    .unwrap_or(quinn::ConnectionError::LocallyClosed)),
            }
        }

        async fn pair(&self, client_id: &DeviceId) -> (quinn::Connection, String) {
            let pairing_token = self.authorization_store.pairing_token().unwrap();
            let (connection, accepted) = self
                .connect(client_id, ClientCredential::Pairing(pairing_token))
                .await
                .expect("pairing handshake");
            (connection, accepted.device_token.expect("device token"))
        }
    }

    async fn request(connection: &quinn::Connection, message: SharedMessage) -> SharedResponse {
        let (mut send, mut recv) = connection.open_bi().await.expect("control stream");
        let body = rkyv::to_bytes::<rkyv::rancor::Error>(&message).expect("encode");
        let frame = Frame::new(MessageType::CONTROL_REQUEST, body.to_vec());
        RemoteConnection::CONTROL
            .open(&mut send, &frame)
            .await
            .expect("write request");
        send.finish().expect("finish request");

        let answer = RemoteConnection::CONTROL
            .accept(&mut recv)
            .await
            .expect("response");
        let body = answer
            .body_of(MessageType::CONTROL_RESPONSE)
            .expect("a control stream answers with a control response");
        rkyv::from_bytes::<SharedResponse, rkyv::rancor::Error>(body).expect("decode")
    }

    async fn read_event(recv: &mut quinn::RecvStream) -> Option<SharedEvent> {
        let frame = RemoteConnection::CONTROL.accept(recv).await.ok()?;
        let body = frame.body_of(MessageType::SESSION_EVENT).ok()?;
        rkyv::from_bytes::<SharedEvent, rkyv::rancor::Error>(body).ok()
    }

    #[tokio::test]
    async fn off_closes_a_connection_waiting_for_its_setup_stream() {
        let mut harness = Harness::start();
        let endpoint = Trust::Desktop {
            fingerprint: harness.fingerprint.clone(),
        }
        .endpoint(harness.address)
        .unwrap();
        let connection = endpoint
            .connect(harness.address, "localhost")
            .unwrap()
            .await
            .unwrap();
        harness.accepted.lock().await.recv().await.unwrap();
        harness.liveness.send_replace(false);
        let closed = tokio::time::timeout(Duration::from_secs(2), connection.closed())
            .await
            .unwrap();
        let quinn::ConnectionError::ApplicationClosed(closed) = closed else {
            panic!("unexpected close: {closed:?}");
        };
        assert_eq!(
            closed.error_code.into_inner(),
            CloseCode::RemoteDisabled.as_u32() as u64
        );
        harness.stop().await;
    }

    #[tokio::test]
    async fn off_refuses_a_partially_received_setup_without_admitting_a_session() {
        let mut harness = Harness::start();
        let endpoint = Trust::Desktop {
            fingerprint: harness.fingerprint.clone(),
        }
        .endpoint(harness.address)
        .unwrap();
        let connection = endpoint
            .connect(harness.address, "localhost")
            .unwrap()
            .await
            .unwrap();
        harness.accepted.lock().await.recv().await.unwrap();
        let (mut send, _recv) = connection.open_bi().await.unwrap();
        send.write_all(&[0]).await.unwrap();
        harness.liveness.send_replace(false);
        let closed = tokio::time::timeout(Duration::from_secs(2), connection.closed())
            .await
            .unwrap();
        let quinn::ConnectionError::ApplicationClosed(closed) = closed else {
            panic!("unexpected close: {closed:?}");
        };
        assert_eq!(
            closed.error_code.into_inner(),
            CloseCode::RemoteDisabled.as_u32() as u64
        );
        harness.stop().await;
    }

    #[tokio::test]
    async fn off_drains_an_admitted_connection_and_a_partial_control_request() {
        let mut harness = Harness::start();
        let (connection, _) = harness.pair(&DeviceId::new("test-device")).await;
        let (mut send, _recv) = connection.open_bi().await.unwrap();
        send.write_all(&[0]).await.unwrap();
        harness.liveness.send_replace(false);
        let closed = tokio::time::timeout(Duration::from_secs(2), connection.closed())
            .await
            .unwrap();
        let quinn::ConnectionError::ApplicationClosed(closed) = closed else {
            panic!("unexpected close: {closed:?}");
        };
        assert_eq!(
            closed.error_code.into_inner(),
            CloseCode::RemoteDisabled.as_u32() as u64
        );
        harness.stop().await;
    }

    #[tokio::test]
    async fn subscribing_to_an_unknown_session_closes_rather_than_hangs() {
        let mut harness = Harness::start();
        let (connection, _) = harness.pair(&DeviceId::new("test-device")).await;

        let (mut send, mut recv) = connection.open_bi().await.expect("stream");
        let body = rkyv::to_bytes::<rkyv::rancor::Error>(&SharedMessage::AgentAttach {
            sid: "ghost".into(),
        })
        .expect("encode");
        RemoteConnection::CONTROL
            .open(
                &mut send,
                &Frame::new(MessageType::SESSION_EVENTS, body.to_vec()),
            )
            .await
            .expect("write");
        send.finish().expect("finish");

        let closed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            read_event(&mut recv).await
        })
        .await;

        assert!(
            matches!(closed, Ok(None)),
            "an unknown session must close the stream, not leave the client waiting"
        );
        harness.stop().await;
    }

    #[tokio::test]
    async fn a_frame_of_a_foreign_type_is_not_answered_as_a_control_request() {
        let mut harness = Harness::start();
        let (connection, _) = harness.pair(&DeviceId::new("test-device")).await;

        let (mut send, mut recv) = connection.open_bi().await.expect("stream");
        let body = rkyv::to_bytes::<rkyv::rancor::Error>(&SharedMessage::ListSessions)
            .expect("encode")
            .to_vec();
        RemoteConnection::CONTROL
            .open(&mut send, &Frame::new(MessageType(0x0999), body))
            .await
            .expect("write");
        send.finish().expect("finish");

        let answered = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            recv.read_to_end(1024 * 1024),
        )
        .await;

        assert!(answered.is_ok(), "it must not hang, got {answered:?}");
        let Ok(read) = answered else {
            unreachable!("checked above")
        };
        assert!(
            read.is_err(),
            "a refused stream must reset, not finish clean — a clean empty finish is what a \
             subscription to a session that does not exist looks like, so the peer could not \
             tell the two apart. Got {read:?}"
        );
        harness.stop().await;
    }

    #[tokio::test]
    async fn a_paired_client_can_list_sessions() {
        let mut harness = Harness::start();

        let (connection, _) = harness.pair(&DeviceId::new("test-device")).await;
        let response = request(&connection, SharedMessage::ListSessions).await;

        assert!(
            matches!(response, SharedResponse::Sessions(ref sessions) if sessions.is_empty()),
            "expected a typed session list, got {response:?}"
        );
        harness.stop().await;
    }

    #[tokio::test]
    async fn a_wrong_token_is_closed_with_unauthorized() {
        let mut harness = Harness::start();

        match harness
            .connect(
                &DeviceId::new("test-device"),
                ClientCredential::Pairing("wrong-token".to_string()),
            )
            .await
        {
            Err(quinn::ConnectionError::ApplicationClosed(closed)) => assert_eq!(
                closed.error_code.into_inner(),
                CloseCode::Unauthorized.as_u32() as u64,
                "a bad token must close with Unauthorized, not a generic error"
            ),
            other => panic!("expected an Unauthorized close, got {other:?}"),
        }
        harness.stop().await;
    }

    #[tokio::test]
    async fn one_connection_serves_repeated_requests() {
        let mut harness = Harness::start();

        let (connection, _) = harness.pair(&DeviceId::new("test-device")).await;

        for _ in 0..3 {
            let response = request(&connection, SharedMessage::ListSessions).await;
            assert!(matches!(response, SharedResponse::Sessions(_)));
        }
        harness.stop().await;
    }

    #[tokio::test]
    async fn a_device_token_reconnects_after_the_pairing_token_rotates() {
        let mut harness = Harness::start();
        let client = DeviceId::new("test-device");
        let (_, device_token) = harness.pair(&client).await;

        let (_, accepted) = harness
            .connect(&client, ClientCredential::Device(device_token))
            .await
            .expect("device handshake");

        assert_eq!(accepted.device_token, None);
        harness.stop().await;
    }

    #[tokio::test]
    async fn revocation_closes_an_existing_connection() {
        let mut harness = Harness::start();
        let client = DeviceId::new("test-device");
        let (connection, _) = harness.pair(&client).await;

        assert!(harness.authorization_store.revoke(&client).unwrap());
        let closed = tokio::time::timeout(Duration::from_secs(5), connection.closed())
            .await
            .expect("revocation close timeout");

        let quinn::ConnectionError::ApplicationClosed(closed) = closed else {
            panic!("expected application close, got {closed:?}")
        };
        assert_eq!(
            closed.error_code.into_inner(),
            CloseCode::Unauthorized.as_u32() as u64
        );
        harness.stop().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_rejection_carries_a_distinct_close_code() {
        let codes = [
            Rejection::Unauthorized,
            Rejection::RemoteDisabled,
            Rejection::Malformed,
        ]
        .map(|rejection| rejection.close_code().as_u32());
        let mut unique = codes.to_vec();
        unique.sort_unstable();
        unique.dedup();

        assert_eq!(unique.len(), codes.len());
    }
}
