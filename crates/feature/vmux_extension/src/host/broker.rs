use crate::protocol::{
    ApiEvent, ApiRequest, ApiResponse, BridgeClientMessage, BridgeServerMessage, EventSubscribe,
    ExtensionApiError, ExtensionCallerContext,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use crossbeam_channel::RecvTimeoutError;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use vmux_ecs::SequenceCounter;
#[cfg(test)]
use vmux_feature::manifest::FeaturePlugin;

use super::access::BridgeAuthorizationRevoked;
use super::bridge::{BridgeAuthorization, BridgeInbound, ExtensionBridgeServer};
use super::capability::{CapabilityKind, CapabilityMatrix, CapabilityStatus};
use super::model::{ExtensionModel, ExtensionModelEvent};
use super::windows::VirtualWindows;

pub(crate) struct ExtensionBrokerPlugin;

impl Plugin for ExtensionBrokerPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        app.add_systems(Startup, spawn)
            .add_systems(
                Update,
                (
                    revoke_bridge_state,
                    bevy::ecs::schedule::ApplyDeferred,
                    drain_bridge_requests,
                    bevy::ecs::schedule::ApplyDeferred,
                )
                    .chain()
                    .in_set(super::DrainBridgeSet)
                    .after(super::access::AccessSet)
                    .after(super::SyncWindowsSet),
            )
            .add_systems(
                Update,
                forward_model_events
                    .after(super::project::ExtensionProjectionSet)
                    .after(super::DrainBridgeSet),
            )
            .add_systems(
                Update,
                fire_conformance_wake_timer.after(super::DrainBridgeSet),
            )
            .add_systems(Update, arm_bridge_wake);
    }
}

#[derive(Component)]
struct BridgeWakeArmed;

#[derive(Clone, Copy)]
struct BridgeEvent;

impl BridgeEvent {
    const MAX_PENDING_EVENTS: usize = 256;
}

#[derive(Clone, Copy)]
struct BridgeOrder;

impl BridgeOrder {
    const MAX_MESSAGES_PER_UPDATE: usize = 128;
    const MAX_SEEN_REQUESTS: usize = 256;
    const MAX_CACHED_RESPONSES: usize = 256;
    const MAX_SUBSCRIPTIONS: usize = 64;
    const CONFORMANCE_NAMESPACE: &'static str = "__vmux_conformance";
    const MODEL_CHANGED_EVENT: &'static str = "modelChanged";
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct BridgeSubscription {
    pub context_id: String,
    pub subscription_id: String,
    pub namespace: String,
    pub event: String,
}

#[derive(Component, Clone, Debug, PartialEq)]
struct PendingBridgeEvent {
    sequence: u64,
    namespace: String,
    event: String,
    arguments: serde_json::Value,
}

impl PendingBridgeEvent {
    fn to_api_event(&self) -> ApiEvent {
        ApiEvent {
            sequence: self.sequence,
            namespace: self.namespace.clone(),
            event: self.event.clone(),
            arguments: self.arguments.clone(),
        }
    }

    fn window_payload(event: &ExtensionModelEvent) -> Option<(&'static str, serde_json::Value)> {
        match event {
            ExtensionModelEvent::WindowCreated(window) => Some((
                "onCreated",
                serde_json::json!([serde_json::to_value(window).expect("window serializes")]),
            )),
            ExtensionModelEvent::WindowRemoved { window_id } => {
                Some(("onRemoved", serde_json::json!([window_id])))
            }
            ExtensionModelEvent::WindowFocusChanged { window_id } => {
                Some(("onFocusChanged", serde_json::json!([window_id])))
            }
            ExtensionModelEvent::WindowBoundsChanged(window) => Some((
                "onBoundsChanged",
                serde_json::json!([serde_json::to_value(window).expect("window serializes")]),
            )),
            _ => None,
        }
    }
}

#[derive(Component, Clone, Debug)]
struct CachedBridgeResponse {
    request_id: String,
    frame_json: String,
    order: u64,
}

impl CachedBridgeResponse {
    fn decode(&self) -> Option<BridgeServerMessage> {
        serde_json::from_str(&self.frame_json).ok()
    }
}

#[derive(Component, Clone, Debug)]
struct SeenBridgeRequest {
    session_id: u64,
    request_id: String,
    order: u64,
}

#[derive(Component, Clone, Copy, Debug)]
struct ConformanceWake {
    deadline: Option<Instant>,
}

#[derive(Component)]
struct ConformanceWakeScheduler {
    delay: Duration,
    scheduler: Option<crossbeam_channel::Sender<Instant>>,
    worker: Option<JoinHandle<()>>,
}

impl Default for ConformanceWakeScheduler {
    fn default() -> Self {
        Self {
            delay: Duration::from_secs(35),
            scheduler: None,
            worker: None,
        }
    }
}

impl ConformanceWakeScheduler {
    fn schedule(&mut self, proxy: Option<&EventLoopProxyWrapper>) -> Option<Instant> {
        let deadline = Instant::now() + self.delay;
        if self.scheduler.is_none() {
            let Some(proxy) = proxy else {
                bevy::log::warn!("extension conformance wake has no event-loop proxy");
                return None;
            };
            let (sender, receiver) = crossbeam_channel::unbounded();
            let proxy = (**proxy).clone();
            match std::thread::Builder::new()
                .name("extension-conformance-wake".into())
                .spawn(move || {
                    let mut deadlines = BinaryHeap::<Reverse<Instant>>::new();
                    loop {
                        let result = deadlines.peek().map_or_else(
                            || receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
                            |deadline| {
                                receiver.recv_timeout(
                                    deadline.0.saturating_duration_since(Instant::now()),
                                )
                            },
                        );
                        match result {
                            Ok(deadline) => deadlines.push(Reverse(deadline)),
                            Err(RecvTimeoutError::Timeout) => {
                                let now = Instant::now();
                                while deadlines.peek().is_some_and(|d| d.0 <= now) {
                                    deadlines.pop();
                                }
                                let _ = proxy.send_event(WinitUserEvent::WakeUp);
                            }
                            Err(RecvTimeoutError::Disconnected) => break,
                        }
                    }
                }) {
                Ok(worker) => {
                    self.scheduler = Some(sender);
                    self.worker = Some(worker);
                }
                Err(error) => {
                    bevy::log::warn!(
                        "failed to start extension conformance wake scheduler: {error}"
                    );
                    return None;
                }
            }
        }
        let sender = self.scheduler.as_ref()?;
        if let Err(error) = sender.send(deadline) {
            self.scheduler = None;
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            bevy::log::warn!("failed to schedule extension conformance wake: {error}");
            return None;
        }
        Some(deadline)
    }
}

impl Drop for ConformanceWakeScheduler {
    fn drop(&mut self) {
        self.scheduler = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(SystemParam)]
struct BrokerRecords<'w, 's> {
    subscriptions: Query<'w, 's, (Entity, &'static crate::Id, &'static mut BridgeSubscription)>,
    pending: Query<'w, 's, (Entity, &'static crate::Id, &'static PendingBridgeEvent)>,
    cached: Query<'w, 's, (Entity, &'static crate::Id, &'static CachedBridgeResponse)>,
    seen: Query<'w, 's, (Entity, &'static crate::Id, &'static SeenBridgeRequest)>,
    wakes: Query<'w, 's, (Entity, &'static crate::Id), With<ConformanceWake>>,
    stack: MessageWriter<'w, vmux_layout::stack::OpenRequest>,
    commands: Commands<'w, 's>,
}

impl BrokerRecords<'_, '_> {
    fn prune_stale_sessions(&mut self, extension_id: &str, session_id: u64) {
        for (entity, id, seen) in &self.seen {
            if id.0 == extension_id && seen.session_id != session_id {
                self.commands.entity(entity).despawn();
            }
        }
    }

    fn cached_response(&self, extension_id: &str, request_id: &str) -> Option<BridgeServerMessage> {
        for (_, id, cached) in &self.cached {
            if id.0 == extension_id && cached.request_id == request_id {
                return cached.decode();
            }
        }
        None
    }

    fn has_seen(&self, extension_id: &str, session_id: u64, request_id: &str) -> bool {
        self.seen.iter().any(|(_, id, seen)| {
            id.0 == extension_id && seen.session_id == session_id && seen.request_id == request_id
        })
    }

    fn record_seen(&mut self, extension_id: &str, session_id: u64, request_id: String, order: u64) {
        let mut count = 0;
        let mut oldest: Option<(u64, Entity)> = None;
        for (entity, id, seen) in &self.seen {
            if id.0 == extension_id && seen.session_id == session_id {
                count += 1;
                if oldest.is_none_or(|(best, _)| seen.order < best) {
                    oldest = Some((seen.order, entity));
                }
            }
        }
        if count >= BridgeOrder::MAX_SEEN_REQUESTS
            && let Some((_, entity)) = oldest
        {
            self.commands.entity(entity).despawn();
        }
        self.commands.spawn((
            Name::new("Extension bridge seen request"),
            crate::Id(extension_id.into()),
            SeenBridgeRequest {
                session_id,
                request_id,
                order,
            },
        ));
    }

    fn record_cached_response(
        &mut self,
        extension_id: &str,
        request_id: String,
        response: BridgeServerMessage,
        order: u64,
    ) {
        let mut count = 0;
        let mut oldest: Option<(u64, Entity)> = None;
        for (entity, id, cached) in &self.cached {
            if id.0 == extension_id {
                count += 1;
                if oldest.is_none_or(|(best, _)| cached.order < best) {
                    oldest = Some((cached.order, entity));
                }
            }
        }
        if count >= BridgeOrder::MAX_CACHED_RESPONSES
            && let Some((_, entity)) = oldest
        {
            self.commands.entity(entity).despawn();
        }
        let Ok(frame_json) = serde_json::to_string(&response) else {
            return;
        };
        self.commands.spawn((
            Name::new("Extension bridge cached response"),
            crate::Id(extension_id.into()),
            CachedBridgeResponse {
                request_id,
                frame_json,
                order,
            },
        ));
    }

    fn resend_pending(&self, server: &ExtensionBridgeServer, extension_id: &str) {
        let mut events = Vec::new();
        for (_, id, pending) in &self.pending {
            if id.0 == extension_id {
                events.push(pending);
            }
        }
        events.sort_unstable_by_key(|event| event.sequence);
        for event in events {
            if let Err(error) = server.send(
                extension_id,
                BridgeServerMessage::Event(event.to_api_event()),
            ) {
                bevy::log::warn!("failed to resend extension bridge event: {error}");
                break;
            }
        }
    }

    fn upsert_subscription(
        &mut self,
        extension_id: &str,
        stored: BridgeSubscription,
    ) -> Result<(), ExtensionApiError> {
        let mut count = 0;
        for (_, id, mut existing) in &mut self.subscriptions {
            if id.0 != extension_id {
                continue;
            }
            if existing.subscription_id == stored.subscription_id {
                *existing = stored;
                return Ok(());
            }
            count += 1;
        }
        if count >= BridgeOrder::MAX_SUBSCRIPTIONS {
            return Err(ExtensionApiError::new(
                "subscription_limit",
                "extension bridge subscription limit reached",
            ));
        }
        self.commands.spawn((
            Name::new("Extension bridge subscription"),
            crate::Id(extension_id.into()),
            stored,
        ));
        Ok(())
    }

    fn remove_subscription(&mut self, extension_id: &str, subscription_id: &str, context_id: &str) {
        for (entity, id, subscription) in &self.subscriptions {
            if id.0 == extension_id
                && subscription.subscription_id == subscription_id
                && subscription.context_id == context_id
            {
                self.commands.entity(entity).despawn();
            }
        }
    }

    fn acknowledge_event(&mut self, extension_id: &str, sequence: u64) {
        for (entity, id, pending) in &self.pending {
            if id.0 == extension_id && pending.sequence == sequence {
                self.commands.entity(entity).despawn();
            }
        }
    }

    fn has_wake(&self, extension_id: &str) -> bool {
        self.wakes.iter().any(|(_, id)| id.0 == extension_id)
    }

    fn spawn_wake(&mut self, extension_id: &str, deadline: Instant) {
        self.commands.spawn((
            Name::new("Extension conformance wake"),
            crate::Id(extension_id.into()),
            ConformanceWake {
                deadline: Some(deadline),
            },
        ));
    }

    fn handle_api_request(
        &mut self,
        server: &ExtensionBridgeServer,
        session: (&str, u64),
        request: ApiRequest,
        context: (&ExtensionModel, &CapabilityMatrix),
        order_sequence: &mut SequenceCounter<BridgeOrder>,
        virtual_windows: &mut VirtualWindows,
    ) {
        let (extension_id, session_id) = session;
        let (model, matrix) = context;
        let request_id = request.request_id.clone();
        if let Err(error) = Self::authorize_api_request(server, extension_id, &request, model) {
            let response = BridgeServerMessage::Response(ApiResponse::failure(request_id, error));
            if let Err(error) = server.send_to_session(extension_id, session_id, response) {
                bevy::log::warn!("failed to send extension bridge response: {error}");
            }
            return;
        }
        let Some(authorization) = server.authorization(extension_id) else {
            Self::send_fatal_to_session(
                server,
                extension_id,
                session_id,
                "unauthorized_extension",
                "extension authorization is unavailable",
            );
            return;
        };
        if let Some(response) = self.cached_response(extension_id, &request_id) {
            if let Err(error) = server.send_to_session(extension_id, session_id, response) {
                bevy::log::warn!("failed to send cached extension bridge response: {error}");
            }
            return;
        }
        if self.has_seen(extension_id, session_id, &request_id) {
            let duplicate = BridgeServerMessage::Response(ApiResponse::failure(
                request_id,
                ExtensionApiError::new("duplicate_request", "duplicate bridge request id"),
            ));
            if let Err(error) = server.send_to_session(extension_id, session_id, duplicate) {
                bevy::log::warn!("failed to send extension bridge response: {error}");
            }
            return;
        }
        let seen_order = order_sequence.next();
        self.record_seen(extension_id, session_id, request_id.clone(), seen_order);
        let response = self.dispatch_api_request(
            matrix,
            request,
            model,
            virtual_windows,
            authorization,
            ExtensionEnvironment::conformance_enabled(),
        );
        let cache_order = order_sequence.next();
        self.record_cached_response(extension_id, request_id, response.clone(), cache_order);
        if let Err(error) = server.send_to_session(extension_id, session_id, response) {
            bevy::log::warn!("failed to send extension bridge response: {error}");
        }
    }

    fn handle_subscribe(
        &mut self,
        server: &ExtensionBridgeServer,
        session: (&str, u64),
        subscription: EventSubscribe,
        model: &ExtensionModel,
        wake_timer: Option<&mut ConformanceWakeScheduler>,
        proxy: Option<&EventLoopProxyWrapper>,
    ) {
        let (extension_id, session_id) = session;
        let caller = match Self::validate_caller_context(
            extension_id,
            &subscription.caller_context,
            model,
        ) {
            Ok(caller) => caller,
            Err(error) => {
                Self::send_fatal_to_session(
                    server,
                    extension_id,
                    session_id,
                    &error.code,
                    &error.message,
                );
                return;
            }
        };
        let conformance_subscription = subscription.namespace == BridgeOrder::CONFORMANCE_NAMESPACE
            && subscription.event == BridgeOrder::MODEL_CHANGED_EVENT
            && wake_timer.is_some()
            && server
                .authorization(extension_id)
                .is_some_and(|authorization| authorization.conformance);
        let windows_subscription = subscription.namespace == "windows"
            && matches!(
                subscription.event.as_str(),
                "onCreated" | "onRemoved" | "onFocusChanged" | "onBoundsChanged"
            );
        if !conformance_subscription && !windows_subscription {
            Self::send_fatal_to_session(
                server,
                extension_id,
                session_id,
                "unsupported_event",
                "bridge event is not supported",
            );
            return;
        }
        let stored = BridgeSubscription {
            context_id: caller.context_id().to_string(),
            subscription_id: subscription.subscription_id,
            namespace: subscription.namespace,
            event: subscription.event,
        };
        if let Err(error) = self.upsert_subscription(extension_id, stored) {
            Self::send_fatal_to_session(
                server,
                extension_id,
                session_id,
                &error.code,
                &error.message,
            );
            return;
        }
        self.resend_pending(server, extension_id);
        if !conformance_subscription || self.has_wake(extension_id) {
            return;
        }
        if let Some(timer) = wake_timer
            && let Some(deadline) = timer.schedule(proxy)
        {
            self.spawn_wake(extension_id, deadline);
        }
    }

    fn dispatch_api_request(
        &mut self,
        matrix: &CapabilityMatrix,
        request: ApiRequest,
        model: &ExtensionModel,
        virtual_windows: &mut VirtualWindows,
        authorization: &BridgeAuthorization,
        conformance_enabled: bool,
    ) -> BridgeServerMessage {
        if request.namespace == BridgeOrder::CONFORMANCE_NAMESPACE {
            if request.method == "snapshot" && conformance_enabled {
                return match serde_json::to_value(model) {
                    Ok(snapshot) => BridgeServerMessage::Response(ApiResponse::success(
                        request.request_id,
                        snapshot,
                    )),
                    Err(error) => BridgeServerMessage::Response(ApiResponse::failure(
                        request.request_id,
                        ExtensionApiError::new("serialization_failed", error.to_string()),
                    )),
                };
            }
            return BridgeServerMessage::Response(ApiResponse::failure(
                request.request_id,
                ExtensionApiError::new("unsupported_api", "reserved conformance API is disabled"),
            ));
        }
        if request.namespace == "windows" {
            return match virtual_windows.dispatch(&request, model, authorization) {
                Ok((result, _)) => {
                    BridgeServerMessage::Response(ApiResponse::success(request.request_id, result))
                }
                Err(error) => {
                    BridgeServerMessage::Response(ApiResponse::failure(request.request_id, error))
                }
            };
        }
        if matches!(
            (request.namespace.as_str(), request.method.as_str()),
            ("tabs", "query" | "get")
        ) {
            return match super::tabs::ExtensionTabs::from(model).dispatch(&request, authorization) {
                Ok(result) => {
                    BridgeServerMessage::Response(ApiResponse::success(request.request_id, result))
                }
                Err(error) => {
                    BridgeServerMessage::Response(ApiResponse::failure(request.request_id, error))
                }
            };
        }
        if matches!(
            (request.namespace.as_str(), request.method.as_str()),
            ("tabs", "create")
        ) {
            return match Self::create_page_request(&request) {
                Ok(stack_request) => {
                    self.stack.write(stack_request);
                    BridgeServerMessage::Response(ApiResponse::success(
                        request.request_id,
                        serde_json::Value::Null,
                    ))
                }
                Err(error) => {
                    BridgeServerMessage::Response(ApiResponse::failure(request.request_id, error))
                }
            };
        }
        Self::unsupported_capability_response(matrix, &request)
    }

    fn unsupported_capability_response(
        matrix: &CapabilityMatrix,
        request: &ApiRequest,
    ) -> BridgeServerMessage {
        let member = format!("{}.{}", request.namespace, request.method);
        let Some(capability) = matrix.lookup(
            ExtensionEnvironment::platform(),
            &request.namespace,
            &request.method,
            CapabilityKind::Method,
        ) else {
            return BridgeServerMessage::Response(ApiResponse::failure(
                request.request_id.clone(),
                ExtensionApiError::new(
                    "unsupported_api",
                    format!(
                        "{member} is not listed for Chromium {} on {}",
                        matrix.chromium_major,
                        ExtensionEnvironment::platform()
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
            request.request_id.clone(),
            ExtensionApiError::new(
                code,
                format!(
                    "{member} is {status} for Chromium {} on {}",
                    matrix.chromium_major,
                    ExtensionEnvironment::platform()
                ),
            ),
        ))
    }

    fn create_page_request(
        request: &ApiRequest,
    ) -> Result<vmux_layout::stack::OpenRequest, ExtensionApiError> {
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
            return Ok(vmux_layout::stack::OpenRequest {
                url: None,
                policy: vmux_layout::stack::OpenPolicy::ReplaceStartup,
            });
        };
        let parsed = url::Url::parse(url)
            .map_err(|_| ExtensionApiError::new("invalid_url", "extension page URL is invalid"))?;
        match parsed.scheme() {
            "http" | "https" => {}
            "chrome-extension"
                if parsed.host_str() == Some(request.caller_context.extension_id()) => {}
            _ => {
                return Err(ExtensionApiError::new(
                    "invalid_url",
                    "extension page URL uses an unsupported scheme",
                ));
            }
        }
        Ok(vmux_layout::stack::OpenRequest {
            url: Some(url.to_string()),
            policy: vmux_layout::stack::OpenPolicy::ReplaceStartup,
        })
    }

    fn authorize_api_request(
        server: &ExtensionBridgeServer,
        extension_id: &str,
        request: &ApiRequest,
        model: &ExtensionModel,
    ) -> Result<(), ExtensionApiError> {
        Self::validate_caller_context(extension_id, &request.caller_context, model)?;
        if request.request_id.is_empty() || request.request_id.len() > 128 {
            return Err(ExtensionApiError::new(
                "invalid_request",
                "extension request id is invalid",
            ));
        }
        let authorization = server.authorization(extension_id).ok_or_else(|| {
            ExtensionApiError::new(
                "permission_denied",
                "extension authorization is unavailable",
            )
        })?;
        if request.namespace == BridgeOrder::CONFORMANCE_NAMESPACE {
            if ExtensionEnvironment::conformance_enabled() && authorization.conformance {
                return Ok(());
            }
            return Err(ExtensionApiError::new(
                "permission_denied",
                "reserved conformance API is not authorized for this extension",
            ));
        }
        if let Some(permission) =
            Self::required_api_permission(&request.namespace, &request.method)?
            && !authorization.permissions.contains(permission)
        {
            return Err(ExtensionApiError::new(
                "permission_denied",
                format!("{} requires the {permission} permission", request.namespace),
            ));
        }
        if Self::requires_host_permission(&request.namespace, &request.method) {
            Self::verify_host_permissions(authorization, request, model)?;
        }
        Ok(())
    }

    fn verify_host_permissions(
        authorization: &BridgeAuthorization,
        request: &ApiRequest,
        model: &ExtensionModel,
    ) -> Result<(), ExtensionApiError> {
        if authorization.host_permissions.is_empty() {
            return Err(ExtensionApiError::new(
                "host_permission_denied",
                format!(
                    "{}.{} requires a host permission",
                    request.namespace, request.method
                ),
            ));
        }
        let urls = Self::request_target_urls(request, model)?;
        if urls.is_empty() {
            return Err(ExtensionApiError::new(
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
            return Err(ExtensionApiError::new(
                "host_permission_denied",
                format!(
                    "{}.{} is not allowed for the requested host",
                    request.namespace, request.method
                ),
            ));
        }
        Ok(())
    }

    fn validate_caller_context<'a>(
        extension_id: &str,
        caller: &'a ExtensionCallerContext,
        model: &ExtensionModel,
    ) -> Result<&'a ExtensionCallerContext, ExtensionApiError> {
        if caller.extension_id() != extension_id || caller.context_id().is_empty() {
            return Err(ExtensionApiError::new(
                "invalid_context",
                "extension caller context is not authorized",
            ));
        }
        if matches!(
            caller,
            ExtensionCallerContext::ServiceWorker { .. }
                | ExtensionCallerContext::ExtensionPage { .. }
        ) && caller
            .url()
            .is_some_and(|url| !url.starts_with(&format!("chrome-extension://{extension_id}/")))
        {
            return Err(ExtensionApiError::new(
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
            return Err(ExtensionApiError::new(
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
                return Err(ExtensionApiError::new(
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
    ) -> Result<Option<&'static str>, ExtensionApiError> {
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
                return Err(ExtensionApiError::new(
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
        model: &ExtensionModel,
    ) -> Result<Vec<url::Url>, ExtensionApiError> {
        let mut urls = Vec::new();
        let mut tab_ids = Vec::new();
        Self::collect_request_targets(&request.arguments, &mut urls, &mut tab_ids, 0);
        if matches!(
            (request.namespace.as_str(), request.method.as_str()),
            ("tabs", "captureVisibleTab")
        ) {
            for tab in &model.tabs {
                if tab.active {
                    tab_ids.push(i64::from(tab.id));
                }
            }
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
                return Err(ExtensionApiError::new(
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
                    Self::collect_request_targets(value, urls, tab_ids, depth + 1);
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
                        Self::collect_request_targets(value, urls, tab_ids, depth + 1);
                    }
                }
            }
            _ => {}
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
            BridgeServerMessage::Fatal(ExtensionApiError::new(code, message)),
        ) {
            bevy::log::warn!("failed to send extension bridge error: {error}");
        }
    }
}

#[derive(SystemParam)]
struct PendingEventDispatch<'w, 's> {
    sequence: Single<'w, 's, &'static mut SequenceCounter<BridgeEvent>>,
    pending: Query<'w, 's, &'static crate::Id, With<PendingBridgeEvent>>,
    commands: Commands<'w, 's>,
}

impl PendingEventDispatch<'_, '_> {
    fn queue(
        &mut self,
        server: &ExtensionBridgeServer,
        extension_id: &str,
        namespace: &str,
        event_name: &str,
        arguments: serde_json::Value,
    ) {
        let count = self
            .pending
            .iter()
            .filter(|id| id.0 == extension_id)
            .count();
        if count >= BridgeEvent::MAX_PENDING_EVENTS {
            bevy::log::error!("extension bridge event queue overflow for extension {extension_id}");
            if let Err(error) = server.send(
                extension_id,
                BridgeServerMessage::Fatal(ExtensionApiError::new(
                    "event_queue_overflow",
                    "extension bridge event queue exceeded 256 entries",
                )),
            ) {
                bevy::log::warn!("failed to send extension bridge error: {error}");
            }
            return;
        }
        let sequence = self.sequence.next();
        let event = ApiEvent {
            sequence,
            namespace: namespace.into(),
            event: event_name.into(),
            arguments: arguments.clone(),
        };
        self.commands.spawn((
            Name::new("Extension bridge pending event"),
            crate::Id(extension_id.into()),
            PendingBridgeEvent {
                sequence,
                namespace: namespace.into(),
                event: event_name.into(),
                arguments,
            },
        ));
        if let Err(error) = server.send(extension_id, BridgeServerMessage::Event(event)) {
            bevy::log::warn!("failed to send extension bridge event: {error}");
        }
    }
}

fn spawn(manifests: Query<&vmux_feature::manifest::FeatureManifest>, mut commands: Commands) {
    let mut matrix = None;
    for manifest in &manifests {
        let candidate = CapabilityMatrix::from_manifest(manifest)
            .expect("browser feature manifest contains a valid extension policy");
        if candidate.is_some() {
            matrix = candidate;
            break;
        }
    }
    let matrix = matrix.expect("browser feature manifest has an extension policy");
    let mut entity = commands.spawn((
        Name::new("Extension broker"),
        SequenceCounter::<BridgeEvent>::starting_at(1),
        SequenceCounter::<BridgeOrder>::starting_at(1),
        matrix,
    ));
    if ExtensionEnvironment::conformance_enabled() {
        entity.insert(ConformanceWakeScheduler::default());
    }
}

fn arm_bridge_wake(
    servers: Query<(Entity, &ExtensionBridgeServer), Without<BridgeWakeArmed>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some(proxy) = proxy else {
        return;
    };
    for (entity, server) in &servers {
        server.arm_wake(&proxy);
        commands.entity(entity).insert(BridgeWakeArmed);
    }
}

fn revoke_bridge_state(
    mut revoked: MessageReader<BridgeAuthorizationRevoked>,
    subscriptions: Query<(Entity, &crate::Id), With<BridgeSubscription>>,
    pending: Query<(Entity, &crate::Id), With<PendingBridgeEvent>>,
    cached: Query<(Entity, &crate::Id), With<CachedBridgeResponse>>,
    seen: Query<(Entity, &crate::Id), With<SeenBridgeRequest>>,
    wakes: Query<(Entity, &crate::Id), With<ConformanceWake>>,
    mut commands: Commands,
) {
    let mut ids = HashSet::new();
    for event in revoked.read() {
        ids.insert(event.0.as_str());
    }
    if ids.is_empty() {
        return;
    }
    for (entity, id) in &subscriptions {
        if ids.contains(id.0.as_str()) {
            commands.entity(entity).despawn();
        }
    }
    for (entity, id) in &pending {
        if ids.contains(id.0.as_str()) {
            commands.entity(entity).despawn();
        }
    }
    for (entity, id) in &cached {
        if ids.contains(id.0.as_str()) {
            commands.entity(entity).despawn();
        }
    }
    for (entity, id) in &seen {
        if ids.contains(id.0.as_str()) {
            commands.entity(entity).despawn();
        }
    }
    for (entity, id) in &wakes {
        if ids.contains(id.0.as_str()) {
            commands.entity(entity).despawn();
        }
    }
}

fn drain_bridge_requests(
    server: Single<&ExtensionBridgeServer>,
    broker: Single<(
        &mut SequenceCounter<BridgeOrder>,
        &CapabilityMatrix,
        Option<&mut ConformanceWakeScheduler>,
    )>,
    model: Single<&ExtensionModel>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut virtual_windows: VirtualWindows,
    mut records: BrokerRecords,
) {
    let (mut order_sequence, matrix, mut wake_timer) = broker.into_inner();
    for _ in 0..BridgeOrder::MAX_MESSAGES_PER_UPDATE {
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
        if server.authorization(&extension_id).is_none() {
            BrokerRecords::send_fatal_to_session(
                &server,
                &extension_id,
                session_id,
                "unauthorized_extension",
                "extension association or runtime is no longer authorized",
            );
            continue;
        }
        records.prune_stale_sessions(&extension_id, session_id);
        if context_id != crate::protocol::BRIDGE_CONTEXT_ID
            || context_kind != crate::protocol::ExtensionContextKind::BridgePage
        {
            BrokerRecords::send_fatal_to_session(
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
                records.handle_api_request(
                    &server,
                    (&extension_id, session_id),
                    request,
                    (&model, matrix),
                    &mut order_sequence,
                    &mut virtual_windows,
                );
            }
            BridgeClientMessage::Subscribe(subscription) => {
                records.handle_subscribe(
                    &server,
                    (&extension_id, session_id),
                    subscription,
                    &model,
                    wake_timer.as_deref_mut(),
                    proxy.as_deref(),
                );
            }
            BridgeClientMessage::Unsubscribe {
                subscription_id,
                caller_context,
            } => {
                if let Ok(caller) =
                    BrokerRecords::validate_caller_context(&extension_id, &caller_context, &model)
                {
                    records.remove_subscription(
                        &extension_id,
                        &subscription_id,
                        caller.context_id(),
                    );
                }
            }
            BridgeClientMessage::Ack { sequence } => {
                records.acknowledge_event(&extension_id, sequence);
            }
            BridgeClientMessage::Hello(_) => BrokerRecords::send_fatal_to_session(
                &server,
                &extension_id,
                session_id,
                "protocol_error",
                "unexpected bridge message",
            ),
        }
        if let Some(proxy) = proxy.as_deref() {
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
        }
        break;
    }
}

fn forward_model_events(
    mut events: MessageReader<ExtensionModelEvent>,
    subscriptions: Query<(&crate::Id, &BridgeSubscription)>,
    server: Single<&ExtensionBridgeServer>,
    mut pending: PendingEventDispatch,
) {
    for event in events.read() {
        let mut conformance_sent = HashSet::new();
        let mut window_sent = HashSet::new();
        let window_payload = PendingBridgeEvent::window_payload(event);
        for (id, subscription) in &subscriptions {
            let extension_id = id.0.as_str();
            if server.authorization(extension_id).is_none() {
                continue;
            }
            if subscription.namespace == BridgeOrder::CONFORMANCE_NAMESPACE
                && subscription.event == BridgeOrder::MODEL_CHANGED_EVENT
                && conformance_sent.insert(extension_id)
            {
                let Ok(value) = serde_json::to_value(event) else {
                    bevy::log::error!("failed to serialize extension model event");
                    continue;
                };
                pending.queue(
                    &server,
                    extension_id,
                    BridgeOrder::CONFORMANCE_NAMESPACE,
                    BridgeOrder::MODEL_CHANGED_EVENT,
                    serde_json::json!([value]),
                );
            }
            if let Some((event_name, arguments)) = &window_payload
                && subscription.namespace == "windows"
                && subscription.event == *event_name
                && window_sent.insert(extension_id)
            {
                pending.queue(
                    &server,
                    extension_id,
                    "windows",
                    event_name,
                    arguments.clone(),
                );
            }
        }
    }
}

fn fire_conformance_wake_timer(
    mut wakes: Query<(&crate::Id, &mut ConformanceWake)>,
    model: Single<&ExtensionModel>,
    server: Single<&ExtensionBridgeServer>,
    mut pending: PendingEventDispatch,
) {
    let now = Instant::now();
    for (id, mut wake) in &mut wakes {
        let Some(deadline) = wake.deadline else {
            continue;
        };
        if deadline > now {
            continue;
        }
        wake.deadline = None;
        let Ok(snapshot) = serde_json::to_value(*model) else {
            bevy::log::error!("failed to serialize extension model snapshot");
            continue;
        };
        pending.queue(
            &server,
            &id.0,
            BridgeOrder::CONFORMANCE_NAMESPACE,
            BridgeOrder::MODEL_CHANGED_EVENT,
            serde_json::json!([snapshot]),
        );
    }
}

pub(crate) struct ExtensionEnvironment;

impl ExtensionEnvironment {
    pub(crate) fn conformance_enabled() -> bool {
        cfg!(feature = "conformance")
            && std::env::var("VMUX_EXTENSION_CONFORMANCE").ok().as_deref() == Some("1")
    }

    pub(crate) const fn platform() -> &'static str {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::bridge::{BridgeAuthorization, BridgeRegistration, ExtensionBridgeServer};
    use crate::host::manager_page::popup;
    use crate::host::windows::ExtensionWindowsPlugin;
    use crate::protocol::{
        ApiRequest, ApiResponse, BRIDGE_PROTOCOL_VERSION, BridgeClientMessage, BridgeHello,
        BridgeServerMessage, EventSubscribe, ExtensionApiError, ExtensionCallerContext,
        ExtensionContextKind,
    };
    use bevy::ecs::system::SystemState;
    use std::time::Duration;
    use tungstenite::{
        Message, WebSocket, client::IntoClientRequest, connect, http::HeaderValue,
        stream::MaybeTlsStream,
    };
    use vmux_layout::stack::CloseStackRequest;

    const EXTENSION_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct Fixture {
        app: App,
    }

    impl Fixture {
        fn new(server: ExtensionBridgeServer, scheduler: Option<ConformanceWakeScheduler>) -> Self {
            let mut app = App::new();
            app.add_plugins((ExtensionWindowsPlugin, ExtensionBrokerPlugin))
                .add_message::<BridgeAuthorizationRevoked>()
                .add_message::<vmux_layout::stack::OpenRequest>()
                .add_message::<CloseStackRequest>()
                .add_message::<popup::OpenRequest>()
                .add_message::<popup::CloseRequest>()
                .add_message::<ExtensionModelEvent>();
            app.world_mut().spawn(ExtensionModel::default());
            app.update();
            if let Some(scheduler) = scheduler {
                let broker = app
                    .world_mut()
                    .query_filtered::<Entity, With<SequenceCounter<BridgeOrder>>>()
                    .single(app.world())
                    .unwrap();
                app.world_mut().entity_mut(broker).insert(scheduler);
            }
            app.world_mut().spawn(server);
            Self { app }
        }

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

        fn server(&mut self) -> &ExtensionBridgeServer {
            self.app
                .world_mut()
                .query::<&ExtensionBridgeServer>()
                .single(self.app.world())
                .unwrap()
        }

        fn connect(
            server: &ExtensionBridgeServer,
        ) -> WebSocket<MaybeTlsStream<std::net::TcpStream>> {
            let identity = server.identity(EXTENSION_ID).unwrap().clone();
            let mut request = server.endpoint().into_client_request().unwrap();
            request.headers_mut().insert(
                "origin",
                HeaderValue::from_str(&format!("chrome-extension://{EXTENSION_ID}")).unwrap(),
            );
            let (mut socket, _) = connect(request).unwrap();
            Self::send(
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
                Self::read(&mut socket),
                BridgeServerMessage::Ready { .. }
            ));
            socket
        }

        fn connect_socket(
            server: &ExtensionBridgeServer,
        ) -> WebSocket<MaybeTlsStream<std::net::TcpStream>> {
            let mut request = server.endpoint().into_client_request().unwrap();
            request.headers_mut().insert(
                "origin",
                HeaderValue::from_str(&format!("chrome-extension://{EXTENSION_ID}")).unwrap(),
            );
            connect(request).unwrap().0
        }

        fn send(
            socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
            message: &BridgeClientMessage,
        ) {
            socket
                .send(Message::Text(
                    serde_json::to_string(message).unwrap().into(),
                ))
                .unwrap();
        }

        fn read(
            socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
        ) -> BridgeServerMessage {
            match socket.read().unwrap() {
                Message::Text(text) => serde_json::from_str(&text).unwrap(),
                message => panic!("unexpected bridge frame: {message:?}"),
            }
        }

        fn pump(&mut self) {
            for _ in 0..20 {
                self.app.update();
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        fn dispatch_direct(
            &mut self,
            request: ApiRequest,
            authorization: &BridgeAuthorization,
            conformance_enabled: bool,
        ) -> BridgeServerMessage {
            let model = self
                .app
                .world_mut()
                .query::<&ExtensionModel>()
                .single(self.app.world())
                .unwrap()
                .clone();
            let matrix = self
                .app
                .world_mut()
                .query::<&CapabilityMatrix>()
                .single(self.app.world())
                .unwrap()
                .clone();
            let mut state =
                SystemState::<(VirtualWindows, BrokerRecords)>::new(self.app.world_mut());
            let (mut windows, mut records) = state.get_mut(self.app.world_mut()).unwrap();
            let response = records.dispatch_api_request(
                &matrix,
                request,
                &model,
                &mut windows,
                authorization,
                conformance_enabled,
            );
            state.apply(self.app.world_mut());
            response
        }
    }

    #[test]
    fn window_create_dispatches_extension_page_through_app_command() {
        let mut fixture = Fixture::new(Fixture::conformance_server(), None);
        let request = ApiRequest {
            request_id: "window-create".into(),
            namespace: "windows".into(),
            method: "create".into(),
            arguments: serde_json::json!([{
                "url": format!("chrome-extension://{EXTENSION_ID}/popup/index.html")
            }]),
            caller_context: Fixture::caller_context(),
        };

        let response = fixture.dispatch_direct(request, &BridgeAuthorization::default(), false);

        let open = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<crate::host::windows::OpenExtensionWindowRequest>>()
            .drain()
            .next()
            .unwrap();
        assert_eq!(open.window_type, "normal");
        assert_eq!(
            open.urls,
            vec![Some(format!(
                "chrome-extension://{EXTENSION_ID}/popup/index.html"
            ))]
        );
        let BridgeServerMessage::Response(response) = response else {
            panic!("expected response");
        };
        assert_eq!(response.request_id, "window-create");
        assert!(response.result.unwrap()["id"].as_i64().is_some());
    }

    #[test]
    fn window_create_rejects_other_extension_origin() {
        let mut fixture = Fixture::new(Fixture::conformance_server(), None);
        let request = ApiRequest {
            request_id: "window-create".into(),
            namespace: "windows".into(),
            method: "create".into(),
            arguments: serde_json::json!([{
                "url": "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/popup.html"
            }]),
            caller_context: Fixture::caller_context(),
        };

        let response = fixture.dispatch_direct(request, &BridgeAuthorization::default(), false);

        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<vmux_layout::stack::OpenRequest>>()
                .is_empty()
        );
        assert_eq!(
            response,
            BridgeServerMessage::Response(ApiResponse::failure(
                "window-create",
                ExtensionApiError::new("invalid_url", "window URL uses an unsupported scheme"),
            ))
        );
    }

    #[test]
    fn rejects_untested_api_request() {
        let server = Fixture::conformance_server();
        let identity = server.identity(EXTENSION_ID).unwrap().clone();
        let mut socket = Fixture::connect_socket(&server);
        Fixture::send(
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
            Fixture::read(&mut socket),
            BridgeServerMessage::Ready { .. }
        ));
        Fixture::send(
            &mut socket,
            &BridgeClientMessage::ApiRequest(ApiRequest {
                request_id: "r1".into(),
                namespace: "runtime".into(),
                method: "sendMessage".into(),
                arguments: serde_json::json!({}),
                caller_context: Fixture::caller_context(),
            }),
        );

        let mut fixture = Fixture::new(server, None);
        fixture.pump();

        let response = Fixture::read(&mut socket);
        assert_eq!(
            response,
            BridgeServerMessage::Response(ApiResponse::failure(
                "r1",
                ExtensionApiError::new(
                    "unsupported_api",
                    format!(
                        "runtime.sendMessage is Untested for Chromium 148 on {}",
                        ExtensionEnvironment::platform()
                    )
                )
            ))
        );

        socket.close(None).unwrap();
        drop(socket);
        std::thread::sleep(Duration::from_millis(50));
        let mut restarted = Fixture::connect(fixture.server());
        Fixture::send(
            &mut restarted,
            &BridgeClientMessage::ApiRequest(ApiRequest {
                request_id: "r1".into(),
                namespace: "tabs".into(),
                method: "query".into(),
                arguments: serde_json::json!({}),
                caller_context: Fixture::caller_context(),
            }),
        );
        fixture.pump();

        assert_eq!(Fixture::read(&mut restarted), response);
        let cached_count = fixture
            .app
            .world_mut()
            .query::<(&crate::Id, &CachedBridgeResponse)>()
            .iter(fixture.app.world())
            .filter(|(id, _)| id.0 == EXTENSION_ID)
            .count();
        assert_eq!(cached_count, 1);
    }

    #[test]
    fn conformance_snapshot_requires_gate() {
        let mut fixture = Fixture::new(Fixture::conformance_server(), None);
        let model = ExtensionModel::default();
        let request = || ApiRequest {
            request_id: "snapshot".into(),
            namespace: BridgeOrder::CONFORMANCE_NAMESPACE.into(),
            method: "snapshot".into(),
            arguments: serde_json::json!({}),
            caller_context: Fixture::caller_context(),
        };

        let enabled = fixture.dispatch_direct(request(), &BridgeAuthorization::default(), true);
        assert_eq!(
            enabled,
            BridgeServerMessage::Response(ApiResponse::success(
                "snapshot",
                serde_json::to_value(&model).unwrap()
            ))
        );
        let disabled = fixture.dispatch_direct(request(), &BridgeAuthorization::default(), false);
        assert_eq!(
            disabled,
            BridgeServerMessage::Response(ApiResponse::failure(
                "snapshot",
                ExtensionApiError::new("unsupported_api", "reserved conformance API is disabled")
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
                        crate::match_pattern::ExtensionMatchPattern::parse(
                            "https://*.example.com/*",
                        )
                        .unwrap(),
                    ],
                    conformance: false,
                },
            }],
        )
        .unwrap();
        let model = ExtensionModel::default();
        let request = |namespace: &str, method: &str, arguments: serde_json::Value| ApiRequest {
            request_id: "request".into(),
            namespace: namespace.into(),
            method: method.into(),
            arguments,
            caller_context: Fixture::caller_context(),
        };

        assert!(
            BrokerRecords::authorize_api_request(
                &server,
                EXTENSION_ID,
                &request("storage.local", "get", serde_json::json!({})),
                &model,
            )
            .is_ok()
        );
        assert_eq!(
            BrokerRecords::authorize_api_request(
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
            BrokerRecords::authorize_api_request(
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
            BrokerRecords::authorize_api_request(
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
            BrokerRecords::authorize_api_request(
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
            BrokerRecords::authorize_api_request(&server, EXTENSION_ID, &invalid_caller, &model)
                .unwrap_err()
                .code,
            "invalid_context"
        );
    }

    #[test]
    fn subscription_resends_pending_event_until_acknowledged() {
        let server = Fixture::conformance_server();
        let mut socket = Fixture::connect(&server);
        let mut fixture = Fixture::new(server, Some(ConformanceWakeScheduler::default()));
        let first_event = ApiEvent {
            sequence: 1,
            namespace: BridgeOrder::CONFORMANCE_NAMESPACE.into(),
            event: BridgeOrder::MODEL_CHANGED_EVENT.into(),
            arguments: serde_json::json!([{ "change": 1 }]),
        };
        fixture.app.world_mut().spawn((
            crate::Id(EXTENSION_ID.into()),
            PendingBridgeEvent {
                sequence: first_event.sequence,
                namespace: first_event.namespace.clone(),
                event: first_event.event.clone(),
                arguments: first_event.arguments.clone(),
            },
        ));
        fixture
            .server()
            .send(
                EXTENSION_ID,
                BridgeServerMessage::Event(first_event.clone()),
            )
            .unwrap();
        assert_eq!(
            Fixture::read(&mut socket),
            BridgeServerMessage::Event(first_event.clone())
        );
        socket.close(None).unwrap();
        drop(socket);
        std::thread::sleep(Duration::from_millis(50));

        let mut restarted = Fixture::connect(fixture.server());
        Fixture::send(
            &mut restarted,
            &BridgeClientMessage::Subscribe(EventSubscribe {
                subscription_id: "model".into(),
                namespace: BridgeOrder::CONFORMANCE_NAMESPACE.into(),
                event: BridgeOrder::MODEL_CHANGED_EVENT.into(),
                caller_context: Fixture::caller_context(),
            }),
        );
        fixture.pump();

        let resent = Fixture::read(&mut restarted);
        assert_eq!(resent, BridgeServerMessage::Event(first_event.clone()));
        Fixture::send(
            &mut restarted,
            &BridgeClientMessage::Ack {
                sequence: first_event.sequence,
            },
        );
        fixture.pump();
        let pending_count = fixture
            .app
            .world_mut()
            .query::<(&crate::Id, &PendingBridgeEvent)>()
            .iter(fixture.app.world())
            .filter(|(id, _)| id.0 == EXTENSION_ID)
            .count();
        assert_eq!(pending_count, 0);

        Fixture::send(
            &mut restarted,
            &BridgeClientMessage::Ack {
                sequence: first_event.sequence,
            },
        );
        fixture.pump();
        Fixture::send(
            &mut restarted,
            &BridgeClientMessage::ApiRequest(ApiRequest {
                request_id: "after-duplicate-ack".into(),
                namespace: "windows".into(),
                method: "getAll".into(),
                arguments: serde_json::json!([{}]),
                caller_context: Fixture::caller_context(),
            }),
        );
        fixture.pump();
        assert!(matches!(
            Fixture::read(&mut restarted),
            BridgeServerMessage::Response(ApiResponse { request_id, .. })
                if request_id == "after-duplicate-ack"
        ));
    }

    #[test]
    fn windows_subscription_receives_window_events() {
        let server = Fixture::conformance_server();
        let mut socket = Fixture::connect(&server);
        Fixture::send(
            &mut socket,
            &BridgeClientMessage::Subscribe(EventSubscribe {
                subscription_id: "windows.onRemoved".into(),
                namespace: "windows".into(),
                event: "onRemoved".into(),
                caller_context: Fixture::caller_context(),
            }),
        );
        let mut fixture = Fixture::new(server, None);
        fixture.pump();
        assert!(
            fixture
                .app
                .world_mut()
                .query::<(&crate::Id, &BridgeSubscription)>()
                .iter(fixture.app.world())
                .any(|(id, entry)| {
                    id.0 == EXTENSION_ID
                        && entry.namespace == "windows"
                        && entry.event == "onRemoved"
                })
        );

        fixture
            .app
            .world_mut()
            .write_message(ExtensionModelEvent::WindowRemoved { window_id: 42 });
        fixture.app.update();

        let BridgeServerMessage::Event(event) = Fixture::read(&mut socket) else {
            panic!("expected window event");
        };
        assert_eq!(event.namespace, "windows");
        assert_eq!(event.event, "onRemoved");
        assert_eq!(event.arguments, serde_json::json!([42]));
    }

    #[test]
    fn duplicate_subscription_schedules_one_wake() {
        let server = Fixture::conformance_server();
        let mut socket = Fixture::connect(&server);
        let subscribe = || {
            BridgeClientMessage::Subscribe(EventSubscribe {
                subscription_id: "model".into(),
                namespace: BridgeOrder::CONFORMANCE_NAMESPACE.into(),
                event: BridgeOrder::MODEL_CHANGED_EVENT.into(),
                caller_context: Fixture::caller_context(),
            })
        };
        Fixture::send(&mut socket, &subscribe());
        let (scheduler, scheduled) = crossbeam_channel::unbounded();
        let mut fixture = Fixture::new(
            server,
            Some(ConformanceWakeScheduler {
                delay: Duration::from_secs(35),
                scheduler: Some(scheduler),
                worker: None,
            }),
        );
        fixture.pump();
        let first_deadline = fixture
            .app
            .world_mut()
            .query::<(&crate::Id, &ConformanceWake)>()
            .iter(fixture.app.world())
            .find_map(|(id, wake)| (id.0 == EXTENSION_ID).then_some(wake.deadline.unwrap()))
            .unwrap();
        assert_eq!(scheduled.try_recv().unwrap(), first_deadline);

        Fixture::send(&mut socket, &subscribe());
        fixture.pump();

        let wakes = fixture
            .app
            .world_mut()
            .query::<(&crate::Id, &ConformanceWake)>()
            .iter(fixture.app.world())
            .filter(|(id, _)| id.0 == EXTENSION_ID)
            .map(|(_, wake)| wake.deadline)
            .collect::<Vec<_>>();
        assert_eq!(wakes, vec![Some(first_deadline)]);
        assert_eq!(
            scheduled.try_recv(),
            Err(crossbeam_channel::TryRecvError::Empty)
        );
    }

    #[test]
    fn wake_timer_delivers_snapshot_event() {
        let server = Fixture::conformance_server();
        let mut socket = Fixture::connect(&server);
        let model = ExtensionModel::default();
        let mut fixture = Fixture::new(
            server,
            Some(ConformanceWakeScheduler {
                delay: Duration::ZERO,
                scheduler: None,
                worker: None,
            }),
        );
        fixture.app.world_mut().spawn((
            crate::Id(EXTENSION_ID.into()),
            ConformanceWake {
                deadline: Some(Instant::now()),
            },
        ));

        fixture.app.update();

        let BridgeServerMessage::Event(event) = Fixture::read(&mut socket) else {
            panic!("expected wake timer event");
        };
        assert_eq!(event.namespace, BridgeOrder::CONFORMANCE_NAMESPACE);
        assert_eq!(event.event, BridgeOrder::MODEL_CHANGED_EVENT);
        assert_eq!(event.arguments, serde_json::json!([model]));
    }

    #[test]
    fn authorization_revocation_despawns_broker_entities_for_extension() {
        let mut fixture = Fixture::new(
            Fixture::conformance_server(),
            Some(ConformanceWakeScheduler::default()),
        );
        fixture.app.world_mut().spawn((
            crate::Id(EXTENSION_ID.into()),
            BridgeSubscription {
                context_id: "worker".into(),
                subscription_id: "sub".into(),
                namespace: "windows".into(),
                event: "onRemoved".into(),
            },
        ));
        fixture.app.world_mut().spawn((
            crate::Id(EXTENSION_ID.into()),
            PendingBridgeEvent {
                sequence: 1,
                namespace: "windows".into(),
                event: "onRemoved".into(),
                arguments: serde_json::json!([1]),
            },
        ));
        fixture.app.world_mut().spawn((
            crate::Id(EXTENSION_ID.into()),
            CachedBridgeResponse {
                request_id: "r1".into(),
                frame_json: serde_json::to_string(&BridgeServerMessage::Heartbeat).unwrap(),
                order: 1,
            },
        ));
        fixture.app.world_mut().spawn((
            crate::Id(EXTENSION_ID.into()),
            SeenBridgeRequest {
                session_id: 1,
                request_id: "r1".into(),
                order: 1,
            },
        ));
        fixture.app.world_mut().spawn((
            crate::Id(EXTENSION_ID.into()),
            ConformanceWake {
                deadline: Some(Instant::now() + Duration::from_secs(60)),
            },
        ));

        fixture
            .app
            .world_mut()
            .write_message(BridgeAuthorizationRevoked(EXTENSION_ID.into()));
        fixture.app.update();

        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&BridgeSubscription>()
                .iter(fixture.app.world())
                .count(),
            0
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&PendingBridgeEvent>()
                .iter(fixture.app.world())
                .count(),
            0
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&CachedBridgeResponse>()
                .iter(fixture.app.world())
                .count(),
            0
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&SeenBridgeRequest>()
                .iter(fixture.app.world())
                .count(),
            0
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&ConformanceWake>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }
}
