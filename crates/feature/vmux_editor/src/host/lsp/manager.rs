use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, UiInput};
#[cfg(test)]
use vmux_ecs::event::FileLine;
use vmux_ecs::event::{
    CompletionItem, DiagSeverity, EditorCapability, FileCodeActionDismissRequest,
    FileCodeActionMoveRequest, FileCodeActionPick, FileCodeActions, FileDiagnostic,
    FileDiagnostics, FileHover, FileHoverState, FileLspStatus, HoverBlock, LspServerState,
    OutlineEvent, RefItem,
};
use vmux_ecs::page::PageReady;
use vmux_ecs::{
    FileUiStateWrite, Job, JobPlugin, JobRequest, JobResult, JobSet, JobWake, Operation,
    OperationCompleted, OperationPlugin, OperationState, OperationTarget, OperationTargetRevision,
};
use vmux_path::PathIdentity;
use vmux_setting::AppSettings;

use crate::host::editor::{Editor, FileView};
use crate::host::highlight::{HIGHLIGHT_MAX_BYTES, Highlighter};
use crate::host::markdown::ParsedNote;
use crate::host::viewport::ViewportRenderRequest;
use crate::lsp::client::ServerClient;
use crate::lsp::registry::{LspRegistry, ServerSpec};
use crate::lsp::server_request::ServerInputSender;
use crate::lsp::{
    LintDiagnosticsInbox, LintDiagnosticsSender, LspDiagnosticsInbox, LspDiagnosticsSender,
    OpenDoc, PathDiagnostics, ServerKey, store,
};

use lsp_types::GotoDefinitionResponse::*;
use lsp_types::{HoverContents, MarkedString};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LspLine(String);

impl LspLine {
    pub(crate) fn new(text: String) -> Self {
        Self(text)
    }

    #[cfg(test)]
    fn from_file_line(line: &FileLine) -> Self {
        Self(line.spans.iter().map(|span| span.text.as_str()).collect())
    }

    fn from_rope(rope: &ropey::Rope, line: u32) -> Self {
        let line = line as usize;
        if line >= rope.len_lines() {
            return Self::default();
        }
        Self(
            rope.line(line)
                .chars()
                .filter(|character| *character != '\n' && *character != '\r')
                .collect(),
        )
    }

    pub(crate) fn read(path: &Path, line: u32) -> Self {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        Self(
            content
                .lines()
                .nth(line as usize)
                .unwrap_or_default()
                .to_string(),
        )
    }

    pub(crate) fn char_col(&self, utf16_col: u32) -> u32 {
        let mut utf16 = 0u32;
        let mut chars = 0u32;
        for character in self.0.chars() {
            if utf16 >= utf16_col {
                return chars;
            }
            utf16 += character.len_utf16() as u32;
            chars += 1;
        }
        chars
    }

    pub(crate) fn utf16_col(&self, char_col: u32) -> u32 {
        self.0
            .chars()
            .take(char_col as usize)
            .map(|character| character.len_utf16() as u32)
            .sum()
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_string(self) -> String {
        self.0
    }
}

impl LspDiagnostics {
    #[cfg(test)]
    fn from_lines(
        lines: &[FileLine],
        diagnostics: &[lsp_types::Diagnostic],
    ) -> Vec<FileDiagnostic> {
        Self::map(diagnostics, |line| {
            lines
                .get(line as usize)
                .map(LspLine::from_file_line)
                .unwrap_or_default()
        })
    }

    fn map(
        diagnostics: &[lsp_types::Diagnostic],
        line_text: impl Fn(u32) -> LspLine,
    ) -> Vec<FileDiagnostic> {
        diagnostics
            .iter()
            .map(|diagnostic| {
                let line = diagnostic.range.start.line;
                let text = line_text(line);
                let start_col = text.char_col(diagnostic.range.start.character);
                let end_col = if diagnostic.range.end.line == line {
                    text.char_col(diagnostic.range.end.character).max(start_col)
                } else {
                    text.as_str().chars().count() as u32
                };
                FileDiagnostic {
                    line,
                    start_col,
                    end_col,
                    severity: Self::severity(diagnostic.severity),
                    message: diagnostic.message.clone(),
                    source: diagnostic.source.clone(),
                }
            })
            .collect()
    }

    fn severity(severity: Option<lsp_types::DiagnosticSeverity>) -> DiagSeverity {
        match severity {
            Some(value) if value == lsp_types::DiagnosticSeverity::ERROR => DiagSeverity::Error,
            Some(value) if value == lsp_types::DiagnosticSeverity::WARNING => DiagSeverity::Warning,
            Some(value) if value == lsp_types::DiagnosticSeverity::HINT => DiagSeverity::Hint,
            _ => DiagSeverity::Info,
        }
    }
}

type ServerOverrides = std::collections::BTreeMap<String, ServerSpec>;

const LSP_MAX_BYTES: u64 = HIGHLIGHT_MAX_BYTES;

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
    stamp: RequestStamp,
    wire_id: i64,
}

#[derive(Component)]
struct RequestDeadline(web_time::Instant);

impl LspRequestOperation {
    pub(crate) fn bundle(self) -> impl Bundle {
        (
            Operation::bundle("lsp.request").unwrap(),
            OperationTarget::new(self.stamp.document),
            OperationTargetRevision(self.stamp.version as u64),
            RequestDeadline(web_time::Instant::now()),
            self,
        )
    }
}

struct RequestStamp {
    document: Entity,
    server: Entity,
    version: i32,
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

impl LspFolds {
    fn regions(value: &serde_json::Value) -> Vec<crate::fold::FoldRegion> {
        value
            .as_array()
            .map(|ranges| {
                ranges
                    .iter()
                    .filter_map(|range| {
                        let start = range.get("startLine")?.as_u64()? as u32;
                        let end = range.get("endLine")?.as_u64()? as u32;
                        (end > start).then_some(crate::fold::FoldRegion { start, end })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[derive(Message)]
pub struct LspRequestedEdit {
    pub entity: Entity,
    pub root: PathBuf,
    pub result: Result<lsp_types::WorkspaceEdit, String>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct LspManager<'w, 's> {
    servers: Query<'w, 's, (Entity, &'static ServerKey), With<Server>>,
    clients: Query<'w, 's, &'static ServerClient>,
    documents: Query<'w, 's, (Entity, &'static DocumentPath, &'static mut OpenDoc)>,
    context: Query<'w, 's, &'static LspContext>,
    bindings: Query<'w, 's, &'static DocumentBinding>,
    wake: Option<Res<'w, JobWake>>,
    roots: Query<'w, 's, (Entity, &'static vmux_ecs::Cwd), With<WorkspaceRoot>>,
}

#[derive(Component)]
struct LspContext {
    diagnostics: LspDiagnosticsSender,
    inputs: ServerInputSender,
}

#[derive(Component, Clone, PartialEq, Eq)]
struct DocumentPath(PathBuf);

#[derive(Component)]
struct WorkspaceRoot;

#[derive(Component)]
struct Server;

#[derive(Component, Clone, Copy)]
struct DocumentBinding(Entity);

#[derive(Component, Clone, Copy)]
struct ServerBinding(Entity);

#[derive(Component)]
struct LspServerStartTask {
    key: ServerKey,
    command: String,
    server: Entity,
}

#[derive(Clone)]
struct StartJob {
    spec: ServerSpec,
    root: PathBuf,
    diagnostics: LspDiagnosticsSender,
    inputs: ServerInputSender,
    wake: Option<std::task::Waker>,
}

impl Job for StartJob {
    type Output = Result<ServerClient, String>;

    fn run(self) -> Self::Output {
        ServerClient::spawn_with_wake(
            &self.spec,
            &self.root,
            self.diagnostics,
            self.inputs,
            self.wake,
        )
        .map_err(|error| error.to_string())
    }

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }
}

#[derive(Component)]
struct LspServerFailed(ServerKey);

#[derive(Message)]
pub(crate) struct LspDocumentChangeRequest {
    pub(crate) path: PathBuf,
    pub(crate) owner: Entity,
    pub(crate) text: Option<String>,
}

#[derive(Message)]
pub(crate) struct LspDocumentCloseRequest {
    pub(crate) path: PathBuf,
    pub(crate) owner: Entity,
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
    let mut changes = HashMap::new();
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

impl LspManager<'_, '_> {
    fn document(&self, path: &Path) -> Option<&OpenDoc> {
        let mut found = None;
        for (_, document_path, document) in &self.documents {
            if document_path.0 != path {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(document);
        }
        found
    }

    fn document_for(&self, owner: Entity, path: &Path) -> Option<&OpenDoc> {
        let binding = self.bindings.get(owner).ok()?;
        let (_, document_path, document) = self.documents.get(binding.0).ok()?;
        if document_path.0 != path {
            return None;
        }
        Some(document)
    }

    fn document_for_server(&self, key: &ServerKey, path: &Path) -> Option<Entity> {
        let mut found = None;
        for (entity, document_path, doc) in &self.documents {
            if document_path.0 != path || doc.key != *key {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(entity);
        }
        found
    }

    fn server_entity(&self, key: &ServerKey) -> Option<Entity> {
        let mut found = None;
        for (entity, server_key) in &self.servers {
            if server_key != key {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(entity);
        }
        found
    }

    pub(crate) fn server_current(&self, server: Entity) -> bool {
        self.servers.contains(server) && self.clients.contains(server)
    }

    fn client(&self, key: &ServerKey) -> Option<&ServerClient> {
        let entity = self.server_entity(key)?;
        self.clients.get(entity).ok()
    }

    fn request_stamp(&self, owner: Entity, path: &Path) -> Option<RequestStamp> {
        let document = self.bindings.get(owner).ok()?.0;
        let doc = self.document_for(owner, path)?;
        Some(RequestStamp {
            document,
            server: self.server_entity(&doc.key)?,
            version: doc.version,
        })
    }

    pub(crate) fn document_version(&self, path: &Path) -> Option<i32> {
        self.document(path).map(|doc| doc.version)
    }

    fn menu_capabilities(&self, owner: Entity, path: &Path) -> Vec<EditorCapability> {
        let Some(doc) = self.document_for(owner, path) else {
            return Vec::new();
        };
        let Some(client) = self.client(&doc.key) else {
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
        &self,
        entity: Entity,
        path: &Path,
        method: &str,
        line: u32,
        utf16_col: u32,
        extra: serde_json::Value,
        kind: ReqKind,
    ) -> Option<LspRequestOperation> {
        let doc = self.document_for(entity, path)?;
        let uri = uri_for(path)?;
        let client = self.client(&doc.key)?;
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
        let (wire_id, rx) = client.send_request(method, params);
        Some(LspRequestOperation {
            target: entity,
            kind,
            rx,
            stamp: self.request_stamp(entity, path)?,
            wire_id,
        })
    }

    pub(crate) fn hover(
        &self,
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
        &self,
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
        &self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
    ) -> Option<LspRequestOperation> {
        self.goto(entity, path, "textDocument/declaration", line, utf16_col)
    }

    pub(crate) fn type_definition(
        &self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
    ) -> Option<LspRequestOperation> {
        self.goto(entity, path, "textDocument/typeDefinition", line, utf16_col)
    }

    pub(crate) fn implementation(
        &self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
    ) -> Option<LspRequestOperation> {
        self.goto(entity, path, "textDocument/implementation", line, utf16_col)
    }

    fn goto(
        &self,
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
        &self,
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
        &self,
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
        &self,
        entity: Entity,
        path: &Path,
        method: &str,
        params_extra: serde_json::Value,
        kind: ReqKind,
    ) -> Option<LspRequestOperation> {
        let doc = self.document_for(entity, path)?;
        let uri = uri_for(path)?;
        let client = self.client(&doc.key)?;
        if !client.provides(method) {
            return None;
        }
        let mut params = serde_json::json!({ "textDocument": { "uri": uri } });
        if let (Some(obj), Some(ex)) = (params.as_object_mut(), params_extra.as_object()) {
            for (k, v) in ex {
                obj.insert(k.clone(), v.clone());
            }
        }
        let (wire_id, rx) = client.send_request(method, params);
        Some(LspRequestOperation {
            target: entity,
            kind,
            rx,
            stamp: self.request_stamp(entity, path)?,
            wire_id,
        })
    }

    pub(crate) fn format_document(
        &self,
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
        &self,
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
        &self,
        entity: Entity,
        path: &Path,
        method: &str,
        extra: serde_json::Value,
    ) -> Option<LspRequestOperation> {
        let doc = self.document_for(entity, path)?;
        let uri = uri_for(path)?;
        let client = self.client(&doc.key)?;
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
        let (wire_id, rx) = client.send_request(method, params);
        Some(LspRequestOperation {
            target: entity,
            kind: ReqKind::Formatting {
                path: path.to_path_buf(),
                root,
            },
            rx,
            stamp: self.request_stamp(entity, path)?,
            wire_id,
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
        &self,
        entity: Entity,
        path: &Path,
        line: u32,
        utf16_col: u32,
        new_name: &str,
    ) -> Option<LspRequestOperation> {
        let root = self
            .document_for(entity, path)
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
        &self,
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

    pub(crate) fn folding_range(&self, entity: Entity, path: &Path) -> Option<LspRequestOperation> {
        let doc = self.document_for(entity, path)?;
        let uri = uri_for(path)?;
        let client = self.client(&doc.key)?;
        if !client.provides("textDocument/foldingRange") {
            return None;
        }
        let params = serde_json::json!({ "textDocument": { "uri": uri } });
        let (wire_id, rx) = client.send_request("textDocument/foldingRange", params);
        Some(LspRequestOperation {
            target: entity,
            kind: ReqKind::Folding {
                path: path.to_path_buf(),
            },
            rx,
            stamp: self.request_stamp(entity, path)?,
            wire_id,
        })
    }

    pub(crate) fn document_symbol(
        &self,
        entity: Entity,
        path: &Path,
    ) -> Option<LspRequestOperation> {
        let doc = self.document_for(entity, path)?;
        let uri = uri_for(path)?;
        let client = self.client(&doc.key)?;
        if !client.provides("textDocument/documentSymbol") {
            return None;
        }
        let params = serde_json::json!({ "textDocument": { "uri": uri } });
        let (wire_id, rx) = client.send_request("textDocument/documentSymbol", params);
        Some(LspRequestOperation {
            target: entity,
            kind: ReqKind::DocumentSymbol,
            rx,
            stamp: self.request_stamp(entity, path)?,
            wire_id,
        })
    }

    pub(crate) fn semantic_tokens(
        &self,
        entity: Entity,
        path: &Path,
    ) -> Option<LspRequestOperation> {
        let doc = self.document_for(entity, path)?;
        let uri = uri_for(path)?;
        let key = doc.key.clone();
        let client = self.client(&key)?;
        if !client.provides("textDocument/semanticTokens/full") {
            return None;
        }
        let params = serde_json::json!({ "textDocument": { "uri": uri } });
        let (wire_id, rx) = client.send_request("textDocument/semanticTokens/full", params);
        Some(LspRequestOperation {
            target: entity,
            kind: ReqKind::SemanticTokens {
                key,
                path: path.to_path_buf(),
            },
            rx,
            stamp: self.request_stamp(entity, path)?,
            wire_id,
        })
    }

    pub fn semantic_legend(
        &self,
        key: &ServerKey,
    ) -> Option<&crate::lsp::semantic::SemanticLegend> {
        self.client(key)?.semantic_legend()
    }
}

fn hover_contents_to_string(c: HoverContents) -> String {
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
                    lines: Highlighter::snippet(&buf, lang.trim()),
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
            lines: Highlighter::snippet(&buf, lang.trim()),
        });
    } else {
        flush_prose(&mut buf, &mut blocks);
    }
    blocks
}

fn loc_tuple(uri: &lsp_types::Uri, pos: lsp_types::Position) -> Option<(PathBuf, u32, u32)> {
    let path = ServerClient::path_from_uri(uri.as_str())?;
    Some((path, pos.line, pos.character))
}

fn parse_definition(value: &serde_json::Value) -> Option<(PathBuf, u32, u32)> {
    let result = value.get("result")?;
    if result.is_null() {
        return None;
    }

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

fn ref_display(path: &Path, line: u32) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    format!("{}:{}", name, line + 1)
}

#[derive(Component)]
pub struct LspOpened;

#[derive(Component)]
struct LspInitialized;

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
                    extensions: vec![ext.clone()],
                    command: o.command.clone(),
                    args: o.args.clone(),
                    language_id: o.language_id.clone(),
                    root_markers: o.root_markers.clone(),
                    package: None,
                },
            )
        })
        .collect()
}

struct ServersPlugin;

impl Plugin for ServersPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<StartJob>::default())
            .add_message::<LspRestartRequest>()
            .add_systems(
                Update,
                (
                    reset_workspaces,
                    restart_servers,
                    finish_server_starts,
                    poll_server_exits,
                )
                    .chain()
                    .in_set(ServerSet)
                    .after(JobSet::Apply)
                    .before(DocumentSet),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ServerSet;

#[derive(Message)]
pub struct LspRestartRequest {
    pub server: Entity,
}

fn restart_servers(
    mut requests: MessageReader<LspRestartRequest>,
    servers: Query<(), With<Server>>,
    views: Query<(Entity, &ServerBinding)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !servers.contains(request.server) {
            continue;
        }
        for (entity, binding) in &views {
            if binding.0 == request.server {
                commands.entity(entity).remove::<(
                    LspOpened,
                    ServerBinding,
                    DocumentBinding,
                    LspInitialized,
                    LspDiagnostics,
                )>();
            }
        }
        commands.entity(request.server).despawn();
    }
}

type WorkspaceChanges = (With<WorkspaceRoot>, Changed<vmux_ecs::Cwd>);

fn reset_workspaces(
    roots: Query<(Entity, Ref<vmux_ecs::Cwd>), WorkspaceChanges>,
    servers: Query<(Entity, &ChildOf), With<Server>>,
    mut commands: Commands,
) {
    for (root, path) in &roots {
        if path.is_added() {
            continue;
        }
        for (entity, parent) in &servers {
            if parent.parent() == root {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn poll_server_exits(
    mut clients: Query<(Entity, &ServerKey, &mut ServerClient)>,
    documents: Query<(Entity, &ChildOf), With<OpenDoc>>,
    mut commands: Commands,
) {
    for (entity, key, mut client) in &mut clients {
        if !client.exited() {
            continue;
        }
        for (document, parent) in &documents {
            if parent.parent() == entity {
                commands.entity(document).despawn();
            }
        }
        commands
            .entity(entity)
            .insert(LspServerFailed(key.clone()))
            .remove::<ServerClient>();
    }
}

fn finish_server_starts(
    mut starts: Query<(Entity, &LspServerStartTask, &mut JobResult<StartJob>)>,
    servers: Query<&ServerKey, With<Server>>,
    mut commands: Commands,
) {
    for (entity, start, mut result) in &mut starts {
        let value = std::mem::replace(&mut result.0, Err("startup result consumed".into()));
        if !servers.get(start.server).is_ok_and(|key| *key == start.key) {
            commands.entity(entity).insert(OperationState::Canceled);
            commands
                .entity(entity)
                .remove::<(LspServerStartTask, JobResult<StartJob>)>();
            continue;
        }
        let value = match value {
            Ok(value) => value,
            Err(error) => Err(error),
        };
        match value {
            Ok(client) => {
                commands
                    .entity(start.server)
                    .insert((Name::new(start.command.clone()), client));
                commands.trigger(OperationCompleted {
                    entity,
                    result: Ok(()),
                });
            }
            Err(error) => {
                commands.entity(start.server).insert((
                    Name::new(format!("Failed LSP server {}", start.command)),
                    LspServerFailed(start.key.clone()),
                ));
                commands.trigger(OperationCompleted {
                    entity,
                    result: Err(error),
                });
            }
        }
        commands
            .entity(entity)
            .remove::<(LspServerStartTask, JobResult<StartJob>)>();
    }
}

fn change_documents(
    mut requests: MessageReader<LspDocumentChangeRequest>,
    mut manager: LspManager,
    views: Query<(Entity, &DocumentBinding)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(binding) = manager.bindings.get(request.owner) else {
            continue;
        };
        let entity = binding.0;
        if !manager
            .documents
            .get(entity)
            .is_ok_and(|(_, path, _)| path.0 == request.path)
        {
            continue;
        }
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
        let (key, version) = {
            let Ok((_, _, mut document)) = manager.documents.get_mut(entity) else {
                continue;
            };
            let Some(version) = document.version.checked_add(1) else {
                continue;
            };
            document.version = version;
            (document.key.clone(), version)
        };
        if let Some(client) = manager.client(&key) {
            client.did_change(&uri, version, &text);
        }
        for (view, binding) in &views {
            if binding.0 == entity {
                commands.entity(view).remove::<LspInitialized>();
            }
        }
    }
}

fn close_documents(
    mut requests: MessageReader<LspDocumentCloseRequest>,
    bindings: Query<&DocumentBinding>,
    documents: Query<&DocumentPath>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(binding) = bindings.get(request.owner) else {
            continue;
        };
        if !documents
            .get(binding.0)
            .is_ok_and(|path| path.0 == request.path)
        {
            continue;
        }
        commands
            .entity(request.owner)
            .remove::<(DocumentBinding, LspOpened, LspInitialized, LspDiagnostics)>();
    }
}

fn refresh_bindings(
    views: Query<(
        Entity,
        &FileView,
        Option<&DocumentBinding>,
        Option<&ServerBinding>,
    )>,
    documents: Query<&DocumentPath>,
    servers: Query<(), With<Server>>,
    mut commands: Commands,
) {
    for (entity, view, document, server) in &views {
        let stale_document = document.is_some_and(|binding| {
            !documents
                .get(binding.0)
                .is_ok_and(|path| path.0 == view.path)
        });
        let stale_server = server.is_some_and(|binding| !servers.contains(binding.0));
        if stale_document || stale_server {
            commands.entity(entity).remove::<(
                DocumentBinding,
                ServerBinding,
                LspOpened,
                LspInitialized,
                LspDiagnostics,
            )>();
        }
    }
}

fn cleanup_documents(manager: LspManager, views: Query<&DocumentBinding>, mut commands: Commands) {
    for (entity, path, document) in &manager.documents {
        if views.iter().any(|binding| binding.0 == entity) {
            continue;
        }
        if let Some(uri) = uri_for(&path.0)
            && let Some(client) = manager.client(&document.key)
        {
            client.did_close(&uri);
        }
        commands.entity(entity).despawn();
    }
}

fn open_documents(
    q: Query<(Entity, &FileView, &Editor), Without<LspOpened>>,
    starts: Query<&LspServerStartTask>,
    failures: Query<&LspServerFailed>,
    registry: Single<&LspRegistry>,
    settings: Res<AppSettings>,
    manager: LspManager,
    mut commands: Commands,
) {
    let overrides = server_overrides(&settings);
    let mut starting = HashSet::new();
    for start in &starts {
        starting.insert(start.key.clone());
    }
    let mut failed = HashSet::new();
    for failure in &failures {
        failed.insert(failure.0.clone());
    }
    let mut pending_roots = Vec::<(PathBuf, Entity)>::new();
    let mut pending_documents = Vec::<(ServerKey, PathBuf, Entity)>::new();
    for (entity, view, _) in &q {
        let Some(extension) = view
            .path
            .extension()
            .and_then(|extension| extension.to_str())
        else {
            commands.entity(entity).insert(LspOpened);
            continue;
        };
        let Some(mut spec) = registry.server(extension, &overrides) else {
            commands.entity(entity).insert(LspOpened);
            continue;
        };
        match store::PackageStore::lsp().resolve_command(&spec.command) {
            store::Resolution::Managed(path) => spec.command = path.to_string_lossy().into_owned(),
            store::Resolution::OnPath => {}
            store::Resolution::Missing => {
                commands.entity(entity).insert(LspOpened);
                continue;
            }
        }
        let root = spec.workspace_root(view.path.parent().unwrap_or(&view.path));
        let key = ServerKey::new(&root, &spec.command).with_arguments(spec.args.clone());
        if let Some(server) = manager.server_entity(&key) {
            commands.entity(entity).insert(ServerBinding(server));
        }
        if failed.contains(&key) {
            commands.entity(entity).insert(LspOpened);
            continue;
        }
        let Some(client) = manager.client(&key) else {
            if starting.insert(key.clone()) {
                let Ok(context) = manager.context.single() else {
                    continue;
                };
                let mut owner = None;
                for (candidate, directory) in &manager.roots {
                    if directory.0 == root {
                        owner = Some(candidate);
                        break;
                    }
                }
                for (directory, candidate) in &pending_roots {
                    if *directory == root {
                        owner = Some(*candidate);
                        break;
                    }
                }
                let owner = match owner {
                    Some(owner) => owner,
                    None => {
                        let owner = commands
                            .spawn((
                                WorkspaceRoot,
                                vmux_ecs::Cwd(root.clone()),
                                Name::new(root.to_string_lossy().into_owned()),
                            ))
                            .id();
                        pending_roots.push((root.clone(), owner));
                        owner
                    }
                };
                let server = commands
                    .spawn((
                        Server,
                        key.clone(),
                        ChildOf(owner),
                        Name::new(format!("Starting LSP server {}", spec.command)),
                    ))
                    .id();
                commands.entity(entity).insert(ServerBinding(server));
                let diagnostics = context.diagnostics.scoped(server);
                let inputs = context.inputs.scoped(server);
                let command = spec.command.clone();
                commands.spawn((
                    Operation::bundle("lsp.start").unwrap(),
                    OperationTarget::new(server),
                    ChildOf(server),
                    LspServerStartTask {
                        key,
                        command,
                        server,
                    },
                    JobRequest(StartJob {
                        spec,
                        root,
                        diagnostics,
                        inputs,
                        wake: manager.wake.as_ref().map(|wake| wake.0.clone()),
                    }),
                ));
                if let Some(wake) = &manager.wake {
                    wake.0.wake_by_ref();
                }
            }
            continue;
        };
        let mut document = manager.document_for_server(&key, &view.path);
        for (pending_key, path, candidate) in &pending_documents {
            if *pending_key == key && *path == view.path {
                document = Some(*candidate);
                break;
            }
        }
        let document = match document {
            Some(document) => document,
            None => {
                let (Some(uri), Some(text)) = (uri_for(&view.path), read_text(&view.path)) else {
                    commands.entity(entity).insert(LspOpened);
                    continue;
                };
                client.did_open(&uri, &spec.language_id, 1, &text);
                let server = manager.server_entity(&key).unwrap();
                let document = commands
                    .spawn((
                        DocumentPath(view.path.clone()),
                        OpenDoc {
                            key: key.clone(),
                            version: 1,
                        },
                        ChildOf(server),
                        Name::new(view.path.to_string_lossy().into_owned()),
                    ))
                    .id();
                pending_documents.push((key.clone(), view.path.clone(), document));
                document
            }
        };
        commands.entity(entity).insert((
            DocumentBinding(document),
            ServerBinding(manager.server_entity(&key).unwrap()),
            LspOpened,
        ));
    }
}

type UninitializedViews = (With<DocumentBinding>, Without<LspInitialized>);

fn initialize_views(
    views: Query<(Entity, &FileView), UninitializedViews>,
    manager: LspManager,
    mut commands: Commands,
) {
    for (entity, fv) in &views {
        if let Some(request) = manager.folding_range(entity, &fv.path) {
            commands.spawn(request.bundle());
        }
        if let Some(request) = manager.semantic_tokens(entity, &fv.path) {
            commands.spawn(request.bundle());
        }
        if !ParsedNote::supports(&fv.path)
            && let Some(request) = manager.document_symbol(entity, &fv.path)
        {
            commands.spawn(request.bundle());
        }
        commands.entity(entity).insert(LspInitialized);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct LspResponseWriters<'w> {
    goto: MessageWriter<'w, LspGoto>,
    folds: MessageWriter<'w, LspFolds>,
    semantic: MessageWriter<'w, LspSemantic>,
    edit: MessageWriter<'w, LspRequestedEdit>,
}

struct RequestsPlugin;

impl Plugin for RequestsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.add_message::<LspGoto>()
            .add_message::<LspCodeActionRequest>()
            .add_message::<LspFolds>()
            .add_message::<LspSemantic>()
            .add_message::<LspRequestedEdit>()
            .add_systems(
                Update,
                (
                    request_code_actions,
                    cancel_requests,
                    drain_requests,
                    apply_semantic_tokens,
                )
                    .chain()
                    .after(DiagnosticsSet)
                    .after(DocumentSet)
                    .after(ServerSet),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DocumentSet;

fn cancel_requests(
    manager: LspManager,
    requests: Query<(
        Entity,
        &LspRequestOperation,
        &RequestDeadline,
        &OperationState,
    )>,
    bindings: Query<&DocumentBinding>,
    mut commands: Commands,
) {
    for (entity, request, deadline, state) in &requests {
        let current = manager
            .documents
            .get(request.stamp.document)
            .is_ok_and(|(_, _, doc)| {
                doc.version == request.stamp.version
                    && manager.server_entity(&doc.key) == Some(request.stamp.server)
            })
            && bindings
                .get(request.target)
                .is_ok_and(|binding| binding.0 == request.stamp.document);
        let expired = deadline.0.elapsed() >= std::time::Duration::from_secs(10);
        if !current
            || expired
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            if let Ok(client) = manager.clients.get(request.stamp.server) {
                client.cancel_request(request.wire_id);
            }
            if expired && current {
                commands.trigger(OperationCompleted {
                    entity,
                    result: Err("language server request timed out".into()),
                });
            } else if !current && matches!(state, OperationState::Pending | OperationState::Running)
            {
                commands.entity(entity).insert(OperationState::Canceled);
            }
            commands
                .entity(entity)
                .remove::<(LspRequestOperation, RequestDeadline)>();
        }
    }
}

fn drain_requests(
    manager: LspManager,
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
                commands.trigger(OperationCompleted {
                    entity: request_entity,
                    result: Err("language server response channel closed".into()),
                });
                commands
                    .entity(request_entity)
                    .remove::<(LspRequestOperation, RequestDeadline)>();
                continue;
            }
        };
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("language server rejected the request");
            commands.trigger(OperationCompleted {
                entity: request_entity,
                result: Err(message.into()),
            });
            commands
                .entity(request_entity)
                .remove::<(LspRequestOperation, RequestDeadline)>();
            continue;
        }
        commands.trigger(OperationCompleted {
            entity: request_entity,
            result: Ok(()),
        });
        commands
            .entity(request_entity)
            .remove::<(LspRequestOperation, RequestDeadline)>();
        let ready = browsers.can_emit_to(&request.target);
        match &request.kind {
            ReqKind::Hover { line, col } => {
                let blocks = parse_hover(&value);
                if !blocks.is_empty() && ready {
                    commands.trigger(FileUiStateWrite::from_event(
                        request.target,
                        &FileHoverState {
                            value: Some(FileHover {
                                line: *line,
                                col: *col,
                                blocks,
                            }),
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
                let payload = value.get("result").cloned().unwrap_or_default();
                let result = if payload.is_null() {
                    Err("the language server would not rename this".to_string())
                } else {
                    serde_json::from_value::<lsp_types::WorkspaceEdit>(payload)
                        .map_err(|e| format!("the rename could not be read: {e}"))
                };
                writers.edit.write(LspRequestedEdit {
                    entity: request.target,
                    root: root.clone(),
                    result,
                });
            }
            ReqKind::CodeAction => {
                let payload = value.get("result").cloned().unwrap_or_default();
                let offered =
                    serde_json::from_value::<Vec<lsp_types::CodeActionOrCommand>>(payload)
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
                    .insert((OfferedCodeActions(offered), CodeActionSelection::default()));
                if !ready {
                    continue;
                }
                if titles.is_empty() {
                    commands.trigger(crate::host::feedback::EditFailure::new(
                        request.target,
                        "no code actions here".to_string(),
                    ));
                    continue;
                }
                commands.trigger(FileUiStateWrite::from_event(
                    request.target,
                    &FileCodeActions {
                        titles,
                        selected: 0,
                    },
                ));
            }
            ReqKind::Formatting { path, root } => {
                let payload = value.get("result").cloned().unwrap_or_default();
                let result = match serde_json::from_value::<Vec<lsp_types::TextEdit>>(payload) {
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
                        let text = LspLine::read(&path, line);
                        let col = text.char_col(utf16_col);
                        RefItem {
                            display: ref_display(&path, line),
                            path: path.to_string_lossy().into_owned(),
                            line,
                            col,
                            preview: text.as_str().trim().to_string(),
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
                    regions: LspFolds::regions(
                        value.get("result").unwrap_or(&serde_json::Value::Null),
                    ),
                });
            }
            ReqKind::DocumentSymbol => {
                let items = crate::host::explorer::OutlineRows::from_lsp(
                    value.get("result").unwrap_or(&serde_json::Value::Null),
                )
                .into_vec();
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

pub(crate) struct ManagerPlugin {
    diagnostics: LspDiagnosticsSender,
    diagnostics_inbox: LspDiagnosticsInbox,
}

impl ManagerPlugin {
    pub(crate) fn new(
        diagnostics: LspDiagnosticsSender,
        diagnostics_inbox: LspDiagnosticsInbox,
    ) -> Self {
        Self {
            diagnostics,
            diagnostics_inbox,
        }
    }
}

impl Plugin for ManagerPlugin {
    fn build(&self, app: &mut App) {
        let (lint, lint_inbox) = LintDiagnosticsSender::channel();
        app.world_mut().spawn((
            Name::new("LSP startup"),
            LspStartup {
                diagnostics: self.diagnostics.clone(),
                diagnostics_inbox: self.diagnostics_inbox.clone(),
                lint,
                lint_inbox,
            },
        ));
        app.add_plugins((ServersPlugin, RequestsPlugin, DiagnosticsPlugin))
            .add_systems(Startup, start)
            .configure_sets(
                Update,
                crate::lsp::server_request::ServerRequestSet::Answer.after(DocumentSet),
            )
            .add_message::<LspRestartRequest>()
            .add_message::<LspCodeActionRequest>()
            .add_message::<LspDocumentChangeRequest>()
            .add_message::<LspDocumentCloseRequest>()
            .add_observer(move_code_action)
            .add_observer(file_code_action_pick)
            .add_observer(dismiss_code_actions)
            .add_systems(
                Update,
                (
                    close_documents,
                    refresh_bindings,
                    change_documents,
                    open_documents,
                    initialize_views,
                    cleanup_documents,
                    lint_on_open,
                    publish_status,
                )
                    .chain()
                    .in_set(DocumentSet)
                    .after(JobSet::Apply),
            );
    }
}

#[derive(Component)]
struct LspStartup {
    diagnostics: LspDiagnosticsSender,
    diagnostics_inbox: LspDiagnosticsInbox,
    lint: LintDiagnosticsSender,
    lint_inbox: LintDiagnosticsInbox,
}

fn start(
    startup: Single<(Entity, &LspStartup)>,
    inputs: Single<&ServerInputSender>,
    mut commands: Commands,
) {
    let (entity, startup) = *startup;
    commands.spawn((
        Name::new("LSP manager"),
        LspContext {
            diagnostics: startup.diagnostics.clone(),
            inputs: inputs.clone(),
        },
    ));
    commands.spawn((
        Name::new("LSP diagnostics"),
        startup.diagnostics_inbox.clone(),
    ));
    commands.spawn((
        Name::new("Lint diagnostics"),
        startup.lint.clone(),
        startup.lint_inbox.clone(),
    ));
    commands.entity(entity).despawn();
}

fn move_code_action(
    trigger: On<UiInput<FileCodeActionMoveRequest>>,
    mut menus: Query<(&OfferedCodeActions, &mut CodeActionSelection)>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((actions, mut selection)) = menus.get_mut(entity) else {
        return;
    };
    let len = actions.0.len();
    if len == 0 {
        return;
    }
    let delta = if trigger.event().payload.next { 1 } else { -1 };
    let next = selection.0 as i64 + delta;
    selection.0 = next.rem_euclid(len as i64) as usize;
    let mut titles = Vec::with_capacity(len);
    for action in &actions.0 {
        let title = match action {
            lsp_types::CodeActionOrCommand::Command(command) => command.title.clone(),
            lsp_types::CodeActionOrCommand::CodeAction(action) => action.title.clone(),
        };
        titles.push(title);
    }
    commands.trigger(FileUiStateWrite::from_event(
        entity,
        &FileCodeActions {
            titles,
            selected: selection.0 as u32,
        },
    ));
}

fn file_code_action_pick(
    trigger: On<UiInput<FileCodeActionPick>>,
    views: Query<(&Editor, &OfferedCodeActions)>,
    manager: LspManager,
    mut edits: MessageWriter<LspRequestedEdit>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    commands.trigger(FileUiStateWrite::from_event(
        entity,
        &FileCodeActions {
            titles: Vec::new(),
            selected: 0,
        },
    ));
    commands
        .entity(entity)
        .remove::<(OfferedCodeActions, CodeActionSelection)>();
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
    let Some(document) = manager.document_for(entity, path) else {
        return;
    };
    let Some(client) = manager.client(&document.key) else {
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

fn dismiss_code_actions(
    trigger: On<UiInput<FileCodeActionDismissRequest>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    commands.trigger(FileUiStateWrite::from_event(
        entity,
        &FileCodeActions {
            titles: Vec::new(),
            selected: 0,
        },
    ));
    commands
        .entity(entity)
        .remove::<(OfferedCodeActions, CodeActionSelection)>();
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
struct CodeActionSelection(usize);

#[derive(Component, Default)]
pub struct DiagSent(Vec<FileDiagnostic>);

fn emit_diagnostics(
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

struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (drain_diagnostics, drain_lint, emit_diagnostics)
                .chain()
                .in_set(DiagnosticsSet)
                .after(DocumentSet),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct DiagnosticsSet;

fn drain_diagnostics(
    inbox: Single<&LspDiagnosticsInbox>,
    views: Query<(Entity, &FileView, &Editor, Option<&DocumentBinding>)>,
    documents: Query<(&OpenDoc, &ChildOf)>,
    mut commands: Commands,
) {
    for PathDiagnostics {
        path,
        diagnostics: diags,
        version,
        server,
    } in inbox.drain()
    {
        let target = PathIdentity::resolve(&path);
        for (entity, view, edit, binding) in &views {
            if PathIdentity::resolve(&view.path) != target {
                continue;
            }
            if version.is_some() || server.is_some() {
                let Some(binding) = binding else { continue };
                let Ok((document, parent)) = documents.get(binding.0) else {
                    continue;
                };
                if version.is_some_and(|version| version != document.version)
                    || server.is_some_and(|server| server != parent.parent())
                {
                    continue;
                }
            }
            let mapped = LspDiagnostics::map(&diags, |line| {
                LspLine::from_rope(&edit.core.buffer.rope, line)
            });
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
    manager: LspManager,
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
            commands.spawn(request.bundle());
        }
    }
}

#[derive(Component)]
pub struct LintRan;

fn lint_on_open(
    q: Query<(Entity, &FileView, &Editor), Without<LintRan>>,
    outbox: Single<&LintDiagnosticsSender>,
    registry: Single<&LspRegistry>,
    mut commands: Commands,
) {
    let store = store::PackageStore::lsp();
    for (entity, fv, _edit) in &q {
        commands.entity(entity).insert(LintRan);
        let Some(ext) = fv.path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(spec) = registry.linter(ext) else {
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

fn publish_status(
    q: Query<(Entity, &FileView, Option<&LspStatusSent>), With<PageReady>>,
    registry: Single<&LspRegistry>,
    settings: Res<AppSettings>,
    manager: LspManager,
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
        let Some(spec) = registry.server(ext, &overrides) else {
            continue;
        };
        let desired = match store.resolve_command(&spec.command) {
            store::Resolution::Missing => LspServerState::Missing,
            _ if manager.document_for(entity, &fv.path).is_some() => LspServerState::Ready,
            _ => LspServerState::Starting,
        };
        if sent.is_some_and(|s| s.state == desired && s.path == fv.path) {
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let package = (!overrides.contains_key(ext))
            .then(|| registry.package(ext))
            .flatten()
            .map(str::to_string);
        commands.trigger(FileUiStateWrite::from_event(
            entity,
            &FileLspStatus {
                path: fv.path.to_string_lossy().into_owned(),
                server: spec.command.clone(),
                package: package.clone(),
                state: desired,
                capabilities: manager.menu_capabilities(entity, &fv.path),
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
    use vmux_ecs::event::StyledSpan;

    use crate::edit::highlight_cache::HighlightCache;
    use crate::edit::{EditCore, EditMode};
    use crate::host::editor::{Editor, FileView};
    use crate::lsp::{LspDiagnosticsInbox, LspDiagnosticsSender};
    use std::path::PathBuf;

    struct RequestFixture {
        app: App,
        server: Entity,
        document: Entity,
        view: Entity,
        key: ServerKey,
    }

    struct DiagnosticFixture {
        app: App,
        sender: LspDiagnosticsSender,
        servers: [Entity; 2],
        documents: [Entity; 2],
        views: [Entity; 2],
    }

    impl DiagnosticFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(DiagnosticsPlugin)
                .insert_non_send(Browsers::default());
            let (sender, inbox) = LspDiagnosticsSender::channel();
            let (_, lint) = LintDiagnosticsSender::channel();
            app.world_mut().spawn(inbox);
            app.world_mut().spawn(lint);
            let path = PathBuf::from("/workspace/shared.rs");
            let mut servers = Vec::new();
            let mut documents = Vec::new();
            let mut views = Vec::new();
            for root in ["/workspace/one", "/workspace/two"] {
                let owner = app
                    .world_mut()
                    .spawn((WorkspaceRoot, vmux_ecs::Cwd(root.into())))
                    .id();
                let key = ServerKey::new(root, "test-server");
                let server = app
                    .world_mut()
                    .spawn((Server, key.clone(), ChildOf(owner)))
                    .id();
                let document = app
                    .world_mut()
                    .spawn((
                        DocumentPath(path.clone()),
                        OpenDoc { key, version: 1 },
                        ChildOf(server),
                    ))
                    .id();
                let core =
                    EditCore::new(path.clone(), "Rust".into(), "fn x() {}\n", EditMode::Normal);
                let view = app
                    .world_mut()
                    .spawn((
                        FileView { path: path.clone() },
                        Editor::new(
                            core,
                            HighlightCache::plain(&path),
                            crate::fold::FoldState::default(),
                        ),
                        DocumentBinding(document),
                        ServerBinding(server),
                    ))
                    .id();
                servers.push(server);
                documents.push(document);
                views.push(view);
            }
            Self {
                app,
                sender,
                servers: servers.try_into().unwrap(),
                documents: documents.try_into().unwrap(),
                views: views.try_into().unwrap(),
            }
        }

        fn publish(&mut self, source: usize, version: Option<i32>, message: &str) {
            self.sender
                .scoped(self.servers[source])
                .send(PathDiagnostics {
                    path: "/workspace/shared.rs".into(),
                    diagnostics: vec![lsp_types::Diagnostic {
                        message: message.into(),
                        ..Default::default()
                    }],
                    version,
                    server: None,
                });
            self.app.update();
        }

        fn message(&self, view: usize) -> Option<&str> {
            self.app
                .world()
                .get::<LspDiagnostics>(self.views[view])
                .and_then(|diagnostics| diagnostics.raw.first())
                .map(|diagnostic| diagnostic.message.as_str())
        }
    }

    #[test]
    fn versioned_diagnostics_stay_with_their_server_and_current_document() {
        let mut fixture = DiagnosticFixture::new();
        fixture.publish(0, Some(1), "one");
        assert_eq!(fixture.message(0), Some("one"));
        assert_eq!(fixture.message(1), None);
        fixture.publish(1, Some(1), "two");
        assert_eq!(fixture.message(0), Some("one"));
        assert_eq!(fixture.message(1), Some("two"));
        fixture
            .app
            .world_mut()
            .get_mut::<OpenDoc>(fixture.documents[0])
            .unwrap()
            .version = 2;
        fixture.publish(0, Some(1), "stale");
        assert_eq!(fixture.message(0), Some("one"));
        fixture.publish(0, Some(2), "fresh");
        assert_eq!(fixture.message(0), Some("fresh"));
        fixture.publish(0, None, "unversioned");
        assert_eq!(fixture.message(0), Some("unversioned"));
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.documents[0])
            .despawn();
        fixture.publish(0, Some(2), "closed");
        assert_eq!(fixture.message(0), Some("unversioned"));
        assert_eq!(fixture.message(1), Some("two"));
    }

    impl RequestFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(RequestsPlugin)
                .insert_non_send(Browsers::default());
            let root = app
                .world_mut()
                .spawn((WorkspaceRoot, vmux_ecs::Cwd("/workspace".into())))
                .id();
            let key = ServerKey::new("/workspace", "test-server");
            let server = app
                .world_mut()
                .spawn((Server, key.clone(), ChildOf(root)))
                .id();
            let document = app
                .world_mut()
                .spawn((
                    DocumentPath("/workspace/file.rs".into()),
                    OpenDoc {
                        key: key.clone(),
                        version: 1,
                    },
                    ChildOf(server),
                ))
                .id();
            let view = app
                .world_mut()
                .spawn((DocumentBinding(document), ServerBinding(server)))
                .id();
            Self {
                app,
                server,
                document,
                view,
                key,
            }
        }

        fn pending(&mut self) -> (Entity, crossbeam_channel::Sender<serde_json::Value>) {
            let (sender, receiver) = crossbeam_channel::bounded(1);
            let entity = self
                .app
                .world_mut()
                .spawn(
                    LspRequestOperation {
                        target: self.view,
                        kind: ReqKind::Definition,
                        rx: receiver,
                        stamp: RequestStamp {
                            document: self.document,
                            server: self.server,
                            version: 1,
                        },
                        wire_id: 1,
                    }
                    .bundle(),
                )
                .id();
            (entity, sender)
        }

        fn accept(&mut self) -> Entity {
            let (entity, sender) = self.pending();
            sender.send(serde_json::json!({ "result": { "uri": "file:///workspace/target.rs", "range": { "start": { "line": 3, "character": 2 }, "end": { "line": 3, "character": 5 } } } })).unwrap();
            entity
        }
    }

    #[test]
    fn response_envelopes_project_folds_code_actions_and_workspace_edits() {
        for output in 0..4 {
            let mut fixture = RequestFixture::new();
            let (request, sender) = fixture.pending();
            let path = PathBuf::from("/workspace/file.rs");
            let root = PathBuf::from("/workspace");
            let (kind, payload) = match output {
                0 => (
                    ReqKind::Folding { path: path.clone() },
                    serde_json::json!([{ "startLine": 2, "endLine": 7 }]),
                ),
                1 => (
                    ReqKind::CodeAction,
                    serde_json::json!([{ "title": "Apply fix", "command": "fix" }]),
                ),
                2 => (
                    ReqKind::Rename { root: root.clone() },
                    serde_json::json!({ "changes": { "file:///workspace/file.rs": [{ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 3 } }, "newText": "replacement" }] } }),
                ),
                3 => (
                    ReqKind::Formatting {
                        path: path.clone(),
                        root: root.clone(),
                    },
                    serde_json::json!([{ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 3 } }, "newText": "replacement" }]),
                ),
                _ => unreachable!(),
            };
            fixture
                .app
                .world_mut()
                .get_mut::<LspRequestOperation>(request)
                .unwrap()
                .kind = kind;
            sender
                .send(serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": payload }))
                .unwrap();
            fixture.app.update();
            match output {
                0 => {
                    let folds = fixture
                        .app
                        .world_mut()
                        .resource_mut::<Messages<LspFolds>>()
                        .drain()
                        .collect::<Vec<_>>();
                    assert_eq!(folds.len(), 1);
                    assert_eq!(folds[0].entity, fixture.view);
                    assert_eq!(
                        folds[0].regions,
                        vec![crate::fold::FoldRegion { start: 2, end: 7 }]
                    );
                }
                1 => {
                    let actions = &fixture
                        .app
                        .world()
                        .get::<OfferedCodeActions>(fixture.view)
                        .unwrap()
                        .0;
                    assert_eq!(actions.len(), 1);
                    let lsp_types::CodeActionOrCommand::Command(command) = &actions[0] else {
                        panic!("a command action")
                    };
                    assert_eq!(command.command, "fix");
                }
                2 | 3 => {
                    let edits = fixture
                        .app
                        .world_mut()
                        .resource_mut::<Messages<LspRequestedEdit>>()
                        .drain()
                        .collect::<Vec<_>>();
                    assert_eq!(edits.len(), 1);
                    assert_eq!(edits[0].entity, fixture.view);
                    assert_eq!(edits[0].root, root);
                    let edit = edits[0].result.as_ref().unwrap();
                    assert_eq!(edit.changes.as_ref().unwrap().len(), 1);
                    for (uri, changes) in edit.changes.as_ref().unwrap() {
                        assert_eq!(uri.as_str(), "file:///workspace/file.rs");
                        assert_eq!(changes[0].new_text, "replacement");
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn restarting_a_server_refuses_queued_diagnostics_and_preserves_other_roots() {
        let mut fixture = DiagnosticFixture::new();
        fixture.app.add_plugins(ServersPlugin);
        fixture.publish(0, Some(1), "one");
        fixture.publish(1, Some(1), "two");
        fixture.app.world_mut().write_message(LspRestartRequest {
            server: fixture.servers[0],
        });
        fixture.publish(0, Some(1), "queued before restart");
        assert_eq!(fixture.message(0), None);
        assert_eq!(fixture.message(1), Some("two"));
        assert!(
            fixture
                .app
                .world()
                .get_entity(fixture.documents[0])
                .is_err()
        );
        assert!(fixture.app.world().get_entity(fixture.documents[1]).is_ok());
    }

    #[test]
    fn invalidated_request_cleanup_preserves_terminal_operation_states() {
        for state in [
            OperationState::Succeeded,
            OperationState::Failed,
            OperationState::Canceled,
        ] {
            let mut fixture = RequestFixture::new();
            let request = fixture.accept();
            fixture
                .app
                .world_mut()
                .entity_mut(request)
                .insert(state.clone());
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.document)
                .despawn();
            fixture.app.update();
            assert_eq!(
                fixture.app.world().get::<OperationState>(request),
                Some(&state)
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<LspRequestOperation>(request)
                    .is_none()
            );
            assert!(
                fixture
                    .app
                    .world_mut()
                    .resource_mut::<Messages<LspGoto>>()
                    .drain()
                    .next()
                    .is_none()
            );
        }
    }

    #[test]
    fn restarting_one_server_cancels_its_work_without_disturbing_another_server() {
        let mut fixture = RequestFixture::new();
        fixture.app.add_plugins(ServersPlugin);
        let request = fixture.accept();
        let root = fixture
            .app
            .world()
            .get::<ChildOf>(fixture.server)
            .unwrap()
            .parent();
        let key = fixture.key.clone().with_arguments(vec!["other".into()]);
        let other_server = fixture
            .app
            .world_mut()
            .spawn((Server, key.clone(), ChildOf(root)))
            .id();
        let other_document = fixture
            .app
            .world_mut()
            .spawn((
                DocumentPath("/workspace/other.rs".into()),
                OpenDoc { key, version: 1 },
                ChildOf(other_server),
            ))
            .id();
        let other_view = fixture
            .app
            .world_mut()
            .spawn((
                DocumentBinding(other_document),
                ServerBinding(other_server),
                LspOpened,
            ))
            .id();
        fixture.app.world_mut().write_message(LspRestartRequest {
            server: fixture.server,
        });
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(request),
            Some(&OperationState::Canceled)
        );
        assert!(fixture.app.world().get_entity(fixture.server).is_err());
        assert!(fixture.app.world().get_entity(fixture.document).is_err());
        assert!(
            fixture
                .app
                .world()
                .get::<DocumentBinding>(fixture.view)
                .is_none()
        );
        assert!(fixture.app.world().get_entity(other_server).is_ok());
        assert!(fixture.app.world().get_entity(other_document).is_ok());
        assert_eq!(
            fixture
                .app
                .world()
                .get::<DocumentBinding>(other_view)
                .unwrap()
                .0,
            other_document
        );
        assert!(fixture.app.world().get::<LspOpened>(other_view).is_some());
    }

    #[test]
    fn request_timeout_channel_failure_and_rpc_error_leave_observable_failures() {
        for failure in 0..3 {
            let mut fixture = RequestFixture::new();
            let (request, sender) = fixture.pending();
            match failure {
                0 => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(request)
                        .insert(RequestDeadline(
                            web_time::Instant::now() - std::time::Duration::from_secs(11),
                        ));
                }
                1 => {
                    drop(sender);
                }
                2 => {
                    sender.send(serde_json::json!({ "error": { "code": -32603, "message": "server refused" } })).unwrap();
                }
                _ => unreachable!(),
            }
            fixture.app.update();
            assert_eq!(
                fixture.app.world().get::<OperationState>(request),
                Some(&OperationState::Failed)
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<vmux_ecs::OperationError>(request)
                    .is_some()
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<LspRequestOperation>(request)
                    .is_none()
            );
            assert!(
                fixture
                    .app
                    .world_mut()
                    .resource_mut::<Messages<LspGoto>>()
                    .drain()
                    .next()
                    .is_none()
            );
        }
    }

    #[test]
    fn request_results_only_reach_the_current_document_server_and_view_binding() {
        for change in 0..5 {
            let mut fixture = RequestFixture::new();
            let request = fixture.accept();
            match change {
                0 => {}
                1 => {
                    fixture
                        .app
                        .world_mut()
                        .get_mut::<OpenDoc>(fixture.document)
                        .unwrap()
                        .version = 2;
                }
                2 => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.document)
                        .despawn();
                }
                3 => {
                    let replacement = fixture
                        .app
                        .world_mut()
                        .spawn((
                            DocumentPath("/workspace/file.rs".into()),
                            OpenDoc {
                                key: fixture.key.clone(),
                                version: 1,
                            },
                            ChildOf(fixture.server),
                        ))
                        .id();
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.view)
                        .insert(DocumentBinding(replacement));
                }
                4 => {
                    fixture.app.world_mut().entity_mut(fixture.server).despawn();
                    fixture.app.world_mut().spawn((Server, fixture.key.clone()));
                }
                _ => unreachable!(),
            }
            fixture.app.update();
            let replies: Vec<_> = fixture
                .app
                .world_mut()
                .resource_mut::<Messages<LspGoto>>()
                .drain()
                .collect();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<LspRequestOperation>(request)
                    .is_none()
            );
            assert_eq!(
                fixture.app.world().get::<OperationState>(request),
                Some(if change == 0 {
                    &OperationState::Succeeded
                } else {
                    &OperationState::Canceled
                })
            );
            if change == 0 {
                assert_eq!(replies.len(), 1);
                assert_eq!(replies[0].entity, fixture.view);
                assert_eq!(replies[0].path, PathBuf::from("/workspace/target.rs"));
                assert_eq!(replies[0].line, 3);
            } else {
                assert!(replies.is_empty());
            }
        }
    }

    #[test]
    fn a_language_string_hover_survives_as_a_highlighted_code_block() {
        let contents =
            HoverContents::Scalar(MarkedString::LanguageString(lsp_types::LanguageString {
                language: "rust".into(),
                value: "fn build(self) -> StartHeroProps".into(),
            }));
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
            fold: vmux_ecs::event::FoldGutter::None,
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
        let out = LspDiagnostics::from_lines(&lines, &[diag(0, 4, 0, 5, 1, "unused")]);
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
        let regs = LspFolds::regions(&v);
        assert_eq!(regs, vec![crate::fold::FoldRegion { start: 0, end: 3 }]);
    }

    #[test]
    fn utf16_emoji_maps_to_char_index() {
        let lines = vec![fline(0, "😀ab")];
        let line = LspLine::new("😀ab".to_string());
        assert_eq!(line.char_col(2), 1);
        assert_eq!(line.char_col(3), 2);
        let out = LspDiagnostics::from_lines(&lines, &[diag(0, 2, 0, 3, 2, "warn")]);
        assert_eq!(out[0].start_col, 1);
        assert_eq!(out[0].end_col, 2);
        assert_eq!(out[0].severity, DiagSeverity::Warning);
    }

    #[test]
    fn out_of_range_columns_clamp() {
        let lines = vec![fline(0, "ab")];
        let out = LspDiagnostics::from_lines(&lines, &[diag(0, 99, 0, 99, 1, "x")]);
        assert_eq!(out[0].start_col, 2);
        assert_eq!(out[0].end_col, 2);
    }

    #[test]
    fn multiline_range_underlines_first_line_to_eol() {
        let lines = vec![fline(0, "abcdef"), fline(1, "ghi")];
        let out = LspDiagnostics::from_lines(&lines, &[diag(0, 2, 1, 1, 1, "multi")]);
        assert_eq!(out[0].line, 0);
        assert_eq!(out[0].start_col, 2);
        assert_eq!(out[0].end_col, 6);
    }

    #[test]
    fn drain_empties_outbox() {
        let mut app = App::new();
        let (outbox, inbox) = LspDiagnosticsSender::channel();
        let probe = inbox.0.clone();
        app.add_plugins(MinimalPlugins);
        app.world_mut().spawn(inbox);
        outbox.send(PathDiagnostics {
            path: PathBuf::from("/x.rs"),
            diagnostics: vec![],
            version: None,
            server: None,
        });
        app.add_systems(Update, |inbox: Single<&LspDiagnosticsInbox>| {
            drop(inbox.drain());
        });
        app.update();
        assert!(probe.is_empty());
    }

    #[test]
    fn char_utf16_roundtrip_surrogate_pair() {
        let text = "a😀b";
        let line = LspLine::new(text.to_string());
        assert_eq!(line.utf16_col(0), 0);
        assert_eq!(line.utf16_col(1), 1);
        assert_eq!(line.utf16_col(2), 3);
        assert_eq!(line.utf16_col(3), 4);
        assert_eq!(line.char_col(3), 2);
    }

    #[test]
    fn diagnostics_map_through_editor() {
        let path = PathBuf::from("/tmp/vmux_lsp_editor.rs");
        let mut app = App::new();
        let (outbox, inbox) = LspDiagnosticsSender::channel();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, drain_diagnostics);
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
        outbox.send(PathDiagnostics {
            path: path.clone(),
            diagnostics: vec![diag],
            version: None,
            server: None,
        });
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
