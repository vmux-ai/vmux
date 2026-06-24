# Editor LSP — Milestone 1 (Client Core + Diagnostics) Implementation Plan

> **For agentic workers:** Implement this plan **directly / inline** (superpowers:executing-plans), NOT via subagents — vmux CEF builds are large and long-running subagents drop their sockets mid-build. Keep a warm target dir. Steps use checkbox (`- [ ]`) syntax for tracking. The user runtime-tests the UI; do not launch vmux yourself.

**Goal:** Add a read-only LSP client to the `file://` editor that spawns language servers (Helix-style registry + PATH detection), opens documents over JSON-RPC/stdio, and renders `publishDiagnostics` as gutter dots, squiggles, and a soft-glass hover card.

**Architecture:** All LSP machinery lives in the `vmux_editor` native backend (`cfg(not(wasm32))`). Each server runs on its own OS threads (reader/writer/stderr); diagnostics reach Bevy through a shared `Arc<Mutex<Vec<…>>>` outbox drained once per frame and emitted to the file's webview entity — the exact pattern `vmux_git` uses (`crates/vmux_git/src/plugin.rs`). No tokio. The wasm Dioxus page (`page.rs`) only renders diagnostics it receives over the existing rkyv bin-event channel.

**Tech Stack:** Rust, Bevy ECS, `bevy_cef` (rkyv-over-CEF IPC), `lsp-types` + `serde_json` (JSON-RPC), Dioxus 0.7 (wasm UI), syntect (existing highlighter, untouched).

**Spec:** `docs/specs/2026-06-24-editor-lsp-design.md`

---

## Reference: existing patterns this plan mirrors

- **Outbox → per-frame drain → emit:** `crates/vmux_git/src/plugin.rs` (`GitOutbox`, `spawn_job`, `drain_git_outbox`).
- **Host→page emit:** `commands.trigger(BinHostEmitEvent::from_rkyv(entity, EVENT_NAME, &payload))`, gated on `browsers.has_browser(entity) && browsers.host_emit_ready(&entity)` (`crates/vmux_editor/src/plugin.rs:340-363`).
- **NonSend resource holding OS handles:** `FileWatch` via `app.insert_non_send(...)` (`crates/vmux_editor/src/plugin.rs:705-717`).
- **rkyv event type + roundtrip test:** `crates/vmux_core/src/event.rs` (`FileViewportPatch`, `file_viewport_patch_rkyv_roundtrip`).
- **Frontend listener:** `use_bin_event_listener::<T, _>(NAME, move |payload| { … })` (`crates/vmux_editor/src/page.rs:357-471`).
- **Native-testable pure UI helpers:** `crates/vmux_editor/src/page_model.rs` (compiled under `cfg(any(target_arch = "wasm32", test))`).

**Important constraint:** `to_styled_span` strips trailing newlines from span text (`highlight.rs:93`), so `FileBuffer.lines` cannot reconstruct exact document text. The LSP `didOpen`/`didChange` text is **re-read from disk** by the manager. Per-line character content still matches `FileBuffer.lines` (highlighting does not alter characters), so diagnostic columns line up.

---

## File Structure

**Create:**
- `crates/vmux_editor/src/lsp.rs` — `LspPlugin`, submodule declarations, shared types (`ServerKey`, `OpenDoc`, `LspOutbox`), plugin wiring.
- `crates/vmux_editor/src/lsp/framing.rs` — `write_message` / `read_message` (Content-Length JSON-RPC framing). Pure, fully unit-tested.
- `crates/vmux_editor/src/lsp/registry.rs` — `ServerSpec`, `spec_for_extension`, `workspace_root`, `executable_on_path`.
- `crates/vmux_editor/src/lsp/client.rs` — `ServerClient` (spawn, threads, `initialize` handshake, `did_open`/`did_change`/`did_close`/`shutdown`) + `dispatch_message` routing.
- `crates/vmux_editor/src/lsp/manager.rs` — `LspManager` (NonSend), open/close/change methods, column conversion, diagnostic mapping, Bevy systems.
- `crates/vmux_editor/src/bin/vmux_mock_lsp.rs` — minimal mock LSP server used only by the integration test.
- `crates/vmux_editor/tests/lsp_integration.rs` — spawn the mock through `ServerClient`, assert handshake + diagnostics land in the outbox.

**Modify:**
- `crates/vmux_core/src/event.rs` — add `DiagSeverity`, `FileDiagnostic`, `FileDiagnosticsEvent`, `FILE_DIAGNOSTICS_EVENT`.
- `crates/vmux_editor/Cargo.toml` — add `serde_json`, `lsp-types` (native), declare the mock bin.
- `crates/vmux_editor/src/lib.rs` — declare `lsp` module (native), re-export `LspPlugin`.
- `crates/vmux_editor/src/plugin.rs` — add `LspPlugin` to `EditorPlugin`; call `did_change` in `reload_changed_files`; call `did_close`/remove `LspOpened` in `on_file_open`.
- `crates/vmux_editor/src/page_model.rs` — add `diagnostics_in_window`, `diag_overlay`, `line_severity` pure helpers.
- `crates/vmux_editor/src/page.rs` — diagnostics listener + gutter dot + squiggle overlay + soft-glass hover card.
- `crates/vmux_setting/src/plugin/runtime.rs` — add `EditorSettings`/`LspSettings` (config override task).

**Where `LspPlugin` is added:** `crates/vmux_desktop` already adds `EditorPlugin`; `LspPlugin` is added inside `EditorPlugin::build` so nothing in `vmux_desktop` changes.

---

## Task 1: Diagnostics data model in `vmux_core`

**Files:**
- Modify: `crates/vmux_core/src/event.rs`

- [ ] **Step 1: Write the failing test**

Append to the `file_event_tests` module in `crates/vmux_core/src/event.rs` (the module starting at `mod file_event_tests`):

```rust
    #[test]
    fn file_diagnostics_event_rkyv_roundtrip() {
        let ev = FileDiagnosticsEvent {
            path: "/src/main.rs".into(),
            diagnostics: vec![FileDiagnostic {
                line: 3,
                start_col: 4,
                end_col: 9,
                severity: DiagSeverity::Error,
                message: "cannot find value `x`".into(),
                source: Some("rustc".into()),
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&ev).expect("ser");
        let back =
            rkyv::from_bytes::<FileDiagnosticsEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(back.path, "/src/main.rs");
        assert_eq!(back.diagnostics.len(), 1);
        assert_eq!(back.diagnostics[0].line, 3);
        assert_eq!(back.diagnostics[0].end_col, 9);
        assert_eq!(back.diagnostics[0].severity, DiagSeverity::Error);
        assert_eq!(back.diagnostics[0].source.as_deref(), Some("rustc"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_core file_diagnostics_event_rkyv_roundtrip`
Expected: FAIL — `cannot find type FileDiagnosticsEvent`.

- [ ] **Step 3: Add the types**

Add near the other `FILE_*` event-name constants (after `pub const FILE_IMAGE_EVENT` at `event.rs:25`):

```rust
pub const FILE_DIAGNOSTICS_EVENT: &str = "file_diagnostics";
```

Add these types (place them right before `#[cfg(test)] mod file_event_tests`, around `event.rs:280`):

```rust
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum DiagSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FileDiagnostic {
    /// 0-based absolute line number.
    pub line: u32,
    /// Char index within the line (NOT UTF-16, NOT byte).
    pub start_col: u32,
    /// Char index within the line, exclusive.
    pub end_col: u32,
    pub severity: DiagSeverity,
    pub message: String,
    pub source: Option<String>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FileDiagnosticsEvent {
    pub path: String,
    pub diagnostics: Vec<FileDiagnostic>,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_core file_diagnostics_event_rkyv_roundtrip`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/event.rs
git commit -m "feat(core): add diagnostics event types for editor LSP"
```

---

## Task 2: Dependencies + LSP module skeleton

This task only makes the crate compile with an empty `LspPlugin` wired in. No behavior yet.

**Files:**
- Modify: `crates/vmux_editor/Cargo.toml`
- Create: `crates/vmux_editor/src/lsp.rs`
- Modify: `crates/vmux_editor/src/lib.rs`
- Modify: `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1: Add dependencies**

In `crates/vmux_editor/Cargo.toml`, under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` (after `notify = { workspace = true }`, around line 28), add:

```toml
serde_json = { workspace = true }
lsp-types = "0.97"
```

If `0.97` does not resolve, run `cargo add lsp-types -p vmux_editor` and use whatever current version it writes.

**URI handling note:** lsp-types 0.96+ replaced `url::Url` with a `fluent_uri`-backed `Uri` type that has **no** `from_file_path`/`to_file_path`. This plan therefore does all path↔URI conversion with the **`url` crate** (already a dependency of `vmux_editor`, `Cargo.toml:26`), treating `lsp_types::Uri` as a string via `.as_str()`. Never write `lsp_types::Url` — it does not exist in 0.97.

The integration test (Task 8) is a separate crate and needs the `url` crate for path→URI conversion. Add it to the existing `[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]` block (which already has `tempfile = "3"`):

```toml
url = { workspace = true }
```

(The mock-server `[[bin]]` declaration is added in Task 7, together with its source file — declaring it here would break `cargo build` because the path would not yet exist.)

- [ ] **Step 2: Create the module skeleton**

Create `crates/vmux_editor/src/lsp.rs`:

```rust
use bevy::prelude::*;

pub mod client;
pub mod framing;
pub mod manager;
pub mod registry;

pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, _app: &mut App) {}
}
```

Create placeholder files so the module declarations resolve:

`crates/vmux_editor/src/lsp/framing.rs`:

```rust
// Content-Length JSON-RPC framing. Filled in Task 3.
```

`crates/vmux_editor/src/lsp/registry.rs`:

```rust
// Language-server registry + PATH detection. Filled in Task 4.
```

`crates/vmux_editor/src/lsp/client.rs`:

```rust
// JSON-RPC server client. Filled in Tasks 5-6.
```

`crates/vmux_editor/src/lsp/manager.rs`:

```rust
// LspManager + Bevy systems. Filled in Tasks 9-11.
```

- [ ] **Step 3: Declare the module and re-export**

In `crates/vmux_editor/src/lib.rs`, after the `plugin` module block (after line 14), add:

```rust
#[cfg(not(target_arch = "wasm32"))]
mod lsp;
#[cfg(not(target_arch = "wasm32"))]
pub use lsp::LspPlugin;
```

- [ ] **Step 4: Wire `LspPlugin` into `EditorPlugin`**

In `crates/vmux_editor/src/plugin.rs`, inside `impl Plugin for EditorPlugin`, change the start of the `add_plugins` chain (line 718) so `LspPlugin` is added first. Replace:

```rust
        app.add_plugins(BinEventEmitterPlugin::<(
```

with:

```rust
        app.add_plugins(crate::lsp::LspPlugin)
            .add_plugins(BinEventEmitterPlugin::<(
```

(The existing `.add_systems(...)` chain still follows; `add_plugins` returns `&mut App` so the chain is unchanged.)

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p vmux_editor`
Expected: builds clean (warnings about unused files are fine). `lsp-types` and `serde_json` resolve.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_editor/Cargo.toml crates/vmux_editor/src/lsp.rs crates/vmux_editor/src/lsp/ crates/vmux_editor/src/lib.rs crates/vmux_editor/src/plugin.rs
git commit -m "feat(editor): scaffold LSP module + deps"
```

---

## Task 3: JSON-RPC framing

**Files:**
- Modify: `crates/vmux_editor/src/lsp/framing.rs`

- [ ] **Step 1: Write the failing tests**

Replace the contents of `crates/vmux_editor/src/lsp/framing.rs`:

```rust
use std::io::{self, BufRead, Write};

use serde_json::Value;

/// Write a single JSON-RPC message with a `Content-Length` header.
pub fn write_message<W: Write>(w: &mut W, msg: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(msg)?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()
}

/// Read a single JSON-RPC message. Returns `Ok(None)` on clean EOF.
pub fn read_message<R: BufRead>(r: &mut R) -> io::Result<Option<Value>> {
    let mut content_len: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line)?;
        if n == 0 {
            return Ok(None); // EOF
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break; // end of headers
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            content_len = rest.trim().parse::<usize>().ok();
        }
    }
    let len = content_len
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    let value = serde_json::from_slice(&buf)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn write_then_read_roundtrip() {
        let msg = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
        let mut buf = Vec::new();
        write_message(&mut buf, &msg).unwrap();
        let header = String::from_utf8(buf[..20].to_vec()).unwrap();
        assert!(header.starts_with("Content-Length: "), "got: {header}");
        let mut cur = Cursor::new(buf);
        let back = read_message(&mut cur).unwrap().unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn reads_two_messages_from_one_stream() {
        let mut buf = Vec::new();
        write_message(&mut buf, &json!({"id": 1})).unwrap();
        write_message(&mut buf, &json!({"id": 2})).unwrap();
        let mut cur = Cursor::new(buf);
        assert_eq!(read_message(&mut cur).unwrap().unwrap(), json!({"id": 1}));
        assert_eq!(read_message(&mut cur).unwrap().unwrap(), json!({"id": 2}));
        assert!(read_message(&mut cur).unwrap().is_none()); // EOF
    }

    #[test]
    fn body_split_across_reads_is_reassembled() {
        // BufReader with a tiny capacity forces read_exact to loop.
        let mut raw = Vec::new();
        write_message(&mut raw, &json!({"hello": "world", "n": 42})).unwrap();
        let mut cur = std::io::BufReader::with_capacity(4, Cursor::new(raw));
        let back = read_message(&mut cur).unwrap().unwrap();
        assert_eq!(back, json!({"hello": "world", "n": 42}));
    }

    #[test]
    fn missing_content_length_errors() {
        let mut cur = Cursor::new(b"\r\n{}".to_vec());
        assert!(read_message(&mut cur).is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vmux_editor --lib lsp::framing`
Expected: 4 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/lsp/framing.rs
git commit -m "feat(editor): LSP Content-Length framing"
```

---

## Task 4: Server registry + PATH detection + workspace root

**Files:**
- Modify: `crates/vmux_editor/src/lsp/registry.rs`

- [ ] **Step 1: Write the failing tests**

Replace the contents of `crates/vmux_editor/src/lsp/registry.rs`:

```rust
use std::path::{Path, PathBuf};

/// How to launch a language server for a given file extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSpec {
    pub command: String,
    pub args: Vec<String>,
    /// LSP `languageId` to send in `didOpen`.
    pub language_id: String,
    /// Ancestor files/dirs that mark the workspace root, most-specific first.
    pub root_markers: Vec<String>,
}

fn spec(command: &str, args: &[&str], language_id: &str, markers: &[&str]) -> ServerSpec {
    ServerSpec {
        command: command.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        language_id: language_id.to_string(),
        root_markers: markers.iter().map(|s| s.to_string()).collect(),
    }
}

/// Built-in registry: file extension -> server spec. Helix/Neovim model.
/// The user installs the server; we only spawn it if found on PATH.
pub fn builtin_spec(ext: &str) -> Option<ServerSpec> {
    Some(match ext {
        "rs" => spec("rust-analyzer", &[], "rust", &["Cargo.toml", ".git"]),
        "py" | "pyi" => spec("pyright-langserver", &["--stdio"], "python", &["pyproject.toml", "setup.py", ".git"]),
        "ts" => spec("typescript-language-server", &["--stdio"], "typescript", &["package.json", "tsconfig.json", ".git"]),
        "tsx" => spec("typescript-language-server", &["--stdio"], "typescriptreact", &["package.json", "tsconfig.json", ".git"]),
        "js" => spec("typescript-language-server", &["--stdio"], "javascript", &["package.json", ".git"]),
        "jsx" => spec("typescript-language-server", &["--stdio"], "javascriptreact", &["package.json", ".git"]),
        "go" => spec("gopls", &[], "go", &["go.mod", ".git"]),
        "c" | "h" => spec("clangd", &[], "c", &["compile_commands.json", ".git"]),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => spec("clangd", &[], "cpp", &["compile_commands.json", ".git"]),
        "lua" => spec("lua-language-server", &[], "lua", &[".luarc.json", ".git"]),
        "rb" => spec("solargraph", &["stdio"], "ruby", &["Gemfile", ".git"]),
        "zig" => spec("zls", &[], "zig", &["build.zig", ".git"]),
        "sh" | "bash" => spec("bash-language-server", &["start"], "shellscript", &[".git"]),
        "json" => spec("vscode-json-language-server", &["--stdio"], "json", &[".git"]),
        "yaml" | "yml" => spec("yaml-language-server", &["--stdio"], "yaml", &[".git"]),
        "toml" => spec("taplo", &["lsp", "stdio"], "toml", &[".git"]),
        "md" | "markdown" => spec("marksman", &["server"], "markdown", &[".git"]),
        "java" => spec("jdtls", &[], "java", &["pom.xml", "build.gradle", ".git"]),
        _ => return None,
    })
}

/// True if `command` resolves to an executable on `PATH` (or is an absolute path
/// that exists). Mirrors a `which`-style lookup without adding a dependency.
pub fn executable_on_path(command: &str) -> bool {
    let p = Path::new(command);
    if p.is_absolute() {
        return p.is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(command).is_file())
}

/// Walk up from `start` (a file's directory) looking for any `markers` entry.
/// Falls back to `start` itself when no marker is found.
pub fn workspace_root(start: &Path, markers: &[String]) -> PathBuf {
    let mut dir = Some(start);
    while let Some(d) = dir {
        for m in markers {
            if d.join(m).exists() {
                return d.to_path_buf();
            }
        }
        dir = d.parent();
    }
    start.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions_map_to_servers() {
        assert_eq!(builtin_spec("rs").unwrap().command, "rust-analyzer");
        assert_eq!(builtin_spec("rs").unwrap().language_id, "rust");
        assert_eq!(builtin_spec("tsx").unwrap().language_id, "typescriptreact");
        assert_eq!(builtin_spec("cpp").unwrap().language_id, "cpp");
        assert!(builtin_spec("xyzzy").is_none());
    }

    #[test]
    fn executable_lookup_finds_a_real_binary() {
        // `cargo` is on PATH in any build environment running this test.
        assert!(executable_on_path("cargo"));
        assert!(!executable_on_path("definitely-not-a-real-binary-zzz"));
    }

    #[test]
    fn workspace_root_finds_marker_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("Cargo.toml"), "").unwrap();
        let nested = root.join("crates").join("a").join("src");
        std::fs::create_dir_all(&nested).unwrap();
        // workspace_root does not canonicalize; it returns the ancestor as walked
        // from `nested`, which equals `root` exactly.
        let found = workspace_root(&nested, &["Cargo.toml".into(), ".git".into()]);
        assert_eq!(found, root);
    }

    #[test]
    fn workspace_root_falls_back_to_start() {
        let tmp = tempfile::tempdir().unwrap();
        let start = tmp.path().join("no").join("markers");
        std::fs::create_dir_all(&start).unwrap();
        assert_eq!(workspace_root(&start, &["Cargo.toml".into()]), start);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vmux_editor --lib lsp::registry`
Expected: 4 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/lsp/registry.rs
git commit -m "feat(editor): LSP server registry + PATH detection"
```

---

## Task 5: Message dispatch + outbox type

This is the pure routing logic the reader thread will use: route responses to pending requests, route `publishDiagnostics` notifications to the outbox.

**Files:**
- Modify: `crates/vmux_editor/src/lsp/client.rs`
- Modify: `crates/vmux_editor/src/lsp.rs` (add `LspOutbox` shared type)

- [ ] **Step 1: Add the shared outbox type**

In `crates/vmux_editor/src/lsp.rs`, replace the file contents with:

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;

pub mod client;
pub mod framing;
pub mod manager;
pub mod registry;

/// Diagnostics produced by any server, keyed by absolute file path. Drained once
/// per frame by `manager::drain_lsp_diagnostics`. Mirrors `vmux_git::GitOutbox`.
#[derive(Resource, Clone, Default)]
pub struct LspOutbox(pub Arc<Mutex<Vec<(PathBuf, Vec<lsp_types::Diagnostic>)>>>);

/// Identifies a running server: workspace root + server command.
pub type ServerKey = (PathBuf, String);

/// A document currently opened against a server.
pub struct OpenDoc {
    pub key: ServerKey,
    pub version: i32,
}

pub type PendingMap = Arc<Mutex<HashMap<i64, std::sync::mpsc::Sender<serde_json::Value>>>>;

pub struct LspPlugin;

impl Plugin for LspPlugin {
    // Wired up in Task 10 once `manager::build` exists. Empty until then so the
    // crate compiles through Tasks 5-9.
    fn build(&self, _app: &mut App) {}
}
```

- [ ] **Step 2: Write the failing dispatch tests**

Replace the contents of `crates/vmux_editor/src/lsp/client.rs`:

```rust
use std::path::PathBuf;

use serde_json::Value;

use crate::lsp::{LspOutbox, PendingMap};

/// Convert a `file://` URI string to a filesystem path (via the `url` crate;
/// `lsp_types::Uri` has no path conversion).
pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}

/// Route one incoming JSON-RPC message.
/// - Responses (have `id` + `result`/`error`) go to the matching pending sender.
/// - `textDocument/publishDiagnostics` notifications go to the outbox.
/// - Everything else is ignored.
pub fn dispatch_message(msg: Value, pending: &PendingMap, outbox: &LspOutbox) {
    if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
        if msg.get("method").is_none() {
            // Response to a request we sent.
            if let Some(tx) = pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id)
            {
                let _ = tx.send(msg);
            }
            return;
        }
        // else: a server->client request; ignored in milestone 1.
    }
    let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
    if method == "textDocument/publishDiagnostics" {
        let Some(params) = msg.get("params") else {
            return;
        };
        let Ok(parsed) =
            serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(params.clone())
        else {
            return;
        };
        if let Some(path) = path_from_uri(parsed.uri.as_str()) {
            outbox
                .0
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push((path, parsed.diagnostics));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc;

    fn outbox() -> LspOutbox {
        LspOutbox::default()
    }
    fn pending() -> PendingMap {
        PendingMap::default()
    }

    #[test]
    fn publish_diagnostics_lands_in_outbox() {
        let ob = outbox();
        let pd = pending();
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": "file:///tmp/main.rs",
                "diagnostics": [{
                    "range": {"start": {"line": 1, "character": 2},
                              "end": {"line": 1, "character": 5}},
                    "severity": 1,
                    "message": "boom",
                    "source": "rustc"
                }]
            }
        });
        dispatch_message(msg, &pd, &ob);
        let q = ob.0.lock().unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].0, PathBuf::from("/tmp/main.rs"));
        assert_eq!(q[0].1.len(), 1);
        assert_eq!(q[0].1[0].message, "boom");
    }

    #[test]
    fn response_routes_to_pending_sender() {
        let ob = outbox();
        let pd = pending();
        let (tx, rx) = mpsc::channel();
        pd.lock().unwrap().insert(7, tx);
        dispatch_message(json!({"jsonrpc": "2.0", "id": 7, "result": {}}), &pd, &ob);
        let got = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
        assert_eq!(got["id"], 7);
        assert!(pd.lock().unwrap().is_empty(), "pending entry consumed");
    }

    #[test]
    fn unknown_notification_is_ignored() {
        let ob = outbox();
        let pd = pending();
        dispatch_message(json!({"method": "window/logMessage", "params": {}}), &pd, &ob);
        assert!(ob.0.lock().unwrap().is_empty());
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p vmux_editor --lib lsp::client`
Expected: 3 tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_editor/src/lsp.rs crates/vmux_editor/src/lsp/client.rs
git commit -m "feat(editor): LSP message dispatch + outbox"
```

---

## Task 6: `ServerClient` — spawn, threads, handshake, document notifications

**Files:**
- Modify: `crates/vmux_editor/src/lsp/client.rs`

- [ ] **Step 1: Add the client implementation**

Append to `crates/vmux_editor/src/lsp/client.rs` (above the `#[cfg(test)]` module):

```rust
use std::io::BufReader;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use std::collections::HashMap;

use crate::lsp::registry::ServerSpec;
use crate::lsp::{framing, ServerKey};

/// A running language-server process plus its I/O threads.
pub struct ServerClient {
    child: Child,
    outgoing: mpsc::Sender<serde_json::Value>,
    pending: PendingMap,
    next_id: AtomicI64,
    _reader: JoinHandle<()>,
    _writer: JoinHandle<()>,
    _stderr: JoinHandle<()>,
}

impl ServerClient {
    /// Spawn `spec.command` rooted at `root`, run the `initialize`/`initialized`
    /// handshake, and start the I/O threads. Diagnostics flow into `outbox`.
    pub fn spawn(spec: &ServerSpec, root: &std::path::Path, outbox: LspOutbox) -> std::io::Result<Self> {
        let mut child = Command::new(&spec.command)
            .args(&spec.args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));

        // Writer thread: serialize outgoing messages and frame them.
        let (outgoing, out_rx) = mpsc::channel::<serde_json::Value>();
        let writer = std::thread::spawn(move || {
            let mut w = stdin;
            while let Ok(msg) = out_rx.recv() {
                if framing::write_message(&mut w, &msg).is_err() {
                    break;
                }
            }
        });

        // Reader thread: parse frames and dispatch.
        let r_pending = pending.clone();
        let r_outbox = outbox.clone();
        let reader = std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            loop {
                match framing::read_message(&mut r) {
                    Ok(Some(msg)) => dispatch_message(msg, &r_pending, &r_outbox),
                    Ok(None) | Err(_) => break, // EOF or fatal parse error
                }
            }
        });

        // stderr thread: drain to the log.
        let cmd_name = spec.command.clone();
        let stderr_thread = std::thread::spawn(move || {
            use std::io::BufRead;
            let r = BufReader::new(stderr);
            for line in r.lines().map_while(Result::ok) {
                tracing::debug!(server = %cmd_name, "lsp stderr: {line}");
            }
        });

        let client = ServerClient {
            child,
            outgoing,
            pending,
            next_id: AtomicI64::new(1),
            _reader: reader,
            _writer: writer,
            _stderr: stderr_thread,
        };

        client.initialize(root)?;
        Ok(client)
    }

    fn notify(&self, method: &str, params: serde_json::Value) {
        let _ = self.outgoing.send(serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    /// Send a request and block up to `timeout` for the matching response.
    fn request(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> std::io::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
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
        rx.recv_timeout(timeout).map_err(|_| {
            self.pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            std::io::Error::new(std::io::ErrorKind::TimedOut, "lsp request timed out")
        })
    }

    fn initialize(&self, root: &std::path::Path) -> std::io::Result<()> {
        let root_uri = url::Url::from_file_path(root)
            .map(|u| u.to_string())
            .unwrap_or_default();
        let params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": { "relatedInformation": false }
                }
            },
            "clientInfo": { "name": "vmux" }
        });
        self.request("initialize", params, Duration::from_secs(10))?;
        self.notify("initialized", serde_json::json!({}));
        Ok(())
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
        // Full-document sync (no editing surface yet).
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

/// Helper used by the manager to key a spawned server.
pub fn server_key(root: &std::path::Path, spec: &ServerSpec) -> ServerKey {
    (root.to_path_buf(), spec.command.clone())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p vmux_editor`
Expected: builds clean.

- [ ] **Step 3: Run existing client tests still pass**

Run: `cargo test -p vmux_editor --lib lsp::client`
Expected: the 3 dispatch tests still PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_editor/src/lsp/client.rs
git commit -m "feat(editor): ServerClient spawn + handshake + document sync"
```

---

## Task 7: Mock LSP server binary

A deterministic stand-in used only by the Task 8 integration test.

**Files:**
- Create: `crates/vmux_editor/src/bin/vmux_mock_lsp.rs`

- [ ] **Step 1: Write the mock**

Create `crates/vmux_editor/src/bin/vmux_mock_lsp.rs`:

```rust
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
```

Now declare the bin in `crates/vmux_editor/Cargo.toml` (add at the end of the file):

```toml
[[bin]]
name = "vmux_mock_lsp"
path = "src/bin/vmux_mock_lsp.rs"
```

- [ ] **Step 2: Make `framing` reachable from the bin**

The bin uses `vmux_editor::lsp::framing`. In `crates/vmux_editor/src/lib.rs`, change the `lsp` module visibility from `mod lsp;` to `pub mod lsp;` (both `#[cfg(not(target_arch = "wasm32"))]` lines added in Task 2):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod lsp;
#[cfg(not(target_arch = "wasm32"))]
pub use lsp::LspPlugin;
```

- [ ] **Step 3: Verify the bin builds**

Run: `cargo build -p vmux_editor --bin vmux_mock_lsp`
Expected: builds an executable.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_editor/src/bin/vmux_mock_lsp.rs crates/vmux_editor/src/lib.rs crates/vmux_editor/Cargo.toml
git commit -m "test(editor): mock LSP server binary"
```

---

## Task 8: End-to-end integration test (spawn → handshake → diagnostics)

**Files:**
- Create: `crates/vmux_editor/tests/lsp_integration.rs`

- [ ] **Step 1: Write the integration test**

Create `crates/vmux_editor/tests/lsp_integration.rs`:

```rust
use std::time::{Duration, Instant};

use vmux_editor::lsp::client::ServerClient;
use vmux_editor::lsp::registry::ServerSpec;
use vmux_editor::lsp::LspOutbox;

#[test]
fn mock_server_handshake_and_diagnostics() {
    let mock = env!("CARGO_BIN_EXE_vmux_mock_lsp");
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("main.rs");
    std::fs::write(&file, "fn x() {}\n").unwrap();

    let spec = ServerSpec {
        command: mock.to_string(),
        args: vec![],
        language_id: "rust".into(),
        root_markers: vec![".git".into()],
    };

    let outbox = LspOutbox::default();
    // spawn() runs the initialize/initialized handshake; Ok means it completed.
    let client = ServerClient::spawn(&spec, tmp.path(), outbox.clone())
        .expect("mock server spawns and initializes");

    let uri = url::Url::from_file_path(&file).unwrap().to_string();
    client.did_open(&uri, "rust", 1, "fn x() {}\n");

    // Poll the outbox until the mock's publishDiagnostics arrives.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some((path, diags)) = outbox.0.lock().unwrap().first().cloned() {
            assert_eq!(path, file);
            assert_eq!(diags.len(), 1);
            assert_eq!(diags[0].message, "mock diagnostic");
            return;
        }
        assert!(Instant::now() < deadline, "no diagnostics within timeout");
        std::thread::sleep(Duration::from_millis(20));
    }
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test -p vmux_editor --test lsp_integration`
Expected: PASS (builds the mock bin automatically, spawns it, asserts diagnostics).

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/tests/lsp_integration.rs
git commit -m "test(editor): LSP client end-to-end via mock server"
```

---

## Task 9: Column conversion + diagnostic mapping (pure)

LSP positions are UTF-16 code units; the page renders char-indexed. Convert host-side using the buffer's line text.

**Files:**
- Modify: `crates/vmux_editor/src/lsp/manager.rs`

- [ ] **Step 1: Write the failing tests**

Replace the contents of `crates/vmux_editor/src/lsp/manager.rs`:

```rust
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
            let text = lines
                .get(line as usize)
                .map(line_text)
                .unwrap_or_default();
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
}
```

Note: `DiagnosticSeverity`'s inner field is private, so both tests and production use the named consts (`ERROR`/`WARNING`/`INFORMATION`/`HINT`); `map_severity` compares with `==` (the type derives `PartialEq`).

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vmux_editor --lib lsp::manager`
Expected: 4 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/lsp/manager.rs
git commit -m "feat(editor): LSP column conversion + diagnostic mapping"
```

---

## Task 10: `LspManager` resource + document lifecycle wiring

**Files:**
- Modify: `crates/vmux_editor/src/lsp/manager.rs`
- Modify: `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1: Add the manager + systems**

Append to `crates/vmux_editor/src/lsp/manager.rs` (above the `#[cfg(test)]` module):

```rust
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::lsp::client::{server_key, ServerClient};
use crate::lsp::registry::{builtin_spec, executable_on_path, workspace_root};
use crate::lsp::{LspOutbox, OpenDoc, ServerKey};

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
    fn ensure_server(&mut self, root: &Path, spec: &crate::lsp::registry::ServerSpec) -> Option<ServerKey> {
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
    pub fn open(&mut self, path: &Path) {
        if self.open_docs.contains_key(path) {
            return;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return;
        };
        let Some(spec) = builtin_spec(ext) else {
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
    mut manager: NonSendMut<LspManager>,
    mut commands: Commands,
) {
    for (entity, fv, buf) in &q {
        if buf.language.starts_with("__error__:") {
            continue;
        }
        manager.open(&fv.path);
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
    .add_systems(Update, lsp_open_documents);
}
```

Teardown: no explicit `AppExit` system. When the app's `World` drops, the NonSend `LspManager` drops, dropping each `ServerClient`, whose `Drop` impl kills and reaps the child (Task 6). Graceful `shutdown` messaging is deferred to a later milestone.

- [ ] **Step 2: Wire `LspPlugin` to share the outbox Arc**

In `crates/vmux_editor/src/lsp.rs`, fill in `LspPlugin::build` (currently an empty stub from Task 5) so the resource and the manager share one `LspOutbox` Arc:

```rust
impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        let outbox = LspOutbox::default();
        app.insert_resource(outbox.clone());
        manager::build(app, outbox);
    }
}
```

- [ ] **Step 3: Make `FileBuffer`/`FileView` reachable**

In `crates/vmux_editor/src/plugin.rs`, the `FileBuffer` struct (line 24) is currently `pub` already? Confirm both are `pub`: `FileView` is `pub struct FileView` (line 19) ✓; `FileBuffer` is `pub struct FileBuffer` (line 24) ✓. No change needed — `use crate::plugin::{FileBuffer, FileView};` in `manager.rs` resolves.

- [ ] **Step 4: Call `change` on watcher reload**

In `crates/vmux_editor/src/plugin.rs`, `reload_changed_files` (line 600) re-highlights changed text files. Add a `did_change` call. Change the function signature to take the manager, and call it in the text branch after the buffer is rebuilt.

Change the signature (line 600-604):

```rust
fn reload_changed_files(
    mut q: Query<(Entity, &FileView, &mut FileViewport), With<FileReloadRequested>>,
    browsers: NonSend<Browsers>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
```

In the text branch, after `commands.entity(entity).insert(buf);` (line 688), add:

```rust
        manager.change(&fv.path);
```

- [ ] **Step 5: Call `close` + reset `LspOpened` on navigation**

In `crates/vmux_editor/src/plugin.rs`, `on_file_open` (line 511) reassigns `fv.path`. Close the old document and clear the marker. Change the signature to add the manager:

```rust
fn on_file_open(
    trigger: On<BinReceive<FileOpenEvent>>,
    mut views: Query<(&mut FileView, &mut FileViewport, &mut PageMetadata)>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
```

Before `fv.path = path;` (line 529), capture and close the old path:

```rust
    manager.close(&fv.path);
```

And add `LspOpened` to the removed components (line 531-536):

```rust
    commands
        .entity(entity)
        .remove::<FileDir>()
        .remove::<FileBuffer>()
        .remove::<FileImage>()
        .remove::<FileInitialMetaSent>()
        .remove::<crate::lsp::manager::LspOpened>();
```

Make `LspManager` and `LspOpened` reachable: they are `pub` in `manager.rs` ✓. The `lsp` module is `pub mod lsp` (Task 7) ✓.

- [ ] **Step 6: Verify it compiles**

Run: `cargo build -p vmux_editor`
Expected: builds clean (the `build` schedule has only `lsp_open_documents`; the drain system is added in Task 11).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_editor/src/lsp/manager.rs crates/vmux_editor/src/lsp.rs crates/vmux_editor/src/plugin.rs
git commit -m "feat(editor): LspManager document lifecycle wiring"
```

---

## Task 11: Diagnostics drain system → emit to webview

**Files:**
- Modify: `crates/vmux_editor/src/lsp/manager.rs`

- [ ] **Step 1: Add the drain system + test**

In `crates/vmux_editor/src/lsp/manager.rs`, add the drain system (above the `#[cfg(test)]` module, after `build`). It is a normal system reading the `LspOutbox` **resource** (Send+Sync), querying file views, and emitting:

```rust
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
```

Add it to the `build` schedule. Change the `add_systems` line from Task 10:

```rust
    .add_systems(Update, lsp_open_documents);
```

to:

```rust
    .add_systems(Update, (lsp_open_documents, drain_lsp_diagnostics));
```

Add a drain test to the `#[cfg(test)]` module in `manager.rs`:

```rust
    #[test]
    fn drain_empties_outbox() {
        use bevy::prelude::*;
        use crate::lsp::LspOutbox;
        use std::path::PathBuf;

        let mut app = App::new();
        let outbox = LspOutbox::default();
        app.add_plugins(MinimalPlugins).insert_resource(outbox.clone());
        // Drain logic isolated: push one entry, run a minimal drain that mirrors prod.
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
```

Note: the production `drain_lsp_diagnostics` requires the `Browsers` NonSend resource (from `bevy_cef`), which is not present under `MinimalPlugins`; the observable diagnostic conversion is already covered by Task 9's `to_file_diagnostics` tests, and this test verifies the drain-empties contract in isolation (same approach as `vmux_git`'s `drain_empties_outbox`).

- [ ] **Step 2: Build and run all editor tests**

Run: `cargo build -p vmux_editor && cargo test -p vmux_editor`
Expected: all unit + integration tests PASS, crate builds clean.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/lsp/manager.rs
git commit -m "feat(editor): drain LSP diagnostics to webview"
```

---

## Task 12: Frontend pure helpers (`page_model`)

**Files:**
- Modify: `crates/vmux_editor/src/page_model.rs`

- [ ] **Step 1: Write the failing tests**

Add to the top of `crates/vmux_editor/src/page_model.rs` (after the existing `use` line, line 1), extend the import:

```rust
use vmux_core::event::{DiagSeverity, FileDiagnostic, FileDirEntry, StyledSpan};
```

Add these helpers after `span_style` (after line 61). (The text view already renders only the visible `lines()`, so per-line filtering below is enough — no separate window filter needed.)

```rust
/// Highest-precedence severity among diagnostics on a given absolute line.
pub fn line_severity(diags: &[FileDiagnostic], line: u32) -> Option<DiagSeverity> {
    diags
        .iter()
        .filter(|d| d.line == line)
        .map(|d| d.severity)
        .min_by_key(|s| match s {
            DiagSeverity::Error => 0,
            DiagSeverity::Warning => 1,
            DiagSeverity::Info => 2,
            DiagSeverity::Hint => 3,
        })
}

/// CSS class for a severity's color (Tailwind ansi palette).
pub fn severity_color_class(sev: DiagSeverity) -> &'static str {
    match sev {
        DiagSeverity::Error => "text-ansi-1",
        DiagSeverity::Warning => "text-ansi-3",
        DiagSeverity::Info => "text-ansi-4",
        DiagSeverity::Hint => "text-ansi-6",
    }
}

/// Inline style for a diagnostic underline overlay positioned by char columns
/// over a monospace line. `--cw` is the measured cell width (falls back to
/// `1ch`). The box spans the line height (transparent) so it is an easy hover
/// target; only its colored bottom border is visible, reading as an underline.
/// (A wavy "squiggle" texture is a cosmetic follow-up.)
pub fn squiggle_style(start_col: u32, end_col: u32, color_rgb: &str) -> String {
    let width = end_col.saturating_sub(start_col).max(1);
    format!(
        "position:absolute;left:calc(var(--cw,1ch) * {start});\
         width:calc(var(--cw,1ch) * {width});bottom:0;height:1.1em;\
         border-bottom:2px solid {color};pointer-events:auto;",
        start = start_col,
        width = width,
        color = color_rgb,
    )
}
```

Add tests to the `#[cfg(test)] mod tests` block (after line 86):

```rust
    #[test]
    fn line_severity_takes_most_severe() {
        let mk = |line, sev| FileDiagnostic {
            line,
            start_col: 0,
            end_col: 1,
            severity: sev,
            message: String::new(),
            source: None,
        };
        let v = vec![mk(3, DiagSeverity::Warning), mk(3, DiagSeverity::Error)];
        assert_eq!(line_severity(&v, 3), Some(DiagSeverity::Error));
        assert_eq!(line_severity(&v, 4), None);
    }

    #[test]
    fn squiggle_style_positions_by_columns() {
        let s = squiggle_style(2, 6, "rgb(255,0,0)");
        assert!(s.contains("left:calc(var(--cw,1ch) * 2)"));
        assert!(s.contains("width:calc(var(--cw,1ch) * 4)"));
    }
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vmux_editor --lib page_model`
Expected: existing + 2 new tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/page_model.rs
git commit -m "feat(editor): diagnostics page-model helpers"
```

---

## Task 13: Frontend rendering (gutter dot + squiggle + soft-glass hover card)

This task changes wasm UI; it is verified by `cargo build` for wasm-shape correctness and by manual runtime testing (no DOM unit tests in this crate).

**Files:**
- Modify: `crates/vmux_editor/src/page.rs`

- [ ] **Step 1: Import the helpers**

In `crates/vmux_editor/src/page.rs`, extend the `page_model` import (line 5):

```rust
use crate::page_model::{
    clamp_selection, gutter_width, image_mime, line_severity, severity_color_class, span_style,
    squiggle_style,
};
```

- [ ] **Step 2: Add diagnostics state + listener**

Inside `pub fn Page()`, add a signal near the other text signals (after `let mut lines = use_signal(...)`, line 333):

```rust
    let mut diagnostics = use_signal(Vec::<FileDiagnostic>::new);
    let mut hover_diag = use_signal(|| Option::<FileDiagnostic>::None);
```

Add a listener near the other `use_bin_event_listener` calls (after the `_vp` listener, line 374):

```rust
    let _diag = use_bin_event_listener::<FileDiagnosticsEvent, _>(
        FILE_DIAGNOSTICS_EVENT,
        move |d| {
            diagnostics.set(d.diagnostics);
        },
    );
```

Clear diagnostics when a new file's metadata arrives. In the `_meta` listener (line 357), after `path.set(m.path);` add:

```rust
        diagnostics.set(Vec::new());
        hover_diag.set(None);
```

- [ ] **Step 3: Render gutter dot + squiggle in the text view**

In `page.rs`, replace the text-view line loop (lines 745-758, the `for line in lines().iter()` block inside the `else` of `show_diff()`). The current block is:

```rust
                                for line in lines().iter() {
                                    div { key: "{line.line_no}", class: "group flex hover:bg-white/[0.035]",
                                        span {
                                            class: "sticky left-0 z-[1] shrink-0 select-none bg-background pl-4 pr-5 text-right tabular-nums opacity-40 group-hover:opacity-90",
                                            style: "min-width:calc(var(--cw, 1ch) * {gw} + 2.25rem);",
                                            "{line.line_no + 1}"
                                        }
                                        span { class: "whitespace-pre pr-8",
                                            for (i, s) in line.spans.iter().enumerate() {
                                                span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                                            }
                                        }
                                    }
                                }
```

Replace it with (adds a gutter dot, a `relative` content wrapper, and per-line squiggle overlays + hover-card triggers):

```rust
                                for line in lines().iter() {
                                    {
                                        let ln = line.line_no;
                                        let diags = diagnostics();
                                        let sev = line_severity(&diags, ln);
                                        let line_diags: Vec<FileDiagnostic> = diags
                                            .iter()
                                            .filter(|d| d.line == ln)
                                            .cloned()
                                            .collect();
                                        rsx! {
                                            div { key: "{ln}", class: "group flex hover:bg-white/[0.035]",
                                                span {
                                                    class: "sticky left-0 z-[1] flex shrink-0 select-none items-center justify-end gap-1 bg-background pl-4 pr-5 text-right tabular-nums opacity-40 group-hover:opacity-90",
                                                    style: "min-width:calc(var(--cw, 1ch) * {gw} + 2.25rem);",
                                                    if let Some(s) = sev {
                                                        span { class: "{severity_color_class(s)}", "●" }
                                                    }
                                                    "{ln + 1}"
                                                }
                                                span { class: "relative whitespace-pre pr-8",
                                                    for (i, s) in line.spans.iter().enumerate() {
                                                        span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                                                    }
                                                    for (di, d) in line_diags.iter().enumerate() {
                                                        {
                                                            let color = match d.severity {
                                                                DiagSeverity::Error => "rgb(239,68,68)",
                                                                DiagSeverity::Warning => "rgb(245,158,11)",
                                                                DiagSeverity::Info => "rgb(56,189,248)",
                                                                DiagSeverity::Hint => "rgb(34,211,238)",
                                                            };
                                                            let dc = d.clone();
                                                            rsx! {
                                                                span {
                                                                    key: "d{di}",
                                                                    style: squiggle_style(d.start_col, d.end_col, color),
                                                                    onmouseenter: move |_| hover_diag.set(Some(dc.clone())),
                                                                    onmouseleave: move |_| hover_diag.set(None),
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
```

- [ ] **Step 4: Render the soft-glass hover card**

Add the hover card as an overlay inside the main container. Insert it right after the `match mode() { … }` block closes (after line 763, before `GitFooter`):

```rust
            {
                hover_diag().map(|d| rsx! {
                    div {
                        class: "pointer-events-none absolute right-4 bottom-12 z-50 max-w-md rounded-xl bg-white/[0.04] px-3 py-2 text-xs text-foreground/90 ring-1 ring-inset ring-white/10 backdrop-blur-2xl shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                        div { class: "flex items-center gap-2",
                            span { class: "{severity_color_class(d.severity)}", "●" }
                            span { class: "whitespace-pre-wrap", "{d.message}" }
                        }
                        if let Some(src) = d.source.as_ref() {
                            div { class: "mt-1 opacity-50", "{src}" }
                        }
                    }
                })
            }
```

- [ ] **Step 5: Verify wasm build shape**

Run: `cargo build -p vmux_editor --target wasm32-unknown-unknown`
Expected: builds clean. (If the wasm target is not installed, run `rustup target add wasm32-unknown-unknown` first.)

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_editor/src/page.rs
git commit -m "feat(editor): render LSP diagnostics (gutter, squiggle, hover card)"
```

---

## Task 14: Config override (`settings.ron` `editor.lsp.servers`)

Lets users add or override servers per extension. Embedded registry remains the fallback (absent key → built-in; never auto-seeded).

**Files:**
- Modify: `crates/vmux_setting/src/plugin/runtime.rs`
- Modify: `crates/vmux_editor/src/lsp/registry.rs`
- Modify: `crates/vmux_editor/src/lsp/manager.rs`

- [ ] **Step 1: Add the settings types**

In `crates/vmux_setting/src/plugin/runtime.rs`, add an `editor` field to `AppSettings` (after the `recording` field, line 34):

```rust
    #[serde(default)]
    pub editor: EditorSettings,
```

Add the structs (near `RecordingSettings`, after line 43):

```rust
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct EditorSettings {
    #[serde(default)]
    pub lsp: LspSettings,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LspSettings {
    /// Extension -> server override. Absent extension falls back to the built-in
    /// registry; this map is never auto-seeded.
    #[serde(default)]
    pub servers: std::collections::BTreeMap<String, LspServerOverride>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LspServerOverride {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub language_id: String,
    #[serde(default)]
    pub root_markers: Vec<String>,
}
```

Re-export them from `crates/vmux_setting/src/lib.rs`. Find the re-export line (line 28 lists `AppSettings, BrowserSettings, ...`) and add `EditorSettings, LspSettings, LspServerOverride` to that list.

- [ ] **Step 2: Write the failing registry test**

In `crates/vmux_editor/src/lsp/registry.rs`, add an override-aware resolver. First add a small mirror struct so the registry does not depend on `vmux_setting` types directly (keeps the registry pure/testable):

```rust
/// Resolve a server spec for `ext`, preferring `overrides` over the built-in
/// registry. `overrides` maps extension -> (command, args, language_id, markers).
pub fn resolve_spec(
    ext: &str,
    overrides: &std::collections::BTreeMap<String, ServerSpec>,
) -> Option<ServerSpec> {
    overrides.get(ext).cloned().or_else(|| builtin_spec(ext))
}
```

Add the test:

```rust
    #[test]
    fn override_takes_precedence_over_builtin() {
        let mut ov = std::collections::BTreeMap::new();
        ov.insert(
            "rs".to_string(),
            ServerSpec {
                command: "my-ra".into(),
                args: vec![],
                language_id: "rust".into(),
                root_markers: vec![".git".into()],
            },
        );
        assert_eq!(resolve_spec("rs", &ov).unwrap().command, "my-ra");
        assert_eq!(resolve_spec("go", &ov).unwrap().command, "gopls");
        assert!(resolve_spec("zzz", &ov).is_none());
    }
```

- [ ] **Step 3: Run the registry test**

Run: `cargo test -p vmux_editor --lib lsp::registry::tests::override_takes_precedence_over_builtin`
Expected: PASS.

- [ ] **Step 4: Thread overrides through the manager**

In `crates/vmux_editor/src/lsp/manager.rs`, change `LspManager::open` to accept the overrides map and use `resolve_spec`. Update the signature:

```rust
    pub fn open(&mut self, path: &Path, overrides: &std::collections::BTreeMap<String, crate::lsp::registry::ServerSpec>) {
```

Replace the `let Some(spec) = builtin_spec(ext) else {` line with:

```rust
        let Some(spec) = crate::lsp::registry::resolve_spec(ext, overrides) else {
```

`builtin_spec` is now only called inside `resolve_spec`, so drop it from the manager's import to avoid an unused-import warning. Change the Task 10 import line:

```rust
use crate::lsp::registry::{builtin_spec, executable_on_path, workspace_root};
```

to:

```rust
use crate::lsp::registry::{executable_on_path, workspace_root};
```

Build the overrides map in `lsp_open_documents` from `AppSettings` and pass it in. Update that system:

```rust
fn lsp_open_documents(
    q: Query<(Entity, &FileView, &FileBuffer), Without<LspOpened>>,
    settings: Res<vmux_setting::AppSettings>,
    mut manager: NonSendMut<LspManager>,
    mut commands: Commands,
) {
    let overrides: std::collections::BTreeMap<String, crate::lsp::registry::ServerSpec> = settings
        .editor
        .lsp
        .servers
        .iter()
        .map(|(ext, o)| {
            (
                ext.clone(),
                crate::lsp::registry::ServerSpec {
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
```

- [ ] **Step 5: Build + full test run**

Run: `cargo build -p vmux_editor && cargo test -p vmux_editor && cargo test -p vmux_setting`
Expected: all PASS, clean build.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_setting/src/plugin/runtime.rs crates/vmux_setting/src/lib.rs crates/vmux_editor/src/lsp/registry.rs crates/vmux_editor/src/lsp/manager.rs
git commit -m "feat(editor): settings.ron LSP server overrides"
```

---

## Task 15: Final checks

- [ ] **Step 1: Format**

Run: `cargo fmt -p vmux_core -p vmux_editor -p vmux_setting`
Then restore any vendored patch reformatting (the known `cargo fmt` gotcha):

```bash
git checkout -- patches/ 2>/dev/null || true
```

- [ ] **Step 2: Clippy**

Run: `cargo clippy -p vmux_editor -p vmux_core -p vmux_setting --all-targets`
Expected: no warnings. Fix any that appear.

- [ ] **Step 3: Full targeted test pass**

Run: `cargo test -p vmux_core -p vmux_editor -p vmux_setting`
Expected: all PASS, including `lsp_integration`.

- [ ] **Step 4: Manual runtime verification (user)**

Build and run vmux. Open a Rust file (in a Cargo project, with `rust-analyzer` on PATH) that contains a real error (e.g. reference an undefined variable). Confirm:
- a red dot appears in the gutter on the error line,
- a red squiggle underlines the error span,
- hovering the squiggle shows the soft-glass card with the message and `source`.
Then introduce an error in another language whose server is installed (e.g. `pyright` for `.py`) and confirm the same. Edit the file externally and confirm diagnostics refresh.

- [ ] **Step 5: Commit any fmt/clippy fixes**

```bash
git add -A
git commit -m "chore(editor): fmt + clippy for LSP milestone 1"
```

- [ ] **Step 6: Delete this plan file** (per AGENTS.md, once fully implemented)

```bash
git rm docs/plans/2026-06-24-editor-lsp.md
git commit -m "chore: remove completed LSP milestone 1 plan"
```

---

## Build/CI notes

- CEF builds are large; keep a warm target dir (do **not** share `CARGO_TARGET_DIR` across worktrees — CEF cmake pins absolute paths).
- `crates/vmux_server/build.rs` already tracks `../vmux_editor/src`, so new files under that dir trigger wasm rebuilds; no build.rs change needed.
- `cargo fmt` reformats vendored `patches/` crates — `git checkout -- patches/` before committing fmt changes.
- The `vmux_mock_lsp` bin is a dependency-only target; the wasm bundle builds `vmux_editor`'s lib (not its bins), so it does not affect the page build.

## Deferred to later milestones (not in this plan)

- Hover, go-to-definition, document symbols (request-based, read-only) — milestone 2.
- Editing surface (cursor/edits/save/version tracking) — milestone 3.
- Completion, signature help, rename, formatting — milestone 4.
- Server auto-install, multi-root workspaces, semantic tokens, idle server shutdown.
