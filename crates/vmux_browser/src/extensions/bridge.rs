use bevy::prelude::Resource;
use polling::{Event, Events, Poller};
use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::StatusCode;
use tungstenite::{Message, WebSocket, protocol::WebSocketConfig};
use vmux_core::extension::match_pattern::ChromeMatchPattern;
use vmux_core::extension::protocol::{
    BRIDGE_CONTEXT_ID, BRIDGE_MAX_FRAME_SIZE, BRIDGE_MAX_MESSAGE_SIZE, BRIDGE_PROTOCOL_VERSION,
    BridgeClientMessage, BridgeServerMessage, ChromeError, ExtensionContextKind,
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
const AUTHENTICATION_TIMEOUT: Duration = Duration::from_secs(5);
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const READ_DRAIN_TIMEOUT: Duration = Duration::from_millis(1);
const READ_BUFFER_SIZE: usize = 4 * 1024;
const WRITE_BUFFER_SIZE: usize = 4 * 1024;
const MAX_WRITE_BUFFER_SIZE: usize = 1024 * 1024;
const MAX_INBOUND_MESSAGES: usize = 64;
const MAX_OUTBOUND_MESSAGES: usize = 64;
const MAX_OUTBOUND_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 64;
const MAX_UNAUTHENTICATED_CONNECTIONS: usize = 8;
const SOCKET_EVENT_KEY: usize = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeIdentity {
    pub extension_id: String,
    pub profile_id: String,
    pub token: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BridgeAuthorization {
    pub permissions: HashSet<String>,
    pub host_permissions: Vec<ChromeMatchPattern>,
    pub conformance: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BridgeRegistration {
    pub extension_id: String,
    pub authorization: BridgeAuthorization,
}

#[derive(Clone, Debug)]
pub struct BridgeInbound {
    pub extension_id: String,
    pub session_id: u64,
    pub context_id: String,
    pub context_kind: ExtensionContextKind,
    pub message: BridgeClientMessage,
}

#[derive(Clone)]
struct BridgeSession {
    session_id: u64,
    outbound_tx: crossbeam_channel::Sender<QueuedServerMessage>,
    queued_bytes: Arc<AtomicUsize>,
    poller: Arc<Poller>,
    cancelled: Arc<AtomicBool>,
}

struct QueuedServerMessage {
    message: BridgeServerMessage,
    bytes: usize,
}

#[derive(Resource)]
pub struct ExtensionBridgeServer {
    endpoint: String,
    identities: HashMap<String, BridgeIdentity>,
    authorizations: HashMap<String, BridgeAuthorization>,
    inbound_rx: crossbeam_channel::Receiver<BridgeInbound>,
    sessions: Arc<Mutex<HashMap<String, BridgeSession>>>,
    shutdown: Arc<AtomicBool>,
    accept_poller: Arc<Poller>,
}

impl ExtensionBridgeServer {
    #[cfg(test)]
    pub fn start<I, S>(profile: impl Into<String>, extension_ids: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::start_registered(
            profile,
            extension_ids
                .into_iter()
                .map(|extension_id| BridgeRegistration {
                    extension_id: extension_id.as_ref().to_string(),
                    authorization: BridgeAuthorization::default(),
                }),
        )
    }

    pub fn start_registered<I>(profile: impl Into<String>, registrations: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = BridgeRegistration>,
    {
        let profile = profile.into();
        let registrations = registrations.into_iter().collect::<Vec<_>>();
        let identities = registrations
            .iter()
            .map(|registration| {
                let extension_id = registration.extension_id.clone();
                (
                    extension_id.clone(),
                    BridgeIdentity {
                        extension_id,
                        profile_id: profile.clone(),
                        token: uuid::Uuid::new_v4().to_string(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let authorizations = registrations
            .into_iter()
            .map(|registration| (registration.extension_id, registration.authorization))
            .collect::<HashMap<_, _>>();
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
        let endpoint = format!(
            "ws://{}",
            listener.local_addr().map_err(|error| error.to_string())?
        );
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let accept_poller = Arc::new(Poller::new().map_err(|error| error.to_string())?);
        unsafe {
            accept_poller
                .add(&listener, Event::readable(SOCKET_EVENT_KEY))
                .map_err(|error| error.to_string())?;
        }
        let (inbound_tx, inbound_rx) = crossbeam_channel::bounded(MAX_INBOUND_MESSAGES);
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let active_connections = Arc::new(AtomicUsize::new(0));
        let unauthenticated_connections = Arc::new(AtomicUsize::new(0));
        let next_session_id = Arc::new(AtomicU64::new(1));
        let thread_identities = identities.clone();
        let thread_sessions = Arc::clone(&sessions);
        let thread_shutdown = Arc::clone(&shutdown);
        let thread_accept_poller = Arc::clone(&accept_poller);
        std::thread::Builder::new()
            .name("extension-bridge-accept".into())
            .spawn(move || {
                accept_loop(
                    listener,
                    thread_identities,
                    inbound_tx,
                    thread_sessions,
                    thread_shutdown,
                    active_connections,
                    unauthenticated_connections,
                    thread_accept_poller,
                    next_session_id,
                );
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            endpoint,
            identities,
            authorizations,
            inbound_rx,
            sessions,
            shutdown,
            accept_poller,
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn identity(&self, extension_id: &str) -> Option<&BridgeIdentity> {
        self.identities.get(extension_id)
    }

    pub fn authorization(&self, extension_id: &str) -> Option<&BridgeAuthorization> {
        self.authorizations.get(extension_id)
    }

    pub fn try_recv(&self) -> Result<BridgeInbound, crossbeam_channel::TryRecvError> {
        self.inbound_rx.try_recv()
    }

    pub fn send(&self, extension_id: &str, message: BridgeServerMessage) -> Result<(), String> {
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let sender = sessions
            .get(extension_id)
            .ok_or_else(|| format!("extension {extension_id} is not connected"))?;
        queue_session_message(sender, message)
    }

    pub fn send_to_session(
        &self,
        extension_id: &str,
        session_id: u64,
        message: BridgeServerMessage,
    ) -> Result<(), String> {
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let session = sessions
            .get(extension_id)
            .filter(|session| session.session_id == session_id)
            .ok_or_else(|| format!("extension {extension_id} session is no longer active"))?;
        queue_session_message(session, message)
    }

    pub fn is_current_session(&self, extension_id: &str, session_id: u64) -> bool {
        self.sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(extension_id)
            .is_some_and(|session| session.session_id == session_id)
    }
}

impl Drop for ExtensionBridgeServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = self.accept_poller.notify();
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for session in sessions.values() {
            session.cancelled.store(true, Ordering::Release);
            let _ = session.poller.notify();
        }
    }
}

fn queue_session_message(
    session: &BridgeSession,
    message: BridgeServerMessage,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(&message)
        .map_err(|error| error.to_string())?
        .len();
    if bytes > BRIDGE_MAX_FRAME_SIZE {
        return Err("extension bridge outbound frame exceeds size limit".into());
    }
    session
        .queued_bytes
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
            queued
                .checked_add(bytes)
                .filter(|total| *total <= MAX_OUTBOUND_BYTES)
        })
        .map_err(|_| "extension bridge outbound byte queue is full".to_string())?;
    if let Err(error) = session
        .outbound_tx
        .try_send(QueuedServerMessage { message, bytes })
    {
        session.queued_bytes.fetch_sub(bytes, Ordering::AcqRel);
        return Err(error.to_string());
    }
    session.poller.notify().map_err(|error| error.to_string())
}

fn accept_loop(
    listener: TcpListener,
    identities: HashMap<String, BridgeIdentity>,
    inbound_tx: crossbeam_channel::Sender<BridgeInbound>,
    sessions: Arc<Mutex<HashMap<String, BridgeSession>>>,
    shutdown: Arc<AtomicBool>,
    active_connections: Arc<AtomicUsize>,
    unauthenticated_connections: Arc<AtomicUsize>,
    poller: Arc<Poller>,
    next_session_id: Arc<AtomicU64>,
) {
    let mut events = Events::new();
    while !shutdown.load(Ordering::Acquire) {
        events.clear();
        if let Err(error) = poller.wait(&mut events, None) {
            bevy::log::warn!("extension bridge accept poll failed: {error}");
            break;
        }
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    let Some(active_permit) = try_acquire(&active_connections, MAX_CONNECTIONS)
                    else {
                        continue;
                    };
                    let Some(unauthenticated_permit) = try_acquire(
                        &unauthenticated_connections,
                        MAX_UNAUTHENTICATED_CONNECTIONS,
                    ) else {
                        drop(active_permit);
                        continue;
                    };
                    let identities = identities.clone();
                    let inbound_tx = inbound_tx.clone();
                    let sessions = Arc::clone(&sessions);
                    let shutdown = Arc::clone(&shutdown);
                    let session_id = next_session_id.fetch_add(1, Ordering::AcqRel).max(1);
                    if let Err(error) = std::thread::Builder::new()
                        .name("extension-bridge-connection".into())
                        .spawn(move || {
                            let _active_permit = active_permit;
                            if let Err(error) = handle_connection(
                                stream,
                                &identities,
                                &inbound_tx,
                                &sessions,
                                &shutdown,
                                unauthenticated_permit,
                                session_id,
                            ) {
                                bevy::log::warn!("extension bridge connection failed: {error}");
                            }
                        })
                    {
                        bevy::log::warn!("failed to spawn extension bridge connection: {error}");
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => {
                    bevy::log::warn!("extension bridge accept failed: {error}");
                    break;
                }
            }
        }
        if let Err(error) = poller.modify(&listener, Event::readable(SOCKET_EVENT_KEY)) {
            bevy::log::warn!("extension bridge accept rearm failed: {error}");
            break;
        }
    }
    let _ = poller.delete(&listener);
}

struct CounterGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for CounterGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
    }
}

fn try_acquire(counter: &Arc<AtomicUsize>, limit: usize) -> Option<CounterGuard> {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < limit).then_some(count + 1)
        })
        .ok()
        .map(|_| CounterGuard {
            counter: Arc::clone(counter),
        })
}

fn handle_connection(
    stream: TcpStream,
    identities: &HashMap<String, BridgeIdentity>,
    inbound_tx: &crossbeam_channel::Sender<BridgeInbound>,
    sessions: &Arc<Mutex<HashMap<String, BridgeSession>>>,
    shutdown: &Arc<AtomicBool>,
    unauthenticated_permit: CounterGuard,
    session_id: u64,
) -> Result<(), String> {
    let authentication_deadline = Instant::now() + AUTHENTICATION_TIMEOUT;
    stream
        .set_nonblocking(false)
        .map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(AUTHENTICATION_TIMEOUT))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|error| error.to_string())?;
    let (mut socket, origin_extension_id) =
        accept_websocket(stream, identities, authentication_deadline)?;
    let Some(extension_id) = authenticate(
        &mut socket,
        identities,
        &origin_extension_id,
        shutdown,
        authentication_deadline,
    )?
    else {
        return Ok(());
    };
    drop(unauthenticated_permit);
    socket
        .get_ref()
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|error| error.to_string())?;
    bevy::log::info!("extension bridge authenticated");
    let poller = Arc::new(Poller::new().map_err(|error| error.to_string())?);
    unsafe {
        poller
            .add(socket.get_ref(), Event::readable(SOCKET_EVENT_KEY))
            .map_err(|error| error.to_string())?;
    }
    let (outbound_tx, outbound_rx) = crossbeam_channel::bounded(MAX_OUTBOUND_MESSAGES);
    let queued_bytes = Arc::new(AtomicUsize::new(0));
    let cancelled = Arc::new(AtomicBool::new(false));
    let session = BridgeSession {
        session_id,
        outbound_tx: outbound_tx.clone(),
        queued_bytes: Arc::clone(&queued_bytes),
        poller: Arc::clone(&poller),
        cancelled: Arc::clone(&cancelled),
    };
    let previous = sessions
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(extension_id.clone(), session);
    if let Some(previous) = previous {
        previous.cancelled.store(true, Ordering::Release);
        let _ = previous.poller.notify();
    }

    let result = write_server_message(
        &mut socket,
        &BridgeServerMessage::Ready {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
        },
    )
    .and_then(|()| {
        route_connection(
            &mut socket,
            &extension_id,
            session_id,
            inbound_tx,
            &outbound_rx,
            &queued_bytes,
            shutdown,
            &cancelled,
            &poller,
        )
    });
    let mut sessions = sessions.lock().unwrap_or_else(|error| error.into_inner());
    if sessions
        .get(&extension_id)
        .is_some_and(|session| session.session_id == session_id)
    {
        sessions.remove(&extension_id);
    }
    drop(sessions);
    let _ = poller.delete(socket.get_ref());
    result
}

#[allow(clippy::result_large_err)]
fn accept_websocket(
    stream: TcpStream,
    identities: &HashMap<String, BridgeIdentity>,
    deadline: Instant,
) -> Result<(WebSocket<TcpStream>, String), String> {
    let config = WebSocketConfig::default()
        .read_buffer_size(READ_BUFFER_SIZE)
        .write_buffer_size(WRITE_BUFFER_SIZE)
        .max_write_buffer_size(MAX_WRITE_BUFFER_SIZE)
        .max_frame_size(Some(BRIDGE_MAX_FRAME_SIZE))
        .max_message_size(Some(BRIDGE_MAX_MESSAGE_SIZE));
    let completed = Arc::new(AtomicBool::new(false));
    let watchdog_completed = Arc::clone(&completed);
    let watchdog_stream = stream.try_clone().map_err(|error| error.to_string())?;
    std::thread::Builder::new()
        .name("extension-bridge-handshake-timeout".into())
        .spawn(move || {
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            if !watchdog_completed.load(Ordering::Acquire) {
                let _ = watchdog_stream.shutdown(Shutdown::Both);
            }
        })
        .map_err(|error| error.to_string())?;
    let mut origin_extension_id = None;
    let socket = tungstenite::accept_hdr_with_config(
        stream,
        |request: &Request, response: Response| {
            let Some(extension_id) = extension_origin(request) else {
                return Err(reject_handshake());
            };
            if !identities.contains_key(&extension_id) {
                return Err(reject_handshake());
            }
            origin_extension_id = Some(extension_id);
            Ok(response)
        },
        Some(config),
    );
    completed.store(true, Ordering::Release);
    let socket = socket.map_err(|error| error.to_string())?;
    origin_extension_id
        .map(|origin| (socket, origin))
        .ok_or_else(|| "bridge handshake did not record an extension origin".into())
}

fn extension_origin(request: &Request) -> Option<String> {
    let origin = request.headers().get("origin")?.to_str().ok()?;
    let extension_id = origin
        .strip_prefix("chrome-extension://")?
        .trim_end_matches('/');
    (extension_id.len() == 32 && extension_id.bytes().all(|byte| matches!(byte, b'a'..=b'p')))
        .then(|| extension_id.to_string())
}

fn reject_handshake() -> ErrorResponse {
    let mut response = ErrorResponse::new(Some("forbidden bridge origin".into()));
    *response.status_mut() = StatusCode::FORBIDDEN;
    response
}

fn authenticate(
    socket: &mut WebSocket<TcpStream>,
    identities: &HashMap<String, BridgeIdentity>,
    origin_extension_id: &str,
    shutdown: &AtomicBool,
    deadline: Instant,
) -> Result<Option<String>, String> {
    loop {
        let now = Instant::now();
        if shutdown.load(Ordering::Acquire) || now >= deadline {
            return Ok(None);
        }
        socket
            .get_ref()
            .set_read_timeout(Some(deadline - now))
            .map_err(|error| error.to_string())?;
        match socket.read() {
            Ok(Message::Text(text)) => {
                let Ok(BridgeClientMessage::Hello(hello)) = serde_json::from_str(&text) else {
                    reject_authentication(socket)?;
                    return Ok(None);
                };
                let authenticated = identities.get(&hello.extension_id).is_some_and(|identity| {
                    hello.protocol_version == BRIDGE_PROTOCOL_VERSION
                        && hello.extension_id == origin_extension_id
                        && hello.extension_id == identity.extension_id
                        && hello.profile_id == identity.profile_id
                        && hello.token == identity.token
                        && hello.context_id == BRIDGE_CONTEXT_ID
                        && hello.context_kind == ExtensionContextKind::BridgePage
                });
                if !authenticated {
                    reject_authentication(socket)?;
                    return Ok(None);
                }
                return Ok(Some(hello.extension_id));
            }
            Ok(Message::Ping(payload)) => socket
                .send(Message::Pong(payload))
                .map_err(|error| error.to_string())?,
            Ok(Message::Close(_)) => return Ok(None),
            Ok(_) => {
                reject_authentication(socket)?;
                return Ok(None);
            }
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(None);
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn reject_authentication(socket: &mut WebSocket<TcpStream>) -> Result<(), String> {
    write_server_message(
        socket,
        &BridgeServerMessage::Fatal(ChromeError::new(
            "authentication_failed",
            "bridge authentication failed",
        )),
    )?;
    socket.close(None).map_err(|error| error.to_string())
}

fn route_connection(
    socket: &mut WebSocket<TcpStream>,
    extension_id: &str,
    session_id: u64,
    inbound_tx: &crossbeam_channel::Sender<BridgeInbound>,
    outbound_rx: &crossbeam_channel::Receiver<QueuedServerMessage>,
    queued_bytes: &AtomicUsize,
    shutdown: &AtomicBool,
    cancelled: &AtomicBool,
    poller: &Poller,
) -> Result<(), String> {
    let mut next_heartbeat = Instant::now() + HEARTBEAT_INTERVAL;
    let mut events = Events::new();
    if !read_available_messages(socket, extension_id, session_id, inbound_tx)? {
        return Ok(());
    }
    poller
        .modify(socket.get_ref(), Event::readable(SOCKET_EVENT_KEY))
        .map_err(|error| error.to_string())?;
    while !shutdown.load(Ordering::Acquire) && !cancelled.load(Ordering::Acquire) {
        loop {
            match outbound_rx.try_recv() {
                Ok(queued) => {
                    queued_bytes.fetch_sub(queued.bytes, Ordering::AcqRel);
                    let fatal = matches!(queued.message, BridgeServerMessage::Fatal(_));
                    write_server_message(socket, &queued.message)?;
                    if fatal {
                        let _ = socket.close(None);
                        return Ok(());
                    }
                }
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => return Ok(()),
            }
        }
        if Instant::now() >= next_heartbeat {
            write_server_message(socket, &BridgeServerMessage::Heartbeat)?;
            next_heartbeat = Instant::now() + HEARTBEAT_INTERVAL;
        }
        events.clear();
        poller
            .wait(
                &mut events,
                Some(next_heartbeat.saturating_duration_since(Instant::now())),
            )
            .map_err(|error| error.to_string())?;
        if shutdown.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
            break;
        }
        if !events
            .iter()
            .any(|event| event.key == SOCKET_EVENT_KEY && event.readable)
        {
            continue;
        }
        if !read_available_messages(socket, extension_id, session_id, inbound_tx)? {
            return Ok(());
        }
        poller
            .modify(socket.get_ref(), Event::readable(SOCKET_EVENT_KEY))
            .map_err(|error| error.to_string())?;
    }
    let _ = socket.close(None);
    Ok(())
}

fn read_available_messages(
    socket: &mut WebSocket<TcpStream>,
    extension_id: &str,
    session_id: u64,
    inbound_tx: &crossbeam_channel::Sender<BridgeInbound>,
) -> Result<bool, String> {
    socket
        .get_ref()
        .set_read_timeout(Some(READ_DRAIN_TIMEOUT))
        .map_err(|error| error.to_string())?;
    let result = loop {
        let result = match socket.read() {
            Ok(Message::Text(text)) => match serde_json::from_str(&text) {
                Ok(message @ BridgeClientMessage::ApiRequest(_))
                | Ok(message @ BridgeClientMessage::Subscribe(_))
                | Ok(message @ BridgeClientMessage::Unsubscribe { .. })
                | Ok(message @ BridgeClientMessage::Ack { .. }) => {
                    inbound_tx
                        .try_send(BridgeInbound {
                            extension_id: extension_id.into(),
                            session_id,
                            context_id: BRIDGE_CONTEXT_ID.into(),
                            context_kind: ExtensionContextKind::BridgePage,
                            message,
                        })
                        .map_err(|error| {
                            format!("extension bridge inbound queue failed: {error}")
                        })?;
                    Ok(())
                }
                Ok(BridgeClientMessage::Hello(_)) => {
                    Err("extension bridge received duplicate hello".into())
                }
                Err(error) => Err(format!(
                    "extension bridge received malformed frame: {error}"
                )),
            },
            Ok(Message::Ping(payload)) => socket
                .send(Message::Pong(payload))
                .map_err(|error| error.to_string()),
            Ok(Message::Pong(_)) => Ok(()),
            Ok(Message::Close(_)) => break Ok(false),
            Ok(_) => Err("extension bridge received unsupported frame".into()),
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                break Ok(true);
            }
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                break Ok(false);
            }
            Err(error) => Err(error.to_string()),
        };
        if let Err(error) = result {
            let _ = write_server_message(
                socket,
                &BridgeServerMessage::Fatal(ChromeError::new("protocol_error", &error)),
            );
            let _ = socket.close(None);
            break Err(error);
        }
    };
    socket
        .get_ref()
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|error| error.to_string())?;
    result
}

fn write_server_message(
    socket: &mut WebSocket<TcpStream>,
    message: &BridgeServerMessage,
) -> Result<(), String> {
    let text = serde_json::to_string(message).map_err(|error| error.to_string())?;
    if text.len() > BRIDGE_MAX_FRAME_SIZE {
        return Err("extension bridge outbound frame exceeds size limit".into());
    }
    socket
        .send(Message::Text(text.into()))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "bridge.test.rs"]
mod tests;
