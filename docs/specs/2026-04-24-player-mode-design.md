# Player Mode Design

Replace the current free camera toggle with a two-mode interaction model: **User Mode** and **Player Mode**. Player Mode lets the user navigate the 3D scene with WASD/mouse, click panes to type into them, and double-click to smoothly return to User Mode.

## Modes

### User Mode (default)

Standard operation. Hover-to-activate works. Keyboard input routes to the active pane's browser/terminal. Camera is locked to the computed framing position.

### Player Mode

Entered via `TogglePlayerMode` command (`Ctrl+G, Enter`). Camera becomes user-controllable (WASD + mouse look). Bloom and sunlight effects fade in. No pane is focused by default.

Player Mode has two sub-states, derived from whether any entity has `CefKeyboardTarget`:

| Sub-state | CefKeyboardTarget | FreeCameraState.enabled | CefSuppressKeyboardInput | Hover-to-activate |
|-----------|-------------------|-------------------------|--------------------------|-------------------|
| Roaming   | None              | true                    | true                     | Disabled          |
| Focused   | One pane's browser | false                  | false                    | Disabled          |

## State Machine

```
                TogglePlayerMode              TogglePlayerMode
                ┌──────────┐                  ┌──────────┐
                │          v                  │          v
          ┌──────────┐  animate  ┌────────────────┐  animate  ┌──────────┐
          │User Mode │ --------> │ Player/Roaming │ --------> │User Mode │
          └──────────┘           └───────┬────────┘           └──────────┘
                                    click│  ^ click empty
                                    pane │  │ space
                                         v  │
                                 ┌───────────────┐
                                 │Player/Focused │--- double-click ---> exit
                                 └───────────────┘
```

### Transitions

| From | Trigger | To | Animation |
|------|---------|-----|-----------|
| User | `TogglePlayerMode` | Player/Roaming | ~300ms bloom/sunlight fade in |
| Player/Roaming | Click pane | Player/Focused | None (camera stays) |
| Player/Focused | Click empty space | Player/Roaming | None |
| Player/Focused | Double-click pane | User | ~300ms camera return + bloom fade out |
| Player/Roaming | `TogglePlayerMode` | User | ~300ms camera return + bloom fade out |
| Player/Roaming | Double-click pane | User | ~300ms camera return + bloom fade out |

## State Representation

```rust
#[derive(Resource, Default, PartialEq, Eq)]
enum InteractionMode {
    #[default]
    User,
    Player,
}
```

Replaces `FreeCameraActive(bool)`. Roaming vs Focused is derived from `CefKeyboardTarget` presence (no additional enum variant needed).

## Camera Animation

Uses Bevy's `AnimationPlayer` + `EasingCurve`.

### Home Position

```rust
#[derive(Resource)]
struct CameraHome(Transform);
```

Captured at startup from `frame_main_camera_transform`. Updated on window resize while in User Mode.

### Entering Player Mode

1. Store current camera transform in `CameraHome`.
2. Spawn sunlight with illuminance 0. Add Bloom with intensity 0.
3. Build `AnimationClip` with `EasingCurve` for bloom intensity (0 -> natural) and sunlight illuminance (0 -> 8000) over 300ms. Easing: `CubicInOut`.
4. Camera stays at current position (no movement on entry).
5. `FreeCameraState.enabled = true` after animation completes.

### Exiting Player Mode

1. `FreeCameraState.enabled = false` immediately (no WASD during transition).
2. Build runtime `AnimationClip`:
   - `Transform::translation`: current -> `CameraHome.0.translation`, `CubicInOut`
   - `Transform::rotation`: current -> `CameraHome.0.rotation`, `CubicInOut`
3. Bloom intensity and sunlight illuminance fade to 0 in parallel.
4. Speed = `1.0 / 0.3` to achieve 300ms wall-clock time.
5. On `is_finished()`: remove animation components, despawn sunlight, remove Bloom, set exact home transform, change `InteractionMode` to `User`.

### Easing

`EaseFunction::CubicInOut` for all curves. Smooth acceleration/deceleration, no overshoot.

## Input Routing

### Player/Roaming

- `CefSuppressKeyboardInput = true`
- `FreeCameraState.enabled = true`
- No `CefKeyboardTarget` on any entity
- `poll_cursor_pane_focus` (hover-to-activate) returns early
- `sync_keyboard_target` returns early

### Player/Focused

- Single-click on a pane: insert `LastActivatedAt` on pane, `CefKeyboardTarget` on its browser
- `CefSuppressKeyboardInput = false`
- `FreeCameraState.enabled = false`
- Camera stays at current position
- Hover-to-activate still disabled

### Returning to Roaming

- Click empty space (no pane hit): remove all `CefKeyboardTarget`
- `suppress_free_camera_when_pane_active` detects empty targets -> re-enables camera control

### Double-Click Detection

- Track last click time and entity in a `Local<Option<(Entity, Instant)>>`
- Second click on same pane within 400ms: trigger exit to User Mode with animation
- Double-click on empty space: no effect

### Drag vs Click

- Track `AccumulatedMouseMotion` between press and release
- If total motion > 2.0: treat as drag, skip pane activation
- Prevents camera look-drag from accidentally activating panes

## Command Rename

| Before | After |
|--------|-------|
| `SceneCommand::ToggleFreeCamera` | `SceneCommand::TogglePlayerMode` |
| menu id `toggle_free_camera` | `toggle_player_mode` |
| label `Toggle Camera Mode` | `Toggle Player Mode` |
| `FreeCameraActive` resource | `InteractionMode` resource |
| `on_toggle_free_camera` system | `on_toggle_player_mode` |
| `click_pane_in_free_camera` system | `click_pane_in_player_mode` |
| `suppress_free_camera_when_pane_active` | retained, updated to use `InteractionMode` |

Keybinding unchanged: `Ctrl+G, Enter`.

## Files Changed

| File | Changes |
|------|---------|
| `crates/vmux_desktop/src/scene.rs` | `InteractionMode` resource, `CameraHome` resource, animation setup/teardown, transition systems, rename |
| `crates/vmux_desktop/src/command.rs` | Rename `ToggleFreeCamera` -> `TogglePlayerMode` |
| `crates/vmux_desktop/src/layout/pane.rs` | Rename function, double-click detection, use `InteractionMode` |
| `crates/vmux_desktop/src/browser.rs` | `FreeCameraActive` -> `InteractionMode` checks |
| `docs/specs/2025-07-13-commands-and-keybindings.md` | Update command name |
