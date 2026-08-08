//! The phone's side of the QUIC link.
//!
//! Presents the same surface `Api` did over HTTP, so the pages calling it do not care which
//! transport is underneath. Three things genuinely change:
//!
//! - **One connection, many streams.** HTTP opened a socket per request; here a request is a
//!   stream on a connection that is already warm, so a prompt costs no handshake.
//! - **Trust is pinned.** The old client called `danger_accept_invalid_certs(true)` for private
//!   addresses, accepting any certificate on the LAN. This accepts exactly the one the pairing
//!   link recorded.
//! - **Failures are typed.** HTTP gave back a status code; `SharedFailure` distinguishes a session
//!   that will never exist from a desktop that merely has no window open yet.

use std::sync::Arc;

use tokio::sync::Mutex;
use vmux_remote::DeviceId;
use vmux_remote::quic::endpoint::Trust;
use vmux_remote::quic::{
    Capability, ClientHello, CloseCode, ProtocolVersion, ServerHello, StreamKind, decode_hello,
    encode_hello,
};
use vmux_wire::protocol::{SharedEvent, SharedFailure, SharedMessage, SharedResponse};

/// Matches the daemon's cap on a control response.
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// What went wrong, in the terms a page needs to decide whether to retry.
#[derive(Debug)]
pub enum QuicError {
    /// Token rejected, or the desktop presented a certificate we are not paired with. Both mean
    /// re-pair; neither is fixed by waiting.
    Unauthorized,
    /// This build speaks a protocol the desktop will not serve, or the reverse.
    VersionMismatch,
    /// Remote is switched off on the desktop. Resolves when the user turns it back on.
    RemoteDisabled,
    /// The desktop answered, and the answer was a refusal.
    Refused(SharedFailure),
    /// The link itself failed. Worth retrying.
    Transport(String),
}

impl std::fmt::Display for QuicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => f.write_str("Pairing expired. Scan the QR on your Mac again."),
            Self::VersionMismatch => f.write_str("Update Vmux Remote to connect to this Mac."),
            Self::RemoteDisabled => f.write_str("Remote is switched off on your Mac."),
            Self::Refused(SharedFailure::NotFound) => f.write_str("That session is gone."),
            Self::Refused(SharedFailure::NoDesktop) => {
                f.write_str("Open the Vmux window on your Mac.")
            }
            Self::Refused(_) => f.write_str("Your Mac could not do that."),
            Self::Transport(message) => f.write_str(message),
        }
    }
}

/// Where and how to reach one paired desktop.
#[derive(Clone, Debug, PartialEq)]
pub struct Endpoint {
    pub address: String,
    pub token: String,
    /// SHA-256 of the desktop's certificate, from the pairing link.
    pub fingerprint: String,
    pub device_id: DeviceId,
}

/// A connection, reconnected on demand.
///
/// Held behind a mutex because a QUIC connection is shared by every page: opening a second one
/// per caller would pay a handshake each time and defeat the point.
#[derive(Clone)]
pub struct QuicApi {
    endpoint: Endpoint,
    connection: Arc<Mutex<Option<quinn::Connection>>>,
}

impl QuicApi {
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            connection: Arc::new(Mutex::new(None)),
        }
    }

    /// Drop the connection so the next call redials.
    ///
    /// Called when the app foregrounds: iOS tears the UDP socket down while suspended without
    /// telling anyone, so a connection that looks alive after a resume usually is not.
    pub async fn reset(&self) {
        if let Some(connection) = self.connection.lock().await.take() {
            connection.close(CloseCode::Normal.as_u32().into(), b"suspended");
        }
    }

    async fn connected(&self) -> Result<quinn::Connection, QuicError> {
        let mut slot = self.connection.lock().await;
        if let Some(existing) = slot.as_ref()
            && existing.close_reason().is_none()
        {
            return Ok(existing.clone());
        }
        let connection = self.dial().await?;
        *slot = Some(connection.clone());
        Ok(connection)
    }

    /// How long a dial may take before it is reported rather than waited on.
    ///
    /// Without this an unreachable desktop is an infinite spinner: quinn's own idle timeout does
    /// not apply until a connection exists, so nothing ever completes and nothing ever fails.
    const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

    async fn dial(&self) -> Result<quinn::Connection, QuicError> {
        match tokio::time::timeout(Self::DIAL_TIMEOUT, self.dial_inner()).await {
            Ok(result) => result,
            Err(_) => Err(QuicError::Transport(format!(
                "No answer from {} after {}s.",
                self.endpoint.address,
                Self::DIAL_TIMEOUT.as_secs()
            ))),
        }
    }

    async fn dial_inner(&self) -> Result<quinn::Connection, QuicError> {
        // The pairing link names the relay by host, so this has to resolve rather than parse: a
        // hostname is not a SocketAddr, and the old parse turned every relay pairing into "that
        // pairing address is not valid".
        let (host, port) = self
            .endpoint
            .address
            .rsplit_once(':')
            .ok_or_else(|| QuicError::Transport("That pairing address is not valid.".into()))?;
        let port: u16 = port
            .parse()
            .map_err(|_| QuicError::Transport("That pairing address has no port.".into()))?;
        let address = vmux_remote::quic::endpoint::resolve_preferring_ipv4(host, port)
            .await
            .map_err(QuicError::Transport)?;

        // The certificate is pinned by fingerprint and the verifier ignores the name, but the
        // relay may still route on SNI, so send the real host rather than a placeholder.
        let server_name = self
            .endpoint
            .address
            .rsplit_once(':')
            .map(|(host, _)| host)
            .unwrap_or(&self.endpoint.address)
            .to_string();
        let endpoint = Trust::Desktop {
            fingerprint: self.endpoint.fingerprint.clone(),
        }
        .endpoint(address)
        .map_err(QuicError::Transport)?;
        let connection = endpoint
            .connect(address, &server_name)
            .map_err(|error| QuicError::Transport(error.to_string()))?
            .await
            .map_err(|error| classify_connection_error(&error))?;

        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        let hello = AuthenticatedHello {
            hello: ClientHello {
                protocol_version: ProtocolVersion::CURRENT,
                device_id: self.endpoint.device_id.clone(),
                capabilities: vec![Capability::InlineMedia],
                // Reserved. Snapshots are refetched on reconnect until the desktop can replay.
                resume_from: None,
            },
            token: self.endpoint.token.clone(),
        };
        let bytes =
            encode_hello(&hello).map_err(|error| QuicError::Transport(error.to_string()))?;
        send.write_all(&bytes)
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        send.finish()
            .map_err(|error| QuicError::Transport(error.to_string()))?;

        let answer = recv
            .read_to_end(64 * 1024)
            .await
            .map_err(|_| classify_close(&connection))?;
        decode_hello::<ServerHello>(&answer).map_err(|_| classify_close(&connection))?;
        Ok(connection)
    }

    /// Subscribe to a session's events.
    ///
    /// The client opens this stream and writes once; everything after flows back. That direction
    /// is not cosmetic — the relay only routes streams the client opens, so a desktop-initiated
    /// stream would work on a direct connection and disappear through the relay.
    pub async fn subscribe(&self, sid: &str) -> Result<Subscription, QuicError> {
        let connection = self.connected().await?;
        let (mut send, recv) = connection
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;

        let request = SharedMessage::AttachPageAgent {
            sid: sid.to_string(),
        };
        let mut frame = vec![StreamKind::SessionEvents.as_byte()];
        frame.extend_from_slice(
            &rkyv::to_bytes::<rkyv::rancor::Error>(&request)
                .map_err(|error| QuicError::Transport(error.to_string()))?,
        );
        send.write_all(&frame)
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        send.finish()
            .map_err(|error| QuicError::Transport(error.to_string()))?;

        Ok(Subscription { recv })
    }

    /// One request, one response, on its own stream.
    pub async fn request(&self, message: SharedMessage) -> Result<SharedResponse, QuicError> {
        let connection = self.connected().await?;
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;

        let mut frame = vec![StreamKind::Control.as_byte()];
        frame.extend_from_slice(
            &rkyv::to_bytes::<rkyv::rancor::Error>(&message)
                .map_err(|error| QuicError::Transport(error.to_string()))?,
        );
        send.write_all(&frame)
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        send.finish()
            .map_err(|error| QuicError::Transport(error.to_string()))?;

        let bytes = recv
            .read_to_end(MAX_RESPONSE_BYTES)
            .await
            .map_err(|_| classify_close(&connection))?;
        // Copied so rkyv sees an aligned buffer.
        let bytes = bytes.to_vec();
        let response = rkyv::from_bytes::<SharedResponse, rkyv::rancor::Error>(&bytes)
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        match response {
            SharedResponse::Failed(failure) => Err(QuicError::Refused(failure)),
            other => Ok(other),
        }
    }
}

/// A live session subscription. Yields events until the desktop closes the stream.
pub struct Subscription {
    recv: quinn::RecvStream,
}

impl Subscription {
    /// The next event, or `None` once the desktop is done sending.
    ///
    /// Events are length-prefixed because many share one stream; a control response is not,
    /// because it is the only thing on its own.
    pub async fn next(&mut self) -> Option<SharedEvent> {
        let mut length = [0u8; 4];
        self.recv.read_exact(&mut length).await.ok()?;
        let length = u32::from_le_bytes(length) as usize;
        if length > MAX_RESPONSE_BYTES {
            return None;
        }
        let mut body = vec![0u8; length];
        self.recv.read_exact(&mut body).await.ok()?;
        rkyv::from_bytes::<SharedEvent, rkyv::rancor::Error>(&body).ok()
    }
}

/// The hello carries the bearer token, since QUIC has no headers.
#[derive(serde::Deserialize, serde::Serialize)]
struct AuthenticatedHello {
    #[serde(flatten)]
    hello: ClientHello,
    token: String,
}

/// A refused handshake is almost always the pinning, and saying so beats a TLS error string.
fn classify_connection_error(error: &quinn::ConnectionError) -> QuicError {
    match error {
        quinn::ConnectionError::ApplicationClosed(closed) => {
            close_code_to_error(closed.error_code.into_inner())
        }
        quinn::ConnectionError::TransportError(_) => QuicError::Unauthorized,
        other => QuicError::Transport(other.to_string()),
    }
}

/// The desktop closes with a code rather than just dropping, so a client can tell the user
/// something specific instead of "connection lost".
fn classify_close(connection: &quinn::Connection) -> QuicError {
    match connection.close_reason() {
        Some(quinn::ConnectionError::ApplicationClosed(closed)) => {
            close_code_to_error(closed.error_code.into_inner())
        }
        Some(other) => QuicError::Transport(other.to_string()),
        None => QuicError::Transport("The connection to your Mac dropped.".into()),
    }
}

fn close_code_to_error(code: u64) -> QuicError {
    if code == CloseCode::Unauthorized.as_u32() as u64 {
        QuicError::Unauthorized
    } else if code == CloseCode::UnsupportedVersion.as_u32() as u64 {
        QuicError::VersionMismatch
    } else if code == CloseCode::RemoteDisabled.as_u32() as u64 {
        QuicError::RemoteDisabled
    } else {
        QuicError::Transport("Your Mac closed the connection.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mapping is what turns a close code into something a user can act on, so each one has
    /// to stay distinct — collapsing two would send someone to re-pair over a version mismatch.
    #[test]
    fn every_close_code_maps_to_its_own_error() {
        assert!(matches!(
            close_code_to_error(CloseCode::Unauthorized.as_u32() as u64),
            QuicError::Unauthorized
        ));
        assert!(matches!(
            close_code_to_error(CloseCode::UnsupportedVersion.as_u32() as u64),
            QuicError::VersionMismatch
        ));
        assert!(matches!(
            close_code_to_error(CloseCode::RemoteDisabled.as_u32() as u64),
            QuicError::RemoteDisabled
        ));
        assert!(matches!(close_code_to_error(9999), QuicError::Transport(_)));
    }

    /// A refusal reaches the user as advice, not a status code. `NoDesktop` in particular must
    /// not read as broken — it clears when a window opens.
    #[test]
    fn a_refusal_explains_what_to_do() {
        assert_eq!(
            QuicError::Refused(SharedFailure::NoDesktop).to_string(),
            "Open the Vmux window on your Mac."
        );
        assert_eq!(
            QuicError::Unauthorized.to_string(),
            "Pairing expired. Scan the QR on your Mac again."
        );
    }
}
