//! Minimal mock LSP server for integration tests.
//! - Responds to `initialize` with empty capabilities.
//! - On `textDocument/didOpen`, emits one diagnostic for the opened uri.
//! - Responds to `shutdown`; exits on `exit`.

use std::io::{self, BufReader, Write};

use serde_json::{json, Value};
use vmux_editor::lsp::framing::{read_message, write_message};

fn main() {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = io::stdout();

    while let Ok(Some(msg)) = read_message(&mut reader) {
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let id = msg.get("id").cloned();
        match method {
            "initialize" => {
                let resp = json!({"jsonrpc": "2.0", "id": id, "result": {"capabilities": {}}});
                let _ = write_message(&mut stdout, &resp);
            }
            "textDocument/didOpen" => {
                let uri = msg
                    .pointer("/params/textDocument/uri")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let note = json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": {
                        "uri": uri,
                        "diagnostics": [{
                            "range": {"start": {"line": 0, "character": 0},
                                      "end": {"line": 0, "character": 3}},
                            "severity": 1,
                            "message": "mock diagnostic",
                            "source": "mock"
                        }]
                    }
                });
                let _ = write_message(&mut stdout, &note);
            }
            "shutdown" => {
                let resp = json!({"jsonrpc": "2.0", "id": id, "result": null});
                let _ = write_message(&mut stdout, &resp);
            }
            "exit" => break,
            _ => {}
        }
        let _ = stdout.flush();
    }
}
