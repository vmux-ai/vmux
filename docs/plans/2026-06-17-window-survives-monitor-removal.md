# Window Survives Monitor Removal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the Vmux window from being despawned when a monitor is removed (sleep/lock/unplug), and relocate it onto a live display if it ends up off-screen.

**Architecture:** Component A patches `bevy_window` to drop the `linked_spawn` attribute on the `HasWindows` relationship target, so despawning a `Monitor` entity no longer cascade-despawns the windows on it. Component B adds a pure-ECS vmux system that, when the monitor set changes, recenters the primary window on the primary display if its frame intersects no live `Monitor`.

**Tech Stack:** Rust, Bevy 0.19.0-rc.2 (`bevy_window`, `bevy_winit`), cargo `[patch.crates-io]` vendoring (existing pattern in `patches/`).

**Build note:** Patching `bevy_window` invalidates most of the Rust build graph. The first `cargo test`/`cargo build` after wiring the patch is long (the native CEF blob is NOT rebuilt, but most Rust crates are). Subsequent edits to `patches/bevy_window` recompile bevy + dependents only. Budget for it.

**Spec:** `docs/specs/2026-06-17-window-survives-monitor-removal-design.md`

---

## File Structure

- `patches/bevy_window-0.19.0-rc.2/` — vendored copy of the published crate, one attribute changed. New.
- `Cargo.toml` — add the `[patch.crates-io]` entry. Modify.
- `crates/vmux_desktop/src/display.rs` — relocate system + pure geometry helper + all tests for both components. New.
- `crates/vmux_desktop/src/lib.rs` — declare `mod display;` and register `display::DisplayPlugin`. Modify.

Both tests live in `vmux_desktop` because the relocate system belongs next to the other native/window-lifecycle modules (`background_lifecycle`, `glass`, `splash`), and the Component A test only needs the patched `bevy_window` types, which are available there.

---

## Task 1: Component A — patch `bevy_window` so windows survive monitor despawn

**Files:**
- Create: `crates/vmux_desktop/src/display.rs`
- Modify: `crates/vmux_desktop/src/lib.rs` (add `mod display;`)
- Create: `patches/bevy_window-0.19.0-rc.2/` (vendored)
- Modify: `Cargo.toml` (`[patch.crates-io]`)

- [ ] **Step 1: Declare the new module**

In `crates/vmux_desktop/src/lib.rs`, add the module declaration next to the other `mod` lines (the block at lines 8–28, e.g. right after `mod background_lifecycle;`):

```rust
mod display;
```

- [ ] **Step 2: Write the failing test**

Create `crates/vmux_desktop/src/display.rs` with only the test (the production code comes in Task 2):

```rust
#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use bevy::window::{Monitor, OnMonitor, Window};

    fn test_monitor(x: i32, y: i32, w: u32, h: u32) -> Monitor {
        Monitor {
            name: Some("test".to_string()),
            physical_height: h,
            physical_width: w,
            physical_position: IVec2::new(x, y),
            refresh_rate_millihertz: Some(60_000),
            scale_factor: 1.0,
            video_modes: Vec::new(),
        }
    }

    #[test]
    fn despawning_monitor_keeps_linked_window() {
        let mut world = World::new();
        let monitor = world.spawn(test_monitor(0, 0, 1920, 1080)).id();
        let window = world.spawn(Window::default()).id();
        // Inserting OnMonitor registers the window in the monitor's HasWindows relationship.
        world.entity_mut(window).insert(OnMonitor(monitor));

        world.entity_mut(monitor).despawn();

        assert!(
            world.get_entity(window).is_ok(),
            "window must survive monitor despawn (linked_spawn cascade must be gone)"
        );
    }
}
```

- [ ] **Step 3: Run the test to confirm it fails against the registry crate**

Run: `cargo test -p vmux_desktop despawning_monitor_keeps_linked_window`
Expected: FAIL — `window must survive monitor despawn`. The registry `bevy_window` still has `linked_spawn`, so despawning the monitor despawns the window. (This is the long build.)

- [ ] **Step 4: Vendor `bevy_window` and make it writable**

Run:

```bash
cp -R ~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_window-0.19.0-rc.2 patches/bevy_window-0.19.0-rc.2
chmod -R u+w patches/bevy_window-0.19.0-rc.2
```

- [ ] **Step 5: Remove the `linked_spawn` attribute**

In `patches/bevy_window-0.19.0-rc.2/src/monitor.rs`, change the `HasWindows` relationship target (around line 60):

```rust
// before
#[relationship_target(relationship = crate::window::OnMonitor, linked_spawn)]
pub struct HasWindows(Vec<Entity>);
```

```rust
// after
#[relationship_target(relationship = crate::window::OnMonitor)]
pub struct HasWindows(Vec<Entity>);
```

- [ ] **Step 6: Wire the patch in the root `Cargo.toml`**

In the `[patch.crates-io]` section (currently lines 137–141), add:

```toml
bevy_window = { path = "patches/bevy_window-0.19.0-rc.2" }
```

The existing `members = ["crates/*", "patches/*"]` picks the crate up automatically; do not add it to the `exclude` list.

- [ ] **Step 7: Run the test to confirm it passes**

Run: `cargo test -p vmux_desktop despawning_monitor_keeps_linked_window`
Expected: PASS. (Rebuilds bevy + dependents; CEF native blob is reused.)

- [ ] **Step 8: Commit**

```bash
git add patches/bevy_window-0.19.0-rc.2 Cargo.toml Cargo.lock crates/vmux_desktop/src/display.rs crates/vmux_desktop/src/lib.rs
git commit -m "fix(window): drop linked_spawn so monitor removal can't despawn the window"
```

---

## Task 2: Component B — relocate a stranded window to a live display

**Files:**
- Modify: `crates/vmux_desktop/src/display.rs` (add helper, system, plugin, tests)
- Modify: `crates/vmux_desktop/src/lib.rs` (register `display::DisplayPlugin`)

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `mod tests` in `crates/vmux_desktop/src/display.rs` (alongside `despawning_monitor_keeps_linked_window`). Add `use super::*;` at the top of `mod tests` so the production items are in scope:

```rust
    #[test]
    fn off_all_monitors_detects_stranded_window() {
        let monitor = IRect::from_corners(IVec2::ZERO, IVec2::new(1920, 1080));
        let stranded = IRect::from_corners(IVec2::new(5000, 5000), IVec2::new(6280, 5720));
        let overlapping = IRect::from_corners(IVec2::new(100, 100), IVec2::new(1380, 820));

        assert!(window_off_all_monitors(stranded, &[monitor]));
        assert!(!window_off_all_monitors(overlapping, &[monitor]));
        assert!(window_off_all_monitors(stranded, &[]));
    }

    fn relocate_app() -> App {
        let mut app = App::new();
        app.add_message::<()>();
        app.add_systems(Update, relocate_window_to_live_display);
        app
    }

    #[test]
    fn stranded_window_is_recentered_on_primary() {
        let mut app = relocate_app();
        let window = app
            .world_mut()
            .spawn((
                Window {
                    position: WindowPosition::At(IVec2::new(5000, 5000)),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        app.world_mut().spawn(test_monitor(0, 0, 1920, 1080));

        app.update();

        assert!(matches!(
            app.world().get::<Window>(window).unwrap().position,
            WindowPosition::Centered(MonitorSelection::Primary)
        ));
    }

    #[test]
    fn window_on_a_monitor_is_left_in_place() {
        let mut app = relocate_app();
        let window = app
            .world_mut()
            .spawn((
                Window {
                    position: WindowPosition::At(IVec2::new(100, 100)),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        app.world_mut().spawn(test_monitor(0, 0, 1920, 1080));

        app.update();

        assert!(matches!(
            app.world().get::<Window>(window).unwrap().position,
            WindowPosition::At(p) if p == IVec2::new(100, 100)
        ));
    }

    #[test]
    fn zero_monitors_does_not_relocate() {
        let mut app = relocate_app();
        let window = app
            .world_mut()
            .spawn((
                Window {
                    position: WindowPosition::At(IVec2::new(5000, 5000)),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        let monitor = app.world_mut().spawn(test_monitor(0, 0, 1920, 1080)).id();
        app.update();
        app.world_mut().entity_mut(monitor).despawn();

        app.update();

        assert!(matches!(
            app.world().get::<Window>(window).unwrap().position,
            WindowPosition::At(p) if p == IVec2::new(5000, 5000)
        ));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vmux_desktop --lib display::`
Expected: FAIL to compile — `window_off_all_monitors` and `relocate_window_to_live_display` are not defined.

- [ ] **Step 3: Implement the helper, system, and plugin**

Add the production code at the top of `crates/vmux_desktop/src/display.rs` (above `#[cfg(test)] mod tests`):

```rust
use bevy::prelude::*;
use bevy::window::{Monitor, MonitorSelection, PrimaryWindow, Window, WindowPosition};

pub(crate) struct DisplayPlugin;

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, relocate_window_to_live_display);
    }
}

fn monitor_rect(monitor: &Monitor) -> IRect {
    let min = monitor.physical_position;
    let size = IVec2::new(monitor.physical_width as i32, monitor.physical_height as i32);
    IRect::from_corners(min, min + size)
}

fn window_off_all_monitors(window: IRect, monitors: &[IRect]) -> bool {
    !monitors
        .iter()
        .any(|m| !m.intersect(window).is_empty())
}

/// When the monitor set changes (sleep/wake, unplug), recenter the primary window on the primary
/// display if its frame no longer intersects any live monitor. With zero monitors (mid-sleep) there
/// is nothing to place onto, so we wait for a monitor to reappear.
fn relocate_window_to_live_display(
    monitors_added: Query<(), Added<Monitor>>,
    monitors_removed: RemovedComponents<Monitor>,
    monitors: Query<&Monitor>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) {
    if monitors_added.is_empty() && monitors_removed.is_empty() {
        return;
    }
    if monitors.is_empty() {
        return;
    }
    let Ok(mut window) = window.single_mut() else {
        return;
    };
    let WindowPosition::At(pos) = window.position else {
        return;
    };
    let size = window.resolution.physical_size().as_ivec2();
    let window_rect = IRect::from_corners(pos, pos + size);
    let monitor_rects: Vec<IRect> = monitors.iter().map(monitor_rect).collect();
    if window_off_all_monitors(window_rect, &monitor_rects) {
        window.position = WindowPosition::Centered(MonitorSelection::Primary);
    }
}
```

- [ ] **Step 4: Register the plugin**

In `crates/vmux_desktop/src/lib.rs`, add `display::DisplayPlugin` to the second `.add_plugins((...))` tuple (the block at lines 106–114, e.g. after `tray::TrayPlugin,`):

```rust
            .add_plugins((
                AgentPlugin,
                vmux_agent::PageAgentPlugin,
                PersistencePlugin,
                LayoutPlugin,
                updater::VmuxUpdater::builder().build().plugin(),
                background_lifecycle::BackgroundLifecyclePlugin,
                tray::TrayPlugin,
                display::DisplayPlugin,
            ));
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p vmux_desktop --lib display::`
Expected: PASS (4 tests: the survival test from Task 1 plus the 3 relocation tests, and the pure-fn test).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_desktop/src/display.rs crates/vmux_desktop/src/lib.rs
git commit -m "feat(window): relocate window to a live display after monitor changes"
```

---

## Task 3: Workspace build + lint + manual verification

**Files:** none (verification only)

- [ ] **Step 1: Full build confirms the patch compiles across the graph**

Run: `cargo build`
Expected: builds clean (the `bevy_window` patch is consumed by every bevy-dependent crate).

- [ ] **Step 2: Format and lint**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets`
Expected: no diffs from fmt, no new clippy warnings in `vmux_desktop`. Let rustfmt reorder any cfg-gated imports.

- [ ] **Step 3: Manual runtime verification**

Run the app, then:
- Let the display sleep (or lock the screen) and wake it → the window must reappear (previously it vanished permanently).
- If on multiple displays, unplug the one the window is on → the window must move onto a remaining display.
- Confirm the tray "Open Window" still shows the window after a hide.

Expected: window survives every case; no `Closing window` line in the log on monitor removal.

- [ ] **Step 4: Commit any fmt-only changes** (if Step 2 produced any)

```bash
git add -A
git commit -m "style: cargo fmt"
```

---

## Self-Review

- **Spec coverage:** Component A (patch `bevy_window`, drop `linked_spawn`) → Task 1. Component B (relocate on monitor change, skip when zero monitors, recenter on primary, leave if on a monitor) → Task 2. Tests A + B → Tasks 1–2. Build/patch-graph risk → Task 3. All spec sections covered.
- **Placeholder scan:** none — every code/command step is concrete.
- **Type consistency:** `window_off_all_monitors(IRect, &[IRect]) -> bool`, `monitor_rect(&Monitor) -> IRect`, `relocate_window_to_live_display` system, and `DisplayPlugin` are named identically in their definition (Task 2 Step 3), registration (Task 2 Step 4), and tests (Task 2 Step 1). `test_monitor` is defined once (Task 1 Step 2) and reused by Task 2 tests in the same `mod tests`.
