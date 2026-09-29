use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, UiInput};
use vmux_core::event::{
    CompletionItem, DiagSeverity, EditorCapability, FileCodeActionPick, FileCodeActions,
    FileDiagnostic, FileDiagnostics, FileEditFailure, FileHover, FileLine, FileLspStatus,
    HoverBlock, LspServerState, OutlineEvent, RefItem,
};
use vmux_core::host::FileUiStateWrite;
use vmux_core::page::PageReady;
use vmux_path::PathIdentity;
use vmux_setting::AppSettings;

use crate::host::editor::{Editor, FileView};
use crate::host::viewport::ViewportRenderRequest;
use crate::lsp::client::ServerClient;
use crate::lsp::registry::{ServerSpec, resolve_spec, workspace_root};
use crate::lsp::server_request::ServerInputSender;
use crate::lsp::{
    LintDiagnosticsInbox, LintDiagnosticsSender, LspDiagnosticsInbox, LspDiagnosticsSender,
    OpenDoc, ServerKey, store,
};

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

type ServerOverrides = std::collections::BTreeMap<String, ServerSpec>;

const LSP_MAX_BYTES: u64 = crate::highlight::HIGHLIGHT_MAX_BYTES;

enum ReqKind {
    Hover { line: u32, col: u32 },
    Definition,
    References,
    Rename { root: PathBuf },
    CodeAction,
    Formatting { path: PathBuf, root: PathBuf },
    Completion { line: u32, replace_from_col: u32 },
    Folding { path: PathBuf },
    DocumentSymbol,
    SemanticTokens { key: ServerKey, path: PathBuf },
}

#[derive(Component)]
pub(crate) struct LspRequestOperation {
    target: Entity,
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
    pub root: PathBuf,
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

#[derive(Component)]
pub struct LspManager {
    servers: HashMap<ServerKey, ServerClient>,
    open_docs: HashMap<PathBuf, OpenDoc>,
    diagnostics: LspDiagnosticsSender,
    inputs: ServerInputSender,
}

#[derive(Component)]
struct LspServerStartTask {
    key: ServerKey,
    command: String,
    task: bevy::tasks::Task<std::io::Result<ServerClient>>,
}

#[derive(Component)]
struct LspServerFailed(ServerKey);

#[derive(Message)]
pub(crate) struct LspDocumentChangeRequest {
    pub(crate) path: PathBuf,
    pub(crate) text: Option<String>,
}

#[derive(Message)]
pub(crate) struct LspDocumentCloseRequest {
    pub(crate) path: PathBuf,
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
    pub(crate) fn new(diagnostics: LspDiagnosticsSender, inputs: ServerInputSender) -> Self {
        Self {
            servers: HashMap::new(),
            open_docs: HashMap::new(),
            diagnostics,
            inputs,
        }
    }

    fn is_open(&self, path: &Path) -> bool {
        self.open_docs.contains_key(path)
    }

    pub(crate) fn document_version(&self, path: &Path) -> Option<i32> {
        self.open_docs.get(path).map(|doc| doc.version)
    }

    fn menu_capabilities(&self, path: &Path) -> Vec<EditorCapability> {
        let Some(doc) = self.open_docs.get(path) else {
            return Vec::new();
        };
        let Some(client) = self.servers.get(&doc.key) else {
            return Vec::new();
        };
        let offered = [
            (
                EditorCapability::GotoDeclaration,
                "textDocument/declaration",
            ),
            (
                EditorCapability::GotoTypeDefinition,
                "textDocument/typeDefinition",
            ),
            (
                EditorCapability::GotoImplementation,
                "textDocument/implementation",
            ),
            (EditorCapability::Rename, "textDocument/rename"),
            (EditorCapability::FormatDocument, "textDocument/formatting"),
            (
                EditorCapability::FormatSelection,
                "textDocument/rangeFormatting",
            ),
            (EditorCapability::CodeAction, "textDocument/codeAction"),
        ];
        let mut operations = Vec::new();
        for (operation, method) in offered {
            if client.provides(method) {
                operations.push(operation);
            }
        }
        operations
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
    ) -> Option<LspRequestOperation> {
        let doc = self.open_docs.get(path)?;
        let uri = uri_for(path)?;
        let client = self.servers.get(&doc.key)?;
        if !client.provides(method) {
            return None;
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
        Some(LspRequestOperation {
            target: entity,
            kind,
            rx,
        })
    }

    pub(crate) fn hover(
        &mut self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
        echo_col: u32,
    ) -> Option<LspRequestOperation> {
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
        )
    }

    pub(crate) fn definition(
        &mut self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
    ) -> Option<LspRequestOperation> {
        self.send_doc_request(
            entity,
            path,
            "textDocument/definition",
            line,
            utf16_col,
            serde_json::json!({}),
            ReqKind::Definition,
        )
    }

    pub(crate) fn declaration(
        &mut self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
    ) -> Option<LspRequestOperation> {
        self.goto(entity, path, "textDocument/declaration", line, utf16_col)
    }

    pub(crate) fn type_definition(
        &mut self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
    ) -> Option<LspRequestOperation> {
        self.goto(entity, path, "textDocument/typeDefinition", line, utf16_col)
    }

    pub(crate) fn implementation(
        &mut self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
    ) -> Option<LspRequestOperation> {
        self.goto(entity, path, "textDocument/implementation", line, utf16_col)
    }

    fn goto(
        &mut self,
        entity: Entity,
        path: &Path,
        method: &str,
        line: u32,
        utf16_col: u32,
    ) -> Option<LspRequestOperation> {
        self.send_doc_request(
            entity,
            path,
            method,
            line,
            utf16_col,
            serde_json::json!({}),
            ReqKind::Definition,
        )
    }

    pub(crate) fn references(
        &mut self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
    ) -> Option<LspRequestOperation> {
        self.send_doc_request(
            entity,
            path,
            "textDocument/references",
            line,
            utf16_col,
            serde_json::json!({ "context": { "includeDeclaration": true } }),
            ReqKind::References,
        )
    }

    pub(crate) fn code_actions(
        &mut self,
        entity: Entity,
        path: &Path,
        from_line: u32,
        to_line: u32,
        diagnostics: &[lsp_types::Diagnostic],
    ) -> Option<LspRequestOperation> {
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
        )
    }

    fn send_doc_request_at(
        &mut self,
        entity: Entity,
        path: &Path,
        method: &str,
        params_extra: serde_json::Value,
        kind: ReqKind,
    ) -> Option<LspRequestOperation> {
        let doc = self.open_docs.get(path)?;
        let uri = uri_for(path)?;
        let client = self.servers.get(&doc.key)?;
        if !client.provides(method) {
            return None;
        }
        let mut params = serde_json::json!({ "textDocument": { "uri": uri } });
        if let (Some(obj), Some(ex)) = (params.as_object_mut(), params_extra.as_object()) {
            for (k, v) in ex {
                obj.insert(k.clone(), v.clone());
            }
        }
        let (_, rx) = client.send_request(method, params);
        Some(LspRequestOperation {
            target: entity,
            kind,
            rx,
        })
    }

    pub(crate) fn format_document(
        &mut self,
        entity: Entity,
        path: &Path,
    ) -> Option<LspRequestOperation> {
        self.send_format(
            entity,
            path,
            "textDocument/formatting",
            serde_json::json!({}),
        )
    }

    pub(crate) fn format_range(
        &mut self,
        entity: Entity,
        path: &Path,
        from_line: u32,
        to_line: u32,
    ) -> Option<LspRequestOperation> {
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
        )
    }

    fn send_format(
        &mut self,
        entity: Entity,
        path: &Path,
        method: &str,
        extra: serde_json::Value,
    ) -> Option<LspRequestOperation> {
        let doc = self.open_docs.get(path)?;
        let uri = uri_for(path)?;
        let client = self.servers.get(&doc.key)?;
        let root = doc.key.root().to_path_buf();
        if !client.provides(method) {
            return None;
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
        Some(LspRequestOperation {
            target: entity,
            kind: ReqKind::Formatting {
                path: path.to_path_buf(),
                root,
            },
            rx,
        })
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

    pub(crate) fn rename(
        &mut self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
        new_name: &str,
    ) -> Option<LspRequestOperation> {
        let root = self
            .open_docs
            .get(path)
            .map(|document| document.key.root().to_path_buf())?;
        self.send_doc_request(
            entity,
            path,
            "textDocument/rename",
            line,
            utf16_col,
            serde_json::json!({ "newName": new_name }),
            ReqKind::Rename { root },
        )
    }

    pub(crate) fn completion(
        &mut self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
        replace_from_col: u32,
    ) -> Option<LspRequestOperation> {
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
        )
    }

    pub(crate) fn folding_range(
        &mut self,
        entity: Entity,
        path: &Path,
    ) -> Option<LspRequestOperation> {
        let doc = self.open_docs.get(path)?;
        let uri = uri_for(path)?;
        let client = self.servers.get(&doc.key)?;
        if !client.provides("textDocument/foldingRange") {
            return None;
        }
        let params = serde_json::json!({ "textDocument": { "uri": uri } });
        let (_, rx) = client.send_request("textDocument/foldingRange", params);
        Some(LspRequestOperation {
            target: entity,
            kind: ReqKind::Folding {
                path: path.to_path_buf(),
            },
            rx,
        })
    }

    pub(crate) fn document_symbol(
        &mut self,
        entity: Entity,
        path: &Path,
    ) -> Option<LspRequestOperation> {
        let doc = self.open_docs.get(path)?;
        let uri = uri_for(path)?;
        let client = self.servers.get(&doc.key)?;
        if !client.provides("textDocument/documentSymbol") {
            return None;
        }
        let params = serde_json::json!({ "textDocument": { "uri": uri } });
        let (_, rx) = client.send_request("textDocument/documentSymbol", params);
        Some(LspRequestOperation {
            target: entity,
            kind: ReqKind::DocumentSymbol,
            rx,
        })
    }

    pub(crate) fn semantic_tokens(
        &mut self,
        entity: Entity,
        path: &Path,
    ) -> Option<LspRequestOperation> {
        let doc = self.open_docs.get(path)?;
        let uri = uri_for(path)?;
        let key = doc.key.clone();
        let client = self.servers.get(&key)?;
        if !client.provides("textDocument/semanticTokens/full") {
            return None;
        }
        let params = serde_json::json!({ "textDocument": { "uri": uri } });
        let (_, rx) = client.send_request("textDocument/semanticTokens/full", params);
        Some(LspRequestOperation {
            target: entity,
            kind: ReqKind::SemanticTokens {
                key,
                path: path.to_path_buf(),
            },
            rx,
        })
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

fn parse_hover(value: &serde_json::Value) -> Vec<HoverBlock> {
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

fn markdown_to_hover_blocks(md: &str) -> Vec<HoverBlock> {
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

fn parse_completion(value: &serde_json::Value) -> Vec<CompletionItem> {
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
            CompletionItem {
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

fn server_overrides(settings: &AppSettings) -> ServerOverrides {
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

fn finish_lsp_server_starts(
    mut starts: Query<(Entity, &mut LspServerStartTask)>,
    mut manager: Single<&mut LspManager>,
    mut commands: Commands,
) {
    use bevy::tasks::futures_lite::future;

    for (entity, mut start) in &mut starts {
        let Some(result) = future::block_on(future::poll_once(&mut start.task)) else {
            continue;
        };
        let key = start.key.clone();
        let command = start.command.clone();
        match result {
            Ok(client) => {
                manager.servers.insert(key, client);
                commands.entity(entity).despawn();
            }
            Err(error) => {
                tracing::warn!(server = %command, "lsp spawn/init failed: {error}");
                commands
                    .entity(entity)
                    .insert((
                        Name::new(format!("Failed LSP server {command}")),
                        LspServerFailed(key),
                    ))
                    .remove::<LspServerStartTask>();
            }
        }
    }
}

fn change_lsp_documents(
    mut requests: MessageReader<LspDocumentChangeRequest>,
    mut manager: Single<&mut LspManager>,
) {
    for request in requests.read() {
        let Some(document) = manager.open_docs.get_mut(&request.path) else {
            continue;
        };
        let Some(uri) = uri_for(&request.path) else {
            continue;
        };
        let text = match &request.text {
            Some(text) => text.clone(),
            None => {
                let Some(text) = read_text(&request.path) else {
                    continue;
                };
                text
            }
        };
        document.version += 1;
        let version = document.version;
        let key = document.key.clone();
        if let Some(client) = manager.servers.get(&key) {
            client.did_change(&uri, version, &text);
        }
    }
}

fn close_lsp_documents(
    mut requests: MessageReader<LspDocumentCloseRequest>,
    mut manager: Single<&mut LspManager>,
) {
    for request in requests.read() {
        let Some(document) = manager.open_docs.get_mut(&request.path) else {
            continue;
        };
        document.refs = document.refs.saturating_sub(1);
        if document.refs > 0 {
            continue;
        }
        let Some(document) = manager.open_docs.remove(&request.path) else {
            continue;
        };
        let Some(uri) = uri_for(&request.path) else {
            continue;
        };
        if let Some(client) = manager.servers.get(&document.key) {
            client.did_close(&uri);
        }
    }
}

fn lsp_open_documents(
    q: Query<(Entity, &FileView, &Editor), Without<LspOpened>>,
    starts: Query<&LspServerStartTask>,
    failures: Query<&LspServerFailed>,
    settings: Res<AppSettings>,
    mut manager: Single<&mut LspManager>,
    mut commands: Commands,
) {
    let overrides = server_overrides(&settings);
    let mut starting = starts
        .iter()
        .map(|start| start.key.clone())
        .collect::<HashSet<_>>();
    let failed = failures
        .iter()
        .map(|failure| failure.0.clone())
        .collect::<HashSet<_>>();
    for (entity, fv, _edit) in &q {
        if let Some(document) = manager.open_docs.get_mut(&fv.path) {
            document.refs += 1;
        } else if let Some(ext) = fv.path.extension().and_then(|extension| extension.to_str())
            && let Some(mut spec) = resolve_spec(ext, &overrides)
        {
            match store::PackageStore::lsp().resolve_command(&spec.command) {
                store::Resolution::Managed(path) => {
                    spec.command = path.to_string_lossy().into_owned();
                }
                store::Resolution::OnPath => {}
                store::Resolution::Missing => {
                    tracing::info!(server = %spec.command, "lsp server not installed/on PATH; skipping {ext}");
                    commands.entity(entity).insert(LspOpened);
                    continue;
                }
            }
            let directory = fv.path.parent().unwrap_or(&fv.path);
            let root = workspace_root(directory, &spec.root_markers);
            let key = ServerKey::new(&root, &spec.command);
            if failed.contains(&key) {
                commands.entity(entity).insert(LspOpened);
                continue;
            }
            if !manager.servers.contains_key(&key) {
                if starting.insert(key.clone()) {
                    let diagnostics = manager.diagnostics.clone();
                    let inputs = manager.inputs.clone();
                    let command = spec.command.clone();
                    let task = bevy::tasks::IoTaskPool::get().spawn(async move {
                        ServerClient::spawn(&spec, &root, diagnostics, inputs)
                    });
                    commands.spawn((
                        Name::new(format!("Starting LSP server {command}")),
                        LspServerStartTask { key, command, task },
                    ));
                }
                continue;
            }
            let (Some(uri), Some(text)) = (uri_for(&fv.path), read_text(&fv.path)) else {
                commands.entity(entity).insert(LspOpened);
                continue;
            };
            if let Some(client) = manager.servers.get(&key) {
                client.did_open(&uri, &spec.language_id, 1, &text);
                manager.open_docs.insert(
                    fv.path.clone(),
                    OpenDoc {
                        key,
                        version: 1,
                        refs: 1,
                    },
                );
            }
        }
        if let Some(request) = manager.folding_range(entity, &fv.path) {
            commands.spawn(request);
        }
        if let Some(request) = manager.semantic_tokens(entity, &fv.path) {
            commands.spawn(request);
        }
        if !crate::explorer_model::is_markdown(&fv.path)
            && let Some(request) = manager.document_symbol(entity, &fv.path)
        {
            commands.spawn(request);
        }
        commands.entity(entity).insert(LspOpened);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct LspResponseWriters<'w> {
    goto: MessageWriter<'w, LspGoto>,
    folds: MessageWriter<'w, LspFolds>,
    semantic: MessageWriter<'w, LspSemantic>,
    edit: MessageWriter<'w, LspRequestedEdit>,
}

fn drain_lsp_requests(
    manager: Single<&LspManager>,
    requests: Query<(Entity, &LspRequestOperation)>,
    browsers: NonSend<Browsers>,
    mut writers: LspResponseWriters,
    mut commands: Commands,
) {
    for (request_entity, request) in &requests {
        let value = match request.rx.try_recv() {
            Ok(v) => v,
            Err(crossbeam_channel::TryRecvError::Empty) => continue,
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                commands.entity(request_entity).despawn();
                continue;
            }
        };
        commands.entity(request_entity).despawn();
        let ready = browsers.can_emit_to(&request.target);
        match &request.kind {
            ReqKind::Hover { line, col } => {
                let blocks = parse_hover(&value);
                if !blocks.is_empty() && ready {
                    commands.trigger(FileUiStateWrite::from_event(
                        request.target,
                        &FileHover {
                            line: *line,
                            col: *col,
                            blocks,
                        },
                    ));
                }
            }
            ReqKind::Definition => {
                if let Some((path, line, utf16_col)) = parse_definition(&value) {
                    writers.goto.write(LspGoto {
                        entity: request.target,
                        path,
                        line,
                        utf16_col,
                    });
                }
            }
            ReqKind::Rename { root } => {
                let result = if value.is_null() {
                    Err("the language server would not rename this".to_string())
                } else {
                    serde_json::from_value::<lsp_types::WorkspaceEdit>(value)
                        .map_err(|e| format!("the rename could not be read: {e}"))
                };
                writers.edit.write(LspRequestedEdit {
                    entity: request.target,
                    root: root.clone(),
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
                commands
                    .entity(request.target)
                    .insert(OfferedCodeActions(offered));
                if !ready {
                    continue;
                }
                if titles.is_empty() {
                    commands.trigger(FileUiStateWrite::from_event(
                        request.target,
                        &FileEditFailure {
                            reason: "no code actions here".to_string(),
                        },
                    ));
                    continue;
                }
                commands.trigger(FileUiStateWrite::from_event(
                    request.target,
                    &FileCodeActions { titles },
                ));
            }
            ReqKind::Formatting { path, root } => {
                let result = match serde_json::from_value::<Vec<lsp_types::TextEdit>>(value) {
                    Ok(edits) if edits.is_empty() => continue,
                    Ok(edits) => one_document_edit(path, edits)
                        .ok_or_else(|| format!("{} has no URI to format", path.display())),
                    Err(_) => Err("the language server would not format this".to_string()),
                };
                writers.edit.write(LspRequestedEdit {
                    entity: request.target,
                    root: root.clone(),
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
                commands.trigger(crate::host::panel::ReferencesResult::new(
                    request.target,
                    items,
                ));
            }
            ReqKind::Completion {
                line,
                replace_from_col,
            } => {
                let items = parse_completion(&value);
                commands.trigger(crate::host::panel::CompletionResult::new(
                    request.target,
                    items,
                    *replace_from_col,
                    *line,
                ));
            }
            ReqKind::Folding { path } => {
                writers.folds.write(LspFolds {
                    entity: request.target,
                    path: path.clone(),
                    regions: parse_folding_ranges(&value),
                });
            }
            ReqKind::DocumentSymbol => {
                let items = crate::explorer_model::flatten_symbols(&value);
                if ready {
                    commands.trigger(FileUiStateWrite::from_event(
                        request.target,
                        &OutlineEvent { items },
                    ));
                }
            }
            ReqKind::SemanticTokens { key, path } => {
                writers.semantic.write(LspSemantic {
                    entity: request.target,
                    path: path.clone(),
                    tokens: parse_semantic_tokens(&value, manager.semantic_legend(key)),
                });
            }
        }
    }
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
    mut views: Query<(&mut Editor, &FileView)>,
    mut commands: Commands,
) {
    for message in reader.read() {
        let Ok((mut edit, view)) = views.get_mut(message.entity) else {
            continue;
        };
        if PathIdentity::resolve(&view.path) != PathIdentity::resolve(&message.path) {
            continue;
        }
        edit.hl
            .set_semantic(crate::lsp::semantic::SemanticHighlight::from(
                message.tokens.clone(),
            ));
        commands.trigger(ViewportRenderRequest::new(message.entity));
    }
}

pub fn build(
    app: &mut App,
    diagnostics: LspDiagnosticsSender,
    diagnostics_inbox: LspDiagnosticsInbox,
) {
    let (lint, lint_inbox) = crate::lsp::LintDiagnosticsSender::channel();
    let startup = std::sync::Mutex::new(Some((diagnostics, diagnostics_inbox, lint, lint_inbox)));
    app.add_systems(
        Startup,
        move |inputs: Single<&ServerInputSender>, mut commands: Commands| {
            let (diagnostics, diagnostics_inbox, lint, lint_inbox) = startup
                .lock()
                .unwrap()
                .take()
                .expect("LSP runtime can only start once");
            commands.spawn((
                Name::new("LSP manager"),
                LspManager::new(diagnostics, inputs.clone()),
            ));
            commands.spawn((Name::new("LSP diagnostics"), diagnostics_inbox));
            commands.spawn((Name::new("Lint diagnostics"), lint, lint_inbox));
        },
    )
    .add_message::<LspGoto>()
    .add_message::<LspFolds>()
    .add_message::<LspSemantic>()
    .add_message::<LspRequestedEdit>()
    .add_message::<LspCodeActionRequest>()
    .add_message::<LspDocumentChangeRequest>()
    .add_message::<LspDocumentCloseRequest>()
    .add_observer(on_file_code_action_pick)
    .add_systems(
        Update,
        (
            finish_lsp_server_starts,
            close_lsp_documents,
            change_lsp_documents,
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

fn on_file_code_action_pick(
    trigger: On<UiInput<FileCodeActionPick>>,
    views: Query<(&Editor, &OfferedCodeActions)>,
    manager: Single<&LspManager>,
    mut edits: MessageWriter<LspRequestedEdit>,
) {
    let entity = trigger.event().webview;
    let Ok((editor, actions)) = views.get(entity) else {
        return;
    };
    let Some(action) = actions
        .0
        .get(trigger.event().payload.index as usize)
        .cloned()
    else {
        return;
    };
    let path = &editor.core.buffer.path;
    let Some(document) = manager.open_docs.get(path) else {
        return;
    };
    let Some(client) = manager.servers.get(&document.key) else {
        return;
    };
    let root = document.key.root().to_path_buf();
    let (command, edit) = match action {
        lsp_types::CodeActionOrCommand::Command(command) => (Some(command), None),
        lsp_types::CodeActionOrCommand::CodeAction(action) => (action.command, action.edit),
    };
    if let Some(command) = command {
        let (_, _response) = client.send_request(
            "workspace/executeCommand",
            serde_json::json!({
                "command": command.command,
                "arguments": command.arguments.unwrap_or_default(),
            }),
        );
    }
    if let Some(edit) = edit {
        edits.write(LspRequestedEdit {
            entity,
            root,
            result: Ok(edit),
        });
    }
}

#[derive(Component, Default)]
struct LspDiagnostics {
    mapped: Vec<FileDiagnostic>,
    raw: Vec<lsp_types::Diagnostic>,
}

#[derive(Component, Default)]
struct LintDiagnostics(Vec<FileDiagnostic>);

#[derive(Component, Default)]
pub(crate) struct OfferedCodeActions(pub(crate) Vec<lsp_types::CodeActionOrCommand>);

#[derive(Component, Default)]
pub struct DiagSent(Vec<FileDiagnostic>);

fn emit_diagnostics_system(
    q: Query<(Entity, &FileView, Option<&DiagSent>), With<PageReady>>,
    lsp_diagnostics: Query<&LspDiagnostics>,
    lint_diagnostics: Query<&LintDiagnostics>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, sent) in &q {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let mut merged: Vec<FileDiagnostic> = Vec::new();
        if let Ok(diagnostics) = lsp_diagnostics.get(entity) {
            merged.extend(diagnostics.mapped.iter().cloned());
        }
        if let Ok(diagnostics) = lint_diagnostics.get(entity) {
            merged.extend(diagnostics.0.iter().cloned());
        }
        match sent {
            Some(s) if s.0 == merged => continue,
            None if merged.is_empty() => continue,
            _ => {}
        }
        commands.trigger(FileUiStateWrite::from_event(
            entity,
            &FileDiagnostics {
                path: fv.path.to_string_lossy().into_owned(),
                diagnostics: merged.clone(),
            },
        ));
        commands.entity(entity).insert(DiagSent(merged));
    }
}

fn drain_lsp_diagnostics(
    inbox: Single<&LspDiagnosticsInbox>,
    views: Query<(Entity, &FileView, &Editor)>,
    mut commands: Commands,
) {
    for (path, diags) in inbox.drain() {
        let target = PathIdentity::resolve(&path);
        for (entity, view, edit) in &views {
            if PathIdentity::resolve(&view.path) != target {
                continue;
            }
            let mapped = map_diags(&diags, |line| rope_line_text(&edit.core.buffer.rope, line));
            commands.entity(entity).insert(LspDiagnostics {
                mapped,
                raw: diags.clone(),
            });
        }
    }
}

fn drain_lint(
    inbox: Single<&LintDiagnosticsInbox>,
    views: Query<(Entity, &FileView)>,
    mut commands: Commands,
) {
    for (path, diags) in inbox.drain() {
        let target = PathIdentity::resolve(&path);
        for (entity, view) in &views {
            if PathIdentity::resolve(&view.path) == target {
                commands
                    .entity(entity)
                    .insert(LintDiagnostics(diags.clone()));
            }
        }
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
    diagnostics: Query<&LspDiagnostics>,
    mut manager: Single<&mut LspManager>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let diagnostics = diagnostics
            .get(request.entity)
            .map(|diagnostics| diagnostics.raw.as_slice())
            .unwrap_or_default();
        let request = manager.code_actions(
            request.entity,
            &request.path,
            request.from_line,
            request.to_line,
            diagnostics,
        );
        if let Some(request) = request {
            commands.spawn(request);
        }
    }
}

#[derive(Component)]
pub struct LintRan;

fn lint_on_open(
    q: Query<(Entity, &FileView, &Editor), Without<LintRan>>,
    outbox: Single<&LintDiagnosticsSender>,
    mut commands: Commands,
) {
    let store = store::PackageStore::lsp();
    for (entity, fv, _edit) in &q {
        commands.entity(entity).insert(LintRan);
        let Some(ext) = fv.path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(spec) = crate::lsp::registry::linter_for(ext) else {
            continue;
        };
        if matches!(
            store.resolve_command(&spec.command),
            store::Resolution::Missing
        ) {
            continue;
        }
        let path = fv.path.clone();
        let sink = LintDiagnosticsSender::clone(&outbox);
        std::thread::spawn(move || {
            let diags = spec.run(&path);
            sink.send((path, diags));
        });
    }
}

#[derive(Component)]
pub struct LspStatusSent {
    state: LspServerState,
    path: PathBuf,
}

fn lsp_status_system(
    q: Query<(Entity, &FileView, Option<&LspStatusSent>), With<PageReady>>,
    settings: Res<AppSettings>,
    manager: Single<&LspManager>,
    browsers: NonSend<Browsers>,
    mut installs: MessageWriter<crate::lsp::manager_page::PackageInstallRequest>,
    mut commands: Commands,
) {
    let overrides = server_overrides(&settings);
    let store = store::PackageStore::lsp();
    for (entity, fv, sent) in &q {
        let Some(ext) = fv.path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(spec) = resolve_spec(ext, &overrides) else {
            continue;
        };
        let desired = match store.resolve_command(&spec.command) {
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
        let package = (!overrides.contains_key(ext))
            .then(|| crate::lsp::registry::preferred_package(ext))
            .flatten()
            .map(str::to_string);
        commands.trigger(FileUiStateWrite::from_event(
            entity,
            &FileLspStatus {
                path: fv.path.to_string_lossy().into_owned(),
                server: spec.command.clone(),
                package: package.clone(),
                state: desired,
                capabilities: manager.menu_capabilities(&fv.path),
            },
        ));
        if desired == LspServerState::Missing
            && let Some(name) = package
        {
            installs.write(crate::lsp::manager_page::PackageInstallRequest {
                target: entity,
                name,
            });
        }
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
        use crate::lsp::{LspDiagnosticsInbox, LspDiagnosticsSender};
        use std::path::PathBuf;

        let mut app = App::new();
        let (outbox, inbox) = LspDiagnosticsSender::channel();
        let probe = inbox.0.clone();
        app.add_plugins(MinimalPlugins);
        app.world_mut().spawn(inbox);
        outbox.send((PathBuf::from("/x.rs"), vec![]));
        app.add_systems(Update, |inbox: Single<&LspDiagnosticsInbox>| {
            drop(inbox.drain());
        });
        app.update();
        assert!(probe.is_empty());
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
    fn diagnostics_map_through_editor() {
        use crate::edit::highlight_cache::HighlightCache;
        use crate::edit::{EditCore, EditMode};
        use crate::host::editor::{Editor, FileView};
        use crate::lsp::LspDiagnosticsSender;
        use std::path::PathBuf;

        let path = PathBuf::from("/tmp/vmux_lsp_editor.rs");
        let mut app = App::new();
        let (outbox, inbox) = LspDiagnosticsSender::channel();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, drain_lsp_diagnostics);
        app.world_mut().spawn(inbox);

        let core = EditCore::new(
            path.clone(),
            "Rust".into(),
            "fn a() {}\nlet x = 1;\n",
            EditMode::Insert,
        );
        let hl = HighlightCache::new(&path);
        let entity = app
            .world_mut()
            .spawn((
                FileView { path: path.clone() },
                Editor::new(core, hl, crate::fold::FoldState::default()),
            ))
            .id();

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
        outbox.send((path.clone(), vec![diag]));
        app.update();

        let mapped = &app
            .world()
            .get::<LspDiagnostics>(entity)
            .expect("diagnostics mapped for Editor entity")
            .mapped;
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].line, 1);
        assert_eq!(mapped[0].start_col, 4);
    }
}
