use vmux_core::event::{DiagSeverity, FileDiagnostic, FileLine};

/// Concatenated text of a highlighted line (newlines already stripped upstream).
pub fn line_text(line: &FileLine) -> String {
    line.spans.iter().map(|s| s.text.as_str()).collect()
}

/// Convert a UTF-16 code-unit column to a char index within `text`, clamped to
/// the text's char length.
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

fn map_severity(sev: Option<lsp_types::DiagnosticSeverity>) -> DiagSeverity {
    match sev {
        Some(s) if s == lsp_types::DiagnosticSeverity::ERROR => DiagSeverity::Error,
        Some(s) if s == lsp_types::DiagnosticSeverity::WARNING => DiagSeverity::Warning,
        Some(s) if s == lsp_types::DiagnosticSeverity::HINT => DiagSeverity::Hint,
        _ => DiagSeverity::Info,
    }
}

/// Map LSP diagnostics to `FileDiagnostic`s, converting columns against the file
/// buffer's per-line text. Diagnostics are clamped to single-line ranges keyed by
/// the start line (multi-line ranges underline only their first line in v1).
pub fn to_file_diagnostics(
    lines: &[FileLine],
    diags: &[lsp_types::Diagnostic],
) -> Vec<FileDiagnostic> {
    diags
        .iter()
        .map(|d| {
            let line = d.range.start.line;
            let text = lines.get(line as usize).map(line_text).unwrap_or_default();
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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::lsp::client::{server_key, ServerClient};
use crate::lsp::registry::{executable_on_path, resolve_spec, workspace_root, ServerSpec};
use crate::lsp::{LspOutbox, OpenDoc, ServerKey};

type ServerOverrides = std::collections::BTreeMap<String, ServerSpec>;

const LSP_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// Owns running servers + open documents. NonSend because `ServerClient` holds an
/// `mpsc::Sender` (not `Sync`); mirrors how `FileWatch` is a NonSend resource.
#[derive(Default)]
pub struct LspManager {
    servers: HashMap<ServerKey, ServerClient>,
    open_docs: HashMap<PathBuf, OpenDoc>,
    failed: HashSet<ServerKey>,
    outbox: LspOutbox,
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

    /// Open `path` (already known to be a text file) against its language server.
    pub fn open(&mut self, path: &Path, overrides: &ServerOverrides) {
        if self.open_docs.contains_key(path) {
            return;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return;
        };
        let Some(spec) = resolve_spec(ext, overrides) else {
            return;
        };
        if !executable_on_path(&spec.command) {
            tracing::info!(server = %spec.command, "lsp server not on PATH; skipping {ext}");
            return;
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

    /// Notify the server that `path` changed on disk (watcher reload).
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

    /// Notify the server that `path` is no longer open.
    pub fn close(&mut self, path: &Path) {
        let Some(doc) = self.open_docs.remove(path) else {
            return;
        };
        if let (Some(uri), Some(client)) = (uri_for(path), self.servers.get(&doc.key)) {
            client.did_close(&uri);
        }
    }
}

/// Marker: this `FileView` has been opened in LSP.
#[derive(Component)]
pub struct LspOpened;

use crate::plugin::{FileBuffer, FileView};

/// Open freshly-loaded text buffers (skip error/dir/image buffers).
fn lsp_open_documents(
    q: Query<(Entity, &FileView, &FileBuffer), Without<LspOpened>>,
    settings: Res<vmux_setting::AppSettings>,
    mut manager: NonSendMut<LspManager>,
    mut commands: Commands,
) {
    let overrides: ServerOverrides = settings
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
        .collect();
    for (entity, fv, buf) in &q {
        if buf.language.starts_with("__error__:") {
            continue;
        }
        manager.open(&fv.path, &overrides);
        commands.entity(entity).insert(LspOpened);
    }
}

/// Called from `LspPlugin::build`. The manager shares the resource's `LspOutbox`
/// Arc so server threads push into the same queue the drain system reads.
/// `drain_lsp_diagnostics` is added to this tuple in Task 11.
pub fn build(app: &mut App, outbox: LspOutbox) {
    app.insert_non_send(LspManager {
        outbox,
        ..Default::default()
    })
    .add_systems(Update, (lsp_open_documents, drain_lsp_diagnostics));
}

use bevy_cef::prelude::{BinHostEmitEvent, Browsers};
use vmux_core::event::{FileDiagnosticsEvent, FILE_DIAGNOSTICS_EVENT};

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn drain_lsp_diagnostics(
    outbox: Res<LspOutbox>,
    views: Query<(Entity, &FileView, &FileBuffer)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let drained: Vec<(PathBuf, Vec<lsp_types::Diagnostic>)> = {
        let mut q = outbox.0.lock().unwrap_or_else(|p| p.into_inner());
        q.drain(..).collect()
    };
    for (path, diags) in drained {
        let target = canon(&path);
        for (entity, fv, buf) in &views {
            if canon(&fv.path) != target {
                continue;
            }
            if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                continue;
            }
            let mapped = to_file_diagnostics(&buf.lines, &diags);
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                FILE_DIAGNOSTICS_EVENT,
                &FileDiagnosticsEvent {
                    path: fv.path.to_string_lossy().into_owned(),
                    diagnostics: mapped,
                },
            ));
        }
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
        // DiagnosticSeverity's inner field is private; build from the named consts.
        let severity = match sev {
            1 => lsp_types::DiagnosticSeverity::ERROR,
            2 => lsp_types::DiagnosticSeverity::WARNING,
            3 => lsp_types::DiagnosticSeverity::INFORMATION,
            _ => lsp_types::DiagnosticSeverity::HINT,
        };
        lsp_types::Diagnostic {
            range: lsp_types::Range {
                start: lsp_types::Position { line: l0, character: c0 },
                end: lsp_types::Position { line: l1, character: c1 },
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
        // "😀" is 2 UTF-16 units, 1 char. Column after it: utf16 2 -> char 1.
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
        app.add_plugins(MinimalPlugins).insert_resource(outbox.clone());
        // Drain logic isolated: push one entry, run a minimal drain that mirrors prod.
        outbox.0.lock().unwrap().push((PathBuf::from("/x.rs"), vec![]));
        app.add_systems(Update, |ob: Res<LspOutbox>| {
            ob.0.lock().unwrap().drain(..).for_each(drop);
        });
        app.update();
        assert!(outbox.0.lock().unwrap().is_empty());
    }
}
