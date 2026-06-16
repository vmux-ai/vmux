# Startup status splash — design

Date: 2026-06-16

## Problem

The startup splash (`crates/vmux_desktop/src/splash.rs`) shows a logo and an
**indeterminate spinner** only. Users wait through a long, opaque loading screen
with no indication of what the app is doing or how far along it is.

## Findings: what gates the splash today

- The splash is a native AppKit `NSPanel`, shown in `Startup` (`show_splash`) and
  faded in `Last` (`dismiss_splash`) once the primary window becomes `visible`
  (20s force-dismiss backstop).
- On macOS the primary window starts hidden (`visible: false`,
  `lib.rs:primary_window_config`). It is revealed by
  `reveal_window_after_layout_ready` (`glass.rs`) the moment the **layout page**
  entity has both `LayoutCef` and `PageReady`.
- That single gate means the splash duration ≈ CEF cold-start (helper subprocess
  spawn + framework dylib load) + layout-page bundle load. **Content pages** (browser
  tabs, terminals, agents) are spawned during space restore
  (`persistence.rs:rebuild_space_views`) but load *after* the window is revealed —
  they do not affect splash length today.

Consequence: a truthful, live "page N/M" requires changing *when* the window
reveals, because per-page readiness currently happens post-reveal.

## Decision

- Show **detailed status text with counts** in the splash, keeping the spinner.
- **Wait for the active page**: the window reveal additionally gates on the
  active/foreground page of the active space reporting ready (with a timeout).
  Background tabs still lazy-load. The window appears slightly later, but the
  focused tab is never blank on reveal.

## Phase sequence shown in the splash

1. `Starting…` — early init, before restore begins (or no saved space).
2. `Restoring space…` — a saved space exists and views are being rebuilt. No
   count here (restore is fast and the total isn't final until rebuild
   completes). Skipped entirely when no saved space exists.
3. `Loading interface…` — restore done (or nothing to restore), waiting on
   the layout page's `PageReady` (the CEF cold-start long pole).
4. `Loading page N/M…` — the layout page is up; `M` = content stacks in the active space,
   `N` = those whose webview child has `PageReady`. Reveal gates on the
   **active** stack specifically.

These read as a sequence because the real signal order is monotonic: restore
(filesystem + ECS spawn) completes well before the layout page's `PageReady` (CEF cold
start). The narrative never regresses because each underlying signal flips
false→true exactly once.

## Architecture

### 1. `boot_status.rs` (new, `crates/vmux_desktop/src/`)

Pure ECS, no AppKit, so it compiles/tests cross-platform.

```rust
pub enum BootPhase {
    Starting,
    RestoringSpace,
    LoadingInterface,
    LoadingPages { ready: usize, total: usize },
}

#[derive(Resource)]
pub struct SplashStatus {
    pub phase: BootPhase,
    pub reveal_ready: bool,
}
```

Default = `{ phase: Starting, reveal_ready: false }`.

A **pure function** carries all logic so it is unit-testable without ECS or
AppKit:

```rust
struct BootInputs {
    space_present: bool,        // saved space file exists
    restore_complete: bool,     // space deserialized + views rebuilt (true immediately when no saved space)
    layout_ready: bool,         // LayoutCef entity has PageReady
    total_pages: usize,         // M: content stacks in the active space (valid once restore_complete)
    ready_pages: usize,         // N: those whose webview child has PageReady
    active_page_ready: bool,    // FocusedStack.stack's webview child has PageReady
    has_active_page: bool,      // an active content stack exists
    elapsed_since_layout: Option<Duration>, // since layout_ready first became true
}

fn compute(inputs: BootInputs) -> (BootPhase, bool); // (phase, reveal_ready)
```

Reveal:

```
reveal_ready = layout_ready && (active_page_ready || !has_active_page || budget_expired)
budget_expired = elapsed_since_layout >= ACTIVE_PAGE_BUDGET   // 8s
```

Phase precedence (first match wins — monotonic given the signal order):

```
if layout_ready && total_pages > 0 => LoadingPages { ready_pages, total_pages }
else if layout_ready               => LoadingInterface   // up, no content; reveal imminent
else if restore_complete           => LoadingInterface   // restore done (or nothing to restore), layout page still loading
else if space_present              => RestoringSpace
else                               => Starting
```

`Starting` therefore shows only the first frame(s) before restore runs. A
saved-space boot reads `Starting → RestoringSpace → LoadingInterface →
LoadingPages`; a fresh boot reads `Starting → LoadingInterface` (no
`RestoringSpace`).

`BootPhase` renders to a display string:
- `Starting…`
- `Restoring space…`
- `Loading interface…`
- `Loading page {ready}/{total}…`

### 2. `compute_boot_status` system

Scheduled in `Update`, **after `ComputeFocusSet`** (so `FocusedStack` is current).
Reads:

- `SpaceFilePresent` resource → `space_present`
- restore completion → `restore_complete`. Derived from `SpaceFilePresent`: if
  no saved space, `true` immediately; otherwise `true` once `rebuild_space_views`
  has run for the loaded space (e.g. observe the `Loaded` event / absence of
  `Stack` entities still missing their `Node` view). A small marker resource set
  when rebuild first completes is acceptable.
- `LayoutCef` + `PageReady` query → `layout_ready`
- content stacks in the active space and their webview children's `PageReady`
  → `total_pages` (M) and `ready_pages` (N). Content stack = `Stack` with a
  webview child (`Browser`/`Terminal`/agent/etc.), excluding the layout-page
  `LayoutCef`.
- `FocusedStack.stack`'s webview child `PageReady` → `active_page_ready` /
  `has_active_page`

Tracks `layout_ready_at: Option<Instant>` (set on first frame the layout page is
ready) to derive `elapsed_since_layout`. Writes `SplashStatus { phase, reveal_ready }`.

Optional: `info!` on each phase transition to record per-phase durations — gives
an empirical answer to "what took so long" going forward.

### 3. Reveal gate change (`glass.rs`)

`reveal_window_after_layout_ready` reveals the window when
`SplashStatus.reveal_ready` is true (single source of truth) instead of querying
`LayoutCef` + `PageReady` directly. The 20s splash backstop in
`dismiss_splash` is retained; the 8s active-page budget guarantees reveal well
inside it even if a page hangs.

### 4. Splash labels (`splash.rs`)

- Replace the logo image (`NSImageView` + the `vmux-icon.png` `include_bytes!`)
  with a **"Vmux" title** `NSTextField` (large, bold, centered) at the top.
- Add a second `NSTextField` (non-editable, non-bordered, centered, secondary
  color) below the spinner for the status line.
- New `update_splash_text` system (`Last`, before/with `dismiss_splash`) reads
  `SplashStatus` and sets the status label's string. Spinner stays.
- Drop the obsolete `splash_embeds_logo` test; update
  `desktop_enables_splash_appkit_features` (no longer needs `NSImageView`/`NSData`
  for the logo; now needs `NSTextField`).

### 5. Registration (`lib.rs`)

- `mod boot_status;`
- `init_resource::<SplashStatus>()` (Default = `Starting`, `reveal_ready: false`)
- add `compute_boot_status` to `Update` after `ComputeFocusSet`.

## Files

- `crates/vmux_desktop/src/boot_status.rs` — new: enum, resource, `compute`,
  display, system, tests
- `crates/vmux_desktop/src/glass.rs` — reveal reads `reveal_ready`
- `crates/vmux_desktop/src/splash.rs` — `NSTextField` + `update_splash_text`
- `crates/vmux_desktop/src/lib.rs` — register module/resource/system

## Testing (TDD)

Pure-logic unit tests on `compute()` and the display formatter (mirrors the
existing `splash_decision` test style):

- `Starting` when `!restore_complete && !space_present && !layout_ready`
- `RestoringSpace` when `space_present && !restore_complete && !layout_ready`
- `LoadingInterface` when `restore_complete && !layout_ready` (covers both the
  saved-space and fresh-boot paths)
- `LoadingPages { ready, total }` when `layout_ready && total_pages > 0`
- `reveal_ready` false until `layout_ready`
- `reveal_ready` only when `layout_ready && active_page_ready`
- `reveal_ready` via budget timeout when the active page hangs
  (`elapsed_since_layout >= 8s`)
- `reveal_ready` when `layout_ready && !has_active_page` (no content pages)
- display strings, incl. `Loading page 2/5`

ECS / source-scrape tests (match existing conventions):

- glass: window reveals only when `SplashStatus.reveal_ready`
- splash: source contains `NSTextField` and reads `SplashStatus`

## Out of scope

- Waiting for *all* pages (only the active page gates reveal).
- Per-step progress *inside* CEF cold-start or the layout-page bundle load (opaque).
- Reducing startup time itself (separate effort; timing logs will inform it).
