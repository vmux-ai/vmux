# Update download progress — design

Date: 2026-06-27

## Problem

The auto-updater downloads and installs a new build silently on a background
thread, then surfaces a card ("New version available" / "Restart to update")
only *after* the install has finished. The user sees nothing during the
download, which on a slow connection looks like nothing is happening.

Add a downloading indicator with a progress bar to the **same card**, so the
card appears as soon as the update starts downloading and walks through:
**Downloading → Installing → Restart to update**.

## Current flow

- `vmux_desktop/src/updater.rs` — Bevy plugin. A background thread runs
  `check_update()`; on a hit it calls `update.download_and_install()` (blocking,
  no progress) and, on success, sends `UpdateResult::Installed { version }` over
  an mpsc channel. `poll_update_result` drains it and sets
  `StagedUpdate(Some(version))`.
- `vmux_browser/src/lib.rs` — `push_update_notice_emit` watches `StagedUpdate`
  and emits `UpdateReadyEvent { version }` (or `UpdateClearedEvent`) to the
  layout page over the rkyv bin bridge, with change-detection + re-emit on page
  ready.
- `vmux_layout/src/page.rs` — `use_bin_event_listener` for `UPDATE_READY_EVENT` /
  `UPDATE_CLEARED_EVENT` drives an `Option<String>` signal; when `Some`, renders
  `UpdateNoticeFooter` (green dot, "New version available", `{version}`,
  "Restart to update" button) inside the side sheet footer.
- `vmux_layout/src/debug_page.rs` — "Simulate update available" /
  "Clear update" / "Trigger restart" buttons drive the flow via
  `DebugUpdateReady` / `DebugUpdateClear` / `RestartRequestEvent`.

## Enabler

`cargo-packager-updater 0.2.3` (already a dependency) exposes:

```rust
pub fn download_and_install_extended<C: Fn(usize, Option<u64>), D: FnOnce()>(
    &self,
    on_chunk: C,            // (chunk_len, content_length)
    on_download_finish: D,
) -> Result<()>
```

`on_chunk` fires per network chunk with the chunk length and the (optional)
total content length. `on_download_finish` fires once, after the last byte and
before the install step runs. This gives real byte-level download progress plus
a clean download→install transition. `on_chunk` is `Fn` (not `FnMut`), so the
accumulator uses interior mutability (`Cell`).

## Data flow

```
updater bg thread: download_and_install_extended(on_chunk, on_download_finish)
   on_chunk(len, content_len) ──┐ throttled to integer-% steps (or ~512KB if
   on_download_finish          ─┤   total unknown), sent over mpsc
   Ok(()) / Err(e)             ─┘
        ↓ poll_update_result (desktop only)
   UpdateState resource:
     Idle | Downloading { version, downloaded, total } | Installing { version } | Ready { version }
        ↓ push_update_notice_emit (vmux_browser, emits on change + on page ready)
   UPDATE_PROGRESS_EVENT | UPDATE_READY_EVENT | UPDATE_CLEARED_EVENT  (rkyv → page)
        ↓ page.rs bin-event listeners → phase signal
   UpdateNoticeFooter renders the matching phase
```

## State model (decision)

Replace `StagedUpdate(Option<String>)` with a single `UpdateState` enum — one
source of truth, no risk of a separate progress resource desyncing from the
ready/cleared resource.

```rust
// vmux_layout/src/lib.rs, desktop-only (#[cfg(not(target_arch = "wasm32"))])
#[derive(Resource, Default, Clone, PartialEq, Debug)]
pub enum UpdateState {
    #[default]
    Idle,
    Downloading { version: String, downloaded: u64, total: u64 }, // total 0 = unknown
    Installing { version: String },
    Ready { version: String },
}
```

`total: u64` with `0` = unknown (rather than `Option<u64>`) keeps the wire event
simple and maps directly to "indeterminate bar".

Rejected alternative: keep `StagedUpdate` for the ready state and add a parallel
`UpdateProgress` resource. Two resources can desync and the browser emit would
have to reconcile both. Not worth the smaller diff.

## Wire event

One new host→page event covers both in-progress phases (download + install):

```rust
// vmux_layout/src/event.rs  (shared wasm + native; rkyv + serde derives)
pub const UPDATE_PROGRESS_EVENT: &str = "update-progress";

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct UpdateProgressEvent {
    pub version: String,
    pub downloaded: u64,
    pub total: u64,        // 0 = unknown (indeterminate)
    pub installing: bool,  // true = install phase; downloaded/total ignored
}
```

`UpdateReadyEvent` and `UpdateClearedEvent` are kept as-is. Host→page emits are
ad hoc by event-id string (`BinHostEmitEvent::from_rkyv`) — no emitter-plugin
registration is needed, only the page-side `use_bin_event_listener`.

## Changes by file

### `crates/vmux_layout/src/event.rs`
- Add `UPDATE_PROGRESS_EVENT` const and `UpdateProgressEvent` struct (derives above).
- Tests: rkyv round-trip for `UpdateProgressEvent`; assert `UPDATE_PROGRESS_EVENT == "update-progress"`.

### `crates/vmux_layout/src/lib.rs` + `plugin.rs`
- Replace `StagedUpdate(Option<String>)` with the `UpdateState` enum (desktop-only); export it.
- `plugin.rs`: `init_resource::<UpdateState>()` in place of `StagedUpdate`.

### `crates/vmux_desktop/src/updater.rs`
- Extend the channel message enum with `Downloading { version, downloaded, total }`
  and `Installing { version }` (alongside `NoUpdate` / `Installed` / `Failed`).
- `run_update_check`: swap `download_and_install()` for
  `download_and_install_extended(on_chunk, on_download_finish)`:
  - `on_chunk(len, content_len)`: `downloaded += len`; compute percent from
    `content_len`; send a `Downloading` message **only when `floor(percent)`
    increases**, or — if `content_len` is `None` — every ~512KB. Accumulator and
    last-sent marker held in `Cell`s.
  - `on_download_finish`: send `Installing { version }`.
  - `Ok(())` → `Installed { version }` (→ `Ready`); `Err(e)` → `Failed` (→ `Idle`).
- `poll_update_result`: map messages onto `UpdateState`:
  - `Downloading`/`Installing` → corresponding state (does **not** set `done`).
  - `Installed` → `Ready`; set `done = true` (stop polling).
  - `Failed` → reset to `Idle` (so a mid-download failure clears the card),
    leave `done = false` so the next poll retries.
  - `NoUpdate` → leave `Idle`.
- Extract a pure throttle/percent helper, e.g.
  `fn progress_step(downloaded: u64, total: Option<u64>, last_sent: u64) -> Option<u64>`
  returning the new "sent marker" (percent bucket, or byte bucket when total
  unknown) when an emit is warranted; unit-test it.

### `crates/vmux_browser/src/lib.rs`
- `push_update_notice_emit`: read `UpdateState` instead of `StagedUpdate`; emit:
  - `Idle` → `UPDATE_CLEARED_EVENT`
  - `Downloading { .. }` → `UPDATE_PROGRESS_EVENT { version, downloaded, total, installing: false }`
  - `Installing { version }` → `UPDATE_PROGRESS_EVENT { version, downloaded: 0, total: 0, installing: true }`
  - `Ready { version }` → `UPDATE_READY_EVENT { version }`
  - Keep change-detection (`Local<Option<UpdateState>>`, `PartialEq`) + re-emit
    when the page becomes ready, replacing `should_emit_update_notice`.
- Debug observers updated to set `UpdateState` (see debug section).
- Tests updated to use `UpdateState`; add coverage for the
  `UpdateState` → event mapping and dedup.

### `crates/vmux_layout/src/page.rs`
- Generalize the update signal from `Option<String>` to a small page-local phase
  type (e.g. `enum UpdatePhase { Downloading { version, downloaded, total },
  Installing { version }, Ready { version } }`, held as `Option`).
- Listeners: `UPDATE_PROGRESS_EVENT` → Downloading/Installing (by `installing`
  flag); `UPDATE_READY_EVENT` → Ready; `UPDATE_CLEARED_EVENT` → clear.
- Rewrite `UpdateNoticeFooter` to take the phase and render:
  - **Downloading**: green dot + "Downloading update" + `{version}`;
    determinate bar (`style: "width:{pct}%"`) when `total > 0`, otherwise an
    indeterminate sliding bar. Optional `{pct}%` label when determinate.
  - **Installing**: "Installing update…" + `{version}`; indeterminate bar.
  - **Ready**: "New version available" + `{version}` + **"Restart to update"**
    button (unchanged behavior — emits `RestartRequestEvent`).
- Keep the exact string `"Restart to update"` (asserted by source-scrape tests
  in `style.rs` / `tests/page_source.rs`).
- Styling via Tailwind utilities; reuse the existing `glass` card classes.

### Indeterminate animation
- Add one small `@keyframes` (horizontal slide) to the layout page CSS for the
  indeterminate bar; everything else stays Tailwind utilities/arbitrary props.
  (Confirm the exact CSS file during implementation; keep additions minimal.)

### `crates/vmux_layout/src/debug_page.rs`
- Add a "Simulate download" button so the bar is testable without a real
  release. Drives a **host-side simulation** via a new `DebugSimulateDownload`
  page→host event: the host advances `UpdateState` Downloading 0→100% over a few
  seconds, then Installing briefly, then Ready — exercising the real emit
  pipeline. Keep existing "Simulate update available" / "Clear update" /
  "Trigger restart" buttons.

## Throttling

Bounded at the source: `on_chunk` only pushes a `Downloading` message when the
integer percent rises (≤100 messages for a full download) or every ~512KB when
the total is unknown. Downstream change-detection then naturally limits bridge
traffic. No per-frame flooding of the CEF bin bridge.

## Edge cases

- **No content-length** (`total == 0`): page shows the indeterminate sliding bar.
- **Download/install failure**: `UpdateState` resets to `Idle`, the card
  disappears, and the next poll retries (existing retry cadence).
- **Card placement**: unchanged — it renders in the side-sheet footer and is
  therefore only visible while the side sheet is open, exactly as today.
- **Page reload mid-download**: a freshly-ready page re-receives the current
  state via the existing page-ready re-emit path.

## Testing

- `vmux_layout`: `UpdateProgressEvent` rkyv round-trip; stable event id;
  native `cargo test -p vmux_layout` to cover the page source-scrape asserts.
- `vmux_browser`: `UpdateState` → event mapping + dedup; updated debug observer
  test.
- `vmux_desktop`: `progress_step` throttle/percent helper unit tests; keep the
  existing `relaunch_plan` / pubkey / endpoint tests green.
- Final manual runtime test (by the user) via debug "Simulate download": confirm
  the bar animates, transitions Downloading → Installing → Restart, and that
  "Restart to update" still restarts.

## Out of scope

- Cancel/pause of an in-progress download.
- Persisting progress across restarts.
- Download error UI beyond clearing the card (silent retry, as today).
- Changing where/when the card is shown (still side-sheet footer).
