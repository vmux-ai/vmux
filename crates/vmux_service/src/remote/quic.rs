//! The only way a phone reaches this Mac.
//!
//! Two controls that came free from HTTP middleware had to be rebuilt, because a QUIC connection
//! is long-lived where a request is not:
//!
//! - **The Remote kill switch.** Re-reading the state file per request meant turning Remote off
//!   took effect immediately. A connection that authenticated once would outlive the switch, so a
//!   single watcher closes every live peer instead.
//! - **Request limits.** `receive_window` bounds what a peer can buffer before the dispatcher's own
//!   size check runs — the job a request body limit used to do.

pub mod dispatch;

pub(crate) mod dialer;

use std::time::Duration;

use tokio::sync::watch;

use vmux_remote::quic::endpoint::SelfSignedIdentity;

use vmux_wire::protocol::{ServiceMessage, SharedMessage};

use vmux_remote::quic::{
    ClientHello, CloseCode, ServerHello, StreamKind, decode_hello, encode_hello,
};

/// How often the kill switch is re-read. Matches the HTTP path's in-stream recheck.
const REMOTE_STATE_POLL: Duration = Duration::from_secs(1);

/// Cap on a hello frame, so a peer cannot make the daemon buffer indefinitely before it has
/// authenticated. Far above any legitimate hello.
const MAX_HELLO_BYTES: usize = 16 * 1024;

/// Load the persisted identity, generating one on first use.
///
/// Reused across launches because the pairing link records the fingerprint; minting per start
/// would unpair every phone on restart.
pub fn ensure_identity() -> std::io::Result<SelfSignedIdentity> {
    let cert_path = crate::remote_cert_path();
    let key_path = crate::remote_key_path();

    if let (Ok(certificate_pem), Ok(private_key_pem)) = (
        std::fs::read_to_string(&cert_path),
        std::fs::read_to_string(&key_path),
    ) && !certificate_pem.trim().is_empty()
        && !private_key_pem.trim().is_empty()
    {
        let identity = SelfSignedIdentity::from_pem(certificate_pem, private_key_pem)
            .map_err(std::io::Error::other)?;
        persist_fingerprint(&identity.fingerprint)?;
        return Ok(identity);
    }

    let identity =
        SelfSignedIdentity::generate(subject_alt_names()).map_err(std::io::Error::other)?;
    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cert_path, &identity.certificate_pem)?;
    super::write_private(&key_path, &identity.private_key_pem)?;
    persist_fingerprint(&identity.fingerprint)?;
    Ok(identity)
}

/// Record the fingerprint beside the certificate, for readers that cannot hash a PEM.
fn persist_fingerprint(fingerprint: &str) -> std::io::Result<()> {
    let path = crate::remote_fingerprint_path();
    if std::fs::read_to_string(&path).is_ok_and(|existing| existing.trim() == fingerprint) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, fingerprint)
}

/// The fingerprint of the certificate this desktop presents, for the pairing link.
///
/// Reads the persisted identity rather than the live listener, so the GUI can build a pairing
/// link without reaching into the daemon's process.
pub fn identity_fingerprint() -> Option<String> {
    let pem = std::fs::read_to_string(crate::remote_cert_path()).ok()?;
    SelfSignedIdentity::fingerprint_of_pem(&pem).ok()
}

/// The names a phone might dial this desktop by. The certificate is pinned by fingerprint, so
/// these are belt-and-braces rather than the trust decision.
fn subject_alt_names() -> Vec<String> {
    vec!["localhost".to_string(), "127.0.0.1".to_string()]
}

/// Why a connection was turned away, so the peer is told rather than merely dropped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rejection {
    Unauthorized,
    RemoteDisabled,
    Malformed,
}

impl Rejection {
    pub fn close_code(self) -> CloseCode {
        match self {
            Self::Unauthorized => CloseCode::Unauthorized,
            Self::RemoteDisabled => CloseCode::RemoteDisabled,
            Self::Malformed => CloseCode::ProtocolError,
        }
    }
}

/// Decide whether a hello is acceptable, given the token and the current kill-switch state.
///
/// Separated from the I/O so the decision is testable without a socket, and so the order of the
/// checks is visible: the kill switch first, then the secret. A peer learns nothing about the
/// token from a `RemoteDisabled` answer.
pub fn admit(
    presented_token: &str,
    expected_token: &str,
    remote_enabled: bool,
) -> Result<ServerHello, Rejection> {
    if !remote_enabled {
        return Err(Rejection::RemoteDisabled);
    }
    if !super::server::secure_eq(presented_token, expected_token) {
        return Err(Rejection::Unauthorized);
    }
    Ok(ServerHello {})
}

/// The bearer token is carried in the hello rather than a header, since QUIC has none.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AuthenticatedHello {
    #[serde(flatten)]
    pub hello: ClientHello,
    pub token: String,
}

/// Watches the Remote kill switch and closes every live connection when it goes off.
///
/// One task and one file read per second for the whole daemon, against the HTTP path's read per
/// request plus a per-stream ticker.
pub fn spawn_liveness_watch() -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(super::server::remote_enabled());
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(REMOTE_STATE_POLL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let enabled = super::server::remote_enabled();
            if *tx.borrow() != enabled {
                if tx.send(enabled).is_err() {
                    return;
                }
                tracing::info!(enabled, "remote quic: exposure changed");
            }
        }
    });
    rx
}

/// Read one hello frame, bounded so an unauthenticated peer cannot buffer without limit.
pub async fn read_hello(stream: &mut quinn::RecvStream) -> Result<AuthenticatedHello, Rejection> {
    let bytes = stream
        .read_to_end(MAX_HELLO_BYTES)
        .await
        .map_err(|_| Rejection::Malformed)?;
    decode_hello::<AuthenticatedHello>(&bytes)
        .map(|(hello, _)| hello)
        .map_err(|_| Rejection::Malformed)
}

/// Serve one accepted connection: exchange hellos, then dispatch its streams.
async fn serve(
    connection: quinn::Connection,
    state: super::server::RemoteState,
    mut liveness: watch::Receiver<bool>,
) {
    let token = state.token.clone();
    let paired = state.paired.clone();
    let Ok((mut send, mut recv)) = connection.accept_bi().await else {
        return;
    };
    let admitted = match read_hello(&mut recv).await {
        Ok(authenticated) => admit(&authenticated.token, &token, *liveness.borrow()),
        Err(rejection) => Err(rejection),
    };

    let server_hello = match admitted {
        Ok(server_hello) => server_hello,
        Err(rejection) => {
            tracing::info!(?rejection, "remote quic: connection refused");
            connection.close(rejection.close_code().as_u32().into(), b"refused");
            return;
        }
    };

    let Ok(bytes) = encode_hello(&server_hello) else {
        connection.close(CloseCode::ProtocolError.as_u32().into(), b"hello");
        return;
    };
    if send.write_all(&bytes).await.is_err() || send.finish().is_err() {
        return;
    }
    super::server::mark_paired(&paired);

    loop {
        tokio::select! {
            changed = liveness.changed() => {
                if changed.is_err() || !*liveness.borrow() {
                    connection.close(CloseCode::RemoteDisabled.as_u32().into(), b"remote off");
                    return;
                }
            }
            accepted = connection.accept_bi() => {
                match accepted {
                    Ok((send, recv)) => {
                        tokio::spawn(dispatch_control(state.clone(), send, recv));
                    }
                    Err(_) => return,
                }
            }
        }
    }
}

/// Largest request frame accepted on a control stream.
///
/// Above the prompt cap so an oversized prompt is refused by the dispatcher with a reason, rather
/// than looking to the client like a broken connection.
const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;

/// One request in, one response out, then the stream closes.
///
/// The stream kind leads so the peer can route without decoding, and so a client that opens the
/// wrong kind is told rather than left waiting.
async fn dispatch_control(
    state: super::server::RemoteState,
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
) {
    let Ok(bytes) = recv.read_to_end(MAX_REQUEST_BYTES).await else {
        return;
    };
    let Some((kind, body)) = bytes.split_first() else {
        return;
    };
    let Some(kind) = StreamKind::from_byte(*kind) else {
        return;
    };
    // Copied so rkyv sees an aligned buffer; a slice at a one-byte offset is not.
    let body = body.to_vec();
    let Ok(request) = rkyv::from_bytes::<SharedMessage, rkyv::rancor::Error>(&body) else {
        return;
    };

    match kind {
        StreamKind::Control => {
            let response = dispatch::dispatch(&state, request).await;
            let Ok(encoded) = rkyv::to_bytes::<rkyv::rancor::Error>(&response) else {
                return;
            };
            if send.write_all(&encoded).await.is_ok() {
                let _ = send.finish();
            }
        }
        StreamKind::SessionEvents => stream_session_events(&state, send, request).await,
    }
}

/// Push a session's events until the client goes away.
///
/// The client opens this stream and writes once; everything after flows the other way. That shape
/// is deliberate: the relay only routes streams the *client* opens, so a desktop-initiated stream
/// would work on a direct connection and vanish through the relay — a difference that would not
/// show up until someone tested off their own network.
async fn stream_session_events(
    state: &super::server::RemoteState,
    mut send: quinn::SendStream,
    request: SharedMessage,
) {
    let SharedMessage::Agent {
        sid,
        action: vmux_wire::protocol::AgentAction::Attach,
    } = request
    else {
        return;
    };
    let Some(mut events) = subscribe(state, &sid).await else {
        // No such session. Closing empty says so without inventing an error frame.
        let _ = send.finish();
        return;
    };

    // Snapshot first, so a client that attaches mid-conversation renders the transcript it
    // missed rather than only what happens next.
    if let Some(snapshot) = session_snapshot(state, &sid).await
        && write_event(&mut send, &snapshot).await.is_err()
    {
        return;
    }

    loop {
        match events.recv().await {
            Ok(message) => {
                // Only the shared half leaves the machine. Everything else a session emits —
                // terminal output, proposed diffs, process lifecycle — is dropped here.
                let ServiceMessage::Shared(event) = message else {
                    continue;
                };
                let event = match resolve(state, &sid, event).await {
                    Some(event) => event,
                    None => continue,
                };
                if write_event(&mut send, &event).await.is_err() {
                    return;
                }
            }
            // Lagged means the client fell behind and frames were dropped. Resending a snapshot
            // is what the HTTP path did, and it beats a gap the client cannot detect.
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                let Some(snapshot) = session_snapshot(state, &sid).await else {
                    return;
                };
                if write_event(&mut send, &snapshot).await.is_err() {
                    return;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                let _ = send.finish();
                return;
            }
        }
    }
}

/// Replace the ACP status events with the session they imply.
///
/// A client renders a session's name, model and workspace from daemon state it has no other way
/// to read, so forwarding the raw event would leave the card stale. The HTTP path resolved these
/// the same way before serialising.
async fn resolve(
    state: &super::server::RemoteState,
    sid: &str,
    event: vmux_wire::protocol::SharedEvent,
) -> Option<vmux_wire::protocol::SharedEvent> {
    use vmux_wire::protocol::SharedEvent as Shared;
    match event {
        Shared::AcpAgentInfo { .. }
        | Shared::AcpModelInfo { .. }
        | Shared::AcpWorkspaceChanged { .. } => {
            let session = super::server::current_session(state, sid).await?;
            Some(Shared::Session { session })
        }
        other => Some(other),
    }
}

/// Length-prefixed, because unlike a control response there are many of these on one stream and
/// the reader needs to know where each ends.
async fn write_event(
    send: &mut quinn::SendStream,
    event: &vmux_wire::protocol::SharedEvent,
) -> Result<(), ()> {
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(event).map_err(|_| ())?;
    let length = u32::try_from(bytes.len()).map_err(|_| ())?;
    send.write_all(&length.to_le_bytes())
        .await
        .map_err(|_| ())?;
    send.write_all(&bytes).await.map_err(|_| ())
}

async fn subscribe(
    state: &super::server::RemoteState,
    sid: &str,
) -> Option<tokio::sync::broadcast::Receiver<ServiceMessage>> {
    if let Some(receiver) = state.acp.lock().await.subscribe(sid) {
        return Some(receiver);
    }
    state.agents.lock().await.subscribe(sid)
}

async fn session_snapshot(
    state: &super::server::RemoteState,
    sid: &str,
) -> Option<vmux_wire::protocol::SharedEvent> {
    let snapshot = if state.acp.lock().await.contains(sid) {
        state.acp.lock().await.snapshot(sid)
    } else {
        state.agents.lock().await.snapshot(sid).await
    }?;
    match snapshot {
        ServiceMessage::Shared(event) => Some(event),
        _ => None,
    }
}

/// Register with the relay and serve the phones it tunnels back.
///
/// There is no listener any more. A desktop behind NAT cannot be dialled, so it dials out and the
/// relay carries phone packets back over that connection; the endpoint those packets terminate on
/// is built in [`inner_endpoint`], over a socket that is not a socket.
pub(crate) fn spawn(
    state: super::server::RemoteState,
) -> std::io::Result<tokio::task::JoinHandle<()>> {
    let liveness = spawn_liveness_watch();
    Ok(dialer::spawn(state, ensure_identity()?, liveness))
}

/// An endpoint that terminates phone sessions arriving through the relay tunnel.
///
/// Same certificate and same server config a direct listener would have used — from the phone's
/// side nothing about the TLS session differs, which is the point: the relay is carrying packets,
/// not participating in them.
pub(crate) fn inner_endpoint(
    socket: std::sync::Arc<vmux_remote::quic::tunnel::TunnelSocket>,
    identity: &SelfSignedIdentity,
) -> Result<quinn::Endpoint, String> {
    let config = identity.server_config()?;
    quinn::Endpoint::new_with_abstract_socket(
        quinn::EndpointConfig::default(),
        Some(config),
        socket,
        std::sync::Arc::new(quinn::TokioRuntime),
    )
    .map_err(|error| format!("tunnel endpoint failed: {error}"))
}

/// Serve tunnelled connections until the control connection underneath them drops.
pub(crate) async fn accept_loop(
    endpoint: quinn::Endpoint,
    state: super::server::RemoteState,
    liveness: watch::Receiver<bool>,
    control: quinn::Connection,
) {
    loop {
        tokio::select! {
            _ = control.closed() => return,
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { return };
                let state = state.clone();
                let liveness = liveness.clone();
                tokio::spawn(async move {
                    match incoming.await {
                        Ok(connection) => serve(connection, state, liveness).await,
                        Err(error) => tracing::debug!(%error, "remote quic: handshake failed"),
                    }
                });
            }
        }
    }
}

/// Bind a real listener for a caller-supplied identity.
///
/// Test-only since the cutover: production is reached through the relay tunnel, never by being
/// dialled. The tests keep binding a socket because driving `serve` over one is what proves the
/// hello exchange and dispatch still work.
#[cfg(test)]
pub(crate) fn spawn_with_identity(
    state: super::server::RemoteState,
    address: std::net::SocketAddr,
    identity: SelfSignedIdentity,
    liveness: watch::Receiver<bool>,
) -> std::io::Result<(tokio::task::JoinHandle<()>, std::net::SocketAddr)> {
    let endpoint = identity.listen(address).map_err(std::io::Error::other)?;
    let bound = endpoint.local_addr()?;
    tracing::info!(
        port = bound.port(),
        fingerprint = %identity.fingerprint,
        "remote quic: listening"
    );

    let handle = tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let state = state.clone();
            let liveness = liveness.clone();
            tokio::spawn(async move {
                match incoming.await {
                    Ok(connection) => serve(connection, state, liveness).await,
                    Err(error) => tracing::debug!(%error, "remote quic: handshake failed"),
                }
            });
        }
    });
    Ok((handle, bound))
}

/// Drives the real listener over a real QUIC connection.
///
/// The tests above cover admission as a pure function and the dispatcher against a bare state.
/// Neither would catch the failures that only appear once bytes move: a hello the server writes
/// and a client cannot parse, a stream-kind byte that puts the frame off by one, or an rkyv buffer
/// that arrives unaligned. This exercises handshake, hello, request and typed response.
#[cfg(test)]
#[path = "quic.live.test.rs"]
mod live;
#[cfg(test)]
#[path = "quic.test.rs"]
mod tests;
