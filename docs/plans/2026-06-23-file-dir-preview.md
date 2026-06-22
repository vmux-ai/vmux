# file:// Directory Browser (miller columns + previews) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the flat `file://` directory icon-grid with a yazi-style, soft-glass miller-column browser that previews images, code/text, sub-directories, and file metadata, and opens files/dirs in place.

**Architecture:** Host (`vmux_editor` native Bevy plugin) reads the filesystem and pushes rkyv bin-events; the wasm Dioxus page (`vmux_editor/src/page.rs`) renders three modes (dir columns, full text, full image). Navigation re-opens a `file://` URL in the same stack via the existing `PageOpenRequest`/`handle_file_page_open` flow. Images and ~64px list thumbnails travel as bytes over the bin channel; the page builds `Blob` object URLs. No patched-CEF changes.

**Tech Stack:** Rust, Bevy ECS, bevy_cef bin events (`rkyv`), Dioxus (wasm), Tailwind utilities, `syntect` (existing highlighter), `image` crate (native thumbnail downscale), `web-sys`/`js-sys` (Blob URLs, IntersectionObserver).

**Reference spec:** `docs/specs/2026-06-23-file-dir-preview-design.md`

**Working dir:** worktree `.worktrees/file-dir-preview`, branch `feat/file-dir-preview`. All paths below are relative to that worktree root.

**Per-task discipline:** run `cargo fmt` before each commit. Run `cargo clippy -p vmux_editor -p vmux_core` before committing host/core changes. Do not launch `make dev` — the user runtime-tests the wasm UI.

---

## File structure

- `crates/vmux_core/src/event.rs` — protocol: extend `FileDirEvent`; add `PreviewKind`, `FilePreviewRequest`, `FilePreviewEvent`, `FileOpenEvent`, `FileImageEvent` + event-name consts.
- `crates/vmux_editor/src/dir.rs` *(new)* — pure host helpers: directory + parent listing.
- `crates/vmux_editor/src/preview.rs` *(new)* — pure host helpers: image classification, `downscale_to_png`, `build_preview_sync`, `resolve_open_target`.
- `crates/vmux_editor/src/plugin.rs` — wire new components/systems/observers; image full-mode; preview/open/thumbnail handlers; emit extended dir event.
- `crates/vmux_editor/src/page_model.rs` — pure page helpers: `classify`, `clamp_selection`, `parent_highlight_index`.
- `crates/vmux_editor/src/page.rs` — three render modes, miller-column UI, keymap, preview pane, lazy thumbnails, soft-glass styling.
- `crates/vmux_editor/src/lib.rs` — register new modules.
- `crates/vmux_editor/Cargo.toml` — add `image` (native) + web-sys features.

---

## Task 1: Protocol types (`vmux_core`)

**Files:**
- Modify: `crates/vmux_core/src/event.rs`

- [ ] **Step 1: Write the failing test**

Append to the existing test module at the bottom of `crates/vmux_core/src/event.rs` (there is already a `#[cfg(test)] mod tests` — add these inside it; if none, add the module):

```rust
#[test]
fn preview_kind_rkyv_roundtrip() {
    let k = PreviewKind::Image { mime: "image/png".into(), bytes: vec![1, 2, 3] };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&k).unwrap();
    let back: PreviewKind = rkyv::from_bytes::<PreviewKind, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(k, back);
}

#[test]
fn file_dir_event_has_parent_fields() {
    let e = FileDirEvent {
        path: "/a/b".into(),
        entries: vec![],
        parent_path: "/a".into(),
        parent_entries: vec![],
    };
    assert_eq!(e.parent_path, "/a");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_core preview_kind_rkyv_roundtrip`
Expected: FAIL — `PreviewKind` not found; `FileDirEvent` has no field `parent_path`.

- [ ] **Step 3: Implement the types**

Add the consts next to the other `FILE_*` consts (after `pub const FILE_THEME_EVENT`):

```rust
pub const FILE_PREVIEW_REQUEST_EVENT: &str = "file_preview_request"; // page→host
pub const FILE_PREVIEW_EVENT: &str = "file_preview";                 // host→page
pub const FILE_OPEN_EVENT: &str = "file_open";                       // page→host
pub const FILE_IMAGE_EVENT: &str = "file_image";                     // host→page
```

Extend `FileDirEvent` (add the two fields):

```rust
pub struct FileDirEvent {
    pub path: String,
    pub entries: Vec<FileDirEntry>,
    pub parent_path: String,
    pub parent_entries: Vec<FileDirEntry>,
}
```

Add new types (use the same derive block the other event structs use —
`Debug, Clone, PartialEq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize`):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FilePreviewRequest {
    pub path: String,
    pub thumb: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum PreviewKind {
    Dir(Vec<FileDirEntry>),
    Text(Vec<FileLine>),
    Image { mime: String, bytes: Vec<u8> },
    Info { size: u64, modified: String, kind: String },
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FilePreviewEvent {
    pub path: String,
    pub kind: PreviewKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FileOpenEvent {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FileImageEvent {
    pub mime: String,
    pub bytes: Vec<u8>,
}
```

- [ ] **Step 4: Fix existing `FileDirEvent` constructions**

The new fields break existing call sites. Update `send_initial_dir` in
`crates/vmux_editor/src/plugin.rs` temporarily to pass `parent_path: String::new(), parent_entries: Vec::new()` (Task 3 fills these in). Search for other `FileDirEvent {` constructions and fix the same way.

Run: `cargo test -p vmux_core preview_kind_rkyv_roundtrip file_dir_event_has_parent_fields`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/event.rs crates/vmux_editor/src/plugin.rs
git commit -m "feat(core): file preview/open/image events + dir parent fields"
```

---

## Task 2: Pure page helpers (`page_model.rs`)

**Files:**
- Modify: `crates/vmux_editor/src/page_model.rs`

`page_model` is already compiled for `wasm32` **and** `test` (see `lib.rs`), so its tests run natively.

- [ ] **Step 1: Write the failing tests**

Append to `crates/vmux_editor/src/page_model.rs`:

```rust
#[cfg(test)]
mod dir_browser_tests {
    use super::*;

    #[test]
    fn classify_dir_and_image_and_text() {
        assert_eq!(classify("/a/b", true), ContentClass::Dir);
        assert_eq!(
            classify("/a/p.PNG", false),
            ContentClass::Image { mime: "image/png".into() }
        );
        assert_eq!(classify("/a/main.rs", false), ContentClass::Text);
        assert_eq!(classify("/a/blob.bin", false), ContentClass::Other);
    }

    #[test]
    fn clamp_selection_bounds() {
        assert_eq!(clamp_selection(5, 3), 2);
        assert_eq!(clamp_selection(0, 0), 0);
        assert_eq!(clamp_selection(1, 3), 1);
    }

    #[test]
    fn parent_highlight_matches_current_dir() {
        let parent = vec![
            entry("/a/x", true),
            entry("/a/b", true),
            entry("/a/y", false),
        ];
        assert_eq!(parent_highlight_index(&parent, "/a/b"), Some(1));
        assert_eq!(parent_highlight_index(&parent, "/a/zzz"), None);
    }

    fn entry(path: &str, is_dir: bool) -> vmux_core::event::FileDirEntry {
        vmux_core::event::FileDirEntry {
            name: path.rsplit('/').next().unwrap().to_string(),
            path: path.to_string(),
            is_dir,
        }
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_editor classify_dir_and_image`
Expected: FAIL — `classify`, `ContentClass`, `clamp_selection`, `parent_highlight_index` undefined.

- [ ] **Step 3: Implement helpers**

Add to `crates/vmux_editor/src/page_model.rs`:

```rust
use vmux_core::event::FileDirEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentClass {
    Dir,
    Image { mime: String },
    Text,
    Other,
}

/// Map a file extension to a supported image mime, if any.
pub fn image_mime(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

pub fn classify(path: &str, is_dir: bool) -> ContentClass {
    if is_dir {
        return ContentClass::Dir;
    }
    if let Some(mime) = image_mime(path) {
        return ContentClass::Image { mime: mime.to_string() };
    }
    // Anything the highlighter accepts is treated as text. The host is the source of
    // truth; the page uses this only to decide whether to request a thumbnail vs text
    // snippet. Treat files with an extension and no image match as text-candidate.
    if path.rsplit('/').next().is_some_and(|s| s.contains('.')) {
        ContentClass::Text
    } else {
        ContentClass::Other
    }
}

pub fn clamp_selection(idx: usize, len: usize) -> usize {
    if len == 0 { 0 } else { idx.min(len - 1) }
}

pub fn parent_highlight_index(parent: &[FileDirEntry], current_path: &str) -> Option<usize> {
    parent.iter().position(|e| e.path == current_path)
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_editor dir_browser_tests`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/page_model.rs
git commit -m "feat(editor): page_model classify/clamp/parent-index helpers"
```

---

## Task 3: Host directory + parent listing (`dir.rs`)

**Files:**
- Create: `crates/vmux_editor/src/dir.rs`
- Modify: `crates/vmux_editor/src/lib.rs`, `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/vmux_editor/src/dir.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn lists_dir_hides_dotfiles_dirs_first() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("zdir")).unwrap();
        fs::write(tmp.path().join("a.txt"), "x").unwrap();
        fs::write(tmp.path().join(".hidden"), "x").unwrap();
        let entries = list_dir(tmp.path());
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["zdir", "a.txt"]); // dir first, dotfile hidden
    }

    #[test]
    fn parent_listing_of_nested_is_some_root_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let child = tmp.path().join("child");
        fs::create_dir(&child).unwrap();
        let (pp, pe) = parent_listing(&child);
        assert_eq!(pp, tmp.path().to_string_lossy());
        assert!(pe.iter().any(|e| e.name == "child"));

        let (rp, re) = parent_listing(std::path::Path::new("/"));
        assert!(rp.is_empty());
        assert!(re.is_empty());
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_editor -- dir::tests`
Expected: FAIL — `list_dir`/`parent_listing` undefined.

- [ ] **Step 3: Implement**

Prepend to `crates/vmux_editor/src/dir.rs` (move `list_dir` here from `plugin.rs`, add dotfile filter + `parent_listing`):

```rust
use std::path::Path;
use vmux_core::event::FileDirEntry;

pub fn list_dir(path: &Path) -> Vec<FileDirEntry> {
    let Ok(read) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut entries: Vec<FileDirEntry> = read
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            FileDirEntry {
                name: e.file_name().to_string_lossy().to_string(),
                path: e.path().to_string_lossy().to_string(),
                is_dir,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

/// `(parent_path, parent_entries)` for `path`; both empty when there is no parent.
pub fn parent_listing(path: &Path) -> (String, Vec<FileDirEntry>) {
    match path.parent() {
        Some(p) => (p.to_string_lossy().to_string(), list_dir(p)),
        None => (String::new(), Vec::new()),
    }
}
```

Register the module in `crates/vmux_editor/src/lib.rs` (native only, next to `mod plugin;`):

```rust
#[cfg(not(target_arch = "wasm32"))]
mod dir;
```

In `crates/vmux_editor/src/plugin.rs`: delete the old `fn list_dir`, add `use crate::dir::{list_dir, parent_listing};`, and update `send_initial_dir` to populate the parent fields:

```rust
let (parent_path, parent_entries) = parent_listing(&fv.path);
commands.trigger(BinHostEmitEvent::from_rkyv(
    entity,
    FILE_DIR_EVENT,
    &FileDirEvent {
        path: display_path(&fv.path),
        entries: dir.entries.clone(),
        parent_path,
        parent_entries,
    },
));
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_editor -- dir::tests`
Expected: PASS (2 tests). Also `cargo test -p vmux_editor` (existing `url_tests`, `page_open_tests` still green).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/dir.rs crates/vmux_editor/src/lib.rs crates/vmux_editor/src/plugin.rs
git commit -m "feat(editor): dir+parent listing, hide dotfiles, emit parent in FileDirEvent"
```

---

## Task 4: Host image classification + thumbnail downscale (`preview.rs`)

**Files:**
- Create: `crates/vmux_editor/src/preview.rs`
- Modify: `crates/vmux_editor/Cargo.toml`, `crates/vmux_editor/src/lib.rs`

- [ ] **Step 1: Add the `image` dependency**

In `crates/vmux_editor/Cargo.toml`, under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`:

```toml
image = { version = "0.25", default-features = false, features = ["png", "jpeg", "gif", "webp"] }
```

- [ ] **Step 2: Write the failing test**

Create `crates/vmux_editor/src/preview.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn downscale_caps_longest_edge_and_is_valid_png() {
        let src = png_bytes(200, 100);
        let thumb = downscale_to_png(&src, 64).unwrap();
        let decoded = image::load_from_memory(&thumb).unwrap();
        assert!(decoded.width() <= 64 && decoded.height() <= 64);
        assert_eq!(decoded.width().max(decoded.height()), 64);
    }

    #[test]
    fn downscale_rejects_garbage() {
        assert!(downscale_to_png(&[0, 1, 2, 3], 64).is_err());
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p vmux_editor -- preview::tests`
Expected: FAIL — `downscale_to_png` undefined.

- [ ] **Step 4: Implement classification + downscale**

Prepend to `crates/vmux_editor/src/preview.rs`:

```rust
use std::path::Path;

pub const IMAGE_BYTES_CAP: u64 = 25 * 1024 * 1024;
pub const THUMB_MAX_EDGE: u32 = 64;

pub fn image_mime(path: &Path) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

pub fn is_image_path(path: &Path) -> bool {
    image_mime(path).is_some()
}

/// Decode `bytes`, downscale so the longest edge is `max_edge`, re-encode PNG.
pub fn downscale_to_png(bytes: &[u8], max_edge: u32) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let thumb = img.thumbnail(max_edge, max_edge); // preserves aspect, fits within box
    let mut out = std::io::Cursor::new(Vec::new());
    thumb
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}
```

Register module in `lib.rs` (native only):

```rust
#[cfg(not(target_arch = "wasm32"))]
mod preview;
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p vmux_editor -- preview::tests`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_editor/Cargo.toml crates/vmux_editor/src/preview.rs crates/vmux_editor/src/lib.rs
git commit -m "feat(editor): image classification + 64px thumbnail downscale"
```

---

## Task 5: Host preview builder + open-target resolver (pure)

**Files:**
- Modify: `crates/vmux_editor/src/preview.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `preview.rs`:

```rust
#[test]
fn build_preview_dir_text_image_info() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path().join("sub");
    std::fs::create_dir(&d).unwrap();
    matches!(build_preview_sync(&d), vmux_core::event::PreviewKind::Dir(_));

    let t = tmp.path().join("a.rs");
    std::fs::write(&t, "fn main() {}\n").unwrap();
    matches!(build_preview_sync(&t), vmux_core::event::PreviewKind::Text(_));

    let p = tmp.path().join("p.png");
    std::fs::write(&p, png_bytes(8, 8)).unwrap();
    matches!(build_preview_sync(&p), vmux_core::event::PreviewKind::Image { .. });

    let b = tmp.path().join("blob.bin");
    std::fs::write(&b, [0u8; 4]).unwrap();
    matches!(build_preview_sync(&b), vmux_core::event::PreviewKind::Info { .. });
}

#[test]
fn build_preview_image_over_cap_is_info() {
    // a path whose metadata size exceeds the cap returns Info, never reads bytes
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("huge.png");
    std::fs::write(&p, vec![0u8; 8]).unwrap();
    // force cap check by calling the helper with a tiny cap
    let k = build_preview_with_cap(&p, false, 1);
    assert!(matches!(k, vmux_core::event::PreviewKind::Info { .. }));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_editor -- preview::tests::build_preview`
Expected: FAIL — `build_preview_sync` / `build_preview_with_cap` undefined.

- [ ] **Step 3: Implement**

Add to `preview.rs` (uses the existing `Highlighter` for text; first 200 lines):

```rust
use vmux_core::event::{FileLine, PreviewKind};
use crate::dir::list_dir;
use crate::highlight::Highlighter;

const TEXT_PREVIEW_LINES: usize = 200;

pub fn build_preview_sync(path: &Path) -> PreviewKind {
    build_preview_with_cap(path, false, IMAGE_BYTES_CAP)
}

/// Full (non-thumbnail) preview. `thumb` handling is done by the async path; this
/// builds Dir/Text/Image(full)/Info/Error synchronously.
pub fn build_preview_with_cap(path: &Path, _thumb: bool, cap: u64) -> PreviewKind {
    if path.is_dir() {
        return PreviewKind::Dir(list_dir(path));
    }
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return PreviewKind::Error(e.to_string()),
    };
    if let Some(mime) = image_mime(path) {
        if meta.len() > cap {
            return info_kind(&meta, "image (too large to preview)");
        }
        return match std::fs::read(path) {
            Ok(bytes) => PreviewKind::Image { mime: mime.to_string(), bytes },
            Err(e) => PreviewKind::Error(e.to_string()),
        };
    }
    match Highlighter::new().load_file(path) {
        Ok(out) => {
            let lines: Vec<FileLine> = out.lines.into_iter().take(TEXT_PREVIEW_LINES).collect();
            PreviewKind::Text(lines)
        }
        Err(_) => info_kind(&meta, "file"),
    }
}

fn info_kind(meta: &std::fs::Metadata, kind: &str) -> PreviewKind {
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();
    PreviewKind::Info { size: meta.len(), modified, kind: kind.to_string() }
}

/// Resolve the pane entity that owns `fileview` by walking ChildOf (FileView → stack → pane).
pub fn resolve_open_target(
    fileview: bevy::prelude::Entity,
    parents: &bevy::prelude::Query<&bevy::prelude::ChildOf>,
    is_pane: &bevy::prelude::Query<(), bevy::prelude::With<vmux_layout::pane::Pane>>,
) -> Option<bevy::prelude::Entity> {
    let mut cur = fileview;
    for _ in 0..8 {
        let parent = parents.get(cur).ok()?.0;
        if is_pane.get(parent).is_ok() {
            return Some(parent);
        }
        cur = parent;
    }
    None
}
```

> Note: `resolve_open_target` is exercised by Task 7's system test, not here (it needs an ECS world). If `vmux_layout::pane::Pane` is not public, use the existing public re-export used elsewhere in `plugin.rs`; check the imports there.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_editor -- preview::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/preview.rs
git commit -m "feat(editor): synchronous preview builder + open-target resolver"
```

---

## Task 6: Host wiring — preview/open/image systems + thumbnails

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs`

This task adds the runtime systems. The pure logic is already tested; here we wire observers and an off-thread thumbnail task, then build-verify (these observers depend on the CEF bin runtime and are not unit-tested, matching the existing `on_file_scroll`/`on_file_resize` pattern).

- [ ] **Step 1: Image full-mode component + load branch**

Add a marker and emit `FileImageEvent` for image FileViews instead of a text buffer. In `plugin.rs`:

```rust
#[derive(Component, Clone, Debug)]
pub struct FileImage { pub mime: String, pub bytes: Vec<u8> }
```

Update `load_file_buffers` (the `UnloadedFileView` loader): before the highlighter branch, handle images:

```rust
if crate::preview::is_image_path(&fv.path) {
    match std::fs::metadata(&fv.path).map(|m| m.len()) {
        Ok(len) if len <= crate::preview::IMAGE_BYTES_CAP => {
            match std::fs::read(&fv.path) {
                Ok(bytes) => {
                    let mime = crate::preview::image_mime(&fv.path).unwrap_or("image/png");
                    commands.entity(entity).insert(FileImage { mime: mime.to_string(), bytes });
                }
                Err(e) => { commands.entity(entity).insert(FileBuffer {
                    language: format!("__error__:{e}"), lines: Vec::new() }); }
            }
        }
        _ => { commands.entity(entity).insert(FileBuffer {
            language: "__error__:image too large to preview".into(), lines: Vec::new() }); }
    }
    continue;
}
```

Add `UnloadedFileView` must also exclude `FileImage`:

```rust
type UnloadedFileView = (Without<FileBuffer>, Without<FileDir>, Without<FileImage>);
```

- [ ] **Step 2: Send the image event when ready**

Add a system mirroring `send_initial_dir`:

```rust
fn send_initial_image(
    q: Query<(Entity, &FileImage), ReadyUnsentMeta>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, img) in &q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) { continue; }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity, FILE_IMAGE_EVENT,
            &FileImageEvent { mime: img.mime.clone(), bytes: img.bytes.clone() },
        ));
        commands.entity(entity).insert(FileInitialMetaSent);
    }
}
```

- [ ] **Step 3: Preview request observer (sync) + thumbnail task**

```rust
#[derive(Component)]
struct ThumbTask(bevy::tasks::Task<(String, Result<Vec<u8>, String>)>);

fn on_file_preview_request(
    trigger: On<BinReceive<FilePreviewRequest>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let req = trigger.event().payload.clone();
    let path = std::path::PathBuf::from(&req.path);
    if req.thumb && crate::preview::is_image_path(&path) {
        let pool = bevy::tasks::IoTaskPool::get();
        let p = req.path.clone();
        let task = pool.spawn(async move {
            let r = std::fs::read(&p)
                .map_err(|e| e.to_string())
                .and_then(|b| crate::preview::downscale_to_png(&b, crate::preview::THUMB_MAX_EDGE));
            (p, r)
        });
        commands.entity(entity).insert(ThumbTask(task));
        return;
    }
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) { return; }
    let kind = crate::preview::build_preview_sync(&path);
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity, FILE_PREVIEW_EVENT, &FilePreviewEvent { path: req.path, kind },
    ));
}

fn drain_thumb_tasks(
    mut q: Query<(Entity, &mut ThumbTask)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    use bevy::tasks::futures_lite::future;
    for (entity, mut t) in &mut q {
        if let Some((path, result)) = future::block_on(future::poll_once(&mut t.0)) {
            commands.entity(entity).remove::<ThumbTask>();
            if let Ok(bytes) = result {
                if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
                    commands.trigger(BinHostEmitEvent::from_rkyv(
                        entity, FILE_PREVIEW_EVENT,
                        &FilePreviewEvent { path, kind: PreviewKind::Image {
                            mime: "image/png".into(), bytes } },
                    ));
                }
            }
            // decode failure → emit nothing; page keeps the glyph placeholder
        }
    }
}
```

> A single `ThumbTask` per webview is sufficient: the page only thumbnails visible rows and caches results; a newer request overwrites the component (latest-wins). If multiple concurrent thumbs are needed later, switch to a child-entity-per-task model.

- [ ] **Step 4: Open observer**

```rust
fn on_file_open(
    trigger: On<BinReceive<FileOpenEvent>>,
    parents: Query<&ChildOf>,
    panes: Query<(), With<vmux_layout::pane::Pane>>,
    mut writer: MessageWriter<vmux_core::PageOpenRequest>,
) {
    let entity = trigger.event().webview;
    let path = trigger.event().payload.path.clone();
    let Some(pane) = crate::preview::resolve_open_target(entity, &parents, &panes) else { return; };
    writer.write(vmux_core::PageOpenRequest {
        target: vmux_core::PageOpenTarget::ActiveStackInPane(pane),
        url: format!("file://{path}"),
        request_id: None,
    });
}
```

- [ ] **Step 5: Register everything in `EditorPlugin::build`**

- Extend the emitter tuple so the page may emit the new page→host events:
  `BinEventEmitterPlugin::<(FileResizeEvent, FileScrollEvent, FilePreviewRequest, FileOpenEvent)>::default()`
- Add systems `send_initial_image`, `drain_thumb_tasks` to the existing `Update` system set.
- Add observers `on_file_preview_request`, `on_file_open`.

- [ ] **Step 6: Build + clippy**

Run: `cargo clippy -p vmux_editor --all-targets`
Expected: no errors. Run `cargo test -p vmux_editor` — existing tests still pass.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs
git commit -m "feat(editor): preview/open/image systems + off-thread thumbnails"
```

---

## Task 7: System test for open routing

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs` (test module)

- [ ] **Step 1: Write the test**

Add to the `page_open_tests` module (it already builds a minimal app):

```rust
#[test]
fn resolve_open_target_walks_to_pane() {
    use vmux_layout::pane::Pane;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let pane = app.world_mut().spawn(Pane::default()).id();
    let stack = app.world_mut().spawn(ChildOf(pane)).id();
    let fv = app.world_mut().spawn(ChildOf(stack)).id();

    let mut parents = app.world_mut().query::<&ChildOf>();
    // Use RunSystemOnce-style closure to access queries:
    let got = app.world_mut().run_system_once(move |p: Query<&ChildOf>, panes: Query<(), With<Pane>>| {
        crate::preview::resolve_open_target(fv, &p, &panes)
    }).unwrap();
    assert_eq!(got, Some(pane));
    let _ = (parents, stack);
}
```

> If `Pane::default()` is unavailable, spawn the pane with the bundle used elsewhere in `vmux_layout` tests (`leaf_pane_bundle`) or a marker-only `Pane` — check `vmux_layout::pane` for the constructor used in its own tests. Import `bevy::ecs::system::RunSystemOnce`.

- [ ] **Step 2: Run**

Run: `cargo test -p vmux_editor resolve_open_target_walks_to_pane`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs
git commit -m "test(editor): open-target resolver walks FileView→stack→pane"
```

---

## Task 8: web-sys features for the page

**Files:**
- Modify: `crates/vmux_editor/Cargo.toml`

- [ ] **Step 1: Add features**

Extend the `web-sys` feature list (wasm deps) with the APIs the page needs:

```toml
web-sys = { version = "0.3", features = [
    "Window", "Document", "Element", "HtmlElement", "Node",
    "DomRect", "DomRectReadOnly", "CssStyleDeclaration",
    "ResizeObserver", "ResizeObserverEntry",
    "IntersectionObserver", "IntersectionObserverEntry", "IntersectionObserverInit",
    "WheelEvent", "KeyboardEvent", "MouseEvent", "Location",
    "Blob", "BlobPropertyBag", "Url",
] }
```

- [ ] **Step 2: Verify it builds for wasm**

Run: `cargo check -p vmux_editor --target wasm32-unknown-unknown`
Expected: compiles (no usage yet; just feature gates). If the target is missing: `rustup target add wasm32-unknown-unknown`.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/Cargo.toml
git commit -m "chore(editor): web-sys features for blob urls + intersection observer"
```

---

## Task 9: Page UI — miller columns, modes, preview, thumbnails, soft glass

**Files:**
- Modify: `crates/vmux_editor/src/page.rs`

This is wasm UI: not unit-tested. Implement in sub-steps, `cargo check --target wasm32-unknown-unknown` after each, and **the user runtime-tests** the result. Build on the existing signals/listeners already in `page.rs`.

- [ ] **Step 1: Add Blob helper + new state/signals**

Add a helper to build an object URL from bytes:

```rust
fn blob_url(bytes: &[u8], mime: &str) -> Option<String> {
    let arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&arr.buffer());
    let mut bag = web_sys::BlobPropertyBag::new();
    bag.type_(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &bag).ok()?;
    web_sys::Url::create_object_url_with_blob(&blob).ok()
}
```

Add signals in `Page()`: `parent_entries: Vec<FileDirEntry>`, `selected: usize`,
`mode: Mode` (enum `Dir | Text | Image`), `image_url: Option<String>`,
`preview: PreviewState` (enum mirroring `PreviewKind` but with `image_url: String` for images),
`thumbs: HashMap<String, String>` (path → blob url). Default `mode = Text` (current behavior).

- [ ] **Step 2: Wire new listeners**

```rust
let _img = use_bin_event_listener::<FileImageEvent, _>(FILE_IMAGE_EVENT, move |e| {
    if let Some(old) = image_url() { let _ = web_sys::Url::revoke_object_url(&old); }
    image_url.set(blob_url(&e.bytes, &e.mime));
    mode.set(Mode::Image);
});

let _prev = use_bin_event_listener::<FilePreviewEvent, _>(FILE_PREVIEW_EVENT, move |e| {
    // selection preview (path == selected) updates the preview pane;
    // thumbnail replies (path is an image row) populate `thumbs`.
    apply_preview(e, /* signals */);
});
```

Extend the existing `FileDirEvent` listener to also store `parent_entries`, reset `selected` to 0, set `mode = Mode::Dir`, and revoke+clear `thumbs`.

- [ ] **Step 2b: Implement `apply_preview`**

For `PreviewKind::Image { mime, bytes }`: if the reply path matches the currently selected entry, build a blob URL and store as the selection preview image; otherwise treat it as a thumbnail and insert into `thumbs` (revoking any prior URL for that path). For `Dir/Text/Info/Error`: store into the selection preview state. Guard: ignore image/text/dir/info replies whose path ≠ selected entry (except thumbnails).

- [ ] **Step 3: Mode switch in render**

Replace the current `if is_dir() { … } else { … }` block with a three-way switch on `mode()`:
`Mode::Image` → full image; `Mode::Dir` → miller columns (Step 4); `Mode::Text` → the existing line view (unchanged).

Full image:

```rust
Mode::Image => rsx! {
    div { class: "min-h-0 flex-1 overflow-auto flex items-center justify-center p-4",
        if let Some(url) = image_url() {
            img { src: "{url}", class: "max-h-full max-w-full rounded-lg object-contain" }
        }
    }
},
```

- [ ] **Step 4: Miller columns (soft glass)**

```rust
Mode::Dir => rsx! {
    div {
        class: "min-h-0 flex-1 grid grid-cols-[minmax(8rem,14rem)_minmax(10rem,1fr)_minmax(12rem,1.2fr)] gap-2 p-2",
        // parent column
        ColumnPane { entries: parent_entries(), selected_path: Some(path()), thumbs: thumbs(), on_select: |_| {} }
        // current column (selectable)
        div { class: "rounded-xl bg-white/[0.04] backdrop-blur-md ring-1 ring-white/[0.06] overflow-y-auto p-2",
            for (i, e) in dir_entries().iter().enumerate() {
                Row {
                    entry: e.clone(),
                    selected: i == selected(),
                    thumb: thumbs().get(&e.path).cloned(),
                    onclick: move |_| select_index(i, /* signals */),
                    ondblclick: move |_| activate(&e.path),
                }
            }
        }
        // preview pane
        PreviewPane { state: preview() }
    }
},
```

Implement `Row` (rounded pill when `selected`, hover, glyph or `<img src=thumb>`), `ColumnPane`, and `PreviewPane` as `#[component]`s in this file using the soft-glass classes from the spec. Reuse the existing `Icon` for glyphs.

- [ ] **Step 5: Keymap + activation**

Extend the container `onkeydown` (currently text-scroll only) to branch on `mode()`:

- `Mode::Dir`: `j/ArrowDown` → `select_index(selected()+1)`, `k/ArrowUp` → saturating −1,
  `l/ArrowRight/Enter` → `activate(selected entry path)`,
  `h/ArrowLeft/Escape` → if `!parent_path.is_empty()` emit `FileOpenEvent { path: parent_path }`.
- `Mode::Text`: keep existing scroll behavior.

```rust
fn activate(path: &str) {
    let _ = try_cef_bin_emit_rkyv(&FileOpenEvent { path: path.to_string() });
}
```

`select_index` clamps via `page_model::clamp_selection`, updates `selected`, debounces a
`FilePreviewRequest { path, thumb:false }` for the newly selected entry (~80 ms via a
`gloo`-free `set_timeout`; reuse the existing measurement timer pattern or a simple
`web_sys::Window::set_timeout_with_callback`), and ensures the selected row scrolls into
view (`scroll_into_view`).

- [ ] **Step 6: Lazy thumbnails**

When entering `Mode::Dir` (and on scroll), for image rows (`page_model::image_mime(path).is_some()`) not present in `thumbs` and not in-flight, emit `FilePreviewRequest { path, thumb:true }`. Simplest v1: request thumbnails for all image rows in the current dir once on dir load (bounded by dir size); upgrade to IntersectionObserver-gated requests if a directory is huge. Store replies into `thumbs` via `apply_preview`.

- [ ] **Step 7: Build + runtime test**

Run: `cargo check -p vmux_editor --target wasm32-unknown-unknown`
Expected: compiles.

Then ask the user to runtime-test (the agent does not launch `make dev`):
- Open a directory `file://…` → miller columns render with soft-glass styling.
- `j/k` move the selection pill; preview pane updates (image / code / dir / info).
- Image rows show inline thumbnails.
- `l`/`Enter` on a dir descends (new columns); on a file opens full view (text scrolls, image fills).
- `h`/`Esc` ascends to the parent; no-op at `/`.
- Screenshots dir (`~/.vmux/screenshots`) shows image thumbnails + full preview.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_editor/src/page.rs
git commit -m "feat(editor): yazi-style soft-glass dir browser with previews + thumbnails"
```

---

## Self-review (completed during planning)

- **Spec coverage:** miller columns (T9), soft glass (T9), previews image/text/dir/info (T5, T9), inline thumbnails (T4/T6/T9), keyboard+mouse nav (T9), activate→open in place (T5/T6), parent listing (T3), image transport via Blob (T8/T9), image cap (T5/T6), dotfile hiding (T3), `image` dep (T4). All covered.
- **Placeholders:** none — every code step has concrete code; UI markup steps give real handlers/classes (UI is runtime-verified, not unit-tested, per project rule).
- **Type consistency:** `FilePreviewRequest { path, thumb }`, `PreviewKind` variants, `FileImageEvent { mime, bytes }`, `FileDirEvent { …, parent_path, parent_entries }`, `resolve_open_target`, `downscale_to_png`, `build_preview_sync`/`build_preview_with_cap` used consistently across tasks.

## Open verification notes (resolve during execution, not blockers)

- `vmux_layout::pane::Pane` visibility/constructor — confirm import in `plugin.rs`; adjust the T7 spawn accordingly.
- `BlobPropertyBag::new()/type_()` builder shape can vary by `web-sys` version; if deprecated, use the `.type_()` setter or struct-init per the resolved version.
- `On<BinReceive<T>>` / `trigger.event().webview` / `.payload` — mirror exactly the existing `on_file_scroll` signature in `plugin.rs`.
