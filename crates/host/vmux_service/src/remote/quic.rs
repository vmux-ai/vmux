//! The only way a phone reaches this Mac.
//!
//! Two controls that came free from HTTP middleware had to be rebuilt, because a QUIC connection
//! is long-lived where a request is not:
//!
//! - **The Remote kill switch.** Re-reading the state file per request meant turning Remote off
//!   took effect immediately. A connection that authenticated once would outlive the switch, so a
//!   single watcher closes every live peer instead. That same watcher is what [`Supervisor`] runs
//!   the dialer from, so the switch decides whether this desktop is reachable at all rather than
//!   only who is let in once it is.
//! - **Request limits.** `receive_window` bounds what a peer can buffer before the dispatcher's own
//!   size check runs — the job a request body limit used to do.

pub mod dispatch;

pub(crate) mod dialer;
mod supervisor;

pub(crate) use supervisor::Supervisor;

use std::time::Duration;

use tokio::sync::watch;

use vmux_remote::quic::endpoint::{RECEIVE_WINDOW, SelfSignedIdentity};

use vmux_wire::protocol::{ServiceMessage, SharedMessage};

use vmux_remote::framing::{Frame, FrameError, FrameStream};
use vmux_remote::quic::{Accepted, ClientSetup, CloseCode, MessageType};

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
    let remote = crate::RemotePaths::current();
    let cert_path = remote.certificate();
    let key_path = remote.key();

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
    let path = crate::RemotePaths::current().fingerprint();
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
    let pem = std::fs::read_to_string(crate::RemotePaths::current().certificate()).ok()?;
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
    /// The peer frames its streams in a layout this build cannot read.
    ///
    /// Distinct from [`Rejection::Malformed`] because it names a build that is merely old rather
    /// than one sending rubbish, and only one of those is worth a bug report.
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

/// Decide whether a hello is acceptable, given the token and the current kill-switch state.
///
/// Separated from the I/O so the decision is testable without a socket, and so the order of the
/// checks is visible: the kill switch first, then the secret. A peer learns nothing about the
/// token from a `RemoteDisabled` answer.
pub fn admit(
    presented_token: &str,
    expected_token: &str,
    remote_enabled: bool,
) -> Result<Accepted, Rejection> {
    if !remote_enabled {
        return Err(Rejection::RemoteDisabled);
    }
    if !super::server::secure_eq(presented_token, expected_token) {
        return Err(Rejection::Unauthorized);
    }
    Ok(Accepted {})
}

/// Watches the Remote kill switch, for everything whose lifetime it decides.
///
/// [`Supervisor`] starts and stops the relay dialer from this, and every live connection closes
/// itself when it goes off. One task and one file read per second for the whole daemon, against
/// the HTTP path's read per request plus a per-stream ticker.
///
/// Polling also settles the order the daemon and the desktop start in: whichever writes the switch
/// second, the next tick reconciles to it.
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

/// Frames on the setup exchange, bounded so an unauthenticated peer cannot buffer without limit.
const SETUP: FrameStream = FrameStream::new(MAX_HELLO_BYTES);

/// Frames on a control or subscription stream, once the peer has authenticated.
const CONTROL: FrameStream = FrameStream::new(MAX_REQUEST_BYTES);

/// Read one setup frame, refusing anything that is not one.
///
/// A frame addressed to another leg is turned away here rather than decoded: a relay setup
/// satisfies this message field for field, so the type has to be what decides.
pub async fn read_setup(stream: &mut quinn::RecvStream) -> Result<ClientSetup, Rejection> {
    match SETUP.accept(stream).await {
        Ok(frame) => frame
            .read_json::<ClientSetup>(MessageType::CLIENT_SETUP)
            .map_err(Rejection::from),
        Err(error) => Err(Rejection::from(error)),
    }
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
    let admitted = match read_setup(&mut recv).await {
        Ok(setup) => admit(&setup.token, &token, *liveness.borrow()),
        Err(rejection) => Err(rejection),
    };

    let accepted = match admitted {
        Ok(accepted) => accepted,
        Err(rejection) => {
            tracing::info!(?rejection, "remote quic: connection refused");
            connection.close(rejection.close_code().as_u32().into(), b"refused");
            return;
        }
    };

    let Ok(frame) = Frame::json(MessageType::SESSION_ACCEPTED, &accepted) else {
        connection.close(CloseCode::ProtocolError.as_u32().into(), b"setup");
        return;
    };
    if SETUP.open(&mut send, &frame).await.is_err() || send.finish().is_err() {
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
/// Above the prompt cap so an oversized prompt is refused by the dispatcher with a reason rather
/// than looking to the client like a broken connection — and well below the connection's window,
/// because nothing a request carries is large. Attachments travel as paths validated against
/// `$HOME`, never as bytes, so the biggest legitimate request is one full prompt and a handful of
/// file names.
///
/// Derived rather than written out, because the invariant is the *relationship*: this used to be
/// exactly [`RECEIVE_WINDOW`], so one max-size request could take the whole connection's
/// allowance and stall the other sixty-three streams behind it.
const MAX_REQUEST_BYTES: usize = (RECEIVE_WINDOW / 8) as usize;

/// One request in, one response out, then the stream closes.
///
/// The stream kind leads so the peer can route without decoding, and so a client that opens the
/// wrong kind is told rather than left waiting.
async fn dispatch_control(
    state: super::server::RemoteState,
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
) {
    let Ok(frame) = CONTROL.accept(&mut recv).await else {
        return;
    };
    // The type decides before the body is touched. Handing another leg's payload to a decoder and
    // rejecting it afterwards would leave the type field decorative — the decoder would already
    // have run on bytes this handler was never addressed by.
    match frame.message_type {
        MessageType::CONTROL_REQUEST | MessageType::SESSION_EVENTS => {}
        unserved => {
            tracing::debug!(
                ?unserved,
                "remote quic: stream refused, unserved message type"
            );
            return;
        }
    }

    // Owned by the frame, so rkyv sees an aligned buffer; the old shape sliced past a leading
    // byte, which is not aligned.
    let Ok(request) = rkyv::from_bytes::<SharedMessage, rkyv::rancor::Error>(&frame.body) else {
        return;
    };

    if frame.message_type == MessageType::SESSION_EVENTS {
        stream_session_events(&state, send, request).await;
        return;
    }

    let response = dispatch::dispatch(&state, request).await;
    let Ok(encoded) = rkyv::to_bytes::<rkyv::rancor::Error>(&response) else {
        return;
    };
    let frame = Frame::new(MessageType::CONTROL_RESPONSE, encoded.to_vec());
    if CONTROL.open(&mut send, &frame).await.is_ok() {
        let _ = send.finish();
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
    // The stream announces its version once, on whichever event goes out first.
    let mut opened = false;

    // Snapshot first, so a client that attaches mid-conversation renders the transcript it
    // missed rather than only what happens next.
    if let Some(snapshot) = session_snapshot(state, &sid).await
        && write_event(&mut send, &snapshot, &mut opened)
            .await
            .is_err()
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
                if write_event(&mut send, &event, &mut opened).await.is_err() {
                    return;
                }
            }
            // Lagged means the client fell behind and frames were dropped. Resending a snapshot
            // is what the HTTP path did, and it beats a gap the client cannot detect.
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                let Some(snapshot) = session_snapshot(state, &sid).await else {
                    return;
                };
                if write_event(&mut send, &snapshot, &mut opened)
                    .await
                    .is_err()
                {
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

/// One event, framed like everything else.
///
/// `opened` says whether this half of the stream has announced its version yet: a subscription
/// sends many of these, and the version belongs to the stream rather than to each message.
async fn write_event(
    send: &mut quinn::SendStream,
    event: &vmux_wire::protocol::SharedEvent,
    opened: &mut bool,
) -> Result<(), ()> {
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(event).map_err(|_| ())?;
    let frame = Frame::new(MessageType::SESSION_EVENT, bytes.to_vec());
    let written = if *opened {
        CONTROL.send(send, &frame).await
    } else {
        *opened = true;
        CONTROL.open(send, &frame).await
    };
    written.map_err(|_| ())
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
mod live {
    use super::*;
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use tokio::sync::{Mutex, broadcast};
    use vmux_remote::DeviceId;
    use vmux_remote::quic::endpoint::{SelfSignedIdentity, Trust};
    use vmux_wire::protocol::SharedResponse;

    struct Harness {
        address: std::net::SocketAddr,
        fingerprint: String,
    }

    fn start(token: &str) -> Harness {
        let (agent_tx, _) = broadcast::channel(8);
        let state = super::super::server::RemoteState {
            token: Arc::from(token),
            paired: Arc::new(AtomicBool::new(false)),
            agents: Arc::new(Mutex::new(Default::default())),
            acp: Arc::new(Mutex::new(Default::default())),
            broker: crate::agent_broker::AgentBroker::new(
                agent_tx,
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            client_ops: Arc::new(Mutex::new(Default::default())),
        };
        // A throwaway certificate, so the test never writes into the user's profile directory.
        let identity = SelfSignedIdentity::generate(vec!["localhost".into()]).expect("identity");
        let fingerprint = identity.fingerprint.clone();
        // Liveness is injected rather than read from disk: a test must not be able to flip the
        // user's real Remote setting, and leaking that write would do exactly that.
        let (_liveness_tx, liveness_rx) = tokio::sync::watch::channel(true);
        std::mem::forget(_liveness_tx);
        let (handle, address) = spawn_with_identity(
            state,
            (Ipv4Addr::LOCALHOST, 0).into(),
            identity,
            liveness_rx,
        )
        .expect("listener");
        // Kept alive for the process; each test binds its own ephemeral port.
        std::mem::forget(handle);
        Harness {
            address,
            fingerprint,
        }
    }

    async fn connect(
        harness: &Harness,
        token: &str,
    ) -> Result<quinn::Connection, quinn::ConnectionError> {
        let endpoint = Trust::Desktop {
            fingerprint: harness.fingerprint.clone(),
        }
        .endpoint(harness.address)
        .expect("client endpoint");
        let connection = endpoint
            .connect(harness.address, "localhost")
            .expect("dial")
            .await?;

        let (mut send, mut recv) = connection.open_bi().await.expect("setup stream");
        let setup = ClientSetup {
            device_id: DeviceId::new("test-device"),
            token: token.to_string(),
        };
        let frame = Frame::json(MessageType::CLIENT_SETUP, &setup).expect("encode");
        SETUP.open(&mut send, &frame).await.expect("write setup");
        send.finish().expect("finish setup");

        match SETUP.accept(&mut recv).await {
            Ok(frame) => {
                frame
                    .read_json::<Accepted>(MessageType::SESSION_ACCEPTED)
                    .expect("accepted");
                Ok(connection)
            }
            Err(_) => Err(connection
                .close_reason()
                .unwrap_or(quinn::ConnectionError::LocallyClosed)),
        }
    }

    async fn request(connection: &quinn::Connection, message: SharedMessage) -> SharedResponse {
        let (mut send, mut recv) = connection.open_bi().await.expect("control stream");
        let body = rkyv::to_bytes::<rkyv::rancor::Error>(&message).expect("encode");
        let frame = Frame::new(MessageType::CONTROL_REQUEST, body.to_vec());
        CONTROL
            .open(&mut send, &frame)
            .await
            .expect("write request");
        send.finish().expect("finish request");

        let answer = CONTROL.accept(&mut recv).await.expect("response");
        let body = answer
            .body_of(MessageType::CONTROL_RESPONSE)
            .expect("a control stream answers with a control response");
        rkyv::from_bytes::<SharedResponse, rkyv::rancor::Error>(body).expect("decode")
    }

    /// Read the first event off a subscription stream, version byte and all.
    async fn read_event(recv: &mut quinn::RecvStream) -> Option<vmux_wire::protocol::SharedEvent> {
        let frame = CONTROL.accept(recv).await.ok()?;
        let body = frame.body_of(MessageType::SESSION_EVENT).ok()?;
        rkyv::from_bytes::<vmux_wire::protocol::SharedEvent, rkyv::rancor::Error>(body).ok()
    }

    /// Subscribing to a session that does not exist must close the stream rather than hang.
    ///
    /// A client waiting forever on a dead subscription is the failure mode that looks like a
    /// network problem and is not, so it is worth pinning down.
    #[tokio::test]
    async fn subscribing_to_an_unknown_session_closes_rather_than_hangs() {
        let harness = start("correct-token");
        let connection = connect(&harness, "correct-token").await.expect("handshake");

        let (mut send, mut recv) = connection.open_bi().await.expect("stream");
        let body = rkyv::to_bytes::<rkyv::rancor::Error>(&SharedMessage::agent(
            "ghost",
            vmux_wire::protocol::AgentAction::Attach,
        ))
        .expect("encode");
        CONTROL
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
    }

    /// A well-formed frame carrying a type this handler does not serve must not be answered as if
    /// it were a control request. The message type is the only thing separating a request from a
    /// subscription, or from another leg's traffic entirely — which is why it is on the wire.
    ///
    /// The body is a perfectly good `ListSessions`, so nothing but the type can refuse it.
    #[tokio::test]
    async fn a_frame_of_a_foreign_type_is_not_answered_as_a_control_request() {
        let harness = start("correct-token");
        let connection = connect(&harness, "correct-token").await.expect("handshake");

        let (mut send, mut recv) = connection.open_bi().await.expect("stream");
        let body = rkyv::to_bytes::<rkyv::rancor::Error>(&SharedMessage::ListSessions)
            .expect("encode")
            .to_vec();
        CONTROL
            .open(&mut send, &Frame::new(MessageType(0x0999), body))
            .await
            .expect("write");
        send.finish().expect("finish");

        let answered = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            recv.read_to_end(1024 * 1024),
        )
        .await;

        let served = matches!(&answered, Ok(Ok(bytes)) if !bytes.is_empty());
        assert!(
            !served,
            "a foreign message type must not be served as a control request, got {answered:?}"
        );
        assert!(answered.is_ok(), "and it must not hang, got {answered:?}");
    }

    #[tokio::test]
    async fn a_paired_client_can_list_sessions() {
        let harness = start("correct-token");

        let connection = connect(&harness, "correct-token")
            .await
            .expect("handshake should succeed");
        let response = request(&connection, SharedMessage::ListSessions).await;

        // No sessions running, so the list is empty — but it is a *typed* empty list, which is
        // the point: the frame round-tripped through rkyv in both directions.
        assert!(
            matches!(response, SharedResponse::Sessions(ref sessions) if sessions.is_empty()),
            "expected a typed session list, got {response:?}"
        );
    }

    /// The token is the whole access control, so a wrong one must not merely fail — it must close
    /// with a code the client can act on.
    #[tokio::test]
    async fn a_wrong_token_is_closed_with_unauthorized() {
        let harness = start("correct-token");

        match connect(&harness, "wrong-token").await {
            Err(quinn::ConnectionError::ApplicationClosed(closed)) => assert_eq!(
                closed.error_code.into_inner(),
                CloseCode::Unauthorized.as_u32() as u64,
                "a bad token must close with Unauthorized, not a generic error"
            ),
            other => panic!("expected an Unauthorized close, got {other:?}"),
        }
    }

    /// Requests are independent streams on one connection, so a second costs no handshake. Were
    /// they ever serialised onto a single stream this would hang rather than merely slow down.
    #[tokio::test]
    async fn one_connection_serves_repeated_requests() {
        let harness = start("correct-token");

        let connection = connect(&harness, "correct-token").await.expect("handshake");

        for _ in 0..3 {
            let response = request(&connection, SharedMessage::ListSessions).await;
            assert!(matches!(response, SharedResponse::Sessions(_)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matching_token_is_admitted() {
        assert_eq!(admit("secret", "secret", true), Ok(Accepted {}));
    }

    #[test]
    fn a_wrong_token_is_refused() {
        assert_eq!(admit("guess", "secret", true), Err(Rejection::Unauthorized));
    }

    /// The kill switch outranks the secret, so flipping Remote off refuses even a correctly
    /// paired phone.
    #[test]
    fn remote_switched_off_outranks_a_valid_token() {
        assert_eq!(
            admit("secret", "secret", false),
            Err(Rejection::RemoteDisabled)
        );
    }

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
