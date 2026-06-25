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
use crate::lsp::{LspOutbox, OpenDoc, ServerKey, store};

type ServerOverrides = std::collections::BTreeMap<String, ServerSpec>;

const LSP_MAX_BYTES: u64 = 5 * 1024 * 1024;

pub enum ReqKind {
    Hover { line: u32, col: u32 },
}

pub struct InFlight {
    entity: Entity,
    kind: ReqKind,
    rx: std::sync::mpsc::Receiver<serde_json::Value>,
}

#[derive(Default)]
pub struct LspManager {
    servers: HashMap<ServerKey, ServerClient>,
    open_docs: HashMap<PathBuf, OpenDoc>,
    failed: HashSet<ServerKey>,
    outbox: LspOutbox,
    inflight: Vec<InFlight>,
}

fn uri_for(path: &Path) -> Option<String> {
    url::Url::from_file_path(path).ok().map(|u| u.to_string())
}

fn read_text(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > LSP_MAX_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

impl LspManager {
    fn ensure_server(
        &mut self,
        root: &Path,
        spec: &crate::lsp::registry::ServerSpec,
    ) -> Option<ServerKey> {
        let key = server_key(root, spec);
        if self.servers.contains_key(&key) {
            return Some(key);
        }
        if self.failed.contains(&key) {
            return None;
        }
        match ServerClient::spawn(spec, root, self.outbox.clone()) {
            Ok(client) => {
                self.servers.insert(key.clone(), client);
                Some(key)
            }
            Err(e) => {
                tracing::warn!(server = %spec.command, "lsp spawn/init failed: {e}");
                self.failed.insert(key);
                None
            }
        }
    }

    pub fn open(&mut self, path: &Path, overrides: &ServerOverrides) {
        if self.open_docs.contains_key(path) {
            return;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return;
        };
        let Some(mut spec) = resolve_spec(ext, overrides) else {
            return;
        };
        match store::resolved_command(&store::default_root(), &spec.command) {
            store::Resolution::Managed(p) => spec.command = p.to_string_lossy().into_owned(),
            store::Resolution::OnPath => {}
            store::Resolution::Missing => {
                tracing::info!(server = %spec.command, "lsp server not installed/on PATH; skipping {ext}");
                return;
            }
        }
        let dir = path.parent().unwrap_or(path);
        let root = workspace_root(dir, &spec.root_markers);
        let Some(key) = self.ensure_server(&root, &spec) else {
            return;
        };
        let (Some(uri), Some(text)) = (uri_for(path), read_text(path)) else {
            return;
        };
        if let Some(client) = self.servers.get(&key) {
            client.did_open(&uri, &spec.language_id, 1, &text);
            self.open_docs
                .insert(path.to_path_buf(), OpenDoc { key, version: 1 });
        }
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

    /// Like `change` but sends the supplied in-memory text (for live, unsaved sync).
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
        let Some(doc) = self.open_docs.remove(path) else {
            return;
        };
        if let (Some(uri), Some(client)) = (uri_for(path), self.servers.get(&doc.key)) {
            client.did_close(&uri);
        }
    }

    /// Request hover at a UTF-16 column. `echo_col` is the original char column to
    /// echo back to the page for anchoring.
    pub fn hover(&mut self, entity: Entity, path: &Path, line: u32, utf16_col: u32, echo_col: u32) {
        let Some(doc) = self.open_docs.get(path) else {
            return;
        };
        let Some(uri) = uri_for(path) else {
            return;
        };
        let Some(client) = self.servers.get(&doc.key) else {
            return;
        };
        let (_, rx) = client.send_request(
            "textDocument/hover",
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": utf16_col },
            }),
        );
        self.inflight.push(InFlight {
            entity,
            kind: ReqKind::Hover {
                line,
                col: echo_col,
            },
            rx,
        });
    }
}

fn hover_contents_to_string(c: lsp_types::HoverContents) -> String {
    use lsp_types::{HoverContents, MarkedString};
    let marked = |m: MarkedString| match m {
        MarkedString::String(s) => s,
        MarkedString::LanguageString(ls) => ls.value,
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

fn parse_hover(value: &serde_json::Value) -> String {
    let Some(result) = value.get("result") else {
        return String::new();
    };
    if result.is_null() {
        return String::new();
    }
    serde_json::from_value::<lsp_types::Hover>(result.clone())
        .map(|h| hover_contents_to_string(h.contents))
        .unwrap_or_default()
}

#[derive(Component)]
pub struct LspOpened;

use crate::plugin::{EditState, FileView};

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
    mut manager: NonSendMut<LspManager>,
    mut commands: Commands,
) {
    let overrides = server_overrides(&settings);
    for (entity, fv, _edit) in &q {
        manager.open(&fv.path, &overrides);
        commands.entity(entity).insert(LspOpened);
    }
}

fn drain_lsp_requests(
    mut manager: NonSendMut<LspManager>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    use vmux_core::event::{FILE_HOVER_EVENT, FileHoverEvent};
    let drained = std::mem::take(&mut manager.inflight);
    let mut still = Vec::new();
    for f in drained {
        match f.rx.try_recv() {
            Ok(value) => match f.kind {
                ReqKind::Hover { line, col } => {
                    let contents = parse_hover(&value);
                    if !contents.is_empty()
                        && browsers.has_browser(f.entity)
                        && browsers.host_emit_ready(&f.entity)
                    {
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            f.entity,
                            FILE_HOVER_EVENT,
                            &FileHoverEvent {
                                line,
                                col,
                                contents,
                            },
                        ));
                    }
                }
            },
            Err(std::sync::mpsc::TryRecvError::Empty) => still.push(f),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
        }
    }
    manager.inflight = still;
}

pub fn build(app: &mut App, outbox: LspOutbox) {
    app.insert_non_send(LspManager {
        outbox,
        ..Default::default()
    })
    .init_resource::<LintOutbox>()
    .init_resource::<DiagState>()
    .add_systems(
        Update,
        (
            lsp_open_documents,
            lint_on_open,
            drain_lsp_diagnostics,
            drain_lint,
            drain_lsp_requests,
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
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
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
        state.lsp.insert(target, mapped);
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
    state: Res<DiagState>,
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
            _ if state.lsp.contains_key(&canon(&fv.path)) => LspServerState::Ready,
            _ => LspServerState::Starting,
        };
        if sent.is_some_and(|s| s.state == desired && s.path == fv.path) {
            continue;
        }
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_LSP_STATUS_EVENT,
            &FileLspStatusEvent {
                path: fv.path.to_string_lossy().into_owned(),
                server: spec.command.clone(),
                state: desired,
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

    fn fline(no: u32, text: &str) -> FileLine {
        FileLine {
            line_no: no,
            spans: vec![StyledSpan {
                text: text.into(),
                fg: [0, 0, 0],
                bold: false,
                italic: false,
            }],
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
        use crate::lsp::LspOutbox;
        use crate::plugin::{EditState, FileView};
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
        app.world_mut()
            .spawn((FileView { path: path.clone() }, EditState { core, hl }));

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
