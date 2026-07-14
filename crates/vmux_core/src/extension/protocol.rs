use serde::{Deserialize, Serialize};

pub const BRIDGE_PROTOCOL_VERSION: u16 = 1;

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiRequest {
    pub request_id: String,
    pub namespace: String,
    pub method: String,
    pub arguments: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubscribe {
    pub subscription_id: String,
    pub namespace: String,
    pub event: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BridgeClientMessage {
    Hello(BridgeHello),
    ApiRequest(ApiRequest),
    Subscribe(EventSubscribe),
    Ack { sequence: u64 },
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
    Response(ApiResponse),
    Event(ApiEvent),
    Fatal(ChromeError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_round_trips_as_tagged_json() {
        let message = BridgeClientMessage::Hello(BridgeHello {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            profile_id: "personal".into(),
            token: "secret".into(),
            context_id: "bridge-page".into(),
            context_kind: ExtensionContextKind::BridgePage,
        });
        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("hello"));
        assert_eq!(
            serde_json::from_str::<BridgeClientMessage>(&json).unwrap(),
            message
        );
    }

    #[test]
    fn api_response_has_exactly_one_result_channel() {
        let response = ApiResponse::success("r1", serde_json::json!({ "ok": true }));
        response.validate().unwrap();
        assert!(
            ApiResponse {
                request_id: "r2".into(),
                result: Some(serde_json::Value::Null),
                error: Some(ChromeError::new("invalid", "bad")),
            }
            .validate()
            .is_err()
        );
    }
}
