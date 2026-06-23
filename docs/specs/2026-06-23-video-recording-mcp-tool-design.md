# Video Recording MCP Tools — Design

Date: 2026-06-23
Status: Approved (pending spec review)

## Goal

Extend the agent's self-testing capability from still screenshots to **video**.
An agent running inside a vmux terminal pane starts a recording, exercises a
feature through other MCP tools (run commands, navigate, switch tabs), then stops
the recording and gets back a small, compressed **mp4** it can drop into a PR
description. Optionally also emits a **GIF** for inline-in-markdown embedding.

This is the natural follow-up to the screenshot tool (which listed "Video /
continuous capture" as out of scope) and reuses its capture plumbing.

## Why start/stop (not a fixed-duration call)

The recorded duration is unknown up front and the **agent itself drives the UI
between start and end** — it interleaves `vmux_run`, `vmux_browser_navigate`,
etc. to actually demonstrate the feature. A single blocking "record N seconds"
call cannot work: it would freeze the agent for the duration and is capped by
vibe's 60s MCP tool timeout. So recording is two MCP calls — `vmux_record_start`
returns immediately, `vmux_record_stop` finalizes — with a **max-duration
auto-stop** safety cap so a crashed or forgetful agent can't leave a recording
running forever.

## Why native VideoToolbox (not ffmpeg)

mp4 is encoded live via **AVFoundation `AVAssetWriter`** with the H.264 codec,
hardware-accelerated by VideoToolbox. The file is compressed as it is written —
no external `ffmpeg` runtime dependency, no post-process pass, fits vmux's
Rust-only / no-external-deps ethos. The optional GIF is produced in pure Rust
(`gif` + `color_quant`) by teeing the same frames we already hold, so it also
needs no external tools.

## Why OS-level capture (same rationale as screenshots)

Browse-mode panes render via **windowed CEF** (a native `NSView` child subview
on a separate GPU surface), invisible to a Bevy framebuffer readback. Capturing
the single composited `NSWindow` by its `CGWindowID` yields a faithful image
including browser content. We use **ScreenCaptureKit** — the screenshot path
uses one-shot `SCScreenshotManager`; video uses continuous **`SCStream`** (the
`SCStream` feature of `objc2-screen-capture-kit` is already enabled). SCK capture
does not steal keyboard input, sidestepping the interactive-screenshot focus bug.

Platform scope: **macOS only**. Non-macOS builds return an error.

## Flow

```
agent (claude/codex/vibe in a vmux terminal)

  ── tools/call "vmux_record_start" { gif?, max_secs?, pane? }
      → socket → vmux_service broker.query
      → GUI: vmux_agent emits RecordStartRequest (Bevy message)
      → vmux_desktop start_recording: preflight permission, reject if already
           active, resolve window id (+ optional crop), build SCStream +
           AVAssetWriter (+ gif thread), startWriting, arm max_secs timer
      → RecordStartResponse (ok ack / err) → AgentQueryResult → text ack
           ("recording started, max 120s"). Returns once setup succeeds, before
           frames are needed — does NOT wait for the recording to end.

  ── (agent exercises the feature via other vmux_* tools while frames stream:
      SCStreamOutput delegate, off-thread, appends each CVPixelBuffer to the
      mp4 adaptor and pushes throttled frames to the gif encoder thread)

  ── tools/call "vmux_record_stop" { dir?, name? }
      → socket → vmux_service broker.query (extended timeout)
      → GUI: vmux_agent emits RecordStopRequest
      → vmux_desktop stop_recording: markAsFinished + finishWriting; finalize
           gif; move mp4(+gif) from temp → dir/name; send outcome over bridge
      → AgentQueryResult::Recording → vmux_mcp: text block
           (mp4 path, duration, size, gif path?)
```

If the `max_secs` timer fires before a stop arrives, the GUI finalizes the
recording to the **default** dir/name and stashes the result; the next
`vmux_record_stop` returns that path annotated `(auto-stopped at <max_secs>s)`.

## Recording control model

- **One active recording per app.** `vmux_record_start` while a recording is
  already active returns an error ("a recording is already in progress; stop it
  first"). This keeps SCStream/AVAssetWriter lifecycle and the bridge unambiguous.
- `vmux_record_stop` with no active recording: if a recording was auto-stopped
  since the last stop, return its path; otherwise error ("no recording in
  progress").
- The recording writes to a **temp path** during capture; `dir`/`name` are
  applied at **stop** time (cheap rename, same filesystem), so the agent can name
  the file after confirming the take is good — or stop to the default to discard
  intent. `gif` and `max_secs` must be set at **start** (the gif tee must be
  decided before frames flow; the timer must be armed at start).

## Components

### 1. Protocol — `crates/vmux_service/src/protocol.rs`

Add to `AgentQuery`:

```rust
RecordStart {
    /// Also emit a GIF alongside the mp4.
    gif: bool,
    /// Auto-stop safety cap in seconds.
    max_secs: u32,
    /// Optional pane/stack id ("pane:3" / "stack:7") to crop to; whole window
    /// when None. Same resolver as Screenshot.
    pane: Option<String>,
},
RecordStop {
    /// Output directory (absolute). None → screenshots_dir().
    dir: Option<String>,
    /// Output basename (no extension). None → "vmux-<timestamp>".
    name: Option<String>,
},
```

Add to `AgentQueryResult`:

```rust
Recording {
    /// Absolute path of the finalized mp4.
    mp4_path: String,
    /// Absolute path of the GIF, if one was requested/produced.
    gif_path: Option<String>,
    /// Recorded duration in milliseconds.
    duration_ms: u64,
    /// mp4 size in bytes.
    bytes: u64,
    /// True if finalized by the max_secs auto-stop rather than an explicit stop.
    auto_stopped: bool,
},
```

Both derive the existing rkyv traits. No media bytes cross the socket — only
paths/metadata (video files are far too large to inline; this also respects the
single-response / 60s MCP constraint).

`RecordStart` succeeds with the existing `AgentQueryResult::Text` ack
("recording started, max <n>s") or `AgentQueryResult::Error` — produced from the
async `start_recording` outcome (see §4), not synthesized synchronously, so
permission / already-active / window-resolution failures surface to the agent.

### 2. Paths — `crates/vmux_core/src/profile.rs`

Reuse the existing `screenshots_dir()` (`~/.vmux/screenshots`) as the default
output directory for recordings — videos live next to screenshots. No new path
function. (`dir` param overrides per call.)

### 3. Capture — `crates/vmux_desktop/src/recording.rs` (new)

New sibling to `screenshot.rs` (filename-based module, no `mod.rs`). Holds the
macOS `SCStream` capture + AVFoundation encode; non-macOS stub returns an error.

**Shared with screenshot.rs** (reuse, do not duplicate): macOS-14 gate, Screen
Recording TCC preflight/request, `window_number` resolution (`WINIT_WINDOWS →
window_handle → NSView.window().windowNumber()`), crop-rect-from-pane math, and
the winit `WakeUp` wake pattern. Factor the shared bits into a small common
helper if cleanly possible; otherwise call across modules.

**Active-session resource** `RecordingSession` (macOS): retains the `SCStream`,
its `SCStreamOutput` delegate, the `AVAssetWriter` + video `AVAssetWriterInput` +
`AVAssetWriterInputPixelBufferAdaptor`, the optional gif-encoder thread handle +
bounded frame `Sender`, the start `Instant`, the temp mp4/gif paths, and the
`max_secs` deadline. `Option<RecordingSession>` is the "one active recording"
invariant.

`start_recording` (reads `RecordStartRequest`):
1. Preflight `CGPreflightScreenCaptureAccess()`; if not granted, fire
   non-blocking `CGRequestScreenCaptureAccess()` and emit `RecordStartResponse`
   Err (same message as screenshots: grant in System Settings ▸ Privacy &
   Security ▸ Screen Recording, then retry).
2. Reject with `RecordStartResponse` Err if a session is already active.
3. Resolve primary `NSWindow` → `CGWindowID` + physical pixel size (and optional
   crop rect from `pane`).
4. Build `SCContentFilter` (`initWithDesktopIndependentWindow:`) +
   `SCStreamConfiguration`:
   - `width`/`height` = window physical size (cropped region applied to the
     pixel buffers, mirroring the screenshot crop step).
   - `pixelFormat` = `kCVPixelFormatType_32BGRA`.
   - `minimumFrameInterval` = 1/60s (cap at 60fps).
   - `showsCursor` = true (demos should show the pointer).
   - `queueDepth` set so SCK drops frames under backpressure rather than stalling.
5. Build `AVAssetWriter(tempURL, .mp4)` + video input
   (`AVVideoCodecKey = .h264`, width/height = output size,
   `AVVideoCompressionPropertiesKey` with an average-bitrate cap for small
   files), `expectsMediaDataInRealTime = true`, + pixel-buffer adaptor (BGRA).
   `startWriting()`.
6. If `gif`: spawn a gif-encoder thread owning a `gif::Encoder` writing the temp
   gif, fed by a **bounded** channel (drop-on-full). Decouples slow palette
   quantization from the capture thread.
7. Add the `SCStreamOutput`, `startCaptureWithCompletionHandler:`, arm the
   `max_secs` deadline, store the `RecordingSession`, emit `RecordStartResponse`
   Ok(max_secs).

`SCStreamOutput` delegate (`stream:didOutputSampleBuffer:ofType:`, off-thread):
- On first frame: `writer.startSession(atSourceTime: pts)`.
- mp4: if `input.isReadyForMoreMediaData`, `adaptor.append(pixelBuffer, pts)`
  (pts from `CMSampleBufferGetPresentationTimeStamp`). Apply crop if set.
- gif (if enabled) and throttled to ~12fps: downscale the BGRA frame, convert to
  RGBA, `try_send` to the gif thread (drop if the channel is full).

`stop_recording` (reads `RecordStopRequest`):
1. Take the active session (error if none — but if `last_auto_stopped` is
   present, return it instead).
2. `stream.stopCaptureWithCompletionHandler:`; `input.markAsFinished()`;
   `writer.finishWriting(completion)`.
3. Close the gif channel; join the gif thread (it flushes the trailer).
4. In the `finishWriting` completion (off-thread): compute duration (start
   `Instant` elapsed, or last pts − first pts), `create_dir_all(dir)`, move temp
   mp4 → `dir/name.mp4` and temp gif → `dir/name.gif`, `stat` the mp4 size, send
   `RecordingOutcome { request_id: Option<u64>, Result<RecordingInfo, String> }`
   over the `RecordingBridge` crossbeam channel (`request_id` is `None` for an
   auto-stop, which has no pending query).

`drain_recordings`: drains `RecordingBridge`. `Some(request_id)` → emit
`RecordStopResponse`. `None` (auto-stop) → store the `RecordingInfo` in
`last_auto_stopped` for the next `record_stop` to return. Stays on the Bevy main
thread, matching the screenshot/layout pattern.

**Auto-stop timer**: a system checks the active session's deadline whenever the
loop wakes; on expiry it runs the same finalize path to the **default** dir/name
with `request_id = None`. The capture thread already wakes the loop via
`WinitUserEvent::WakeUp`, so the deadline is observed promptly without
`UpdateMode::Continuous`.

**No `UpdateMode::Continuous`.** All per-frame encoding happens on the SCK
callback thread and the gif thread — Bevy systems only run for start/stop/drain
and the deadline check, woken by the existing `WakeUp` bridge.

Non-macOS: `start_recording` emits `Err("recording is only supported on macOS")`.

New deps (`crates/vmux_desktop/Cargo.toml`):
- macOS: `objc2-av-foundation` (AVAssetWriter*, AVVideo* keys),
  `objc2-core-media` (`CMSampleBuffer`, `CMTime`), `objc2-core-video`
  (`CVPixelBuffer`). Enable the `SCStream` + `SCStreamConfiguration` /
  `SCContentFilter` features on the existing `objc2-screen-capture-kit`.
  `block2` / `dispatch2` already present.
- cross-platform: `gif` and `color_quant` (gated to the gif tee).

### 4. GUI wiring — `crates/vmux_agent/src/{events,plugin}.rs`

Define messages in `vmux_agent` (referenced by both `vmux_agent` and
`vmux_desktop`):

```rust
pub struct RecordStartRequest  { pub request_id: u64, pub gif: bool, pub max_secs: u32, pub pane: Option<String> }
pub struct RecordStartResponse { pub request_id: u64, pub result: Result<u32 /* max_secs */, String> }
pub struct RecordStopRequest   { pub request_id: u64, pub dir: Option<String>, pub name: Option<String> }
pub struct RecordStopResponse  { pub request_id: u64, pub result: Result<RecordingInfo, String> }
pub struct RecordingInfo { pub mp4_path: String, pub gif_path: Option<String>, pub duration_ms: u64, pub bytes: u64, pub auto_stopped: bool }
```

- `handle_agent_queries`: `AgentQuery::RecordStart{..}` → write `RecordStartRequest`;
  `AgentQuery::RecordStop{..}` → write `RecordStopRequest`. Both answers come back
  asynchronously via the forwarders below (matching the screenshot pattern) — the
  query is not answered inline, because start can fail in the desktop system.
- `forward_record_start_responses` (new): `RecordStartResponse` →
  `AgentQueryResult::Text("recording started, max <n>s")` on Ok / `Error` on Err.
- `forward_record_stop_responses` (new): `RecordStopResponse` →
  `AgentQueryResult::Recording{..}` on Ok / `Error` on Err.
- Register the four messages + both forwarders in the plugin.

`start_recording` emits `RecordStartResponse` once synchronous setup
(permission, session-active check, window resolve, `AVAssetWriter.startWriting`)
resolves — it does not block on the recording's lifetime. A rare async
`startCapture` completion error is surfaced at stop time.

### 5. MCP — `crates/vmux_mcp/src/{tools,protocol}.rs`

- `record_start_definition()` → tool `vmux_record_start`:
  - params: `gif` (bool, default false), `max_secs` (int, default 120),
    `pane` (string id, optional). `additionalProperties: false`.
  - description: starts recording the vmux window to an mp4 (optionally a GIF
    too); returns immediately so you can drive the UI with other tools; call
    `vmux_record_stop` when done; auto-stops after `max_secs`; first use prompts
    for macOS Screen Recording permission; macOS only.
- `record_stop_definition()` → tool `vmux_record_stop`:
  - params: `dir` (string absolute path, optional, default `~/.vmux/screenshots`),
    `name` (string basename, optional, default `vmux-<timestamp>`).
    `additionalProperties: false`.
  - description: stops the active recording, writes `dir/name.mp4` (+ `.gif`),
    returns the path(s), duration, and size.
- `dispatch_with_anchor` (after the `vmux_` prefix is stripped):
  - `"record_start"` → parse params → `DispatchTarget::Query(AgentQuery::RecordStart{..})`
  - `"record_stop"`  → parse params → `DispatchTarget::Query(AgentQuery::RecordStop{..})`
- `query_result_to_mcp_response`: `AgentQueryResult::Recording{..}` → a single
  text content block, e.g.
  `recorded 7.4s → /…/feature-x.mp4 (1.2 MB) + /…/feature-x.gif`
  (append `(auto-stopped at 120s)` when `auto_stopped`). No image/video block.

### 6. Stop query timeout — `crates/vmux_*` broker

`finishWriting` after live encoding is normally sub-second, but a large clip can
take a few seconds to flush the moov atom. The screenshot path's 5s
`AGENT_QUERY_TIMEOUT` is too tight a guarantee for stop. Give `RecordStop` a
longer bound (e.g. 30s — comfortably under vibe's 60s MCP timeout) via a
per-query timeout or a dedicated constant. `RecordStart` keeps the short bound.

### 7. Demo convention — `docs/features/`

Establish `docs/features/` as the home for committed demo clips, seeded with a
short `README.md` explaining the convention: agents pass
`dir=<repo>/docs/features`, `name=<feature>` to `vmux_record_stop`; keep clips
short. Note the tradeoff: committing mp4/gif bloats git history — prefer brief,
small clips; git-lfs is an option but out of scope here. This is a **convention +
seed only**; the tool stays repo-agnostic (it just writes to whatever `dir`).

## Constants

- `RECORD_MAX_SECS_DEFAULT = 120` — default auto-stop cap.
- `GIF_FPS = 12` — gif frame-sampling rate.
- `GIF_MAX_EDGE` — long-edge downscale cap for gif frames (e.g. 800), keeps gif
  size/CPU sane. Tunable.
- `RECORD_STOP_TIMEOUT_SECS = 30` — broker timeout for the stop round-trip.
- mp4 average-bitrate cap — adaptive to output size; pick a value that keeps a
  typical window clip in the low single-digit MB range. Tunable.

## Error handling

All failures surface as `AgentQueryResult::Error` → MCP `isError` text:

- permission not granted (prompt shown, retry)
- recording already in progress (`record_start`)
- no recording in progress (`record_stop`, and nothing auto-stopped)
- no primary window / cannot resolve `NSWindow`
- `pane` id not found
- `dir` not writable / move failed
- SCK or AVAssetWriter reported an error
- non-macOS platform

## Testing

- `vmux_mcp`: `vmux_record_start` / `vmux_record_stop` appear in
  `tool_definitions()` and are `vmux_`-prefixed; dispatch routes to the right
  `AgentQuery` with defaults applied (gif=false, max_secs=120, dir/name None);
  `query_result_to_mcp_response` for `Recording` produces the expected text
  (with and without gif, with and without `auto_stopped`).
- `vmux_service`: rkyv round-trip for `AgentQuery::RecordStart` /
  `RecordStop` and `AgentQueryResult::Recording`.
- `vmux_agent`: `RecordStart` emits `RecordStartRequest`; `RecordStartResponse`
  (ok/err) forwards `Text` ack / `Error`; `RecordStop` emits `RecordStopRequest`;
  `RecordStopResponse` (ok/err) forwards `Recording` / `Error`.
- Pure helpers unit-tested without native capture: gif frame-sampling cadence,
  downscale dims, default-name/timestamp formatting, dir/name → final path
  resolution, duration math.
- Native SCStream + AVAssetWriter capture is verified manually (requires a real
  window + permission); the macOS encode path is isolated behind the message
  boundary so the rest is testable headlessly.
- A `no_continuous_update_mode`-style invariant remains satisfied (recording adds
  no Continuous mode).

## Out of scope (v1)

- Audio capture (video only).
- WebM / HEVC outputs (H.264 mp4 + optional GIF only).
- Multi-window / multi-monitor selection (always the primary vmux window).
- Pre-recording demos for every existing feature (this ships the capability +
  the `docs/features/` convention, not the content).
- Pausing/resuming a recording; multiple concurrent recordings.
- git-lfs setup for committed clips.
