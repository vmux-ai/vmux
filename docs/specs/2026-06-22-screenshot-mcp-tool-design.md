# Screenshot MCP Tool — Design

Date: 2026-06-22
Status: Approved (pending spec review)

## Goal

Expose an MCP tool, `screenshot`, so an agent running inside a vmux terminal
pane can capture the vmux window and *see* the result. This lets the agent
test-drive its own UI work: make a change, screenshot, verify visually, iterate.

## Why OS-level capture

Browse-mode browser panes render via **windowed CEF**: a native CEF `NSView`
attached as a child subview of the main window
(`patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs` — `set_as_child`
+ `parent_view` + `addSubview`). Its pixels live on a separate GPU surface
composited by macOS, not in Bevy's framebuffer. A Bevy render-target readback
would therefore capture the layout chrome, terminals, and 3D-mode OSR content,
but leave browser panes as blank holes.

Because windowed CEF is a **child subview of the one main `NSWindow`** (not a
separate top-level window), capturing that single window by its `CGWindowID`
yields a faithful, fully-composited image including browser content.

Capture API: **ScreenCaptureKit** (`SCScreenshotManager`), the Apple-supported
path on macOS 14+ (the project runs on macOS 26 / Darwin 25, where the legacy
`CGWindowListCreateImage` is deprecated). Programmatic SCK capture does **not**
grab keyboard input, so it sidesteps the known interactive-screenshot focus bug
(macOS `screencapture` / Cmd-Shift tools steal input ownership).

Platform scope: **macOS only**. Non-macOS builds return an error.

## Flow

Mirrors the existing async `read_layout` query path (request relayed to the GUI,
answered on a later frame):

```
agent (claude/codex/vibe in a vmux terminal)
  → vmux_mcp  tools/call "screenshot" { pane? }
  → socket → vmux_service  broker.query (5s timeout)
  → GUI: vmux_agent handle_agent_queries emits ScreenshotRequest (Bevy message)
  → vmux_desktop start_screenshots: preflight permission, resolve window id
       (+ optional crop rect), kick off async SCK capture
  → SCK completion handler (off-thread): encode full-res PNG → save to disk,
       downscale → PNG bytes, send outcome over ScreenshotBridge channel
  → vmux_desktop drain_screenshots: bridge → ScreenshotResponse (Bevy message)
  → vmux_agent forward_screenshot_responses: → AgentQueryResult::Image
  → socket → vmux_mcp: text block (saved path + dims) + image block (base64 PNG)
```

The 5s `AGENT_QUERY_TIMEOUT` (`agent_broker.rs::query`) bounds the whole
round-trip. A granted-permission capture completes well under that. The
first-call permission prompt is **not** allowed to block the query (see
Permission UX) so it cannot blow the timeout.

## Components

### 1. Protocol — `crates/vmux_service/src/protocol.rs`

Add to `AgentQuery`:

```rust
Screenshot {
    /// Optional pane/stack id (kind:value, e.g. "pane:3" / "stack:7") to crop
    /// to. Whole window when None.
    pane: Option<String>,
},
```

Add to `AgentQueryResult`:

```rust
Image {
    /// Absolute path of the full-resolution PNG saved on disk.
    path: String,
    /// Downscaled PNG bytes (long edge <= MAX_INLINE_EDGE) for the inline block.
    png: Vec<u8>,
    width: u32,   // downscaled dims
    height: u32,
},
```

Both derive the existing rkyv traits. Only the **downscaled** PNG travels over
the socket (~<=1 MB), keeping the rkyv frame small; the full-res image lives only
on disk.

### 2. Paths — `crates/vmux_core/src/profile.rs`

```rust
/// Screenshot output directory: `~/.vmux/screenshots`.
pub fn screenshots_dir() -> PathBuf { config_dir().join("screenshots") }
```

(`config_dir()` already resolves to `~/.vmux`.)

### 3. Capture — `crates/vmux_desktop/src/screenshot.rs` (new)

Bevy messages (defined in `vmux_agent`, see §4) drive two systems registered by
the desktop plugin:

- `start_screenshots` (reads `ScreenshotRequest`):
  1. **Preflight** `CGPreflightScreenCaptureAccess()`. If not granted:
     fire non-blocking `CGRequestScreenCaptureAccess()` (shows the system
     prompt) and immediately emit an error outcome:
     `"Screen Recording permission required — grant it in System Settings ▸
     Privacy & Security ▸ Screen Recording, then retry."`
  2. Resolve the primary window's `NSWindow` via the established
     `WINIT_WINDOWS → window_handle() → RawWindowHandle::AppKit → NSView →
     .window()` pattern (see `background_lifecycle.rs::activate_native_window`),
     then `window.windowNumber()` → `CGWindowID`. Also read the window's
     backing scale factor.
  3. If `pane` is set: resolve the entity from the id and read its on-screen
     rect from Bevy UI geometry (`ComputedNode` size + `UiGlobalTransform`
     translation), scaled to physical px → crop rectangle. Unknown id → error
     outcome.
  4. Kick off async capture: `SCShareableContent.getShareableContent…` → find
     the `SCWindow` whose `windowID` matches → `SCContentFilter`
     (`initWithDesktopIndependentWindow:`) + `SCStreamConfiguration` (width/
     height set to the window's physical pixel size) →
     `SCScreenshotManager.captureImageWithFilter:configuration:completionHandler:`
     → `CGImage`.
  5. Completion handler (runs off-thread): `CGImage` → RGBA buffer →
     `image::RgbaImage`. Optional crop. Encode **full-res PNG**, save to
     `~/.vmux/screenshots/vmux-YYYYMMDD-HHMMSS-mmm.png` (`create_dir_all`
     first). Downscale a copy to `MAX_INLINE_EDGE` (1568) long edge
     (`image::imageops`), encode to PNG bytes. Send
     `ScreenshotOutcome { request_id, Result<{path, png, w, h}, String> }`
     over the `ScreenshotBridge` crossbeam channel.

- `drain_screenshots`: drains the `ScreenshotBridge` receiver and writes a
  `ScreenshotResponse` Bevy message.

Resource `ScreenshotBridge { tx: Sender, rx: Receiver }` carries finished
outcomes from the SCK callback thread back onto the Bevy main thread (so the
service send stays on the main thread, matching the layout pattern).

Non-macOS: `start_screenshots` emits
`Err("screenshots are only supported on macOS")`.

Crates available, no new heavy deps beyond ScreenCaptureKit bindings:
`block2`, `dispatch2`, `objc2`, `objc2-app-kit`, `objc2-core-graphics`,
`raw_window_handle`, `image`, `png`, `base64` are already in the tree.
Add `objc2-screen-capture-kit` to `vmux_desktop`.

### 4. GUI wiring — `crates/vmux_agent/src/plugin.rs`

Define messages (in `vmux_agent`, which both `vmux_agent` and `vmux_desktop`
can reference — `vmux_desktop` depends on `vmux_agent`):

```rust
pub struct ScreenshotRequest { pub request_id: u64, pub pane: Option<String> }
pub struct ScreenshotResponse { pub request_id: u64, pub result: Result<ScreenshotImage, String> }
pub struct ScreenshotImage { pub path: String, pub png: Vec<u8>, pub width: u32, pub height: u32 }
```

- `handle_agent_queries`: add `AgentQuery::Screenshot { pane } =>`
  write a `ScreenshotRequest`.
- `forward_screenshot_responses` (new system): read `ScreenshotResponse` →
  send `ClientMessage::AgentQueryResponse` with `AgentQueryResult::Image{…}`
  on success or `AgentQueryResult::Error` on failure.
- Register both messages and the forwarder in the plugin.

### 5. MCP — `crates/vmux_mcp/src/{tools,protocol}.rs`

- `screenshot_definition()` tool:
  - name `screenshot`; optional `pane` (string id like `read_layout` returns).
  - description: captures the whole vmux window (all visible tabs/panes) as it
    looks on screen, optionally cropped to a pane/stack id; saves a full-res PNG
    under `~/.vmux/screenshots/` and returns the image inline; first use on a
    fresh machine prompts for macOS Screen Recording permission — grant it and
    retry; macOS only.
- `dispatch_with_anchor`: `name == "screenshot"` →
  `DispatchTarget::Query(AgentQuery::Screenshot { pane })` (the anchor param is
  unused for screenshots).
- `query_result_to_mcp_response`: `AgentQueryResult::Image { path, png, width,
  height } =>` content array with a text block
  (`saved {path} ({width}×{height})`) and an image block
  (`{ "type": "image", "data": base64(png), "mimeType": "image/png" }`).

## Constants

- `MAX_INLINE_EDGE = 1568` — long-edge cap for the inline image (Claude's vision
  downscale threshold; keeps base64 token cost sane). Tunable.

## Error handling

All failures surface as `AgentQueryResult::Error` → MCP `isError` text, so the
agent gets an actionable message:

- permission not granted (prompt shown, retry)
- no primary window / cannot resolve `NSWindow`
- `pane` id not found
- SCK reported an error / no matching `SCWindow`
- non-macOS platform

## Testing

- `vmux_mcp`: `screenshot` appears in `tool_definitions()`; dispatch routes to
  `AgentQuery::Screenshot` with and without `pane`;
  `query_result_to_mcp_response` for `Image` produces one text + one image block
  with correct base64 and mimeType.
- `vmux_service`: rkyv round-trip for `AgentQuery::Screenshot` and
  `AgentQueryResult::Image`.
- `vmux_agent`: `AgentQuery::Screenshot` emits a `ScreenshotRequest`;
  `ScreenshotResponse` (ok/err) forwards the matching `AgentQueryResult`.
- `vmux_core`: `screenshots_dir()` == `~/.vmux/screenshots`.
- Pure image helpers (downscale dims, crop-rect math, filename formatting) are
  unit-tested without native capture.
- Native SCK capture is verified manually (requires a real window + permission);
  the macOS capture fn is isolated behind the message boundary so the rest is
  testable headlessly.

## Out of scope (v1)

- Multi-window / multi-monitor selection (always the primary vmux window).
- Video / continuous capture.
- Annotating or diffing screenshots.
- Returning full-res inline (full-res is disk-only; inline is downscaled).
