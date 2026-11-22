use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Mutex;
use vmux_api::protocol::{SharedEvent, SharedFailure, SharedMessage, SharedResponse};
use vmux_transport::framing::{Frame, FrameStream};
use vmux_transport::quic::endpoint::Trust;
use vmux_transport::quic::tunnel::TunnelSocket;
use vmux_transport::quic::{
    Accepted, ClientSetup, CloseCode, MessageType, RelaySetup, SessionAccepted,
};
use vmux_transport::{ClientCredential, DeviceId, PeerRole};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const SETUP: FrameStream = FrameStream::new(16 * 1024);
const CONTROL: FrameStream = FrameStream::new(MAX_RESPONSE_BYTES);

#[derive(Debug)]
pub(crate) enum QuicError {
    Unauthorized,
    RemoteDisabled,
    Refused(SharedFailure),
    Closed,
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
            Self::Closed => f.write_str(&translate("mobile-error-connection-closed")),
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

#[derive(Debug)]
pub(crate) enum StreamFrame {
    Event(Box<SharedEvent>),
    Malformed,
    Ended,
    Refused(QuicError),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Endpoint {
    pub(crate) address: String,
    pub(crate) relay_token: String,
    pub(crate) credential: ClientCredential,
    pub(crate) client_id: DeviceId,
    pub(crate) fingerprint: String,
    pub(crate) desktop: DeviceId,
}

#[derive(Clone)]
pub(crate) struct QuicApi {
    endpoint: Endpoint,
    credential: Arc<Mutex<ClientCredential>>,
    connection: Arc<Mutex<Option<Dialled>>>,
    sealed: Arc<AtomicBool>,
}

struct Dialled {
    connection: quinn::Connection,
    _relay: quinn::Endpoint,
    _inner: quinn::Endpoint,
}

impl Dialled {
    fn close_with(self, reason: &[u8]) {
        self.connection
            .close(CloseCode::Normal.as_u32().into(), reason);
    }
}

impl QuicApi {
    const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

    pub(crate) fn new(endpoint: Endpoint) -> Self {
        let credential = endpoint.credential.clone();
        Self {
            endpoint,
            credential: Arc::new(Mutex::new(credential)),
            connection: Arc::new(Mutex::new(None)),
            sealed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn seal(&self) {
        self.sealed.store(true, Ordering::Release);
        if let Ok(mut slot) = self.connection.try_lock()
            && let Some(dialled) = slot.take()
        {
            dialled.close_with(b"replaced");
        }
    }

    pub(crate) async fn close_and_drain(&self) {
        self.sealed.store(true, Ordering::Release);
        let dialled = self.connection.lock().await.take();
        if let Some(dialled) = dialled {
            dialled.close_with(b"replaced");
        }
    }

    pub(crate) async fn reset(&self) {
        let dialled = self.connection.lock().await.take();
        if let Some(dialled) = dialled {
            dialled.close_with(b"suspended");
        }
    }

    pub(crate) async fn credential(&self) -> ClientCredential {
        self.credential.lock().await.clone()
    }

    pub(crate) fn is_sealed(&self) -> bool {
        self.sealed.load(Ordering::Acquire)
    }

    async fn connected(&self) -> Result<quinn::Connection, QuicError> {
        if self.is_sealed() {
            return Err(QuicError::Closed);
        }
        let mut slot = self.connection.lock().await;
        if self.is_sealed() {
            if let Some(existing) = slot.take() {
                existing.close_with(b"replaced");
            }
            return Err(QuicError::Closed);
        }
        if let Some(existing) = slot.as_ref()
            && existing.connection.close_reason().is_none()
        {
            return Ok(existing.connection.clone());
        }
        let dialled = self.dial().await?;
        if self.is_sealed() {
            dialled.close_with(b"replaced");
            return Err(QuicError::Closed);
        }
        let connection = dialled.connection.clone();
        *slot = Some(dialled);
        Ok(connection)
    }

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
        let address = vmux_transport::quic::endpoint::resolve_preferring_ipv4(host, port)
            .await
            .map_err(QuicError::Transport)?;
        let server_name = host.to_string();
        let relay_endpoint = Trust::Relay
            .endpoint(address)
            .map_err(QuicError::Transport)?;
        let control = relay_endpoint
            .connect(address, &server_name)
            .map_err(|error| QuicError::Transport(error.to_string()))?
            .await
            .map_err(|error| QuicError::from_connection_error(&error))?;
        self.say_hello_to_the_relay(&control).await?;
        let tunnel = TunnelSocket::new(control);
        let inner_endpoint = Trust::Desktop {
            fingerprint: self.endpoint.fingerprint.clone(),
        }
        .endpoint_on(tunnel)
        .map_err(QuicError::Transport)?;
        let connection = inner_endpoint
            .connect(
                TunnelSocket::relayed_peer(TunnelSocket::DESKTOP_TAG),
                "desktop",
            )
            .map_err(|error| QuicError::Transport(error.to_string()))?
            .await
            .map_err(|error| QuicError::from_connection_error(&error))?;
        self.authenticate_desktop(&connection).await?;
        Ok(Dialled {
            connection,
            _relay: relay_endpoint,
            _inner: inner_endpoint,
        })
    }

    async fn authenticate_desktop(&self, connection: &quinn::Connection) -> Result<(), QuicError> {
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        let setup = ClientSetup {
            client_id: self.endpoint.client_id.clone(),
            credential: self.credential.lock().await.clone(),
        };
        let frame = Frame::json(MessageType::CLIENT_SETUP, &setup)
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        SETUP
            .open(&mut send, &frame)
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        send.finish()
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        let accepted = SETUP
            .accept(&mut recv)
            .await
            .map_err(|_| QuicError::from_close(connection))?
            .read_json::<SessionAccepted>(MessageType::SESSION_ACCEPTED)
            .map_err(|_| QuicError::from_close(connection))?;
        if let Some(device_token) = accepted.device_token {
            *self.credential.lock().await = ClientCredential::Device(device_token);
        }
        Ok(())
    }

    async fn say_hello_to_the_relay(&self, control: &quinn::Connection) -> Result<(), QuicError> {
        let (mut send, mut recv) = control
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        let setup = RelaySetup {
            device_id: self.endpoint.desktop.clone(),
            role: PeerRole::Client,
            token: self.endpoint.relay_token.clone(),
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

    pub(crate) async fn subscribe(&self, sid: &str) -> Result<Subscription, QuicError> {
        let connection = self.connected().await?;
        let (mut send, recv) = connection
            .open_bi()
            .await
            .map_err(|error| QuicError::Transport(error.to_string()))?;
        let request = SharedMessage::AgentAttach {
            sid: sid.to_string(),
        };
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
            connection,
            recv,
            opened: false,
        })
    }

    pub(crate) async fn request(
        &self,
        message: SharedMessage,
    ) -> Result<SharedResponse, QuicError> {
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

pub(crate) struct Subscription {
    connection: quinn::Connection,
    recv: quinn::RecvStream,
    opened: bool,
}

impl Subscription {
    pub(crate) fn stop(&mut self) {
        let _ = self.recv.stop(CloseCode::Normal.as_u32().into());
    }

    pub(crate) async fn next_frame(&mut self) -> StreamFrame {
        let frame = if self.opened {
            match CONTROL.next(&mut self.recv).await {
                Ok(Some(frame)) => frame,
                Ok(None) => return StreamFrame::Ended,
                Err(_) => return self.read_failure(),
            }
        } else {
            self.opened = true;
            match CONTROL.accept(&mut self.recv).await {
                Ok(frame) => frame,
                Err(_) => return self.read_failure(),
            }
        };
        Self::decode_frame(&frame)
    }

    fn read_failure(&self) -> StreamFrame {
        if self.connection.close_reason().is_some() {
            return StreamFrame::Refused(QuicError::from_close(&self.connection));
        }
        StreamFrame::Ended
    }

    fn decode_frame(frame: &Frame) -> StreamFrame {
        let Ok(body) = frame.body_of(MessageType::SESSION_EVENT) else {
            return StreamFrame::Malformed;
        };
        match rkyv::from_bytes::<SharedEvent, rkyv::rancor::Error>(body) {
            Ok(event) => StreamFrame::Event(Box::new(event)),
            Err(_) => StreamFrame::Malformed,
        }
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

    #[test]
    fn sealing_while_connection_mutex_is_held_refuses_new_connections_and_drains() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let api = QuicApi::new(Endpoint {
                address: "127.0.0.1:8787".to_string(),
                relay_token: "relay".to_string(),
                credential: ClientCredential::Pairing("pair".to_string()),
                client_id: DeviceId::new("phone-a"),
                fingerprint: "deadbeef".to_string(),
                desktop: DeviceId::new("mac-a"),
            });
            let held = api.connection.lock().await;
            api.seal();
            assert!(api.is_sealed());

            let drain_api = api.clone();
            let drain = tokio::spawn(async move {
                drain_api.close_and_drain().await;
            });
            tokio::task::yield_now().await;
            assert!(!drain.is_finished());

            drop(held);
            drain.await.unwrap();

            assert!(matches!(api.connected().await, Err(QuicError::Closed)));
            assert!(api.connection.lock().await.is_none());
        });
    }

    #[test]
    fn malformed_session_event_frame_is_distinct_from_clean_end() {
        let wrong_type = Frame::new(MessageType::CONTROL_RESPONSE, vec![1, 2, 3]);
        assert!(matches!(
            Subscription::decode_frame(&wrong_type),
            StreamFrame::Malformed
        ));

        let invalid_payload = Frame::new(MessageType::SESSION_EVENT, vec![0xff, 0x00, 0x13]);
        assert!(matches!(
            Subscription::decode_frame(&invalid_payload),
            StreamFrame::Malformed
        ));
    }
}
