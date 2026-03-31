//! Deserialization for `window.cef.emit` payloads (V8/JSON can diverge from strict Rust types).

use serde::{Deserialize, Deserializer};

fn deserialize_loose_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
    Ok(match v {
        serde_json::Value::Bool(b) => b,
        serde_json::Value::Number(n) => n.as_u64().is_some_and(|x| x != 0) || n.as_i64().is_some_and(|x| x != 0),
        serde_json::Value::String(s) => s.eq_ignore_ascii_case("true") || s == "1",
        serde_json::Value::Null => false,
        _ => false,
    })
}

fn deserialize_loose_opt_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
    match v {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                if let Ok(x) = u32::try_from(u) {
                    return Ok(Some(x));
                }
            }
            if let Some(f) = n.as_f64() {
                return Ok(Some(f.round() as u32));
            }
            Ok(None)
        }
        serde_json::Value::String(s) => s.parse().map(Some).map_err(serde::de::Error::custom),
        _ => Ok(None),
    }
}

/// Payload from `window.cef.emit(...)` (single JSON object). Preload uses `{ url }`; history UI uses `{ vmux_open_in_pane }`, etc.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WebviewDocumentUrlEmit {
    #[serde(default)]
    pub url: Option<String>,
    /// When set (e.g. from history pane), open this URL in the active main pane.
    #[serde(default, rename = "vmux_open_in_pane")]
    pub vmux_open_in_pane: Option<String>,
    /// History pane asks the host to push the current list (after `cef.listen` is registered).
    #[serde(
        default,
        rename = "vmux_request_history",
        deserialize_with = "deserialize_loose_bool"
    )]
    pub vmux_request_history: bool,
    /// Echoed on the next `vmux_history` host emit so the UI can confirm the bridge delivered (`u32` so JS numbers stay exact).
    #[serde(
        default,
        rename = "vmux_history_sync_nonce",
        deserialize_with = "deserialize_loose_opt_u32"
    )]
    pub vmux_history_sync_nonce: Option<u32>,
    /// History pane asks the host to wipe persisted visit list.
    #[serde(
        default,
        rename = "vmux_clear_history",
        deserialize_with = "deserialize_loose_bool"
    )]
    pub vmux_clear_history: bool,
}

#[cfg(test)]
mod tests {
    use super::WebviewDocumentUrlEmit;

    #[test]
    fn deserializes_request_history_strict_bool() {
        let e: WebviewDocumentUrlEmit =
            serde_json::from_str(r#"{"vmux_request_history":true}"#).unwrap();
        assert!(e.vmux_request_history);
    }

    #[test]
    fn deserializes_request_history_numeric() {
        let e: WebviewDocumentUrlEmit =
            serde_json::from_str(r#"{"vmux_request_history":1}"#).unwrap();
        assert!(e.vmux_request_history);
    }

    #[test]
    fn deserializes_sync_nonce_float() {
        let e: WebviewDocumentUrlEmit =
            serde_json::from_str(r#"{"vmux_request_history":true,"vmux_history_sync_nonce":42.7}"#)
                .unwrap();
        assert_eq!(e.vmux_history_sync_nonce, Some(43));
    }
}
