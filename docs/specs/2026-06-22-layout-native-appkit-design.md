# Native AppKit Layout Renderer

Migrate the layout (header, side sheets, command bar) off CEF off-screen rendering
(OSR) + Dioxus to a native macOS AppKit renderer driven from the ECS, with real OS
Liquid Glass.

## Motivation

Today the layout chrome is a Dioxus app rendered by CEF in **transparent OSR** mode,
composited into the window as an IOSurface-backed `CALayer` (User mode) or a Bevy mesh
texture (Player/3D mode). Running a full Chromium instance to paint static chrome costs
CPU, and the layout OSR mesh repaints on every resize tick. Liquid Glass today is a
single full-window `NSGlassEffectView` backdrop (`glass.rs`); we cannot get real
per-region glass under the header/side-sheets because the chrome is delivered as flat
pixels, not native views.

Goal: kill the layout's OSR CPU cost and get **real OS Liquid Glass per region** by
rendering the chrome as native AppKit views, while keeping the ECS as the single source
of truth.

## Goals

- Render header, side sheets, and command bar as native AppKit views on macOS.
- Real `NSGlassEffectView` glass per region (header strip, each side-sheet panel),
  blurring the page content behind them — GPU/WindowServer, ~0 CPU.
- **Maximize Liquid Glass; minimize chrome.** Surfaces are clear glass
  (`NSGlassEffectViewStyle::Clear`, clear tint), not opaque colored panels. Keep visual
  styling minimal — no solid fills, heavy tint, or borders; content is text/icons
  floating over glass. Let the glass carry the look.
- Drive the native views entirely from existing ECS data: the `LayoutSnapshot`
  (pane/tab tree) plus the header HostEmit payloads (url/title/favicon, tabs, profile,
  facepile, indicators). No new "source of truth."
- Remove the layout OSR mesh and the `glass.rs` IOSurface overlay hacks once parity is
  reached.
- Stay **pure Rust** (objc2). No Swift, no Xcode, no FFI seam, no new build toolchain.
- Keep `main` shippable throughout via a renderer flag; migrate region by region.

## Non-Goals

- The browser **pages** stay windowed CEF native views (unchanged; already cheap).
- `vmux_ui` / Dioxus is **not** removed — the terminal and pages still use it. Only the
  *layout chrome* renderer changes.
- No Player/3D-mode redesign in this work. 3D mode keeps the OSR mesh path for now; the
  flag selects native only for User (2D) mode. (3D-mode native story is a follow-up.)
- No Linux UI. Linux is CI-only (the app ships macOS-only: `packaging/macos`,
  `Casks/vmux.rb`). The native renderer is `#[cfg(target_os = "macos")]`; Linux gets a
  no-op stub so the crate compiles and ECS/snapshot tests still run.

## Background: current architecture

- **Frontend is already a "dumb" Dioxus renderer.** `vmux_ui` is a Dioxus WASM component
  library; the layout is a Dioxus app rendered by CEF.
- **ECS already computes and dispatches structured state.** `snapshot.rs::build_layout_snapshot`
  builds a `LayoutSnapshot` (`protocol.rs`: tabs → `LayoutNode` tree of Pane/Split/Stack,
  plus `Focus`), with **stable string ids** via `format_id(kind, entity.to_bits())`.
  `reconcile.rs` validates and can `apply` a snapshot back to the ECS. Header metadata
  (url/title/favicon/profile/facepile) flows via HostEmit events.
- **Compositing is already native in User mode.** `glass.rs::sync_layout_overlay` takes
  the layout's OSR frame as an `IOSurface` and sets it as the `contents` of a `CALayer`
  sublayer; pages are native windowed CEF views below it; one full-window
  `NSGlassEffectView` backdrop sits behind everything (`glass.rs::install_window_glass`).
- **The cost** is (a) running Chromium to paint the chrome and (b) the OSR layout mesh in
  `vmux_browser` repainting on resize. Real per-region glass is impossible because the
  chrome is flat pixels.

## Design overview

Swap **only the renderer**. The ECS → snapshot/HostEmit layer is unchanged and stays
cross-platform (and Linux-tested). A new macOS-only renderer consumes that same data and
maintains a native AppKit view tree.

```
ECS (Bevy)                         macOS render (NonSend, main thread, Last schedule)
──────────                         ────────────────────────────────────────────────
build_layout_snapshot ─┐
header HostEmit events ─┴─► LayoutView model ─► reconciler ─► AppKit view tree
                                                  (diff by      ├─ NSGlassEffectView panels
native view actions ◄── AppCommand channel ◄──    stable id)    └─ control subviews
```

- **Inputs:** `LayoutSnapshot` (structure) + header HostEmit payloads (content), merged
  into a single immutable `LayoutView` value each change.
- **Reconciler:** diffs the new `LayoutView` against the retained native tree, keyed by
  the snapshot's existing stable ids, and creates/updates/removes `NSView`s.
- **Output actions:** native targets (button clicks, URL submit, command-bar input) post
  `vmux_command::AppCommand` into the ECS via a channel, replacing the Dioxus
  JsEmit→IPC path.

## Components

### 1. `layout_native` module (`vmux_desktop`)

New module `crates/vmux_desktop/src/layout_native.rs` (+ `layout_native/` dir,
filename-based modules, no `mod.rs`). Lives in `vmux_desktop` because that is where the
existing objc2 native code lives (`glass.rs`, `splash.rs`, `background_lifecycle.rs`).

- A `LayoutNativePlugin` registers systems in `Last` (alongside the existing glass
  overlays) so the native tree updates after ECS state settles.
- State is held in a `NonSend` resource (AppKit is main-thread-only; follows the
  `GlassState`/`LayoutOverlay` pattern). Every AppKit touch goes through
  `MainThreadMarker`.
- `#[cfg(target_os = "macos")]` for the real impl; a no-op stub for other targets
  (see §6).

### 2. Data inputs → `LayoutView`

A platform-agnostic `LayoutView` struct (pure Rust, in `vmux_layout` or a shared crate)
is assembled from:

- `LayoutSnapshot` — tabs, active tab, pane/split/stack tree, focus, zoom (already built
  by `build_layout_snapshot`).
- Header HostEmit payloads — the same data the Dioxus header consumes today: per-stack
  `PageMetadata` (url/title/favicon), profile/active-profile, facepile, indicators
  (e.g. zoom pill).

A Bevy system builds `LayoutView` on change (change-detection gated, like the existing
push systems) and hands it to the renderer. `LayoutView` is `PartialEq` so we skip work
when nothing changed. This struct is the rendering contract and is unit-testable on
Linux.

### 3. Native view reconciler

AppKit has no diffing, so we build a small retained reconciler:

- A `HashMap<NodeId, RetainedView>` mapping stable ids → `NSView` handles.
- On each new `LayoutView`: walk the view model; for each node, **create** the `NSView`
  if the id is new, **update** properties in place if it exists, and **remove** views
  whose ids disappeared. Reuse the snapshot's stable ids (`format_id`) as keys so
  identity is stable across frames (no fl/re-create churn).
- Layout (frames/auto-layout) is computed from the view model rects; flex weights from
  `PaneSize` already exist in the snapshot.
- This mirrors `reconcile.rs` in spirit (diff a snapshot), but the target is the AppKit
  view tree rather than the ECS. It is the in-house substitute for a declarative
  framework — the ergonomic win without a new toolchain.

### 4. View tree, glass, and z-order

All chrome lives in the primary window's content view, layered:

```
content view
├─ (bottom) windowed CEF page views                  ── native, unchanged
├─ NSGlassEffectView region panels                   ── real OS glass per region
│    ├─ header strip
│    └─ side-sheet panels (left / right / bottom)
└─ (top) control subviews                            ── tabs, URL field, facepile, …
```

- Each glass panel is an `NSGlassEffectView` sized/positioned from the view model rects,
  sitting **above** the page views so it frosts the page beneath it.
- Control subviews (buttons, `NSTextField`, icons) are children on top of their panel,
  with clear backgrounds so the glass shows through.
- This extends `glass.rs` from one full-window backdrop to N region panels driven by the
  layout. The existing full-window backdrop can be retired once regions cover it.
- Corner radius / insets reuse the existing `LayoutSettings` (padding, radius).
- **Visual direction — maximize glass, minimize chrome.** Default to `Style::Clear` +
  clear tint (as `glass.rs` already does); no opaque backgrounds, gradients, or borders
  on the panels. Active/hover/focus states use subtle glass variations (slightly brighter
  glass, a thin accent underline on the active tab) rather than solid color fills.
  Text/icons sit directly on glass. The page showing through is the primary visual; the
  chrome is near-invisible furniture. Avoid introducing new colors/opacity unless a state
  genuinely can't read without it.

### 5. Input, focus, and commands

- Native views own their own hit-testing and keyboard. A click/submit posts a
  `vmux_command::AppCommand` (e.g. `LayoutCommand`, `BrowserCommand`, `PageOpenRequest`)
  into the ECS over an `mpsc`/crossbeam channel drained by a Bevy system each frame.
- Native focus replaces the OSR focus-ring + `CefKeyboardTarget` juggling for the layout.
  Page focus (windowed CEF) coordination is preserved: focusing a native control resigns
  CEF keyboard focus and vice versa.
- Global shortcuts stay in `vmux_command` unchanged.

### 6. Command bar

Replace the OSR command-bar modal (`window.rs` `Modal` + `glass.rs::sync_command_bar_overlay`
IOSurface overlay) with a native `NSPanel`:

- `NSGlassEffectView` background, `NSSearchField` input, results as an `NSTableView` /
  stacked rows.
- Open/close driven by the same `is_command_bar_open` state; selection posts the existing
  command-bar `AppCommand`s.
- Removes the `WebviewNativeLiquidGlass` + native-overlay path for the modal.

### 7. Linux / CI gating

- The renderer module is `#[cfg(target_os = "macos")]`. A `#[cfg(not(target_os = "macos"))]`
  stub provides a no-op `LayoutNativePlugin` so `vmux_desktop` builds on `ubuntu-latest`.
- `LayoutView` assembly, the reconciler's **diff logic** (pure data → a list of
  create/update/remove ops), `snapshot.rs`, and `reconcile.rs` stay platform-agnostic and
  keep their tests. Only the op→`NSView` application is macOS-gated.

### 8. `LayoutRenderer` flag

A setting (`LayoutRenderer::{ Cef, Native }`, default `Cef` until parity) selects the
renderer, per region during migration:

- `Cef` → current Dioxus/OSR path.
- `Native` → `layout_native`.
- Lets us flip header→native first, validate, then side sheets, then command bar, without
  regressing `main`. Removed once migration completes and `Native` is the only path.

## Migration phases

Each phase is independently shippable behind the flag.

- **P1 — Scaffold.** `layout_native` plugin (macOS) + Linux stub; `LayoutView` assembly
  system + `PartialEq`; reconciler skeleton (create/remove by id, no real widgets);
  `LayoutRenderer` flag. No visible change at default. Tests: `LayoutView` assembly +
  reconciler diff ops on Linux.
- **P2 — Glass backdrops.** `NSGlassEffectView` region panels for header + side sheets,
  sized from the view model. Visual glass only; content still CEF. Validates z-order and
  page-frosting.
- **P3 — Native header.** Tabs, URL field, facepile, profile, indicators as native
  controls wired to `LayoutView` + the `AppCommand` channel. Flip header→`Native`.
- **P4 — Native side sheets.** Left/right/bottom panels native.
- **P5 — Native command bar.** `NSPanel` + `NSSearchField` + results; retire the OSR
  modal + `sync_command_bar_overlay`.
- **P6 — Remove the User-mode CEF layout path.** Delete the Dioxus layout renderer entry
  for User mode and the `glass.rs` IOSurface overlays for the layout; drop the flag for
  User mode. The OSR layout mesh in `vmux_browser` is **retained for Player/3D mode** and
  gated to it (its removal is a follow-up tied to the deferred 3D-mode native story).
  (`vmux_ui`/Dioxus stays for terminal/pages.)

## Testing strategy

- **Platform-agnostic (Linux CI):** `LayoutView` assembly from a `LayoutSnapshot` +
  fixture HostEmit data; reconciler diff producing the correct create/update/remove op
  list across snapshot transitions (add tab, close tab, reorder, focus change, zoom).
- **macOS:** smoke tests that the plugin installs, the reconciler builds a non-empty view
  tree for a fixture `LayoutView`, and glass panels are created with
  `NSGlassEffectViewStyle::Clear` (string-scrape style tests like the existing
  `glass.rs` tests). Manual: run the app, confirm real per-region glass and low idle CPU.
- Keep `no_continuous_update_mode` and existing layout tests green.

## Risks & mitigations

- **CEF build fragility / size.** Implement directly with a warm target dir; do not
  subagent-drive. Phases keep diffs small.
- **`NSGlassEffectView` needs macOS 26.** Already handled in `glass.rs` (class-presence
  check + warn). Reuse that guard; fall back to a plain tinted panel pre-26.
- **Main-thread correctness.** All AppKit work behind `MainThreadMarker` in `NonSend`
  state, in `Last`, mirroring `glass.rs`.
- **Focus coordination regressions** between native controls and windowed CEF pages.
  Covered by the flag (A/B against the CEF path) and manual focus tests.
- **Reconciler churn.** Keying by stable snapshot ids avoids destroy/recreate; assert no
  spurious removes in diff tests.

## Files (indicative)

Create:
- `crates/vmux_desktop/src/layout_native.rs` (+ `layout_native/` for reconciler, views,
  glass, command bar, input channel)
- `LayoutView` model + assembly (in `vmux_layout`)

Modify:
- `crates/vmux_desktop/src/lib.rs` — register `LayoutNativePlugin`
- `crates/vmux_desktop/src/glass.rs` — hand region rects to native panels; retire
  full-window backdrop + IOSurface overlays as phases land
- `crates/vmux_layout/src/settings.rs` (or equivalent) — `LayoutRenderer` flag
- `crates/vmux_browser/src/lib.rs` — gate the OSR layout mesh to Player/3D mode only at
  P6 (User mode no longer uses it; full removal deferred to the 3D-mode native story)

## Open questions

- Exact source list/types for header HostEmit payloads (pin during planning by tracing
  the current Dioxus header's inputs).
- Whether `LayoutView` lives in `vmux_layout` or a new shared crate to avoid a
  `vmux_desktop`→`vmux_layout` data coupling.
- Player/3D-mode native story (deferred; OSR mesh retained there for now).
