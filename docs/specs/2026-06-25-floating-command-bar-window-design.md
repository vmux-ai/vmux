# Floating system-wide command-bar window — design

Date: 2026-06-25
Status: approved (pending spec review)

## Summary

Add a standalone, system-wide floating command bar — a macOS `NSPanel` (Spotlight/Raycast
style) that hosts the command bar as a **windowed** CEF view. It is summoned by a global
hotkey (default `Cmd+Shift+Space`, configurable) even when vmux is unfocused or hidden,
becomes key without fully activating the app, dismisses on blur, is draggable, and remembers
its position.

The existing in-window command bar (the OSR `Modal`, opened by `Cmd+K`) is left **untouched**
and continues to work exactly as today. The floating panel is a **second, independent**
command-bar surface that coexists with it.

## Goals

- Global hotkey summons the panel from anywhere, including when vmux is not the active app or
  is hidden to the menu bar.
- Panel floats above all apps and on the current Space; becomes key without foregrounding vmux.
- Native input: the windowed CEF view is first responder, so keyboard/mouse are handled by CEF
  directly (no manual event forwarding, no dependence on the primary window being key).
- Dismiss on blur (`resignKey`) and on `Esc`.
- Draggable; position is persisted and restored (clamped to a visible screen).
- Accepting a result that opens content in the layout activates the main vmux window.

## Non-goals

- Replacing or changing the in-window `Cmd+K` modal (explicitly kept as-is).
- Rounded *per-pixel-transparent* panel corners over arbitrary apps (not possible with windowed
  CEF — see "Corner treatment").
- Windows/Linux support (macOS-only feature; gate native code with `#[cfg(target_os = "macos")]`).

## Background / current state

- The command bar today is an OSR CEF webview on the `Modal` entity
  (`crates/vmux_layout/src/window.rs:390-420`): `WebviewNativeOverlay` +
  `WebviewNativeLiquidGlass` + `WebviewWindowedNativeFocus`, `display: None` until opened.
- Open data (spaces, tabs, commands, pages, url) is gathered in `handle_open_command_bar` and
  pushed **once** as the `COMMAND_BAR_OPEN_EVENT` rkyv payload to the modal webview
  (`crates/vmux_layout/src/command_bar/handler.rs:909-956`); the page filters client-side as the
  user types.
- The open/size/reveal systems are bound to the single `Modal` entity via `single`/`single_mut`.
- The OSR IOSurface is composited into the primary window's content view by
  `glass.rs::sync_command_bar_overlay` (`crates/vmux_desktop/src/glass.rs:469-548`).
- A `native_windowed` styling path is already wired end-to-end in the page
  (`command_bar_root_class(true)` / `command_bar_shell_class(true)` in
  `crates/vmux_layout/src/command_bar/style.rs:9-23`; branches in
  `crates/vmux_layout/src/command_bar/page.rs`) but is **dead** — nothing sets
  `WebviewWindowed` on the modal, so `native_windowed` is always `false` at runtime. The
  floating panel webview will set it `true`, activating this existing styling.
- The app is already `LSUIElement` (menu-bar accessory) — `packaging/macos/Info.plist:33-34`,
  enforced by `crates/vmux_desktop/tests/info_plist.rs`.
- bevy_cef windowed browsers parent to a native `NSView` pointer; the core
  `create_browser` is handle-agnostic (`patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs:266-289`),
  but the bevy_cef system `create_webviews` only sources that pointer from a winit window
  (`patches/bevy_cef-0.5.2/src/webview.rs:309-331`). This is the one piece that must be patched.
- NSPanel precedent: `glass.rs::install_window_glass` and `splash.rs` build nonactivating
  borderless panels (`CanJoinAllSpaces | FullScreenAuxiliary | IgnoresCycle`,
  `becomesKeyOnlyIfNeeded`).
- No global hotkey exists today; all key paths are frontmost-gated
  (`native_keyboard.rs` local monitor; `event_tap.rs` session tap gated by `app_is_frontmost`).
- Window geometry persistence uses moonshine-save into `store.ron`
  (`crates/vmux_desktop/src/persistence.rs:190` allow-list; `WindowGeometry` save/restore in
  `window_state.rs`).

## Architecture

Chosen approach: **windowed CEF hosted in a native NSPanel** (vs. compositing the OSR IOSurface
into the panel). Windowed CEF handles its own keyboard and mouse natively once it is first
responder of the key panel, which is the robust answer to "typing must work even when vmux is
unfocused." It also revives the already-built `native_windowed` page path.

### Corner treatment

Windowed CEF is an opaque remote GPU layer that CoreAnimation cannot clip, and over arbitrary
apps there is no backdrop to fake rounded corners against. Therefore: the **panel** is a rounded
glass tray (`NSGlassEffectView`, rounded content-view layer), and the opaque CEF view is **inset**
with padding so its square corners sit inside the rounded glass frame. The visible rounding is the
panel's; the CEF rectangle never reaches the rounded edge.

### ECS ↔ native boundary

All cross-boundary behavior goes through Bevy messages/resources (per project convention), not
direct calls:

- `SummonCommandBar` (message) — native hotkey/waker → ECS, "show the panel now".
- `CommandBarPanelShow` / `CommandBarPanelHide` (messages) — ECS → native window ordering.
- `CommandBarPanelResize { logical_size }` (message) — ECS (from the page size event) → native
  `setFrame`.
- `CommandBarPanelDismissed` (message) — native (`resignKey`) → ECS, "treat as closed".
- `CommandBarPanelView` (component marker) — identifies the panel's webview entity.
- `CommandBarPanelGeometry { position, size }` (persisted singleton component) — saved/restored.

## Components

### 1. `crates/vmux_layout/src/command_bar/panel.rs` (new)

Filename-module pattern (no `mod.rs`); register in `crates/vmux_layout/src/command_bar.rs` and
the command-bar plugin.

- Defines `CommandBarPanelView` marker, the messages above, and
  `build_command_bar_open_payload(...) -> CommandBarOpenEvent` (the shared gatherer — see §2).
- System `summon_command_bar_panel`: on `SummonCommandBar`, builds the payload with
  `native_windowed = true`, emits `COMMAND_BAR_OPEN_EVENT` (`BinHostEmitEvent::from_rkyv`) to the
  `CommandBarPanelView` entity, and sends `CommandBarPanelShow`. Reuses the windowed reveal-timing
  helpers already present for `native_windowed`.
- System `resize_command_bar_panel`: observes the page's command-bar size event for the panel
  entity → sends `CommandBarPanelResize`.
- System `close_command_bar_panel`: on `Esc`/accept/`CommandBarPanelDismissed`, sends
  `CommandBarPanelHide` and resets the panel page state.
- Accept path is unchanged/shared: the page emits the same actions, consumed by the existing
  `handler.rs` action systems (global `AppCommand`s, not `Modal`-bound).

### 2. Shared payload gatherer — refactor `crates/vmux_layout/src/command_bar/handler.rs`

Extract the data-gathering currently inside `handle_open_command_bar` (spaces/tabs/commands/
pages/url, roughly handler.rs:600-919) into:

- a `SystemParam` bundle `CommandBarPayloadSources` exposing the needed queries/resources, and
- a free fn `build_command_bar_open_payload(open_id, native_windowed, target, &sources) ->
  CommandBarOpenEvent` (supersedes the thin `command_bar_open_payload` at handler.rs:962).

`handle_open_command_bar` (modal path) and `summon_command_bar_panel` (panel path) both call it.
The modal path keeps all its existing `Modal`-entity reveal/display logic; only the data-gathering
is shared. **Behavior of the modal must not change** — guarded by the `include_str!`/page-source
tests (`style.rs` tests; `tests/page_source.rs`) which require native `cargo test -p vmux_layout`.

### 3. bevy_cef patch — `patches/bevy_cef-0.5.2/src/webview.rs`

- New component `WebviewExternalHost(pub usize)` (raw `*mut NSView`), exported from the patched
  crate's prelude.
- In `create_webviews`: when an entity has `WebviewExternalHost`, create the browser as
  **windowed** parented to that NSView pointer (build a `RawWindowHandle::AppKit`), bypassing the
  winit-handle path and the `if windowed && host_window.is_none() { continue; }` guard.
- Native first responder: allow the panel webview to take native focus (use the existing
  `allow_native_focus` / `set_windowed_focus` path, `browsers.rs:525-531`) and ensure the
  `FocusCanceler` (`patches/bevy_cef_core-0.5.2/src/browser_process/client_handler.rs:23-39`) does
  **not** cancel first-responder for this entity. Gate this on a flag/marker so existing windowed
  content panes keep current behavior.
- Note: `cargo fmt` rewrites vendored `patches/` — after fmt, `git checkout -- patches/` and
  re-apply only the intended patch edits before committing (per project convention).

### 4. `crates/vmux_desktop/src/command_bar_panel.rs` (new)

All macOS-native, `#[cfg(target_os = "macos")]`. Modeled on `glass.rs`.

- Startup: create the `NSPanel` (borderless `NonactivatingPanel`, floating window level
  `NSWindowLevel::Floating` (or status), `collectionBehavior = CanJoinAllSpaces |
  FullScreenAuxiliary | IgnoresCycle`, `opaque(false)`, clear bg, `hasShadow(true)`,
  `movableByWindowBackground(true)`). Content view = rounded layer (`cornerRadius`,
  `masksToBounds`) hosting an `NSGlassEffectView` tray; reserve inset padding for the CEF child
  view. Start ordered out (hidden). Store the panel + content-view pointer in a `NonSend` resource.
- Spawn the `CommandBarPanelView` webview entity once the content-view pointer exists:
  `WebviewSource::new(COMMAND_BAR_PAGE_URL)`, `WebviewWindowed`, `WebviewExternalHost(view_ptr)`,
  `WebviewWindowedNativeFocus`, `CommandBarPanelView`. CEF creates the windowed browser into the
  panel and is kept warm across summons.
- Systems:
  - `apply_panel_show`: on `CommandBarPanelShow`, position the panel (remembered geometry if on a
    live screen, else centered on the screen under the mouse cursor), `orderFrontRegardless` +
    `makeKeyWindow`. Bump winit power mode (Reactive) so CEF paints.
  - `apply_panel_hide`: on `CommandBarPanelHide`, `orderOut`.
  - `apply_panel_resize`: on `CommandBarPanelResize`, `setFrame_display` (preserve top-left;
    grow downward), keep the CEF child view inset.
  - `observe_panel_resign_key`: `NSWindowDidResignKeyNotification` → send
    `CommandBarPanelDismissed`.
  - `observe_panel_move`: `NSWindowDidMove` / move-end → update `CommandBarPanelGeometry`.
- Accept-activates-vmux: when the accepted action targets the layout, activate the main window via
  the existing `background_lifecycle::ensure_native_window_active` path.

### 5. Global hotkey — `crates/vmux_desktop` + `global-hotkey` dep

- Add `global-hotkey` to `crates/vmux_desktop/Cargo.toml`.
- `GlobalHotKeyManager::new()` on the **main thread** at startup, held as a `NonSend` resource
  (dropping it unregisters). Register `HotKey` parsed from settings (`"super+shift+Space"`
  default).
- Wake integration: a dedicated thread blocks on `GlobalHotKeyEvent::receiver().recv()`; on a
  `Pressed` event for our hotkey id, it pushes to a shared queue (e.g. `Arc<SegQueue>`/`Mutex<Vec>`)
  **and** calls `EventLoopProxy::send_event(WinitUserEvent::WakeUp)` so the reactive loop ticks.
  Required because in `UpdateMode::Reactive` the loop is asleep when idle/unfocused and would not
  otherwise poll the channel. (This mirrors the documented vmux wake pattern; never switch to
  `Continuous`.)
- A Bevy system drains the queue → sends `SummonCommandBar`.

### 6. Settings — `crates/vmux_setting`

- Add `command_bar.global_hotkey: String`. Absent ⇒ fall back to the default
  `"super+shift+Space"` in code (no auto-seed of the config file, per project convention).
- Parse to `global_hotkey::hotkey::HotKey` via `FromStr`; on parse error, log and use the default.

### 7. Persistence — `crates/vmux_desktop/src/persistence.rs` + `window_state.rs`

- New persisted singleton component `CommandBarPanelGeometry { position: Option<IVec2>, size:
  Option<Vec2> }`, added to the moonshine-save allow-list alongside `WindowGeometry`
  (`persistence.rs:190`). Capture on panel move/resize; restore on summon, validated to lie on a
  currently-connected `NSScreen` (else re-center on the cursor screen).

## Data flow

1. **Summon** — global hotkey fires → waker thread wakes the loop + enqueues → Bevy drains →
   `SummonCommandBar` → `summon_command_bar_panel` builds payload (`native_windowed = true`) +
   emits `COMMAND_BAR_OPEN_EVENT` to the panel entity + `CommandBarPanelShow` → native shows/keys
   the panel and bumps power mode → CEF paints the page.
2. **Input** — panel is key, CEF view is first responder → CEF handles keyboard/mouse natively;
   the page filters results client-side from the open payload.
3. **Resize** — page emits its size event for the panel entity → `CommandBarPanelResize` → native
   `setFrame`.
4. **Accept** — page emits an action → existing `handler.rs` action systems → `AppCommand` →
   layout; if the action targets the layout, activate the main vmux window; then
   `CommandBarPanelHide`.
5. **Dismiss** — `resignKey` (blur) or `Esc` → `CommandBarPanelDismissed` / close →
   `CommandBarPanelHide` (`orderOut`). Because the panel is nonactivating, focus returns to the
   previously active app automatically.

## Build sequence (one spec, two milestones)

### M1 — Panel as a new in-app surface (no global hotkey, no modal changes)

1. bevy_cef patch: `WebviewExternalHost` + windowed-parent-to-NSView + native first-responder gate.
2. `command_bar_panel.rs` (native): build the NSPanel (rounded glass tray, inset CEF, drag,
   shadow), store view ptr, spawn the `CommandBarPanelView` webview entity (prewarmed).
3. Extract `build_command_bar_open_payload` + `CommandBarPayloadSources`; switch the modal path to
   use it (assert no behavior change).
4. `command_bar/panel.rs` (ECS): `SummonCommandBar`/show/hide/resize/dismiss messages + systems.
5. Wire a **temporary** trigger (e.g. reuse the `Cmd+K` request to *also* fire `SummonCommandBar`,
   or a dev-only keybind) to exercise show/typing/accept/resize/dismiss-on-blur/drag+remember.
6. Accept-activates-vmux; position persistence.

End state: a floating windowed-CEF command bar above vmux, fully functional, summoned by the
temporary trigger. The `Cmd+K` modal is unchanged.

### M2 — System-wide summon

1. Add `global-hotkey`; create the manager (main thread) + register `Cmd+Shift+Space` from
   settings.
2. Waker thread → `EventLoopProxy` WakeUp + queue → `SummonCommandBar`.
3. Summon from hidden/unfocused: ensure power-mode bump shows + paints the panel while vmux is
   backgrounded; remove the temporary trigger.
4. Settings key for the chord.

## Testing

Per the project's finish-then-test workflow, one runtime pass at the end of each milestone:

- Native unit tests: `cargo test -p vmux_layout` (guards the modal/page source-scrape tests after
  the payload extraction) and `cargo test -p vmux_desktop`.
- Runtime (manual) — M1: summon while vmux focused; type and see filtered results; arrow/enter
  selects; accept opens in the layout and activates the main window; click another window →
  dismiss-on-blur; `Esc` dismisses; drag the panel and confirm position is remembered after
  reopen; multi-monitor placement.
- Runtime (manual) — M2: summon from a different frontmost app; summon while vmux is hidden to the
  menu bar; confirm the panel becomes key without foregrounding vmux and focus returns on dismiss;
  confirm idle CPU is unchanged (no `Continuous` update mode).

## Risks / mitigations

- **Vendored patch churn** — `cargo fmt` rewrites `patches/`; `git checkout -- patches/` after fmt,
  commit only intended patch hunks.
- **CEF painting while backgrounded** — must bump winit power mode on summon; verify frames render
  when vmux is hidden/unfocused (validate early in M2).
- **Native first responder for windowed CEF** — depends on suppressing `FocusCanceler` for the
  panel entity and enabling native focus; validate keyboard reaches CEF while another app is the
  active app (validate early in M1).
- **Payload-gatherer extraction regressions** — keep the modal path byte-for-byte equivalent;
  rely on `cargo test -p vmux_layout`.
- **Corner aesthetics** — square CEF over arbitrary apps mitigated by the inset rounded glass tray.
- **Two warm CEF browsers** — slightly higher memory; acceptable for an always-ready Spotlight bar.
