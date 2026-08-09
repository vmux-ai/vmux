use serde::{Deserialize, Serialize};

pub const BRIDGE_PROTOCOL_VERSION: u16 = 2;
pub const BRIDGE_CHANNEL: &str = "__vmux_extension_bridge_v2";
pub const BRIDGE_CONTEXT_ID: &str = "bridge-page";
pub const BRIDGE_MAX_FRAME_SIZE: usize = 256 * 1024;
pub const BRIDGE_MAX_MESSAGE_SIZE: usize = 1024 * 1024;
pub const KEEPALIVE_CHANNEL: &str = "__vmux_extension_keepalive_v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionContextKind {
    BridgePage,
    ServiceWorker,
    ExtensionPage,
    ContentScript,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeHello {
    pub protocol_version: u16,
    pub extension_id: String,
    pub profile_id: String,
    pub token: String,
    pub context_id: String,
    pub context_kind: ExtensionContextKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "context_kind", rename_all = "snake_case")]
pub enum ExtensionCallerContext {
    ServiceWorker {
        extension_id: String,
        context_id: String,
        url: Option<String>,
    },
    ExtensionPage {
        extension_id: String,
        context_id: String,
        url: String,
        document_id: String,
    },
    ContentScript {
        extension_id: String,
        context_id: String,
        url: String,
        tab_id: i64,
        frame_id: i64,
        document_id: Option<String>,
    },
}

impl ExtensionCallerContext {
    pub fn extension_id(&self) -> &str {
        match self {
            Self::ServiceWorker { extension_id, .. }
            | Self::ExtensionPage { extension_id, .. }
            | Self::ContentScript { extension_id, .. } => extension_id,
        }
    }

    pub fn context_id(&self) -> &str {
        match self {
            Self::ServiceWorker { context_id, .. }
            | Self::ExtensionPage { context_id, .. }
            | Self::ContentScript { context_id, .. } => context_id,
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            Self::ServiceWorker { url, .. } => url.as_deref(),
            Self::ExtensionPage { url, .. } | Self::ContentScript { url, .. } => Some(url),
        }
    }

    pub fn tab_id(&self) -> Option<i64> {
        match self {
            Self::ContentScript { tab_id, .. } => Some(*tab_id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiRequest {
    pub request_id: String,
    pub namespace: String,
    pub method: String,
    pub arguments: serde_json::Value,
    pub caller_context: ExtensionCallerContext,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubscribe {
    pub subscription_id: String,
    pub namespace: String,
    pub event: String,
    pub caller_context: ExtensionCallerContext,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BridgeClientMessage {
    Hello(BridgeHello),
    ApiRequest(ApiRequest),
    Subscribe(EventSubscribe),
    Unsubscribe {
        subscription_id: String,
        caller_context: ExtensionCallerContext,
    },
    Ack {
        sequence: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChromeError {
    pub code: String,
    pub message: String,
}

impl ChromeError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiResponse {
    pub request_id: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<ChromeError>,
}

impl ApiResponse {
    pub fn success(request_id: impl Into<String>, result: serde_json::Value) -> Self {
        Self {
            request_id: request_id.into(),
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(request_id: impl Into<String>, error: ChromeError) -> Self {
        Self {
            request_id: request_id.into(),
            result: None,
            error: Some(error),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match (self.result.is_some(), self.error.is_some()) {
            (true, false) | (false, true) => Ok(()),
            _ => Err(format!(
                "response {} must contain exactly one result channel",
                self.request_id
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiEvent {
    pub sequence: u64,
    pub namespace: String,
    pub event: String,
    pub arguments: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BridgeServerMessage {
    Ready { protocol_version: u16 },
    Heartbeat,
    Response(ApiResponse),
    Event(ApiEvent),
    Fatal(ChromeError),
}

#[cfg(test)]
#[path = "protocol.test.rs"]
mod tests;
