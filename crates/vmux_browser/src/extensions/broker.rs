use bevy::prelude::*;
use std::collections::HashMap;
use vmux_core::extension::protocol::{
    ApiRequest, ApiResponse, BridgeClientMessage, BridgeServerMessage, ChromeError,
};

use super::bridge::{BridgeInbound, ExtensionBridgeServer};
use super::capability::{CapabilityKind, CapabilityMatrix, CapabilityStatus};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeSubscription {
    pub context_id: String,
    pub subscription_id: String,
    pub namespace: String,
    pub event: String,
}

#[derive(Resource, Default)]
pub struct BridgeSubscriptions(pub HashMap<String, Vec<BridgeSubscription>>);

pub fn drain_bridge_requests(
    server: Res<ExtensionBridgeServer>,
    mut subscriptions: ResMut<BridgeSubscriptions>,
) {
    let matrix = CapabilityMatrix::embedded().expect("valid embedded capability matrix");
    while let Ok(inbound) = server.try_recv() {
        let BridgeInbound {
            extension_id,
            context_id,
            message,
        } = inbound;
        match message {
            BridgeClientMessage::ApiRequest(request) => {
                let response = dispatch_api_request(&matrix, request);
                if let Err(error) = server.send(&extension_id, response) {
                    bevy::log::warn!("failed to send extension bridge response: {error}");
                }
            }
            BridgeClientMessage::Subscribe(subscription) => {
                let stored = BridgeSubscription {
                    context_id,
                    subscription_id: subscription.subscription_id,
                    namespace: subscription.namespace,
                    event: subscription.event,
                };
                let entries = subscriptions.0.entry(extension_id).or_default();
                if let Some(existing) = entries
                    .iter_mut()
                    .find(|entry| entry.subscription_id == stored.subscription_id)
                {
                    *existing = stored;
                } else {
                    entries.push(stored);
                }
            }
            BridgeClientMessage::Hello(_) | BridgeClientMessage::Ack { .. } => {
                if let Err(error) = server.send(
                    &extension_id,
                    BridgeServerMessage::Fatal(ChromeError::new(
                        "protocol_error",
                        "unexpected bridge message",
                    )),
                ) {
                    bevy::log::warn!("failed to send extension bridge error: {error}");
                }
            }
        }
    }
}

fn dispatch_api_request(matrix: &CapabilityMatrix, request: ApiRequest) -> BridgeServerMessage {
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
    use tungstenite::{Message, connect};
    use vmux_core::extension::protocol::{
        ApiRequest, ApiResponse, BRIDGE_PROTOCOL_VERSION, BridgeClientMessage, BridgeHello,
        BridgeServerMessage, ChromeError, ExtensionContextKind,
    };

    const EXTENSION_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

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
}
