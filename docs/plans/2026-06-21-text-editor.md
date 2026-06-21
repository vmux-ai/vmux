# Text Editor — `files://` Read-Only IDE Viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render any file in a VSCode/vim-style page (path header, line-number gutter, monospace, syntax highlighting), addressed by a real `files://` custom CEF scheme and streamed with terminal-style server-owned viewport virtualization. Read-only.

**Architecture:** New crate `vmux_editor` mirrors `vmux_terminal` (native backend plugin + Dioxus/WASM `page::Page`). The backend reads a file once, highlights it with `syntect`, and streams only the visible line window over the rkyv bin bridge as the frontend reports its viewport height and scroll offset. A new `files://` CEF scheme serves the shared SPA shell (with `<base href="vmux://files/">` injected so assets keep loading via the existing embedded scheme); routing switches from `location.host` to `location.protocol === "files:"`, and the file path is read from `location.pathname`.

**Tech Stack:** Rust, Bevy 0.19-rc.2, bevy_cef (CEF 148), Dioxus 0.7 (WASM), rkyv 0.8, syntect 5 (pure-Rust fancy-regex), patched `bevy_cef_core` / `bevy_cef`.

**Spec:** `docs/specs/2026-06-21-text-editor-design.md`

**Reference files to copy patterns from (read before starting):**
- `crates/vmux_terminal/src/plugin.rs` — backend plugin, page-open handler, bundle, bin emit, observers
- `crates/vmux_terminal/src/page.rs` — frontend page, ResizeObserver, bin listeners, per-row signals
- `crates/vmux_terminal/src/render_model.rs` — span→class/style helpers + their unit tests
- `crates/vmux_core/src/event.rs` — event const + rkyv struct pattern
- `crates/vmux_core/src/page_open.rs` — `PageOpenTask`, `PageOpenSet`, `PageOpenHandled`
- `crates/vmux_terminal/Cargo.toml` — native/wasm split

---

## File Structure

**Create:**
- `crates/vmux_editor/Cargo.toml` — native+wasm split crate manifest
- `crates/vmux_editor/src/lib.rs` — `pub mod` wiring; re-exports `EditorPlugin` (native) and `page` (wasm)
- `crates/vmux_editor/src/highlight.rs` — syntect highlighting + language detection (native)
- `crates/vmux_editor/src/viewport.rs` — pure window-slice / clamp / rows math (native, shared logic)
- `crates/vmux_editor/src/plugin.rs` — `EditorPlugin`, `FileView`, page-open handler, streaming systems (native)
- `crates/vmux_editor/src/page.rs` — Dioxus `Page` (wasm)
- `crates/vmux_editor/src/page_model.rs` — pure frontend helpers (gutter width, span style) + tests (wasm-agnostic)

**Modify:**
- `crates/vmux_core/src/event.rs` — add file event consts + rkyv structs
- `Cargo.toml` (root) — add `syntect` to `[workspace.dependencies]`
- `crates/vmux_server/Cargo.toml` — add optional `vmux_editor` dep + `webview` feature entry
- `crates/vmux_server/src/lib.rs` — `current_host()` protocol branch + `render_files` macro line
- `crates/vmux_desktop/src/lib.rs` — add `EditorPlugin` to the native plugin set
- `crates/vmux_layout/src/snapshot.rs` — `build_stack` kind for `files:`
- `crates/vmux_layout/src/page.rs` — `StackIcon` document icon + `format_address` for `files:`
- `crates/vmux_layout/src/command_bar/page.rs` — treat `files:` input as a URL, not a search
- `patches/bevy_cef_core-0.5.2/src/util.rs` — `inject_base_href` helper + `embedded_page_host` note
- `patches/bevy_cef_core-0.5.2/src/browser_process/app.rs` — register `files` scheme
- `patches/bevy_cef_core-0.5.2/src/render_process/app.rs` — register `files` scheme
- `patches/bevy_cef_core-0.5.2/src/browser_process/localhost.rs` — serve files:// document (index.html + base href)
- `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs` — register the files scheme handler factory

**Naming (fixed — keep identical across tasks):**
- Crate `vmux_editor`; scheme string `"files"`; URL prefix `"files://"`
- Components: `FileView { path: PathBuf }`, `FileBuffer { language: String, lines: Vec<FileLine> }`, `FileViewport { top_line: u32, rows: u16 }`, `FileInitialMetaSent`
- Event consts: `FILE_META_EVENT`, `FILE_VIEWPORT_EVENT`, `FILE_ERROR_EVENT`, `FILE_RESIZE_EVENT`, `FILE_SCROLL_EVENT`
- Structs: `StyledSpan`, `FileLine`, `FileMetaEvent`, `FileViewportPatch`, `FileErrorEvent`, `FileResizeEvent`, `FileScrollEvent`
- Const `FILE_VIEW_MAX_BYTES: u64 = 5 * 1024 * 1024`

---

## Task 1: Protocol types (events + structs)

**Files:**
- Modify: `crates/vmux_core/src/event.rs`

- [ ] **Step 1: Write the failing test**

Append to the bottom of `crates/vmux_core/src/event.rs` (inside a new test module):

```rust
#[cfg(test)]
mod file_event_tests {
    use super::*;

    #[test]
    fn file_viewport_patch_rkyv_roundtrip() {
        let patch = FileViewportPatch {
            first_line: 100,
            total_lines: 5000,
            lines: vec![FileLine {
                line_no: 100,
                spans: vec![StyledSpan {
                    text: "fn main() {".into(),
                    fg: [220, 220, 170],
                    bold: false,
                    italic: false,
                }],
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&patch).expect("ser");
        let decoded =
            rkyv::from_bytes::<FileViewportPatch, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded.first_line, 100);
        assert_eq!(decoded.total_lines, 5000);
        assert_eq!(decoded.lines[0].line_no, 100);
        assert_eq!(decoded.lines[0].spans[0].text, "fn main() {");
        assert_eq!(decoded.lines[0].spans[0].fg, [220, 220, 170]);
    }

    #[test]
    fn file_scroll_and_resize_roundtrip() {
        let s = FileScrollEvent { top_line: 42 };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&s).unwrap();
        assert_eq!(
            rkyv::from_bytes::<FileScrollEvent, rkyv::rancor::Error>(&b)
                .unwrap()
                .top_line,
            42
        );
        let r = FileResizeEvent {
            char_height: 16.0,
            viewport_height: 480.0,
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let d = rkyv::from_bytes::<FileResizeEvent, rkyv::rancor::Error>(&b).unwrap();
        assert_eq!(d.char_height, 16.0);
        assert_eq!(d.viewport_height, 480.0);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_core file_event_tests 2>&1 | tail -20`
Expected: FAIL — `cannot find type FileViewportPatch` etc.

- [ ] **Step 3: Add the consts + structs**

Add the consts near the other event consts (after line 13, before `TERMINAL_PAGE_URL`):

```rust
pub const FILE_META_EVENT: &str = "file_meta";
pub const FILE_VIEWPORT_EVENT: &str = "file_viewport";
pub const FILE_ERROR_EVENT: &str = "file_error";
pub const FILE_RESIZE_EVENT: &str = "file_resize";
pub const FILE_SCROLL_EVENT: &str = "file_scroll";
pub const FILE_PAGE_SCHEME: &str = "files";
```

Add the structs (anywhere in the file alongside the other event structs; use the exact same derive set as `TermResizeEvent`):

```rust
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct StyledSpan {
    pub text: String,
    pub fg: [u8; 3],
    pub bold: bool,
    pub italic: bool,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct FileLine {
    pub line_no: u32,
    pub spans: Vec<StyledSpan>,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct FileMetaEvent {
    pub path: String,
    pub language: String,
    pub total_lines: u32,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct FileViewportPatch {
    pub first_line: u32,
    pub total_lines: u32,
    pub lines: Vec<FileLine>,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct FileErrorEvent {
    pub message: String,
}

#[derive(
    Debug, Clone, PartialEq, Default, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct FileResizeEvent {
    pub char_height: f32,
    pub viewport_height: f32,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct FileScrollEvent {
    pub top_line: u32,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_core file_event_tests 2>&1 | tail -20`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/event.rs
git commit -m "feat(editor): add files viewer bin-event protocol types"
```

---

## Task 2: Crate skeleton (`vmux_editor`) + viewport math

**Files:**
- Create: `crates/vmux_editor/Cargo.toml`
- Create: `crates/vmux_editor/src/lib.rs`
- Create: `crates/vmux_editor/src/viewport.rs`

- [ ] **Step 1: Create the manifest**

`crates/vmux_editor/Cargo.toml` (mirror `vmux_terminal`, drop unused deps, add `syntect` native-only):

```toml
[package]
name = "vmux_editor"
description = "Bevy + CEF + Dioxus file viewer webview"
version.workspace = true
edition.workspace = true
publish = false

[features]
default = []
web = []

[lib]
path = "src/lib.rs"

[dependencies]
serde = { workspace = true }
rkyv = { workspace = true }
vmux_core = { path = "../vmux_core" }

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
bevy = { workspace = true }
bevy_cef = { workspace = true }
bevy_ecs = { workspace = true }
bevy_reflect = { workspace = true }
tracing = "0.1"
url = { workspace = true }
syntect = { workspace = true }
vmux_command = { path = "../vmux_command" }
vmux_layout = { path = "../vmux_layout" }
vmux_setting = { path = "../vmux_setting" }
vmux_space = { path = "../vmux_space" }

[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]
tempfile = "3"

[target.'cfg(target_arch = "wasm32")'.dependencies]
dioxus = { workspace = true }
js-sys = "0.3"
unicode-width = "0.2"
vmux_ui = { path = "../vmux_ui", default-features = false }
wasm-bindgen = { workspace = true }
web-sys = { version = "0.3", features = [
    "Window", "Document", "Element", "HtmlElement", "Node",
    "DomRect", "CssStyleDeclaration",
    "ResizeObserver", "ResizeObserverEntry",
    "WheelEvent", "KeyboardEvent", "Location",
] }
```

- [ ] **Step 2: Add `syntect` to root workspace deps**

In root `Cargo.toml`, under `[workspace.dependencies]` (after the `regex = "1"` line), add:

```toml
syntect = { version = "5", default-features = false, features = ["default-fancy"] }
```

(`default-fancy` = pure-Rust fancy-regex engine + bundled syntaxes/themes; avoids the oniguruma C dependency so Linux CI builds cleanly.)

- [ ] **Step 3: Write the failing viewport test**

`crates/vmux_editor/src/viewport.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_clamps_at_end() {
        // 10 lines, 4 rows, scrolled near the end
        assert_eq!(window_range(10, 8, 4), (6, 10));
    }

    #[test]
    fn window_from_top() {
        assert_eq!(window_range(10, 0, 4), (0, 4));
    }

    #[test]
    fn window_smaller_than_viewport() {
        assert_eq!(window_range(3, 0, 10), (0, 3));
    }

    #[test]
    fn window_empty_file() {
        assert_eq!(window_range(0, 5, 10), (0, 0));
    }

    #[test]
    fn clamp_top_caps_at_max_scroll() {
        assert_eq!(clamp_top_line(99, 10, 4), 6);
        assert_eq!(clamp_top_line(2, 10, 4), 2);
        assert_eq!(clamp_top_line(5, 3, 10), 0);
    }

    #[test]
    fn rows_from_viewport_floors() {
        assert_eq!(rows_from_viewport(16.0, 480.0), 30);
        assert_eq!(rows_from_viewport(0.0, 480.0), 0);
        assert_eq!(rows_from_viewport(16.0, 8.0), 0);
    }
}
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test -p vmux_editor viewport 2>&1 | tail -20`
Expected: FAIL — `cannot find function window_range`.

- [ ] **Step 5: Implement viewport math**

Prepend to `crates/vmux_editor/src/viewport.rs`:

```rust
/// Largest valid `top_line` so a full viewport stays in range.
pub fn clamp_top_line(top_line: u32, total_lines: u32, rows: u16) -> u32 {
    let max_top = total_lines.saturating_sub(rows as u32);
    top_line.min(max_top)
}

/// Visible line range `[first, end)` after clamping the scroll offset.
pub fn window_range(total_lines: u32, top_line: u32, rows: u16) -> (u32, u32) {
    let first = clamp_top_line(top_line, total_lines, rows);
    let end = first.saturating_add(rows as u32).min(total_lines);
    (first, end)
}

/// Whole rows that fit in `viewport_height` at `char_height` px per row.
pub fn rows_from_viewport(char_height: f32, viewport_height: f32) -> u16 {
    if char_height <= 0.0 || viewport_height <= 0.0 {
        return 0;
    }
    (viewport_height / char_height).floor() as u16
}
```

- [ ] **Step 6: Create `lib.rs` exposing the module**

`crates/vmux_editor/src/lib.rs`:

```rust
pub mod viewport;

#[cfg(not(target_arch = "wasm32"))]
pub mod highlight;

#[cfg(not(target_arch = "wasm32"))]
mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::{EditorPlugin, FileView};

#[cfg(all(target_arch = "wasm32", feature = "web"))]
pub mod page;
#[cfg(any(target_arch = "wasm32", test))]
pub mod page_model;
```

Note: `highlight`, `plugin`, `page`, `page_model` don't exist yet — comment out the lines for modules not yet created so the crate compiles after each task, OR implement tasks in order and uncomment as you go. For Task 2, keep only `pub mod viewport;` and add the rest in their tasks.

For now `lib.rs` is just:

```rust
pub mod viewport;
```

- [ ] **Step 7: Run tests + verify the crate builds**

Run: `cargo test -p vmux_editor viewport 2>&1 | tail -20`
Expected: PASS (6 tests).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/vmux_editor/Cargo.toml crates/vmux_editor/src/lib.rs crates/vmux_editor/src/viewport.rs
git commit -m "feat(editor): scaffold vmux_editor crate with viewport math"
```

---

## Task 3: Syntax highlighting + language detection

**Files:**
- Create: `crates/vmux_editor/src/highlight.rs`
- Modify: `crates/vmux_editor/src/lib.rs`

- [ ] **Step 1: Write the failing test**

`crates/vmux_editor/src/highlight.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_keywords_distinctly() {
        let hl = Highlighter::new();
        let out = hl.highlight("fn main() {}\n", std::path::Path::new("a.rs"));
        assert_eq!(out.language, "Rust");
        assert_eq!(out.lines.len(), 1);
        assert_eq!(out.lines[0].line_no, 0);
        let joined: String = out.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined.trim_end(), "fn main() {}");
        // More than one color run => highlighting actually happened.
        let distinct: std::collections::HashSet<_> =
            out.lines[0].spans.iter().map(|s| s.fg).collect();
        assert!(distinct.len() > 1, "expected multiple colors, got {distinct:?}");
    }

    #[test]
    fn unknown_extension_is_plaintext_single_span() {
        let hl = Highlighter::new();
        let out = hl.highlight("just text\n", std::path::Path::new("notes.xyzzy"));
        assert_eq!(out.language, "Plain Text");
        assert_eq!(out.lines.len(), 1);
    }

    #[test]
    fn line_count_matches_input() {
        let hl = Highlighter::new();
        let out = hl.highlight("a\nb\nc\n", std::path::Path::new("a.txt"));
        assert_eq!(out.lines.len(), 3);
        assert_eq!(out.lines[2].line_no, 2);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

First add `pub mod highlight;` to `lib.rs` (native cfg), then:
Run: `cargo test -p vmux_editor highlight 2>&1 | tail -20`
Expected: FAIL — `cannot find type Highlighter`.

- [ ] **Step 3: Implement the highlighter**

Prepend to `crates/vmux_editor/src/highlight.rs`:

```rust
use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use vmux_core::event::{FileLine, StyledSpan};

pub struct HighlightedFile {
    pub language: String,
    pub lines: Vec<FileLine>,
}

pub struct Highlighter {
    syntaxes: SyntaxSet,
    themes: ThemeSet,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            themes: ThemeSet::load_defaults(),
        }
    }

    pub fn highlight(&self, content: &str, path: &Path) -> HighlightedFile {
        let syntax = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|ext| self.syntaxes.find_syntax_by_extension(ext))
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let theme = &self.themes.themes["base16-ocean.dark"];
        let mut h = HighlightLines::new(syntax, theme);

        let mut lines = Vec::new();
        for (idx, line) in LinesWithEndings::from(content).enumerate() {
            let ranges: Vec<(Style, &str)> =
                h.highlight_line(line, &self.syntaxes).unwrap_or_default();
            let spans = ranges
                .into_iter()
                .map(|(style, text)| to_styled_span(style, text))
                .filter(|s| !s.text.is_empty())
                .collect();
            lines.push(FileLine {
                line_no: idx as u32,
                spans,
            });
        }
        // A file ending without a trailing newline still has its last line above;
        // a file ending WITH a newline should not produce a phantom empty line.
        HighlightedFile {
            language: syntax.name.clone(),
            lines,
        }
    }
}

fn to_styled_span(style: Style, text: &str) -> StyledSpan {
    StyledSpan {
        text: text.trim_end_matches(['\n', '\r']).to_string(),
        fg: [style.foreground.r, style.foreground.g, style.foreground.b],
        bold: style.font_style.contains(FontStyle::BOLD),
        italic: style.font_style.contains(FontStyle::ITALIC),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_editor highlight 2>&1 | tail -20`
Expected: PASS (3 tests). If `base16-ocean.dark` is absent in this syntect version, list `hl.themes.themes.keys()` in a scratch test and pick a bundled dark theme.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/highlight.rs crates/vmux_editor/src/lib.rs
git commit -m "feat(editor): syntect highlighting + language detection"
```

---

## Task 4: File reading guards (load → buffer or error)

**Files:**
- Modify: `crates/vmux_editor/src/highlight.rs` (add a `load_file` boundary fn)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `highlight.rs`:

```rust
    #[test]
    fn load_rejects_missing_file() {
        let hl = Highlighter::new();
        let err = hl
            .load_file(std::path::Path::new("/no/such/file.rs"))
            .unwrap_err();
        assert!(err.contains("/no/such/file.rs"), "got: {err}");
    }

    #[test]
    fn load_rejects_directory() {
        let hl = Highlighter::new();
        let dir = std::env::temp_dir();
        let err = hl.load_file(&dir).unwrap_err();
        assert!(err.to_lowercase().contains("not a file"), "got: {err}");
    }

    #[test]
    fn load_reads_and_highlights() {
        let hl = Highlighter::new();
        let mut p = std::env::temp_dir();
        p.push(format!("vmux-editor-{}.rs", std::process::id()));
        std::fs::write(&p, "fn x() {}\n").unwrap();
        let out = hl.load_file(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(out.language, "Rust");
        assert_eq!(out.lines.len(), 1);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_editor load_ 2>&1 | tail -20`
Expected: FAIL — `no method named load_file`.

- [ ] **Step 3: Implement `load_file`**

Add to `impl Highlighter` in `highlight.rs`, and a const at top of file:

```rust
pub const FILE_VIEW_MAX_BYTES: u64 = 5 * 1024 * 1024;
```

```rust
    /// Boundary: read a real file from disk, with guards, then highlight it.
    /// Returns a user-facing error string on failure.
    pub fn load_file(&self, path: &Path) -> Result<HighlightedFile, String> {
        let meta = std::fs::metadata(path)
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        if !meta.is_file() {
            return Err(format!("not a file: {}", path.display()));
        }
        if meta.len() > FILE_VIEW_MAX_BYTES {
            return Err(format!(
                "file too large ({} bytes, max {})",
                meta.len(),
                FILE_VIEW_MAX_BYTES
            ));
        }
        let bytes = std::fs::read(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let content = String::from_utf8(bytes)
            .map_err(|_| format!("not a UTF-8 text file: {}", path.display()))?;
        Ok(self.highlight(&content, path))
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_editor load_ 2>&1 | tail -20`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/highlight.rs
git commit -m "feat(editor): file load boundary with size/type/utf8 guards"
```

---

## Task 5: URL → path parsing

**Files:**
- Create: helper in `crates/vmux_editor/src/plugin.rs` (start the file with just this fn + test)
- Modify: `crates/vmux_editor/src/lib.rs` (add `mod plugin;` native)

- [ ] **Step 1: Write the failing test**

Start `crates/vmux_editor/src/plugin.rs` with:

```rust
use std::path::PathBuf;

/// Parse the absolute filesystem path out of a `files://` URL.
/// `files:///Users/me/a%20b.rs` -> `/Users/me/a b.rs`.
fn path_from_files_url(url: &str) -> Option<PathBuf> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "files" {
        return None;
    }
    let decoded = percent_decode(parsed.path());
    if decoded.is_empty() {
        return None;
    }
    Some(PathBuf::from(decoded))
}

fn percent_decode(s: &str) -> String {
    url::Url::parse(&format!("files://{s}"))
        .ok()
        .map(|u| u.path().to_string())
        .filter(|p| !p.contains('%'))
        .unwrap_or_else(|| s.to_string())
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn parses_simple_path() {
        assert_eq!(
            path_from_files_url("files:///Users/me/src/main.rs"),
            Some(PathBuf::from("/Users/me/src/main.rs"))
        );
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(
            path_from_files_url("files:///Users/me/a%20b.rs"),
            Some(PathBuf::from("/Users/me/a b.rs"))
        );
    }

    #[test]
    fn rejects_non_files_scheme() {
        assert_eq!(path_from_files_url("vmux://terminal/"), None);
    }

    #[test]
    fn rejects_empty_path() {
        assert_eq!(path_from_files_url("files:///"), Some(PathBuf::from("/")));
    }
}
```

Note: the `url` crate already percent-decodes `parsed.path()` for standard schemes. Verify in Step 2; if `parsed.path()` is already decoded, delete `percent_decode` and use `parsed.path().to_string()` directly. Keep whichever makes both tests pass.

- [ ] **Step 2: Run test to verify it fails, then simplify**

Add `#[cfg(not(target_arch = "wasm32"))] mod plugin;` to `lib.rs`.
Run: `cargo test -p vmux_editor url_tests 2>&1 | tail -30`
Expected: FAIL initially (module/needs `url`). Get it to PASS; **prefer** the simplest version — if `url::Url::path()` returns already-decoded text, the final `path_from_files_url` is:

```rust
fn path_from_files_url(url: &str) -> Option<PathBuf> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "files" {
        return None;
    }
    let path = parsed.path();
    (!path.is_empty()).then(|| PathBuf::from(path))
}
```

Run again until PASS (4 tests). Delete the unused helper.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs crates/vmux_editor/src/lib.rs
git commit -m "feat(editor): parse filesystem path from files:// url"
```

---

## Task 6: `EditorPlugin` + page-open handler (spawn webview + FileView)

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs`

Read first: `crates/vmux_terminal/src/plugin.rs:239-300` (plugin build), `:391-500` (page-open + bundle), `:574-639` (`new_terminal_bundle_with_cwd`).

- [ ] **Step 1: Write the failing test**

Add to `plugin.rs`:

```rust
#[cfg(test)]
mod page_open_tests {
    use super::*;
    use bevy::prelude::*;
    use vmux_core::page_open::{PageOpenHandled, PageOpenTask, PageOpenId};

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<bevy_cef::prelude::WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_file_page_open);
        app
    }

    #[test]
    fn claims_files_url_and_attaches_fileview() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "files:///etc/hostname".to_string(),
                request_id: None,
            })
            .id();
        app.update();
        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        // A child FileView webview is parented to the stack.
        let children = app.world().get::<Children>(stack).expect("stack children");
        let fv = children
            .iter()
            .find(|c| app.world().get::<FileView>(*c).is_some())
            .expect("FileView child");
        assert_eq!(
            app.world().get::<FileView>(fv).unwrap().path,
            std::path::PathBuf::from("/etc/hostname")
        );
    }

    #[test]
    fn ignores_non_files_url() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://terminal/".to_string(),
                request_id: None,
            })
            .id();
        app.update();
        assert!(app.world().get::<PageOpenHandled>(task).is_none());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_editor page_open_tests 2>&1 | tail -30`
Expected: FAIL — `handle_file_page_open` / `FileView` not found.

- [ ] **Step 3: Implement components, bundle, handler, plugin**

Add to `plugin.rs` (imports at top; mirror terminal's bundle shape):

```rust
use bevy::prelude::*;
use bevy::ui::{PositionType, Val};
use bevy_cef::prelude::*;
use vmux_core::event::*;
use vmux_core::page_open::{PageOpenHandled, PageOpenSet, PageOpenTask};
use vmux_layout::Browser;
use vmux_layout::event::TERMINAL_CEF_BG_COLOR;
use vmux_layout::PageMetadata;

#[derive(Component, Clone, Debug)]
pub struct FileView {
    pub path: PathBuf,
}

#[derive(Component, Clone, Debug)]
pub struct FileBuffer {
    pub language: String,
    pub lines: Vec<FileLine>,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct FileViewport {
    pub top_line: u32,
    pub rows: u16,
}

#[derive(Component)]
pub struct FileInitialMetaSent;

type PendingPageOpen = (
    bevy::prelude::Without<PageOpenHandled>,
    bevy::prelude::Without<vmux_core::page_open::PageOpenError>,
);

fn new_file_view_bundle(
    url: &str,
    path: PathBuf,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> impl Bundle {
    let title = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    (
        (
            FileView { path },
            FileViewport { top_line: 0, rows: 0 },
            Browser,
            PageMetadata {
                title,
                url: url.to_string(),
                favicon_url: String::new(),
                bg_color: Some(TERMINAL_CEF_BG_COLOR.to_string()),
            },
            WebviewSource::new(url),
            ResolvedWebviewUri(url.to_string()),
            Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                Vec3::Z,
                Vec2::splat(0.5),
            ))),
        ),
        (
            MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Inherited,
            Pickable::default(),
        ),
    )
}

fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
}

pub fn handle_file_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for (entity, task) in &tasks {
        if !task.url.starts_with("files:") {
            continue;
        }
        let Some(path) = path_from_files_url(&task.url) else {
            commands.entity(entity).insert(vmux_core::page_open::PageOpenError {
                message: format!("malformed files URL '{}'", task.url),
            });
            continue;
        };
        clear_stack_children(task.stack, &children_q, &mut commands);
        commands.spawn((
            new_file_view_bundle(&task.url, path, &mut meshes, &mut webview_mt),
            ChildOf(task.stack),
        ));
        commands.entity(entity).insert(PageOpenHandled);
    }
}

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(
            FileResizeEvent,
            FileScrollEvent,
        )>::default())
            .add_systems(
                Update,
                handle_file_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_systems(Update, (load_file_buffers, send_initial_meta))
            .add_observer(on_file_resize)
            .add_observer(on_file_scroll);
    }
}
```

**Why `::default()` (no host scoping):** frontend→backend bin events carry a `host` derived from the embedded-scheme URL (`bin_emit_event_handler.rs:84` → `embedded_page_host_of`). A `files://` document is not the embedded scheme, so its host resolves to `""`. `for_hosts(&["files"])` would therefore DROP every `FileResize`/`FileScroll` event. `::default()` accepts any host and sidesteps this.

The systems `load_file_buffers`, `send_initial_meta`, `on_file_resize`, `on_file_scroll` are implemented in Task 7 — add empty stubs now so the plugin compiles:

```rust
fn load_file_buffers() {}
fn send_initial_meta() {}
fn on_file_resize(_t: On<BinReceive<FileResizeEvent>>) {}
fn on_file_scroll(_t: On<BinReceive<FileScrollEvent>>) {}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_editor page_open_tests 2>&1 | tail -30`
Expected: PASS (2 tests). Fix any import path drift (e.g. `PageMetadata`, `TERMINAL_CEF_BG_COLOR`) by matching `vmux_terminal/src/plugin.rs` imports.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs
git commit -m "feat(editor): EditorPlugin + files:// page-open handler"
```

---

## Task 7: Streaming systems (load buffer, meta, viewport patches)

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs` (replace the stubs)

Read first: `vmux_terminal/src/plugin.rs:1142-1216` (emit gate + `BinHostEmitEvent::from_rkyv`), `:2636-2666` (resize observer reading `WebviewSize`).

- [ ] **Step 1: Replace stubs with real systems**

Replace the four stubs from Task 6 with:

```rust
use crate::highlight::Highlighter;
use crate::viewport::{rows_from_viewport, window_range};

fn load_file_buffers(
    q: Query<(Entity, &FileView), Without<FileBuffer>>,
    mut commands: Commands,
) {
    for (entity, fv) in &q {
        let hl = Highlighter::new();
        match hl.load_file(&fv.path) {
            Ok(out) => {
                commands.entity(entity).insert(FileBuffer {
                    language: out.language,
                    lines: out.lines,
                });
            }
            Err(message) => {
                // Mark as an empty buffer so we don't retry every frame; the error
                // is emitted once the webview is ready (send_initial_meta).
                commands.entity(entity).insert(FileBuffer {
                    language: format!("__error__:{message}"),
                    lines: Vec::new(),
                });
            }
        }
    }
}

fn send_initial_meta(
    q: Query<(Entity, &FileView, &FileBuffer), Without<FileInitialMetaSent>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, buf) in &q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        if let Some(message) = buf.language.strip_prefix("__error__:") {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                FILE_ERROR_EVENT,
                &FileErrorEvent {
                    message: message.to_string(),
                },
            ));
        } else {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                FILE_META_EVENT,
                &FileMetaEvent {
                    path: fv.path.to_string_lossy().to_string(),
                    language: buf.language.clone(),
                    total_lines: buf.lines.len() as u32,
                },
            ));
        }
        commands.entity(entity).insert(FileInitialMetaSent);
    }
}

fn emit_window(
    entity: Entity,
    buf: &FileBuffer,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    let total = buf.lines.len() as u32;
    let (first, end) = window_range(total, vp.top_line, vp.rows);
    let lines = buf.lines[first as usize..end as usize].to_vec();
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_VIEWPORT_EVENT,
        &FileViewportPatch {
            first_line: first,
            total_lines: total,
            lines,
        },
    ));
}

fn on_file_resize(
    trigger: On<BinReceive<FileResizeEvent>>,
    mut q: Query<(&FileBuffer, &mut FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    let Ok((buf, mut vp)) = q.get_mut(entity) else {
        return;
    };
    vp.rows = rows_from_viewport(evt.char_height, evt.viewport_height);
    emit_window(entity, buf, &vp, &browsers, &mut commands);
}

fn on_file_scroll(
    trigger: On<BinReceive<FileScrollEvent>>,
    mut q: Query<(&FileBuffer, &mut FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    let Ok((buf, mut vp)) = q.get_mut(entity) else {
        return;
    };
    vp.top_line = crate::viewport::clamp_top_line(evt.top_line, buf.lines.len() as u32, vp.rows);
    emit_window(entity, buf, &vp, &browsers, &mut commands);
}
```

Add `use bevy_cef::prelude::Browsers;` (confirm the exact import path of `Browsers`, `BinHostEmitEvent`, `BinReceive` against `vmux_terminal/src/plugin.rs:14` `use bevy_cef::prelude::*;`).

- [ ] **Step 2: Write a logic test for the window slice**

Most of this is CEF-coupled (NonSend `Browsers`), so test the slice composition via a small pure helper. Add to `viewport.rs`:

```rust
/// Pick the visible slice indices for a buffer of `total` lines.
pub fn visible_slice(total: u32, top_line: u32, rows: u16) -> std::ops::Range<usize> {
    let (first, end) = window_range(total, top_line, rows);
    (first as usize)..(end as usize)
}
```

Add test in `viewport.rs` tests module:

```rust
    #[test]
    fn visible_slice_indices() {
        assert_eq!(visible_slice(10, 8, 4), 6..10);
        assert_eq!(visible_slice(0, 0, 10), 0..0);
    }
```

Refactor `emit_window` to use `buf.lines[crate::viewport::visible_slice(total, vp.top_line, vp.rows)].to_vec()` and `first = ` the start. (Keep `window_range` for `first`/`total` reporting.)

- [ ] **Step 3: Run tests + build the crate**

Run: `cargo test -p vmux_editor 2>&1 | tail -30`
Expected: PASS (all prior + `visible_slice_indices`).
Run: `cargo build -p vmux_editor 2>&1 | tail -20`
Expected: builds (native).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs crates/vmux_editor/src/viewport.rs
git commit -m "feat(editor): stream file meta + viewport windows over bin bridge"
```

---

## Task 8: Frontend pure helpers (`page_model.rs`)

**Files:**
- Create: `crates/vmux_editor/src/page_model.rs`
- Modify: `crates/vmux_editor/src/lib.rs`

- [ ] **Step 1: Write the failing test**

`crates/vmux_editor/src/page_model.rs`:

```rust
use vmux_core::event::StyledSpan;

/// Gutter character width = digits in the largest line number, min 3.
pub fn gutter_width(total_lines: u32) -> usize {
    let digits = total_lines.max(1).to_string().len();
    digits.max(3)
}

/// Inline CSS for a styled span: `color` + optional bold/italic.
pub fn span_style(span: &StyledSpan) -> String {
    let [r, g, b] = span.fg;
    let mut s = format!("color:rgb({r},{g},{b});");
    if span.bold {
        s.push_str("font-weight:700;");
    }
    if span.italic {
        s.push_str("font-style:italic;");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_width_min_three() {
        assert_eq!(gutter_width(0), 3);
        assert_eq!(gutter_width(9), 3);
        assert_eq!(gutter_width(1000), 4);
        assert_eq!(gutter_width(99999), 5);
    }

    #[test]
    fn span_style_emits_color_and_styles() {
        let s = span_style(&StyledSpan {
            text: "x".into(),
            fg: [10, 20, 30],
            bold: true,
            italic: true,
        });
        assert!(s.contains("color:rgb(10,20,30)"));
        assert!(s.contains("font-weight:700"));
        assert!(s.contains("font-style:italic"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails, then passes**

Add to `lib.rs`: `#[cfg(any(target_arch = "wasm32", test))] pub mod page_model;`
Run: `cargo test -p vmux_editor page_model 2>&1 | tail -20`
Expected: PASS (2 tests) — the impl is already in the file above.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/page_model.rs crates/vmux_editor/src/lib.rs
git commit -m "feat(editor): frontend gutter-width + span-style helpers"
```

---

## Task 9: Frontend page (`page.rs`)

**Files:**
- Create: `crates/vmux_editor/src/page.rs`
- Modify: `crates/vmux_editor/src/lib.rs`

Read first: `crates/vmux_terminal/src/page.rs` in full — copy its structure (measurement span + `ResizeObserver` → emit resize, `use_bin_event_listener` per event, per-row `Signal`s, wheel handler → emit). Replace terminal specifics with file specifics below. This task is wasm-only; it is verified by `cargo check` for the wasm target plus the manual run in Task 14.

- [ ] **Step 1: Write `page.rs`**

`crates/vmux_editor/src/page.rs` (key shape — adapt names from the terminal page you just read):

```rust
#![allow(non_snake_case)]

use crate::page_model::{gutter_width, span_style};
use crate::viewport::rows_from_viewport;
use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

const CONTAINER_ID: &str = "file-container";
const MEASURE_ID: &str = "file-measure";

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut path = use_signal(String::new);
    let mut language = use_signal(String::new);
    let mut total_lines = use_signal(|| 0u32);
    let mut first_line = use_signal(|| 0u32);
    let mut lines = use_signal(Vec::<FileLine>::new);
    let mut error = use_signal(String::new);
    let cell_dims = use_signal(|| (0.0f64, 0.0f64));

    let _meta = use_bin_event_listener::<FileMetaEvent, _>(FILE_META_EVENT, move |m| {
        path.set(m.path.clone());
        language.set(m.language);
        total_lines.set(m.total_lines);
        if let Some(win) = web_sys::window() {
            if let Some(doc) = win.document() {
                let name = m.path.rsplit('/').next().unwrap_or(&m.path).to_string();
                doc.set_title(&name);
            }
        }
    });

    let _vp = use_bin_event_listener::<FileViewportPatch, _>(FILE_VIEWPORT_EVENT, move |p| {
        first_line.set(p.first_line);
        total_lines.set(p.total_lines);
        lines.set(p.lines);
    });

    let _err = use_bin_event_listener::<FileErrorEvent, _>(FILE_ERROR_EVENT, move |e| {
        error.set(e.message);
    });

    use_effect(move || setup_measurement(cell_dims));

    let gw = gutter_width(total_lines());
    let base = first_line();

    rsx! {
        div {
            id: CONTAINER_ID,
            tabindex: "0",
            class: "relative h-full w-full overflow-hidden bg-term-bg text-term-fg font-mono text-sm leading-tight select-none",
            style: "outline:none;",
            onwheel: move |e: Event<WheelData>| {
                e.prevent_default();
                let (_, ch) = cell_dims();
                let line_px = if ch > 0.0 { ch } else { 16.0 };
                let data = e.data();
                let Some(raw) = data.downcast::<web_sys::WheelEvent>() else { return; };
                let notches = (raw.delta_y() / line_px).round() as i64;
                if notches == 0 { return; }
                let cur = first_line() as i64;
                let next = (cur + notches).max(0) as u32;
                let _ = try_cef_bin_emit_rkyv(&FileScrollEvent { top_line: next });
            },
            onkeydown: move |e: Event<KeyboardData>| {
                let key = e.key().to_string();
                let cur = first_line() as i64;
                let next = match key.as_str() {
                    "ArrowDown" => cur + 1,
                    "ArrowUp" => cur - 1,
                    "PageDown" => cur + 20,
                    "PageUp" => cur - 20,
                    "Home" => 0,
                    _ => return,
                };
                e.prevent_default();
                let _ = try_cef_bin_emit_rkyv(&FileScrollEvent { top_line: next.max(0) as u32 });
            },

            // Path header
            div {
                class: "flex h-7 shrink-0 items-center gap-2 border-b border-white/10 px-3 text-xs text-muted-foreground",
                span { class: "truncate", "{path}" }
                if !language().is_empty() {
                    span { class: "ml-auto opacity-60", "{language}" }
                }
            }

            // Error overlay
            {
                let msg = error.read().clone();
                (!msg.is_empty()).then(|| rsx! {
                    div { class: "absolute inset-0 z-50 flex items-center justify-center",
                        style: "background:rgba(0,0,0,0.6);",
                        div { class: "rounded-md border border-ansi-1 bg-term-bg px-4 py-2 text-sm text-ansi-1", "{msg}" }
                    }
                })
            }

            // Content
            div { class: "p-1",
                for line in lines().iter() {
                    div { key: "{line.line_no}", class: "flex whitespace-pre",
                        span {
                            class: "shrink-0 select-none pr-3 text-right opacity-40",
                            style: "width:calc(var(--cw, 1ch) * {gw});",
                            "{line.line_no + 1}"
                        }
                        span {
                            for (i, s) in line.spans.iter().enumerate() {
                                span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

Add `setup_measurement` / `do_measure` by copying the terminal page's versions verbatim (`vmux_terminal/src/page.rs:443-555`), but:
- change `CONTAINER_ID`/`MEASURE_ID` to the file ones,
- replace the final `try_cef_bin_emit_rkyv(&TermResizeEvent { .. })` with:

```rust
    let _ = try_cef_bin_emit_rkyv(&FileResizeEvent {
        char_height: ch as f32,
        viewport_height: vh as f32,
    });
```

(`rows_from_viewport` is imported for parity/possible future use; if unused, delete the import to satisfy clippy.)

- [ ] **Step 2: Add to `lib.rs`**

```rust
#[cfg(all(target_arch = "wasm32", feature = "web"))]
pub mod page;
```

- [ ] **Step 3: Build-check the wasm target**

Run: `cargo check -p vmux_editor --target wasm32-unknown-unknown --features web 2>&1 | tail -30`
Expected: compiles. Fix web-sys feature gaps (add missing features to `Cargo.toml`) and any Dioxus 0.7 RSX drift by diffing against `vmux_terminal/src/page.rs`.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_editor/src/page.rs crates/vmux_editor/src/lib.rs
git commit -m "feat(editor): read-only file viewer page (gutter + highlight + scroll)"
```

---

## Task 10: Register the page in the shared app + protocol routing

**Files:**
- Modify: `crates/vmux_server/Cargo.toml`
- Modify: `crates/vmux_server/src/lib.rs`

- [ ] **Step 1: Add the dependency + feature**

In `crates/vmux_server/Cargo.toml`:
- under the `webview`/feature list that contains `"dep:vmux_terminal"`, add `"dep:vmux_editor",`
- in `[dependencies]`, add `vmux_editor = { path = "../vmux_editor", optional = true }`

- [ ] **Step 2: Write the failing test for `current_host`**

In `crates/vmux_server/src/lib.rs`, extract the protocol decision into a pure fn and test it. Add near `current_host`:

```rust
#[cfg(not(target_arch = "wasm32"))]
fn host_for(protocol: &str, host: &str) -> String {
    if protocol == "files:" {
        "files".to_string()
    } else {
        host.to_string()
    }
}

#[cfg(all(not(target_arch = "wasm32"), test))]
mod host_tests {
    use super::*;

    #[test]
    fn files_protocol_routes_to_files_host() {
        assert_eq!(host_for("files:", ""), "files");
        assert_eq!(host_for("vmux:", "terminal"), "terminal");
        assert_eq!(host_for("https:", "example.com"), "example.com");
    }
}
```

- [ ] **Step 3: Run test to verify it fails, then passes**

Run: `cargo test -p vmux_server host_tests 2>&1 | tail -20`
Expected: PASS once `host_for` is added (the test imports it).

- [ ] **Step 4: Wire `host_for` into `current_host` + register the page**

Change `current_host()` (the wasm path at `lib.rs:66`) to consult protocol first:

```rust
#[cfg(all(target_arch = "wasm32", feature = "web"))]
fn current_host() -> String {
    let win = web_sys::window();
    let loc = win.as_ref().map(|w| w.location());
    let protocol = loc.as_ref().and_then(|l| l.protocol().ok()).unwrap_or_default();
    if protocol == "files:" {
        return "files".to_string();
    }
    loc.and_then(|l| l.host().ok()).unwrap_or_default()
}
```

Add to the `register_pages!` macro invocation (after `render_terminal`):

```rust
    render_files: "files" => vmux_editor::page::Page,
```

(Confirm the macro is gated under `#[cfg(all(target_arch = "wasm32", feature = "web"))]` so the native build doesn't need `vmux_editor::page`.)

- [ ] **Step 5: Build-check both targets**

Run: `cargo check -p vmux_server 2>&1 | tail -20`
Run: `cargo check -p vmux_server --target wasm32-unknown-unknown --features web 2>&1 | tail -20`
Expected: both compile.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_server/Cargo.toml crates/vmux_server/src/lib.rs
git commit -m "feat(editor): route files:// protocol to the editor page"
```

---

## Task 11: `files://` CEF scheme — base-href helper + registration

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/src/util.rs`
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/app.rs`
- Modify: `patches/bevy_cef_core-0.5.2/src/render_process/app.rs`

- [ ] **Step 1: Write the failing test for the base-href injector**

Add to `patches/bevy_cef_core-0.5.2/src/util.rs` (in its `#[cfg(test)] mod tests`, or a new one):

```rust
#[cfg(test)]
mod files_scheme_tests {
    use super::*;

    #[test]
    fn injects_base_after_head() {
        let html = "<!doctype html><html><head><title>x</title></head><body></body></html>";
        let out = inject_base_href(html, "vmux://files/");
        assert!(out.contains(r#"<base href="vmux://files/">"#));
        // base must come right after <head>, before <title>
        let head = out.find("<head>").unwrap();
        let base = out.find("<base").unwrap();
        let title = out.find("<title>").unwrap();
        assert!(head < base && base < title);
    }

    #[test]
    fn no_head_falls_back_to_prepending() {
        let html = "<html><body>hi</body></html>";
        let out = inject_base_href(html, "vmux://files/");
        assert!(out.contains(r#"<base href="vmux://files/">"#));
    }

    #[test]
    fn idempotent_when_base_present() {
        let html = r#"<head><base href="vmux://files/"></head>"#;
        let out = inject_base_href(html, "vmux://files/");
        assert_eq!(out.matches("<base").count(), 1);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p bevy_cef_core files_scheme_tests 2>&1 | tail -20`
Expected: FAIL — `inject_base_href` not found.

- [ ] **Step 3: Implement `inject_base_href`**

Add near the other `pub fn` helpers in `util.rs`:

```rust
pub const FILES_SCHEME: &str = "files";

/// Insert `<base href="...">` immediately after the first `<head>` so a document
/// served on the `files://` scheme resolves relative asset URLs against the
/// embedded `vmux://` scheme instead of the file's own directory.
pub fn inject_base_href(html: &str, base: &str) -> String {
    if html.contains("<base ") {
        return html.to_string();
    }
    let tag = format!(r#"<base href="{base}">"#);
    if let Some(idx) = html.find("<head>") {
        let cut = idx + "<head>".len();
        let mut out = String::with_capacity(html.len() + tag.len());
        out.push_str(&html[..cut]);
        out.push_str(&tag);
        out.push_str(&html[cut..]);
        out
    } else {
        format!("{tag}{html}")
    }
}
```

- [ ] **Step 4: Register the scheme name in both processes**

`browser_process/app.rs` `on_register_custom_schemes` (after line 96):

```rust
            registrar.add_custom_scheme(Some(&FILES_SCHEME.into()), cef_scheme_flags() as _);
```

`render_process/app.rs` (after line 45, the embedded scheme registration):

```rust
            registrar.add_custom_scheme(Some(&FILES_SCHEME.into()), cef_scheme_flags() as _);
```

Add `FILES_SCHEME` (and `inject_base_href` if needed later) to the `use crate::util::{...}` imports in each file. Match each file's existing import style (e.g. `app.rs:7 use crate::util::{SCHEME_CEF, cef_scheme_flags};`).

- [ ] **Step 5: Run test + build the patched crate**

Run: `cargo test -p bevy_cef_core files_scheme_tests 2>&1 | tail -20`
Expected: PASS (3 tests).
Run: `cargo build -p bevy_cef_core 2>&1 | tail -20`
Expected: builds.

- [ ] **Step 6: Commit**

```bash
git add patches/bevy_cef_core-0.5.2/src/util.rs patches/bevy_cef_core-0.5.2/src/browser_process/app.rs patches/bevy_cef_core-0.5.2/src/render_process/app.rs
git commit -m "feat(cef): register files:// scheme + base-href injector"
```

---

## Task 12: `files://` scheme handler — serve the SPA shell

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/localhost.rs`
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs`

Read first: `localhost.rs:75-135` (`asset_load_path_from_request_url`), `:173-340` (`LocalSchemaHandlerBuilder` + the resource handler `open`/response). `browsers.rs:195-225` (global factory registration) and `:1420-1455` (per-context registration).

This is the unsafe-glue task. The pure decision was already isolated and tested in Task 11 (`inject_base_href`). Here you wire it into the resource handler.

- [ ] **Step 1: Make the handler recognize files:// documents**

In `localhost.rs`, the request URL → asset mapping currently handles `cef://localhost/...` and the embedded scheme. Add a branch: when `url` starts with `files://`, the asset to load is the SPA index document — the same embedded asset the embedded scheme serves for its default host. Resolve it to the embedded index (e.g. `embedded://index.html`; confirm the exact embedded path the existing handler uses for a host's `index.html`). Return a marker so the response step injects the base href.

Concretely, in the function that produces the response bytes (the `ResourceHandler`/`DataResponser` path around `localhost.rs:300-330`), after the bytes for a files:// document are read, transform them:

```rust
// Pseudocode location: where `bytes` + `mime` are finalized for the response.
if request_url.starts_with("files://") && mime == "text/html" {
    let base = format!("{}://{}/", crate::util::FILES_SCHEME_BASE_SCHEME, "files");
    // base = "vmux://files/"  (embedded scheme + "files" host)
    let html = String::from_utf8_lossy(&bytes);
    bytes = crate::util::inject_base_href(&html, &base).into_bytes();
}
```

Define `FILES_SCHEME_BASE_SCHEME` as the resolved embedded scheme (`resolved_cef_embedded_page_config().scheme`), or build the base string directly as `format!("{}files/", resolved_cef_embedded_page_config().scheme_prefix())` → `"vmux://files/"`. Use whichever matches the existing helpers; the goal is the literal `vmux://files/`.

For mapping the files:// request to the index asset, mirror the embedded scheme's "unknown host → default document" path so any `files://...` document loads `index.html`.

- [ ] **Step 2: Register the factory for the files scheme**

In `browsers.rs`, everywhere the embedded scheme factory is registered (global block ~`:212-224` and per-context block ~`:1431-1452`), add an identical registration for the `files` scheme:

```rust
let mut files_factory = LocalSchemaHandlerBuilder::build(requester_for_global.clone());
let ok_files = register_scheme_handler_factory(
    Some(&crate::util::FILES_SCHEME.into()),
    None, // all domains
    Some(&mut files_factory),
);
webview_debug_log(format!("register_scheme_handler_factory files://* ok={ok_files}"));
```

For the per-context registrations (`context.register_scheme_handler_factory(...)`), add a matching call with `FILES_SCHEME`. Match the exact argument shape used by the existing embedded-scheme calls on those lines.

- [ ] **Step 3: Build the patched crate + dependent app**

Run: `cargo build -p bevy_cef_core 2>&1 | tail -30`
Expected: builds. (CEF build is heavy; expect minutes.)

- [ ] **Step 4: Commit**

```bash
git add patches/bevy_cef_core-0.5.2/src/browser_process/localhost.rs patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs
git commit -m "feat(cef): serve SPA shell for files:// documents with base href"
```

---

## Task 13: Native wiring + layout glue

**Files:**
- Modify: `crates/vmux_desktop/src/lib.rs`
- Modify: `crates/vmux_layout/src/snapshot.rs`
- Modify: `crates/vmux_layout/src/page.rs`
- Modify: `crates/vmux_layout/src/command_bar/page.rs`

- [ ] **Step 1: Add `EditorPlugin` to the native app**

`crates/vmux_desktop/src/lib.rs`:
- add `vmux_editor::EditorPlugin,` to the import list near line 43 (`vmux_terminal::TerminalPlugin,`)
- add `EditorPlugin,` to the `add_plugins((...))` tuple near line 111 (next to `TerminalPlugin,`)
- add `vmux_editor = { path = "../vmux_editor" }` to `crates/vmux_desktop/Cargo.toml` `[dependencies]`

- [ ] **Step 2: Write the failing snapshot-kind test**

In `crates/vmux_layout/src/snapshot.rs` tests module, add:

```rust
    #[test]
    fn files_url_maps_to_files_kind() {
        // build_stack is private to the module; assert via the kind helper it uses.
        assert_eq!(stack_kind_for_url("files:///a/b.rs"), "files");
        assert_eq!(stack_kind_for_url("vmux://terminal/"), "terminal");
        assert_eq!(stack_kind_for_url("https://x.com"), "browser");
    }
```

- [ ] **Step 3: Extract + extend the kind helper**

In `snapshot.rs`, replace the inline `let kind = if url.starts_with("vmux://terminal/") {...}` in `build_stack` (line ~140) with a call to a new pure fn, and define it:

```rust
fn stack_kind_for_url(url: &str) -> &'static str {
    if url.starts_with("vmux://terminal/") {
        "terminal"
    } else if url.starts_with("files:") {
        "files"
    } else {
        "browser"
    }
}
```

Use it: `let kind = stack_kind_for_url(&url);`

- [ ] **Step 4: Run test**

Run: `cargo test -p vmux_layout files_url 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: StackIcon + address bar + command bar**

`crates/vmux_layout/src/page.rs`:
- In `StackIcon` (line ~317) add a branch before the favicon branch:

```rust
        } else if url.starts_with("files:") {
            Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                path { d: "M14 2v6h6" }
            }
```

- In `format_address` (line ~179) extend the early-return:

```rust
    if stack.url.starts_with("vmux://") || stack.url.starts_with("files:") {
        return stack.url.clone();
    }
```

`crates/vmux_layout/src/command_bar/page.rs` (line ~130): the predicate that treats input as a URL vs. a search — add `|| trimmed.starts_with("files:")` alongside the existing `vmux://` check.

- [ ] **Step 6: Build + commit**

Run: `cargo check -p vmux_layout -p vmux_desktop 2>&1 | tail -20`
Expected: compiles.

```bash
git add crates/vmux_desktop/src/lib.rs crates/vmux_desktop/Cargo.toml crates/vmux_layout/src/snapshot.rs crates/vmux_layout/src/page.rs crates/vmux_layout/src/command_bar/page.rs
git commit -m "feat(editor): wire EditorPlugin + files:// layout/address/command-bar glue"
```

---

## Task 14: Full build, format, clippy, manual verification

**Files:** none (verification only)

- [ ] **Step 1: Workspace checks**

Run: `cargo fmt --all`
Run: `cargo clippy --workspace --all-targets 2>&1 | tail -40`
Expected: no warnings in new code. Fix any.

- [ ] **Step 2: Targeted tests green**

Run: `cargo test -p vmux_core -p vmux_editor -p vmux_layout -p vmux_server -p bevy_cef_core 2>&1 | tail -40`
Expected: all PASS.

- [ ] **Step 3: Build the app (rebuilds the webview dist + CEF)**

Run the project's normal dev build (do not launch an unbounded `make dev` yourself — per project rules, let the user run/verify the UI). Confirm it compiles:
Run: `cargo build -p vmux_desktop 2>&1 | tail -20`

- [ ] **Step 4: Manual verification checklist (user-driven)**

Hand to the user / run interactively:
1. Launch vmux. In the address bar type `files:///<an absolute path to a source file>` and open it.
2. Expect: path header shows the file path + detected language; line-number gutter; syntax-colored monospace content.
3. Scroll with the wheel and arrow/Page keys → content streams (gutter numbers advance; large files scroll without loading the whole file).
4. Open a non-existent path → error overlay with a clear message.
5. Open a binary file (e.g. an image) → "not a UTF-8 text file" error overlay (not a crash).
6. Confirm assets loaded: no blank page (verifies the `<base href="vmux://files/">` cross-scheme asset path). If blank, switch to the §1 fallback (serve `/wasm/*` + `/assets/*` from the files handler with root-absolute refs).

- [ ] **Step 5: Final commit (if fmt/clippy changed anything)**

```bash
git add -A
git commit -m "chore(editor): fmt + clippy cleanup"
```

---

## Self-Review notes (already reconciled)

- **Spec coverage:** scheme registration+handler (T11–T12), routing (T10), page-open+FileView (T6), streaming/virtualization (T7), syntect (T3–T4), frontend page+gutter+header (T8–T9), layout glue (T13), protocol types (T1). All spec sections map to a task.
- **Empty-host bin gating:** handled in T6 with `::default()` + rationale.
- **u32 line indices:** all protocol/viewport types use `u32` (T1, T2).
- **Type consistency:** `FileView`, `FileBuffer`, `FileViewport`, `window_range`, `clamp_top_line`, `visible_slice`, `gutter_width`, `span_style`, `inject_base_href`, `host_for` names are used identically across tasks.
- **Risk (cross-scheme assets):** primary `<base>` approach in T12 with an explicit fallback in T14 step 4.6.
```
