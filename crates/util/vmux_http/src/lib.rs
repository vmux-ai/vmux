use std::convert::Infallible;
use std::fmt;
use std::net::SocketAddr;
use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bytes::Bytes;
pub use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::runtime::Handle;
use tokio::sync::{oneshot, watch};
use vmux_channel::{AsyncChannelPlugin, ChannelWake, SpawnReceiver, SpawnSender};
use vmux_ecs::{
    Shutdown, ShutdownBarrier, ShutdownComplete, ShutdownPhase, ShutdownPlugin, ShutdownSet,
    ShutdownState,
};
use vmux_task::TokioTask;

type IncomingRequest = (ChildOf, HttpRequest, HttpResponder);

pub struct HttpServerPlugin;

impl Plugin for HttpServerPlugin {
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
                AsyncChannelPlugin::spawn::<IncomingRequest>().components(),
            ))
            .configure_sets(
                Update,
                (
                    HttpServerSet::Accept,
                    HttpServerSet::Serve,
                    HttpServerSet::Reap,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    start_servers.in_set(HttpServerSet::Accept),
                    reap_responded.in_set(HttpServerSet::Reap),
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
pub enum HttpServerSet {
    Accept,
    Serve,
    Reap,
}

#[derive(Resource, Clone, Debug)]
pub struct HttpRuntime(Handle);

impl HttpRuntime {
    pub fn new(handle: Handle) -> Self {
        Self(handle)
    }

    pub fn handle(&self) -> &Handle {
        &self.0
    }
}

#[derive(Component)]
#[component(on_remove = clear_server_state)]
pub struct HttpServer {
    addr: Option<SocketAddr>,
    listener: Option<std::net::TcpListener>,
    runtime: Option<Handle>,
    max_body_bytes: usize,
}

impl HttpServer {
    pub const DEFAULT_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

    pub fn bind(addr: SocketAddr) -> Self {
        Self {
            addr: Some(addr),
            listener: None,
            runtime: None,
            max_body_bytes: Self::DEFAULT_MAX_BODY_BYTES,
        }
    }

    pub fn from_listener(listener: std::net::TcpListener) -> Self {
        let addr = listener.local_addr().ok();
        Self {
            addr,
            listener: Some(listener),
            runtime: None,
            max_body_bytes: Self::DEFAULT_MAX_BODY_BYTES,
        }
    }

    pub fn with_runtime(mut self, runtime: Handle) -> Self {
        self.runtime = Some(runtime);
        self
    }

    pub fn with_max_body_bytes(mut self, max_body_bytes: usize) -> Self {
        self.max_body_bytes = max_body_bytes;
        self
    }

    pub fn addr(&self) -> Option<SocketAddr> {
        self.addr
    }

    fn resolve_runtime(&self, default_runtime: Option<&HttpRuntime>) -> Result<Handle, String> {
        if let Some(handle) = &self.runtime {
            return Ok(handle.clone());
        }
        if let Some(runtime) = default_runtime {
            return Ok(runtime.handle().clone());
        }
        Handle::try_current().map_err(|_| "no tokio runtime available for HTTP server".to_string())
    }

    fn take_listener(&mut self, handle: &Handle) -> Result<(TokioTcpListener, SocketAddr), String> {
        let std_listener = if let Some(listener) = self.listener.take() {
            listener
        } else if let Some(addr) = self.addr {
            std::net::TcpListener::bind(addr)
                .map_err(|error| format!("failed to bind {addr}: {error}"))?
        } else {
            return Err("HTTP server listener already consumed".to_string());
        };
        std_listener
            .set_nonblocking(true)
            .map_err(|error| format!("failed to set nonblocking listener: {error}"))?;
        let local_addr = std_listener
            .local_addr()
            .map_err(|error| format!("failed to read listener local address: {error}"))?;
        let _guard = handle.enter();
        let listener = TokioTcpListener::from_std(std_listener)
            .map_err(|error| format!("failed to register tokio TCP listener: {error}"))?;
        Ok((listener, local_addr))
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HttpServerBound(SocketAddr);

impl HttpServerBound {
    pub const fn addr(&self) -> SocketAddr {
        self.0
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct HttpServerError(String);

impl HttpServerError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HttpServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Component, Clone, Debug)]
pub struct HttpRequest {
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Vec<u8>,
    remote_addr: Option<SocketAddr>,
}

impl HttpRequest {
    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    pub fn path(&self) -> &str {
        self.uri.path()
    }

    pub fn query(&self) -> Option<&str> {
        self.uri.query()
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn body_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.body)
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    pub fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote_addr
    }
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl HttpResponse {
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: Bytes::new(),
        }
    }

    pub fn ok(body: impl Into<Bytes>) -> Self {
        Self::new(StatusCode::OK).with_body(body)
    }

    pub fn text(status: StatusCode, text: impl Into<String>) -> Self {
        Self::new(status)
            .with_header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )
            .with_body(Bytes::from(text.into()))
    }

    pub fn json<T: Serialize>(status: StatusCode, value: &T) -> Result<Self, serde_json::Error> {
        let body = serde_json::to_vec(value)?;
        Ok(Self::new(status)
            .with_header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )
            .with_body(body))
    }

    pub fn not_found() -> Self {
        Self::text(StatusCode::NOT_FOUND, "not found")
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::text(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn with_header<K, V>(mut self, name: K, value: V) -> Self
    where
        K: TryInto<HeaderName>,
        V: TryInto<HeaderValue>,
    {
        match (name.try_into(), value.try_into()) {
            (Ok(name), Ok(value)) => {
                self.headers.insert(name, value);
            }
            _ => {
                tracing::warn!("ignoring invalid HTTP response header");
            }
        }
        self
    }

    pub fn with_body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = body.into();
        self
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    fn into_hyper(self) -> hyper::Response<Full<Bytes>> {
        let mut response = hyper::Response::new(Full::new(self.body));
        *response.status_mut() = self.status;
        *response.headers_mut() = self.headers;
        response
    }
}

#[derive(Component)]
pub struct HttpResponder {
    tx: Option<oneshot::Sender<HttpResponse>>,
}

impl HttpResponder {
    fn new(tx: oneshot::Sender<HttpResponse>) -> Self {
        Self { tx: Some(tx) }
    }

    pub fn respond(&mut self, response: HttpResponse) -> bool {
        let Some(tx) = self.tx.take() else {
            return false;
        };
        tx.send(response).is_ok()
    }

    pub fn is_responded(&self) -> bool {
        self.tx.is_none()
    }
}

impl Drop for HttpResponder {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(HttpResponse::internal_error("unhandled request"));
        }
    }
}

#[derive(Component)]
struct ServerRuntime {
    accept_task: Option<TokioTask>,
    shutdown: watch::Sender<bool>,
    active_connections: Arc<AtomicUsize>,
    drain_waker: Arc<Mutex<Option<Waker>>>,
}

impl ServerRuntime {
    fn active_connections(&self) -> usize {
        self.active_connections.load(Ordering::Acquire)
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
    }
}

impl Drop for ServerRuntime {
    fn drop(&mut self) {
        self.stop_accepting();
    }
}

struct ConnectionGuard {
    active: Arc<AtomicUsize>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl ConnectionGuard {
    fn new(active: Arc<AtomicUsize>, waker: Arc<Mutex<Option<Waker>>>) -> Self {
        active.fetch_add(1, Ordering::AcqRel);
        Self { active, waker }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if self.active.fetch_sub(1, Ordering::AcqRel) == 1
            && let Ok(guard) = self.waker.lock()
            && let Some(waker) = guard.as_ref()
        {
            waker.wake_by_ref();
        }
    }
}

#[derive(Clone)]
struct AcceptConfig {
    server_entity: Entity,
    sender: SpawnSender<IncomingRequest>,
    max_body_bytes: usize,
    shutdown: watch::Receiver<bool>,
    active_connections: Arc<AtomicUsize>,
    drain_waker: Arc<Mutex<Option<Waker>>>,
}

impl AcceptConfig {
    async fn run_accept_loop(mut self, listener: TokioTcpListener, handle: Handle) {
        loop {
            tokio::select! {
                biased;
                changed = self.shutdown.changed() => {
                    if changed.is_err() || *self.shutdown.borrow() {
                        break;
                    }
                }
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, peer_addr)) => {
                            let config = self.clone();
                            let guard = ConnectionGuard::new(
                                Arc::clone(&self.active_connections),
                                Arc::clone(&self.drain_waker),
                            );
                            handle.spawn(async move {
                                let _guard = guard;
                                config.serve_connection(stream, peer_addr).await;
                            });
                        }
                        Err(error) => {
                            tracing::warn!(%error, "HTTP accept failed");
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        }
                    }
                }
            }
        }
    }

    async fn serve_connection(mut self, stream: tokio::net::TcpStream, peer_addr: SocketAddr) {
        let io = TokioIo::new(stream);
        let server_entity = self.server_entity;
        let sender = self.sender.clone();
        let max_body_bytes = self.max_body_bytes;
        let service = service_fn(move |request: hyper::Request<Incoming>| {
            let sender = sender.clone();
            async move {
                Ok::<_, Infallible>(
                    Self::dispatch_request(
                        server_entity,
                        sender,
                        max_body_bytes,
                        peer_addr,
                        request,
                    )
                    .await,
                )
            }
        });
        let mut conn = pin!(http1::Builder::new().serve_connection(io, service));
        tokio::select! {
            result = &mut conn => {
                if let Err(error) = result {
                    tracing::debug!(%error, %peer_addr, "HTTP connection ended with error");
                }
            }
            draining = Self::wait_draining(&mut self.shutdown) => {
                if draining {
                    conn.as_mut().graceful_shutdown();
                    if let Err(error) = conn.await {
                        tracing::debug!(%error, %peer_addr, "HTTP connection ended during shutdown");
                    }
                }
            }
        }
    }

    async fn wait_draining(shutdown: &mut watch::Receiver<bool>) -> bool {
        shutdown.wait_for(|draining| *draining).await.is_ok()
    }

    async fn dispatch_request(
        server_entity: Entity,
        sender: SpawnSender<IncomingRequest>,
        max_body_bytes: usize,
        peer_addr: SocketAddr,
        request: hyper::Request<Incoming>,
    ) -> hyper::Response<Full<Bytes>> {
        let (parts, body) = request.into_parts();
        let collected = match Limited::new(body, max_body_bytes).collect().await {
            Ok(collected) => collected.to_bytes().to_vec(),
            Err(error) => {
                if error.downcast_ref::<LengthLimitError>().is_some() {
                    return HttpResponse::text(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "request body too large",
                    )
                    .into_hyper();
                }
                return HttpResponse::text(StatusCode::BAD_REQUEST, "failed to read request body")
                    .into_hyper();
            }
        };
        let http_request = HttpRequest {
            method: parts.method,
            uri: parts.uri,
            headers: parts.headers,
            body: collected,
            remote_addr: Some(peer_addr),
        };
        let (tx, rx) = oneshot::channel();
        if sender
            .send((ChildOf(server_entity), http_request, HttpResponder::new(tx)))
            .is_err()
        {
            return HttpResponse::text(StatusCode::SERVICE_UNAVAILABLE, "server shutting down")
                .into_hyper();
        }
        match rx.await {
            Ok(response) => response.into_hyper(),
            Err(_) => HttpResponse::internal_error("request dropped without response").into_hyper(),
        }
    }
}

fn clear_server_state(mut world: DeferredWorld, context: HookContext) {
    world.commands().entity(context.entity).try_remove::<(
        ServerRuntime,
        SpawnReceiver<IncomingRequest>,
        HttpServerBound,
        HttpServerError,
        ShutdownBarrier,
    )>();
}

type PendingServers<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut HttpServer),
    (Without<ServerRuntime>, Without<HttpServerError>),
>;

fn start_servers(
    mut pending: PendingServers,
    default_runtime: Option<Res<HttpRuntime>>,
    wake: Option<Res<ChannelWake>>,
    shutdown: Option<Res<ShutdownState>>,
    mut commands: Commands,
) {
    if shutdown.is_some_and(|state| state.phase() != ShutdownPhase::Running) {
        return;
    }
    for (entity, mut server) in &mut pending {
        let handle = match server.resolve_runtime(default_runtime.as_deref()) {
            Ok(handle) => handle,
            Err(message) => {
                commands
                    .entity(entity)
                    .insert(HttpServerError::new(message));
                continue;
            }
        };
        let (listener, local_addr) = match server.take_listener(&handle) {
            Ok(bound) => bound,
            Err(message) => {
                commands
                    .entity(entity)
                    .insert(HttpServerError::new(message));
                continue;
            }
        };
        let (sender, receiver) = SpawnReceiver::<IncomingRequest>::unbounded();
        let drain_waker = Arc::new(Mutex::new(wake.as_ref().map(|wake| wake.0.clone())));
        if let Some(wake) = &wake {
            sender.bind_waker(wake.0.clone());
        }
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let active_connections = Arc::new(AtomicUsize::new(0));
        let config = AcceptConfig {
            server_entity: entity,
            sender,
            max_body_bytes: server.max_body_bytes,
            shutdown: shutdown_rx,
            active_connections: Arc::clone(&active_connections),
            drain_waker: Arc::clone(&drain_waker),
        };
        let accept_handle = handle.clone();
        let accept_task =
            TokioTask::spawn(&handle, config.run_accept_loop(listener, accept_handle));
        commands.entity(entity).insert((
            HttpServerBound(local_addr),
            receiver,
            ServerRuntime {
                accept_task: Some(accept_task),
                shutdown: shutdown_tx,
                active_connections,
                drain_waker,
            },
        ));
    }
}

fn reap_responded(requests: Query<(Entity, &HttpResponder)>, mut commands: Commands) {
    for (entity, responder) in &requests {
        if responder.is_responded() {
            commands.entity(entity).try_despawn();
        }
    }
}

fn begin_shutdown(
    mut servers: Query<(
        Entity,
        &mut ServerRuntime,
        Option<&SpawnReceiver<IncomingRequest>>,
    )>,
    requests: Query<(&ChildOf, &HttpResponder), With<HttpRequest>>,
    wake: Option<Res<ChannelWake>>,
    mut commands: Commands,
) {
    for (entity, mut runtime, receiver) in &mut servers {
        if let Some(wake) = &wake {
            runtime.bind_waker(&wake.0);
        }
        runtime.stop_accepting();
        if let Some(receiver) = receiver {
            receiver.close();
        }
        commands.entity(entity).remove::<HttpServerBound>();
        let has_pending_requests = requests
            .iter()
            .any(|(parent, responder)| parent.parent() == entity && !responder.is_responded());
        if has_pending_requests || runtime.active_connections() > 0 {
            commands.entity(entity).insert(ShutdownBarrier);
        }
    }
}

fn sync_shutdown_barriers(
    servers: Query<(Entity, &ServerRuntime), With<ShutdownBarrier>>,
    requests: Query<(&ChildOf, &HttpResponder), With<HttpRequest>>,
    mut commands: Commands,
) {
    for (entity, runtime) in &servers {
        let has_pending_requests = requests
            .iter()
            .any(|(parent, responder)| parent.parent() == entity && !responder.is_responded());
        if !has_pending_requests && runtime.active_connections() == 0 {
            commands.entity(entity).remove::<ShutdownBarrier>();
        }
    }
}

fn finish_shutdown(
    requests: Query<Entity, With<HttpRequest>>,
    servers: Query<Entity, With<ServerRuntime>>,
    mut commands: Commands,
) {
    for entity in &requests {
        commands.entity(entity).try_despawn();
    }
    for entity in &servers {
        commands.entity(entity).try_remove::<(
            ServerRuntime,
            SpawnReceiver<IncomingRequest>,
            HttpServerBound,
            ShutdownBarrier,
        )>();
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    use serde::Deserialize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream as TokioTcpStream;
    use tokio::runtime::Runtime;
    use vmux_ecs::ShutdownRequest;
    use vmux_runner::HeadlessTokioRunner;

    use super::*;

    fn test_runtime() -> Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
    }

    fn send_raw_http(addr: SocketAddr, request: &str) -> String {
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        stream.flush().unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    async fn send_raw_http_async(addr: SocketAddr, request: &str) -> String {
        let mut stream = TokioTcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        String::from_utf8(response).unwrap()
    }

    fn wait_port_closed(addr: SocketAddr) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_err() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "port {addr} stayed open after server despawn"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
    struct EchoPayload {
        message: String,
    }

    fn handle_echo_requests(mut requests: Query<(&HttpRequest, &mut HttpResponder)>) {
        for (request, mut responder) in &mut requests {
            if request.method() == Method::GET && request.path() == "/hello" {
                let query = request.query().unwrap_or("");
                let response = HttpResponse::text(StatusCode::OK, format!("hello:{query}"))
                    .with_header("x-vmux-test", "ok");
                assert!(responder.respond(response));
            } else if request.method() == Method::POST && request.path() == "/echo" {
                let payload: EchoPayload = request.json().unwrap();
                let response = HttpResponse::json(StatusCode::CREATED, &payload).unwrap();
                assert!(responder.respond(response));
            } else {
                assert!(responder.respond(HttpResponse::not_found()));
            }
        }
    }

    #[test]
    fn get_and_post_requests_roundtrip_through_ecs_systems() {
        let runtime = test_runtime();
        let mut app = App::new();
        app.add_plugins((HttpServerPlugin, HttpServerPlugin))
            .insert_resource(HttpRuntime::new(runtime.handle().clone()))
            .add_systems(Update, handle_echo_requests.in_set(HttpServerSet::Serve));

        let server = app
            .world_mut()
            .spawn(HttpServer::bind("127.0.0.1:0".parse().unwrap()))
            .id();
        app.update();
        let addr = app
            .world()
            .get::<HttpServerBound>(server)
            .expect("server bound")
            .addr();

        let get_handle = std::thread::spawn(move || {
            send_raw_http(
                addr,
                "GET /hello?name=vmux HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while !get_handle.is_finished() {
            assert!(Instant::now() < deadline, "timed out waiting for GET");
            app.update();
            std::thread::sleep(Duration::from_millis(5));
        }
        let get_response = get_handle.join().unwrap();
        assert!(
            get_response.starts_with("HTTP/1.1 200 OK\r\n"),
            "{get_response}"
        );
        assert!(
            get_response.contains("x-vmux-test: ok\r\n"),
            "{get_response}"
        );
        assert!(
            get_response.ends_with("\r\n\r\nhello:name=vmux"),
            "{get_response}"
        );

        let body = r#"{"message":"ping"}"#;
        let post_request = format!(
            "POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let post_handle = std::thread::spawn(move || send_raw_http(addr, &post_request));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !post_handle.is_finished() {
            assert!(Instant::now() < deadline, "timed out waiting for POST");
            app.update();
            std::thread::sleep(Duration::from_millis(5));
        }
        let post_response = post_handle.join().unwrap();
        assert!(
            post_response.starts_with("HTTP/1.1 201 Created\r\n"),
            "{post_response}"
        );
        assert!(
            post_response.contains("content-type: application/json\r\n"),
            "{post_response}"
        );
        assert!(post_response.ends_with(body), "{post_response}");

        app.update();
        let remaining = app
            .world_mut()
            .query_filtered::<Entity, With<HttpRequest>>()
            .iter(app.world())
            .count();
        assert_eq!(remaining, 0);

        app.world_mut().despawn(server);
        wait_port_closed(addr);
    }

    fn despawn_unhandled_requests(
        requests: Query<Entity, With<HttpRequest>>,
        mut commands: Commands,
    ) {
        for entity in &requests {
            commands.entity(entity).despawn();
        }
    }

    #[test]
    fn despawning_unhandled_request_sends_500_instead_of_hanging() {
        let runtime = test_runtime();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let mut app = App::new();
        app.add_plugins(HttpServerPlugin).add_systems(
            Update,
            despawn_unhandled_requests.in_set(HttpServerSet::Serve),
        );

        let server = app
            .world_mut()
            .spawn(HttpServer::from_listener(listener).with_runtime(runtime.handle().clone()))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<HttpServerBound>(server).unwrap().addr(),
            addr
        );

        let client = std::thread::spawn(move || {
            send_raw_http(
                addr,
                "GET /drop HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while !client.is_finished() {
            assert!(Instant::now() < deadline, "client hung on dropped request");
            app.update();
            std::thread::sleep(Duration::from_millis(5));
        }
        let response = client.join().unwrap();
        assert!(
            response.starts_with("HTTP/1.1 500 Internal Server Error\r\n"),
            "{response}"
        );

        app.world_mut().despawn(server);
        wait_port_closed(addr);
    }

    #[derive(Resource)]
    struct DelayedReply {
        addr_tx: Option<oneshot::Sender<SocketAddr>>,
        release_rx: Option<oneshot::Receiver<()>>,
        shutdown_sent: bool,
    }

    fn publish_bound_addr(
        bound: Query<&HttpServerBound, Added<HttpServerBound>>,
        mut state: ResMut<DelayedReply>,
    ) {
        for bound in &bound {
            if let Some(tx) = state.addr_tx.take() {
                let _ = tx.send(bound.addr());
            }
        }
    }

    fn hold_then_reply_during_shutdown(
        mut requests: Query<(&HttpRequest, &mut HttpResponder)>,
        mut state: ResMut<DelayedReply>,
        mut shutdown: MessageWriter<ShutdownRequest>,
    ) {
        for (request, mut responder) in &mut requests {
            if request.path() == "/slow" && !state.shutdown_sent {
                state.shutdown_sent = true;
                shutdown.write(ShutdownRequest::success());
            }
            if let Some(rx) = state.release_rx.as_mut()
                && rx.try_recv().is_ok()
            {
                state.release_rx = None;
                responder.respond(HttpResponse::text(StatusCode::OK, "drained-ok"));
            }
        }
    }

    #[test]
    fn graceful_shutdown_drains_in_flight_request_and_closes_port() {
        let runtime = test_runtime();
        let (addr_tx, addr_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let (handle, mut runner) = HeadlessTokioRunner::handles(runtime.handle().clone());
        runner = runner.with_housekeeping_floor(Duration::from_secs(30));

        let mut app = App::new();
        app.add_plugins((ShutdownPlugin, HttpServerPlugin))
            .insert_resource(HttpRuntime::new(runtime.handle().clone()))
            .insert_resource(DelayedReply {
                addr_tx: Some(addr_tx),
                release_rx: Some(release_rx),
                shutdown_sent: false,
            })
            .add_systems(
                Update,
                (publish_bound_addr, hold_then_reply_during_shutdown).in_set(HttpServerSet::Serve),
            );
        app.world_mut()
            .spawn(HttpServer::bind("127.0.0.1:0".parse().unwrap()));

        let client_handle = runtime.spawn(async move {
            let addr = addr_rx.await.unwrap();
            let release_handle = handle.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                let _ = release_tx.send(());
                release_handle.wake();
            });
            let response = send_raw_http_async(
                addr,
                "GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .await;
            (addr, response)
        });

        let started = Instant::now();
        let exit = runner.run(&mut app);
        assert_eq!(exit, bevy_app::AppExit::Success);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "shutdown should complete immediately after in-flight request drains"
        );
        let (addr, response) = runtime.block_on(client_handle).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        assert!(response.ends_with("\r\n\r\ndrained-ok"), "{response}");
        wait_port_closed(addr);
    }

    #[test]
    fn missing_runtime_records_server_error() {
        let mut app = App::new();
        app.add_plugins(HttpServerPlugin);
        let server = app
            .world_mut()
            .spawn(HttpServer::bind("127.0.0.1:0".parse().unwrap()))
            .id();
        app.update();
        assert!(app.world().get::<HttpServerBound>(server).is_none());
        let error = app.world().get::<HttpServerError>(server).unwrap();
        assert!(error.message().contains("tokio runtime"));
    }

    #[test]
    fn oversized_request_body_returns_413_without_spawning_entity() {
        let runtime = test_runtime();
        let mut app = App::new();
        app.add_plugins(HttpServerPlugin)
            .insert_resource(HttpRuntime::new(runtime.handle().clone()));

        let server = app
            .world_mut()
            .spawn(HttpServer::bind("127.0.0.1:0".parse().unwrap()).with_max_body_bytes(4))
            .id();
        app.update();
        let addr = app
            .world()
            .get::<HttpServerBound>(server)
            .expect("server bound")
            .addr();

        let response = send_raw_http(
            addr,
            "POST /upload HTTP/1.1\r\nHost: localhost\r\nContent-Length: 8\r\nConnection: close\r\n\r\n12345678",
        );
        assert!(
            response.starts_with("HTTP/1.1 413 Payload Too Large\r\n"),
            "{response}"
        );
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<HttpRequest>>()
                .iter(app.world())
                .count(),
            0
        );

        app.world_mut().despawn(server);
        wait_port_closed(addr);
    }
}
