use crate::browser_process::client_handler::ProcessMessageHandler;
use crate::prelude::PROCESS_MESSAGE_BIN_JS_EMIT;
use crate::util::IntoString;
use async_channel::Sender;
use bevy::prelude::Entity;
use cef::{Browser, Frame, ImplBinaryValue, ImplFrame, ImplListValue, ListValue};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinIpcEventRaw {
    pub webview: Entity,
    pub host: String,
    pub id: String,
    pub payload: Vec<u8>,
}

const BIN_IPC_ENVELOPE_MAGIC: &[u8] = b"vmux-bin-ipc-v1\0";

fn decode_bin_ipc_envelope(bytes: &[u8]) -> Option<(String, Vec<u8>)> {
    let id_len_start = BIN_IPC_ENVELOPE_MAGIC.len();
    let id_start = id_len_start + 4;
    if bytes.len() < id_start || !bytes.starts_with(BIN_IPC_ENVELOPE_MAGIC) {
        return None;
    }
    let id_len = u32::from_le_bytes(bytes[id_len_start..id_start].try_into().ok()?) as usize;
    let payload_start = id_start.checked_add(id_len)?;
    if bytes.len() < payload_start {
        return None;
    }
    let id = std::str::from_utf8(&bytes[id_start..payload_start])
        .ok()?
        .to_string();
    Some((id, bytes[payload_start..].to_vec()))
}

#[cfg(test)]
mod bin_ipc_envelope_tests {
    use super::*;

    fn encode_test_envelope(id: &str, payload: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(BIN_IPC_ENVELOPE_MAGIC);
        encoded.extend_from_slice(&(id.len() as u32).to_le_bytes());
        encoded.extend_from_slice(id.as_bytes());
        encoded.extend_from_slice(payload);
        encoded
    }

    #[test]
    fn decode_bin_ipc_envelope_extracts_id_and_payload() {
        let encoded = encode_test_envelope("event-id", &[1, 2, 3]);

        let (id, payload) = decode_bin_ipc_envelope(&encoded).expect("envelope");

        assert_eq!(id, "event-id");
        assert_eq!(payload, vec![1, 2, 3]);
    }

    #[test]
    fn decode_bin_ipc_envelope_ignores_legacy_payload() {
        assert!(decode_bin_ipc_envelope(&[1, 2, 3]).is_none());
    }
}

pub struct BinEmitEventHandler {
    webview: Entity,
    sender: Sender<BinIpcEventRaw>,
}

impl BinEmitEventHandler {
    pub const fn new(webview: Entity, sender: Sender<BinIpcEventRaw>) -> Self {
        Self { sender, webview }
    }
}

impl ProcessMessageHandler for BinEmitEventHandler {
    fn process_name(&self) -> &'static str {
        PROCESS_MESSAGE_BIN_JS_EMIT
    }

    fn handle_message(&self, _browser: &mut Browser, frame: &mut Frame, args: Option<ListValue>) {
        let Some(args) = args else {
            return;
        };
        let host =
            crate::util::embedded_page_host_of(&frame.url().into_string()).unwrap_or_default();
        let id = args.string(0).into_string();
        let payload_index = if id.is_empty() { 0 } else { 1 };
        let payload = match args.binary(payload_index) {
            Some(binary) => {
                let len = binary.size();
                let mut buf = vec![0u8; len];
                binary.data(Some(&mut buf), 0);
                buf
            }
            None => Vec::new(),
        };
        let (id, payload) = if id.is_empty() {
            decode_bin_ipc_envelope(&payload).unwrap_or((id, payload))
        } else {
            (id, payload)
        };
        crate::util::webview_debug_log(format!(
            "browser bin_js_emit entity={:?} id={} payload_len={}",
            self.webview,
            id,
            payload.len()
        ));
        let _ = self.sender.send_blocking(BinIpcEventRaw {
            webview: self.webview,
            host,
            id,
            payload,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;

    #[test]
    fn bin_ipc_event_raw_construction() {
        let webview = Entity::PLACEHOLDER;
        let payload = vec![1, 2, 3, 4];
        let raw = BinIpcEventRaw {
            webview,
            host: "history".to_string(),
            id: "test-id".to_string(),
            payload: payload.clone(),
        };
        assert_eq!(raw.webview, webview);
        assert_eq!(raw.host, "history");
        assert_eq!(raw.id, "test-id");
        assert_eq!(raw.payload, payload);
    }
}
