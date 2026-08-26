use std::path::PathBuf;

use crate::lsp::{LspOutbox, PendingMap};

pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}

use std::io::BufReader;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use std::collections::HashMap;

use crate::lsp::reader::Reader;
use crate::lsp::registry::ServerSpec;
use crate::lsp::server_request::ServerEvent;
use crate::lsp::{ServerKey, framing};

pub struct ServerClient {
    child: Child,
    outgoing: mpsc::Sender<serde_json::Value>,
    pending: PendingMap,
    next_id: AtomicI64,
    capabilities: Capabilities,
    _reader: JoinHandle<()>,
    _writer: JoinHandle<()>,
    _stderr: JoinHandle<()>,
}

#[derive(Default)]
pub struct Capabilities {
    server: lsp_types::ServerCapabilities,
    semantic: Option<crate::lsp::semantic::SemanticLegend>,
}

impl Capabilities {
    fn of(reply: &serde_json::Value) -> Self {
        let server: lsp_types::ServerCapabilities = reply
            .get("result")
            .and_then(|result| result.get("capabilities"))
            .cloned()
            .and_then(|caps| serde_json::from_value(caps).ok())
            .unwrap_or_default();
        let semantic = crate::lsp::semantic::SemanticLegend::of(&server);
        Self { server, semantic }
    }

    pub fn semantic_legend(&self) -> Option<&crate::lsp::semantic::SemanticLegend> {
        self.semantic.as_ref()
    }

    pub fn allows(&self, method: &str) -> bool {
        let caps = &self.server;
        match method {
            "textDocument/hover" => match &caps.hover_provider {
                Some(lsp_types::HoverProviderCapability::Simple(yes)) => *yes,
                Some(lsp_types::HoverProviderCapability::Options(_)) => true,
                None => false,
            },
            "textDocument/foldingRange" => match &caps.folding_range_provider {
                Some(lsp_types::FoldingRangeProviderCapability::Simple(yes)) => *yes,
                Some(_) => true,
                None => false,
            },
            "textDocument/codeAction" => match &caps.code_action_provider {
                Some(lsp_types::CodeActionProviderCapability::Simple(yes)) => *yes,
                Some(lsp_types::CodeActionProviderCapability::Options(_)) => true,
                None => false,
            },
            "textDocument/semanticTokens/full" => caps.semantic_tokens_provider.is_some(),
            "textDocument/completion" => caps.completion_provider.is_some(),
            "textDocument/definition" => Self::offered(&caps.definition_provider),
            "textDocument/declaration" => caps.declaration_provider.is_some(),
            "textDocument/typeDefinition" => caps.type_definition_provider.is_some(),
            "textDocument/implementation" => caps.implementation_provider.is_some(),
            "textDocument/references" => Self::offered(&caps.references_provider),
            "textDocument/documentSymbol" => Self::offered(&caps.document_symbol_provider),
            "textDocument/rename" => Self::offered(&caps.rename_provider),
            "textDocument/formatting" => Self::offered(&caps.document_formatting_provider),
            "textDocument/rangeFormatting" => {
                Self::offered(&caps.document_range_formatting_provider)
            }
            _ => true,
        }
    }

    fn offered<T>(provider: &Option<lsp_types::OneOf<bool, T>>) -> bool {
        match provider {
            Some(lsp_types::OneOf::Left(yes)) => *yes,
            Some(lsp_types::OneOf::Right(_)) => true,
            None => false,
        }
    }
}

impl ServerClient {
    pub fn spawn(
        spec: &ServerSpec,
        root: &std::path::Path,
        outbox: LspOutbox,
        events: crossbeam_channel::Sender<ServerEvent>,
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

        let dispatcher = Reader::new(pending.clone(), outbox, outgoing.clone(), events, root);
        let reader = std::thread::spawn(move || dispatcher.run(stdout));

        let cmd_name = spec.command.clone();
        let stderr_thread = std::thread::spawn(move || {
            use std::io::BufRead;
            let r = BufReader::new(stderr);
            for line in r.lines().map_while(Result::ok) {
                tracing::debug!(server = %cmd_name, "lsp stderr: {line}");
            }
        });

        let mut client = ServerClient {
            child,
            outgoing,
            pending,
            next_id: AtomicI64::new(1),
            capabilities: Capabilities::default(),
            _reader: reader,
            _writer: writer,
            _stderr: stderr_thread,
        };

        client.capabilities = client.initialize(root)?;
        Ok(client)
    }

    pub fn provides(&self, method: &str) -> bool {
        self.capabilities.allows(method)
    }

    pub fn semantic_legend(&self) -> Option<&crate::lsp::semantic::SemanticLegend> {
        self.capabilities.semantic_legend()
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
    ) -> (i64, crossbeam_channel::Receiver<serde_json::Value>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = crossbeam_channel::unbounded();
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

    fn initialize(&self, root: &std::path::Path) -> std::io::Result<Capabilities> {
        let root_uri = url::Url::from_file_path(root)
            .map(|u| u.to_string())
            .unwrap_or_default();
        let params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": Self::capabilities(),
            "clientInfo": { "name": "vmux" }
        });
        let reply = self.request("initialize", params, Duration::from_secs(10))?;
        self.notify("initialized", serde_json::json!({}));
        Ok(Capabilities::of(&reply))
    }

    fn capabilities() -> lsp_types::ClientCapabilities {
        lsp_types::ClientCapabilities {
            workspace: Some(lsp_types::WorkspaceClientCapabilities {
                apply_edit: Some(true),
                workspace_edit: Some(lsp_types::WorkspaceEditClientCapabilities {
                    document_changes: Some(true),
                    resource_operations: Some(vec![]),
                    failure_handling: Some(lsp_types::FailureHandlingKind::Abort),
                    ..Default::default()
                }),
                configuration: Some(true),
                workspace_folders: Some(true),
                ..Default::default()
            }),
            text_document: Some(lsp_types::TextDocumentClientCapabilities {
                hover: Some(lsp_types::HoverClientCapabilities {
                    content_format: Some(vec![lsp_types::MarkupKind::Markdown]),
                    ..Default::default()
                }),
                publish_diagnostics: Some(lsp_types::PublishDiagnosticsClientCapabilities {
                    related_information: Some(false),
                    ..Default::default()
                }),
                document_symbol: Some(lsp_types::DocumentSymbolClientCapabilities {
                    hierarchical_document_symbol_support: Some(true),
                    ..Default::default()
                }),
                semantic_tokens: Some(lsp_types::SemanticTokensClientCapabilities {
                    requests: lsp_types::SemanticTokensClientCapabilitiesRequests {
                        full: Some(lsp_types::SemanticTokensFullOptions::Bool(true)),
                        range: Some(false),
                    },
                    token_types: crate::lsp::semantic::SEMANTIC_TOKEN_TYPES
                        .iter()
                        .map(|name| lsp_types::SemanticTokenType::new(name))
                        .collect(),
                    token_modifiers: Vec::new(),
                    formats: vec![lsp_types::TokenFormat::RELATIVE],
                    ..Default::default()
                }),
                ..Default::default()
            }),
            window: Some(lsp_types::WindowClientCapabilities {
                work_done_progress: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        }
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
mod tests {
    use super::*;
    use serde_json::json;

    fn advertising(capabilities: serde_json::Value) -> Capabilities {
        Capabilities::of(&json!({ "id": 1, "result": { "capabilities": capabilities } }))
    }

    #[test]
    fn a_server_that_said_nothing_is_asked_for_nothing() {
        let caps = advertising(json!({}));
        assert!(!caps.allows("textDocument/hover"));
        assert!(!caps.allows("textDocument/foldingRange"));
        assert!(!caps.allows("textDocument/documentSymbol"));
    }

    #[test]
    fn an_explicit_false_is_honoured() {
        assert!(
            !advertising(json!({ "definitionProvider": false })).allows("textDocument/definition")
        );
        assert!(
            advertising(json!({ "definitionProvider": true })).allows("textDocument/definition")
        );
    }

    #[test]
    fn options_objects_count_as_provided() {
        let caps = advertising(json!({
            "renameProvider": { "prepareProvider": true },
            "codeActionProvider": { "codeActionKinds": ["quickfix"] },
            "hoverProvider": { "workDoneProgress": false },
        }));
        assert!(caps.allows("textDocument/rename"));
        assert!(caps.allows("textDocument/codeAction"));
        assert!(caps.allows("textDocument/hover"));
    }

    #[test]
    fn a_method_we_do_not_model_is_not_blocked() {
        assert!(advertising(json!({})).allows("textDocument/didOpen"));
    }

    #[test]
    fn an_unparseable_reply_provides_nothing() {
        let caps = Capabilities::of(&json!({ "id": 1, "result": "not an object" }));
        assert!(!caps.allows("textDocument/hover"));
    }
}
