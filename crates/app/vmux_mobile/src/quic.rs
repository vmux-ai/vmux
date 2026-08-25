use std::sync::Arc;

use tokio::sync::Mutex;
use vmux_remote::DeviceId;
use vmux_remote::PeerRole;
use vmux_remote::framing::{Frame, FrameStream};
use vmux_remote::quic::endpoint::Trust;
use vmux_remote::quic::tunnel::{DESKTOP_TAG, TunnelSocket, relayed_peer};
use vmux_remote::quic::{Accepted, ClientSetup, CloseCode, MessageType, RelaySetup};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_wire::protocol::{AgentAction, SharedEvent, SharedFailure, SharedMessage, SharedResponse};

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

const SETUP: FrameStream = FrameStream::new(16 * 1024);

const CONTROL: FrameStream = FrameStream::new(MAX_RESPONSE_BYTES);

#[derive(Debug)]
pub enum QuicError {
    Unauthorized,
    RemoteDisabled,
    Refused(SharedFailure),
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
    fn from_connection_error(error: &quinn::ConnectionError) -> Self {
        match error {
            quinn::ConnectionError::ApplicationClosed(closed) => {
                Self::from_close_code(closed.error_code.into_inner())
            }
            quinn::ConnectionError::TransportError(_) => Self::Unauthorized,
            other => Self::Transport(other.to_string()),
        }
    }

    fn from_close(connection: &quinn::Connection) -> Self {
        match connection.close_reason() {
            Some(quinn::ConnectionError::ApplicationClosed(closed)) => {
                Self::from_close_code(closed.error_code.into_inner())
            }
            Some(other) => Self::Transport(other.to_string()),
            None => Self::Transport(translate("mobile-error-connection-dropped")),
        }
    }

    fn from_close_code(code: u64) -> Self {
        match u32::try_from(code).ok().and_then(CloseCode::from_u32) {
            Some(CloseCode::Unauthorized) => Self::Unauthorized,
            Some(CloseCode::RemoteDisabled) => Self::RemoteDisabled,
            _ => Self::Transport(translate("mobile-error-connection-closed")),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Endpoint {
    pub address: String,
    pub token: String,
    pub fingerprint: String,
    pub desktop: DeviceId,
}

#[derive(Clone)]
pub struct QuicApi {
    endpoint: Endpoint,
    connection: Arc<Mutex<Option<Dialled>>>,
}

struct Dialled {
    connection: quinn::Connection,
    _relay: quinn::Endpoint,
    _inner: quinn::Endpoint,
}

impl QuicApi {
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            connection: Arc::new(Mutex::new(None)),
        }
    }

    pub fn close(&self) {
        let Ok(mut slot) = self.connection.try_lock() else {
            return;
        };
        if let Some(dialled) = slot.take() {
            dialled
                .connection
                .close(CloseCode::Normal.as_u32().into(), b"replaced");
        }
    }

    pub async fn reset(&self) {
        if let Some(dialled) = self.connection.lock().await.take() {
            dialled
                .connection
                .close(CloseCode::Normal.as_u32().into(), b"suspended");
        }
    }

    async fn connected(&self) -> Result<quinn::Connection, QuicError> {
        let mut slot = self.connection.lock().await;
        if let Some(existing) = slot.as_ref()
            && existing.connection.close_reason().is_none()
        {
            return Ok(existing.connection.clone());
        }
        let dialled = self.dial().await?;
        let connection = dialled.connection.clone();
        *slot = Some(dialled);
        Ok(connection)
    }

    const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

    async fn dial(&self) -> Result<Dialled, QuicError> {
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

    async fn dial_inner(&self) -> Result<Dialled, QuicError> {
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

        self.say_hello_to_the_relay(&control).await?;

        let tunnel = TunnelSocket::new(control.clone());
        let inner_endpoint = Trust::Desktop {
            fingerprint: self.endpoint.fingerprint.clone(),
        }
        .endpoint_on(tunnel)
        .map_err(QuicError::Transport)?;
        let connection = inner_endpoint
            .connect(relayed_peer(DESKTOP_TAG), "desktop")
            .map_err(|error| QuicError::Transport(error.to_string()))?
            .await
            .map_err(|error| QuicError::from_connection_error(&error))?;

        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        let setup = ClientSetup {
            device_id: self.endpoint.desktop.clone(),
            token: self.endpoint.token.clone(),
        };
        let frame = Frame::json(MessageType::CLIENT_SETUP, &setup)
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        SETUP
            .open(&mut send, &frame)
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        send.finish()
            .map_err(|error| QuicError::Transport(error.to_string()))?;

        SETUP
            .accept(&mut recv)
            .await
            .map_err(|_| QuicError::from_close(&connection))?
            .read_json::<Accepted>(MessageType::SESSION_ACCEPTED)
            .map_err(|_| QuicError::from_close(&connection))?;
        Ok(Dialled {
            connection,
            _relay: relay_endpoint,
            _inner: inner_endpoint,
        })
    }

    async fn say_hello_to_the_relay(&self, control: &quinn::Connection) -> Result<(), QuicError> {
        let (mut send, mut recv) = control
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        let setup = RelaySetup {
            device_id: self.endpoint.desktop.clone(),
            role: PeerRole::Client,
            token: self.endpoint.token.clone(),
        };
        let frame = Frame::json(MessageType::RELAY_SETUP, &setup)
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        SETUP
            .open(&mut send, &frame)
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        send.finish()
            .map_err(|error| QuicError::Transport(error.to_string()))?;

        SETUP
            .accept(&mut recv)
            .await
            .map_err(|_| QuicError::from_close(control))?
            .read_json::<Accepted>(MessageType::RELAY_ACCEPTED)
            .map_err(|_| QuicError::from_close(control))?;
        Ok(())
    }

    pub async fn subscribe(&self, sid: &str) -> Result<Subscription, QuicError> {
        let connection = self.connected().await?;
        let (mut send, recv) = connection
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;

        let request = SharedMessage::agent(sid, AgentAction::Attach);
        let body = rkyv::to_bytes::<rkyv::rancor::Error>(&request)
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        let frame = Frame::new(MessageType::SESSION_EVENTS, body.to_vec());
        CONTROL
            .open(&mut send, &frame)
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        send.finish()
            .map_err(|error| QuicError::Transport(error.to_string()))?;

        Ok(Subscription {
            recv,
            opened: false,
        })
    }

    pub async fn request(&self, message: SharedMessage) -> Result<SharedResponse, QuicError> {
        let connection = self.connected().await?;
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;

        let body = rkyv::to_bytes::<rkyv::rancor::Error>(&message)
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        let frame = Frame::new(MessageType::CONTROL_REQUEST, body.to_vec());
        CONTROL
            .open(&mut send, &frame)
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        send.finish()
            .map_err(|error| QuicError::Transport(error.to_string()))?;

        let answer = CONTROL
            .accept(&mut recv)
            .await
            .map_err(|_| QuicError::from_close(&connection))?;
        let body = answer
            .body_of(MessageType::CONTROL_RESPONSE)
            .map_err(|_| QuicError::from_close(&connection))?;
        let response = rkyv::from_bytes::<SharedResponse, rkyv::rancor::Error>(body)
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        match response {
            SharedResponse::Failed(failure) => Err(QuicError::Refused(failure)),
            other => Ok(other),
        }
    }
}

pub struct Subscription {
    recv: quinn::RecvStream,
    opened: bool,
}

impl Subscription {
    pub async fn next(&mut self) -> Option<SharedEvent> {
        let frame = if self.opened {
            CONTROL.next(&mut self.recv).await.ok()??
        } else {
            self.opened = true;
            CONTROL.accept(&mut self.recv).await.ok()?
        };
        let body = frame.body_of(MessageType::SESSION_EVENT).ok()?;
        rkyv::from_bytes::<SharedEvent, rkyv::rancor::Error>(body).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
