# file:// media viewer — design

Date: 2026-06-28
Status: approved (pending spec review)
Branch: `file-media-viewer` (worktree `.worktrees/file-media`)

## Problem

Opening a media file via `file://` in the editor shows a blank text editor.
Images (png/jpg/gif/webp) are wired end-to-end since #132, but the bytes ride a
CEF **process message** (`CefBinaryValue` via `emit_event_bytes`), which silently
drops multi-MB payloads (e.g. a Retina screenshot png) — the page never receives
`FILE_IMAGE_EVENT` and stays in its default `Mode::Text` (no error toast, just a
text cursor). Video, audio, and PDF have zero support.

## Goals

- Render **images** (png, jpg/jpeg, gif, webp, avif, bmp, ico, svg) inline.
- Render **video** (mp4, m4v, mov, webm, ogv) with `<video controls>`, play + seek.
- Render **audio** (mp3, m4a/aac, wav, flac, ogg/opus) with `<audio controls>`.
- **PDF**: info card with "open externally" (no PDFium reliance in v1).
- No size cap: large video must stream (ranged reads), not OOM.
- Fix the blank-png regression as a consequence of the new transport.

## Non-goals (v1)

- heic/heif/tiff (need backend transcode) — phase 2, noted only.
- Inline PDF rendering via PDFium — fast-follow if desired.
- Migrating the dir-browser thumbnail/preview pane (stays on the existing small,
  capped IPC/blob path — payloads are tiny downscaled PNGs).
- True multi-range / `multipart/byteranges` (single-range only, which is what
  `<video>`/`<audio>` use).

## Approach: serve media over CEF's resource pipe (Transport B)

Media bytes do **not** go through the process-message channel. Instead the page
points media elements at the file's **own** `file://` URL with a raw marker:

```
file:///abs/path.mp4?vmux-raw=1
```

This is same-origin (the editor SPA's document origin is already `file://`), so no
new scheme/host and no cross-origin issues. The CEF resource handler intercepts
it, reads the requested byte range directly from disk, and streams it back with
`206 Partial Content`. The CEF C++ framework is prebuilt; the changes below are a
Rust recompile of the integration crates (`bevy_cef_core`, `bevy_cef`).

### Why this fixes the bug

The blank png is caused by the process-message size ceiling. Serving image bytes
over the resource pipe removes that channel entirely (and the 25 MB cap), so png
of any size renders.

## Data flow

1. **Classify** (`load_file_buffers`, vmux_editor): if the path is media (new
   classifier), insert `FileMedia { kind, mime }` — **no bytes** — and register
   the abs path in the media allowlist. Otherwise existing dir/text path.
2. **Emit** (`send_initial_media`, new): trigger `FILE_MEDIA_EVENT { kind, mime,
   url }` where `url = file:///abs/path.ext?vmux-raw=1`. (Reuses the existing
   `BinHostEmitEvent` host→page channel — tiny payload, no byte transfer.)
3. **Render** (page.rs): `Mode::Media(MediaKind)`:
   - `Image` → `<img src=url>`
   - `Video` → `<video controls src=url>`
   - `Audio` → `<audio controls src=url>`
   - `Pdf`   → info card (name, size, type) + "Open externally" button
4. **Fetch**: the media element requests `url`. CEF resource handler `open()`
   parses `Range`, sends `CefRequest { uri, range }`; the raw-file branch in
   `coming_request` reads only that range from disk and returns it; `read()`
   streams it to CEF as `206`.

## Components

### vmux_core (shared, no new crate)

- `media.rs`: `MediaKind { Image, Video, Audio, Pdf }`, `media_kind(path) ->
  Option<MediaKind>`, `media_mime(path) -> Option<&'static str>`. Single source of
  truth, replacing the three scattered ext maps (`vmux_editor::preview::image_mime`,
  `vmux_editor::page_model::image_mime`, and the image arm of
  `vmux_ui::file_icon`). Those call into the shared classifier.
- `event.rs`: `FileMediaEvent { kind: MediaKind, mime: String, url: String }`
  (serde + rkyv). Add `FILE_MEDIA_EVENT` const. Add `FileOpenExternalRequest {
  path: String }` (page→backend intent for the PDF "open externally" button).
- Remove `FileImageEvent` / `FILE_IMAGE_EVENT` once the image path is migrated
  (or keep as a thin alias if other callers exist — verify first).

### vmux_editor (backend)

- `plugin.rs`:
  - `load_file_buffers`: replace the `is_image_path` branch with the media
    classifier → insert `FileMedia { kind, mime }` (no read). Drop the 25 MB read.
  - `MediaAllowlist` Resource (`HashSet<PathBuf>` or `HashSet<String>` of abs
    paths currently open as media views). Insert on `FileMedia` spawn; remove on
    view despawn / navigation.
  - `send_initial_media` system (mirrors `send_initial_image`): emits
    `FILE_MEDIA_EVENT` once page is ready.
  - `on_file_open_external` observer: `BinReceive<FileOpenExternalRequest>` →
    validate path ∈ allowlist → `open`/`xdg-open` the file.
  - Delete `FileImage` component, `send_initial_image`, and the `FILE_IMAGE_EVENT`
    re-emit in `reload_changed_files` (replace with media re-emit).
- `preview.rs` / `page_model.rs`: image classification delegates to
  `vmux_core::media`. Dir-preview pane keeps its current small IPC/blob image path.

### vmux_editor (page.rs / page_model.rs)

- `Mode`: replace `Image` with `Media(MediaKind)`.
- `_media` listener for `FILE_MEDIA_EVENT`: set url + `Mode::Media(kind)`.
- Render arm per kind (above). Drop `blob_url` use for the main view (still used by
  the dir preview pane / thumbnails). Drop the `_img` listener.
- PDF card "Open externally" → `try_cef_bin_emit_rkyv(&FileOpenExternalRequest { path })`.

### patches/bevy_cef_core-0.5.2

- `browser_process/localhost.rs`:
  - `CefRequest`: add `range: Option<(u64, Option<u64>)>` (or reuse the existing
    parsed range type).
  - `asset_load_path_from_request_url_with`: for a `file://` URL with `vmux-raw=1`
    in the query, return a raw-disk uri (e.g. `vmuxraw://<abs>`) instead of the
    SPA shell. All other `file://` behavior unchanged.
  - `open()`: pass the parsed `range` into `CefRequest`.

### patches/bevy_cef-0.5.2

- `common/localhost/responser.rs` `coming_request`: branch on the `vmuxraw://`
  uri → read **only the requested byte range** from disk (`File::seek` + bounded
  read), set mime from the patch's existing `EXTENSION_MAP` (asset_loader.rs — do
  not add a patch→vmux_core dependency), return a
  `CefResponse` carrying the ranged bytes + total length so `HeadersResponser`
  emits correct `Content-Range`/`Content-Length`/`Accept-Ranges`. Non-raw uris
  unchanged (asset-server path).
  - `CefResponse` may need a `total_len` hint (or reuse existing range plumbing)
    so 206 headers report the full resource size, not the slice size. Verify
    `HeadersResponser`/`DataResponser` semantics and feed them pre-sliced data
    with the correct full length.

## Security

The page can only emit URLs the backend handed it, but a buggy/compromised page
must not read arbitrary disk. The raw-file branch serves a path **only if it is in
the `MediaAllowlist`** (abs paths currently open as media views). Anything else →
`404`. The allowlist Resource is read by `coming_request`; the editor plugin is
the sole writer. Paths are canonicalized before allowlist insert and before
compare to defeat `..`/symlink tricks.

## Error / fallback handling

- Non-allowlisted or missing file → `404` (element shows broken state; acceptable).
- Unsupported-but-detected codec (e.g. HEVC mov) → browser shows its own media
  error; best-effort, documented.
- PDF → always the info card (no inline attempt in v1).
- heic/tiff → fall through to existing "not a UTF-8 text file" path for now
  (phase 2 transcode). Acceptable; documented.

## Testing

Native (`cargo test -p vmux_core -p vmux_editor`, plus patch package checks):

- `vmux_core::media`: ext → kind + mime table (incl. case-insensitivity, unknown).
- `bevy_cef_core` localhost: `asset_load_path_from_request_url_with` maps
  `file:///a/b.mp4?vmux-raw=1` → raw uri; plain `file:///a/b.rs` → SPA shell
  (existing tests still pass); range parse threaded into `CefRequest`.
- Allowlist: raw request for a non-allowlisted path → 404; allowlisted → 200/206.
- Ranged read: `bytes=N-M` returns exactly that slice with correct length.
- page source-scrape tests (`vmux_layout` style.rs / tests/page_source.rs and any
  vmux_editor include_str! asserts) updated for the new render arms.

Runtime (user, one pass at end — per "finish then test"):

- the original blank screenshot png; a large Retina png; jpg, svg, gif, webp;
  mp4 (play + scrub/seek); mov; mp3 (play); pdf (card + open externally).

Temporary: one **default-on** log of the raw-request uri + range + served length
during dev; stripped before commit (AGENTS.md debugging rule).

## Risks / notes

- Touches patched CEF crates → run their package checks (AGENTS.md). `cargo fmt`
  reformats `patches/` — `git checkout -- patches/` unrelated churn, commit only
  intended edits (but DO commit the intended patch edits).
- Worktree has its own target dir (do not share CARGO_TARGET_DIR across worktrees
  — CEF cmake pins absolute paths).
- `<video>`/`<audio>` seeking depends on correct 206 + `Content-Range`; get the
  header math right (full length vs slice length) or scrubbing breaks.
- Confirm `vmux-raw=1` survives as a sub-resource request through the CEF resource
  handler (query string reaches `open()` `request.url()`).

## Build sequence

1. `vmux_core::media` classifier + `FileMediaEvent` / `FileOpenExternalRequest`.
2. Patch: URL mapping + range-in-`CefRequest` + ranged-disk raw branch.
3. vmux_editor backend: `FileMedia`, allowlist, `send_initial_media`,
   open-external observer; delete old image path.
4. page.rs: `Mode::Media`, listeners, render arms, PDF card.
5. Tests (native) + fix source-scrape asserts.
6. One runtime pass (user).
