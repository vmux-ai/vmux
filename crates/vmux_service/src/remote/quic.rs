//! The QUIC listener, running beside the HTTP server until the cutover.
//!
//! The controls the HTTP path gets from axum middleware have to be rebuilt here, because a QUIC
//! connection is long-lived where a request is not. Two in particular do not survive a naive port:
//!
//! - **The Remote kill switch.** `authorize` re-reads the state file on every request, so turning
//!   Remote off takes effect immediately. A connection that authenticated once would otherwise
//!   outlive the switch, so a single watcher closes every live peer instead.
//! - **Request limits.** `receive_window` bounds what a peer can buffer before the dispatcher's own
//!   size check runs, which on HTTP was the body limit doing that job.

pub mod dispatch;

use std::net::SocketAddr;
use std::time::Duration;

use tokio::sync::watch;
use vmux_remote::quic::endpoint::{SelfSignedIdentity, generate_self_signed, server_endpoint};
use vmux_wire::protocol::{ServiceMessage, SharedMessage};

use vmux_remote::quic::{
    ClientHello, CloseCode, ProtocolVersion, ServerHello, StreamKind, decode_hello, encode_hello,
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
        let fingerprint = fingerprint_of(&certificate_pem)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        return Ok(SelfSignedIdentity {
            certificate_pem,
            private_key_pem,
            fingerprint,
        });
    }

    let identity = generate_self_signed(subject_alt_names())
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cert_path, &identity.certificate_pem)?;
    write_private(&key_path, &identity.private_key_pem)?;
    Ok(identity)
}

/// The fingerprint of the certificate this desktop presents, for the pairing link.
///
/// Reads the persisted identity rather than the live listener, so the GUI can build a pairing
/// link without reaching into the daemon's process.
pub fn identity_fingerprint() -> Option<String> {
    let pem = std::fs::read_to_string(crate::remote_cert_path()).ok()?;
    fingerprint_of(&pem).ok()
}

/// The names a phone might dial this desktop by. The certificate is pinned by fingerprint, so
/// these are belt-and-braces rather than the trust decision.
fn subject_alt_names() -> Vec<String> {
    vec!["localhost".to_string(), "127.0.0.1".to_string()]
}

fn fingerprint_of(certificate_pem: &str) -> Result<String, String> {
    let certificate = rustls_pemfile::certs(&mut certificate_pem.as_bytes())
        .next()
        .ok_or_else(|| "certificate file held no certificate".to_string())?
        .map_err(|error| format!("certificate parse failed: {error}"))?;
    Ok(vmux_remote::quic::endpoint::certificate_fingerprint(
        &certificate,
    ))
}

/// Write at `0600` from the start rather than narrowing after, so the key is never briefly
/// readable by other local users.
fn write_private(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path);
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents.as_bytes())
    }
    #[cfg(not(unix))]
    std::fs::write(path, contents)
}

/// Why a connection was turned away, so the peer is told rather than merely dropped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rejection {
    UnsupportedVersion,
    Unauthorized,
    RemoteDisabled,
    Malformed,
}

impl Rejection {
    pub fn close_code(self) -> CloseCode {
        match self {
            Self::UnsupportedVersion => CloseCode::UnsupportedVersion,
            Self::Unauthorized => CloseCode::Unauthorized,
            Self::RemoteDisabled => CloseCode::RemoteDisabled,
            Self::Malformed => CloseCode::ProtocolError,
        }
    }
}

/// Decide whether a hello is acceptable, given the token and the current kill-switch state.
///
/// Separated from the I/O so the decision is testable without a socket, and so the order of the
/// checks is visible: liveness first, then version, then the secret. A peer learns nothing about
/// the token from a `RemoteDisabled` or `UnsupportedVersion` answer.
pub fn admit(
    hello: &ClientHello,
    presented_token: &str,
    expected_token: &str,
    remote_enabled: bool,
) -> Result<ServerHello, Rejection> {
    if !remote_enabled {
        return Err(Rejection::RemoteDisabled);
    }
    if !hello.protocol_version.is_supported() {
        return Err(Rejection::UnsupportedVersion);
    }
    if !super::server::secure_eq(presented_token, expected_token) {
        return Err(Rejection::Unauthorized);
    }
    Ok(ServerHello {
        protocol_version: ProtocolVersion::CURRENT,
        capabilities: Vec::new(),
    })
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
        Ok(authenticated) => admit(
            &authenticated.hello,
            &authenticated.token,
            &token,
            *liveness.borrow(),
        ),
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
    let SharedMessage::AttachPageAgent { sid } = request else {
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

/// Bind the listener and serve until the endpoint closes.
pub(crate) fn spawn(
    state: super::server::RemoteState,
    address: SocketAddr,
) -> std::io::Result<tokio::task::JoinHandle<()>> {
    let liveness = spawn_liveness_watch();
    spawn_with_identity(state, address, ensure_identity()?, liveness).map(|(handle, _)| handle)
}

/// Bind with a caller-supplied identity.
///
/// Split out so a test can use a throwaway certificate rather than writing one into the user's
/// profile directory, which `ensure_identity` would otherwise do as a side effect.
pub(crate) fn spawn_with_identity(
    state: super::server::RemoteState,
    address: SocketAddr,
    identity: SelfSignedIdentity,
    liveness: watch::Receiver<bool>,
) -> std::io::Result<(tokio::task::JoinHandle<()>, SocketAddr)> {
    let endpoint = server_endpoint(
        address,
        &identity.certificate_pem,
        &identity.private_key_pem,
    )
    .map_err(std::io::Error::other)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_remote::DeviceId;

    fn hello(version: u32) -> ClientHello {
        ClientHello {
            protocol_version: ProtocolVersion(version),
            device_id: DeviceId::new("device"),
            capabilities: Vec::new(),
            resume_from: None,
        }
    }

    #[test]
    fn a_matching_token_on_a_supported_version_is_admitted() {
        let accepted = admit(&hello(ProtocolVersion::CURRENT.0), "secret", "secret", true);

        assert_eq!(
            accepted.map(|server| server.protocol_version),
            Ok(ProtocolVersion::CURRENT)
        );
    }

    #[test]
    fn a_wrong_token_is_refused() {
        assert_eq!(
            admit(&hello(ProtocolVersion::CURRENT.0), "guess", "secret", true),
            Err(Rejection::Unauthorized)
        );
    }

    /// The kill switch outranks the secret, so flipping Remote off refuses even a correctly
    /// paired phone.
    #[test]
    fn remote_switched_off_outranks_a_valid_token() {
        assert_eq!(
            admit(
                &hello(ProtocolVersion::CURRENT.0),
                "secret",
                "secret",
                false
            ),
            Err(Rejection::RemoteDisabled)
        );
    }

    /// Checked before the token, so a peer cannot use the difference between "wrong version" and
    /// "wrong token" to learn anything about the secret.
    #[test]
    fn an_unsupported_version_is_refused_without_consulting_the_token() {
        let far_future = ProtocolVersion::CURRENT.0 + 99;

        assert_eq!(
            admit(&hello(far_future), "guess", "secret", true),
            Err(Rejection::UnsupportedVersion)
        );
    }

    #[test]
    fn each_rejection_carries_a_distinct_close_code() {
        let codes = [
            Rejection::UnsupportedVersion,
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
    use vmux_remote::quic::endpoint::{client_endpoint, generate_self_signed};
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
        let identity = generate_self_signed(vec!["localhost".into()]).expect("identity");
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
        let endpoint = client_endpoint(&harness.fingerprint).expect("client endpoint");
        let connection = endpoint
            .connect(harness.address, "localhost")
            .expect("dial")
            .await?;

        let (mut send, mut recv) = connection.open_bi().await.expect("hello stream");
        let hello = AuthenticatedHello {
            hello: ClientHello {
                protocol_version: ProtocolVersion::CURRENT,
                device_id: DeviceId::new("test-device"),
                capabilities: Vec::new(),
                resume_from: None,
            },
            token: token.to_string(),
        };
        send.write_all(&encode_hello(&hello).expect("encode"))
            .await
            .expect("write hello");
        send.finish().expect("finish hello");

        match recv.read_to_end(64 * 1024).await {
            Ok(bytes) => {
                decode_hello::<vmux_remote::quic::ServerHello>(&bytes).expect("server hello");
                Ok(connection)
            }
            Err(_) => Err(connection
                .close_reason()
                .unwrap_or(quinn::ConnectionError::LocallyClosed)),
        }
    }

    async fn request(connection: &quinn::Connection, message: SharedMessage) -> SharedResponse {
        let (mut send, mut recv) = connection.open_bi().await.expect("control stream");
        let mut frame = vec![StreamKind::Control.as_byte()];
        frame.extend_from_slice(&rkyv::to_bytes::<rkyv::rancor::Error>(&message).expect("encode"));
        send.write_all(&frame).await.expect("write request");
        send.finish().expect("finish request");

        let bytes = recv.read_to_end(8 * 1024 * 1024).await.expect("response");
        // Copied so rkyv sees an aligned buffer, for the same reason the production readers do.
        let bytes = bytes.to_vec();
        rkyv::from_bytes::<SharedResponse, rkyv::rancor::Error>(&bytes).expect("decode")
    }

    /// Read one length-prefixed event off a subscription stream.
    async fn read_event(recv: &mut quinn::RecvStream) -> Option<vmux_wire::protocol::SharedEvent> {
        let mut length = [0u8; 4];
        recv.read_exact(&mut length).await.ok()?;
        let mut body = vec![0u8; u32::from_le_bytes(length) as usize];
        recv.read_exact(&mut body).await.ok()?;
        rkyv::from_bytes::<vmux_wire::protocol::SharedEvent, rkyv::rancor::Error>(&body).ok()
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
        let mut frame = vec![StreamKind::SessionEvents.as_byte()];
        frame.extend_from_slice(
            &rkyv::to_bytes::<rkyv::rancor::Error>(&SharedMessage::AttachPageAgent {
                sid: "ghost".into(),
            })
            .expect("encode"),
        );
        send.write_all(&frame).await.expect("write");
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

    /// A control request on a subscription stream, or the reverse, must not be silently treated
    /// as the other. The kind byte is the only thing distinguishing them.
    #[tokio::test]
    async fn an_unknown_stream_kind_is_dropped() {
        let harness = start("correct-token");
        let connection = connect(&harness, "correct-token").await.expect("handshake");

        let (mut send, mut recv) = connection.open_bi().await.expect("stream");
        let mut frame = vec![200u8];
        frame.extend_from_slice(
            &rkyv::to_bytes::<rkyv::rancor::Error>(&SharedMessage::ListSessions).expect("encode"),
        );
        send.write_all(&frame).await.expect("write");
        send.finish().expect("finish");

        let answered = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            recv.read_to_end(1024 * 1024),
        )
        .await;

        assert!(
            matches!(answered, Ok(Ok(bytes)) if bytes.is_empty()),
            "an unrecognised stream kind must not be answered as if it were a control request"
        );
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
