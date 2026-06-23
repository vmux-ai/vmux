use crate::browser_process::client_handler::ProcessMessageHandler;
use crate::prelude::{IntoString, PROCESS_MESSAGE_SNAPSHOT_RESULT};
use async_channel::Sender;
use bevy::prelude::Entity;
use cef::{Browser, Frame, ImplListValue, ListValue};

#[derive(Debug, Clone)]
pub struct SnapshotResultRaw {
    pub webview: Entity,
    pub request_id: String,
    pub json: String,
}

pub struct SnapshotResultHandler {
    webview: Entity,
    sender: Sender<SnapshotResultRaw>,
}

impl SnapshotResultHandler {
    pub const fn new(webview: Entity, sender: Sender<SnapshotResultRaw>) -> Self {
        Self { sender, webview }
    }
}

impl ProcessMessageHandler for SnapshotResultHandler {
    fn process_name(&self) -> &'static str {
        PROCESS_MESSAGE_SNAPSHOT_RESULT
    }

    fn handle_message(&self, _browser: &mut Browser, _frame: &mut Frame, args: Option<ListValue>) {
        if let Some(args) = args {
            let event = SnapshotResultRaw {
                webview: self.webview,
                request_id: args.string(0).into_string(),
                json: args.string(1).into_string(),
            };
            let _ = self.sender.send_blocking(event);
        }
    }
}
