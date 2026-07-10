# Event-Driven OSR Efficiency Design

## Goal

Make normal User mode idle like Chrome while preserving responsive OSR rendering and a fully working Player mode round trip.

Success criteria:

- Static User mode produces no continuous layout OSR paints, texture wakes, or Bevy updates beyond the configured reactive fallback tick.
- The main `vmux_desktop` process stays below 5% CPU at idle on a focused 120 Hz display.
- Windowed CEF pages do not increase the Bevy update rate.
- OSR paint bursts are coalesced to at most one Bevy wake per display interval.
- Player mode renders continuously while active without using `UpdateMode::Continuous`.
- User → Player → User restores camera, layout, CEF backends, focus, and native view visibility.

## Findings

macOS CEF message pumping is already decoupled from Bevy. A native `CFRunLoop` timer calls `cef::do_message_loop_work()` without sending Winit events.

The expensive path is:

```text
layout CEF OnPaint
  → RenderTextureMessage
  → texture wake throttler
  → WinitUserEvent::WakeUp
  → Bevy App::update()
  → full extract/render/present
```

The layout shell is the only windowless CEF browser in User mode. Persistent layout CSS animation therefore causes a full Bevy frame for every OSR paint.

Current settled-state paint sources include the two `animate-pulse` done indicators. The focus-ring material also advances time on every focused Bevy update even when the ring is hidden in normal macOS User mode.

Player mode currently has no explicit frame-demand wake. Its transitions and free camera can therefore depend accidentally on unrelated OSR paints or input events. Removing persistent layout animation without replacing that accidental clock would make Player mode stall.

Switching between User and Player also changes content pages between windowed CEF and OSR CEF. That requires browser recreation. The existing User → Player → User bug may be in scene cleanup, camera restoration, backend recreation, or native-view reconciliation. It must be isolated before changing that path.

## Scope

- Remove continuous animation from settled layout-shell UI.
- Stop hidden focus-ring asset mutation.
- Add explicit Player-mode frame demand through `WinitUserEvent::WakeUp`.
- Diagnose and fix the existing Player → User restoration bug.
- Add regression tests for idle animation, Player frame demand, and mode round trips.
- Temporarily instrument paint, wake, update, render, and mode-transition rates.

## Non-goals

- `UpdateMode::Continuous`.
- Replacing Bevy's Winit runner.
- Moving the layout shell to windowed CEF.
- Redesigning CEF Alloy/Chrome runtime selection.
- Changing windowed-page behavior outside mode transitions.

## Design

### User mode

User mode remains reactive and event-driven.

- Done indicators remain visible as static amber dots.
- Infinite CSS animation is forbidden for persistent layout-shell state.
- Loading animation remains allowed only while a real transient loading state is active and must stop when that state clears.
- One-shot animation such as reload spin remains allowed.
- The focus-ring gradient clock updates only when the mesh is visible and needs animation. A hidden ring must not mutate its material asset.
- CEF texture delivery continues to wake Bevy only after an OSR paint is queued. Existing display-rate coalescing remains unchanged.

When the DOM and ECS are settled, no source should request another frame.

### Player mode frame demand

Player mode gets an explicit self-sustaining wake source in `BackgroundLifecyclePlugin`.

The wake predicate is true while either condition holds:

- `InteractionMode::Player`
- `ModeTransition` exists

The system sends `WinitUserEvent::WakeUp` once per Bevy update. Winit redraw/presentation pacing limits the resulting loop. The wake stops immediately after the exit transition completes and mode returns to User.

This preserves free-camera movement, bloom/light fades, camera return animation, CEF OSR updates, and backend reconciliation without relying on a CSS animation. It complies with the project ban on `UpdateMode::Continuous`.

### Player return correctness

The mode round trip is treated as one restoration transaction with explicit invariants.

After Player → User completes:

- `InteractionMode` is `User`.
- `ModeTransition`, `CameraHome`, `PendingAnimationStart`, `AnimationPlayer`, and `AnimationGraphHandle` are absent.
- `FreeCameraState` is disabled and its velocity, pitch, yaw, multiplier, and rotation curve are reset.
- Main-camera transform equals the current framed home transform.
- `VmuxWindow` transform equals the current window frame transform.
- Layout mesh remains visible with blend alpha for the User-mode OSR shell.
- On macOS, non-layout CEF surfaces reconcile back to windowed mode.
- Native views are sized, shown, raised, and focused according to current ECS state.
- No stale Player-mode keyboard target or suppression state remains.
- Player frame-demand wakes stop.

First add a failing round-trip test and temporary transition diagnostics. Determine whether the broken invariant belongs to scene cleanup or CEF backend reconciliation. Fix only the proven failing layer.

### Diagnostics

Temporary diagnostics are unconditional and use Bevy tracing macros.

Once per second, report:

- layout OSR paints
- texture wake requests and delivered wakes
- Bevy updates
- render frames
- current interaction mode and transition direction

At Player transition boundaries, report camera/window transforms, CEF backend style, browser existence, page-ready state, and native-view visibility.

All temporary diagnostics are removed before commit.

## Testing

### Automated

- Layout page source contains no persistent `animate-pulse`, `animate-ping`, `animate-bounce`, or unbounded spin classes.
- Hidden User-mode focus ring does not mutate its material time.
- Visible Player-mode focus ring still animates.
- Player-frame wake predicate is false for settled User mode and true for Player mode or either transition.
- User → Player → User transition restores every scene invariant.
- CEF backend round trip restores windowed markers on macOS and keeps the layout shell OSR.
- Existing no-`UpdateMode::Continuous` invariant remains green.

### Runtime

Use a normal dev build and read Vmux logs directly.

1. Idle in User mode with no loading state. Confirm paint, wake, update, and render rates settle to zero between the one-second reactive fallback ticks.
2. Enter Player mode. Confirm smooth camera movement and transition animation at display cadence.
3. Focus and unfocus a pane in Player mode.
4. Exit Player mode by command and double-click. Confirm layout, pages, focus, and glass restore correctly.
5. Leave windowed pages active, including an animated page or video. Confirm they do not raise Bevy update rate.
6. Confirm main-process idle CPU stays below 5% on the focused 120 Hz display.

## Fallback

If User mode still exceeds the target with zero layout paints, investigate non-CEF Bevy wake sources before changing architecture.

If required layout animation still makes OSR too expensive, evaluate a separate hybrid/native layout-shell design. That is not part of this change.
