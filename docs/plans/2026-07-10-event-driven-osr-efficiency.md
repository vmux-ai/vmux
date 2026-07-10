# Event-Driven OSR Efficiency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop settled User-mode OSR from driving Bevy frames, preserve display-rate Player mode while focused, and fix the Player → User restoration regression.

**Architecture:** Keep the existing macOS CFRunLoop CEF pump and OSR texture-wake pipeline. Remove persistent User-mode paint sources, prevent hidden focus-ring mutation, and add a focused-window Player wake source in `Last`. Restore exit-animation targeting and reset native-view visibility when pages re-enter the windowed backend. Verify a User → Player → User round trip before runtime CPU measurement.

**Tech Stack:** Rust, Bevy 0.19.0-rc.2, Winit reactive update mode, CEF 148 Alloy OSR/windowed rendering, Dioxus/Tailwind layout shell.

---

### Task 1: Remove settled layout CSS animation

**Files:**
- Modify: `crates/vmux_layout/tests/page_source.rs`
- Modify: `crates/vmux_layout/src/page.rs:538-540`
- Modify: `crates/vmux_layout/src/page.rs:646-650`

- [ ] **Step 1: Write the failing source regression test**

Append to `crates/vmux_layout/tests/page_source.rs`:

```rust
#[test]
fn persistent_layout_state_has_no_infinite_animation() {
    let source = include_str!("../src/page.rs");

    for class in ["animate-pulse", "animate-ping", "animate-bounce"] {
        assert!(!source.contains(class), "persistent layout uses {class}");
    }
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p vmux_layout --test page_source persistent_layout_state_has_no_infinite_animation -- --exact
```

Expected: FAIL with `persistent layout uses animate-pulse`.

- [ ] **Step 3: Make done indicators static**

Change the two done-indicator classes in `crates/vmux_layout/src/page.rs` to:

```rust
span { class: "size-2 shrink-0 rounded-full bg-amber-400 ring-2 ring-background" }
```

and:

```rust
span { class: "absolute -bottom-0.5 -right-0.5 size-2 rounded-full bg-amber-400 ring-2 ring-background" }
```

Do not change `animate-spin-once` or loading-only animation outside `page.rs`.

- [ ] **Step 4: Run the test and verify GREEN**

Run:

```bash
cargo test -p vmux_layout --test page_source persistent_layout_state_has_no_infinite_animation -- --exact
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/tests/page_source.rs crates/vmux_layout/src/page.rs
git commit -m "perf(layout): stop idle done indicator animation"
```

### Task 2: Stop hidden focus-ring material updates

**Files:**
- Modify: `crates/vmux_layout/src/focus_ring.rs:185-205`
- Test: `crates/vmux_layout/src/focus_ring.rs`

- [ ] **Step 1: Write the failing ECS test**

Add to the existing `focus_ring.rs` test module:

```rust
#[test]
fn hidden_focus_ring_does_not_advance_gradient_time() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<FocusRingMaterial>>()
        .add_systems(Update, tick_focus_ring_gradient_time);

    let mut material = build_focus_ring_material(
        320.0,
        240.0,
        &test_layout_settings(),
        7.0,
        false,
    );
    material.gradient_params.w = 7.0;
    let handle = app
        .world_mut()
        .resource_mut::<Assets<FocusRingMaterial>>()
        .add(material);
    app.world_mut().spawn((
        FocusRing,
        MeshMaterial3d(handle.clone()),
        Visibility::Hidden,
    ));

    app.update();

    let material = app
        .world()
        .resource::<Assets<FocusRingMaterial>>()
        .get(handle.id())
        .expect("focus ring material");
    assert_eq!(material.gradient_params.w, 7.0);
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p vmux_layout hidden_focus_ring_does_not_advance_gradient_time -- --nocapture
```

Expected: FAIL because the hidden ring's `gradient_params.w` is replaced with elapsed time.

- [ ] **Step 3: Gate the tick system by visibility**

Change `tick_focus_ring_gradient_time` to query visibility and skip hidden rings:

```rust
fn tick_focus_ring_gradient_time(
    time: Res<Time>,
    mut materials: ResMut<Assets<FocusRingMaterial>>,
    rings: Query<
        (&MeshMaterial3d<FocusRingMaterial>, &Visibility),
        With<FocusRing>,
    >,
) {
    let t = time.elapsed_secs();
    for (mesh_mat, visibility) in &rings {
        if matches!(visibility, Visibility::Hidden) {
            continue;
        }
        let id = mesh_mat.id();
        let Some(m) = materials.get(id) else {
            continue;
        };
        if m.gradient_params.x <= 0.5 {
            continue;
        }
        let Some(mut m) = materials.get_mut(id) else {
            continue;
        };
        m.gradient_params.w = t;
    }
}
```

- [ ] **Step 4: Add the visible-ring preservation test**

Add a second test using `Visibility::Visible`, initial `gradient_params.w = -1.0`, one `app.update()`, and:

```rust
assert_ne!(material.gradient_params.w, -1.0);
```

- [ ] **Step 5: Run focus-ring tests and verify GREEN**

Run:

```bash
cargo test -p vmux_layout focus_ring -- --nocapture
```

Expected: all focus-ring tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/focus_ring.rs
git commit -m "perf(layout): stop ticking hidden focus ring"
```

### Task 3: Add explicit Player-mode frame demand

**Files:**
- Modify: `crates/vmux_desktop/src/background_lifecycle.rs:35-55`
- Modify: `crates/vmux_desktop/src/background_lifecycle.rs:488-526`
- Test: `crates/vmux_desktop/src/background_lifecycle.rs`

- [ ] **Step 1: Write failing predicate tests**

Add to the existing test module:

```rust
#[test]
fn player_frame_demand_only_runs_for_player_or_transition() {
    assert!(!player_frame_should_wake(InteractionMode::User, false, true));
    assert!(player_frame_should_wake(InteractionMode::Player, false, true));
    assert!(player_frame_should_wake(InteractionMode::User, true, true));
    assert!(player_frame_should_wake(InteractionMode::Player, true, true));
    assert!(!player_frame_should_wake(InteractionMode::Player, false, false));
    assert!(!player_frame_should_wake(InteractionMode::User, true, false));
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p vmux_desktop player_frame_demand_only_runs_for_player_or_transition -- --nocapture
```

Expected: compile failure because `player_frame_should_wake` does not exist.

- [ ] **Step 3: Implement the wake predicate and system**

Add:

```rust
fn player_frame_should_wake(
    mode: InteractionMode,
    transition_active: bool,
    window_active: bool,
) -> bool {
    window_active && (mode == InteractionMode::Player || transition_active)
}

fn keep_awake_while_player_active(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mode: Res<InteractionMode>,
    transition: Option<Res<vmux_layout::scene::ModeTransition>>,
    windows: Query<&Window>,
) {
    let window_active = windows.iter().any(|window| window.visible && window.focused);
    if !player_frame_should_wake(*mode, transition.is_some(), window_active) {
        return;
    }
    if let Some(proxy) = proxy {
        let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
    }
}
```

Register it in `Last` so deferred transition changes are visible:

```rust
.add_systems(Last, keep_awake_while_player_active)
```

- [ ] **Step 4: Add a registration regression test**

Extract `BackgroundLifecyclePlugin::build` source and assert it contains `.add_systems(Last, keep_awake_while_player_active)`.

- [ ] **Step 5: Run targeted tests and verify GREEN**

Run:

```bash
cargo test -p vmux_desktop player_frame -- --nocapture
```

Expected: predicate and registration tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_desktop/src/background_lifecycle.rs
git commit -m "fix(player): drive reactive frames while active"
```

### Task 4: Restore native page visibility after Player mode

**Files:**
- Modify: `crates/vmux_browser/src/lib.rs:1134-1360`
- Modify: `crates/vmux_browser/src/lib.rs:4355-4380`
- Modify: `crates/vmux_browser/src/lib.rs:4787-4975`
- Modify: `crates/vmux_layout/src/scene.rs:339-417`
- Modify: `crates/vmux_layout/src/scene.rs:419-479`
- Modify: `crates/vmux_layout/src/scene.rs:571-630`

- [ ] **Step 1: Write the failing recreated-page visibility test**

Add beside `windowed_pages_hide_on_deactivate_and_first_show`:

```rust
#[test]
fn recreated_inactive_windowed_page_is_hidden() {
    let page = Entity::from_bits(1);

    assert_eq!(
        windowed_pages_to_hide(&[page], &[], &[page], &[page]),
        vec![page]
    );
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p vmux_browser recreated_inactive_windowed_page_is_hidden -- --nocapture
```

Expected: compile failure because `windowed_pages_to_hide` does not yet accept recreated windowed pages.

- [ ] **Step 3: Track pages re-entering the windowed backend**

Add `last_windowed_pages` to `sync_windowed_frames`:

```rust
mut last_windowed_pages: Local<Vec<Entity>>,
```

After collecting `visible` and `hidden`, calculate current and newly windowed pages:

```rust
let current_windowed: Vec<Entity> = visible.iter().chain(&hidden).copied().collect();
let newly_windowed: Vec<Entity> = current_windowed
    .iter()
    .copied()
    .filter(|entity| !last_windowed_pages.contains(entity))
    .collect();
```

Pass `newly_windowed` to the helper and store the current set:

```rust
for entity in windowed_pages_to_hide(
    &hidden,
    &last_visible_pages,
    &ever_shown,
    &newly_windowed,
) {
    browsers.set_windowed_hidden(&entity, true);
}
*last_visible_pages = visible;
*last_windowed_pages = current_windowed;
```

Change the helper to:

```rust
fn windowed_pages_to_hide(
    hidden: &[Entity],
    prev_visible: &[Entity],
    ever_shown: &[Entity],
    newly_windowed: &[Entity],
) -> Vec<Entity> {
    hidden
        .iter()
        .copied()
        .filter(|entity| {
            prev_visible.contains(entity)
                || !ever_shown.contains(entity)
                || newly_windowed.contains(entity)
        })
        .collect()
}
```

Pass `&[]` as the fourth argument in `windowed_pages_hide_on_deactivate_and_first_show`.

- [ ] **Step 4: Run the visibility tests and verify GREEN**

Run:

```bash
cargo test -p vmux_browser windowed_pages_hide -- --nocapture
cargo test -p vmux_browser recreated_inactive_windowed_page_is_hidden -- --nocapture
```

Expected: all visibility tests PASS.

- [ ] **Step 5: Write the failing camera animation target test**

Add:

```rust
#[test]
fn exit_transition_wires_main_camera_animation_target() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<AnimationClip>>()
        .init_resource::<Assets<AnimationGraph>>()
        .insert_resource(CameraHome(Transform::IDENTITY))
        .insert_resource(ModeTransition::new(TransitionDirection::ExitPlayer))
        .add_systems(Update, setup_exit_camera_animation);

    let camera = app
        .world_mut()
        .spawn((MainCamera, Transform::from_xyz(3.0, 2.0, 1.0)))
        .id();

    app.update();

    let target = AnimationTargetId::from_name(&Name::new("main_camera"));
    assert_eq!(app.world().get::<AnimationTargetId>(camera), Some(&target));
    assert_eq!(
        app.world().get::<AnimatedBy>(camera).map(|animated| animated.0),
        Some(camera)
    );
}
```

- [ ] **Step 6: Run the target test and verify RED**

Run:

```bash
cargo test -p vmux_layout exit_transition_wires_main_camera_animation_target -- --nocapture
```

Expected: FAIL because the camera has neither `AnimationTargetId` nor `AnimatedBy`.

- [ ] **Step 7: Wire the animation to the camera**

Change the camera insertion in `setup_exit_camera_animation` to:

```rust
commands.entity(*camera_entity).insert((
    Name::new("main_camera"),
    target_id,
    AnimatedBy(*camera_entity),
    AnimationGraphHandle(graph_handle),
    AnimationPlayer::default(),
));
```

Run the target test again. Expected: PASS.

- [ ] **Step 8: Write the failing scene cleanup assertions**

In `exiting_player_mode_resets_free_camera_state`, insert a pending animation resource before `app.update()`:

```rust
let (_, node_index) =
    AnimationGraph::from_clip(Handle::<AnimationClip>::default());
app.insert_resource(PendingAnimationStart(node_index));
```

Spawn `AnimationPlayer::default()`, `AnimationGraphHandle::default()`, `Name::new("main_camera")`, `AnimationTargetId::from_name(&Name::new("main_camera"))`, and `AnimatedBy(camera)` on the camera. Spawn one `SceneSunlight` entity. After `app.update()`, assert:

```rust
assert_eq!(
    *app.world().resource::<InteractionMode>(),
    InteractionMode::User
);
assert!(!app.world().contains_resource::<ModeTransition>());
assert!(!app.world().contains_resource::<CameraHome>());
assert!(!app.world().contains_resource::<PendingAnimationStart>());

let camera = app
    .world_mut()
    .query_filtered::<Entity, With<MainCamera>>()
    .single(app.world())
    .expect("main camera");
assert!(app.world().get::<Bloom>(camera).is_none());
assert!(app.world().get::<AnimationPlayer>(camera).is_none());
assert!(app.world().get::<AnimationGraphHandle>(camera).is_none());
assert!(app.world().get::<AnimationTargetId>(camera).is_none());
assert!(app.world().get::<AnimatedBy>(camera).is_none());
assert!(app.world().get::<Name>(camera).is_none());
assert_eq!(*app.world().get::<Transform>(camera).unwrap(), home);
assert_eq!(
    app.world_mut()
        .query_filtered::<Entity, With<SceneSunlight>>()
        .iter(app.world())
        .count(),
    0
);
```

- [ ] **Step 9: Run the scene test and verify RED**

Run:

```bash
cargo test -p vmux_layout exiting_player_mode_resets_free_camera_state -- --nocapture
```

Expected: FAIL because `PendingAnimationStart`, `AnimationTargetId`, and `AnimatedBy` remain after exit cleanup.

- [ ] **Step 10: Clear all exit-animation state**

Remove the target components with the other camera animation components:

```rust
commands
    .entity(*camera)
    .remove::<AnimationPlayer>()
    .remove::<AnimationGraphHandle>()
    .remove::<AnimationTargetId>()
    .remove::<AnimatedBy>()
    .remove::<Name>();
```

Also remove the pending resource:

```rust
commands.remove_resource::<PendingAnimationStart>();
```

Run the scene test again. Expected: PASS.

- [ ] **Step 11: Add the backend marker round-trip test**

Add `user_player_user_backend_round_trip`. Create a primary `Window`, a home-positioned `MainCamera`, layout, modal, and content browsers. Call `sync_cef_backend_for_interaction_mode` in User mode, replace `InteractionMode` with Player and call it again, then replace it with User and call it again.

Assert:

```rust
assert!(app.world().get::<WebviewWindowed>(layout).is_none());
assert_eq!(
    app.world().get::<WebviewWindowed>(modal).is_some(),
    cfg!(target_os = "macos")
);
assert_eq!(
    app.world().get::<WebviewWindowed>(page).is_some(),
    cfg!(target_os = "macos")
);
```

- [ ] **Step 12: Run restoration tests and verify GREEN**

Run:

```bash
cargo test -p vmux_layout exiting_player_mode_resets_free_camera_state -- --nocapture
cargo test -p vmux_layout exit_transition_wires_main_camera_animation_target -- --nocapture
cargo test -p vmux_browser user_player_user_backend_round_trip -- --nocapture
cargo test -p vmux_browser windowed_pages_hide -- --nocapture
```

Expected: all restoration tests PASS.

- [ ] **Step 13: Commit**

```bash
git add crates/vmux_layout/src/scene.rs crates/vmux_browser/src/lib.rs
git commit -m "fix(player): restore user mode browser visibility"
```

### Task 5: Runtime diagnosis and acceptance test

**Files:**
- Temporarily modify: `patches/bevy_cef_core-0.5.2/src/browser_process/renderer_handler.rs`
- Temporarily modify: `patches/bevy_cef-0.5.2/src/webview.rs`
- Temporarily modify: `crates/vmux_desktop/src/background_lifecycle.rs`
- Revert all temporary diagnostics before commit.

- [ ] **Step 1: Add unconditional one-second rate logs**

Use `bevy::log::info!` to log:

```text
perf osr_paints_per_sec=<n> webview=<entity>
perf texture_wake_requests_per_sec=<n> texture_wakes_delivered_per_sec=<n>
perf bevy_updates_per_sec=<n> mode=<User|Player> transition=<none|enter|exit>
perf render_frames_per_sec=<n>
```

Count `on_paint` before buffer allocation, every texture-wake request entering `spawn_texture_wake_throttler`, successful `WinitUserEvent::WakeUp` sends from that throttler, one `Last` system invocation, and one `Render` schedule invocation after `RenderSystems::Render`. Use atomics and the one-second main-app reporter so idle zeroes are emitted. Do not gate logs behind an environment variable or feature.

- [ ] **Step 2: Build the normal dev app**

Run:

```bash
make sign-mac-debug
```

Expected: signed dev app build succeeds.

- [ ] **Step 3: User runtime sequence**

Run the normal app and perform:

```text
idle User mode for 10 seconds
play an animated windowed page or video in User mode for 10 seconds
enter Player mode
move camera
focus and unfocus one pane
unfocus and refocus the app while Player mode remains active
exit by command
enter again
exit by pane double-click
idle User mode for 10 seconds
```

- [ ] **Step 4: Read logs directly**

Read:

```text
~/Library/Application Support/Vmux/dev/logs/vmux-dev.<date>.log
~/Library/Application Support/Vmux/dev/profiles/personal/chrome_debug.log
```

If this build uses the legacy profile layout, read the matching paths under `~/Library/Application Support/Vmux/logs/` and `profiles/personal/`.

Expected:

```text
User idle: OSR paints, texture wake requests, delivered wakes, Bevy updates, and render frames settle to the one-second fallback only.
Player: Bevy updates and render frames run at display cadence while the app is focused.
Return: mode=User, transition=none, layout/pages visible and aligned.
```

- [ ] **Step 5: Fix any remaining Player-return failure with TDD**

Use the first failed invariant from Task 4 and runtime logs. Add one failing test in `scene.rs` for scene state or `vmux_browser/src/lib.rs` for backend/native state, run it red, implement the smallest fix, and rerun green. Do not combine unrelated fixes.

- [ ] **Step 6: Remove every temporary diagnostic**

Run:

```bash
git diff -- patches/bevy_cef_core-0.5.2/src/browser_process/renderer_handler.rs patches/bevy_cef-0.5.2/src/webview.rs crates/vmux_desktop/src/background_lifecycle.rs
```

Remove only the temporary counters/logs while preserving the Player wake implementation.

- [ ] **Step 7: Measure idle CPU**

Run:

```bash
pgrep -afil vmux_desktop
ps -p <main-pid> -o pid,ppid,%cpu,%mem,etime,command
```

Expected: main `vmux_desktop` remains below 5% CPU after User mode settles.

### Task 6: Targeted verification and plan cleanup

**Files:**
- Delete: `docs/plans/2026-07-10-event-driven-osr-efficiency.md`

- [ ] **Step 1: Run targeted tests**

Run:

```bash
cargo test -p vmux_layout --test page_source
cargo test -p vmux_layout focus_ring
cargo test -p vmux_layout exiting_player_mode_resets_free_camera_state -- --nocapture
cargo test -p vmux_layout exit_transition_wires_main_camera_animation_target -- --nocapture
cargo test -p vmux_desktop player_frame -- --nocapture
cargo test -p vmux_browser user_player_user_backend_round_trip -- --nocapture
cargo test -p vmux_browser windowed_pages_hide -- --nocapture
```

Expected: all commands PASS with no warnings introduced by the change.

- [ ] **Step 2: Run formatting for touched Rust code**

Run:

```bash
cargo fmt --check -p vmux_layout -p vmux_browser -p vmux_desktop
```

Expected: PASS.

- [ ] **Step 3: Verify no banned update mode or temporary diagnostics remain**

Run:

```bash
rg -n "UpdateMode::Continuous|perf osr_paints_per_sec|perf texture_wake_requests_per_sec|perf texture_wakes_delivered_per_sec|perf bevy_updates_per_sec|perf render_frames_per_sec" crates patches
```

Expected: no `UpdateMode::Continuous` usage and no temporary performance log strings.

- [ ] **Step 4: Delete the completed plan**

Delete `docs/plans/2026-07-10-event-driven-osr-efficiency.md` with `apply_patch`.

- [ ] **Step 5: Commit final verification changes**

```bash
git add -A
git commit -m "docs: remove completed efficiency plan"
```

- [ ] **Step 6: Confirm clean worktree**

Run:

```bash
git status --short --branch
```

Expected: branch ahead of `origin/main`; no uncommitted files.
