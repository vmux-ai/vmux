#[cfg(host)]
use serde_json::{Value, json};
#[cfg(host)]
use std::fs::OpenOptions;
#[cfg(host)]
use std::io::{self, BufReader, Write};
#[cfg(host)]
use vmux_ecs::event::DiagSeverity;
#[cfg(host)]
use vmux_editor::lsp::framing::LspFrame;

#[cfg(host)]
struct MockLsp {
    probe_uri: String,
    recording: bool,
    severity: DiagSeverity,
}

#[cfg(host)]
impl MockLsp {
    fn diagnostic(&self, uri: &str, message: String) -> Value {
        let severity = match self.severity {
            DiagSeverity::Error => 1,
            DiagSeverity::Warning => 2,
            DiagSeverity::Info => 3,
            DiagSeverity::Hint => 4,
        };
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "diagnostics": [{
                    "range": {"start": {"line": 0, "character": 0},
                              "end": {"line": 0, "character": 3}},
                    "severity": severity,
                    "message": message,
                    "source": "mock"
                }]
            }
        })
    }

    fn handle(&mut self, msg: Value, stdout: &mut io::Stdout) -> bool {
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let id = msg.get("id").cloned();
        match method {
            "initialize" => {
                let mut capabilities = json!({"foldingRangeProvider": true});
                if self.recording {
                    capabilities["documentSymbolProvider"] = json!(true);
                    capabilities["semanticTokensProvider"] = json!({
                        "legend": {"tokenTypes": ["type"], "tokenModifiers": []},
                        "full": true
                    });
                }
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {"capabilities": capabilities}
                });
                let _ = LspFrame::new(resp).write(stdout);
            }
            "textDocument/foldingRange" => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": if self.recording { json!([]) } else { json!([{"startLine": 0, "endLine": 2}]) }
                });
                let _ = LspFrame::new(resp).write(stdout);
            }
            "textDocument/documentSymbol" => {
                let _ = LspFrame::new(json!({"jsonrpc":"2.0", "id":id, "result":[]})).write(stdout);
            }
            "textDocument/semanticTokens/full" => {
                let _ = LspFrame::new(json!({"jsonrpc":"2.0", "id":id, "result":{"data":[]}}))
                    .write(stdout);
            }
            "textDocument/didOpen" if !self.recording => {
                let uri = msg
                    .pointer("/params/textDocument/uri")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let _ =
                    LspFrame::new(self.diagnostic(&uri, "mock diagnostic".into())).write(stdout);
                if uri.contains("probe-requests") {
                    self.probe_uri = uri;
                    let _ = LspFrame::new(json!({
                        "jsonrpc": "2.0",
                        "id": 1000,
                        "method": "window/showDocument",
                        "params": {"uri": self.probe_uri},
                    }))
                    .write(stdout);
                }
            }
            "shutdown" => {
                let resp = json!({"jsonrpc": "2.0", "id": id, "result": null});
                let _ = LspFrame::new(resp).write(stdout);
            }
            "exit" => return false,
            "" if id.is_some() => {
                let code = msg
                    .pointer("/error/code")
                    .and_then(Value::as_i64)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "ok".to_string());
                let _ = LspFrame::new(self.diagnostic(&self.probe_uri, format!("answered {code}")))
                    .write(stdout);
            }
            _ => {}
        }
        true
    }
}

fn main() {
    #[cfg(host)]
    {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        let mut stdout = io::stdout();
        let recording = std::env::args().any(|arg| arg == "--record");
        let mut record = if recording {
            Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("lsp-wire.jsonl")
                    .expect("open LSP wire recording"),
            )
        } else {
            None
        };
        let mut server = MockLsp {
            probe_uri: String::new(),
            recording,
            severity: DiagSeverity::Error,
        };
        while let Ok(Some(frame)) = LspFrame::read(&mut reader) {
            let msg = frame.into_message();
            if let Some(record) = &mut record {
                let mut line = serde_json::to_vec(&msg).expect("encode LSP recording line");
                line.push(b'\n');
                record.write_all(&line).expect("record LSP message");
            }
            if !server.handle(msg, &mut stdout) {
                break;
            }
            let _ = stdout.flush();
        }
    }
}
