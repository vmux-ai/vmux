use std::path::PathBuf;

use serde_json::Value;

use crate::lsp::{LspOutbox, PendingMap};

pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}

pub fn dispatch_message(msg: Value, pending: &PendingMap, outbox: &LspOutbox) {
    if let Some(id) = msg.get("id").and_then(|v| v.as_i64())
        && msg.get("method").is_none()
    {
        if let Some(tx) = pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id)
        {
            let _ = tx.send(msg);
        }
        return;
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
use crate::lsp::{ServerKey, framing};

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
    pub fn spawn(
        spec: &ServerSpec,
        root: &std::path::Path,
        outbox: LspOutbox,
    ) -> std::io::Result<Self> {
        let store_root = crate::lsp::store::default_root();
        let mut child = Command::new(&spec.command)
            .args(&spec.args)
            .current_dir(root)
            .env("PATH", crate::lsp::store::server_path_env(&store_root))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));

        let (outgoing, out_rx) = mpsc::channel::<serde_json::Value>();
        let writer = std::thread::spawn(move || {
            let mut w = stdin;
            while let Ok(msg) = out_rx.recv() {
                if framing::write_message(&mut w, &msg).is_err() {
                    break;
                }
            }
        });

        let r_pending = pending.clone();
        let r_outbox = outbox.clone();
        let reader = std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            while let Ok(Some(msg)) = framing::read_message(&mut r) {
                dispatch_message(msg, &r_pending, &r_outbox);
            }
        });

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

    pub fn send_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> (i64, mpsc::Receiver<serde_json::Value>) {
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
        (id, rx)
    }

    fn request(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> std::io::Result<serde_json::Value> {
        let (id, rx) = self.send_request(method, params);
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
                    "publishDiagnostics": { "relatedInformation": false },
                    "documentSymbol": { "hierarchicalDocumentSymbolSupport": true }
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

pub fn server_key(root: &std::path::Path, spec: &ServerSpec) -> ServerKey {
    (root.to_path_buf(), spec.command.clone())
}

#[cfg(test)]
#[path = "client.test.rs"]
mod tests;
