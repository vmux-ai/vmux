use bevy::prelude::Resource;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};
use vmux_core::extension::protocol::{
    BRIDGE_PROTOCOL_VERSION, BridgeClientMessage, BridgeServerMessage, ChromeError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeIdentity {
    pub extension_id: String,
    pub profile_id: String,
    pub token: String,
}

#[derive(Clone, Debug)]
pub struct BridgeInbound {
    pub extension_id: String,
    pub context_id: String,
    pub message: BridgeClientMessage,
}

#[derive(Resource)]
pub struct ExtensionBridgeServer {
    endpoint: String,
    identities: HashMap<String, BridgeIdentity>,
    inbound_rx: crossbeam_channel::Receiver<BridgeInbound>,
    sessions: Arc<Mutex<HashMap<String, crossbeam_channel::Sender<BridgeServerMessage>>>>,
    shutdown: Arc<AtomicBool>,
}

impl ExtensionBridgeServer {
    pub fn start<I, S>(profile: impl Into<String>, extension_ids: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let profile = profile.into();
        let identities = extension_ids
            .into_iter()
            .map(|extension_id| {
                let extension_id = extension_id.as_ref().to_string();
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
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
        let endpoint = format!(
            "ws://{}",
            listener.local_addr().map_err(|error| error.to_string())?
        );
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let (inbound_tx, inbound_rx) = crossbeam_channel::unbounded();
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_identities = identities.clone();
        let thread_sessions = Arc::clone(&sessions);
        let thread_shutdown = Arc::clone(&shutdown);
        std::thread::Builder::new()
            .name("extension-bridge-accept".into())
            .spawn(move || {
                accept_loop(
                    listener,
                    thread_identities,
                    inbound_tx,
                    thread_sessions,
                    thread_shutdown,
                );
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            endpoint,
            identities,
            inbound_rx,
            sessions,
            shutdown,
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn identity(&self, extension_id: &str) -> Option<&BridgeIdentity> {
        self.identities.get(extension_id)
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
        sender.send(message).map_err(|error| error.to_string())
    }
}

impl Drop for ExtensionBridgeServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

fn accept_loop(
    listener: TcpListener,
    identities: HashMap<String, BridgeIdentity>,
    inbound_tx: crossbeam_channel::Sender<BridgeInbound>,
    sessions: Arc<Mutex<HashMap<String, crossbeam_channel::Sender<BridgeServerMessage>>>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let identities = identities.clone();
                let inbound_tx = inbound_tx.clone();
                let sessions = Arc::clone(&sessions);
                let shutdown = Arc::clone(&shutdown);
                if let Err(error) = std::thread::Builder::new()
                    .name("extension-bridge-connection".into())
                    .spawn(move || {
                        if let Err(error) = handle_connection(
                            stream,
                            &identities,
                            &inbound_tx,
                            &sessions,
                            &shutdown,
                        ) {
                            bevy::log::warn!("extension bridge connection failed: {error}");
                        }
                    })
                {
                    bevy::log::warn!("failed to spawn extension bridge connection: {error}");
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                bevy::log::warn!("extension bridge accept failed: {error}");
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

fn handle_connection(
    stream: TcpStream,
    identities: &HashMap<String, BridgeIdentity>,
    inbound_tx: &crossbeam_channel::Sender<BridgeInbound>,
    sessions: &Arc<Mutex<HashMap<String, crossbeam_channel::Sender<BridgeServerMessage>>>>,
    shutdown: &Arc<AtomicBool>,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_millis(25)))
        .map_err(|error| error.to_string())?;
    let mut socket = tungstenite::accept(stream).map_err(|error| error.to_string())?;
    let Some((extension_id, context_id)) = authenticate(&mut socket, identities, shutdown)? else {
        return Ok(());
    };
    let (outbound_tx, outbound_rx) = crossbeam_channel::unbounded();
    sessions
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(extension_id.clone(), outbound_tx.clone());
    write_server_message(
        &mut socket,
        &BridgeServerMessage::Ready {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
        },
    )?;

    let result = route_connection(
        &mut socket,
        &extension_id,
        &context_id,
        inbound_tx,
        &outbound_rx,
        shutdown,
    );
    let mut sessions = sessions.lock().unwrap_or_else(|error| error.into_inner());
    if sessions
        .get(&extension_id)
        .is_some_and(|sender| sender.same_channel(&outbound_tx))
    {
        sessions.remove(&extension_id);
    }
    result
}

fn authenticate(
    socket: &mut WebSocket<TcpStream>,
    identities: &HashMap<String, BridgeIdentity>,
    shutdown: &AtomicBool,
) -> Result<Option<(String, String)>, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if shutdown.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Ok(None);
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                let Ok(BridgeClientMessage::Hello(hello)) = serde_json::from_str(&text) else {
                    reject_authentication(socket)?;
                    return Ok(None);
                };
                let authenticated = identities.get(&hello.extension_id).is_some_and(|identity| {
                    hello.protocol_version == BRIDGE_PROTOCOL_VERSION
                        && hello.extension_id == identity.extension_id
                        && hello.profile_id == identity.profile_id
                        && hello.token == identity.token
                });
                if !authenticated {
                    reject_authentication(socket)?;
                    return Ok(None);
                }
                return Ok(Some((hello.extension_id, hello.context_id)));
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
    context_id: &str,
    inbound_tx: &crossbeam_channel::Sender<BridgeInbound>,
    outbound_rx: &crossbeam_channel::Receiver<BridgeServerMessage>,
    shutdown: &AtomicBool,
) -> Result<(), String> {
    while !shutdown.load(Ordering::Acquire) {
        for message in outbound_rx.try_iter() {
            write_server_message(socket, &message)?;
        }
        match socket.read() {
            Ok(Message::Text(text)) => match serde_json::from_str(&text) {
                Ok(message @ BridgeClientMessage::ApiRequest(_))
                | Ok(message @ BridgeClientMessage::Subscribe(_))
                | Ok(message @ BridgeClientMessage::Ack { .. }) => inbound_tx
                    .send(BridgeInbound {
                        extension_id: extension_id.into(),
                        context_id: context_id.into(),
                        message,
                    })
                    .map_err(|error| error.to_string())?,
                Ok(BridgeClientMessage::Hello(_)) => {
                    bevy::log::warn!("extension bridge received duplicate hello");
                }
                Err(error) => {
                    bevy::log::warn!("extension bridge received malformed frame: {error}");
                }
            },
            Ok(Message::Ping(payload)) => socket
                .send(Message::Pong(payload))
                .map_err(|error| error.to_string())?,
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => bevy::log::warn!("extension bridge received unsupported frame"),
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(());
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    let _ = socket.close(None);
    Ok(())
}

fn write_server_message(
    socket: &mut WebSocket<TcpStream>,
    message: &BridgeServerMessage,
) -> Result<(), String> {
    let text = serde_json::to_string(message).map_err(|error| error.to_string())?;
    socket
        .send(Message::Text(text.into()))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use tungstenite::{Message, WebSocket, connect, stream::MaybeTlsStream};
    use vmux_core::extension::protocol::{
        ApiRequest, ApiResponse, BRIDGE_PROTOCOL_VERSION, BridgeClientMessage, BridgeHello,
        BridgeServerMessage, ChromeError, ExtensionContextKind,
    };

    const EXTENSION_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn send_json(
        socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
        message: &impl serde::Serialize,
    ) {
        socket
            .send(Message::Text(
                serde_json::to_string(message).unwrap().into(),
            ))
            .unwrap();
    }

    fn read_json<T: serde::de::DeserializeOwned>(
        socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    ) -> T {
        loop {
            match socket.read().unwrap() {
                Message::Text(text) => return serde_json::from_str(&text).unwrap(),
                Message::Ping(payload) => socket.send(Message::Pong(payload)).unwrap(),
                _ => {}
            }
        }
    }

    fn hello(identity: &BridgeIdentity, token: String) -> BridgeClientMessage {
        BridgeClientMessage::Hello(BridgeHello {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            extension_id: identity.extension_id.clone(),
            profile_id: identity.profile_id.clone(),
            token,
            context_id: "bridge-page".into(),
            context_kind: ExtensionContextKind::BridgePage,
        })
    }

    fn recv_inbound(server: &ExtensionBridgeServer) -> BridgeInbound {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(inbound) = server.try_recv() {
                return inbound;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for bridge frame"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn authenticates_and_routes_bidirectionally() {
        let server = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
        let identity = server.identity(EXTENSION_ID).unwrap().clone();
        let (mut socket, _) = connect(server.endpoint()).unwrap();
        send_json(&mut socket, &hello(&identity, identity.token.clone()));

        let ready: BridgeServerMessage = read_json(&mut socket);
        assert_eq!(
            ready,
            BridgeServerMessage::Ready {
                protocol_version: BRIDGE_PROTOCOL_VERSION
            }
        );
        let request = ApiRequest {
            request_id: "r1".into(),
            namespace: "tabs".into(),
            method: "query".into(),
            arguments: serde_json::json!({}),
        };
        send_json(
            &mut socket,
            &BridgeClientMessage::ApiRequest(request.clone()),
        );
        let inbound = recv_inbound(&server);
        assert_eq!(inbound.extension_id, EXTENSION_ID);
        assert_eq!(inbound.message, BridgeClientMessage::ApiRequest(request));
        server
            .send(
                EXTENSION_ID,
                BridgeServerMessage::Response(ApiResponse::success(
                    "r1",
                    serde_json::json!({ "ok": true }),
                )),
            )
            .unwrap();
        let response: BridgeServerMessage = read_json(&mut socket);
        assert_eq!(
            response,
            BridgeServerMessage::Response(ApiResponse::success(
                "r1",
                serde_json::json!({ "ok": true })
            ))
        );
    }

    #[test]
    fn rejects_wrong_token_and_closes_socket() {
        let server = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
        let identity = server.identity(EXTENSION_ID).unwrap().clone();
        let (mut socket, _) = connect(server.endpoint()).unwrap();
        send_json(&mut socket, &hello(&identity, "wrong".into()));

        let fatal: BridgeServerMessage = read_json(&mut socket);
        assert_eq!(
            fatal,
            BridgeServerMessage::Fatal(ChromeError::new(
                "authentication_failed",
                "bridge authentication failed"
            ))
        );
        assert!(matches!(
            socket.read(),
            Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed)
        ));
    }
}
