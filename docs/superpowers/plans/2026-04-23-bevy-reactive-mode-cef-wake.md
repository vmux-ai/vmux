# Bevy Reactive Mode + CEF Wake Integration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Bevy's `Continuous` update mode with event-driven wakes so the app sleeps when idle. Integrate CEF's message-pump scheduling into winit's event loop so webviews stay responsive without polling every frame.

**Architecture:** Stock `bevy_winit::WinitPlugin` already exposes an `EventLoopProxyWrapper` resource that any code path can use to wake the loop. A full custom `app.set_runner` is not required — we use `WinitSettings::desktop_app()` (reactive mode) plus proactive wake-ups from CEF's `BrowserProcessHandler::on_schedule_message_pump_work` callback. A short `reactive(wait)` interval serves as a safety-net fallback.

**Tech Stack:** Rust 2024 edition, Bevy 0.18 (`bevy_winit`, `WinitSettings`, `EventLoopProxyWrapper`, `WinitUserEvent`), local patches of `bevy_cef 0.5.2` and `bevy_cef_core 0.5.2` under `patches/`.

**Scope:** This plan covers the Bevy runner / CEF wake integration only. The investigation surfaced other independent optimizations (trimming `DefaultPlugins`, lowering CEF `windowless_frame_rate`, adding `Changed<_>` filters to per-frame systems, converting polling systems to event readers). Those are deliberately **out of scope** for this plan — they should get their own plan(s) once this lands and we can measure incremental wins cleanly.

**Out-of-scope (follow-up plans):**
- Disable unused plugins from `DefaultPlugins` (`AudioPlugin`, `GilrsPlugin`, `GltfPlugin`, `AnimationPlugin`, `GizmoPlugin`)
- Make CEF `windowless_frame_rate` configurable / drop from 120 → 60
- Gate per-frame systems with `Changed<_>` / `Added<_>` / `run_if`
- Convert `poll_cursor_pane_focus` and `pane_gap_drag_resize` to event-driven

**Testing approach:** There is no existing test harness for runner behavior; CPU-idle and webview-responsiveness are inherently manual/observational. Each task includes a **Smoke** step with explicit PASS/FAIL criteria (Activity Monitor CPU%, visible behavior) in place of unit tests. Compile checks via `cargo check -p vmux_desktop` are automated.

**Key files touched:**
- `crates/vmux_desktop/src/lib.rs` — app construction, insert `WinitSettings`
- `patches/bevy_cef_core-0.5.2/src/browser_process/browser_process_handler.rs` — add proxy field, wake on schedule callback
- `patches/bevy_cef_core-0.5.2/src/browser_process/app.rs` — thread proxy through `BrowserProcessAppBuilder`
- `patches/bevy_cef-0.5.2/src/common/message_loop.rs` — thread proxy from Bevy app into CEF
- `patches/bevy_cef_core-0.5.2/src/browser_process.rs` — re-export updated builder API (if needed)

---

## Phase 0 — Reactive update mode

Smallest possible change: flip Bevy into reactive mode. No wake plumbing yet. This phase intentionally ships with known UX regressions (webview animations stutter without input) to establish a clean baseline before Phase 1 fixes them.

### Task 0.1: Insert `WinitSettings::desktop_app()`

**Files:**
- Modify: `crates/vmux_desktop/src/lib.rs`

- [ ] **Step 1: Read the current lib.rs to locate insertion point**

Run: `head -60 crates/vmux_desktop/src/lib.rs`
Expected: see the `VmuxPlugin::build` function starting around line 30, `app.add_plugins((...))` starting around line 46.

- [ ] **Step 2: Add `WinitSettings` to the import line**

Change line 16 from:

```rust
use bevy::winit::WinitWindows;
```

to:

```rust
use bevy::winit::{WinitSettings, WinitWindows};
```

- [ ] **Step 3: Insert `WinitSettings::desktop_app()` as a resource**

Change the `app.add_plugins((` call at line 46 from:

```rust
        app.add_plugins((
            DefaultPlugins
                .set(WebAssetPlugin {
                    silence_startup_warning: true,
                })
                .set(window_plugin)
                .set(bevy::log::LogPlugin {
                    filter: "bevy_camera_controller=warn".into(),
                    ..default()
                }),
```

to:

```rust
        app.insert_resource(WinitSettings::desktop_app())
        .add_plugins((
            DefaultPlugins
                .set(WebAssetPlugin {
                    silence_startup_warning: true,
                })
                .set(window_plugin)
                .set(bevy::log::LogPlugin {
                    filter: "bevy_camera_controller=warn".into(),
                    ..default()
                }),
```

- [ ] **Step 4: Compile check**

Run: `bash -c "env -u CEF_PATH cargo check -p vmux_desktop 2>&1 | tail -5"`
Expected: `Finished \`dev\` profile` with no errors.

- [ ] **Step 5: Build debug binary**

Run: `bash -c "env -u CEF_PATH cargo build -p vmux_desktop --features debug 2>&1 | tail -5"`
Expected: `Finished \`dev\` profile` with no errors, binary at `target/debug/Vmux`.

- [ ] **Step 6: Smoke — app launches and window appears**

Run: `bash -c "env -u CEF_PATH ./target/debug/Vmux" &` (in separate terminal)
Expected: the Vmux window appears within ~5s. You see the glass UI and a browser webview.
PASS if window appears. FAIL if crash or blank window — re-read step 3 for typo.

- [ ] **Step 7: Smoke — idle CPU drops**

With the window visible and NOT touching mouse/keyboard, open Activity Monitor, find "Vmux", observe CPU %.
Expected: CPU % significantly lower than before the change (was continuous render; now reactive).
PASS if idle CPU drops notably (e.g. from 15–30% to under 5%). Record the numbers for reference.
Known regression: webview animations will stutter — this is expected and fixed in Phase 1.

- [ ] **Step 8: Smoke — input still works**

Move mouse over the webview, click the URL bar, type a URL, press Enter.
Expected: the URL bar accepts input, the webview navigates. Pane focus ring appears on hover (after ~80ms debounce).
PASS if input is responsive (modulo some stutter between keystrokes).

- [ ] **Step 9: Commit**

```bash
git add crates/vmux_desktop/src/lib.rs
git commit -m "feat: switch Bevy to reactive update mode via WinitSettings::desktop_app"
```

---

## Phase 1 — CEF wake integration

Phase 0 made the app sleep, but CEF's internal message pump now only runs when winit wakes (on window events). CEF-initiated work (JS timers, painting, IPC from render process) does not wake the loop — it waits up to 5 seconds for the reactive fallback. Phase 1 plumbs CEF's `on_schedule_message_pump_work` callback to wake winit immediately via `EventLoopProxyWrapper`.

### Task 1.1: Add `EventLoopProxy` field to CEF browser-process handler

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/browser_process_handler.rs`

- [ ] **Step 1: Add the proxy type alias and struct field**

Replace the current `BrowserProcessHandlerBuilder` struct (lines 9–13) with:

```rust
use winit::event_loop::EventLoopProxy;

pub type WakeProxy = EventLoopProxy<bevy_winit::WinitUserEvent>;

pub struct BrowserProcessHandlerBuilder {
    object: *mut RcImpl<cef_dll_sys::cef_browser_process_handler_t, Self>,
    message_loop_working_requester: Sender<MessageLoopTimer>,
    extensions: CefExtensions,
    wake_proxy: Option<WakeProxy>,
}
```

- [ ] **Step 2: Update the `build` signature**

Change the `impl BrowserProcessHandlerBuilder` block (lines 15–26) to:

```rust
impl BrowserProcessHandlerBuilder {
    pub fn build(
        message_loop_working_requester: Sender<MessageLoopTimer>,
        extensions: CefExtensions,
        wake_proxy: Option<WakeProxy>,
    ) -> BrowserProcessHandler {
        BrowserProcessHandler::new(Self {
            object: core::ptr::null_mut(),
            message_loop_working_requester,
            extensions,
            wake_proxy,
        })
    }
}
```

- [ ] **Step 3: Update `Clone` impl to clone the proxy**

Change the `Clone` impl (lines 43–57) to include `wake_proxy: self.wake_proxy.clone(),` in the returned struct.

- [ ] **Step 4: Wake winit in `on_schedule_message_pump_work`**

Change the `on_schedule_message_pump_work` method (lines 83–87) to:

```rust
    fn on_schedule_message_pump_work(&self, delay_ms: i64) {
        let _ = self
            .message_loop_working_requester
            .send(MessageLoopTimer::new(delay_ms));
        if let Some(proxy) = &self.wake_proxy {
            let _ = proxy.send_event(bevy_winit::WinitUserEvent::WakeUp);
        }
    }
```

- [ ] **Step 5: Add `bevy_winit` and `winit` dependencies to `bevy_cef_core` Cargo.toml**

Check `patches/bevy_cef_core-0.5.2/Cargo.toml` — if `bevy_winit` is not already listed, add:

```toml
bevy_winit = "0.18"
winit = "0.30"
```

(Match the `winit` version already used transitively by `bevy_winit 0.18` — verify with `cargo tree -p winit` if unsure.)

- [ ] **Step 6: Compile check (will fail at callers — expected)**

Run: `bash -c "env -u CEF_PATH cargo check -p bevy_cef_core 2>&1 | tail -20"`
Expected: error `expected 3 arguments, found 2` pointing at `BrowserProcessAppBuilder::build` calls. This is the handoff to Task 1.2.

- [ ] **Step 7: Commit**

```bash
git add patches/bevy_cef_core-0.5.2
git commit -m "feat(bevy_cef_core): add EventLoopProxy field to browser process handler for winit wake"
```

### Task 1.2: Thread `wake_proxy` through `BrowserProcessAppBuilder`

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/app.rs`

- [ ] **Step 1: Read the file**

Run: `cat patches/bevy_cef_core-0.5.2/src/browser_process/app.rs`
Note the existing `message_loop_working_requester` field and `build` signature — we mirror that pattern for `wake_proxy`.

- [ ] **Step 2: Add `wake_proxy` field to `BrowserProcessAppBuilder`**

Following the existing `message_loop_working_requester: Sender<MessageLoopTimer>` field pattern, add:

```rust
use crate::browser_process::browser_process_handler::WakeProxy;

// inside BrowserProcessAppBuilder struct:
wake_proxy: Option<WakeProxy>,
```

- [ ] **Step 3: Update `build` signature to accept `wake_proxy`**

Change the `build` associated function signature from (approximate shape):

```rust
pub fn build(
    message_loop_working_requester: Sender<MessageLoopTimer>,
    config: CommandLineConfig,
    extensions: CefExtensions,
) -> ...
```

to:

```rust
pub fn build(
    message_loop_working_requester: Sender<MessageLoopTimer>,
    config: CommandLineConfig,
    extensions: CefExtensions,
    wake_proxy: Option<WakeProxy>,
) -> ...
```

Store `wake_proxy` in the returned struct.

- [ ] **Step 4: Pass `wake_proxy` to `BrowserProcessHandlerBuilder::build`**

Find the internal call to `BrowserProcessHandlerBuilder::build(...)`. Add `self.wake_proxy.clone()` (or equivalent) as the third argument so it receives what we threaded in.

- [ ] **Step 5: Update `Clone` impl if one exists**

If `BrowserProcessAppBuilder` has a `Clone` impl, add `wake_proxy: self.wake_proxy.clone(),`.

- [ ] **Step 6: Re-export `WakeProxy`**

In `patches/bevy_cef_core-0.5.2/src/browser_process.rs` (or wherever `browser_process_handler` is re-exported), ensure `WakeProxy` is re-exported so `bevy_cef` can reach it:

```rust
pub use crate::browser_process::browser_process_handler::WakeProxy;
```

Add to `prelude` as well if the crate uses a prelude module for its public surface.

- [ ] **Step 7: Compile check (will fail at `bevy_cef` caller — expected)**

Run: `bash -c "env -u CEF_PATH cargo check -p bevy_cef_core 2>&1 | tail -10"`
Expected: `bevy_cef_core` compiles. `bevy_cef` will now fail to compile because of the new required arg — that's Task 1.3.

- [ ] **Step 8: Commit**

```bash
git add patches/bevy_cef_core-0.5.2
git commit -m "feat(bevy_cef_core): thread EventLoopProxy through BrowserProcessAppBuilder"
```

### Task 1.3: Pass `EventLoopProxyWrapper` from Bevy app into CEF

**Files:**
- Modify: `patches/bevy_cef-0.5.2/src/common/message_loop.rs`

- [ ] **Step 1: Read the `MessageLoopPlugin::build` method**

Run: `sed -n '18,70p' patches/bevy_cef-0.5.2/src/common/message_loop.rs`
Note the call to `BrowserProcessAppBuilder::build(tx, self.config.clone(), self.extensions.clone())` around line 30.

- [ ] **Step 2: Grab the proxy from the Bevy world before building the CEF app**

Insert at the top of `build` (before the call to `BrowserProcessAppBuilder::build`):

```rust
let wake_proxy = app
    .world()
    .get_resource::<bevy::winit::EventLoopProxyWrapper>()
    .map(|wrapper| (**wrapper).clone());
```

Notes for the engineer:
- `EventLoopProxyWrapper` is inserted by `WinitPlugin`. Because `CefPlugin` is added as part of a plugin tuple with `DefaultPlugins` in [crates/vmux_desktop/src/lib.rs:46-71](../../crates/vmux_desktop/src/lib.rs), `DefaultPlugins` (which contains `WinitPlugin`) should already be applied before `CefPlugin::build` runs — verify plugin ordering once the code compiles.
- The `.clone()` works because `EventLoopProxy<T>` is `Clone`.
- Use `get_resource` (returns `Option`) so tests or headless setups without winit still compile.

- [ ] **Step 3: Pass `wake_proxy` to the CEF app builder**

Change:

```rust
let mut cef_app =
    BrowserProcessAppBuilder::build(tx, self.config.clone(), self.extensions.clone());
```

to:

```rust
let mut cef_app = BrowserProcessAppBuilder::build(
    tx,
    self.config.clone(),
    self.extensions.clone(),
    wake_proxy,
);
```

- [ ] **Step 4: Compile check**

Run: `bash -c "env -u CEF_PATH cargo check -p vmux_desktop 2>&1 | tail -10"`
Expected: clean build across the entire workspace (both patches and `vmux_desktop`).

- [ ] **Step 5: Build debug binary**

Run: `bash -c "env -u CEF_PATH cargo build -p vmux_desktop --features debug 2>&1 | tail -5"`
Expected: `Finished \`dev\` profile`.

- [ ] **Step 6: Smoke — app launches and CEF renders**

Run: `bash -c "env -u CEF_PATH ./target/debug/Vmux" &`
Expected: window appears within ~5s, main webview paints (displays a page, not a black rectangle).
PASS if webview is rendered. FAIL (black rectangle or app hang) means the wake callback is being invoked from a bad thread — inspect stderr for CEF complaints.

- [ ] **Step 7: Smoke — webview animation no longer stutters**

Navigate to a page with visible animation (e.g. `https://csszengarden.com/` or any page with a loading spinner). Do NOT touch mouse or keyboard.
Expected: animation runs smoothly. The loading spinner rotates continuously, not in 5-second bursts.
PASS if animation is smooth while idle. FAIL means the wake-up isn't actually reaching winit — inspect by adding a `println!("wake")` inside `on_schedule_message_pump_work` to confirm it fires.

- [ ] **Step 8: Smoke — idle CPU still low**

With the window visible and no mouse/keyboard activity, static page (e.g. `about:blank`), observe Activity Monitor CPU%.
Expected: CPU % stays comparable to Phase 0 idle (not regressed back to continuous-render levels).
PASS if idle CPU is in the same order of magnitude as after Task 0.1 Step 7.

- [ ] **Step 9: Commit**

```bash
git add patches/bevy_cef-0.5.2
git commit -m "feat(bevy_cef): wake winit event loop from CEF message-pump scheduler"
```

### Task 1.4: Tune reactive `wait` fallback

**Files:**
- Modify: `crates/vmux_desktop/src/lib.rs`

Rationale: `WinitSettings::desktop_app()` uses `reactive(5s)` when focused. That's the safety-net fallback for anything we forgot to plumb. A shorter fallback (e.g. 500ms) recovers faster from any missed wake without meaningfully increasing idle CPU.

- [ ] **Step 1: Replace `desktop_app()` with an explicit `WinitSettings`**

Change:

```rust
app.insert_resource(WinitSettings::desktop_app())
```

to:

```rust
app.insert_resource(WinitSettings {
    focused_mode: bevy::winit::UpdateMode::reactive(std::time::Duration::from_millis(500)),
    unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(std::time::Duration::from_secs(5)),
})
```

- [ ] **Step 2: Compile check**

Run: `bash -c "env -u CEF_PATH cargo check -p vmux_desktop 2>&1 | tail -5"`
Expected: clean.

- [ ] **Step 3: Build**

Run: `bash -c "env -u CEF_PATH cargo build -p vmux_desktop --features debug 2>&1 | tail -5"`
Expected: clean.

- [ ] **Step 4: Smoke — app still works**

Launch and verify webview renders, input works, animation is smooth.
PASS if no regression from Task 1.3.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/lib.rs
git commit -m "fix: tighten reactive-mode fallback to 500ms for faster recovery from missed wakes"
```

---

## Phase 2 — Measurement checkpoint

Before declaring complete, confirm the before/after numbers.

### Task 2.1: Capture idle CPU baseline

**Files:** none (measurement only)

- [ ] **Step 1: Record current-branch (post-Phase 1) numbers**

With the app launched and idle (1 tab, `about:blank`), observe Activity Monitor for 30s and record:
- Idle CPU % (main process): ______
- Idle energy impact: ______

- [ ] **Step 2: Record `main` branch numbers**

```bash
git stash  # stash any uncommitted work
git checkout main
env -u CEF_PATH cargo build -p vmux_desktop --features debug
env -u CEF_PATH ./target/debug/Vmux
```

Observe for 30s, record:
- Idle CPU % (main process): ______
- Idle energy impact: ______

Quit the app, then:

```bash
git checkout -  # returns to the branch
git stash pop   # if you stashed
```

- [ ] **Step 3: Document results**

If Phase 1 reduced idle CPU by ≥50%, the plan is successful — proceed to Phase 3 (or stop if no further optimization needed).

If Phase 1 did NOT reduce CPU meaningfully, STOP — investigate why before merging. Candidates:
- The `on_schedule_message_pump_work` callback isn't firing with proxy set (add debug logging)
- A different system is burning CPU (run `sample Vmux 30 -f /tmp/out.txt` and inspect for hot stacks)
- CEF's internal rendering at 120Hz is the true dominant cost — then the follow-up plan to drop `windowless_frame_rate` becomes urgent.

---

## Phase 3 — PR preparation

### Task 3.1: Confirm no regressions in common interaction paths

**Files:** none

- [ ] **Step 1: Run the full manual smoke matrix**

With the post-Phase-1 debug build running, verify each item below still works and record PASS/FAIL:
- Open a new tab (⌘T equivalent / keybinding): ______
- Close a tab: ______
- Navigate via URL bar: ______
- Click a link in a webview: ______
- Keyboard input to the webview (typing in a form): ______
- Split pane (keybinding): ______
- Switch between panes (Ctrl+B, h/j/k/l): ______
- Open command palette (⌘K): ______
- Drag pane gap to resize: ______
- Focus ring appears on pane hover: ______
- Webview loading spinner animates smoothly during page load: ______

Any FAIL → document, investigate, fix before moving on. Do not silently paper over.

- [ ] **Step 2: Commit if any fixes applied**

```bash
git add -u
git commit -m "fix: <specific regression>"
```

### Task 3.2: Prepare PR

- [ ] **Step 1: Invoke the writing-pr-description skill**

(This will guide writing the PR title + body using `gh pr create`.)

---

## Known risks / follow-up candidates

These are *out of scope* for this plan but are worth tracking as follow-up work, in rough priority order:

1. **CEF `windowless_frame_rate = 120`** ([browsers.rs:190](../../patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs:190)) — every webview OSR-renders at 120Hz. Under reactive mode this still happens inside CEF's own process. Dropping to 60 (or 30 for unfocused tabs) should yield large GPU+memory savings independently of this plan.
2. **`DefaultPlugins` trim** — `AudioPlugin`, `GilrsPlugin`, `GltfPlugin`, `AnimationPlugin`, `GizmoPlugin` are loaded but unused.
3. **Per-frame systems without change filters** — `sync_focus_ring_to_active_pane`, `push_tabs_host_emit`, `push_pane_tree_emit`, etc. run every tick regardless of whether their inputs changed.
4. **Polling systems** — `poll_cursor_pane_focus` reads cursor every frame; should react to `CursorMoved` events.
5. **Second CEF pump per frame** — [message_loop.rs:193](../../patches/bevy_cef-0.5.2/src/common/message_loop.rs:193) calls `do_message_loop_work()` twice; re-evaluate whether the second pass is still needed after Phase 1.
6. **OSR paint wake** — this plan only wakes on CEF's `on_schedule_message_pump_work`. The OSR paint callback (`TextureSender` delivery) is a second CEF→Bevy channel that also benefits from a proxy wake. If paint latency feels bad after Phase 1, add the same wake plumbing to the texture-delivery side.
