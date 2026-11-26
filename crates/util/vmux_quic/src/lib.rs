use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;
use std::time::Duration;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
pub use bytes::Bytes;
use quinn::crypto::rustls::HandshakeData;
pub use quinn::{Connection, Endpoint, ServerConfig};
use quinn::{Incoming, RecvStream, SendStream};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use vmux_channel::{AsyncChannelPlugin, ChannelWake, MessageSender};
use vmux_ecs::{
    Shutdown, ShutdownBarrier, ShutdownComplete, ShutdownPhase, ShutdownPlugin, ShutdownSet,
    ShutdownState,
};
use vmux_task::TokioTask;
use vmux_time::{Clock, RateLimitPlugin};
pub use vmux_time::{RateLimitDecision, RateLimitPolicy, RateLimiter};
pub use vmux_transport::framing::{Frame, FrameError, FrameStream, LengthPrefixed};
pub use vmux_transport::quic::endpoint::{SelfSignedIdentity, Trust};

pub struct QuicServerPlugin;

impl Plugin for QuicServerPlugin {
    fn is_unique(&self) -> bool {
        false
    }

    fn build(&self, app: &mut App) {
        if app.world().contains_resource::<ServerInstalled>() {
            return;
        }
        app.init_resource::<ServerInstalled>()
            .add_plugins((
                ShutdownPlugin,
                RateLimitPlugin::<IpAddr>::default(),
                AsyncChannelPlugin::message::<IncomingAttempt>(),
                AsyncChannelPlugin::message::<HandshakeOutcome>(),
                AsyncChannelPlugin::message::<QuicFrameReceived>(),
                AsyncChannelPlugin::message::<PeerClosed>(),
            ))
            .add_message::<QuicPeerConnected>()
            .add_message::<QuicPeerDisconnected>()
            .configure_sets(
                Update,
                (
                    QuicServerSet::Accept,
                    QuicServerSet::Receive,
                    QuicServerSet::Reap,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    (start_servers, accept_incoming)
                        .chain()
                        .in_set(QuicServerSet::Accept),
                    (complete_handshakes, notify_disconnections)
                        .chain()
                        .in_set(QuicServerSet::Receive),
                    reap_peers.in_set(QuicServerSet::Reap),
                ),
            )
            .add_systems(PostUpdate, sync_shutdown_barriers)
            .add_systems(Shutdown, begin_shutdown.in_set(ShutdownSet::Drain))
            .add_systems(
                ShutdownComplete,
                finish_shutdown.in_set(ShutdownSet::Complete),
            );
    }
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ServerInstalled;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuicServerSet {
    Accept,
    Receive,
    Reap,
}

#[derive(Resource, Clone, Debug)]
pub struct QuicRuntime(Handle);

impl QuicRuntime {
    pub fn new(handle: Handle) -> Self {
        Self(handle)
    }

    pub fn handle(&self) -> &Handle {
        &self.0
    }
}

#[derive(Component)]
#[component(on_remove = clear_server_state)]
pub struct QuicServer {
    addr: Option<SocketAddr>,
    server_config: Option<ServerConfig>,
    endpoint: Option<Endpoint>,
    runtime: Option<Handle>,
    max_peers: usize,
    max_frame_bytes: usize,
    handshake_timeout: Duration,
    rate_limit: Option<RateLimitPolicy>,
    framed_streams: bool,
}

impl QuicServer {
    pub const DEFAULT_MAX_PEERS: usize = 256;
    pub const DEFAULT_MAX_FRAME_BYTES: usize = 1024 * 1024;
    pub const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

    pub fn bind(addr: SocketAddr, server_config: ServerConfig) -> Self {
        Self {
            addr: Some(addr),
            server_config: Some(server_config),
            endpoint: None,
            runtime: None,
            max_peers: Self::DEFAULT_MAX_PEERS,
            max_frame_bytes: Self::DEFAULT_MAX_FRAME_BYTES,
            handshake_timeout: Self::DEFAULT_HANDSHAKE_TIMEOUT,
            rate_limit: None,
            framed_streams: true,
        }
    }

    pub fn from_endpoint(endpoint: Endpoint) -> Self {
        let addr = endpoint.local_addr().ok();
        Self {
            addr,
            server_config: None,
            endpoint: Some(endpoint),
            runtime: None,
            max_peers: Self::DEFAULT_MAX_PEERS,
            max_frame_bytes: Self::DEFAULT_MAX_FRAME_BYTES,
            handshake_timeout: Self::DEFAULT_HANDSHAKE_TIMEOUT,
            rate_limit: None,
            framed_streams: true,
        }
    }

    pub fn with_runtime(mut self, runtime: Handle) -> Self {
        self.runtime = Some(runtime);
        self
    }

    pub fn with_max_peers(mut self, max_peers: usize) -> Self {
        self.max_peers = max_peers.max(1);
        self
    }

    pub fn with_max_frame_bytes(mut self, max_frame_bytes: usize) -> Self {
        self.max_frame_bytes = max_frame_bytes;
        self
    }

    pub fn with_handshake_timeout(mut self, timeout: Duration) -> Self {
        self.handshake_timeout = timeout;
        self
    }

    pub fn with_rate_limit(mut self, policy: RateLimitPolicy) -> Self {
        self.rate_limit = Some(policy);
        self
    }

    pub fn with_framed_streams(mut self, enabled: bool) -> Self {
        self.framed_streams = enabled;
        self
    }

    pub fn addr(&self) -> Option<SocketAddr> {
        self.addr
    }

    pub fn max_peers(&self) -> usize {
        self.max_peers
    }

    pub fn max_frame_bytes(&self) -> usize {
        self.max_frame_bytes
    }

    pub fn handshake_timeout(&self) -> Duration {
        self.handshake_timeout
    }

    pub fn rate_limit(&self) -> Option<RateLimitPolicy> {
        self.rate_limit
    }

    pub fn framed_streams(&self) -> bool {
        self.framed_streams
    }

    fn resolve_runtime(&self, default_runtime: Option<&QuicRuntime>) -> Result<Handle, String> {
        if let Some(handle) = &self.runtime {
            return Ok(handle.clone());
        }
        if let Some(runtime) = default_runtime {
            return Ok(runtime.handle().clone());
        }
        Handle::try_current().map_err(|_| "no tokio runtime available for QUIC server".to_string())
    }

    fn take_endpoint(&mut self, handle: &Handle) -> Result<(Endpoint, SocketAddr), String> {
        let endpoint = if let Some(endpoint) = self.endpoint.take() {
            endpoint
        } else if let (Some(addr), Some(config)) = (self.addr, self.server_config.take()) {
            let _guard = handle.enter();
            Endpoint::server(config, addr)
                .map_err(|error| format!("failed to bind {addr}: {error}"))?
        } else {
            return Err("QUIC server endpoint already consumed".to_string());
        };
        let local_addr = endpoint
            .local_addr()
            .map_err(|error| format!("failed to read endpoint local address: {error}"))?;
        Ok((endpoint, local_addr))
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QuicServerBound(SocketAddr);

impl QuicServerBound {
    pub const fn addr(&self) -> SocketAddr {
        self.0
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct QuicServerError(String);

impl QuicServerError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for QuicServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Component)]
pub struct QuicPeer {
    remote_addr: SocketAddr,
    alpn: Option<Vec<u8>>,
    connection: Connection,
    streams: Arc<Mutex<PeerStreams>>,
    runtime: Handle,
    max_frame_bytes: usize,
}

impl QuicPeer {
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    pub fn alpn(&self) -> Option<&[u8]> {
        self.alpn.as_deref()
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn send_frame(&self, payload: Bytes) -> bool {
        if self.is_closed() || u32::try_from(payload.len()).is_err() {
            return false;
        }
        let sent = match self.streams.lock() {
            Ok(mut streams) => streams.try_send_latest(&payload),
            Err(_) => return false,
        };
        if sent {
            return true;
        }
        let connection = self.connection.clone();
        let max_frame_bytes = self.max_frame_bytes;
        self.runtime.spawn(async move {
            let Ok((mut send_stream, _recv_stream)) = connection.open_bi().await else {
                return;
            };
            let codec = LengthPrefixed::new(max_frame_bytes);
            if codec.write(&mut send_stream, &payload).await.is_ok() {
                let _ = send_stream.finish();
            }
        });
        true
    }

    pub fn send_frame_on(&self, stream_id: u64, payload: Bytes) -> bool {
        if self.is_closed() || u32::try_from(payload.len()).is_err() {
            return false;
        }
        match self.streams.lock() {
            Ok(mut streams) => streams.send_on(stream_id, payload),
            Err(_) => false,
        }
    }

    pub fn finish_stream(&self, stream_id: u64) -> bool {
        match self.streams.lock() {
            Ok(mut streams) => streams.remove(stream_id),
            Err(_) => false,
        }
    }

    pub fn close(&self, code: u32, reason: &[u8]) {
        if let Ok(mut streams) = self.streams.lock() {
            streams.clear();
        }
        self.connection.close(code.into(), reason);
    }

    pub fn is_closed(&self) -> bool {
        self.connection.close_reason().is_some()
    }
}

impl Drop for QuicPeer {
    fn drop(&mut self) {
        if let Ok(mut streams) = self.streams.lock() {
            streams.clear();
        }
        if self.connection.close_reason().is_none() {
            self.connection.close(0u32.into(), b"peer removed");
        }
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuicPeerConnected {
    pub server: Entity,
    pub peer: Entity,
    pub remote_addr: SocketAddr,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct QuicFrameReceived {
    pub server: Entity,
    pub peer: Entity,
    pub stream_id: u64,
    pub payload: Bytes,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct QuicPeerDisconnected {
    pub server: Entity,
    pub peer: Entity,
    pub reason: String,
}

#[derive(Default)]
struct PeerStreams {
    outbound: HashMap<u64, mpsc::UnboundedSender<Bytes>>,
    latest: Option<u64>,
}

impl PeerStreams {
    fn insert(&mut self, stream_id: u64, sender: mpsc::UnboundedSender<Bytes>) {
        self.outbound.insert(stream_id, sender);
        self.latest = Some(stream_id);
    }

    fn remove(&mut self, stream_id: u64) -> bool {
        let removed = self.outbound.remove(&stream_id).is_some();
        if self.latest == Some(stream_id) {
            self.latest = self.outbound.keys().copied().max();
        }
        removed
    }

    fn send_on(&mut self, stream_id: u64, payload: Bytes) -> bool {
        let Some(sender) = self.outbound.get(&stream_id) else {
            return false;
        };
        if sender.send(payload).is_ok() {
            return true;
        }
        self.remove(stream_id);
        false
    }

    fn try_send_latest(&mut self, payload: &Bytes) -> bool {
        let Some(stream_id) = self.latest else {
            return false;
        };
        self.send_on(stream_id, payload.clone())
    }

    fn clear(&mut self) {
        self.outbound.clear();
        self.latest = None;
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PendingHandshake;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PeerClosing;

#[derive(Message)]
struct IncomingAttempt {
    server: Entity,
    incoming: Option<Incoming>,
}

#[derive(Message)]
struct HandshakeOutcome {
    server: Entity,
    peer: Entity,
    result: Option<Result<Connection, String>>,
}

#[derive(Message)]
struct PeerClosed {
    server: Entity,
    peer: Entity,
    reason: String,
}

#[derive(Component)]
struct ServerRuntime {
    endpoint: Endpoint,
    runtime: Handle,
    accept_task: Option<TokioTask>,
    shutdown: watch::Sender<bool>,
    active_tasks: Arc<AtomicUsize>,
    drain_waker: Arc<Mutex<Option<Waker>>>,
    max_peers: usize,
    max_frame_bytes: usize,
    handshake_timeout: Duration,
    framed_streams: bool,
}

impl ServerRuntime {
    fn active_tasks(&self) -> usize {
        self.active_tasks.load(Ordering::Acquire)
    }

    fn bind_waker(&self, waker: &Waker) {
        if let Ok(mut guard) = self.drain_waker.lock() {
            *guard = Some(waker.clone());
        }
    }

    fn stop_accepting(&mut self) {
        let _ = self.shutdown.send(true);
        if let Some(task) = self.accept_task.take() {
            task.abort();
        }
        self.endpoint.set_server_config(None);
    }

    fn close_endpoint(&mut self) {
        self.stop_accepting();
        self.endpoint.close(0u32.into(), b"server stopped");
    }

    fn spawn_handshake(
        &self,
        server: Entity,
        peer: Entity,
        incoming: Incoming,
        sender: MessageSender<HandshakeOutcome>,
    ) -> TokioTask {
        let guard = TaskGuard::new(&self.active_tasks, &self.drain_waker);
        let timeout = self.handshake_timeout;
        TokioTask::spawn(&self.runtime, async move {
            let _guard = guard;
            let result = match tokio::time::timeout(timeout, incoming).await {
                Ok(Ok(connection)) => Ok(connection),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err("QUIC handshake timed out".to_string()),
            };
            let _ = sender.send(HandshakeOutcome {
                server,
                peer,
                result: Some(result),
            });
        })
    }

    fn spawn_peer_driver(
        &self,
        server: Entity,
        peer: Entity,
        connection: Connection,
        streams: Arc<Mutex<PeerStreams>>,
        frames: MessageSender<QuicFrameReceived>,
        closed: MessageSender<PeerClosed>,
    ) -> TokioTask {
        let guard = TaskGuard::new(&self.active_tasks, &self.drain_waker);
        if self.framed_streams {
            let max_frame_bytes = self.max_frame_bytes;
            TokioTask::spawn(
                &self.runtime,
                PeerDriver::drive_framed(
                    server,
                    peer,
                    connection,
                    streams,
                    max_frame_bytes,
                    frames,
                    closed,
                    guard,
                ),
            )
        } else {
            TokioTask::spawn(
                &self.runtime,
                PeerDriver::watch_close(server, peer, connection, closed, guard),
            )
        }
    }
}

impl Drop for ServerRuntime {
    fn drop(&mut self) {
        self.close_endpoint();
    }
}

struct TaskGuard {
    active: Arc<AtomicUsize>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl TaskGuard {
    fn new(active: &Arc<AtomicUsize>, waker: &Arc<Mutex<Option<Waker>>>) -> Self {
        active.fetch_add(1, Ordering::AcqRel);
        Self {
            active: Arc::clone(active),
            waker: Arc::clone(waker),
        }
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        if self.active.fetch_sub(1, Ordering::AcqRel) == 1
            && let Ok(guard) = self.waker.lock()
            && let Some(waker) = guard.as_ref()
        {
            waker.wake_by_ref();
        }
    }
}

struct PeerDriver;

impl PeerDriver {
    async fn accept_loop(
        server: Entity,
        endpoint: Endpoint,
        sender: MessageSender<IncomingAttempt>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                accepted = endpoint.accept() => {
                    let Some(incoming) = accepted else {
                        break;
                    };
                    if sender
                        .send(IncomingAttempt {
                            server,
                            incoming: Some(incoming),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn drive_framed(
        server: Entity,
        peer: Entity,
        connection: Connection,
        streams: Arc<Mutex<PeerStreams>>,
        max_frame_bytes: usize,
        frames: MessageSender<QuicFrameReceived>,
        closed: MessageSender<PeerClosed>,
        _guard: TaskGuard,
    ) {
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = connection.closed() => break,
                accepted = connection.accept_bi() => {
                    match accepted {
                        Ok((send_stream, recv_stream)) => {
                            let stream_id = u64::from(recv_stream.id());
                            let (out_tx, out_rx) = mpsc::unbounded_channel();
                            if let Ok(mut registry) = streams.lock() {
                                registry.insert(stream_id, out_tx);
                            }
                            let write_streams = Arc::clone(&streams);
                            let write_conn = connection.clone();
                            tasks.spawn(Self::write_stream(
                                stream_id,
                                send_stream,
                                out_rx,
                                write_conn,
                                write_streams,
                                max_frame_bytes,
                            ));
                            tasks.spawn(Self::read_stream(
                                server,
                                peer,
                                stream_id,
                                recv_stream,
                                max_frame_bytes,
                                frames.clone(),
                            ));
                        }
                        Err(_) => break,
                    }
                }
                Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
            }
        }
        if let Ok(mut registry) = streams.lock() {
            registry.clear();
        }
        tasks.abort_all();
        let _ = closed.send(PeerClosed {
            server,
            peer,
            reason: Self::close_reason(&connection),
        });
    }

    async fn watch_close(
        server: Entity,
        peer: Entity,
        connection: Connection,
        closed: MessageSender<PeerClosed>,
        _guard: TaskGuard,
    ) {
        let error = connection.closed().await;
        let _ = closed.send(PeerClosed {
            server,
            peer,
            reason: error.to_string(),
        });
    }

    async fn write_stream(
        stream_id: u64,
        mut send_stream: SendStream,
        mut outbound: mpsc::UnboundedReceiver<Bytes>,
        connection: Connection,
        streams: Arc<Mutex<PeerStreams>>,
        max_frame_bytes: usize,
    ) {
        let codec = LengthPrefixed::new(max_frame_bytes);
        loop {
            tokio::select! {
                biased;
                _ = connection.closed() => break,
                frame = outbound.recv() => {
                    let Some(payload) = frame else {
                        let _ = send_stream.finish();
                        break;
                    };
                    if codec.write(&mut send_stream, &payload).await.is_err() {
                        break;
                    }
                }
            }
        }
        if let Ok(mut registry) = streams.lock() {
            registry.remove(stream_id);
        }
    }

    async fn read_stream(
        server: Entity,
        peer: Entity,
        stream_id: u64,
        mut recv_stream: RecvStream,
        max_frame_bytes: usize,
        frames: MessageSender<QuicFrameReceived>,
    ) {
        let codec = LengthPrefixed::new(max_frame_bytes);
        loop {
            match codec.read(&mut recv_stream).await {
                Ok(Some(body)) => {
                    if frames
                        .send(QuicFrameReceived {
                            server,
                            peer,
                            stream_id,
                            payload: Bytes::from(body),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    tracing::debug!(%error, stream_id, "QUIC frame read failed");
                    break;
                }
            }
        }
    }

    fn alpn_of(connection: &Connection) -> Option<Vec<u8>> {
        connection
            .handshake_data()?
            .downcast::<HandshakeData>()
            .ok()?
            .protocol
    }

    fn close_reason(connection: &Connection) -> String {
        connection
            .close_reason()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "closed".to_string())
    }
}

fn clear_server_state(mut world: DeferredWorld, context: HookContext) {
    world.commands().entity(context.entity).try_remove::<(
        ServerRuntime,
        QuicServerBound,
        QuicServerError,
        ShutdownBarrier,
        RateLimiter<IpAddr>,
    )>();
}

type PendingServers<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut QuicServer,
        Option<&'static RateLimiter<IpAddr>>,
    ),
    (Without<ServerRuntime>, Without<QuicServerError>),
>;

type PeerSlots<'w, 's> = Query<
    'w,
    's,
    &'static ChildOf,
    (
        Or<(With<QuicPeer>, With<PendingHandshake>)>,
        Without<PeerClosing>,
    ),
>;

fn start_servers(
    mut pending: PendingServers,
    default_runtime: Option<Res<QuicRuntime>>,
    incoming_tx: Res<MessageSender<IncomingAttempt>>,
    wake: Option<Res<ChannelWake>>,
    shutdown: Option<Res<ShutdownState>>,
    mut commands: Commands,
) {
    if shutdown.is_some_and(|state| state.phase() != ShutdownPhase::Running) {
        return;
    }
    for (entity, mut server, existing_limiter) in &mut pending {
        let handle = match server.resolve_runtime(default_runtime.as_deref()) {
            Ok(handle) => handle,
            Err(message) => {
                commands
                    .entity(entity)
                    .insert(QuicServerError::new(message));
                continue;
            }
        };
        let (endpoint, local_addr) = match server.take_endpoint(&handle) {
            Ok(bound) => bound,
            Err(message) => {
                commands
                    .entity(entity)
                    .insert(QuicServerError::new(message));
                continue;
            }
        };
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let drain_waker = Arc::new(Mutex::new(wake.as_ref().map(|wake| wake.0.clone())));
        let accept_task = TokioTask::spawn(
            &handle,
            PeerDriver::accept_loop(entity, endpoint.clone(), incoming_tx.clone(), shutdown_rx),
        );
        let mut target = commands.entity(entity);
        target.insert((
            QuicServerBound(local_addr),
            ServerRuntime {
                endpoint,
                runtime: handle,
                accept_task: Some(accept_task),
                shutdown: shutdown_tx,
                active_tasks: Arc::new(AtomicUsize::new(0)),
                drain_waker,
                max_peers: server.max_peers,
                max_frame_bytes: server.max_frame_bytes,
                handshake_timeout: server.handshake_timeout,
                framed_streams: server.framed_streams,
            },
        ));
        if existing_limiter.is_none()
            && let Some(policy) = server.rate_limit
        {
            target.insert(RateLimiter::<IpAddr>::new(policy));
        }
    }
}

fn accept_incoming(
    mut attempts: MessageMutator<IncomingAttempt>,
    mut servers: Query<(&ServerRuntime, Option<&mut RateLimiter<IpAddr>>)>,
    slots: PeerSlots,
    clock: Res<Clock>,
    handshake_tx: Res<MessageSender<HandshakeOutcome>>,
    shutdown: Option<Res<ShutdownState>>,
    mut commands: Commands,
) {
    let draining = shutdown.is_some_and(|state| state.phase() != ShutdownPhase::Running);
    let mut counts: HashMap<Entity, usize> = HashMap::new();
    for parent in &slots {
        *counts.entry(parent.parent()).or_insert(0) += 1;
    }
    for attempt in attempts.read() {
        let Some(incoming) = attempt.incoming.take() else {
            continue;
        };
        if draining {
            incoming.refuse();
            continue;
        }
        let Ok((runtime, limiter)) = servers.get_mut(attempt.server) else {
            incoming.refuse();
            continue;
        };
        let remote_ip = incoming.remote_address().ip();
        if let Some(mut limiter) = limiter
            && limiter.check(remote_ip, &clock).is_limited()
        {
            tracing::debug!(%remote_ip, "QUIC connection refused: rate limit exceeded");
            incoming.refuse();
            continue;
        }
        let count = counts.entry(attempt.server).or_insert(0);
        if *count >= runtime.max_peers {
            tracing::debug!(%remote_ip, "QUIC connection refused: max peers reached");
            incoming.refuse();
            continue;
        }
        let peer = commands
            .spawn((ChildOf(attempt.server), PendingHandshake))
            .id();
        let task = runtime.spawn_handshake(attempt.server, peer, incoming, handshake_tx.clone());
        commands.entity(peer).insert(task);
        *count += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn complete_handshakes(
    mut outcomes: MessageMutator<HandshakeOutcome>,
    pending: Query<(), With<PendingHandshake>>,
    servers: Query<&ServerRuntime>,
    frames_tx: Res<MessageSender<QuicFrameReceived>>,
    closed_tx: Res<MessageSender<PeerClosed>>,
    mut connected: MessageWriter<QuicPeerConnected>,
    shutdown: Option<Res<ShutdownState>>,
    mut commands: Commands,
) {
    let draining = shutdown.is_some_and(|state| state.phase() != ShutdownPhase::Running);
    for outcome in outcomes.read() {
        let Some(result) = outcome.result.take() else {
            continue;
        };
        if !pending.contains(outcome.peer) {
            if let Ok(connection) = result {
                connection.close(0u32.into(), b"cancelled");
            }
            continue;
        }
        let Ok(runtime) = servers.get(outcome.server) else {
            if let Ok(connection) = result {
                connection.close(0u32.into(), b"server closed");
            }
            commands.entity(outcome.peer).try_despawn();
            continue;
        };
        if draining {
            if let Ok(connection) = result {
                connection.close(0u32.into(), b"shutdown");
            }
            commands.entity(outcome.peer).try_despawn();
            continue;
        }
        match result {
            Ok(connection) => {
                let remote_addr = connection.remote_address();
                let alpn = PeerDriver::alpn_of(&connection);
                let streams = Arc::new(Mutex::new(PeerStreams::default()));
                let driver_task = runtime.spawn_peer_driver(
                    outcome.server,
                    outcome.peer,
                    connection.clone(),
                    Arc::clone(&streams),
                    frames_tx.clone(),
                    closed_tx.clone(),
                );
                commands
                    .entity(outcome.peer)
                    .remove::<PendingHandshake>()
                    .insert((
                        QuicPeer {
                            remote_addr,
                            alpn,
                            connection,
                            streams,
                            runtime: runtime.runtime.clone(),
                            max_frame_bytes: runtime.max_frame_bytes,
                        },
                        driver_task,
                    ));
                connected.write(QuicPeerConnected {
                    server: outcome.server,
                    peer: outcome.peer,
                    remote_addr,
                });
            }
            Err(error) => {
                tracing::debug!(%error, "QUIC handshake failed");
                commands.entity(outcome.peer).try_despawn();
            }
        }
    }
}

fn notify_disconnections(
    mut closed: MessageReader<PeerClosed>,
    peers: Query<(), (With<QuicPeer>, Without<PeerClosing>)>,
    mut disconnected: MessageWriter<QuicPeerDisconnected>,
    mut commands: Commands,
) {
    for event in closed.read() {
        if !peers.contains(event.peer) {
            continue;
        }
        disconnected.write(QuicPeerDisconnected {
            server: event.server,
            peer: event.peer,
            reason: event.reason.clone(),
        });
        commands.entity(event.peer).insert(PeerClosing);
    }
}

fn reap_peers(closing: Query<Entity, With<PeerClosing>>, mut commands: Commands) {
    for entity in &closing {
        commands.entity(entity).try_despawn();
    }
}

fn begin_shutdown(
    mut servers: Query<(Entity, &mut ServerRuntime)>,
    handshakes: Query<(Entity, &ChildOf), With<PendingHandshake>>,
    peers: Query<&ChildOf, With<QuicPeer>>,
    wake: Option<Res<ChannelWake>>,
    mut commands: Commands,
) {
    for (entity, _) in &handshakes {
        commands.entity(entity).try_despawn();
    }
    for (entity, mut runtime) in &mut servers {
        if let Some(wake) = &wake {
            runtime.bind_waker(&wake.0);
        }
        runtime.stop_accepting();
        commands.entity(entity).remove::<QuicServerBound>();
        let has_peers = peers.iter().any(|parent| parent.parent() == entity);
        if has_peers || runtime.active_tasks() > 0 {
            commands.entity(entity).insert(ShutdownBarrier);
        }
    }
}

fn sync_shutdown_barriers(
    servers: Query<(Entity, &ServerRuntime), With<ShutdownBarrier>>,
    slots: PeerSlots,
    mut commands: Commands,
) {
    for (entity, runtime) in &servers {
        let has_children = slots.iter().any(|parent| parent.parent() == entity);
        if !has_children && runtime.active_tasks() == 0 {
            commands.entity(entity).remove::<ShutdownBarrier>();
        }
    }
}

fn finish_shutdown(
    handshakes: Query<Entity, With<PendingHandshake>>,
    peers: Query<Entity, With<QuicPeer>>,
    mut servers: Query<(Entity, &mut ServerRuntime)>,
    mut commands: Commands,
) {
    for entity in &handshakes {
        commands.entity(entity).try_despawn();
    }
    for entity in &peers {
        commands.entity(entity).try_despawn();
    }
    for (entity, mut runtime) in &mut servers {
        runtime.close_endpoint();
        commands
            .entity(entity)
            .try_remove::<(ServerRuntime, QuicServerBound, ShutdownBarrier)>();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use bevy_app::AppExit;
    use tokio::sync::oneshot;
    use vmux_ecs::{ShutdownRequest, ShutdownTimeout};
    use vmux_runner::HeadlessTokioRunner;
    use vmux_transport::quic::{ALPN, MessageType};

    use super::*;

    fn test_identity() -> SelfSignedIdentity {
        SelfSignedIdentity::generate(vec!["localhost".into(), "127.0.0.1".into()]).unwrap()
    }

    fn test_client(identity: &SelfSignedIdentity, addr: SocketAddr) -> Endpoint {
        Trust::Desktop {
            fingerprint: identity.fingerprint.clone(),
        }
        .endpoint(addr)
        .unwrap()
    }

    #[derive(Resource)]
    struct BoundChannel(Option<oneshot::Sender<SocketAddr>>);

    fn publish_bound(
        bound: Query<&QuicServerBound, Added<QuicServerBound>>,
        mut channel: ResMut<BoundChannel>,
    ) {
        for bound in &bound {
            if let Some(tx) = channel.0.take() {
                let _ = tx.send(bound.addr());
            }
        }
    }

    #[derive(Resource, Default)]
    struct PeerEvents {
        connected: Vec<(Entity, SocketAddr)>,
        disconnected: Vec<Entity>,
        alpns: Vec<Option<Vec<u8>>>,
        shutdown_on_disconnect: bool,
    }

    fn echo_frames(
        mut connected: MessageReader<QuicPeerConnected>,
        mut frames: MessageReader<QuicFrameReceived>,
        mut disconnected: MessageReader<QuicPeerDisconnected>,
        peers: Query<&QuicPeer>,
        mut events: ResMut<PeerEvents>,
        mut shutdown: MessageWriter<ShutdownRequest>,
    ) {
        for event in connected.read() {
            events.connected.push((event.peer, event.remote_addr));
            if let Ok(peer) = peers.get(event.peer) {
                events.alpns.push(peer.alpn().map(|alpn| alpn.to_vec()));
            }
        }
        for frame in frames.read() {
            let Ok(peer) = peers.get(frame.peer) else {
                continue;
            };
            if frame.payload.as_ref() == b"stop" {
                assert!(peer.send_frame_on(frame.stream_id, Bytes::from_static(b"bye")));
                shutdown.write(ShutdownRequest::success());
            } else {
                let mut reply = b"echo:".to_vec();
                reply.extend_from_slice(&frame.payload);
                assert!(peer.send_frame_on(frame.stream_id, Bytes::from(reply)));
            }
        }
        for event in disconnected.read() {
            events.disconnected.push(event.peer);
            if events.shutdown_on_disconnect {
                shutdown.write(ShutdownRequest::success());
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn framed_roundtrip_and_peer_lifecycle_clean_up_on_disconnect() {
        let identity = test_identity();
        let server_config = identity.server_config().unwrap();
        let (addr_tx, addr_rx) = oneshot::channel();
        let mut runner = HeadlessTokioRunner::new(Handle::current())
            .with_housekeeping_floor(Duration::from_secs(30));

        let mut app = App::new();
        app.add_plugins((QuicServerPlugin, QuicServerPlugin))
            .insert_resource(QuicRuntime::new(Handle::current()))
            .insert_resource(BoundChannel(Some(addr_tx)))
            .insert_resource(PeerEvents {
                shutdown_on_disconnect: true,
                ..Default::default()
            })
            .add_systems(Update, publish_bound.in_set(QuicServerSet::Receive))
            .add_systems(
                Update,
                echo_frames
                    .after(QuicServerSet::Receive)
                    .before(QuicServerSet::Reap),
            );
        app.world_mut().spawn(QuicServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            server_config,
        ));

        let client_identity = identity.clone();
        let client_task = tokio::spawn(async move {
            let addr = addr_rx.await.unwrap();
            let client = test_client(&client_identity, addr);
            let conn = client.connect(addr, "localhost").unwrap().await.unwrap();
            let codec = LengthPrefixed::new(1024 * 1024);

            let (mut send, mut recv) = conn.open_bi().await.unwrap();
            codec.write(&mut send, b"hello").await.unwrap();
            let first = codec.read(&mut recv).await.unwrap().unwrap();
            assert_eq!(first, b"echo:hello");

            codec.write(&mut send, b"world").await.unwrap();
            let second = codec.read(&mut recv).await.unwrap().unwrap();
            assert_eq!(second, b"echo:world");

            conn.close(0u32.into(), b"done");
            client.wait_idle().await;
        });

        let started = Instant::now();
        assert_eq!(runner.run_async(&mut app).await, AppExit::Success);
        assert!(started.elapsed() < Duration::from_secs(5));
        client_task.await.unwrap();

        let events = app.world().resource::<PeerEvents>();
        assert_eq!(events.connected.len(), 1);
        assert_eq!(events.disconnected.len(), 1);
        assert_eq!(events.connected[0].0, events.disconnected[0]);
        assert_eq!(events.alpns, vec![Some(ALPN.to_vec())]);
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<QuicPeer>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn max_peers_refuses_excess_and_recovers_after_disconnect() {
        let identity = test_identity();
        let server_spec = QuicServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            identity.server_config().unwrap(),
        )
        .with_max_peers(0)
        .with_handshake_timeout(Duration::from_secs(5))
        .with_max_frame_bytes(64 * 1024);
        assert_eq!(server_spec.max_peers(), 1);
        assert_eq!(server_spec.handshake_timeout(), Duration::from_secs(5));
        assert_eq!(server_spec.max_frame_bytes(), 64 * 1024);

        let (addr_tx, addr_rx) = oneshot::channel();
        let (handle, mut runner) = HeadlessTokioRunner::handles(Handle::current());
        runner = runner.with_housekeeping_floor(Duration::from_secs(30));

        let mut app = App::new();
        app.add_plugins(QuicServerPlugin)
            .insert_resource(QuicRuntime::new(Handle::current()))
            .insert_resource(BoundChannel(Some(addr_tx)))
            .init_resource::<PeerEvents>()
            .add_systems(Update, publish_bound.in_set(QuicServerSet::Receive))
            .add_systems(
                Update,
                echo_frames
                    .after(QuicServerSet::Receive)
                    .before(QuicServerSet::Reap),
            );
        app.world_mut().spawn(server_spec);

        let client_identity = identity.clone();
        let driver_handle = handle.clone();
        let client_task = tokio::spawn(async move {
            let addr = addr_rx.await.unwrap();
            let client_a = test_client(&client_identity, addr);
            let conn_a = client_a.connect(addr, "localhost").unwrap().await.unwrap();
            let codec = LengthPrefixed::new(64 * 1024);
            let (mut send_a, mut recv_a) = conn_a.open_bi().await.unwrap();
            codec.write(&mut send_a, b"first").await.unwrap();
            assert_eq!(
                codec.read(&mut recv_a).await.unwrap().unwrap(),
                b"echo:first"
            );

            let client_b = test_client(&client_identity, addr);
            let refused = client_b.connect(addr, "localhost").unwrap().await;
            assert!(
                refused.is_err(),
                "second peer must be refused at max_peers=1"
            );

            conn_a.close(0u32.into(), b"bye");
            client_a.wait_idle().await;

            let deadline = Instant::now() + Duration::from_secs(3);
            let conn_c = loop {
                let client_c = test_client(&client_identity, addr);
                if let Ok(conn) = client_c.connect(addr, "localhost").unwrap().await {
                    break (client_c, conn);
                }
                assert!(Instant::now() < deadline, "peer slot never recovered");
                tokio::time::sleep(Duration::from_millis(15)).await;
            };
            let (mut send_c, mut recv_c) = conn_c.1.open_bi().await.unwrap();
            codec.write(&mut send_c, b"recovered").await.unwrap();
            assert_eq!(
                codec.read(&mut recv_c).await.unwrap().unwrap(),
                b"echo:recovered"
            );
            conn_c.1.close(0u32.into(), b"done");
            conn_c.0.wait_idle().await;
            driver_handle.shutdown();
        });

        assert_eq!(runner.run_async(&mut app).await, AppExit::Success);
        client_task.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rate_limit_sheds_excess_connections_from_same_ip_deterministically() {
        let identity = test_identity();
        let clock = Clock::fake(1_000);
        let policy = RateLimitPolicy::per_second(2, 1);
        let server_spec = QuicServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            identity.server_config().unwrap(),
        )
        .with_rate_limit(policy);
        assert_eq!(server_spec.rate_limit(), Some(policy));

        let (addr_tx, addr_rx) = oneshot::channel();
        let (handle, mut runner) = HeadlessTokioRunner::handles(Handle::current());
        runner = runner.with_housekeeping_floor(Duration::from_secs(30));

        let mut app = App::new();
        app.insert_resource(clock.clone())
            .add_plugins(QuicServerPlugin)
            .insert_resource(QuicRuntime::new(Handle::current()))
            .insert_resource(BoundChannel(Some(addr_tx)))
            .add_systems(Update, publish_bound.in_set(QuicServerSet::Receive));
        app.world_mut().spawn(server_spec);

        let client_identity = identity.clone();
        let test_clock = clock.clone();
        let driver_handle = handle.clone();
        let client_task = tokio::spawn(async move {
            let addr = addr_rx.await.unwrap();

            let c1 = test_client(&client_identity, addr);
            let conn1 = c1.connect(addr, "localhost").unwrap().await.unwrap();
            let c2 = test_client(&client_identity, addr);
            let conn2 = c2.connect(addr, "localhost").unwrap().await.unwrap();

            let c3 = test_client(&client_identity, addr);
            let limited = c3.connect(addr, "localhost").unwrap().await;
            assert!(
                limited.is_err(),
                "third burst connection must be rate-limited"
            );

            test_clock.advance(Duration::from_secs(1));

            let c4 = test_client(&client_identity, addr);
            let conn4 = c4.connect(addr, "localhost").unwrap().await.unwrap();

            conn1.close(0u32.into(), b"done");
            conn2.close(0u32.into(), b"done");
            conn4.close(0u32.into(), b"done");
            c1.wait_idle().await;
            c2.wait_idle().await;
            c4.wait_idle().await;
            driver_handle.shutdown();
        });

        assert_eq!(runner.run_async(&mut app).await, AppExit::Success);
        client_task.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn graceful_shutdown_drains_active_peer_and_honors_timeout() {
        let identity = test_identity();
        let (addr_tx, addr_rx) = oneshot::channel();
        let mut runner = HeadlessTokioRunner::new(Handle::current())
            .with_housekeeping_floor(Duration::from_secs(30));

        let mut app = App::new();
        app.add_plugins((ShutdownPlugin, QuicServerPlugin))
            .insert_resource(QuicRuntime::new(Handle::current()))
            .insert_resource(BoundChannel(Some(addr_tx)))
            .init_resource::<PeerEvents>()
            .add_systems(Update, publish_bound.in_set(QuicServerSet::Receive))
            .add_systems(
                Update,
                echo_frames
                    .after(QuicServerSet::Receive)
                    .before(QuicServerSet::Reap),
            );
        app.world_mut().spawn(QuicServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            identity.server_config().unwrap(),
        ));

        let client_identity = identity.clone();
        let client_task = tokio::spawn(async move {
            let addr = addr_rx.await.unwrap();
            let client = test_client(&client_identity, addr);
            let conn = client.connect(addr, "localhost").unwrap().await.unwrap();
            let codec = LengthPrefixed::new(1024 * 1024);
            let (mut send, mut recv) = conn.open_bi().await.unwrap();
            codec.write(&mut send, b"stop").await.unwrap();
            let reply = codec.read(&mut recv).await.unwrap().unwrap();
            assert_eq!(reply, b"bye");
            conn.close(0u32.into(), b"done");
            client.wait_idle().await;
            addr
        });

        let started = Instant::now();
        assert_eq!(runner.run_async(&mut app).await, AppExit::Success);
        assert!(started.elapsed() < Duration::from_secs(5));
        let addr = client_task.await.unwrap();

        let probe = test_client(&identity, addr);
        let rejected = tokio::time::timeout(
            Duration::from_millis(250),
            probe.connect(addr, "localhost").unwrap(),
        )
        .await;
        assert!(
            rejected.is_err() || rejected.unwrap().is_err(),
            "endpoint must reject connections after shutdown"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_timeout_force_closes_stalled_peer() {
        let identity = test_identity();
        let (addr_tx, addr_rx) = oneshot::channel();
        let (handle, mut runner) = HeadlessTokioRunner::handles(Handle::current());
        runner = runner.with_housekeeping_floor(Duration::from_secs(30));

        let mut app = App::new();
        app.insert_resource(ShutdownTimeout::new(Duration::from_millis(80)))
            .add_plugins((ShutdownPlugin, QuicServerPlugin))
            .insert_resource(QuicRuntime::new(Handle::current()))
            .insert_resource(BoundChannel(Some(addr_tx)))
            .add_systems(Update, publish_bound.in_set(QuicServerSet::Receive));
        app.world_mut().spawn(QuicServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            identity.server_config().unwrap(),
        ));

        let client_identity = identity.clone();
        let driver_handle = handle.clone();
        let client_task = tokio::spawn(async move {
            let addr = addr_rx.await.unwrap();
            let client = test_client(&client_identity, addr);
            let conn = client.connect(addr, "localhost").unwrap().await.unwrap();
            driver_handle.shutdown();
            let closed = tokio::time::timeout(Duration::from_secs(2), conn.closed()).await;
            assert!(
                closed.is_ok(),
                "stalled peer should be closed on shutdown timeout"
            );
        });

        let started = Instant::now();
        assert_eq!(runner.run_async(&mut app).await, AppExit::Success);
        assert!(started.elapsed() < Duration::from_secs(2));
        client_task.await.unwrap();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<QuicPeer>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[derive(Component)]
    struct RawEchoStarted;

    fn serve_raw_connections(
        peers: Query<(Entity, &QuicPeer), Without<RawEchoStarted>>,
        runtime: Res<QuicRuntime>,
        mut commands: Commands,
    ) {
        for (entity, peer) in &peers {
            let connection = peer.connection().clone();
            let task = TokioTask::spawn(runtime.handle(), async move {
                let codec = FrameStream::new(64 * 1024);
                let Ok((mut send, mut recv)) = connection.accept_bi().await else {
                    return;
                };
                if let Ok(frame) = codec.accept(&mut recv).await {
                    let response = Frame::new(MessageType::CONTROL_RESPONSE, frame.body);
                    let _ = codec.open(&mut send, &response).await;
                    let _ = send.finish();
                }
            });
            commands.entity(entity).insert(RawEchoStarted);
            commands.spawn((ChildOf(entity), task));
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn raw_connection_mode_exposes_quinn_connection_for_custom_protocols() {
        let identity = test_identity();
        let (addr_tx, addr_rx) = oneshot::channel();
        let (handle, mut runner) = HeadlessTokioRunner::handles(Handle::current());
        runner = runner.with_housekeeping_floor(Duration::from_secs(30));

        let mut app = App::new();
        app.add_plugins(QuicServerPlugin)
            .insert_resource(QuicRuntime::new(Handle::current()))
            .insert_resource(BoundChannel(Some(addr_tx)))
            .add_systems(Update, publish_bound.in_set(QuicServerSet::Receive))
            .add_systems(
                Update,
                serve_raw_connections
                    .after(QuicServerSet::Receive)
                    .before(QuicServerSet::Reap),
            );
        let server_spec = QuicServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            identity.server_config().unwrap(),
        )
        .with_framed_streams(false);
        assert!(!server_spec.framed_streams());
        app.world_mut().spawn(server_spec);

        let client_identity = identity.clone();
        let driver_handle = handle.clone();
        let client_task = tokio::spawn(async move {
            let addr = addr_rx.await.unwrap();
            let client = test_client(&client_identity, addr);
            let conn = client.connect(addr, "localhost").unwrap().await.unwrap();
            let codec = FrameStream::new(64 * 1024);
            let (mut send, mut recv) = conn.open_bi().await.unwrap();
            codec
                .open(
                    &mut send,
                    &Frame::new(MessageType::CONTROL_REQUEST, b"raw-ping".to_vec()),
                )
                .await
                .unwrap();
            send.finish().unwrap();
            let reply = codec.accept(&mut recv).await.unwrap();
            assert_eq!(reply.message_type, MessageType::CONTROL_RESPONSE);
            assert_eq!(reply.body, b"raw-ping");
            conn.close(0u32.into(), b"done");
            client.wait_idle().await;
            driver_handle.shutdown();
        });

        assert_eq!(runner.run_async(&mut app).await, AppExit::Success);
        client_task.await.unwrap();
    }

    #[test]
    fn missing_runtime_records_server_error() {
        let identity = test_identity();
        let mut app = App::new();
        app.add_plugins(QuicServerPlugin);
        let server = app
            .world_mut()
            .spawn(QuicServer::bind(
                "127.0.0.1:0".parse().unwrap(),
                identity.server_config().unwrap(),
            ))
            .id();
        app.update();
        assert!(app.world().get::<QuicServerBound>(server).is_none());
        let error = app.world().get::<QuicServerError>(server).unwrap();
        assert!(error.message().contains("tokio runtime"));
    }
}
