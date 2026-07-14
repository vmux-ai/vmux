use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use crossbeam_channel::RecvTimeoutError;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use vmux_core::extension::protocol::{
    ApiEvent, ApiRequest, ApiResponse, BridgeClientMessage, BridgeServerMessage, ChromeError,
};

use super::bridge::{BridgeInbound, ExtensionBridgeServer};
use super::capability::{CapabilityKind, CapabilityMatrix, CapabilityStatus};
use super::model::{ChromeModel, ChromeModelEvent};

const CONFORMANCE_NAMESPACE: &str = "__vmux_conformance";
const MODEL_CHANGED_EVENT: &str = "modelChanged";
const MAX_PENDING_EVENTS: usize = 256;
static CAPABILITY_MATRIX: LazyLock<CapabilityMatrix> = LazyLock::new(|| {
    let matrix = CapabilityMatrix::embedded().expect("valid embedded capability matrix");
    matrix
        .validate()
        .expect("valid extension capability matrix");
    matrix
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeSubscription {
    pub context_id: String,
    pub subscription_id: String,
    pub namespace: String,
    pub event: String,
}

#[derive(Resource, Default)]
pub struct BridgeSubscriptions(pub HashMap<String, Vec<BridgeSubscription>>);

#[derive(Resource)]
pub struct PendingBridgeEvents {
    next_sequence: u64,
    events: HashMap<String, BTreeMap<u64, ApiEvent>>,
}

impl Default for PendingBridgeEvents {
    fn default() -> Self {
        Self {
            next_sequence: 1,
            events: HashMap::new(),
        }
    }
}

#[derive(Resource)]
pub struct ConformanceWakeTimer {
    delay: Duration,
    deadlines: HashMap<String, Instant>,
    scheduled: HashSet<String>,
    scheduler: Option<crossbeam_channel::Sender<Instant>>,
}

impl Default for ConformanceWakeTimer {
    fn default() -> Self {
        Self {
            delay: Duration::from_secs(35),
            deadlines: HashMap::new(),
            scheduled: HashSet::new(),
            scheduler: None,
        }
    }
}

pub fn drain_bridge_requests(
    server: Res<ExtensionBridgeServer>,
    mut subscriptions: ResMut<BridgeSubscriptions>,
    mut pending: ResMut<PendingBridgeEvents>,
    model: Res<ChromeModel>,
    mut wake_timer: Option<ResMut<ConformanceWakeTimer>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    while let Ok(inbound) = server.try_recv() {
        let BridgeInbound {
            extension_id,
            context_id,
            message,
        } = inbound;
        match message {
            BridgeClientMessage::ApiRequest(request) => {
                let response = dispatch_api_request(
                    &CAPABILITY_MATRIX,
                    request,
                    &model,
                    extension_conformance_enabled(),
                );
                if let Err(error) = server.send(&extension_id, response) {
                    bevy::log::warn!("failed to send extension bridge response: {error}");
                }
            }
            BridgeClientMessage::Subscribe(subscription) => {
                if wake_timer.is_none()
                    || subscription.namespace != CONFORMANCE_NAMESPACE
                    || subscription.event != MODEL_CHANGED_EVENT
                {
                    send_fatal(
                        &server,
                        &extension_id,
                        "unsupported_event",
                        "bridge event is not supported",
                    );
                    continue;
                }
                let stored = BridgeSubscription {
                    context_id,
                    subscription_id: subscription.subscription_id,
                    namespace: subscription.namespace,
                    event: subscription.event,
                };
                let entries = subscriptions.0.entry(extension_id.clone()).or_default();
                if let Some(existing) = entries
                    .iter_mut()
                    .find(|entry| entry.subscription_id == stored.subscription_id)
                {
                    *existing = stored;
                } else {
                    entries.push(stored);
                }
                resend_pending(&server, &pending, &extension_id);
                if let Some(timer) = wake_timer.as_mut() {
                    if timer.scheduled.contains(&extension_id) {
                        continue;
                    }
                    let delay = timer.delay;
                    let deadline = Instant::now() + delay;
                    if timer.scheduler.is_none() {
                        let Some(proxy) = proxy.as_deref() else {
                            bevy::log::warn!("extension conformance wake has no event-loop proxy");
                            continue;
                        };
                        let (sender, receiver) = crossbeam_channel::unbounded();
                        let proxy = (**proxy).clone();
                        match std::thread::Builder::new()
                            .name("extension-conformance-wake".into())
                            .spawn(move || {
                                let mut deadlines = BinaryHeap::<Reverse<Instant>>::new();
                                loop {
                                    let result = deadlines.peek().map_or_else(
                                        || {
                                            receiver
                                                .recv()
                                                .map_err(|_| RecvTimeoutError::Disconnected)
                                        },
                                        |deadline| {
                                            receiver.recv_timeout(
                                                deadline
                                                    .0
                                                    .saturating_duration_since(Instant::now()),
                                            )
                                        },
                                    );
                                    match result {
                                        Ok(deadline) => deadlines.push(Reverse(deadline)),
                                        Err(RecvTimeoutError::Timeout) => {
                                            let now = Instant::now();
                                            while deadlines
                                                .peek()
                                                .is_some_and(|deadline| deadline.0 <= now)
                                            {
                                                deadlines.pop();
                                            }
                                            let _ = proxy.send_event(WinitUserEvent::WakeUp);
                                        }
                                        Err(RecvTimeoutError::Disconnected) => break,
                                    }
                                }
                            }) {
                            Ok(_) => timer.scheduler = Some(sender),
                            Err(error) => {
                                bevy::log::warn!(
                                    "failed to start extension conformance wake scheduler: {error}"
                                );
                                continue;
                            }
                        }
                    }
                    let Some(scheduler) = timer.scheduler.as_ref() else {
                        continue;
                    };
                    if let Err(error) = scheduler.send(deadline) {
                        timer.scheduler = None;
                        bevy::log::warn!("failed to schedule extension conformance wake: {error}");
                        continue;
                    }
                    timer.scheduled.insert(extension_id.clone());
                    timer.deadlines.insert(extension_id, deadline);
                }
            }
            BridgeClientMessage::Ack { sequence } => {
                let removed = pending
                    .events
                    .get_mut(&extension_id)
                    .and_then(|events| events.remove(&sequence));
                if removed.is_none() {
                    send_fatal(
                        &server,
                        &extension_id,
                        "protocol_error",
                        "unknown bridge event acknowledgement",
                    );
                }
            }
            BridgeClientMessage::Hello(_) => send_fatal(
                &server,
                &extension_id,
                "protocol_error",
                "unexpected bridge message",
            ),
        }
    }
}

pub fn forward_chrome_model_events(
    mut events: MessageReader<ChromeModelEvent>,
    subscriptions: Res<BridgeSubscriptions>,
    server: Res<ExtensionBridgeServer>,
    mut pending: ResMut<PendingBridgeEvents>,
) {
    for event in events.read() {
        let Ok(value) = serde_json::to_value(event) else {
            bevy::log::error!("failed to serialize Chrome model event");
            continue;
        };
        for extension_id in subscriptions.0.keys() {
            queue_event(
                &server,
                &mut pending,
                extension_id,
                serde_json::json!([value.clone()]),
            );
        }
    }
}

pub fn fire_conformance_wake_timer(
    timer: Option<ResMut<ConformanceWakeTimer>>,
    model: Res<ChromeModel>,
    server: Res<ExtensionBridgeServer>,
    mut pending: ResMut<PendingBridgeEvents>,
) {
    let Some(mut timer) = timer else {
        return;
    };
    let now = Instant::now();
    let ready = timer
        .deadlines
        .iter()
        .filter_map(|(extension_id, deadline)| (*deadline <= now).then_some(extension_id.clone()))
        .collect::<Vec<_>>();
    for extension_id in ready {
        timer.deadlines.remove(&extension_id);
        let Ok(snapshot) = serde_json::to_value(&*model) else {
            bevy::log::error!("failed to serialize Chrome model snapshot");
            continue;
        };
        queue_event(
            &server,
            &mut pending,
            &extension_id,
            serde_json::json!([snapshot]),
        );
    }
}

fn dispatch_api_request(
    matrix: &CapabilityMatrix,
    request: ApiRequest,
    model: &ChromeModel,
    conformance_enabled: bool,
) -> BridgeServerMessage {
    if request.namespace == CONFORMANCE_NAMESPACE {
        if request.method == "snapshot" && conformance_enabled {
            return match serde_json::to_value(model) {
                Ok(snapshot) => BridgeServerMessage::Response(ApiResponse::success(
                    request.request_id,
                    snapshot,
                )),
                Err(error) => BridgeServerMessage::Response(ApiResponse::failure(
                    request.request_id,
                    ChromeError::new("serialization_failed", error.to_string()),
                )),
            };
        }
        return BridgeServerMessage::Response(ApiResponse::failure(
            request.request_id,
            ChromeError::new("unsupported_api", "reserved conformance API is disabled"),
        ));
    }
    let member = format!("{}.{}", request.namespace, request.method);
    let Some(capability) = matrix.lookup(
        current_platform(),
        &request.namespace,
        &request.method,
        CapabilityKind::Method,
    ) else {
        return BridgeServerMessage::Response(ApiResponse::failure(
            request.request_id,
            ChromeError::new(
                "unsupported_api",
                format!(
                    "{member} is not listed for Chromium {} on {}",
                    matrix.chromium_major,
                    current_platform()
                ),
            ),
        ));
    };
    let (code, status) = match &capability.status {
        CapabilityStatus::Untested => ("unsupported_api", "Untested".to_string()),
        CapabilityStatus::Unsupported { reason } => {
            ("unsupported_api", format!("Unsupported: {reason}"))
        }
        CapabilityStatus::Native => ("native_api_not_bridged", "Native".to_string()),
        CapabilityStatus::Bridged => ("bridge_handler_missing", "Bridged".to_string()),
    };
    BridgeServerMessage::Response(ApiResponse::failure(
        request.request_id,
        ChromeError::new(
            code,
            format!(
                "{member} is {status} for Chromium {} on {}",
                matrix.chromium_major,
                current_platform()
            ),
        ),
    ))
}

fn queue_event(
    server: &ExtensionBridgeServer,
    pending: &mut PendingBridgeEvents,
    extension_id: &str,
    arguments: serde_json::Value,
) {
    if pending
        .events
        .get(extension_id)
        .is_some_and(|events| events.len() >= MAX_PENDING_EVENTS)
    {
        bevy::log::error!("extension bridge event queue overflow for extension {extension_id}");
        send_fatal(
            server,
            extension_id,
            "event_queue_overflow",
            "extension bridge event queue exceeded 256 entries",
        );
        return;
    }
    let sequence = pending.next_sequence;
    pending.next_sequence = pending.next_sequence.saturating_add(1).max(1);
    let event = ApiEvent {
        sequence,
        namespace: CONFORMANCE_NAMESPACE.into(),
        event: MODEL_CHANGED_EVENT.into(),
        arguments,
    };
    pending
        .events
        .entry(extension_id.into())
        .or_default()
        .insert(sequence, event.clone());
    if let Err(error) = server.send(extension_id, BridgeServerMessage::Event(event)) {
        bevy::log::warn!("failed to send extension bridge event: {error}");
    }
}

fn resend_pending(
    server: &ExtensionBridgeServer,
    pending: &PendingBridgeEvents,
    extension_id: &str,
) {
    let Some(events) = pending.events.get(extension_id) else {
        return;
    };
    for event in events.values() {
        if let Err(error) = server.send(extension_id, BridgeServerMessage::Event(event.clone())) {
            bevy::log::warn!("failed to resend extension bridge event: {error}");
            break;
        }
    }
}

fn send_fatal(server: &ExtensionBridgeServer, extension_id: &str, code: &str, message: &str) {
    if let Err(error) = server.send(
        extension_id,
        BridgeServerMessage::Fatal(ChromeError::new(code, message)),
    ) {
        bevy::log::warn!("failed to send extension bridge error: {error}");
    }
}

pub(crate) fn extension_conformance_enabled() -> bool {
    cfg!(feature = "conformance")
        && std::env::var("VMUX_EXTENSION_CONFORMANCE").ok().as_deref() == Some("1")
}

pub(crate) const fn current_platform() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        "unsupported"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::bridge::ExtensionBridgeServer;
    use std::time::Duration;
    use tungstenite::{Message, WebSocket, connect, stream::MaybeTlsStream};
    use vmux_core::extension::protocol::{
        ApiRequest, ApiResponse, BRIDGE_PROTOCOL_VERSION, BridgeClientMessage, BridgeHello,
        BridgeServerMessage, ChromeError, EventSubscribe, ExtensionContextKind,
    };

    const EXTENSION_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn connect_bridge(
        server: &ExtensionBridgeServer,
    ) -> WebSocket<MaybeTlsStream<std::net::TcpStream>> {
        let identity = server.identity(EXTENSION_ID).unwrap().clone();
        let (mut socket, _) = connect(server.endpoint()).unwrap();
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

    fn read_server(
        socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    ) -> BridgeServerMessage {
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
    fn rejects_untested_api_request() {
        let server = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
        let identity = server.identity(EXTENSION_ID).unwrap().clone();
        let (mut socket, _) = connect(server.endpoint()).unwrap();
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
                }))
                .unwrap()
                .into(),
            ))
            .unwrap();

        let mut app = App::new();
        app.insert_resource(server)
            .init_resource::<BridgeSubscriptions>()
            .init_resource::<PendingBridgeEvents>()
            .init_resource::<ChromeModel>()
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
        };

        assert_eq!(
            dispatch_api_request(&matrix, request(), &model, true),
            BridgeServerMessage::Response(ApiResponse::success(
                "snapshot",
                serde_json::to_value(&model).unwrap()
            ))
        );
        assert_eq!(
            dispatch_api_request(&matrix, request(), &model, false),
            BridgeServerMessage::Response(ApiResponse::failure(
                "snapshot",
                ChromeError::new("unsupported_api", "reserved conformance API is disabled")
            ))
        );
    }

    #[test]
    fn subscription_resends_pending_event_until_acknowledged() {
        let server = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
        let mut socket = connect_bridge(&server);
        let mut pending = PendingBridgeEvents::default();
        queue_event(
            &server,
            &mut pending,
            EXTENSION_ID,
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
            }),
        );
        let mut app = App::new();
        app.insert_resource(server)
            .insert_resource(pending)
            .init_resource::<BridgeSubscriptions>()
            .init_resource::<ChromeModel>()
            .init_resource::<ConformanceWakeTimer>()
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
    }

    #[test]
    fn duplicate_subscription_schedules_one_wake() {
        let server = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
        let mut socket = connect_bridge(&server);
        let subscribe = || {
            BridgeClientMessage::Subscribe(EventSubscribe {
                subscription_id: "model".into(),
                namespace: CONFORMANCE_NAMESPACE.into(),
                event: MODEL_CHANGED_EVENT.into(),
            })
        };
        send_client(&mut socket, &subscribe());
        let (scheduler, scheduled) = crossbeam_channel::unbounded();
        let mut app = App::new();
        app.insert_resource(server)
            .init_resource::<BridgeSubscriptions>()
            .init_resource::<PendingBridgeEvents>()
            .init_resource::<ChromeModel>()
            .insert_resource(ConformanceWakeTimer {
                scheduler: Some(scheduler),
                ..Default::default()
            })
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
        let server = ExtensionBridgeServer::start("personal", [EXTENSION_ID]).unwrap();
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
}
