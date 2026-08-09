use super::*;
use crate::extensions::bridge::{BridgeAuthorization, BridgeRegistration, ExtensionBridgeServer};
use std::time::Duration;
use tungstenite::{
    Message, WebSocket, client::IntoClientRequest, connect, http::HeaderValue,
    stream::MaybeTlsStream,
};
use vmux_core::extension::protocol::{
    ApiRequest, ApiResponse, BRIDGE_PROTOCOL_VERSION, BridgeClientMessage, BridgeHello,
    BridgeServerMessage, ChromeError, EventSubscribe, ExtensionCallerContext, ExtensionContextKind,
};

const EXTENSION_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn caller_context() -> ExtensionCallerContext {
    ExtensionCallerContext::ServiceWorker {
        extension_id: EXTENSION_ID.into(),
        context_id: "service-worker".into(),
        url: None,
    }
}

fn conformance_server() -> ExtensionBridgeServer {
    ExtensionBridgeServer::start_registered(
        "personal",
        [BridgeRegistration {
            extension_id: EXTENSION_ID.into(),
            authorization: BridgeAuthorization {
                conformance: true,
                ..Default::default()
            },
        }],
    )
    .unwrap()
}

fn connect_bridge(
    server: &ExtensionBridgeServer,
) -> WebSocket<MaybeTlsStream<std::net::TcpStream>> {
    let identity = server.identity(EXTENSION_ID).unwrap().clone();
    let mut request = server.endpoint().into_client_request().unwrap();
    request.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("chrome-extension://{EXTENSION_ID}")).unwrap(),
    );
    let (mut socket, _) = connect(request).unwrap();
    send_client(
        &mut socket,
        &BridgeClientMessage::Hello(BridgeHello {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            extension_id: identity.extension_id,
            profile_id: identity.profile_id,
            token: identity.token,
            context_id: "bridge-page".into(),
            context_kind: ExtensionContextKind::BridgePage,
        }),
    );
    assert!(matches!(
        read_server(&mut socket),
        BridgeServerMessage::Ready { .. }
    ));
    socket
}

fn send_client(
    socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    message: &BridgeClientMessage,
) {
    socket
        .send(Message::Text(
            serde_json::to_string(message).unwrap().into(),
        ))
        .unwrap();
}

fn read_server(socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>) -> BridgeServerMessage {
    match socket.read().unwrap() {
        Message::Text(text) => serde_json::from_str(&text).unwrap(),
        message => panic!("unexpected bridge frame: {message:?}"),
    }
}

fn pump(app: &mut App) {
    for _ in 0..20 {
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn window_create_dispatches_extension_page_through_app_command() {
    let request = ApiRequest {
        request_id: "window-create".into(),
        namespace: "windows".into(),
        method: "create".into(),
        arguments: serde_json::json!([{
            "url": format!("chrome-extension://{EXTENSION_ID}/popup/index.html")
        }]),
        caller_context: caller_context(),
    };

    let dispatched = dispatch_api_request(
        &CapabilityMatrix::embedded().unwrap(),
        request,
        &ChromeModel::default(),
        &mut ExtensionWindows::default(),
        &BridgeAuthorization::default(),
        false,
    );

    assert!(matches!(
        &dispatched.effects[0],
        WindowEffect::Open(urls)
            if urls == &vec![Some(format!(
                "chrome-extension://{EXTENSION_ID}/popup/index.html"
            ))]
    ));
    let BridgeServerMessage::Response(response) = dispatched.response else {
        panic!("expected response");
    };
    assert_eq!(response.request_id, "window-create");
    assert!(response.result.unwrap()["id"].as_i64().is_some());
}

#[test]
fn window_create_rejects_other_extension_origin() {
    let request = ApiRequest {
        request_id: "window-create".into(),
        namespace: "windows".into(),
        method: "create".into(),
        arguments: serde_json::json!([{
            "url": "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/popup.html"
        }]),
        caller_context: caller_context(),
    };

    let dispatched = dispatch_api_request(
        &CapabilityMatrix::embedded().unwrap(),
        request,
        &ChromeModel::default(),
        &mut ExtensionWindows::default(),
        &BridgeAuthorization::default(),
        false,
    );

    assert!(dispatched.commands.is_empty());
    assert_eq!(
        dispatched.response,
        BridgeServerMessage::Response(ApiResponse::failure(
            "window-create",
            ChromeError::new("invalid_url", "window URL uses an unsupported scheme",),
        ))
    );
}

#[test]
fn rejects_untested_api_request() {
    let server = conformance_server();
    let identity = server.identity(EXTENSION_ID).unwrap().clone();
    let mut socket = connect_bridge_socket(&server);
    socket
        .send(Message::Text(
            serde_json::to_string(&BridgeClientMessage::Hello(BridgeHello {
                protocol_version: BRIDGE_PROTOCOL_VERSION,
                extension_id: identity.extension_id,
                profile_id: identity.profile_id,
                token: identity.token,
                context_id: "bridge-page".into(),
                context_kind: ExtensionContextKind::BridgePage,
            }))
            .unwrap()
            .into(),
        ))
        .unwrap();
    let ready: BridgeServerMessage = match socket.read().unwrap() {
        Message::Text(text) => serde_json::from_str(&text).unwrap(),
        message => panic!("unexpected bridge frame: {message:?}"),
    };
    assert!(matches!(ready, BridgeServerMessage::Ready { .. }));
    socket
        .send(Message::Text(
            serde_json::to_string(&BridgeClientMessage::ApiRequest(ApiRequest {
                request_id: "r1".into(),
                namespace: "tabs".into(),
                method: "query".into(),
                arguments: serde_json::json!({}),
                caller_context: caller_context(),
            }))
            .unwrap()
            .into(),
        ))
        .unwrap();

    let mut app = App::new();
    app.insert_resource(server)
        .init_resource::<BridgeSubscriptions>()
        .init_resource::<BridgeResponseCache>()
        .init_resource::<PendingBridgeEvents>()
        .init_resource::<ChromeModel>()
        .init_resource::<ExtensionWindows>()
        .add_message::<AppCommand>()
        .add_message::<CloseExtensionWindowRequest>()
        .add_message::<UpdateHostWindowRequest>()
        .add_message::<ChromeModelEvent>()
        .add_systems(Update, drain_bridge_requests);
    for _ in 0..20 {
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }

    let response: BridgeServerMessage = match socket.read().unwrap() {
        Message::Text(text) => serde_json::from_str(&text).unwrap(),
        message => panic!("unexpected bridge frame: {message:?}"),
    };
    assert_eq!(
        response,
        BridgeServerMessage::Response(ApiResponse::failure(
            "r1",
            ChromeError::new(
                "unsupported_api",
                format!(
                    "tabs.query is Untested for Chromium 148 on {}",
                    current_platform()
                )
            )
        ))
    );

    socket.close(None).unwrap();
    drop(socket);
    std::thread::sleep(Duration::from_millis(50));
    let mut restarted = connect_bridge(app.world().resource::<ExtensionBridgeServer>());
    send_client(
        &mut restarted,
        &BridgeClientMessage::ApiRequest(ApiRequest {
            request_id: "r1".into(),
            namespace: "tabs".into(),
            method: "query".into(),
            arguments: serde_json::json!({}),
            caller_context: caller_context(),
        }),
    );
    pump(&mut app);

    assert_eq!(read_server(&mut restarted), response);
    assert_eq!(
        app.world().resource::<BridgeResponseCache>().0[EXTENSION_ID].len(),
        1
    );
}

fn connect_bridge_socket(
    server: &ExtensionBridgeServer,
) -> WebSocket<MaybeTlsStream<std::net::TcpStream>> {
    let mut request = server.endpoint().into_client_request().unwrap();
    request.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("chrome-extension://{EXTENSION_ID}")).unwrap(),
    );
    connect(request).unwrap().0
}

#[test]
fn conformance_snapshot_requires_gate() {
    let matrix = CapabilityMatrix::embedded().unwrap();
    let model = ChromeModel::default();
    let request = || ApiRequest {
        request_id: "snapshot".into(),
        namespace: CONFORMANCE_NAMESPACE.into(),
        method: "snapshot".into(),
        arguments: serde_json::json!({}),
        caller_context: caller_context(),
    };

    let enabled = dispatch_api_request(
        &matrix,
        request(),
        &model,
        &mut ExtensionWindows::default(),
        &BridgeAuthorization::default(),
        true,
    );
    assert!(enabled.commands.is_empty());
    assert_eq!(
        enabled.response,
        BridgeServerMessage::Response(ApiResponse::success(
            "snapshot",
            serde_json::to_value(&model).unwrap()
        ))
    );
    let disabled = dispatch_api_request(
        &matrix,
        request(),
        &model,
        &mut ExtensionWindows::default(),
        &BridgeAuthorization::default(),
        false,
    );
    assert!(disabled.commands.is_empty());
    assert_eq!(
        disabled.response,
        BridgeServerMessage::Response(ApiResponse::failure(
            "snapshot",
            ChromeError::new("unsupported_api", "reserved conformance API is disabled")
        ))
    );
}

#[test]
fn broker_enforces_api_and_host_permissions() {
    let server = ExtensionBridgeServer::start_registered(
        "personal",
        [BridgeRegistration {
            extension_id: EXTENSION_ID.into(),
            authorization: BridgeAuthorization {
                permissions: ["storage".into(), "scripting".into()].into_iter().collect(),
                host_permissions: vec![
                    vmux_core::extension::match_pattern::ChromeMatchPattern::parse(
                        "https://*.example.com/*",
                    )
                    .unwrap(),
                ],
                conformance: false,
            },
        }],
    )
    .unwrap();
    let model = ChromeModel::default();
    let request = |namespace: &str, method: &str, arguments: serde_json::Value| ApiRequest {
        request_id: "request".into(),
        namespace: namespace.into(),
        method: method.into(),
        arguments,
        caller_context: caller_context(),
    };

    assert!(
        authorize_api_request(
            &server,
            EXTENSION_ID,
            &request("storage.local", "get", serde_json::json!({})),
            &model,
        )
        .is_ok()
    );
    assert_eq!(
        authorize_api_request(
            &server,
            EXTENSION_ID,
            &request("history", "search", serde_json::json!({})),
            &model,
        )
        .unwrap_err()
        .code,
        "permission_denied"
    );
    assert!(
        authorize_api_request(
            &server,
            EXTENSION_ID,
            &request(
                "scripting",
                "executeScript",
                serde_json::json!({ "url": "https://login.example.com/form" })
            ),
            &model,
        )
        .is_ok()
    );
    assert_eq!(
        authorize_api_request(
            &server,
            EXTENSION_ID,
            &request(
                "scripting",
                "executeScript",
                serde_json::json!({ "url": "https://example.org/form" })
            ),
            &model,
        )
        .unwrap_err()
        .code,
        "host_permission_denied"
    );
    assert_eq!(
        authorize_api_request(
            &server,
            EXTENSION_ID,
            &request(
                "scripting",
                "executeScript",
                serde_json::json!({ "target": { "tabId": 42 } })
            ),
            &model,
        )
        .unwrap_err()
        .code,
        "host_permission_denied"
    );
    let mut invalid_caller = request("storage.local", "get", serde_json::json!({}));
    invalid_caller.caller_context = ExtensionCallerContext::ContentScript {
        extension_id: EXTENSION_ID.into(),
        context_id: "document".into(),
        url: "https://login.example.com/form".into(),
        tab_id: 42,
        frame_id: 0,
        document_id: Some("document".into()),
    };
    assert_eq!(
        authorize_api_request(&server, EXTENSION_ID, &invalid_caller, &model)
            .unwrap_err()
            .code,
        "invalid_context"
    );
}

#[test]
fn subscription_resends_pending_event_until_acknowledged() {
    let server = conformance_server();
    let mut socket = connect_bridge(&server);
    let mut pending = PendingBridgeEvents::default();
    queue_event(
        &server,
        &mut pending,
        EXTENSION_ID,
        CONFORMANCE_NAMESPACE,
        MODEL_CHANGED_EVENT,
        serde_json::json!([{ "change": 1 }]),
    );
    let first = read_server(&mut socket);
    let BridgeServerMessage::Event(first_event) = first else {
        panic!("expected bridge event");
    };
    socket.close(None).unwrap();
    drop(socket);
    std::thread::sleep(Duration::from_millis(50));

    let mut restarted = connect_bridge(&server);
    send_client(
        &mut restarted,
        &BridgeClientMessage::Subscribe(EventSubscribe {
            subscription_id: "model".into(),
            namespace: CONFORMANCE_NAMESPACE.into(),
            event: MODEL_CHANGED_EVENT.into(),
            caller_context: caller_context(),
        }),
    );
    let mut app = App::new();
    app.insert_resource(server)
        .insert_resource(pending)
        .init_resource::<BridgeSubscriptions>()
        .init_resource::<BridgeResponseCache>()
        .init_resource::<ChromeModel>()
        .init_resource::<ConformanceWakeTimer>()
        .init_resource::<ExtensionWindows>()
        .add_message::<AppCommand>()
        .add_message::<CloseExtensionWindowRequest>()
        .add_message::<UpdateHostWindowRequest>()
        .add_message::<ChromeModelEvent>()
        .add_systems(Update, drain_bridge_requests);
    pump(&mut app);

    let resent = read_server(&mut restarted);
    assert_eq!(resent, BridgeServerMessage::Event(first_event.clone()));
    send_client(
        &mut restarted,
        &BridgeClientMessage::Ack {
            sequence: first_event.sequence,
        },
    );
    pump(&mut app);
    assert!(
        app.world()
            .resource::<PendingBridgeEvents>()
            .events
            .get(EXTENSION_ID)
            .is_none_or(BTreeMap::is_empty)
    );

    send_client(
        &mut restarted,
        &BridgeClientMessage::Ack {
            sequence: first_event.sequence,
        },
    );
    pump(&mut app);
    send_client(
        &mut restarted,
        &BridgeClientMessage::ApiRequest(ApiRequest {
            request_id: "after-duplicate-ack".into(),
            namespace: "windows".into(),
            method: "getAll".into(),
            arguments: serde_json::json!([{}]),
            caller_context: caller_context(),
        }),
    );
    pump(&mut app);
    assert!(matches!(
        read_server(&mut restarted),
        BridgeServerMessage::Response(ApiResponse { request_id, .. })
            if request_id == "after-duplicate-ack"
    ));
}

#[test]
fn windows_subscription_receives_window_events() {
    let server = conformance_server();
    let mut socket = connect_bridge(&server);
    send_client(
        &mut socket,
        &BridgeClientMessage::Subscribe(EventSubscribe {
            subscription_id: "windows.onRemoved".into(),
            namespace: "windows".into(),
            event: "onRemoved".into(),
            caller_context: caller_context(),
        }),
    );
    let mut app = App::new();
    app.insert_resource(server)
        .init_resource::<BridgeSubscriptions>()
        .init_resource::<BridgeResponseCache>()
        .init_resource::<PendingBridgeEvents>()
        .init_resource::<ChromeModel>()
        .init_resource::<ExtensionWindows>()
        .add_message::<AppCommand>()
        .add_message::<CloseExtensionWindowRequest>()
        .add_message::<UpdateHostWindowRequest>()
        .add_message::<ChromeModelEvent>()
        .add_systems(
            Update,
            (
                drain_bridge_requests,
                forward_chrome_model_events.after(drain_bridge_requests),
            ),
        );
    pump(&mut app);
    assert!(
        app.world().resource::<BridgeSubscriptions>().0[EXTENSION_ID]
            .iter()
            .any(|entry| entry.namespace == "windows" && entry.event == "onRemoved")
    );

    app.world_mut()
        .write_message(ChromeModelEvent::WindowRemoved { window_id: 42 });
    app.update();

    let BridgeServerMessage::Event(event) = read_server(&mut socket) else {
        panic!("expected window event");
    };
    assert_eq!(event.namespace, "windows");
    assert_eq!(event.event, "onRemoved");
    assert_eq!(event.arguments, serde_json::json!([42]));
}

#[test]
fn duplicate_subscription_schedules_one_wake() {
    let server = conformance_server();
    let mut socket = connect_bridge(&server);
    let subscribe = || {
        BridgeClientMessage::Subscribe(EventSubscribe {
            subscription_id: "model".into(),
            namespace: CONFORMANCE_NAMESPACE.into(),
            event: MODEL_CHANGED_EVENT.into(),
            caller_context: caller_context(),
        })
    };
    send_client(&mut socket, &subscribe());
    let (scheduler, scheduled) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.insert_resource(server)
        .init_resource::<BridgeSubscriptions>()
        .init_resource::<BridgeResponseCache>()
        .init_resource::<PendingBridgeEvents>()
        .init_resource::<ChromeModel>()
        .insert_resource(ConformanceWakeTimer {
            scheduler: Some(scheduler),
            ..Default::default()
        })
        .init_resource::<ExtensionWindows>()
        .add_message::<AppCommand>()
        .add_message::<CloseExtensionWindowRequest>()
        .add_message::<UpdateHostWindowRequest>()
        .add_message::<ChromeModelEvent>()
        .add_systems(Update, drain_bridge_requests);
    pump(&mut app);
    let first_deadline = app.world().resource::<ConformanceWakeTimer>().deadlines[EXTENSION_ID];
    assert_eq!(scheduled.try_recv().unwrap(), first_deadline);

    send_client(&mut socket, &subscribe());
    pump(&mut app);

    let timer = app.world().resource::<ConformanceWakeTimer>();
    assert_eq!(timer.scheduled.len(), 1);
    assert_eq!(timer.deadlines.len(), 1);
    assert_eq!(timer.deadlines[EXTENSION_ID], first_deadline);
    assert_eq!(
        scheduled.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    );
}

#[test]
fn wake_timer_delivers_snapshot_event() {
    let server = conformance_server();
    let mut socket = connect_bridge(&server);
    let model = ChromeModel::default();
    let mut timer = ConformanceWakeTimer {
        delay: Duration::ZERO,
        deadlines: HashMap::new(),
        scheduled: HashSet::new(),
        scheduler: None,
    };
    timer.deadlines.insert(EXTENSION_ID.into(), Instant::now());
    let mut app = App::new();
    app.insert_resource(server)
        .insert_resource(model.clone())
        .insert_resource(timer)
        .init_resource::<PendingBridgeEvents>()
        .add_systems(Update, fire_conformance_wake_timer);

    app.update();

    let BridgeServerMessage::Event(event) = read_server(&mut socket) else {
        panic!("expected wake timer event");
    };
    assert_eq!(event.namespace, CONFORMANCE_NAMESPACE);
    assert_eq!(event.event, MODEL_CHANGED_EVENT);
    assert_eq!(event.arguments, serde_json::json!([model]));
}
