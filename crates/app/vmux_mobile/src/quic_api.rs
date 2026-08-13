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
use vmux_remote::PeerRole;
use vmux_remote::quic::endpoint::Trust;
use vmux_remote::quic::tunnel::{DESKTOP_TAG, TunnelSocket, relayed_peer};
use vmux_remote::quic::{
    ClientHello, CloseCode, RelayAccepted, RelayHello, ServerHello, StreamKind, decode_hello,
    encode_hello,
};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_wire::protocol::{AgentAction, SharedEvent, SharedFailure, SharedMessage, SharedResponse};

/// Matches the daemon's cap on a control response.
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// What went wrong, in the terms a page needs to decide whether to retry.
#[derive(Debug)]
pub enum QuicError {
    /// Token rejected, or the desktop presented a certificate we are not paired with. Both mean
    /// re-pair; neither is fixed by waiting.
    Unauthorized,
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
            Self::Unauthorized => f.write_str(&translate("mobile-error-pairing-expired")),
            Self::RemoteDisabled => f.write_str(&translate("mobile-error-remote-disabled")),
            Self::Refused(SharedFailure::NotFound) => {
                f.write_str(&translate("mobile-error-session-gone"))
            }
            Self::Refused(SharedFailure::NoDesktop) => {
                f.write_str(&translate("mobile-error-no-desktop"))
            }
            Self::Refused(_) => f.write_str(&translate("mobile-error-refused")),
            Self::Transport(message) => f.write_str(message),
        }
    }
}

impl QuicError {
    /// Why a dial failed.
    ///
    /// A refused handshake is almost always the pinning, and saying so beats a TLS error string.
    fn from_connection_error(error: &quinn::ConnectionError) -> Self {
        match error {
            quinn::ConnectionError::ApplicationClosed(closed) => {
                Self::from_close_code(closed.error_code.into_inner())
            }
            quinn::ConnectionError::TransportError(_) => Self::Unauthorized,
            other => Self::Transport(other.to_string()),
        }
    }

    /// Why a live connection ended.
    ///
    /// The desktop closes with a code rather than just dropping, so a client can tell the user
    /// something specific instead of "connection lost".
    fn from_close(connection: &quinn::Connection) -> Self {
        match connection.close_reason() {
            Some(quinn::ConnectionError::ApplicationClosed(closed)) => {
                Self::from_close_code(closed.error_code.into_inner())
            }
            Some(other) => Self::Transport(other.to_string()),
            None => Self::Transport(translate("mobile-error-connection-dropped")),
        }
    }

    /// One close code, as advice the user can act on.
    fn from_close_code(code: u64) -> Self {
        match u32::try_from(code).ok().and_then(CloseCode::from_u32) {
            Some(CloseCode::Unauthorized) => Self::Unauthorized,
            Some(CloseCode::RemoteDisabled) => Self::RemoteDisabled,
            _ => Self::Transport(translate("mobile-error-connection-closed")),
        }
    }
}

/// Where and how to reach one paired desktop.
#[derive(Clone, Debug, PartialEq)]
pub struct Endpoint {
    /// The relay, which every desktop is reached through.
    pub address: String,
    pub token: String,
    /// SHA-256 of the desktop's certificate, from the pairing link.
    pub fingerprint: String,
    /// Which desktop to ask the relay for. Not this phone's own id — the relay has no use for
    /// that, since it routes to a pair rather than to a peer.
    pub desktop: DeviceId,
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
            Err(_) => Err(QuicError::Transport(translate_with(
                "mobile-error-no-answer",
                &[
                    ("address", TranslationValue::String(&self.endpoint.address)),
                    (
                        "seconds",
                        TranslationValue::Number(Self::DIAL_TIMEOUT.as_secs() as i64),
                    ),
                ],
            ))),
        }
    }

    /// Dial the relay, ask it for this pairing's desktop, and stack the session that terminates
    /// on that desktop over the tunnel it hands back.
    ///
    /// Two QUIC connections, deliberately. The outer one is with the relay and verified against
    /// the public roots; the inner one terminates on the desktop and is pinned by fingerprint, so
    /// the relay carries bytes it holds no key for.
    async fn dial_inner(&self) -> Result<quinn::Connection, QuicError> {
        // The pairing link names the relay by host, so this has to resolve rather than parse: a
        // hostname is not a SocketAddr, and the old parse turned every relay pairing into "that
        // pairing address is not valid".
        let (host, port) = self
            .endpoint
            .address
            .rsplit_once(':')
            .ok_or_else(|| QuicError::Transport(translate("mobile-error-address-invalid")))?;
        let port: u16 = port
            .parse()
            .map_err(|_| QuicError::Transport(translate("mobile-error-address-no-port")))?;
        let address = vmux_remote::quic::endpoint::resolve_preferring_ipv4(host, port)
            .await
            .map_err(QuicError::Transport)?;

        let server_name = self
            .endpoint
            .address
            .rsplit_once(':')
            .map(|(host, _)| host)
            .unwrap_or(&self.endpoint.address)
            .to_string();

        let relay_endpoint = Trust::Relay {
            host: server_name.clone(),
        }
        .endpoint(address)
        .map_err(QuicError::Transport)?;
        let control = relay_endpoint
            .connect(address, &server_name)
            .map_err(|error| QuicError::Transport(error.to_string()))?
            .await
            .map_err(|error| QuicError::from_connection_error(&error))?;
        // Dropping the endpoint would close the connection with it.
        std::mem::forget(relay_endpoint);

        self.say_hello_to_the_relay(&control).await?;

        let tunnel = TunnelSocket::new(control.clone());
        let inner_endpoint = Trust::Desktop {
            fingerprint: self.endpoint.fingerprint.clone(),
        }
        .endpoint_on(tunnel)
        .map_err(QuicError::Transport)?;
        // The name is not checked — the certificate is pinned outright — but quinn requires one.
        let connection = inner_endpoint
            .connect(relayed_peer(DESKTOP_TAG), "desktop")
            .map_err(|error| QuicError::Transport(error.to_string()))?
            .await
            .map_err(|error| QuicError::from_connection_error(&error))?;
        std::mem::forget(inner_endpoint);

        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        // The desktop does not read this — it admits on the token alone — so it names the pairing
        // rather than the phone, which is the only identity either end shares.
        let hello = AuthenticatedHello {
            hello: ClientHello {
                device_id: self.endpoint.desktop.clone(),
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
            .map_err(|_| QuicError::from_close(&connection))?;
        // The answer carries nothing; reading a well-formed one is the accept signal. A refusal
        // arrives as a close code instead, which is what `from_close` turns into advice.
        decode_hello::<ServerHello>(&answer).map_err(|_| QuicError::from_close(&connection))?;
        Ok(connection)
    }

    /// Name the desktop this pairing is for, and wait for the relay to admit it.
    ///
    /// The token is the desktop's own registration token: it proves the pair rather than the
    /// peer, which is what stops anyone who merely knows a device id attaching to it.
    async fn say_hello_to_the_relay(&self, control: &quinn::Connection) -> Result<(), QuicError> {
        let (mut send, mut recv) = control
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        let hello = RelayHello {
            device_id: self.endpoint.desktop.clone(),
            role: PeerRole::Client,
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
            .read_to_end(16 * 1024)
            .await
            .map_err(|_| QuicError::from_close(control))?;
        decode_hello::<RelayAccepted>(&answer).map_err(|_| QuicError::from_close(control))?;
        Ok(())
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

        let request = SharedMessage::agent(sid, AgentAction::Attach);
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
            .map_err(|_| QuicError::from_close(&connection))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The mapping is what turns a close code into something a user can act on, so each one has
    /// to stay distinct — collapsing two would send someone to re-pair over a version mismatch.
    #[test]
    fn every_close_code_maps_to_its_own_error() {
        assert!(matches!(
            QuicError::from_close_code(CloseCode::Unauthorized.as_u32() as u64),
            QuicError::Unauthorized
        ));
        assert!(matches!(
            QuicError::from_close_code(CloseCode::RemoteDisabled.as_u32() as u64),
            QuicError::RemoteDisabled
        ));
        assert!(matches!(
            QuicError::from_close_code(9999),
            QuicError::Transport(_)
        ));
    }

    /// A refusal reaches the user as advice, not a status code. `NoDesktop` in particular must
    /// not read as broken — it clears when a window opens.
    ///
    /// Pins the locale first: `Display` reads the current one, which otherwise comes from
    /// whatever the machine running the tests is set to.
    #[test]
    fn a_refusal_explains_what_to_do() {
        vmux_ui::i18n::Locale::from("en-US").make_current();
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
