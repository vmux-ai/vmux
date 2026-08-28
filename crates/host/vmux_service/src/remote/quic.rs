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

const REMOTE_STATE_POLL: Duration = Duration::from_secs(1);

const MAX_HELLO_BYTES: usize = 16 * 1024;

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

pub fn identity_fingerprint() -> Option<String> {
    let pem = std::fs::read_to_string(crate::RemotePaths::current().certificate()).ok()?;
    SelfSignedIdentity::fingerprint_of_pem(&pem).ok()
}

fn subject_alt_names() -> Vec<String> {
    vec!["localhost".to_string(), "127.0.0.1".to_string()]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rejection {
    Unauthorized,
    RemoteDisabled,
    Malformed,
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

const SETUP: FrameStream = FrameStream::new(MAX_HELLO_BYTES);

const CONTROL: FrameStream = FrameStream::new(MAX_REQUEST_BYTES);

pub async fn read_setup(stream: &mut quinn::RecvStream) -> Result<ClientSetup, Rejection> {
    match SETUP.accept(stream).await {
        Ok(frame) => frame
            .read_json::<ClientSetup>(MessageType::CLIENT_SETUP)
            .map_err(Rejection::from),
        Err(error) => Err(Rejection::from(error)),
    }
}

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

const MAX_REQUEST_BYTES: usize = (RECEIVE_WINDOW / 8) as usize;

async fn dispatch_control(
    state: super::server::RemoteState,
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
) {
    let frame = match CONTROL.accept(&mut recv).await {
        Ok(frame) => frame,
        Err(error) => {
            refuse(&mut send, &mut recv, Rejection::from(error));
            return;
        }
    };
    match frame.message_type {
        MessageType::CONTROL_REQUEST | MessageType::SESSION_EVENTS => {}
        unserved => {
            tracing::debug!(
                ?unserved,
                "remote quic: stream refused, unserved message type"
            );
            refuse(&mut send, &mut recv, Rejection::Malformed);
            return;
        }
    }

    let Ok(request) = rkyv::from_bytes::<SharedMessage, rkyv::rancor::Error>(&frame.body) else {
        refuse(&mut send, &mut recv, Rejection::Malformed);
        return;
    };

    if frame.message_type == MessageType::SESSION_EVENTS {
        stream_session_events(&state, send, request).await;
        return;
    }

    let response = dispatch::dispatch(&state, request).await;
    let Ok(encoded) = rkyv::to_bytes::<rkyv::rancor::Error>(&response) else {
        refuse(&mut send, &mut recv, Rejection::Malformed);
        return;
    };
    let frame = Frame::new(MessageType::CONTROL_RESPONSE, encoded.to_vec());
    if CONTROL.open(&mut send, &frame).await.is_ok() {
        let _ = send.finish();
    }
}

fn refuse(send: &mut quinn::SendStream, recv: &mut quinn::RecvStream, rejection: Rejection) {
    let code = quinn::VarInt::from(rejection.close_code().as_u32());
    let _ = send.reset(code);
    let _ = recv.stop(code);
}

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
        let _ = send.finish();
        return;
    };
    let mut opened = false;

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
        let identity = SelfSignedIdentity::generate(vec!["localhost".into()]).expect("identity");
        let fingerprint = identity.fingerprint.clone();
        let (_liveness_tx, liveness_rx) = tokio::sync::watch::channel(true);
        std::mem::forget(_liveness_tx);
        let (handle, address) = spawn_with_identity(
            state,
            (Ipv4Addr::LOCALHOST, 0).into(),
            identity,
            liveness_rx,
        )
        .expect("listener");
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

    async fn read_event(recv: &mut quinn::RecvStream) -> Option<vmux_wire::protocol::SharedEvent> {
        let frame = CONTROL.accept(recv).await.ok()?;
        let body = frame.body_of(MessageType::SESSION_EVENT).ok()?;
        rkyv::from_bytes::<vmux_wire::protocol::SharedEvent, rkyv::rancor::Error>(body).ok()
    }

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

        assert!(answered.is_ok(), "it must not hang, got {answered:?}");
        let Ok(read) = answered else {
            unreachable!("checked above")
        };
        assert!(
            read.is_err(),
            "a refused stream must reset, not finish clean — a clean empty finish is what a \
             subscription to a session that does not exist looks like, so the peer could not \
             tell the two apart. Got {read:?}"
        );
    }

    #[tokio::test]
    async fn a_paired_client_can_list_sessions() {
        let harness = start("correct-token");

        let connection = connect(&harness, "correct-token")
            .await
            .expect("handshake should succeed");
        let response = request(&connection, SharedMessage::ListSessions).await;

        assert!(
            matches!(response, SharedResponse::Sessions(ref sessions) if sessions.is_empty()),
            "expected a typed session list, got {response:?}"
        );
    }

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
