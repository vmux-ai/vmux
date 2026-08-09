use super::*;
use std::time::{Duration, Instant};
use tungstenite::{
    Message, WebSocket, client::IntoClientRequest, connect, http::HeaderValue,
    stream::MaybeTlsStream,
};
use vmux_core::extension::protocol::{
    ApiRequest, ApiResponse, BRIDGE_PROTOCOL_VERSION, BridgeClientMessage, BridgeHello,
    BridgeServerMessage, ChromeError, ExtensionContextKind,
};

const EXTENSION_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn connect_bridge(
    server: &ExtensionBridgeServer,
) -> WebSocket<MaybeTlsStream<std::net::TcpStream>> {
    connect_with_origin(server, &format!("chrome-extension://{EXTENSION_ID}"))
        .unwrap()
        .0
}

fn connect_with_origin(
    server: &ExtensionBridgeServer,
    origin: &str,
) -> tungstenite::Result<(
    WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    tungstenite::handshake::client::Response,
)> {
    let mut request = server.endpoint().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_str(origin).unwrap());
    connect(request)
}

fn send_json(
    socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    message: &impl serde::Serialize,
) {
    socket
        .send(Message::Text(
            serde_json::to_string(message).unwrap().into(),
        ))
        .unwrap();
}

fn read_json<T: serde::de::DeserializeOwned>(
    socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
) -> T {
    loop {
        match socket.read().unwrap() {
            Message::Text(text) => return serde_json::from_str(&text).unwrap(),
            Message::Ping(payload) => socket.send(Message::Pong(payload)).unwrap(),
            _ => {}
        }
    }
}

fn hello(identity: &BridgeIdentity, token: String) -> BridgeClientMessage {
    BridgeClientMessage::Hello(BridgeHello {
        protocol_version: BRIDGE_PROTOCOL_VERSION,
        extension_id: identity.extension_id.clone(),
        profile_id: identity.profile_id.clone(),
        token,
        context_id: "bridge-page".into(),
        context_kind: ExtensionContextKind::BridgePage,
    })
}

fn recv_inbound(server: &ExtensionBridgeServer) -> BridgeInbound {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(inbound) = server.try_recv() {
            return inbound;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for bridge frame"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn authenticates_and_routes_bidirectionally() {
    let server = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
    let identity = server.identity(EXTENSION_ID).unwrap().clone();
    let mut socket = connect_bridge(&server);
    send_json(&mut socket, &hello(&identity, identity.token.clone()));

    let ready: BridgeServerMessage = read_json(&mut socket);
    assert_eq!(
        ready,
        BridgeServerMessage::Ready {
            protocol_version: BRIDGE_PROTOCOL_VERSION
        }
    );
    let request = ApiRequest {
        request_id: "r1".into(),
        namespace: "tabs".into(),
        method: "query".into(),
        arguments: serde_json::json!({}),
        caller_context: vmux_core::extension::protocol::ExtensionCallerContext::ServiceWorker {
            extension_id: EXTENSION_ID.into(),
            context_id: "service-worker".into(),
            url: None,
        },
    };
    send_json(
        &mut socket,
        &BridgeClientMessage::ApiRequest(request.clone()),
    );
    let inbound = recv_inbound(&server);
    assert_eq!(inbound.extension_id, EXTENSION_ID);
    assert_eq!(inbound.message, BridgeClientMessage::ApiRequest(request));
    server
        .send(
            EXTENSION_ID,
            BridgeServerMessage::Response(ApiResponse::success(
                "r1",
                serde_json::json!({ "ok": true }),
            )),
        )
        .unwrap();
    let response: BridgeServerMessage = read_json(&mut socket);
    assert_eq!(
        response,
        BridgeServerMessage::Response(ApiResponse::success(
            "r1",
            serde_json::json!({ "ok": true })
        ))
    );
}

#[test]
fn rejects_wrong_token_and_closes_socket() {
    let server = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
    let identity = server.identity(EXTENSION_ID).unwrap().clone();
    let mut socket = connect_bridge(&server);
    send_json(&mut socket, &hello(&identity, "wrong".into()));

    let fatal: BridgeServerMessage = read_json(&mut socket);
    assert_eq!(
        fatal,
        BridgeServerMessage::Fatal(ChromeError::new(
            "authentication_failed",
            "bridge authentication failed"
        ))
    );
    assert!(matches!(
        socket.read(),
        Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed)
    ));
}

#[test]
fn rejects_non_extension_websocket_origin() {
    let server = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();

    let error = connect_with_origin(&server, "https://example.com").unwrap_err();

    assert!(
        matches!(error, tungstenite::Error::Http(response) if response.status() == StatusCode::FORBIDDEN)
    );
}

#[test]
fn rejects_client_selected_context_authority() {
    let server = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
    let identity = server.identity(EXTENSION_ID).unwrap().clone();
    let mut socket = connect_bridge(&server);
    send_json(
        &mut socket,
        &BridgeClientMessage::Hello(BridgeHello {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            extension_id: identity.extension_id,
            profile_id: identity.profile_id,
            token: identity.token,
            context_id: "worker".into(),
            context_kind: ExtensionContextKind::ServiceWorker,
        }),
    );

    let fatal: BridgeServerMessage = read_json(&mut socket);

    assert_eq!(
        fatal,
        BridgeServerMessage::Fatal(ChromeError::new(
            "authentication_failed",
            "bridge authentication failed"
        ))
    );
}

#[test]
fn connection_counter_enforces_limit() {
    let counter = Arc::new(AtomicUsize::new(0));
    let first = try_acquire(&counter, 1).unwrap();

    assert!(try_acquire(&counter, 1).is_none());
    drop(first);
    assert!(try_acquire(&counter, 1).is_some());
}

#[test]
fn replacement_session_cancels_old_inbound_route() {
    let server = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
    let identity = server.identity(EXTENSION_ID).unwrap().clone();
    let mut first = connect_bridge(&server);
    send_json(&mut first, &hello(&identity, identity.token.clone()));
    let _: BridgeServerMessage = read_json(&mut first);
    let mut second = connect_bridge(&server);
    send_json(&mut second, &hello(&identity, identity.token.clone()));
    let _: BridgeServerMessage = read_json(&mut second);
    let request = ApiRequest {
        request_id: "replacement".into(),
        namespace: "tabs".into(),
        method: "query".into(),
        arguments: serde_json::json!({}),
        caller_context: vmux_core::extension::protocol::ExtensionCallerContext::ServiceWorker {
            extension_id: EXTENSION_ID.into(),
            context_id: "service-worker".into(),
            url: None,
        },
    };

    let _ = first.send(Message::Text(
        serde_json::to_string(&BridgeClientMessage::ApiRequest(request.clone()))
            .unwrap()
            .into(),
    ));
    std::thread::sleep(Duration::from_millis(20));
    if let Ok(stale) = server.try_recv() {
        assert!(!server.is_current_session(&stale.extension_id, stale.session_id));
    }

    send_json(
        &mut second,
        &BridgeClientMessage::ApiRequest(request.clone()),
    );
    let inbound = recv_inbound(&server);
    assert_eq!(inbound.message, BridgeClientMessage::ApiRequest(request));
}
