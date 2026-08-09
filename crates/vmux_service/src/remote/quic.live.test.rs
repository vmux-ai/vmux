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

    let (mut send, mut recv) = connection.open_bi().await.expect("hello stream");
    let hello = AuthenticatedHello {
        hello: ClientHello {
            device_id: DeviceId::new("test-device"),
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
        &rkyv::to_bytes::<rkyv::rancor::Error>(&SharedMessage::agent(
            "ghost",
            vmux_wire::protocol::AgentAction::Attach,
        ))
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
