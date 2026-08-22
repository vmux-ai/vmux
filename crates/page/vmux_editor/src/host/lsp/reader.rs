//! The thread that reads one server's stdout, and the only place that answers it.
//!
//! Requests that need no world state are answered here, where the server is blocked and
//! latency matters. Anything else is handed to the world with the write handle attached. The
//! catch-all matters as much as the handled cases: an unanswered request leaves an id pending
//! in the server forever, and some servers serialise behind it.

use std::io::BufReader;
use std::process::ChildStdout;
use std::sync::mpsc;

use serde_json::Value;

use crate::lsp::client::path_from_uri;
use crate::lsp::server_request::{ReplyHandle, ServerEvent};
use crate::lsp::wire::{ErrorCode, Incoming, RequestId};
use crate::lsp::{LspOutbox, PendingMap, framing};

pub struct Reader {
    pending: PendingMap,
    outbox: LspOutbox,
    outgoing: mpsc::Sender<Value>,
    events: crossbeam_channel::Sender<ServerEvent>,
    root_uri: String,
    root_name: String,
}

impl Reader {
    pub fn new(
        pending: PendingMap,
        outbox: LspOutbox,
        outgoing: mpsc::Sender<Value>,
        events: crossbeam_channel::Sender<ServerEvent>,
        root: &std::path::Path,
    ) -> Self {
        let root_uri = url::Url::from_file_path(root)
            .map(|u| u.to_string())
            .unwrap_or_default();
        let root_name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        Self {
            pending,
            outbox,
            outgoing,
            events,
            root_uri,
            root_name,
        }
    }

    pub fn run(self, stdout: ChildStdout) {
        let mut r = BufReader::new(stdout);
        while let Ok(Some(msg)) = framing::read_message(&mut r) {
            self.dispatch(msg);
        }
    }

    fn dispatch(&self, msg: Value) {
        match Incoming::of(msg) {
            Incoming::Response { id, body } => self.resolve(id, body),
            Incoming::Request { id, method, params } => self.answer(id, &method, params),
            Incoming::Notification { method, params } => self.observe(&method, params),
            Incoming::Invalid => {}
        }
    }

    fn resolve(&self, id: RequestId, body: Value) {
        let Some(key) = id.to_key() else {
            return;
        };
        let Some(tx) = self
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&key)
        else {
            return;
        };
        let _ = tx.send(body);
    }

    fn answer(&self, id: RequestId, method: &str, params: Value) {
        match method {
            "client/registerCapability"
            | "client/unregisterCapability"
            | "window/workDoneProgress/create"
            | "workspace/semanticTokens/refresh"
            | "workspace/codeLens/refresh"
            | "workspace/inlayHint/refresh"
            | "workspace/diagnostic/refresh" => self.send(id.ok(Value::Null)),
            "workspace/workspaceFolders" => self.send(id.ok(self.workspace_folders())),
            "workspace/configuration" => self.send(id.ok(Self::no_configuration(&params))),
            "workspace/applyEdit" => self.forward_apply_edit(id, params),
            _ => self.send(id.err(ErrorCode::MethodNotFound)),
        }
    }

    fn observe(&self, method: &str, params: Value) {
        match method {
            "textDocument/publishDiagnostics" => self.publish_diagnostics(params),
            "window/showMessage" | "window/logMessage" => self.log(params),
            _ => {}
        }
    }

    fn forward_apply_edit(&self, id: RequestId, params: Value) {
        let Ok(params) = serde_json::from_value::<lsp_types::ApplyWorkspaceEditParams>(params)
        else {
            self.send(id.err(ErrorCode::InvalidParams));
            return;
        };
        let reply = ReplyHandle::new(id, self.outgoing.clone());
        let event = ServerEvent::ApplyEdit {
            reply: reply.clone(),
            params,
        };
        if self.events.send(event).is_err() {
            reply.err(ErrorCode::RequestCancelled);
        }
    }

    fn publish_diagnostics(&self, params: Value) {
        let Ok(parsed) = serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(params)
        else {
            return;
        };
        let Some(path) = path_from_uri(parsed.uri.as_str()) else {
            return;
        };
        self.outbox
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((path, parsed.diagnostics));
    }

    fn log(&self, params: Value) {
        let Ok(parsed) = serde_json::from_value::<lsp_types::LogMessageParams>(params) else {
            return;
        };
        let _ = self.events.send(ServerEvent::Log {
            level: parsed.typ,
            text: parsed.message,
        });
    }

    fn workspace_folders(&self) -> Value {
        serde_json::json!([{ "uri": self.root_uri, "name": self.root_name }])
    }

    /// One `null` per requested section: spec-legal, and reads as "use your defaults".
    fn no_configuration(params: &Value) -> Value {
        let sections = params
            .get("items")
            .and_then(|v| v.as_array())
            .map_or(0, |items| items.len());
        Value::Array(vec![Value::Null; sections])
    }

    fn send(&self, msg: Value) {
        let _ = self.outgoing.send(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    struct Harness {
        reader: Reader,
        sent: mpsc::Receiver<Value>,
        events: crossbeam_channel::Receiver<ServerEvent>,
    }

    impl Harness {
        fn start() -> Self {
            let (outgoing, sent) = mpsc::channel();
            let (event_tx, events) = crossbeam_channel::unbounded();
            let reader = Reader::new(
                PendingMap::default(),
                LspOutbox::default(),
                outgoing,
                event_tx,
                std::path::Path::new("/tmp/proj"),
            );
            Self {
                reader,
                sent,
                events,
            }
        }

        fn reply_to(&self, msg: Value) -> Value {
            self.reader.dispatch(msg);
            self.sent.try_recv().expect("a request must be answered")
        }
    }

    #[test]
    fn publish_diagnostics_lands_in_outbox() {
        let h = Harness::start();
        h.reader.dispatch(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": "file:///tmp/main.rs",
                "diagnostics": [{
                    "range": {"start": {"line": 1, "character": 2},
                              "end": {"line": 1, "character": 5}},
                    "severity": 1,
                    "message": "boom",
                    "source": "rustc"
                }]
            }
        }));
        let q = h.reader.outbox.0.lock().unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].0, PathBuf::from("/tmp/main.rs"));
        assert_eq!(q[0].1[0].message, "boom");
    }

    #[test]
    fn response_routes_to_pending_sender() {
        let h = Harness::start();
        let (tx, rx) = mpsc::channel();
        h.reader.pending.lock().unwrap().insert(7, tx);
        h.reader
            .dispatch(json!({"jsonrpc": "2.0", "id": 7, "result": {}}));
        let got = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
        assert_eq!(got["id"], 7);
        assert!(
            h.reader.pending.lock().unwrap().is_empty(),
            "pending entry consumed"
        );
    }

    #[test]
    fn unknown_notification_is_ignored() {
        let h = Harness::start();
        h.reader
            .dispatch(json!({"method": "telemetry/event", "params": {}}));
        assert!(h.reader.outbox.0.lock().unwrap().is_empty());
        assert!(
            h.sent.try_recv().is_err(),
            "a notification is never answered"
        );
    }

    #[test]
    fn unknown_request_gets_method_not_found() {
        let h = Harness::start();
        let reply = h.reply_to(json!({"id": 4, "method": "window/showDocument", "params": {}}));
        assert_eq!(reply["id"], 4);
        assert_eq!(reply["error"]["code"], -32601);
    }

    #[test]
    fn register_capability_is_acknowledged() {
        let h = Harness::start();
        let reply = h.reply_to(json!({
            "id": 1, "method": "client/registerCapability", "params": {"registrations": []},
        }));
        assert!(reply["result"].is_null());
        assert!(reply.get("error").is_none(), "must not be an error");
    }

    #[test]
    fn workspace_folders_names_the_root() {
        let h = Harness::start();
        let reply = h.reply_to(json!({"id": 2, "method": "workspace/workspaceFolders"}));
        assert_eq!(reply["result"][0]["name"], "proj");
        assert_eq!(reply["result"][0]["uri"], "file:///tmp/proj");
    }

    #[test]
    fn configuration_answers_one_entry_per_requested_section() {
        let h = Harness::start();
        let reply = h.reply_to(json!({
            "id": 3,
            "method": "workspace/configuration",
            "params": {"items": [{"section": "rust-analyzer"}, {"section": "files"}]},
        }));
        assert_eq!(reply["result"], json!([null, null]));
    }

    #[test]
    fn apply_edit_reaches_the_world_unanswered() {
        let h = Harness::start();
        h.reader.dispatch(json!({
            "id": 9,
            "method": "workspace/applyEdit",
            "params": {"edit": {"changes": {}}},
        }));
        assert!(
            h.sent.try_recv().is_err(),
            "the world answers applyEdit, not the reader"
        );
        assert!(matches!(
            h.events.try_recv(),
            Ok(ServerEvent::ApplyEdit { .. })
        ));
    }

    #[test]
    fn malformed_apply_edit_is_refused_rather_than_forwarded() {
        let h = Harness::start();
        let reply = h.reply_to(json!({"id": 9, "method": "workspace/applyEdit", "params": 7}));
        assert_eq!(reply["error"]["code"], -32602);
        assert!(h.events.try_recv().is_err(), "nothing reaches the world");
    }

    #[test]
    fn log_messages_reach_the_world() {
        let h = Harness::start();
        h.reader.dispatch(json!({
            "method": "window/logMessage",
            "params": {"type": 1, "message": "boom"},
        }));
        let Ok(ServerEvent::Log { text, .. }) = h.events.try_recv() else {
            panic!("a log notification should be observable");
        };
        assert_eq!(text, "boom");
    }
}
