# file:// media viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Execution note:** Implement **inline** (not subagent-driven). vmux CEF builds are large and long-lived subagents drop sockets (project memory: "Subagent CEF build fragility"). Keep a warm `cargo build` in the worktree's own target dir.

**Goal:** Render images, video, audio, and PDF when a `file://` media file is opened in the editor, replacing the byte-over-IPC image path that silently drops multi-MB payloads.

**Architecture:** Media bytes are served over CEF's resource pipe (range-streamed from disk) via the file's own `file://` URL plus a `?vmux-raw=1` marker — same-origin, no new scheme. The backend emits a tiny `FILE_MEDIA_EVENT{kind,mime,url}` (no bytes); the page renders the matching HTML element pointed at the raw URL. A process-global allowlist (synced from live `FileMedia` entities) gates which paths the handler will read.

**Tech Stack:** Rust, Bevy ECS, Dioxus (WASM page), patched `bevy_cef_core` (CEF integration — Rust recompile, not C++), rkyv IPC.

---

## File structure

- `crates/vmux_core/src/media.rs` *(new)* — shared `MediaKind` + ext→kind/mime classifier. One source of truth.
- `crates/vmux_core/src/lib.rs` — register `pub mod media;`.
- `crates/vmux_core/src/event.rs` — `FileMediaEvent`, `FileOpenExternalRequest`, new event-name consts.
- `patches/bevy_cef_core-0.5.2/src/util.rs` — media allowlist global, `raw_media_request(url)`, `raw_media_mime(path)`, `build_raw_media_response(...)` pure helper.
- `patches/bevy_cef_core-0.5.2/src/browser_process/localhost.rs` — raw-media branch in `ImplResourceHandler::open`.
- `crates/vmux_editor/src/plugin.rs` — `FileMedia` component, classify in `load_file_buffers`, `sync_media_allowlist`, `send_initial_media`, open-external observer; delete old image path (final task).
- `crates/vmux_editor/src/preview.rs` — `image_mime`/`is_image_path` delegate to `vmux_core::media`.
- `crates/vmux_editor/src/page_model.rs` — `image_mime` delegate (dir thumbnails stay image-only).
- `crates/vmux_editor/src/page.rs` — `Mode::Media(MediaKind)`, `_media` listener, render arms, PDF card; drop `_img`/`Mode::Image`.

All paths below are relative to the worktree root `/Users/junichi.sugiura/Projects/github.com/vmux-ai/vmux/.worktrees/file-media`.

---

### Task 1: Shared media classifier in `vmux_core`

**Files:**
- Create: `crates/vmux_core/src/media.rs`
- Modify: `crates/vmux_core/src/lib.rs` (add module after `pub mod icon;`, keep alpha-ish order — insert `pub mod media;` near line 7)

- [ ] **Step 1: Write the failing test** — append to the new file `crates/vmux_core/src/media.rs`:

```rust
//! Extension-based media classification shared by the editor backend and the
//! `file://` page: decides whether a path is an image, video, audio, or PDF and
//! what MIME type to serve it as.

use serde::{Deserialize, Serialize};

/// The kind of media a `file://` path resolves to.
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
pub enum MediaKind {
    /// Raster or vector image rendered with `<img>`.
    Image,
    /// Video rendered with `<video controls>`.
    Video,
    /// Audio rendered with `<audio controls>`.
    Audio,
    /// PDF shown via an info card (no inline render in v1).
    Pdf,
}

fn ext_of(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// MIME type for a media path, or `None` if the extension is not recognized media.
pub fn media_mime(path: &str) -> Option<&'static str> {
    Some(match ext_of(path).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" | "opus" => "audio/ogg",
        "pdf" => "application/pdf",
        _ => return None,
    })
}

/// Classify a path into a [`MediaKind`], or `None` if not media.
pub fn media_kind(path: &str) -> Option<MediaKind> {
    Some(match ext_of(path).as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "ico" | "svg" => {
            MediaKind::Image
        }
        "mp4" | "m4v" | "mov" | "webm" | "ogv" => MediaKind::Video,
        "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "opus" => MediaKind::Audio,
        "pdf" => MediaKind::Pdf,
        _ => return None,
    })
}

/// MIME type for an image path only (used by the dir-browser thumbnail path,
/// which renders raster previews and must not treat video/audio/pdf as images).
pub fn image_mime(path: &str) -> Option<&'static str> {
    match media_kind(path) {
        Some(MediaKind::Image) => media_mime(path),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_each_kind() {
        assert_eq!(media_kind("/a/b/c.PNG"), Some(MediaKind::Image));
        assert_eq!(media_kind("x.svg"), Some(MediaKind::Image));
        assert_eq!(media_kind("clip.mp4"), Some(MediaKind::Video));
        assert_eq!(media_kind("v.MOV"), Some(MediaKind::Video));
        assert_eq!(media_kind("song.flac"), Some(MediaKind::Audio));
        assert_eq!(media_kind("doc.pdf"), Some(MediaKind::Pdf));
        assert_eq!(media_kind("main.rs"), None);
        assert_eq!(media_kind("no_ext"), None);
    }

    #[test]
    fn mime_matches_kind() {
        assert_eq!(media_mime("a.webp"), Some("image/webp"));
        assert_eq!(media_mime("a.mp4"), Some("video/mp4"));
        assert_eq!(media_mime("a.mp3"), Some("audio/mpeg"));
        assert_eq!(media_mime("a.pdf"), Some("application/pdf"));
        assert_eq!(media_mime("a.rs"), None);
    }

    #[test]
    fn image_mime_excludes_non_images() {
        assert_eq!(image_mime("a.png"), Some("image/png"));
        assert_eq!(image_mime("a.mp4"), None);
        assert_eq!(image_mime("a.pdf"), None);
    }
}
```

Then add to `crates/vmux_core/src/lib.rs` (near the other `pub mod` lines, ~line 7):

```rust
pub mod media;
```

- [ ] **Step 2: Run test to verify it fails (before lib.rs edit) / passes (after)**

Run: `cargo test -p vmux_core media::`
Expected: PASS (3 tests). If you ran before adding the module to lib.rs, expect a compile error "file not found for module `media`".

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_core/src/media.rs crates/vmux_core/src/lib.rs
git commit -m "feat(core): shared file media classifier (image/video/audio/pdf)"
```

---

### Task 2: New wire events in `vmux_core::event`

**Files:**
- Modify: `crates/vmux_core/src/event.rs` (consts block ~line 26; struct defs near `FileImageEvent` ~line 300)

- [ ] **Step 1: Add event-name consts** — in `crates/vmux_core/src/event.rs`, after the `FILE_IMAGE_EVENT` line (26):

```rust
pub const FILE_MEDIA_EVENT: &str = "file_media";
pub const FILE_OPEN_EXTERNAL_EVENT: &str = "file_open_external";
```

- [ ] **Step 2: Add struct defs** — after the `FileImageEvent` struct (ends ~line 303). `MediaKind` comes from the sibling module:

```rust
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
pub struct FileMediaEvent {
    pub kind: crate::media::MediaKind,
    pub mime: String,
    /// Raw-media URL (`file://…?vmux-raw=1`) for the media element `src`.
    pub url: String,
    /// Absolute filesystem path, for the PDF "open externally" intent.
    pub abs_path: String,
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
pub struct FileOpenExternalRequest {
    pub path: String,
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_core`
Expected: clean (no errors).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_core/src/event.rs
git commit -m "feat(core): FileMediaEvent + FileOpenExternalRequest wire types"
```

---

### Task 3: CEF-core util — allowlist global, raw URL parse, raw mime

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/src/util.rs` (append; uses `std::sync::RwLock`, `LazyLock`, `PathBuf`/`Path` already imported per existing `webview_debug_log_path` usage — verify imports, add `use std::collections::HashSet;` and `use std::sync::RwLock;` if absent)

- [ ] **Step 1: Write the failing test + impl** — append to `crates/.../util.rs` (patch path `patches/bevy_cef_core-0.5.2/src/util.rs`):

```rust
use std::collections::HashSet;
use std::sync::RwLock;

static MEDIA_ALLOWLIST: LazyLock<RwLock<HashSet<PathBuf>>> =
    LazyLock::new(|| RwLock::new(HashSet::new()));

/// Replace the set of absolute paths the raw-media resource handler is allowed to
/// read. Paths are canonicalized; callers pass the live set each frame.
pub fn set_media_allowlist(paths: HashSet<PathBuf>) {
    let canon: HashSet<PathBuf> = paths
        .into_iter()
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .collect();
    if let Ok(mut w) = MEDIA_ALLOWLIST.write() {
        *w = canon;
    }
}

/// Whether `path` is currently allowed to be served as raw media.
pub fn is_media_path_allowed(path: &Path) -> bool {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    MEDIA_ALLOWLIST
        .read()
        .map(|s| s.contains(&canon))
        .unwrap_or(false)
}

/// If `url` is a `file://` URL carrying the `vmux-raw=1` marker, return its
/// decoded absolute path. Otherwise `None` (normal document/asset navigation).
pub fn raw_media_request(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let (path_part, query) = match rest.split_once('?') {
        Some((p, q)) => (p, q),
        None => return None,
    };
    if !query.split('&').any(|kv| kv == "vmux-raw=1") {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(path_part)
        .decode_utf8()
        .ok()?;
    let path = PathBuf::from(decoded.as_ref());
    path.is_absolute().then_some(path)
}

/// MIME type for raw-media serving (kept local to avoid a patch→vmux_core dep).
pub fn raw_media_mime(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" | "opus" => "audio/ogg",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod media_raw_tests {
    use super::*;

    #[test]
    fn raw_marker_detected_and_decoded() {
        assert_eq!(
            raw_media_request("file:///a/b/Screenshot%20x.png?vmux-raw=1"),
            Some(PathBuf::from("/a/b/Screenshot x.png"))
        );
        assert_eq!(
            raw_media_request("file:///a/b/c.mp4?foo=1&vmux-raw=1"),
            Some(PathBuf::from("/a/b/c.mp4"))
        );
    }

    #[test]
    fn non_raw_returns_none() {
        assert_eq!(raw_media_request("file:///a/b/c.png"), None);
        assert_eq!(raw_media_request("file:///a/b/c.png?other=1"), None);
        assert_eq!(raw_media_request("vmux://files/index.html?vmux-raw=1"), None);
    }

    #[test]
    fn mime_lookup() {
        assert_eq!(raw_media_mime(Path::new("a.mp4")), "video/mp4");
        assert_eq!(raw_media_mime(Path::new("a.png")), "image/png");
        assert_eq!(raw_media_mime(Path::new("a.bin")), "application/octet-stream");
    }
}
```

Confirm `percent_encoding` is a dependency of `bevy_cef_core` — check `patches/bevy_cef_core-0.5.2/Cargo.toml`. If absent, add `percent_encoding = "2"` to its `[dependencies]`.

- [ ] **Step 2: Run tests**

Run: `cargo test -p bevy_cef_core media_raw_tests`
Expected: PASS (3 tests). Fix any missing imports (`HashSet`, `RwLock`, `PathBuf`, `Path`, `LazyLock`, `percent_encoding`).

- [ ] **Step 3: Commit**

```bash
git add patches/bevy_cef_core-0.5.2/src/util.rs patches/bevy_cef_core-0.5.2/Cargo.toml
git commit -m "feat(cef): media allowlist + raw-media url/mime helpers"
```

---

### Task 4: CEF-core resource handler — raw-media branch

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/src/util.rs` (add testable `build_raw_media_response`)
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/localhost.rs` (`ImplResourceHandler::open`)

- [ ] **Step 1: Write the failing test + pure helper** — append to `patches/bevy_cef_core-0.5.2/src/util.rs`:

```rust
/// Result of resolving a raw-media request: HTTP status, the bytes to stream
/// (already sliced to the requested range), and the response headers.
pub struct RawMediaResponse {
    pub status: u32,
    pub data: Vec<u8>,
    pub headers: Vec<(String, String)>,
    pub mime: String,
}

/// Read the requested byte range of an allowlisted media file from disk and build
/// the response. `range` is the parsed single byte-range `(start, end_exclusive?)`.
/// Returns a 404 response (empty body) if the path is not allowlisted or unreadable.
pub fn build_raw_media_response(
    path: &Path,
    range: &Option<(usize, Option<usize>)>,
) -> RawMediaResponse {
    use std::io::{Read, Seek, SeekFrom};

    let cors = vec![
        ("Access-Control-Allow-Origin".to_string(), "*".to_string()),
        ("Access-Control-Allow-Methods".to_string(), "*".to_string()),
        ("Access-Control-Allow-Headers".to_string(), "*".to_string()),
    ];
    if !is_media_path_allowed(path) {
        return RawMediaResponse {
            status: 404,
            data: Vec::new(),
            headers: cors,
            mime: "text/plain".to_string(),
        };
    }
    let mime = raw_media_mime(path).to_string();
    let (mut file, total) = match std::fs::File::open(path).and_then(|f| {
        let len = f.metadata()?.len() as usize;
        Ok((f, len))
    }) {
        Ok(v) => v,
        Err(_) => {
            return RawMediaResponse {
                status: 404,
                data: Vec::new(),
                headers: cors,
                mime: "text/plain".to_string(),
            };
        }
    };

    let mut headers = cors;
    headers.push(("Accept-Ranges".to_string(), "bytes".to_string()));

    let (start, end) = match range {
        Some((s, e)) => (*s, e.unwrap_or(total).min(total)),
        None => (0, total),
    };
    let start = start.min(total);
    let end = end.max(start);
    let len = end - start;

    let mut data = vec![0u8; len];
    if len > 0 {
        if file.seek(SeekFrom::Start(start as u64)).is_err() {
            return RawMediaResponse {
                status: 404,
                data: Vec::new(),
                headers,
                mime,
            };
        }
        if let Err(_) = file.read_exact(&mut data) {
            // Short read (file shrank) — serve what we got.
            data.clear();
        }
    }

    let status = if range.is_some() {
        headers.push((
            "Content-Range".to_string(),
            format!("bytes {start}-{end}/{total}"),
        ));
        206
    } else {
        200
    };

    RawMediaResponse {
        status,
        data,
        headers,
        mime,
    }
}

#[cfg(test)]
mod raw_response_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn denied_when_not_allowlisted() {
        let r = build_raw_media_response(Path::new("/no/such/file.png"), &None);
        assert_eq!(r.status, 404);
        assert!(r.data.is_empty());
    }

    #[test]
    fn serves_range_with_content_range_header() {
        let dir = std::env::temp_dir().join("vmux_raw_test");
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("blob.bin");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(&(0u8..100).collect::<Vec<u8>>()).unwrap();
        drop(f);

        let mut set = HashSet::new();
        set.insert(p.clone());
        set_media_allowlist(set);

        let r = build_raw_media_response(&p, &Some((10, Some(20))));
        assert_eq!(r.status, 206);
        assert_eq!(r.data, (10u8..20).collect::<Vec<u8>>());
        assert!(r
            .headers
            .iter()
            .any(|(k, v)| k == "Content-Range" && v == "bytes 10-20/100"));

        set_media_allowlist(HashSet::new());
        let _ = std::fs::remove_file(&p);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p bevy_cef_core raw_response_tests`
Expected: PASS (2 tests).

- [ ] **Step 3: Wire the helper into `open()`** — in `patches/bevy_cef_core-0.5.2/src/browser_process/localhost.rs`, inside `ImplResourceHandler::open`, immediately after `let url = request.url().into_string();` (currently line ~319) and before computing `uri`, insert the raw branch. The `range` variable is already parsed above (line ~311).

```rust
        if let Some(media_path) = crate::util::raw_media_request(&url) {
            let headers_responser = self.headers.clone();
            let data_responser = self.data.clone();
            IoTaskPool::get()
                .spawn(async move {
                    let resp = crate::util::build_raw_media_response(&media_path, &range);
                    {
                        let mut h = headers_responser.lock().unwrap();
                        h.mime_type = resp.mime;
                        h.status_code = resp.status;
                        h.response_length = resp.data.len();
                        h.headers = resp.headers;
                    }
                    let n = resp.data.len();
                    data_responser
                        .lock()
                        .unwrap()
                        .prepare(resp.data, &Some((0, Some(n))));
                    callback.cont();
                })
                .detach();
            return 1;
        }
```

Note: `callback` is already `cloned()` above (line ~312) and `range` is in scope. `HeadersResponser` fields (`mime_type`, `status_code`, `response_length`, `headers`) are all `pub`. `DataResponser::prepare(slice, &Some((0, Some(n))))` streams the whole pre-sliced buffer.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p bevy_cef_core`
Expected: clean. (CEF FFI types compile; the branch only uses already-imported `IoTaskPool` and the cloned responser `Arc`s.)

- [ ] **Step 5: Commit**

```bash
git add patches/bevy_cef_core-0.5.2/src/util.rs patches/bevy_cef_core-0.5.2/src/browser_process/localhost.rs
git commit -m "feat(cef): range-stream allowlisted raw media in resource handler"
```

---

### Task 5: Editor backend — `FileMedia`, classify, allowlist sync, emit

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs` (component near line 50; `load_file_buffers` ~232; new systems; registration ~1471)
- Modify: `crates/vmux_editor/src/preview.rs` (`image_mime`/`is_image_path` delegate)

- [ ] **Step 1: Add the `FileMedia` component** — in `crates/vmux_editor/src/plugin.rs`, after the `FileImage` struct (line ~54):

```rust
#[derive(Component, Clone, Debug)]
pub struct FileMedia {
    pub kind: vmux_core::media::MediaKind,
    pub mime: String,
}
```

- [ ] **Step 2: Classify media in `load_file_buffers`** — replace the existing image branch (the `if preview::is_image_path(&fv.path) { ... continue; }` block, lines ~232–255) with:

```rust
        if let Some(kind) = vmux_core::media::media_kind(&fv.path.to_string_lossy()) {
            let mime = vmux_core::media::media_mime(&fv.path.to_string_lossy())
                .unwrap_or("application/octet-stream")
                .to_string();
            commands.entity(entity).insert(FileMedia { kind, mime });
            continue;
        }
```

- [ ] **Step 3: Add allowlist sync + media emit systems** — add these functions in `crates/vmux_editor/src/plugin.rs` (near `send_initial_image`, ~line 576):

```rust
fn sync_media_allowlist(media: Query<&FileView, With<FileMedia>>) {
    let paths: std::collections::HashSet<std::path::PathBuf> =
        media.iter().map(|fv| fv.path.clone()).collect();
    bevy_cef_core::util::set_media_allowlist(paths);
}

fn raw_media_url(path: &std::path::Path) -> String {
    let mut url = url::Url::from_file_path(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", path.to_string_lossy()));
    url.push_str("?vmux-raw=1");
    url
}

fn send_initial_media(
    q: Query<(Entity, &FileView, &FileMedia), ReadyUnsentMeta>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, media) in &q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_MEDIA_EVENT,
            &FileMediaEvent {
                kind: media.kind,
                mime: media.mime.clone(),
                url: raw_media_url(&fv.path),
                abs_path: fv.path.to_string_lossy().into_owned(),
            },
        ));
        commands.entity(entity).insert(FileInitialMetaSent);
    }
}
```

`bevy_cef_core` is already an indirect dep via `bevy_cef`; confirm `vmux_editor/Cargo.toml` can name `bevy_cef_core` (it is re-exported as `bevy_cef::...prelude` — if `bevy_cef_core` is not a direct dep, call through `bevy_cef::prelude` re-export or add `bevy_cef_core` to `[dependencies]`). Check with `grep bevy_cef_core crates/vmux_editor/Cargo.toml`; add if missing.

- [ ] **Step 4: Register the new systems** — in the `.add_systems(Update, ( ... ))` tuple (line ~1473), add `send_initial_media,` next to `send_initial_image,` and `sync_media_allowlist,` to the tuple.

- [ ] **Step 5: Delegate `preview.rs` image helpers** — in `crates/vmux_editor/src/preview.rs`, replace the body of `image_mime` (lines 12–25) so it delegates, keeping the `&Path` signature:

```rust
pub fn image_mime(path: &Path) -> Option<&'static str> {
    vmux_core::media::image_mime(&path.to_string_lossy())
}
```

Leave `is_image_path` (calls `image_mime`) unchanged.

- [ ] **Step 6: Verify it compiles + existing preview tests pass**

Run: `cargo test -p vmux_editor preview::`
Expected: PASS (existing `build_preview_*` tests still green — image set unchanged for png).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs crates/vmux_editor/src/preview.rs crates/vmux_editor/Cargo.toml
git commit -m "feat(editor): FileMedia classify + allowlist sync + FILE_MEDIA_EVENT"
```

---

### Task 6: Editor backend — open-external intent

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs` (observer + registration ~1450, ~1490)

- [ ] **Step 1: Add the receiver observer** — add in `crates/vmux_editor/src/plugin.rs` near `on_file_open` (~line 675):

```rust
fn on_file_open_external(
    trigger: On<BinReceive<FileOpenExternalRequest>>,
    views: Query<&FileView, With<FileMedia>>,
) {
    let entity = trigger.event().webview;
    let Ok(fv) = views.get(entity) else {
        return;
    };
    let req_path = std::path::PathBuf::from(&trigger.event().payload.path);
    if fv.path != req_path {
        return;
    }
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(target_os = "macos"))]
    let program = "xdg-open";
    let _ = std::process::Command::new(program).arg(&req_path).spawn();
}
```

- [ ] **Step 2: Register the received type + observer** — add `FileOpenExternalRequest` to the second `BinEventEmitterPlugin::<(...)>` tuple (line ~1462, currently 3 entries):

```rust
            .add_plugins(BinEventEmitterPlugin::<(
                FileCompletionRequest,
                FileGotoRequest,
                FileCompletionCommit,
                FileOpenExternalRequest,
            )>::default())
```

And add the observer near the other `.add_observer(...)` calls (~line 1490):

```rust
            .add_observer(on_file_open_external)
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_editor`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs
git commit -m "feat(editor): open-external intent for PDF/media files"
```

---

### Task 7: Page — `Mode::Media` rendering

**Files:**
- Modify: `crates/vmux_editor/src/page.rs` (Mode enum ~23; listeners ~398; render arm ~705)
- Modify: `crates/vmux_editor/src/page_model.rs` (`image_mime` delegate ~58)

- [ ] **Step 1: Delegate page-side `image_mime`** — in `crates/vmux_editor/src/page_model.rs`, replace `image_mime` body (lines 58–67):

```rust
pub fn image_mime(path: &str) -> Option<&'static str> {
    vmux_core::media::image_mime(path)
}
```

(Dir-browser thumbnails stay image-only — correct, `image_mime` excludes video/audio/pdf.)

- [ ] **Step 2: Replace the `Mode` enum** — in `crates/vmux_editor/src/page.rs` (lines 23–28):

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Dir,
    Text,
    Media(vmux_core::media::MediaKind),
}
```

- [ ] **Step 3: Add media state + listener; remove the `_img` listener** — in `Page()`:
  - Add a signal near `image_url` (line ~253): `let mut media = use_signal(|| Option::<FileMediaEvent>::None);`
  - Replace the `_img` listener block (lines ~398–405) with:

```rust
    let _media = use_bin_event_listener::<FileMediaEvent, _>(FILE_MEDIA_EVENT, move |e| {
        clear_blob_state(image_url, preview, thumbs);
        let kind = e.kind;
        media.set(Some(e));
        mode.set(Mode::Media(kind));
        diagnostics.set(Vec::new());
        hover_diag.set(None);
        lsp_status.set(None);
    });
```

  - In the `_meta` and `_dir` listeners, after their existing `clear_blob_state(...)` calls, add `media.set(None);` so navigating away from media clears it.

- [ ] **Step 4: Replace the `Mode::Image` render arm** — in the `match mode()` block (lines ~705–712), replace the `Mode::Image => ...` arm with:

```rust
                Mode::Media(kind) => rsx! {
                    div { class: "flex min-h-0 flex-1 items-center justify-center overflow-auto p-4",
                        if let Some(m) = media() {
                            match kind {
                                vmux_core::media::MediaKind::Image => rsx! {
                                    img { src: "{m.url}", class: "max-h-full max-w-full rounded-xl object-contain shadow-[0_0_30px_-8px_rgba(34,211,238,0.4)] ring-1 ring-cyan-400/20" }
                                },
                                vmux_core::media::MediaKind::Video => rsx! {
                                    video {
                                        src: "{m.url}",
                                        controls: true,
                                        autoplay: false,
                                        class: "max-h-full max-w-full rounded-xl shadow-[0_0_30px_-8px_rgba(34,211,238,0.4)] ring-1 ring-cyan-400/20",
                                    }
                                },
                                vmux_core::media::MediaKind::Audio => rsx! {
                                    audio { src: "{m.url}", controls: true, class: "w-2/3" }
                                },
                                vmux_core::media::MediaKind::Pdf => {
                                    let display = path();
                                    let abs = m.abs_path.clone();
                                    rsx! {
                                        div { class: "flex flex-col items-center gap-3 rounded-2xl bg-white/[0.03] px-8 py-6 ring-1 ring-inset ring-cyan-400/15 backdrop-blur-2xl",
                                            span { class: "text-xs uppercase tracking-wide text-foreground/70", "PDF" }
                                            span { class: "max-w-md truncate text-sm text-foreground/90", "{display}" }
                                            button {
                                                class: "rounded-lg bg-cyan-400/15 px-3 py-1.5 text-xs font-semibold text-cyan-200 hover:bg-cyan-400/25",
                                                onclick: move |_| {
                                                    let _ = try_cef_bin_emit_rkyv(&FileOpenExternalRequest { path: abs.clone() });
                                                },
                                                "Open externally"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
```

Note: `m` (the `FileMediaEvent`) is in scope inside `if let Some(m) = media()`, so the PDF button reads `m.abs_path` directly — the backend `on_file_open_external` matches it against `fv.path`. The `<img>/<video>/<audio>` `src` is `m.url` (the `?vmux-raw=1` URL).

- [ ] **Step 5: Drop the now-unused image plumbing in the page**
  - Remove the `image_url` usages tied to the main view if they become dead, but **keep `clear_blob_state`, `blob_url`, `thumbs`, `preview`** (still used by the dir browser). `image_url` is still set by `clear_blob_state`; leave the signal in place to minimize churn (it now only ever holds dir-preview-cleared state). Do not delete `blob_url`.

- [ ] **Step 6: Verify the page typechecks (wasm target)**

Run: `cargo check -p vmux_editor --target wasm32-unknown-unknown`
Expected: clean. (Per project memory, full build runs Dioxus bundling; `check` is enough to typecheck the page.)

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_editor/src/page.rs crates/vmux_editor/src/page_model.rs
git commit -m "feat(editor): page renders Mode::Media (image/video/audio/pdf)"
```

---

### Task 8: Remove the old image byte-IPC path

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs` (delete `FileImage`, `send_initial_image`, registration entry, `reload_changed_files` image re-emit, `on_file_open` `.remove::<FileImage>()`)
- Modify: `crates/vmux_core/src/event.rs` (delete `FileImageEvent`, `FILE_IMAGE_EVENT`)

- [ ] **Step 1: Repoint `reload_changed_files`** — find the block that re-emits `FILE_IMAGE_EVENT` on external change (plugin.rs ~815–836). Replace it to re-emit media with a cache-busting nonce so the `<img>/<video>` refetches:

```rust
        if let Some(media) = world_media {
            // (inside the existing changed-file loop, when the entity has FileMedia)
            let mut url = raw_media_url(&fv.path);
            url.push_str(&format!("&v={}", change_nonce));
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                FILE_MEDIA_EVENT,
                &FileMediaEvent {
                    kind: media.kind,
                    mime: media.mime.clone(),
                    abs_path: fv.path.to_string_lossy().into_owned(),
                    url,
                },
            ));
        }
```

Read the actual `reload_changed_files` body first and adapt: query `&FileMedia` alongside the existing components, derive `change_nonce` from any existing counter or a per-call `u32` (e.g. reuse the watch event count; if none, a `Local<u32>` incremented per emit). Remove the old `FileImage`/`FILE_IMAGE_EVENT` re-read+emit.

- [ ] **Step 2: Delete the dead image path**
  - Remove `send_initial_image` fn and its entry in the `add_systems` tuple.
  - Remove the `FileImage` struct (plugin.rs ~50–54).
  - Remove `.remove::<FileImage>()` in `on_file_open` (~701) and replace with `.remove::<FileMedia>()`.
  - In `crates/vmux_core/src/event.rs`, delete `FileImageEvent` (300–303) and `FILE_IMAGE_EVENT` const (26).

- [ ] **Step 3: Find any stragglers**

Run: `cargo build -p vmux_editor 2>&1 | rg "FileImage|FILE_IMAGE" -n` (and `rg -n "FileImage|FILE_IMAGE_EVENT" crates/`)
Expected: no references remain. Fix each compile error the deletion surfaces.

- [ ] **Step 4: Verify workspace compiles + editor tests pass**

Run: `cargo test -p vmux_core -p vmux_editor`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs crates/vmux_core/src/event.rs
git commit -m "refactor(editor): remove byte-IPC image path, superseded by raw media"
```

---

### Task 9: Source-scrape tests, full checks, runtime pass

**Files:**
- Modify (if broken): `crates/vmux_layout/src/page.rs` style/source tests, `crates/vmux_layout/tests/page_source.rs`, any `vmux_editor` `include_str!` asserts (project memory: refactors to page.rs break these text-assert tests).

- [ ] **Step 1: Run the source-scrape suites**

Run: `cargo test -p vmux_layout` and `cargo test -p vmux_editor`
Expected: PASS. If a text-assert fails because it scraped an old `Mode::Image`/`file_image` string, update the expected substring to the new render (`<video`, `file_media`, etc.).

- [ ] **Step 2: Patched-crate package checks** (AGENTS.md: changed patched CEF crates)

Run: `cargo test -p bevy_cef_core` then `cargo clippy -p bevy_cef_core -p vmux_core -p vmux_editor --all-targets`
Expected: green. Fix clippy.

- [ ] **Step 3: fmt without polluting patches**

Run: `cargo fmt` then `git diff --name-only patches/ | rg -v 'bevy_cef_core-0.5.2/(src/util.rs|src/browser_process/localhost.rs)' | xargs -r git checkout --`
Expected: only the two intentionally-edited patch files remain modified by fmt; unrelated patch churn reverted. (Project memory: `cargo fmt` reformats vendored `patches/` crates.)

- [ ] **Step 4: Build the app once (warm target)**

Run: `cargo build -p vmux_desktop` (let it complete; CEF integration crates recompile).
Expected: success.

- [ ] **Step 5: Runtime verification (user drives — single pass, per "finish then test")**

Open each via `file://` in the editor and confirm:
  - the original blank screenshot png now renders;
  - a large Retina png; a jpg; an svg; a gif; an animated webp;
  - an `.mp4` (plays, **scrub/seek works** — confirms 206/Content-Range);
  - a `.mov`; an `.mp3` (plays); a `.pdf` (card shows + "Open externally" launches Preview).

- [ ] **Step 6: Strip diagnostics + final commit**

Remove any temporary default-on logs added during debugging. Then:

```bash
git add -A
git commit -m "test(editor): media viewer source-scrape + checks"
```

---

## Self-review notes

- **Spec coverage:** images (Task 1/5/7), video/audio (1/7), PDF card + open-external (6/7), no-cap ranged reads (3/4), allowlist security (3/5), shared classifier (1/5/7), blank-png fix (transport change, Tasks 4–5–7), tests (each task + 9). All covered.
- **Type consistency:** `MediaKind` defined once in `vmux_core::media`, used by `FileMediaEvent`, `FileMedia`, page `Mode::Media`. `FileMediaEvent` fields `{kind,mime,url,abs_path}` consistent across emit (Task 5/8) and page consume (Task 7). `build_raw_media_response` signature stable between Tasks 4 and its caller.
- **Range type:** the patch uses `(usize, Option<usize>)` (from `parse_bytes_single_range`) — all new helpers match that exact type.
- **Known v1 limits (documented in spec):** heic/tiff fall through to text-error path; a no-`Range` GET of a huge file reads it whole (browsers send `Range` for media, so rare); PDF is a card, not inline.
