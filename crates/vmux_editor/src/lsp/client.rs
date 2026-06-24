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

use std::io::BufReader;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use std::collections::HashMap;

use crate::lsp::registry::ServerSpec;
use crate::lsp::{framing, ServerKey};

/// A running language-server process plus its I/O threads.
pub struct ServerClient {
    child: Child,
    outgoing: mpsc::Sender<serde_json::Value>,
    pending: PendingMap,
    next_id: AtomicI64,
    _reader: JoinHandle<()>,
    _writer: JoinHandle<()>,
    _stderr: JoinHandle<()>,
}

impl ServerClient {
    /// Spawn `spec.command` rooted at `root`, run the `initialize`/`initialized`
    /// handshake, and start the I/O threads. Diagnostics flow into `outbox`.
    pub fn spawn(
        spec: &ServerSpec,
        root: &std::path::Path,
        outbox: LspOutbox,
    ) -> std::io::Result<Self> {
        let mut child = Command::new(&spec.command)
            .args(&spec.args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));

        // Writer thread: serialize outgoing messages and frame them.
        let (outgoing, out_rx) = mpsc::channel::<serde_json::Value>();
        let writer = std::thread::spawn(move || {
            let mut w = stdin;
            while let Ok(msg) = out_rx.recv() {
                if framing::write_message(&mut w, &msg).is_err() {
                    break;
                }
            }
        });

        // Reader thread: parse frames and dispatch.
        let r_pending = pending.clone();
        let r_outbox = outbox.clone();
        let reader = std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            loop {
                match framing::read_message(&mut r) {
                    Ok(Some(msg)) => dispatch_message(msg, &r_pending, &r_outbox),
                    Ok(None) | Err(_) => break, // EOF or fatal parse error
                }
            }
        });

        // stderr thread: drain to the log.
        let cmd_name = spec.command.clone();
        let stderr_thread = std::thread::spawn(move || {
            use std::io::BufRead;
            let r = BufReader::new(stderr);
            for line in r.lines().map_while(Result::ok) {
                tracing::debug!(server = %cmd_name, "lsp stderr: {line}");
            }
        });

        let client = ServerClient {
            child,
            outgoing,
            pending,
            next_id: AtomicI64::new(1),
            _reader: reader,
            _writer: writer,
            _stderr: stderr_thread,
        };

        client.initialize(root)?;
        Ok(client)
    }

    fn notify(&self, method: &str, params: serde_json::Value) {
        let _ = self.outgoing.send(serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    /// Send a request and block up to `timeout` for the matching response.
    fn request(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> std::io::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, tx);
        let _ = self.outgoing.send(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        rx.recv_timeout(timeout).map_err(|_| {
            self.pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            std::io::Error::new(std::io::ErrorKind::TimedOut, "lsp request timed out")
        })
    }

    fn initialize(&self, root: &std::path::Path) -> std::io::Result<()> {
        let root_uri = url::Url::from_file_path(root)
            .map(|u| u.to_string())
            .unwrap_or_default();
        let params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": { "relatedInformation": false }
                }
            },
            "clientInfo": { "name": "vmux" }
        });
        self.request("initialize", params, Duration::from_secs(10))?;
        self.notify("initialized", serde_json::json!({}));
        Ok(())
    }

    pub fn did_open(&self, uri: &str, language_id: &str, version: i32, text: &str) {
        self.notify(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": version,
                    "text": text,
                }
            }),
        );
    }

    pub fn did_change(&self, uri: &str, version: i32, text: &str) {
        // Full-document sync (no editing surface yet).
        self.notify(
            "textDocument/didChange",
            serde_json::json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }]
            }),
        );
    }

    pub fn did_close(&self, uri: &str) {
        self.notify(
            "textDocument/didClose",
            serde_json::json!({ "textDocument": { "uri": uri } }),
        );
    }

    pub fn shutdown(&mut self) {
        let _ = self.request("shutdown", serde_json::Value::Null, Duration::from_secs(2));
        self.notify("exit", serde_json::json!({}));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ServerClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Helper used by the manager to key a spawned server.
pub fn server_key(root: &std::path::Path, spec: &ServerSpec) -> ServerKey {
    (root.to_path_buf(), spec.command.clone())
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
