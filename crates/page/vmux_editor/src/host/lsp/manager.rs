use vmux_core::event::{DiagSeverity, FileDiagnostic, FileLine};

pub fn line_text(line: &FileLine) -> String {
    line.spans.iter().map(|s| s.text.as_str()).collect()
}

pub fn utf16_to_char_col(text: &str, utf16_col: u32) -> u32 {
    let mut utf16 = 0u32;
    let mut chars = 0u32;
    for ch in text.chars() {
        if utf16 >= utf16_col {
            return chars;
        }
        utf16 += ch.len_utf16() as u32;
        chars += 1;
    }
    chars
}

pub fn char_to_utf16_col(text: &str, char_col: u32) -> u32 {
    text.chars()
        .take(char_col as usize)
        .map(|c| c.len_utf16() as u32)
        .sum()
}

fn map_severity(sev: Option<lsp_types::DiagnosticSeverity>) -> DiagSeverity {
    match sev {
        Some(s) if s == lsp_types::DiagnosticSeverity::ERROR => DiagSeverity::Error,
        Some(s) if s == lsp_types::DiagnosticSeverity::WARNING => DiagSeverity::Warning,
        Some(s) if s == lsp_types::DiagnosticSeverity::HINT => DiagSeverity::Hint,
        _ => DiagSeverity::Info,
    }
}

pub fn to_file_diagnostics(
    lines: &[FileLine],
    diags: &[lsp_types::Diagnostic],
) -> Vec<FileDiagnostic> {
    map_diags(diags, |line| {
        lines.get(line as usize).map(line_text).unwrap_or_default()
    })
}

fn map_diags(
    diags: &[lsp_types::Diagnostic],
    line_text: impl Fn(u32) -> String,
) -> Vec<FileDiagnostic> {
    diags
        .iter()
        .map(|d| {
            let line = d.range.start.line;
            let text = line_text(line);
            let start_col = utf16_to_char_col(&text, d.range.start.character);
            let end_col = if d.range.end.line == line {
                utf16_to_char_col(&text, d.range.end.character).max(start_col)
            } else {
                text.chars().count() as u32
            };
            FileDiagnostic {
                line,
                start_col,
                end_col,
                severity: map_severity(d.severity),
                message: d.message.clone(),
                source: d.source.clone(),
            }
        })
        .collect()
}

fn rope_line_text(rope: &ropey::Rope, line: u32) -> String {
    let l = line as usize;
    if l >= rope.len_lines() {
        return String::new();
    }
    rope.line(l)
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .collect()
}

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::lsp::client::{ServerClient, server_key};
use crate::lsp::registry::{ServerSpec, resolve_spec, workspace_root};
use crate::lsp::server_request::{ServerEvent, ServerEvents};
use crate::lsp::{LspOutbox, OpenDoc, ServerKey, store};

type ServerOverrides = std::collections::BTreeMap<String, ServerSpec>;

const LSP_MAX_BYTES: u64 = crate::highlight::HIGHLIGHT_MAX_BYTES;

pub enum ReqKind {
    Hover { line: u32, col: u32 },
    Definition,
    References,
    Rename,
    CodeAction,
    Formatting { path: PathBuf },
    Completion { line: u32, replace_from_col: u32 },
    Folding { path: PathBuf },
    DocumentSymbol,
    SemanticTokens { key: ServerKey, path: PathBuf },
}

pub struct InFlight {
    entity: Entity,
    kind: ReqKind,
    rx: crossbeam_channel::Receiver<serde_json::Value>,
}

#[derive(Message)]
pub struct LspGoto {
    pub entity: Entity,
    pub path: PathBuf,
    pub line: u32,
    pub utf16_col: u32,
}

#[derive(Message)]
pub struct LspFolds {
    pub entity: Entity,
    pub path: PathBuf,
    pub regions: Vec<crate::fold::FoldRegion>,
}

#[derive(Message)]
pub struct LspRequestedEdit {
    pub entity: Entity,
    pub result: Result<lsp_types::WorkspaceEdit, String>,
}
pub fn parse_folding_ranges(value: &serde_json::Value) -> Vec<crate::fold::FoldRegion> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    let s = r.get("startLine")?.as_u64()? as u32;
                    let e = r.get("endLine")?.as_u64()? as u32;
                    (e > s).then_some(crate::fold::FoldRegion { start: s, end: e })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Resource)]
pub struct LspManager {
    servers: HashMap<ServerKey, ServerClient>,
    starting: HashMap<ServerKey, StartingServer>,
    open_docs: HashMap<PathBuf, OpenDoc>,
    failed: HashSet<ServerKey>,
    outbox: LspOutbox,
    events: crossbeam_channel::Sender<ServerEvent>,
    inflight: Vec<InFlight>,
    offered_actions: HashMap<Entity, Vec<lsp_types::CodeActionOrCommand>>,
}

struct StartingServer {
    command: String,
    task: bevy::tasks::Task<std::io::Result<ServerClient>>,
}

enum ServerReadiness {
    Ready(ServerKey),
    Starting,
    Unavailable,
}

fn uri_for(path: &Path) -> Option<String> {
    url::Url::from_file_path(path).ok().map(|u| u.to_string())
}

#[allow(clippy::mutable_key_type)]
fn one_document_edit(
    path: &Path,
    edits: Vec<lsp_types::TextEdit>,
) -> Option<lsp_types::WorkspaceEdit> {
    let uri: lsp_types::Uri = uri_for(path)?.parse().ok()?;
    let mut changes = std::collections::HashMap::new();
    changes.insert(uri, edits);
    Some(lsp_types::WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

fn read_text(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > LSP_MAX_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

impl LspManager {
    pub(crate) fn new(outbox: LspOutbox, events: crossbeam_channel::Sender<ServerEvent>) -> Self {
        Self {
            servers: HashMap::new(),
            starting: HashMap::new(),
            open_docs: HashMap::new(),
            failed: HashSet::new(),
            outbox,
            events,
            inflight: Vec::new(),
            offered_actions: HashMap::new(),
        }
    }

    fn is_open(&self, path: &Path) -> bool {
        self.open_docs.contains_key(path)
    }

    pub(crate) fn document_version(&self, path: &Path) -> Option<i32> {
        self.open_docs.get(path).map(|doc| doc.version)
    }

    fn menu_actions(&self, path: &Path) -> Vec<vmux_core::event::EditorAction> {
        use vmux_core::event::EditorAction;
        let Some(doc) = self.open_docs.get(path) else {
            return Vec::new();
        };
        let Some(client) = self.servers.get(&doc.key) else {
            return Vec::new();
        };
        let offered = [
            (EditorAction::GotoDeclaration, "textDocument/declaration"),
            (
                EditorAction::GotoTypeDefinition,
                "textDocument/typeDefinition",
            ),
            (
                EditorAction::GotoImplementation,
                "textDocument/implementation",
            ),
            (EditorAction::Rename, "textDocument/rename"),
            (EditorAction::FormatDocument, "textDocument/formatting"),
            (
                EditorAction::FormatSelection,
                "textDocument/rangeFormatting",
            ),
            (EditorAction::CodeAction, "textDocument/codeAction"),
        ];
        let mut actions = Vec::new();
        for (action, method) in offered {
            if client.provides(method) {
                actions.push(action);
            }
        }
        actions
    }

    fn ensure_server(
        &mut self,
        root: &Path,
        spec: &crate::lsp::registry::ServerSpec,
    ) -> ServerReadiness {
        let key = server_key(root, spec);
        if self.servers.contains_key(&key) {
            return ServerReadiness::Ready(key);
        }
        if self.failed.contains(&key) {
            return ServerReadiness::Unavailable;
        }
        if self.starting.contains_key(&key) {
            return ServerReadiness::Starting;
        }
        let spec = spec.clone();
        let root = root.to_path_buf();
        let outbox = self.outbox.clone();
        let events = self.events.clone();
        let command = spec.command.clone();
        let task = bevy::tasks::IoTaskPool::get()
            .spawn(async move { ServerClient::spawn(&spec, &root, outbox, events) });
        self.starting.insert(key, StartingServer { command, task });
        ServerReadiness::Starting
    }

    fn settle_starting_servers(&mut self) {
        use bevy::tasks::futures_lite::future;

        let mut settled = Vec::new();
        for (key, starting) in &mut self.starting {
            let Some(result) = future::block_on(future::poll_once(&mut starting.task)) else {
                continue;
            };
            settled.push((key.clone(), starting.command.clone(), result));
        }
        for (key, command, result) in settled {
            self.starting.remove(&key);
            match result {
                Ok(client) => {
                    self.servers.insert(key, client);
                }
                Err(error) => {
                    tracing::warn!(server = %command, "lsp spawn/init failed: {error}");
                    self.failed.insert(key);
                }
            }
        }
    }

    pub fn open(&mut self, path: &Path, overrides: &ServerOverrides) -> bool {
        if let Some(doc) = self.open_docs.get_mut(path) {
            doc.refs += 1;
            return true;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return true;
        };
        let Some(mut spec) = resolve_spec(ext, overrides) else {
            return true;
        };
        match store::resolved_command(&store::default_root(), &spec.command) {
            store::Resolution::Managed(p) => spec.command = p.to_string_lossy().into_owned(),
            store::Resolution::OnPath => {}
            store::Resolution::Missing => {
                tracing::info!(server = %spec.command, "lsp server not installed/on PATH; skipping {ext}");
                return true;
            }
        }
        let dir = path.parent().unwrap_or(path);
        let root = workspace_root(dir, &spec.root_markers);
        let key = match self.ensure_server(&root, &spec) {
            ServerReadiness::Ready(key) => key,
            ServerReadiness::Starting => return false,
            ServerReadiness::Unavailable => return true,
        };
        let (Some(uri), Some(text)) = (uri_for(path), read_text(path)) else {
            return true;
        };
        if let Some(client) = self.servers.get(&key) {
            client.did_open(&uri, &spec.language_id, 1, &text);
            self.open_docs.insert(
                path.to_path_buf(),
                OpenDoc {
                    key,
                    version: 1,
                    refs: 1,
                },
            );
        }
        true
    }

    pub fn change(&mut self, path: &Path) {
        let Some(doc) = self.open_docs.get_mut(path) else {
            return;
        };
        let (Some(uri), Some(text)) = (uri_for(path), read_text(path)) else {
            return;
        };
        doc.version += 1;
        let version = doc.version;
        let key = doc.key.clone();
        if let Some(client) = self.servers.get(&key) {
            client.did_change(&uri, version, &text);
        }
    }

    pub fn change_with_text(&mut self, path: &Path, text: &str) {
        let Some(doc) = self.open_docs.get_mut(path) else {
            return;
        };
        let Some(uri) = uri_for(path) else {
            return;
        };
        doc.version += 1;
        let version = doc.version;
        let key = doc.key.clone();
        if let Some(client) = self.servers.get(&key) {
            client.did_change(&uri, version, text);
        }
    }

    pub fn close(&mut self, path: &Path) {
        let Some(doc) = self.open_docs.get_mut(path) else {
            return;
        };
        doc.refs = doc.refs.saturating_sub(1);
        if doc.refs > 0 {
            return;
        }
        let Some(doc) = self.open_docs.remove(path) else {
            return;
        };
        if let (Some(uri), Some(client)) = (uri_for(path), self.servers.get(&doc.key)) {
            client.did_close(&uri);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn send_doc_request(
        &mut self,
        entity: Entity,
        path: &Path,
        method: &str,
        line: u32,
        utf16_col: u32,
        extra: serde_json::Value,
        kind: ReqKind,
    ) {
        let Some(doc) = self.open_docs.get(path) else {
            return;
        };
        let Some(uri) = uri_for(path) else {
            return;
        };
        let Some(client) = self.servers.get(&doc.key) else {
            return;
        };
        if !client.provides(method) {
            return;
        }
        let mut params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": utf16_col },
        });
        if let (Some(obj), Some(ex)) = (params.as_object_mut(), extra.as_object()) {
            for (k, v) in ex {
                obj.insert(k.clone(), v.clone());
            }
        }
        let (_, rx) = client.send_request(method, params);
        self.inflight.push(InFlight { entity, kind, rx });
    }

    pub fn hover(&mut self, entity: Entity, path: &Path, line: u32, utf16_col: u32, echo_col: u32) {
        self.send_doc_request(
            entity,
            path,
            "textDocument/hover",
            line,
            utf16_col,
            serde_json::json!({}),
            ReqKind::Hover {
                line,
                col: echo_col,
            },
        );
    }

    pub fn definition(&mut self, entity: Entity, path: &Path, line: u32, utf16_col: u32) {
        self.send_doc_request(
            entity,
            path,
            "textDocument/definition",
            line,
            utf16_col,
            serde_json::json!({}),
            ReqKind::Definition,
        );
    }

    pub fn declaration(&mut self, entity: Entity, path: &Path, line: u32, utf16_col: u32) {
        self.goto(entity, path, "textDocument/declaration", line, utf16_col);
    }

    pub fn type_definition(&mut self, entity: Entity, path: &Path, line: u32, utf16_col: u32) {
        self.goto(entity, path, "textDocument/typeDefinition", line, utf16_col);
    }

    pub fn implementation(&mut self, entity: Entity, path: &Path, line: u32, utf16_col: u32) {
        self.goto(entity, path, "textDocument/implementation", line, utf16_col);
    }

    fn goto(&mut self, entity: Entity, path: &Path, method: &str, line: u32, utf16_col: u32) {
        self.send_doc_request(
            entity,
            path,
            method,
            line,
            utf16_col,
            serde_json::json!({}),
            ReqKind::Definition,
        );
    }

    pub fn references(&mut self, entity: Entity, path: &Path, line: u32, utf16_col: u32) {
        self.send_doc_request(
            entity,
            path,
            "textDocument/references",
            line,
            utf16_col,
            serde_json::json!({ "context": { "includeDeclaration": true } }),
            ReqKind::References,
        );
    }

    pub fn code_actions(
        &mut self,
        entity: Entity,
        path: &Path,
        from_line: u32,
        to_line: u32,
        diagnostics: &[lsp_types::Diagnostic],
    ) {
        let overlapping: Vec<&lsp_types::Diagnostic> = diagnostics
            .iter()
            .filter(|d| d.range.start.line <= to_line && d.range.end.line >= from_line)
            .collect();
        let end_col = self.line_len_utf16(path, to_line);
        self.send_doc_request_at(
            entity,
            path,
            "textDocument/codeAction",
            serde_json::json!({
                "range": {
                    "start": { "line": from_line, "character": 0 },
                    "end": { "line": to_line, "character": end_col },
                },
                "context": { "diagnostics": overlapping },
            }),
            ReqKind::CodeAction,
        );
    }

    pub fn run_code_action(
        &mut self,
        entity: Entity,
        index: usize,
        path: &Path,
    ) -> Option<lsp_types::WorkspaceEdit> {
        let chosen = self.offered_actions.get(&entity)?.get(index)?;
        match chosen.clone() {
            lsp_types::CodeActionOrCommand::Command(command) => {
                self.execute_command(path, &command);
                None
            }
            lsp_types::CodeActionOrCommand::CodeAction(action) => {
                if let Some(command) = &action.command {
                    self.execute_command(path, command);
                }
                action.edit
            }
        }
    }

    fn execute_command(&self, path: &Path, command: &lsp_types::Command) {
        let Some(doc) = self.open_docs.get(path) else {
            return;
        };
        let Some(client) = self.servers.get(&doc.key) else {
            return;
        };
        let (_, _rx) = client.send_request(
            "workspace/executeCommand",
            serde_json::json!({
                "command": command.command,
                "arguments": command.arguments.clone().unwrap_or_default(),
            }),
        );
    }

    fn send_doc_request_at(
        &mut self,
        entity: Entity,
        path: &Path,
        method: &str,
        params_extra: serde_json::Value,
        kind: ReqKind,
    ) {
        let Some(doc) = self.open_docs.get(path) else {
            return;
        };
        let Some(uri) = uri_for(path) else {
            return;
        };
        let Some(client) = self.servers.get(&doc.key) else {
            return;
        };
        if !client.provides(method) {
            return;
        }
        let mut params = serde_json::json!({ "textDocument": { "uri": uri } });
        if let (Some(obj), Some(ex)) = (params.as_object_mut(), params_extra.as_object()) {
            for (k, v) in ex {
                obj.insert(k.clone(), v.clone());
            }
        }
        let (_, rx) = client.send_request(method, params);
        self.inflight.push(InFlight { entity, kind, rx });
    }

    pub fn format_document(&mut self, entity: Entity, path: &Path) {
        self.send_format(
            entity,
            path,
            "textDocument/formatting",
            serde_json::json!({}),
        );
    }

    pub fn format_range(&mut self, entity: Entity, path: &Path, from_line: u32, to_line: u32) {
        let end_col = self.line_len_utf16(path, to_line);
        self.send_format(
            entity,
            path,
            "textDocument/rangeFormatting",
            serde_json::json!({
                "range": {
                    "start": { "line": from_line, "character": 0 },
                    "end": { "line": to_line, "character": end_col },
                }
            }),
        );
    }

    fn send_format(&mut self, entity: Entity, path: &Path, method: &str, extra: serde_json::Value) {
        let Some(doc) = self.open_docs.get(path) else {
            return;
        };
        let Some(uri) = uri_for(path) else {
            return;
        };
        let Some(client) = self.servers.get(&doc.key) else {
            return;
        };
        if !client.provides(method) {
            return;
        }
        let mut params = serde_json::json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        });
        if let (Some(obj), Some(ex)) = (params.as_object_mut(), extra.as_object()) {
            for (k, v) in ex {
                obj.insert(k.clone(), v.clone());
            }
        }
        let (_, rx) = client.send_request(method, params);
        self.inflight.push(InFlight {
            entity,
            kind: ReqKind::Formatting {
                path: path.to_path_buf(),
            },
            rx,
        });
    }

    fn line_len_utf16(&self, path: &Path, line: u32) -> u32 {
        let Some(text) = read_text(path) else {
            return 0;
        };
        let Some(l) = text.lines().nth(line as usize) else {
            return 0;
        };
        l.chars().map(|c| c.len_utf16() as u32).sum()
    }

    pub fn rename(
        &mut self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
        new_name: &str,
    ) {
        self.send_doc_request(
            entity,
            path,
            "textDocument/rename",
            line,
            utf16_col,
            serde_json::json!({ "newName": new_name }),
            ReqKind::Rename,
        );
    }

    pub fn completion(
        &mut self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
        replace_from_col: u32,
    ) {
        self.send_doc_request(
            entity,
            path,
            "textDocument/completion",
            line,
            utf16_col,
            serde_json::json!({}),
            ReqKind::Completion {
                line,
                replace_from_col,
            },
        );
    }

    pub fn folding_range(&mut self, entity: Entity, path: &Path) {
        let Some(doc) = self.open_docs.get(path) else {
            return;
        };
        let Some(uri) = uri_for(path) else {
            return;
        };
        let Some(client) = self.servers.get(&doc.key) else {
            return;
        };
        if !client.provides("textDocument/foldingRange") {
            return;
        }
        let params = serde_json::json!({ "textDocument": { "uri": uri } });
        let (_, rx) = client.send_request("textDocument/foldingRange", params);
        self.inflight.push(InFlight {
            entity,
            kind: ReqKind::Folding {
                path: path.to_path_buf(),
            },
            rx,
        });
    }

    pub fn document_symbol(&mut self, entity: Entity, path: &Path) {
        let Some(doc) = self.open_docs.get(path) else {
            return;
        };
        let Some(uri) = uri_for(path) else {
            return;
        };
        let Some(client) = self.servers.get(&doc.key) else {
            return;
        };
        if !client.provides("textDocument/documentSymbol") {
            return;
        }
        let params = serde_json::json!({ "textDocument": { "uri": uri } });
        let (_, rx) = client.send_request("textDocument/documentSymbol", params);
        self.inflight.push(InFlight {
            entity,
            kind: ReqKind::DocumentSymbol,
            rx,
        });
    }

    pub fn semantic_tokens(&mut self, entity: Entity, path: &Path) {
        let Some(doc) = self.open_docs.get(path) else {
            return;
        };
        let Some(uri) = uri_for(path) else {
            return;
        };
        let key = doc.key.clone();
        let Some(client) = self.servers.get(&key) else {
            return;
        };
        if !client.provides("textDocument/semanticTokens/full") {
            return;
        }
        let params = serde_json::json!({ "textDocument": { "uri": uri } });
        let (_, rx) = client.send_request("textDocument/semanticTokens/full", params);
        self.inflight.push(InFlight {
            entity,
            kind: ReqKind::SemanticTokens {
                key,
                path: path.to_path_buf(),
            },
            rx,
        });
    }

    pub fn semantic_legend(
        &self,
        key: &ServerKey,
    ) -> Option<&crate::lsp::semantic::SemanticLegend> {
        self.servers.get(key)?.semantic_legend()
    }
}

fn hover_contents_to_string(c: lsp_types::HoverContents) -> String {
    use lsp_types::{HoverContents, MarkedString};
    let marked = |m: MarkedString| match m {
        MarkedString::String(s) => s,
        MarkedString::LanguageString(ls) => {
            format!("```{}\n{}\n```", ls.language, ls.value)
        }
    };
    match c {
        HoverContents::Scalar(m) => marked(m),
        HoverContents::Array(items) => items
            .into_iter()
            .map(marked)
            .collect::<Vec<_>>()
            .join("\n\n"),
        HoverContents::Markup(mc) => mc.value,
    }
}

fn parse_hover(value: &serde_json::Value) -> Vec<vmux_core::event::HoverBlock> {
    let Some(result) = value.get("result") else {
        return Vec::new();
    };
    if result.is_null() {
        return Vec::new();
    }
    let md = serde_json::from_value::<lsp_types::Hover>(result.clone())
        .map(|h| hover_contents_to_string(h.contents))
        .unwrap_or_default();
    markdown_to_hover_blocks(&md)
}

fn markdown_to_hover_blocks(md: &str) -> Vec<vmux_core::event::HoverBlock> {
    use vmux_core::event::HoverBlock;
    let mut blocks = Vec::new();
    let mut in_code = false;
    let mut lang = String::new();
    let mut buf = String::new();
    let flush_prose = |buf: &mut String, blocks: &mut Vec<HoverBlock>| {
        let t = buf.trim();
        if !t.is_empty() {
            blocks.push(HoverBlock {
                code: false,
                text: t.to_string(),
                lines: Vec::new(),
            });
        }
        buf.clear();
    };
    for line in md.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("```") {
            if in_code {
                blocks.push(HoverBlock {
                    code: true,
                    text: String::new(),
                    lines: crate::highlight::highlight_snippet(&buf, lang.trim()),
                });
                buf.clear();
                in_code = false;
            } else {
                flush_prose(&mut buf, &mut blocks);
                in_code = true;
                lang = rest.trim().to_string();
            }
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
    }
    if in_code {
        blocks.push(HoverBlock {
            code: true,
            text: String::new(),
            lines: crate::highlight::highlight_snippet(&buf, lang.trim()),
        });
    } else {
        flush_prose(&mut buf, &mut blocks);
    }
    blocks
}

fn loc_tuple(uri: &lsp_types::Uri, pos: lsp_types::Position) -> Option<(PathBuf, u32, u32)> {
    let path = crate::lsp::client::path_from_uri(uri.as_str())?;
    Some((path, pos.line, pos.character))
}

fn parse_definition(value: &serde_json::Value) -> Option<(PathBuf, u32, u32)> {
    let result = value.get("result")?;
    if result.is_null() {
        return None;
    }

    use lsp_types::GotoDefinitionResponse::*;
    match serde_json::from_value::<lsp_types::GotoDefinitionResponse>(result.clone()).ok()? {
        Scalar(l) => loc_tuple(&l.uri, l.range.start),
        Array(ls) => ls
            .into_iter()
            .find_map(|l| loc_tuple(&l.uri, l.range.start)),
        Link(lls) => lls
            .into_iter()
            .find_map(|l| loc_tuple(&l.target_uri, l.target_range.start)),
    }
}

fn parse_references(value: &serde_json::Value) -> Vec<(PathBuf, u32, u32)> {
    let Some(result) = value.get("result") else {
        return Vec::new();
    };
    serde_json::from_value::<Vec<lsp_types::Location>>(result.clone())
        .map(|ls| {
            ls.into_iter()
                .filter_map(|l| loc_tuple(&l.uri, l.range.start))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_completion(value: &serde_json::Value) -> Vec<vmux_core::event::CompletionItem> {
    let Some(result) = value.get("result") else {
        return Vec::new();
    };
    if result.is_null() {
        return Vec::new();
    }
    let items = match serde_json::from_value::<lsp_types::CompletionResponse>(result.clone()) {
        Ok(lsp_types::CompletionResponse::Array(a)) => a,
        Ok(lsp_types::CompletionResponse::List(l)) => l.items,
        Err(_) => return Vec::new(),
    };
    items
        .into_iter()
        .take(200)
        .map(|it| {
            let insert_text = it.insert_text.clone().unwrap_or_else(|| it.label.clone());
            vmux_core::event::CompletionItem {
                label: it.label.clone(),
                insert_text,
                detail: it.detail.clone().unwrap_or_default(),
                kind: it.kind.map(|k| format!("{k:?}")).unwrap_or_default(),
            }
        })
        .collect()
}

pub fn disk_line(path: &Path, line: u32) -> String {
    let Ok(content) = std::fs::read_to_string(path) else {
        return String::new();
    };
    content
        .lines()
        .nth(line as usize)
        .unwrap_or_default()
        .to_string()
}

fn ref_display(path: &Path, line: u32) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    format!("{}:{}", name, line + 1)
}

#[derive(Component)]
pub struct LspOpened;

use crate::host::plugin::{EditState, FileView, FileViewport};

fn server_overrides(settings: &vmux_setting::AppSettings) -> ServerOverrides {
    settings
        .editor
        .lsp
        .servers
        .iter()
        .map(|(ext, o)| {
            (
                ext.clone(),
                ServerSpec {
                    command: o.command.clone(),
                    args: o.args.clone(),
                    language_id: o.language_id.clone(),
                    root_markers: o.root_markers.clone(),
                },
            )
        })
        .collect()
}

fn lsp_open_documents(
    q: Query<(Entity, &FileView, &EditState), Without<LspOpened>>,
    settings: Res<vmux_setting::AppSettings>,
    mut manager: ResMut<LspManager>,
    mut commands: Commands,
) {
    manager.settle_starting_servers();
    let overrides = server_overrides(&settings);
    for (entity, fv, _edit) in &q {
        if !manager.open(&fv.path, &overrides) {
            continue;
        }
        manager.folding_range(entity, &fv.path);
        manager.semantic_tokens(entity, &fv.path);
        if !crate::explorer_model::is_markdown(&fv.path) {
            manager.document_symbol(entity, &fv.path);
        }
        commands.entity(entity).insert(LspOpened);
    }
}

fn drain_lsp_requests(
    mut manager: ResMut<LspManager>,
    browsers: NonSend<Browsers>,
    mut goto_w: MessageWriter<LspGoto>,
    mut folds_w: MessageWriter<LspFolds>,
    mut semantic_w: MessageWriter<LspSemantic>,
    mut edit_w: MessageWriter<LspRequestedEdit>,
    mut commands: Commands,
) {
    use vmux_core::event::{
        EXPLORER_OUTLINE_EVENT, FILE_COMPLETION_EVENT, FILE_HOVER_EVENT, FILE_REFERENCES_EVENT,
        FileCompletionEvent, FileHoverEvent, FileReferencesEvent, OutlineEvent, RefItem,
    };
    let drained = std::mem::take(&mut manager.inflight);
    let mut still = Vec::new();
    for f in drained {
        let value = match f.rx.try_recv() {
            Ok(v) => v,
            Err(crossbeam_channel::TryRecvError::Empty) => {
                still.push(f);
                continue;
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => continue,
        };
        let ready = browsers.can_emit_to(&f.entity);
        match f.kind {
            ReqKind::Hover { line, col } => {
                let blocks = parse_hover(&value);
                if !blocks.is_empty() && ready {
                    commands.trigger(BinHostEmitEvent::from_rkyv(
                        f.entity,
                        FILE_HOVER_EVENT,
                        &FileHoverEvent { line, col, blocks },
                    ));
                }
            }
            ReqKind::Definition => {
                if let Some((path, line, utf16_col)) = parse_definition(&value) {
                    goto_w.write(LspGoto {
                        entity: f.entity,
                        path,
                        line,
                        utf16_col,
                    });
                }
            }
            ReqKind::Rename => {
                let result = if value.is_null() {
                    Err("the language server would not rename this".to_string())
                } else {
                    serde_json::from_value::<lsp_types::WorkspaceEdit>(value)
                        .map_err(|e| format!("the rename could not be read: {e}"))
                };
                edit_w.write(LspRequestedEdit {
                    entity: f.entity,
                    result,
                });
            }
            ReqKind::CodeAction => {
                let offered = serde_json::from_value::<Vec<lsp_types::CodeActionOrCommand>>(value)
                    .unwrap_or_default();
                let titles: Vec<String> = offered
                    .iter()
                    .map(|item| match item {
                        lsp_types::CodeActionOrCommand::Command(c) => c.title.clone(),
                        lsp_types::CodeActionOrCommand::CodeAction(a) => a.title.clone(),
                    })
                    .collect();
                manager.offered_actions.insert(f.entity, offered);
                if !ready {
                    continue;
                }
                if titles.is_empty() {
                    commands.trigger(BinHostEmitEvent::from_rkyv(
                        f.entity,
                        vmux_core::event::FILE_EDIT_FAILED_EVENT,
                        &vmux_core::event::FileEditFailedEvent {
                            reason: "no code actions here".to_string(),
                        },
                    ));
                    continue;
                }
                commands.trigger(BinHostEmitEvent::from_rkyv(
                    f.entity,
                    vmux_core::event::FILE_CODE_ACTIONS_EVENT,
                    &vmux_core::event::FileCodeActionsEvent { titles },
                ));
            }
            ReqKind::Formatting { path } => {
                let result = match serde_json::from_value::<Vec<lsp_types::TextEdit>>(value) {
                    Ok(edits) if edits.is_empty() => continue,
                    Ok(edits) => one_document_edit(&path, edits)
                        .ok_or_else(|| format!("{} has no URI to format", path.display())),
                    Err(_) => Err("the language server would not format this".to_string()),
                };
                edit_w.write(LspRequestedEdit {
                    entity: f.entity,
                    result,
                });
            }
            ReqKind::References => {
                let items: Vec<RefItem> = parse_references(&value)
                    .into_iter()
                    .map(|(path, line, utf16_col)| {
                        let text = disk_line(&path, line);
                        let col = utf16_to_char_col(&text, utf16_col);
                        RefItem {
                            display: ref_display(&path, line),
                            path: path.to_string_lossy().into_owned(),
                            line,
                            col,
                            preview: text.trim().to_string(),
                        }
                    })
                    .collect();
                if !items.is_empty() && ready {
                    commands.trigger(BinHostEmitEvent::from_rkyv(
                        f.entity,
                        FILE_REFERENCES_EVENT,
                        &FileReferencesEvent { items },
                    ));
                }
            }
            ReqKind::Completion {
                line,
                replace_from_col,
            } => {
                let items = parse_completion(&value);
                if ready {
                    commands.trigger(BinHostEmitEvent::from_rkyv(
                        f.entity,
                        FILE_COMPLETION_EVENT,
                        &FileCompletionEvent {
                            items,
                            replace_from_col,
                            line,
                        },
                    ));
                }
            }
            ReqKind::Folding { path } => {
                folds_w.write(LspFolds {
                    entity: f.entity,
                    path,
                    regions: parse_folding_ranges(&value),
                });
            }
            ReqKind::DocumentSymbol => {
                let items = crate::explorer_model::flatten_symbols(&value);
                if ready {
                    commands.trigger(BinHostEmitEvent::from_rkyv(
                        f.entity,
                        EXPLORER_OUTLINE_EVENT,
                        &OutlineEvent { items },
                    ));
                }
            }
            ReqKind::SemanticTokens { key, path } => {
                semantic_w.write(LspSemantic {
                    entity: f.entity,
                    path,
                    tokens: parse_semantic_tokens(&value, manager.semantic_legend(&key)),
                });
            }
        }
    }
    manager.inflight = still;
}

fn parse_semantic_tokens(
    value: &serde_json::Value,
    legend: Option<&crate::lsp::semantic::SemanticLegend>,
) -> Vec<crate::lsp::semantic::SemanticToken> {
    let Some(legend) = legend else {
        return Vec::new();
    };
    let Some(data) = value.pointer("/result/data").and_then(|d| d.as_array()) else {
        return Vec::new();
    };
    let data: Vec<u32> = data
        .iter()
        .filter_map(|n| n.as_u64().map(|n| n as u32))
        .collect();
    legend.decode(&data)
}

#[derive(Message)]
pub struct LspSemantic {
    pub entity: Entity,
    pub path: PathBuf,
    pub tokens: Vec<crate::lsp::semantic::SemanticToken>,
}

fn apply_semantic_tokens(
    mut reader: MessageReader<LspSemantic>,
    mut views: Query<(&mut EditState, &FileView, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for message in reader.read() {
        let Ok((mut edit, view, vp)) = views.get_mut(message.entity) else {
            continue;
        };
        if crate::host::plugin::canon(&view.path) != crate::host::plugin::canon(&message.path) {
            continue;
        }
        edit.hl
            .set_semantic(crate::lsp::semantic::SemanticHighlight::of(
                message.tokens.clone(),
            ));
        crate::host::plugin::repaint_window(
            message.entity,
            &mut edit,
            vp,
            &browsers,
            &mut commands,
        );
    }
}

pub fn build(app: &mut App, outbox: LspOutbox) {
    let events = app.world().resource::<ServerEvents>().sender();
    app.insert_resource(LspManager::new(outbox, events))
        .init_resource::<LintOutbox>()
        .init_resource::<DiagState>()
        .add_message::<LspGoto>()
        .add_message::<LspFolds>()
        .add_message::<LspSemantic>()
        .add_message::<LspRequestedEdit>()
        .add_message::<LspCodeActionRequest>()
        .add_systems(
            Update,
            (
                lsp_open_documents,
                lint_on_open,
                drain_lsp_diagnostics,
                drain_lint,
                request_code_actions,
                drain_lsp_requests,
                apply_semantic_tokens,
                emit_diagnostics_system,
                lsp_status_system,
            )
                .chain(),
        );
}

use bevy_cef::prelude::{BinHostEmitEvent, Browsers};
use vmux_core::event::{FILE_DIAGNOSTICS_EVENT, FileDiagnosticsEvent};

use crate::lsp::LintOutbox;

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

#[derive(Resource, Default)]
struct DiagState {
    lsp: HashMap<PathBuf, Vec<FileDiagnostic>>,
    lint: HashMap<PathBuf, Vec<FileDiagnostic>>,
    raw: HashMap<PathBuf, Vec<lsp_types::Diagnostic>>,
}

#[derive(Component, Default)]
pub struct DiagSent(Vec<FileDiagnostic>);

fn emit_diagnostics_system(
    q: Query<(Entity, &FileView, Option<&DiagSent>), With<vmux_core::page::PageReady>>,
    state: Res<DiagState>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, sent) in &q {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let target = canon(&fv.path);
        let mut merged: Vec<FileDiagnostic> = Vec::new();
        if let Some(d) = state.lsp.get(&target) {
            merged.extend(d.iter().cloned());
        }
        if let Some(d) = state.lint.get(&target) {
            merged.extend(d.iter().cloned());
        }
        match sent {
            Some(s) if s.0 == merged => continue,
            None if merged.is_empty() => continue,
            _ => {}
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_DIAGNOSTICS_EVENT,
            &FileDiagnosticsEvent {
                path: fv.path.to_string_lossy().into_owned(),
                diagnostics: merged.clone(),
            },
        ));
        commands.entity(entity).insert(DiagSent(merged));
    }
}

fn drain_lsp_diagnostics(
    outbox: Res<LspOutbox>,
    mut state: ResMut<DiagState>,
    views: Query<(Entity, &FileView, &EditState)>,
) {
    let drained: Vec<(PathBuf, Vec<lsp_types::Diagnostic>)> = {
        let mut q = outbox.0.lock().unwrap_or_else(|p| p.into_inner());
        q.drain(..).collect()
    };
    for (path, diags) in drained {
        let target = canon(&path);
        let mapped = views
            .iter()
            .find(|(_, fv, _)| canon(&fv.path) == target)
            .map(|(_, _, edit)| map_diags(&diags, |l| rope_line_text(&edit.core.buffer.rope, l)))
            .unwrap_or_default();
        state.lsp.insert(target.clone(), mapped);
        state.raw.insert(target, diags);
    }
}

fn drain_lint(outbox: Res<LintOutbox>, mut state: ResMut<DiagState>) {
    let drained: Vec<(PathBuf, Vec<FileDiagnostic>)> = {
        let mut q = outbox.0.lock().unwrap_or_else(|p| p.into_inner());
        q.drain(..).collect()
    };
    for (path, diags) in drained {
        state.lint.insert(canon(&path), diags);
    }
}

#[derive(Message)]
pub struct LspCodeActionRequest {
    pub entity: Entity,
    pub path: PathBuf,
    pub from_line: u32,
    pub to_line: u32,
}

fn request_code_actions(
    mut reader: MessageReader<LspCodeActionRequest>,
    state: Res<DiagState>,
    mut manager: ResMut<LspManager>,
) {
    for request in reader.read() {
        let diagnostics = state
            .raw
            .get(&canon(&request.path))
            .cloned()
            .unwrap_or_default();
        manager.code_actions(
            request.entity,
            &request.path,
            request.from_line,
            request.to_line,
            &diagnostics,
        );
    }
}

#[derive(Component)]
pub struct LintRan;

fn lint_on_open(
    q: Query<(Entity, &FileView, &EditState), Without<LintRan>>,
    outbox: Res<LintOutbox>,
    mut commands: Commands,
) {
    for (entity, fv, _edit) in &q {
        commands.entity(entity).insert(LintRan);
        let Some(ext) = fv.path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(spec) = crate::lsp::registry::linter_for(ext) else {
            continue;
        };
        if matches!(
            store::resolved_command(&store::default_root(), &spec.command),
            store::Resolution::Missing
        ) {
            continue;
        }
        let path = fv.path.clone();
        let sink = outbox.clone();
        std::thread::spawn(move || {
            let diags = crate::lsp::lint::run_linter(&spec, &path);
            sink.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((path, diags));
        });
    }
}

#[derive(Component)]
pub struct LspStatusSent {
    state: vmux_core::event::LspServerState,
    path: PathBuf,
}

fn lsp_status_system(
    q: Query<(Entity, &FileView, Option<&LspStatusSent>), With<vmux_core::page::PageReady>>,
    settings: Res<vmux_setting::AppSettings>,
    manager: Res<LspManager>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    use vmux_core::event::{FILE_LSP_STATUS_EVENT, FileLspStatusEvent, LspServerState};
    let overrides = server_overrides(&settings);
    for (entity, fv, sent) in &q {
        let Some(ext) = fv.path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(spec) = resolve_spec(ext, &overrides) else {
            continue;
        };
        let desired = match store::resolved_command(&store::default_root(), &spec.command) {
            store::Resolution::Missing => LspServerState::Missing,
            _ if manager.is_open(&fv.path) => LspServerState::Ready,
            _ => LspServerState::Starting,
        };
        if sent.is_some_and(|s| s.state == desired && s.path == fv.path) {
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_LSP_STATUS_EVENT,
            &FileLspStatusEvent {
                path: fv.path.to_string_lossy().into_owned(),
                server: spec.command.clone(),
                package: (!overrides.contains_key(ext))
                    .then(|| crate::lsp::registry::preferred_package(ext))
                    .flatten()
                    .map(str::to_string),
                state: desired,
                actions: manager.menu_actions(&fv.path),
            },
        ));
        commands.entity(entity).insert(LspStatusSent {
            state: desired,
            path: fv.path.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::event::StyledSpan;

    #[test]
    fn a_language_string_hover_survives_as_a_highlighted_code_block() {
        let contents = lsp_types::HoverContents::Scalar(lsp_types::MarkedString::LanguageString(
            lsp_types::LanguageString {
                language: "rust".into(),
                value: "fn build(self) -> StartHeroProps".into(),
            },
        ));
        let blocks = markdown_to_hover_blocks(&hover_contents_to_string(contents));

        let [block] = blocks.as_slice() else {
            panic!("one block, got {}", blocks.len());
        };
        assert!(block.code, "a language string is code, not prose");
        let colours: std::collections::HashSet<_> = block
            .lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.fg))
            .collect();
        assert!(
            colours.len() > 1,
            "`fn` and the identifier should not come back the same colour"
        );
    }

    fn fline(no: u32, text: &str) -> FileLine {
        FileLine {
            line_no: no,
            fold: vmux_core::event::FoldGutter::None,
            spans: vec![StyledSpan {
                text: text.into(),
                fg: [0, 0, 0],
                bold: false,
                italic: false,
            }],
            indent_levels: 0,
        }
    }

    fn diag(l0: u32, c0: u32, l1: u32, c1: u32, sev: i32, msg: &str) -> lsp_types::Diagnostic {
        let severity = match sev {
            1 => lsp_types::DiagnosticSeverity::ERROR,
            2 => lsp_types::DiagnosticSeverity::WARNING,
            3 => lsp_types::DiagnosticSeverity::INFORMATION,
            _ => lsp_types::DiagnosticSeverity::HINT,
        };
        lsp_types::Diagnostic {
            range: lsp_types::Range {
                start: lsp_types::Position {
                    line: l0,
                    character: c0,
                },
                end: lsp_types::Position {
                    line: l1,
                    character: c1,
                },
            },
            severity: Some(severity),
            message: msg.into(),
            source: Some("rustc".into()),
            ..Default::default()
        }
    }

    #[test]
    fn ascii_columns_pass_through() {
        let lines = vec![fline(0, "let x = 1;")];
        let out = to_file_diagnostics(&lines, &[diag(0, 4, 0, 5, 1, "unused")]);
        assert_eq!(out[0].start_col, 4);
        assert_eq!(out[0].end_col, 5);
        assert_eq!(out[0].severity, DiagSeverity::Error);
    }

    #[test]
    fn parses_folding_ranges() {
        let v = serde_json::json!([
            { "startLine": 0, "endLine": 3 },
            { "startLine": 1, "endLine": 1 },
        ]);
        let regs = parse_folding_ranges(&v);
        assert_eq!(regs, vec![crate::fold::FoldRegion { start: 0, end: 3 }]);
    }

    #[test]
    fn utf16_emoji_maps_to_char_index() {
        let lines = vec![fline(0, "😀ab")];
        assert_eq!(utf16_to_char_col("😀ab", 2), 1);
        assert_eq!(utf16_to_char_col("😀ab", 3), 2);
        let out = to_file_diagnostics(&lines, &[diag(0, 2, 0, 3, 2, "warn")]);
        assert_eq!(out[0].start_col, 1);
        assert_eq!(out[0].end_col, 2);
        assert_eq!(out[0].severity, DiagSeverity::Warning);
    }

    #[test]
    fn out_of_range_columns_clamp() {
        let lines = vec![fline(0, "ab")];
        let out = to_file_diagnostics(&lines, &[diag(0, 99, 0, 99, 1, "x")]);
        assert_eq!(out[0].start_col, 2);
        assert_eq!(out[0].end_col, 2);
    }

    #[test]
    fn multiline_range_underlines_first_line_to_eol() {
        let lines = vec![fline(0, "abcdef"), fline(1, "ghi")];
        let out = to_file_diagnostics(&lines, &[diag(0, 2, 1, 1, 1, "multi")]);
        assert_eq!(out[0].line, 0);
        assert_eq!(out[0].start_col, 2);
        assert_eq!(out[0].end_col, 6);
    }

    #[test]
    fn drain_empties_outbox() {
        use crate::lsp::LspOutbox;
        use std::path::PathBuf;

        let mut app = App::new();
        let outbox = LspOutbox::default();
        app.add_plugins(MinimalPlugins)
            .insert_resource(outbox.clone());
        outbox
            .0
            .lock()
            .unwrap()
            .push((PathBuf::from("/x.rs"), vec![]));
        app.add_systems(Update, |ob: Res<LspOutbox>| {
            ob.0.lock().unwrap().drain(..).for_each(drop);
        });
        app.update();
        assert!(outbox.0.lock().unwrap().is_empty());
    }

    #[test]
    fn char_utf16_roundtrip_surrogate_pair() {
        let text = "a😀b";
        assert_eq!(char_to_utf16_col(text, 0), 0);
        assert_eq!(char_to_utf16_col(text, 1), 1);
        assert_eq!(char_to_utf16_col(text, 2), 3);
        assert_eq!(char_to_utf16_col(text, 3), 4);
        assert_eq!(utf16_to_char_col(text, 3), 2);
    }

    #[test]
    fn diagnostics_map_through_editstate() {
        use crate::edit::highlight_cache::HighlightCache;
        use crate::edit::{EditCore, EditMode};
        use crate::host::plugin::{EditState, FileView};
        use crate::lsp::LspOutbox;
        use std::path::PathBuf;

        let path = PathBuf::from("/tmp/vmux_lsp_editstate.rs");
        let mut app = App::new();
        let outbox = LspOutbox::default();
        app.add_plugins(MinimalPlugins)
            .init_resource::<DiagState>()
            .insert_resource(outbox.clone())
            .add_systems(Update, drain_lsp_diagnostics);

        let core = EditCore::new(
            path.clone(),
            "Rust".into(),
            "fn a() {}\nlet x = 1;\n",
            EditMode::Insert,
        );
        let hl = HighlightCache::new(&path);
        app.world_mut().spawn((
            FileView { path: path.clone() },
            EditState::new(core, hl, crate::fold::FoldState::default()),
        ));

        let diag = lsp_types::Diagnostic {
            range: lsp_types::Range {
                start: lsp_types::Position {
                    line: 1,
                    character: 4,
                },
                end: lsp_types::Position {
                    line: 1,
                    character: 5,
                },
            },
            message: "boom".into(),
            ..Default::default()
        };
        outbox.0.lock().unwrap().push((path.clone(), vec![diag]));
        app.update();

        let state = app.world().resource::<DiagState>();
        let mapped = state
            .lsp
            .get(&canon(&path))
            .expect("diagnostics mapped for EditState entity");
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].line, 1);
        assert_eq!(mapped[0].start_col, 4);
    }
}
