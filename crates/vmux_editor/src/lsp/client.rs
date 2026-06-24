use std::path::PathBuf;

use serde_json::Value;

use crate::lsp::{LspOutbox, PendingMap};

/// Convert a `file://` URI string to a filesystem path (via the `url` crate;
/// `lsp_types::Uri` has no path conversion).
pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}

/// Route one incoming JSON-RPC message.
/// - Responses (have `id` + `result`/`error`) go to the matching pending sender.
/// - `textDocument/publishDiagnostics` notifications go to the outbox.
/// - Everything else is ignored.
pub fn dispatch_message(msg: Value, pending: &PendingMap, outbox: &LspOutbox) {
    if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
        if msg.get("method").is_none() {
            // Response to a request we sent.
            if let Some(tx) = pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id)
            {
                let _ = tx.send(msg);
            }
            return;
        }
        // else: a server->client request; ignored in milestone 1.
    }
    let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
    if method == "textDocument/publishDiagnostics" {
        let Some(params) = msg.get("params") else {
            return;
        };
        let Ok(parsed) =
            serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(params.clone())
        else {
            return;
        };
        if let Some(path) = path_from_uri(parsed.uri.as_str()) {
            outbox
                .0
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push((path, parsed.diagnostics));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc;

    fn outbox() -> LspOutbox {
        LspOutbox::default()
    }
    fn pending() -> PendingMap {
        PendingMap::default()
    }

    #[test]
    fn publish_diagnostics_lands_in_outbox() {
        let ob = outbox();
        let pd = pending();
        let msg = json!({
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
        });
        dispatch_message(msg, &pd, &ob);
        let q = ob.0.lock().unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].0, PathBuf::from("/tmp/main.rs"));
        assert_eq!(q[0].1.len(), 1);
        assert_eq!(q[0].1[0].message, "boom");
    }

    #[test]
    fn response_routes_to_pending_sender() {
        let ob = outbox();
        let pd = pending();
        let (tx, rx) = mpsc::channel();
        pd.lock().unwrap().insert(7, tx);
        dispatch_message(json!({"jsonrpc": "2.0", "id": 7, "result": {}}), &pd, &ob);
        let got = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
        assert_eq!(got["id"], 7);
        assert!(pd.lock().unwrap().is_empty(), "pending entry consumed");
    }

    #[test]
    fn unknown_notification_is_ignored() {
        let ob = outbox();
        let pd = pending();
        dispatch_message(json!({"method": "window/logMessage", "params": {}}), &pd, &ob);
        assert!(ob.0.lock().unwrap().is_empty());
    }
}
