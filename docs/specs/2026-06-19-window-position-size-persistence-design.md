# Persist Window Position, Size & Fullscreen

## Problem

The OS window does not remember its geometry. Every launch runs
`maximize_window_to_screen` (window.rs), which sizes the window to fill the current
monitor once at startup (gated by the one-shot `ScreenMaximized` resource). Position
is never read or saved; size is always "fill the monitor". Quitting a moved,
resized, or fullscreen window and relaunching loses that state.

## Goal

Persist and restore the primary window's geometry across launches:

- **Windowed frame**: position + size.
- **Fullscreen / maximized**: if the window was native-fullscreen (macOS green-button
  / `toggleFullScreen`) or otherwise screen-filling at quit, restore it.
- **First launch** (no saved geometry): open a **centered** window at a default size
  (`1280x800`), not maximized.

Geometry lives in the existing **`store.ron`** moonshine scene
(`shared_data_dir()/store.ron`) as a saved ECS component — reusing the debounced
`AutoSave` save pipeline — not a new file and not in hand-edited `settings.ron`.

## Non-goals

- No separate windowed "maximized" (zoom) state distinct from fullscreen. vmux hides
  the native window buttons; the green button maps to native fullscreen, and a
  manually screen-filling window is reproduced by its saved size. So "maximized" ==
  fullscreen for restore purposes.
- No multi-window / per-space geometry. There is one OS window; geometry is global
  (one singleton entity in the scene).
- No migration. Absent/again-stale `store.ron` ⇒ first-launch defaults.

## Storage: a saved component in `store.ron`

`store.ron` is a single moonshine scene saved by `save_space_to_path`
(persistence.rs) via a component allowlist and debounced `AutoSave`
(`mark_dirty_on_change` → 0.5s debounce; 60s periodic), loaded on startup via
`LoadWorld`. Window geometry becomes one more saved component on a **singleton
entity**.

New component (in `vmux_layout`, next to `Main`/`VmuxWindow`, mirroring `PaneSize`):

```rust
#[derive(Component, Reflect, Clone, Copy, Debug, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::window"]
#[require(Save)]
pub struct WindowGeometry {
    pub fullscreen: bool,
    pub position: Option<IVec2>, // physical, the WINDOWED outer position
    pub size: Option<Vec2>,      // logical w,h, the WINDOWED size
}
```

- Registered with `app.register_type::<WindowGeometry>()` in the layout plugin (so
  moonshine reflection can (de)serialize it).
- Stable `#[type_path = "vmux_desktop::layout::window"]` — scene files store
  fully-qualified type paths; a stable path avoids the stale-load crash class
  (cf. `stale_space_save_crash`).
- `position`/`size` **always track the windowed frame**, even while fullscreen, so
  exiting fullscreen lands on a sane frame.
- Added to the `save_space_to_path` allowlist: `.allow::<WindowGeometry>()`.

`rebuild_space_views`, stale-detection (`space_is_prompt_only_empty_url`,
`space_contains_stale_agent_url`), and multi-space scoping ignore this entity (no
Tab/Pane/Stack/PageMetadata), so it does not interfere.

## Save (capture)

A `WindowFullscreen(bool)` resource carries the live fullscreen signal:

- **macOS**: `glass.rs::sync_window_glass_visibility` already computes
  `fullscreen = bevy_fullscreen || native_fullscreen` (the latter from NSWindow
  `styleMask().contains(FullScreen)`). It writes that value into `WindowFullscreen`.
- **Linux** (`#[cfg(not(macos))]`): a small system sets `WindowFullscreen` from
  `window.mode` (`BorderlessFullscreen`/`Fullscreen`).

A capture system (poll-and-diff, event-independent) keeps `WindowGeometry` in sync
with the live window and marks the scene dirty:

- Set `WindowGeometry.fullscreen = WindowFullscreen.0`.
- If **not** fullscreen: when `window.position == WindowPosition::At(p)` and
  `window.resolution` differ from the stored values (beyond epsilon), update
  `position`/`size`. While fullscreen, leave `position`/`size` untouched.
- Guards: ignore captures with logical width/height `< 100` (transient); only read
  `At(_)` positions (skip `Centered`/`Automatic` before winit reports a real frame).
- Mutating `WindowGeometry` triggers `Changed<WindowGeometry>`, which is added to
  `mark_dirty_on_change` so the existing `AutoSave` writes `store.ron`.

The capture system runs only after restore is complete (see gating) so the startup
restore can't be clobbered.

## Restore (load)

**1. First-launch default (create time).** `primary_window_config` sets
`position: WindowPosition::Centered(MonitorSelection::Primary)` and
`resolution: 1280x800`. With `maximize_window_to_screen` removed, this is the
first-run window. (Window is hidden until splash reveal on macOS, so no flicker.)

**2. Ensure singleton.** A startup/update system spawns one `WindowGeometry` (default)
if none exists, so first-run geometry is captured and saved. After a load there is
exactly one (dedupe if a load + spawn ever race).

**3. Apply windowed frame (post-load).** When a `WindowGeometry` is `Added` (loaded
from the scene), apply it to the primary window once:
- `position = Some(p)` ⇒ `window.position = WindowPosition::At(p)`.
- `size = Some(s)` ⇒ `window.resolution.set(s.x, s.y)`.
- `None` fields leave the create-time centered default in place (first run).
- If `fullscreen == true`, insert resource `PendingFullscreenRestore` (consumed in
  step 4). Window stays hidden until reveal, so this is flicker-free on macOS.

**4. Restore fullscreen (post-reveal, one-shot).**
- **macOS** (`glass.rs`, owns the NSWindow): after reveal, if `PendingFullscreenRestore`
  exists and the window is not already native-fullscreen, call
  `parent_window.toggleFullScreen(None)`; remove the resource; set
  `WindowRestoreComplete`.
- **Linux**: a system sets `window.mode = BorderlessFullscreen(Primary)`, removes the
  resource, sets `WindowRestoreComplete`.
- If no `PendingFullscreenRestore`, `WindowRestoreComplete` is set immediately after
  the windowed-frame apply.

**Gating.** The capture system runs only once `WindowRestoreComplete` is set,
preventing the transient windowed startup state (before native fullscreen engages)
from overwriting a saved `fullscreen = true`.

The existing `relocate_window_to_live_display` (display.rs) already recenters a
window stranded off all monitors, so restoring an off-screen position on a changed
monitor layout is self-healing.

## Remove launch-maximize

Delete `maximize_window_to_screen` and the `ScreenMaximized` resource. The
"screen-filling ⇒ full padding" decision then relies solely on
`window_uses_full_padding` (window.rs), which already covers:
- Bevy fullscreen `WindowMode` (Linux), and
- native fullscreen + manual maximize via `window_fills_monitor` (the fullscreen/
  filled window's `resolution.physical_size()` equals the monitor size).

Consumers updated to drop the `ScreenMaximized` param:
- `sync_window_layout_to_settings` (window.rs:~648,672).
- `sync_window_padding_to_layout_hidden` (toggle.rs:34,39).

(macOS native fullscreen is also already handled at runtime by `glass.rs`, which sets
the clear-color backdrop based on the styleMask signal.)

## Platform gating

- NSWindow / `toggleFullScreen` / styleMask paths are `#[cfg(target_os = "macos")]`
  (in `glass.rs`, which already holds the `_parent_window` handle).
- Linux uses Bevy `WindowMode` for both the fullscreen signal and restore.
- On Linux the window is visible from creation (no splash), so apply-on-load may
  produce a brief centered→restored resize. CI-only; acceptable.

## Module layout

- `WindowGeometry` component + `register_type`: `vmux_layout` (window.rs).
- `WindowFullscreen`, `PendingFullscreenRestore`, `WindowRestoreComplete`,
  ensure-singleton / capture / apply-on-load systems, allowlist entry: new module
  `crates/vmux_desktop/src/window_state.rs` (`WindowStatePlugin`), wired in
  `vmux_desktop/src/lib.rs`. Filename-based module (no mod.rs).
- Fullscreen capture/restore hooks: `glass.rs` (macOS) + Linux system in
  `window_state.rs`.

## Removed / changed

- `maximize_window_to_screen` system + `ScreenMaximized` resource (window.rs).
- `ScreenMaximized` usage in toggle.rs and window.rs padding systems.
- `primary_window_config` gains centered + `1280x800` defaults.

## Risks

- **Fullscreen detection on macOS** is via NSWindow styleMask, not Bevy `WindowMode`
  (verified in glass.rs). Capture/restore must use the same signal; runtime-verify.
- **Startup ordering**: windowed apply before reveal (flicker-free), fullscreen
  restore after reveal (brief windowed→fullscreen animation — inherent to macOS).
- **store.ron staleness**: if the whole file is wiped by stale-detection, geometry
  resets to first-launch defaults (rare; acceptable).
- **Padding refactor** removing `ScreenMaximized` touches two crates + three tests;
  behavior must stay identical for fullscreen/maximized windows.

## Testing

- Unit: `WindowGeometry` reflection round-trips through a moonshine save/load
  (position/size/fullscreen preserved); capture rules (skip pos/size while
  fullscreen; min-size guard; ignore non-`At` positions).
- System (Bevy app): apply-on-load sets `window.position`/`resolution` from a loaded
  `WindowGeometry`; `fullscreen = true` inserts `PendingFullscreenRestore`; capture
  gated until `WindowRestoreComplete`.
- Padding: replace the `ScreenMaximized`-based tests with fullscreen-mode /
  fills-monitor equivalents in toggle.rs and window.rs.
- `primary_window_config` default-size/centered test.
- Runtime (manual, macOS): move+resize+quit+relaunch restores frame; fullscreen+quit
  +relaunch restores fullscreen; fresh profile opens centered 1280x800.

## Open implementation checks (verify during build, not assumed)

- Bevy reflects `Option<IVec2>` / `Option<Vec2>` through moonshine the same way it
  reflects `Option<String>` (`PageMetadata.bg_color`); if not, fall back to explicit
  scalar fields with sentinels.
- Whether `Added<WindowGeometry>` is the right trigger for apply-on-load given
  moonshine's spawn timing, vs. a `Loaded`-triggered pass like
  `mark_space_views_need_rebuild`.
