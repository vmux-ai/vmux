# vmux Dynamic Island — design

Date: 2026-06-26
Status: approved (pending spec review)
Supersedes: `2026-06-25-floating-command-bar-window-design.md` (deleted)

## Summary

A always-on **Dynamic Island** for vmux: a compact pill pinned to the top-center of the screen,
floating above all apps and all Spaces, present even when vmux is hidden. It **morphs shape and
size as actions run and information flows** — expanding into the command bar for search, showing
live agent/terminal/browser activity, and peeking notifications, then settling back to the idle
pill.

The island **is** the command bar: both the global hotkey (`Cmd+Shift+Space`) and the in-app
`Cmd+K` expand the island into search. The existing in-window command-bar modal is **removed**.

Rendering is **OSR** (off-screen): the island web page is rendered to an alpha IOSurface and
composited into the panel's layer, so the surface can be any shape with true transparency and can
morph/animate freely — which an opaque windowed CEF view cannot do over arbitrary apps.

## Goals

- Always-visible ambient pill, top-center, over every app and Space, including when vmux is hidden
  to the menu bar.
- Morphs between visual states driven by live events; smooth animated shape/size transitions.
- Becomes key without foregrounding vmux; search input works while another app is active.
- Unified command bar (hotkey + `Cmd+K` → island search); in-window modal removed.
- **Near-zero idle cost**: the idle pill is static (no animation, no repaint), so an always-on
  overlay adds no measurable CPU/GPU when nothing is happening. Never use `Continuous` update mode.
- v1 live feeds: search (core) + agent activity + terminal jobs + notifications + browser/media.

## Non-goals

- Click-to-expand / hover-peek (expansion is keyboard-only in v1).
- Draggable / repositionable island (pinned top-center).
- Windows/Linux (macOS-only; gate native code with `#[cfg(target_os = "macos")]`).
- A general third-party notification framework (v1 notifications are vmux's own in-app events).

## Background / current state (reuse points)

- The command bar today is an **OSR** CEF webview on the `Modal` entity
  (`crates/vmux_layout/src/window.rs:390-420`): `WebviewNativeOverlay` + `WebviewNativeLiquidGlass`,
  `display: None` until opened. Its alpha IOSurface is composited into the primary window's content
  view by `glass.rs::sync_command_bar_overlay` (`crates/vmux_desktop/src/glass.rs:469-548`) via
  `bevy_cef::prelude::NativeOverlayFrames` (an `Entity → AcceleratedFrame{io_surface}` map). **This
  is exactly the pipeline the island reuses**, retargeted from the primary window to a dedicated
  panel.
- Open data (spaces/tabs/commands/pages/url) is gathered in `handle_open_command_bar` and pushed
  once as the `COMMAND_BAR_OPEN_EVENT` rkyv payload to the webview
  (`crates/vmux_layout/src/command_bar/handler.rs:909-956`); the page filters client-side. The
  island's **search state** reuses this page, handler, and `results.rs` wholesale.
- `native_windowed` styling exists in the page but stays unused (the island is OSR, not windowed).
- App is `LSUIElement` (menu-bar accessory) — `packaging/macos/Info.plist:33-34` — so it keeps
  running and can own an always-on overlay without a Dock/menu presence.
- NSPanel precedent: `glass.rs::install_window_glass` / `splash.rs` build nonactivating borderless
  panels (`CanJoinAllSpaces | FullScreenAuxiliary | IgnoresCycle`, `becomesKeyOnlyIfNeeded`).
- No global hotkey today (all key paths are frontmost-gated). `global-hotkey` crate + a waker
  thread that calls `EventLoopProxy::send_event(WakeUp)` is the plan (the loop sleeps in Reactive
  mode and must be woken to poll the hotkey channel).
- objc2 / objc2-app-kit features available include `NSPanel`, `NSWindow`, `NSScreen`, `NSEvent`,
  `NSView`, `NSGlassEffectView`, `CALayer` (`crates/vmux_desktop/Cargo.toml`).

## Architecture

### Island host panel (native, `vmux_desktop`)

A persistent borderless transparent `NSPanel`, created at startup (objc2, modeled on `glass.rs`):

- Style: `Borderless | NonactivatingPanel`, `opaque(false)`, clear background, `hasShadow(false)`
  (the page draws its own shadow into the alpha surface).
- Level: floating (`NSWindowLevel::Floating`, raised toward status level if it must sit above other
  floating panels).
- `collectionBehavior = CanJoinAllSpaces | FullScreenAuxiliary | IgnoresCycle` → present on every
  Space and over fullscreen apps.
- Pinned top-center of the **main** display; reposition on display/topology change; offset below a
  physical notch when present.
- `ignoresMouseEvents(true)` while idle/ambient (clicks pass through to apps below); set `false`
  only while expanded for search (result clicks/scroll).
- Content view hosts a `CALayer` that displays the OSR IOSurface (transparent, non-opaque).

### Rendering (OSR composite)

- One OSR webview (entity marker `Island`) renders the island page; it is a `WebviewNativeOverlay`
  producing `NativeOverlayFrames` (reuse the existing accelerated path).
- A `sync_island_overlay` system (sibling of the current `sync_command_bar_overlay`, which is
  deleted) composites the island's IOSurface into the panel's layer.
- The panel frame follows the page's **reported content size**: the page emits a size event on each
  morph; native animates `setFrame` (Core Animation) to the new size, keeping the top-center anchor.
- **Idle budget**: the idle pill is fully static — no CSS animation, no DOM mutation → the OSR view
  produces no new frames → no compositing, no wake. Only morphs, spinners, progress, and typing
  schedule CEF paints (which wake the loop via the existing CEF wake throttler). A test mirrors
  `no_continuous_update_mode` to guard against regressions.

### Morph state machine

- The island **page** owns the visual states and transitions:
  `Idle → Search`, `Idle ⇄ Activity(kind)`, `→ Notification(peek) → Idle`, with priority/stacking
  rules when multiple activities are live (e.g. agent working while a job finishes).
- Rust sends typed events to the page (rkyv, like `COMMAND_BAR_OPEN_EVENT`):
  `IslandEvent::{ ExpandSearch(payload), Collapse, Activity(IslandActivity), Notify(IslandNotice) }`.
- After applying an event the page reports its new size; native animates the frame to match.
- The page is the single source of truth for shape; Rust never computes pixel geometry beyond the
  reported size.

### Search (unified command bar)

- Expand triggers: `Cmd+Shift+Space` (global) and `Cmd+K` (in-app) → `IslandEvent::ExpandSearch`.
- On expand: native sets the panel key (`makeKeyWindow` + `orderFrontRegardless` if needed) and
  `ignoresMouseEvents(false)`. A first-responder `NSView` subclass (objc2 `define_class!`) on the
  panel captures `keyDown`/`keyUp`/`flagsChanged` and forwards them to CEF `send_key` for the
  `Island` webview entity. Because a key nonactivating panel receives key events regardless of
  whether vmux is the active app, **typing works while another app is focused**.
- The search state reuses the existing command-bar page UI, `handler.rs` action path, and
  `results.rs`. The open payload is built by a shared `build_command_bar_open_payload(...)`
  extracted from `handle_open_command_bar` (the data-gathering, ~handler.rs:600-919) into a
  `SystemParam` + free fn.
- Dismiss: `Esc`, accept, or `resignKey` (blur) → `IslandEvent::Collapse`; panel returns to the
  idle pill and `ignoresMouseEvents(true)`. Accepting an action that targets the layout activates
  the main vmux window (`background_lifecycle::ensure_native_window_active`).
- **Remove** the in-window modal: delete the `Modal` spawn (`window.rs:390-420`) and
  `glass.rs::sync_command_bar_overlay` + `CommandBarOverlay`; re-point all command-bar open
  commands at the island.

### Feeds (v1)

Each feed is a thin adapter that emits a Bevy message consumed by an island system, which
translates to an `IslandEvent` for the page. No feed reaches into the page directly.

- **Agent** (`vmux_agent`): vibe/codex session running → `Activity{ kind: Agent, label, progress }`;
  completion → done/notice.
- **Terminal** (`vmux_terminal`): long-running foreground job exits → `Activity`/`Notify` with exit
  status and duration.
- **Notifications**: vmux's own in-app events (errors, task completions) → `Notify` peek. External
  sources (GitHub PR/mention) are out of v1 scope.
- **Browser + media** (`vmux_browser` + recording): page load / download progress → `Activity`;
  recording status / now-playing → `Activity{ kind: Media }`.

### Global hotkey

- `global-hotkey` crate; `GlobalHotKeyManager` created on the main thread at startup (NonSend, kept
  alive). Register the chord parsed from settings (default `"super+shift+Space"`; absent ⇒ default,
  no config auto-seed).
- A dedicated thread blocks on `GlobalHotKeyEvent::receiver().recv()`; on our hotkey it enqueues +
  `EventLoopProxy::send_event(WinitUserEvent::WakeUp)` so the reactive loop ticks and a Bevy system
  drains the queue → `IslandEvent::ExpandSearch`.

## Components (by crate)

- `crates/vmux_desktop/src/dynamic_island.rs` (new, `#[cfg(target_os="macos")]`): NSPanel
  lifecycle, positioning (top-center/main display/notch-aware), `sync_island_overlay` compositing,
  key/first-responder `NSView` subclass + key forwarding, mouse passthrough toggle, `resignKey`
  observer, `global-hotkey` manager + waker thread.
- `crates/vmux_layout/src/island.rs` (+ `island/` dir, filename-module): `Island` webview entity,
  `IslandEvent`/`IslandActivity`/`IslandNotice` types, the state-machine bridge systems
  (Rust→page events, size→panel-resize messages, feed messages→island events).
- `crates/vmux_layout/src/command_bar/handler.rs`: extract `build_command_bar_open_payload` +
  `CommandBarPayloadSources`; the island search reuses it. Repoint open commands at the island;
  delete the `Modal`-specific open/reveal once search is on the island.
- `crates/vmux_layout/src/window.rs`: remove the `Modal` spawn.
- `crates/vmux_desktop/src/glass.rs`: remove `sync_command_bar_overlay` + `CommandBarOverlay`.
- Island page: reuse the command-bar search components for the Search state; add pill + activity +
  notification components. Built within existing page infrastructure (no new workspace crate; track
  any new `src` in `vmux_server/build.rs` + `@source` globs so WASM/Tailwind pick it up).
- `crates/vmux_setting`: `command_bar.global_hotkey` setting (default `"super+shift+Space"`).
- `crates/vmux_desktop/src/persistence.rs`: no panel geometry to persist (pinned). Persist only the
  user's enabled-feeds preferences if exposed (optional).
- `crates/vmux_desktop/Cargo.toml`: add `global-hotkey`.

## Data flow

1. **Idle** — static pill; no repaints; loop asleep.
2. **Activity** — a feed emits a Bevy message → island system → `IslandEvent::Activity` to the page
   → page morphs (pill grows, shows spinner/progress) + reports size → native animates frame. When
   the activity ends, page returns to idle.
3. **Search** — hotkey/`Cmd+K` → `ExpandSearch(payload)` → page shows the command bar; native keys
   the panel + enables mouse + forwards keystrokes → CEF. Accept → action path → (activate main
   window if needed) → `Collapse`. Blur/`Esc` → `Collapse`.
4. **Notification** — feed → `Notify` → page peeks a notice pill, auto-collapses after a timeout.

## Build phases (one spec, sequenced)

- **P1 — Island shell:** always-on OSR `NSPanel` (top-center, over all apps/Spaces, nonactivating,
  mouse-passthrough), the island page with the morph state machine + idle/activity/notification
  states, `IslandEvent` contract, `sync_island_overlay` compositing, animated frame resize,
  near-zero idle repaint (+ guard test). Prove morphing with a temporary manual trigger.
- **P2 — Search / unify:** extract `build_command_bar_open_payload`; render the command bar as the
  island Search state; `global-hotkey` + `Cmd+K` → expand; native key forwarding; dismiss on
  blur/Esc; accept-activates-vmux; **remove** the in-window modal and `sync_command_bar_overlay`.
- **P3 — Feeds:** agent, terminal, notifications, browser/media adapters → island events. (Large;
  may get its own implementation plan. P1+P2 is the coherent "island replaces command bar"
  deliverable and can ship first.)

## Testing

Per the project's finish-then-test workflow, one runtime pass at the end of each phase.

- Native unit tests: `cargo test -p vmux_layout` (guards the command-bar source-scrape/style tests
  after the payload extraction) and `cargo test -p vmux_desktop` (+ a new idle-repaint guard test).
- Runtime — P1: pill visible top-center over other apps and on a second Space / over a fullscreen
  app; morph to an activity and back; confirm idle CPU is unchanged (no continuous repaint).
- Runtime — P2: `Cmd+Shift+Space` from another frontmost app expands search and accepts keystrokes;
  `Cmd+K` in vmux expands the island (not the old modal); accept opens in the layout and activates
  the main window; blur and `Esc` collapse; the old in-window modal is gone.
- Runtime — P3: trigger each feed (run an agent, finish a long shell job, fire an in-app
  notification, start a download / recording) and confirm the correct morph + collapse.

## Risks / mitigations

- **Always-on idle cost** — the single biggest risk for a permanent overlay. Mitigate: static idle
  pill (no animation/DOM mutation), reactive loop only, CEF wake throttler for morphs, a guard test
  asserting no `Continuous` mode and (ideally) no idle frame production.
- **Search input while vmux unfocused** — depends on the first-responder `NSView` subclass on a key
  nonactivating panel; validate early in P2 that `keyDown` reaches CEF while another app is active.
- **Rendering while vmux is hidden/backgrounded** — the island must paint morphs even when the main
  window is hidden; ensure the power-mode logic treats "island morphing/expanded" as a wake source.
- **Payload-gatherer extraction regressions** — keep modal→island search behavior equivalent;
  rely on `cargo test -p vmux_layout`.
- **Multi-display / notch** — reposition on topology change; offset below the notch.
- **Feed scope creep (P3)** — notifications limited to in-app events in v1; external sources later.

## Open questions (resolve during planning)

- Exact island page module/crate placement vs. the existing command-bar page (reuse boundary).
- Activity stacking/priority rules when multiple feeds are live simultaneously.
- Whether enabled feeds are user-configurable in v1 or all-on.
