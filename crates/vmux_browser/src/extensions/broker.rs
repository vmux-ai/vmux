use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use crossbeam_channel::RecvTimeoutError;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet, VecDeque};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use vmux_command::{AppCommand, BrowserCommand, open::OpenCommand};
use vmux_core::extension::protocol::{
    ApiEvent, ApiRequest, ApiResponse, BridgeClientMessage, BridgeServerMessage, ChromeError,
    ExtensionCallerContext,
};

use super::bridge::{BridgeAuthorization, BridgeInbound, ExtensionBridgeServer};
use super::capability::{CapabilityKind, CapabilityMatrix, CapabilityStatus};
use super::model::{ChromeModel, ChromeModelEvent};
use super::windows::{
    CloseExtensionWindowRequest, ExtensionWindows, UpdateHostWindowRequest, WindowEffect,
};

const CONFORMANCE_NAMESPACE: &str = "__vmux_conformance";
const MODEL_CHANGED_EVENT: &str = "modelChanged";
const MAX_BRIDGE_MESSAGES_PER_UPDATE: usize = 128;
const MAX_PENDING_EVENTS: usize = 256;
const MAX_SEEN_REQUESTS: usize = 256;
const MAX_CACHED_RESPONSES_PER_EXTENSION: usize = 256;
const MAX_SUBSCRIPTIONS_PER_EXTENSION: usize = 64;
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

#[derive(Default)]
pub(crate) struct SeenBridgeRequests(HashMap<(String, u64), VecDeque<String>>);

#[derive(Resource, Default)]
pub struct BridgeResponseCache(HashMap<String, VecDeque<(String, BridgeServerMessage)>>);

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
    mut response_cache: ResMut<BridgeResponseCache>,
    mut extension_windows: ResMut<ExtensionWindows>,
    mut seen: Local<SeenBridgeRequests>,
    mut app_commands: MessageWriter<AppCommand>,
    mut close_window_requests: MessageWriter<CloseExtensionWindowRequest>,
    mut update_host_window_requests: MessageWriter<UpdateHostWindowRequest>,
    mut model_events: MessageWriter<ChromeModelEvent>,
) {
    for _ in 0..MAX_BRIDGE_MESSAGES_PER_UPDATE {
        let Ok(inbound) = server.try_recv() else {
            break;
        };
        let BridgeInbound {
            extension_id,
            session_id,
            context_id,
            context_kind,
            message,
        } = inbound;
        if !server.is_current_session(&extension_id, session_id) {
            continue;
        }
        seen.0
            .retain(|(id, seen_session), _| id != &extension_id || *seen_session == session_id);
        if context_id != vmux_core::extension::protocol::BRIDGE_CONTEXT_ID
            || context_kind != vmux_core::extension::protocol::ExtensionContextKind::BridgePage
        {
            send_fatal_to_session(
                &server,
                &extension_id,
                session_id,
                "invalid_context",
                "bridge caller context is not authorized",
            );
            continue;
        }
        match message {
            BridgeClientMessage::ApiRequest(request) => {
                let request_id = request.request_id.clone();
                if let Err(error) = authorize_api_request(&server, &extension_id, &request, &model)
                {
                    let response = BridgeServerMessage::Response(ApiResponse::failure(
                        request.request_id,
                        error,
                    ));
                    if let Err(error) = server.send_to_session(&extension_id, session_id, response)
                    {
                        bevy::log::warn!("failed to send extension bridge response: {error}");
                    }
                    continue;
                }
                let Some(authorization) = server.authorization(&extension_id) else {
                    send_fatal_to_session(
                        &server,
                        &extension_id,
                        session_id,
                        "unauthorized_extension",
                        "extension authorization is unavailable",
                    );
                    continue;
                };
                if let Some(response) = response_cache.0.get(&extension_id).and_then(|responses| {
                    responses
                        .iter()
                        .find(|(cached_id, _)| cached_id == &request_id)
                        .map(|(_, response)| response.clone())
                }) {
                    if let Err(error) = server.send_to_session(&extension_id, session_id, response)
                    {
                        bevy::log::warn!(
                            "failed to send cached extension bridge response: {error}"
                        );
                    }
                    continue;
                }
                let requests = seen
                    .0
                    .entry((extension_id.clone(), session_id))
                    .or_default();
                if requests.contains(&request_id) {
                    let duplicate = BridgeServerMessage::Response(ApiResponse::failure(
                        request_id,
                        ChromeError::new("duplicate_request", "duplicate bridge request id"),
                    ));
                    if let Err(error) = server.send_to_session(&extension_id, session_id, duplicate)
                    {
                        bevy::log::warn!("failed to send extension bridge response: {error}");
                    }
                    continue;
                }
                requests.push_back(request_id.clone());
                while requests.len() > MAX_SEEN_REQUESTS {
                    requests.pop_front();
                }
                let dispatched = dispatch_api_request(
                    &CAPABILITY_MATRIX,
                    request,
                    &model,
                    &mut extension_windows,
                    authorization,
                    extension_conformance_enabled(),
                );
                for command in dispatched.commands {
                    app_commands.write(command);
                }
                for effect in dispatched.effects {
                    match effect {
                        WindowEffect::Open(urls) => {
                            for url in urls {
                                app_commands.write(AppCommand::Browser(BrowserCommand::Open(
                                    OpenCommand::InNewStack { url },
                                )));
                            }
                        }
                        WindowEffect::Close { tab_ids, urls } => {
                            close_window_requests
                                .write(CloseExtensionWindowRequest { tab_ids, urls });
                        }
                        WindowEffect::UpdateHost { window_id, update } => {
                            update_host_window_requests
                                .write(UpdateHostWindowRequest { window_id, update });
                        }
                    }
                }
                for event in dispatched.events {
                    model_events.write(event);
                }
                let response = dispatched.response;
                let responses = response_cache.0.entry(extension_id.clone()).or_default();
                responses.push_back((request_id, response.clone()));
                while responses.len() > MAX_CACHED_RESPONSES_PER_EXTENSION {
                    responses.pop_front();
                }
                if let Err(error) = server.send_to_session(&extension_id, session_id, response) {
                    bevy::log::warn!("failed to send extension bridge response: {error}");
                }
            }
            BridgeClientMessage::Subscribe(subscription) => {
                let caller = match validate_caller_context(
                    &extension_id,
                    &subscription.caller_context,
                    &model,
                ) {
                    Ok(caller) => caller,
                    Err(error) => {
                        send_fatal_to_session(
                            &server,
                            &extension_id,
                            session_id,
                            &error.code,
                            &error.message,
                        );
                        continue;
                    }
                };
                let conformance_subscription = subscription.namespace == CONFORMANCE_NAMESPACE
                    && subscription.event == MODEL_CHANGED_EVENT
                    && wake_timer.is_some()
                    && server
                        .authorization(&extension_id)
                        .is_some_and(|authorization| authorization.conformance);
                let windows_subscription = subscription.namespace == "windows"
                    && matches!(
                        subscription.event.as_str(),
                        "onCreated" | "onRemoved" | "onFocusChanged" | "onBoundsChanged"
                    );
                if !conformance_subscription && !windows_subscription {
                    send_fatal_to_session(
                        &server,
                        &extension_id,
                        session_id,
                        "unsupported_event",
                        "bridge event is not supported",
                    );
                    continue;
                }
                let stored = BridgeSubscription {
                    context_id: caller.context_id().to_string(),
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
                } else if entries.len() >= MAX_SUBSCRIPTIONS_PER_EXTENSION {
                    send_fatal_to_session(
                        &server,
                        &extension_id,
                        session_id,
                        "subscription_limit",
                        "extension bridge subscription limit reached",
                    );
                    continue;
                } else {
                    entries.push(stored);
                }
                resend_pending(&server, &pending, &extension_id);
                if !conformance_subscription {
                    continue;
                }
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
            BridgeClientMessage::Unsubscribe {
                subscription_id,
                caller_context,
            } => {
                let Ok(caller) = validate_caller_context(&extension_id, &caller_context, &model)
                else {
                    continue;
                };
                if let Some(entries) = subscriptions.0.get_mut(&extension_id) {
                    entries.retain(|entry| {
                        entry.subscription_id != subscription_id
                            || entry.context_id != caller.context_id()
                    });
                }
            }
            BridgeClientMessage::Ack { sequence } => {
                pending
                    .events
                    .get_mut(&extension_id)
                    .and_then(|events| events.remove(&sequence));
            }
            BridgeClientMessage::Hello(_) => send_fatal_to_session(
                &server,
                &extension_id,
                session_id,
                "protocol_error",
                "unexpected bridge message",
            ),
        }
    }
}

fn authorize_api_request(
    server: &ExtensionBridgeServer,
    extension_id: &str,
    request: &ApiRequest,
    model: &ChromeModel,
) -> Result<(), ChromeError> {
    validate_caller_context(extension_id, &request.caller_context, model)?;
    if request.request_id.is_empty() || request.request_id.len() > 128 {
        return Err(ChromeError::new(
            "invalid_request",
            "extension request id is invalid",
        ));
    }
    let authorization = server.authorization(extension_id).ok_or_else(|| {
        ChromeError::new(
            "permission_denied",
            "extension authorization is unavailable",
        )
    })?;
    if request.namespace == CONFORMANCE_NAMESPACE {
        if extension_conformance_enabled() && authorization.conformance {
            return Ok(());
        }
        return Err(ChromeError::new(
            "permission_denied",
            "reserved conformance API is not authorized for this extension",
        ));
    }
    if let Some(permission) = required_api_permission(&request.namespace, &request.method)?
        && !authorization.permissions.contains(permission)
    {
        return Err(ChromeError::new(
            "permission_denied",
            format!("{} requires the {permission} permission", request.namespace),
        ));
    }
    if requires_host_permission(&request.namespace, &request.method) {
        if authorization.host_permissions.is_empty() {
            return Err(ChromeError::new(
                "host_permission_denied",
                format!(
                    "{}.{} requires a host permission",
                    request.namespace, request.method
                ),
            ));
        }
        let urls = request_target_urls(request, model)?;
        if urls.is_empty() {
            return Err(ChromeError::new(
                "host_permission_denied",
                format!(
                    "{}.{} target host could not be resolved",
                    request.namespace, request.method
                ),
            ));
        }
        if urls.iter().any(|url| {
            url.scheme() == "file"
                || !authorization
                    .host_permissions
                    .iter()
                    .take(64)
                    .any(|pattern| pattern.matches(url))
        }) {
            return Err(ChromeError::new(
                "host_permission_denied",
                format!(
                    "{}.{} is not allowed for the requested host",
                    request.namespace, request.method
                ),
            ));
        }
    }
    Ok(())
}

fn validate_caller_context<'a>(
    extension_id: &str,
    caller: &'a ExtensionCallerContext,
    model: &ChromeModel,
) -> Result<&'a ExtensionCallerContext, ChromeError> {
    if caller.extension_id() != extension_id || caller.context_id().is_empty() {
        return Err(ChromeError::new(
            "invalid_context",
            "extension caller context is not authorized",
        ));
    }
    if matches!(
        caller,
        ExtensionCallerContext::ServiceWorker { .. } | ExtensionCallerContext::ExtensionPage { .. }
    ) && caller
        .url()
        .is_some_and(|url| !url.starts_with(&format!("chrome-extension://{extension_id}/")))
    {
        return Err(ChromeError::new(
            "invalid_context",
            "extension caller URL does not match its extension origin",
        ));
    }
    if let ExtensionCallerContext::ExtensionPage {
        context_id,
        document_id,
        ..
    } = caller
        && context_id != document_id
    {
        return Err(ChromeError::new(
            "invalid_context",
            "extension page context identity is inconsistent",
        ));
    }
    if let ExtensionCallerContext::ContentScript {
        url,
        tab_id,
        frame_id,
        ..
    } = caller
    {
        let tab = i32::try_from(*tab_id)
            .ok()
            .and_then(|tab_id| model.tabs.iter().find(|tab| tab.id == tab_id));
        if *frame_id < 0 || tab.is_none_or(|tab| tab.url != *url) {
            return Err(ChromeError::new(
                "invalid_context",
                "content-script caller does not match the browser model",
            ));
        }
    }
    Ok(caller)
}

fn required_api_permission(
    namespace: &str,
    method: &str,
) -> Result<Option<&'static str>, ChromeError> {
    let namespace = namespace.split('.').next().unwrap_or(namespace);
    let permission = match namespace {
        "runtime" | "tabs" | "windows" | "action" | "commands" => None,
        "bookmarks" => Some("bookmarks"),
        "browsingData" => Some("browsingData"),
        "contentSettings" => Some("contentSettings"),
        "contextMenus" => Some("contextMenus"),
        "cookies" => Some("cookies"),
        "debugger" => Some("debugger"),
        "declarativeNetRequest" => Some("declarativeNetRequest"),
        "downloads" => Some("downloads"),
        "geolocation" => Some("geolocation"),
        "history" => Some("history"),
        "idle" => Some("idle"),
        "management" => Some("management"),
        "nativeMessaging" => Some("nativeMessaging"),
        "notifications" => Some("notifications"),
        "sessions" => Some("sessions"),
        "storage" => Some("storage"),
        "topSites" => Some("topSites"),
        "webNavigation" => Some("webNavigation"),
        "webRequest" => Some("webRequest"),
        "scripting" => Some("scripting"),
        _ => {
            return Err(ChromeError::new(
                "permission_policy_missing",
                format!("{namespace}.{method} has no permission policy"),
            ));
        }
    };
    if namespace == "tabs" && matches!(method, "executeScript" | "insertCSS" | "removeCSS") {
        return Ok(Some("tabs"));
    }
    Ok(permission)
}

fn requires_host_permission(namespace: &str, method: &str) -> bool {
    matches!(
        (namespace, method),
        ("cookies", _)
            | ("scripting", "executeScript" | "insertCSS" | "removeCSS")
            | (
                "tabs",
                "executeScript" | "insertCSS" | "removeCSS" | "captureVisibleTab"
            )
            | ("webRequest", _)
    )
}

fn request_target_urls(
    request: &ApiRequest,
    model: &ChromeModel,
) -> Result<Vec<url::Url>, ChromeError> {
    let mut urls = Vec::new();
    let mut tab_ids = Vec::new();
    collect_request_targets(&request.arguments, &mut urls, &mut tab_ids, 0);
    if matches!(
        (request.namespace.as_str(), request.method.as_str()),
        ("tabs", "captureVisibleTab")
    ) {
        tab_ids.extend(
            model
                .tabs
                .iter()
                .filter(|tab| tab.active)
                .map(|tab| i64::from(tab.id)),
        );
    }
    if tab_ids.is_empty()
        && let Some(tab_id) = request.caller_context.tab_id()
    {
        tab_ids.push(tab_id);
    }
    for tab_id in tab_ids.into_iter().take(16) {
        let Some(tab) = i32::try_from(tab_id)
            .ok()
            .and_then(|tab_id| model.tabs.iter().find(|tab| tab.id == tab_id))
        else {
            return Err(ChromeError::new(
                "host_permission_denied",
                "extension request target tab is unavailable",
            ));
        };
        if let Ok(url) = url::Url::parse(&tab.url) {
            urls.push(url);
        }
    }
    urls.truncate(16);
    Ok(urls)
}

fn collect_request_targets(
    value: &serde_json::Value,
    urls: &mut Vec<url::Url>,
    tab_ids: &mut Vec<i64>,
    depth: usize,
) {
    if depth >= 8 || urls.len() >= 16 || tab_ids.len() >= 16 {
        return;
    }
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                collect_request_targets(value, urls, tab_ids, depth + 1);
            }
        }
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                if matches!(key.as_str(), "url" | "documentUrl" | "originUrl") {
                    if let Some(value) = value.as_str()
                        && let Ok(url) = url::Url::parse(value)
                        && matches!(
                            url.scheme(),
                            "http" | "https" | "ws" | "wss" | "ftp" | "file"
                        )
                    {
                        urls.push(url);
                    }
                } else if key == "tabId" {
                    if let Some(tab_id) = value.as_i64() {
                        tab_ids.push(tab_id);
                    }
                } else {
                    collect_request_targets(value, urls, tab_ids, depth + 1);
                }
            }
        }
        _ => {}
    }
}

pub fn forward_chrome_model_events(
    mut events: MessageReader<ChromeModelEvent>,
    subscriptions: Res<BridgeSubscriptions>,
    server: Res<ExtensionBridgeServer>,
    mut pending: ResMut<PendingBridgeEvents>,
) {
    for event in events.read() {
        for (extension_id, entries) in &subscriptions.0 {
            if entries.iter().any(|entry| {
                entry.namespace == CONFORMANCE_NAMESPACE && entry.event == MODEL_CHANGED_EVENT
            }) {
                let Ok(value) = serde_json::to_value(event) else {
                    bevy::log::error!("failed to serialize Chrome model event");
                    continue;
                };
                queue_event(
                    &server,
                    &mut pending,
                    extension_id,
                    CONFORMANCE_NAMESPACE,
                    MODEL_CHANGED_EVENT,
                    serde_json::json!([value]),
                );
            }
            if let Some((event_name, arguments)) = super::windows::event_payload(event)
                && entries
                    .iter()
                    .any(|entry| entry.namespace == "windows" && entry.event == event_name)
            {
                queue_event(
                    &server,
                    &mut pending,
                    extension_id,
                    "windows",
                    event_name,
                    arguments,
                );
            }
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
            CONFORMANCE_NAMESPACE,
            MODEL_CHANGED_EVENT,
            serde_json::json!([snapshot]),
        );
    }
}

struct DispatchedApiRequest {
    response: BridgeServerMessage,
    commands: Vec<AppCommand>,
    effects: Vec<WindowEffect>,
    events: Vec<ChromeModelEvent>,
}

fn dispatched_response(response: BridgeServerMessage) -> DispatchedApiRequest {
    DispatchedApiRequest {
        response,
        commands: Vec::new(),
        effects: Vec::new(),
        events: Vec::new(),
    }
}

fn create_page_command(request: &ApiRequest) -> Result<AppCommand, ChromeError> {
    let create_info = request
        .arguments
        .as_array()
        .and_then(|arguments| arguments.first())
        .unwrap_or(&request.arguments);
    let url = create_info
        .get("url")
        .and_then(|url| match url {
            serde_json::Value::String(url) => Some(url.as_str()),
            serde_json::Value::Array(urls) => urls.first().and_then(serde_json::Value::as_str),
            _ => None,
        })
        .filter(|url| !url.is_empty());
    let Some(url) = url else {
        return Ok(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InNewStack { url: None },
        )));
    };
    let parsed = url::Url::parse(url)
        .map_err(|_| ChromeError::new("invalid_url", "extension page URL is invalid"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        "chrome-extension" if parsed.host_str() == Some(request.caller_context.extension_id()) => {}
        _ => {
            return Err(ChromeError::new(
                "invalid_url",
                "extension page URL uses an unsupported scheme",
            ));
        }
    }
    Ok(AppCommand::Browser(BrowserCommand::Open(
        OpenCommand::InNewStack {
            url: Some(url.to_string()),
        },
    )))
}

fn dispatch_api_request(
    matrix: &CapabilityMatrix,
    request: ApiRequest,
    model: &ChromeModel,
    extension_windows: &mut ExtensionWindows,
    authorization: &BridgeAuthorization,
    conformance_enabled: bool,
) -> DispatchedApiRequest {
    if request.namespace == CONFORMANCE_NAMESPACE {
        if request.method == "snapshot" && conformance_enabled {
            return dispatched_response(match serde_json::to_value(model) {
                Ok(snapshot) => BridgeServerMessage::Response(ApiResponse::success(
                    request.request_id,
                    snapshot,
                )),
                Err(error) => BridgeServerMessage::Response(ApiResponse::failure(
                    request.request_id,
                    ChromeError::new("serialization_failed", error.to_string()),
                )),
            });
        }
        return dispatched_response(BridgeServerMessage::Response(ApiResponse::failure(
            request.request_id,
            ChromeError::new("unsupported_api", "reserved conformance API is disabled"),
        )));
    }
    if request.namespace == "windows" {
        return match super::windows::dispatch(&request, model, extension_windows, authorization) {
            Ok(dispatched) => DispatchedApiRequest {
                response: BridgeServerMessage::Response(ApiResponse::success(
                    request.request_id,
                    dispatched.result,
                )),
                commands: Vec::new(),
                effects: dispatched.effects,
                events: dispatched.events,
            },
            Err(error) => dispatched_response(BridgeServerMessage::Response(ApiResponse::failure(
                request.request_id,
                error,
            ))),
        };
    }
    if matches!(
        (request.namespace.as_str(), request.method.as_str()),
        ("tabs", "create")
    ) {
        return match create_page_command(&request) {
            Ok(command) => DispatchedApiRequest {
                response: BridgeServerMessage::Response(ApiResponse::success(
                    request.request_id,
                    serde_json::Value::Null,
                )),
                commands: vec![command],
                effects: Vec::new(),
                events: Vec::new(),
            },
            Err(error) => dispatched_response(BridgeServerMessage::Response(ApiResponse::failure(
                request.request_id,
                error,
            ))),
        };
    }
    let member = format!("{}.{}", request.namespace, request.method);
    let Some(capability) = matrix.lookup(
        current_platform(),
        &request.namespace,
        &request.method,
        CapabilityKind::Method,
    ) else {
        return dispatched_response(BridgeServerMessage::Response(ApiResponse::failure(
            request.request_id,
            ChromeError::new(
                "unsupported_api",
                format!(
                    "{member} is not listed for Chromium {} on {}",
                    matrix.chromium_major,
                    current_platform()
                ),
            ),
        )));
    };
    let (code, status) = match &capability.status {
        CapabilityStatus::Untested => ("unsupported_api", "Untested".to_string()),
        CapabilityStatus::Unsupported { reason } => {
            ("unsupported_api", format!("Unsupported: {reason}"))
        }
        CapabilityStatus::Native => ("native_api_not_bridged", "Native".to_string()),
        CapabilityStatus::Bridged => ("bridge_handler_missing", "Bridged".to_string()),
    };
    dispatched_response(BridgeServerMessage::Response(ApiResponse::failure(
        request.request_id,
        ChromeError::new(
            code,
            format!(
                "{member} is {status} for Chromium {} on {}",
                matrix.chromium_major,
                current_platform()
            ),
        ),
    )))
}

fn queue_event(
    server: &ExtensionBridgeServer,
    pending: &mut PendingBridgeEvents,
    extension_id: &str,
    namespace: &str,
    event_name: &str,
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
        namespace: namespace.into(),
        event: event_name.into(),
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

fn send_fatal_to_session(
    server: &ExtensionBridgeServer,
    extension_id: &str,
    session_id: u64,
    code: &str,
    message: &str,
) {
    if let Err(error) = server.send_to_session(
        extension_id,
        session_id,
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
    use crate::extensions::bridge::{
        BridgeAuthorization, BridgeRegistration, ExtensionBridgeServer,
    };
    use std::time::Duration;
    use tungstenite::{
        Message, WebSocket, client::IntoClientRequest, connect, http::HeaderValue,
        stream::MaybeTlsStream,
    };
    use vmux_core::extension::protocol::{
        ApiRequest, ApiResponse, BRIDGE_PROTOCOL_VERSION, BridgeClientMessage, BridgeHello,
        BridgeServerMessage, ChromeError, EventSubscribe, ExtensionCallerContext,
        ExtensionContextKind,
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
}
