#[cfg(host)]
fn main() {
    use std::io::{self, BufReader, Write};

    use serde_json::{Value, json};
    use vmux_editor::lsp::framing::{read_message, write_message};

    fn diagnostic(uri: &str, message: String) -> Value {
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

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = io::stdout();
    let mut probe_uri = String::new();

    while let Ok(Some(msg)) = read_message(&mut reader) {
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let id = msg.get("id").cloned();
        match method {
            "initialize" => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {"capabilities": {"foldingRangeProvider": true}}
                });
                let _ = write_message(&mut stdout, &resp);
            }
            "textDocument/foldingRange" => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": [{"startLine": 0, "endLine": 2}]
                });
                let _ = write_message(&mut stdout, &resp);
            }
            "textDocument/didOpen" => {
                let uri = msg
                    .pointer("/params/textDocument/uri")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let _ = write_message(&mut stdout, &diagnostic(&uri, "mock diagnostic".into()));
                if uri.contains("probe-requests") {
                    probe_uri = uri;
                    let _ = write_message(
                        &mut stdout,
                        &json!({
                            "jsonrpc": "2.0",
                            "id": 1000,
                            "method": "window/showDocument",
                            "params": {"uri": probe_uri},
                        }),
                    );
                }
            }
            "shutdown" => {
                let resp = json!({"jsonrpc": "2.0", "id": id, "result": null});
                let _ = write_message(&mut stdout, &resp);
            }
            "exit" => break,
            "" if id.is_some() => {
                let code = msg
                    .pointer("/error/code")
                    .and_then(Value::as_i64)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "ok".to_string());
                let _ = write_message(
                    &mut stdout,
                    &diagnostic(&probe_uri, format!("answered {code}")),
                );
            }
            _ => {}
        }
        let _ = stdout.flush();
    }
}

#[cfg(not(host))]
fn main() {}
