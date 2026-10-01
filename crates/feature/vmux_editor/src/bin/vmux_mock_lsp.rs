#[cfg(host)]
use serde_json::{Value, json};
#[cfg(host)]
use std::io::{self, BufReader, Write};
#[cfg(host)]
use vmux_editor::lsp::framing::LspFrame;

#[cfg(host)]
struct Diagnostic;

#[cfg(host)]
impl Diagnostic {
    fn message(uri: &str, message: String) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "diagnostics": [{
                    "range": {"start": {"line": 0, "character": 0},
                              "end": {"line": 0, "character": 3}},
                    "severity": 1,
                    "message": message,
                    "source": "mock"
                }]
            }
        })
    }
}

#[cfg(host)]
fn main() {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = io::stdout();
    let mut probe_uri = String::new();

    while let Ok(Some(frame)) = LspFrame::read(&mut reader) {
        let msg = frame.into_message();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let id = msg.get("id").cloned();
        match method {
            "initialize" => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {"capabilities": {"foldingRangeProvider": true}}
                });
                let _ = LspFrame::new(resp).write(&mut stdout);
            }
            "textDocument/foldingRange" => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": [{"startLine": 0, "endLine": 2}]
                });
                let _ = LspFrame::new(resp).write(&mut stdout);
            }
            "textDocument/didOpen" => {
                let uri = msg
                    .pointer("/params/textDocument/uri")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let _ = LspFrame::new(Diagnostic::message(&uri, "mock diagnostic".into()))
                    .write(&mut stdout);
                if uri.contains("probe-requests") {
                    probe_uri = uri;
                    let _ = LspFrame::new(json!({
                        "jsonrpc": "2.0",
                        "id": 1000,
                        "method": "window/showDocument",
                        "params": {"uri": probe_uri},
                    }))
                    .write(&mut stdout);
                }
            }
            "shutdown" => {
                let resp = json!({"jsonrpc": "2.0", "id": id, "result": null});
                let _ = LspFrame::new(resp).write(&mut stdout);
            }
            "exit" => break,
            "" if id.is_some() => {
                let code = msg
                    .pointer("/error/code")
                    .and_then(Value::as_i64)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "ok".to_string());
                let _ = LspFrame::new(Diagnostic::message(&probe_uri, format!("answered {code}")))
                    .write(&mut stdout);
            }
            _ => {}
        }
        let _ = stdout.flush();
    }
}

#[cfg(not(host))]
fn main() {}
