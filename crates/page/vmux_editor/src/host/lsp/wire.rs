use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

impl RequestId {
    pub fn of(value: &Value) -> Option<Self> {
        if let Some(n) = value.as_i64() {
            return Some(Self::Number(n));
        }
        let s = value.as_str()?;
        Some(Self::String(s.to_string()))
    }

    pub fn to_key(&self) -> Option<i64> {
        match self {
            Self::Number(n) => Some(*n),
            Self::String(s) => s.parse().ok(),
        }
    }

    pub fn ok(&self, result: Value) -> Value {
        serde_json::json!({ "jsonrpc": "2.0", "id": self, "result": result })
    }

    pub fn err(&self, code: ErrorCode) -> Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": self,
            "error": { "code": code.code(), "message": code.message() },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    MethodNotFound,
    InvalidParams,
    RequestCancelled,
}

impl ErrorCode {
    fn code(self) -> i64 {
        match self {
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::RequestCancelled => -32800,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::MethodNotFound => "method not found",
            Self::InvalidParams => "invalid params",
            Self::RequestCancelled => "request cancelled",
        }
    }
}

pub enum Incoming {
    Response {
        id: RequestId,
        body: Value,
    },
    Request {
        id: RequestId,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Invalid,
}

impl Incoming {
    pub fn of(mut msg: Value) -> Self {
        let id = msg.get("id").and_then(RequestId::of);
        let Some(method) = msg.get("method").and_then(|v| v.as_str()).map(String::from) else {
            return match id {
                Some(id) => Self::Response { id, body: msg },
                None => Self::Invalid,
            };
        };
        let params = msg
            .get_mut("params")
            .map(Value::take)
            .unwrap_or(Value::Null);
        match id {
            Some(id) => Self::Request { id, method, params },
            None => Self::Notification { method, params },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_carries_both_id_and_method() {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 1000,
            "method": "workspace/applyEdit",
            "params": {"edit": {}},
        });
        let Incoming::Request { id, method, params } = Incoming::of(msg) else {
            panic!("id + method must classify as a request, not a response");
        };
        assert_eq!(id, RequestId::Number(1000));
        assert_eq!(method, "workspace/applyEdit");
        assert_eq!(params, json!({"edit": {}}));
    }

    #[test]
    fn response_keeps_the_whole_envelope() {
        let Incoming::Response { id, body } =
            Incoming::of(json!({"jsonrpc": "2.0", "id": 7, "result": {"ok": true}}))
        else {
            panic!("id without method is a response");
        };
        assert_eq!(id, RequestId::Number(7));
        assert_eq!(body["result"]["ok"], true);
    }

    #[test]
    fn notification_has_no_id() {
        let Incoming::Notification { method, .. } =
            Incoming::of(json!({"method": "window/logMessage", "params": {}}))
        else {
            panic!("method without id is a notification");
        };
        assert_eq!(method, "window/logMessage");
    }

    #[test]
    fn neither_id_nor_method_is_invalid() {
        assert!(matches!(
            Incoming::of(json!({"jsonrpc": "2.0"})),
            Incoming::Invalid
        ));
    }

    #[test]
    fn string_ids_survive_classification_and_replay() {
        let Incoming::Request { id, .. } = Incoming::of(json!({
            "id": "req-1", "method": "client/registerCapability", "params": {},
        })) else {
            panic!("a string id is still an id");
        };
        assert_eq!(id, RequestId::String("req-1".to_string()));
        assert_eq!(id.ok(Value::Null)["id"], json!("req-1"));
    }

    #[test]
    fn stringified_numeric_id_still_correlates() {
        assert_eq!(RequestId::String("7".to_string()).to_key(), Some(7));
        assert_eq!(RequestId::Number(7).to_key(), Some(7));
        assert_eq!(RequestId::String("req-1".to_string()).to_key(), None);
    }

    #[test]
    fn error_replies_carry_the_code_and_no_result() {
        let reply = RequestId::Number(3).err(ErrorCode::MethodNotFound);
        assert_eq!(reply["id"], 3);
        assert_eq!(reply["error"]["code"], -32601);
        assert!(reply.get("result").is_none());
    }
}
