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
use vmux_wire::protocol::SharedMessage;

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
    if StreamKind::from_byte(*kind) != Some(StreamKind::Control) {
        return;
    }
    // Copied so rkyv sees an aligned buffer; a slice at a one-byte offset is not.
    let body = body.to_vec();
    let Ok(request) = rkyv::from_bytes::<SharedMessage, rkyv::rancor::Error>(&body) else {
        return;
    };

    let response = dispatch::dispatch(&state, request).await;
    let Ok(encoded) = rkyv::to_bytes::<rkyv::rancor::Error>(&response) else {
        return;
    };
    if send.write_all(&encoded).await.is_ok() {
        let _ = send.finish();
    }
}

/// Bind the listener and serve until the endpoint closes.
pub(crate) fn spawn(
    state: super::server::RemoteState,
    address: SocketAddr,
) -> std::io::Result<tokio::task::JoinHandle<()>> {
    let identity = ensure_identity()?;
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

    let liveness = spawn_liveness_watch();
    Ok(tokio::spawn(async move {
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
    }))
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
