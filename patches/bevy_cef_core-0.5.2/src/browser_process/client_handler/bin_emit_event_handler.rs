use crate::browser_process::client_handler::ProcessMessageHandler;
use crate::prelude::PROCESS_MESSAGE_BIN_JS_EMIT;
use crate::util::IntoString;
use async_channel::Sender;
use bevy::prelude::Entity;
use cef::{Browser, Frame, ImplBinaryValue, ImplListValue, ListValue};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinIpcEventRaw {
    pub webview: Entity,
    pub id: String,
    pub payload: Vec<u8>,
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

    fn handle_message(&self, _browser: &mut Browser, _frame: &mut Frame, args: Option<ListValue>) {
        let Some(args) = args else {
            return;
        };
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
        crate::util::webview_debug_log(format!(
            "browser bin_js_emit entity={:?} id={} payload_len={}",
            self.webview,
            id,
            payload.len()
        ));
        let _ = self.sender.send_blocking(BinIpcEventRaw {
            webview: self.webview,
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
            id: "test-id".to_string(),
            payload: payload.clone(),
        };
        assert_eq!(raw.webview, webview);
        assert_eq!(raw.id, "test-id");
        assert_eq!(raw.payload, payload);
    }
}
