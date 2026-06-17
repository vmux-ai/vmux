# Window survives monitor removal + relocates to a live display

Date: 2026-06-17
Status: Approved

## Problem

The Vmux window sometimes disappears and cannot be reopened — the OS-menu / tray
"Open Window" does nothing. Observed in production, more often in dev.

Log signature:

```
INFO bevy_winit::system: Monitor removed 463v0
INFO bevy_winit::system: Monitor removed 856v1
INFO bevy_winit::system: Closing window 65v0
WARN bevy_winit::state: Skipped event Destroyed for unknown winit Window Id WindowId(1288494651008)
```

## Root cause

A Bevy relationship semantic, not vmux code.

1. On display sleep / lock / disconnect, macOS drops the `NSScreen`(s) — including the
   built-in panel on a MacBook during sleep (the log shows *both* monitors removed).
2. `bevy_winit::create_monitors` despawns the corresponding `Monitor` entities
   (`bevy_winit/system.rs:232`) → `Monitor removed …`.
3. bevy_winit auto-adds `OnMonitor(monitor)` to the window (`bevy_winit/system.rs:484`).
4. `bevy_window` declares the reverse side as
   `#[relationship_target(relationship = OnMonitor, linked_spawn)]` on `HasWindows`
   (`bevy_window/monitor.rs:60`). **`linked_spawn` recursively despawns every window
   linked to a monitor when that monitor is despawned** → `Closing window 65v0`.
5. The dropped `NSWindow`'s raw id later emits a winit `Destroyed` that bevy no longer
   tracks → `Skipped event Destroyed for unknown winit Window Id` (a symptom).
6. `WindowPlugin { close_when_requested: false, exit_condition: DontExit }`
   (`vmux_desktop/lib.rs:75-76`) keeps the process alive — now with **zero `Window`
   entities**.

Why "Open Window" then fails: the reopen path only flips `window.visible = true` on
existing windows (`background_lifecycle.rs:344`, tray `ShowAllWindows`). Zero windows →
no-op. The architecture assumes the primary window entity never dies, only hides.

This is a recognized Bevy footgun (bevyengine/bevy#20252): `linked_spawn` makes it easy
to lose windows on monitor changes. A window outliving its monitor is normal for a
desktop app.

## Goals

- The primary window must survive monitor removal (sleep, lock, unplug). Never despawned.
- After a monitor change, if the window ends up off every live display, move it onto a
  surviving display.
- Fix at the root semantic, not via an application-layer workaround.

## Non-goals

- No respawn / re-bind path. The window entity id is referenced throughout the layout
  via `HostWindow(pw)`; keeping the *same* entity alive is required, so respawning is
  explicitly out of scope.
- No change to the hide-on-close or tray reopen behavior (they work once the entity
  survives).

## Design

### Component A — patch `bevy_window` to drop `linked_spawn` (the fix)

Vendor `bevy_window` and remove the cascade.

- Copy the published crate from the cargo registry
  (`bevy_window-0.19.0-rc.2`) into `patches/bevy_window-0.19.0-rc.2`.
- In `src/monitor.rs`, change:
  - `#[relationship_target(relationship = crate::window::OnMonitor, linked_spawn)]`
  - → `#[relationship_target(relationship = crate::window::OnMonitor)]`
- Wire the patch in the root `Cargo.toml`:
  - `[patch.crates-io]` → `bevy_window = { path = "patches/bevy_window-0.19.0-rc.2" }`
  - `members = ["patches/*"]` already picks it up; `patches/bevy_window-0.19.0-rc.2`
    must not be in the workspace `exclude` list.

Effect: despawning a `Monitor` now merely clears the `OnMonitor` relationship from its
windows (standard relationship cleanup) instead of despawning them. Fixes all windows,
no framework-fighting. Matches the published rc exactly except the one attribute.

Maintenance: the patch tracks `0.19.0-rc.2`. Re-check when bumping to 0.19 stable
(same workflow as the existing bevy_cef / moonshine patches).

### Component B — relocate the window onto a live display

A vmux system (in `vmux_desktop`, alongside the other native/window-lifecycle modules)
that runs when the `Monitor` set changes (`Added<Monitor>` or
`RemovedComponents<Monitor>` non-empty):

- If there are zero `Monitor` entities (mid-sleep), do nothing — revisit when a monitor
  reappears.
- Otherwise, read the window's live outer rect (via the winit window, as
  `activate_native_window` already does through `WINIT_WINDOWS`) and test intersection
  against each `Monitor`'s rect (`physical_position` + physical size).
- If the window intersects no monitor, recenter it on the primary display by setting
  `window.position = WindowPosition::Centered(MonitorSelection::Primary)`.
- If it still intersects some monitor, leave it (avoid a jarring jump; let macOS keep
  its placement).

Covers: unplug one of several displays → window was on the dead one → recentered onto a
live one; sleep → wake → if the window came back stranded, recenter, else leave it.

## Testing

- **A (headless ECS):** spawn a `Window` entity and two `Monitor` entities, link the
  window with `OnMonitor(monitor)`, `despawn` one monitor → assert the window entity
  still exists. Fails before the patch (window despawned), passes after.
- **B (headless ECS):** with the relocate system, place the window outside all `Monitor`
  rects and run the system → assert `window.position` becomes
  `Centered(MonitorSelection::Primary)`. Place it overlapping a monitor → assert
  position unchanged. Zero monitors → assert no panic, no change.

Both tests are native-Bevy ECS (no real winit window); the `linked_spawn` despawn is
driven by component hooks that are active without `WindowPlugin`.

## Risks

- Patching `bevy_window` invalidates most of the build graph → the worktree's first
  build is long (compounded by cold CEF). One-time cost.
- The vendored crate's `Cargo.toml` (published form) must build standalone as a
  workspace member; adjust any leftover workspace-inherited fields to pinned
  `0.19.0-rc.2` deps if the build complains.
- Reading the window's live position: prefer the winit outer position (reliable) over
  `Window.position`, which may be `Automatic`/`Centered` rather than a concrete rect.

## Out of scope

- bevy_winit's monitor despawn behavior (correct — it mirrors the OS).
- Multi-window scenarios (vmux is single-window; the fix is general regardless).
