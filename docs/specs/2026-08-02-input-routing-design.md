# Input Routing

## Summary

A keystroke or a click entering vmux can end up in one of eighteen places. Which one it reaches is decided by roughly a dozen systems spread across five crates, each answering some version of "who has focus?" or "what is under the cursor?" from its own query, in its own coordinate space, against state that may have been written one frame earlier by a different system on a different thread.

There is no single place that knows where input is supposed to go. This document defines one.

The immediate motivation is the command bar, which has resisted repair for days. The diagnosis below shows why: six independent predicates answer "is the command bar open?", and they disagree with each other during precisely the frames when the command bar is opening.

## Diagnosis

### Six predicates, one question

`reveal_command_bar` holds the modal in `display: Flex` with `Visibility::Hidden` and `CefKeyboardTarget` attached for between two and ten frames while the CEF page renders its first frame. During that window:

| Predicate | Reads `Visibility` | Verdict |
| --- | --- | --- |
| `sync_keyboard_target` inline test (`vmux_browser/src/lib.rs:1051`) | no | open |
| `command_bar_windowed_view_is_open` (`vmux_browser/src/lib.rs:2687`) | no | open |
| `command_bar_modal_is_open` (`vmux_layout/src/command_bar/handler.rs:368`) | no | open |
| `command_bar_windowed_view_should_show` (`vmux_browser/src/lib.rs:2679`) | yes | closed |
| `command_bar_modal_is_visible` (`vmux_layout/src/command_bar/handler.rs:372`) | yes | closed |
| `compute_host_focus_intent` inline test (`vmux_browser/src/host_focus.rs:73`) | yes | closed |

The consequences compound. `sync_keyboard_target` believes the bar is open, so it strips `CefKeyboardTarget` from every other browser and returns early. `compute_host_focus_intent` does not see a command bar, falls through to the focused-stack branch, and hands AppKit first-responder to the active page's `NSView`. `sync_windowed_command_bar` takes its `render_hidden` branch and parks the native view offscreen with `set_windowed_focus(false)`. `NATIVE_COMMAND_BAR_OPEN` — fed by the no-visibility predicate — reads true, so the `NSEvent` local monitor hand-delivers each `keyDown` to `window.firstResponder()`, which is the page, and then swallows the event so winit never sees it.

Every keystroke typed while the command bar is revealing is delivered to the page the user was looking at, and no Bevy system ever learns that it happened.

The same split affects pointer input. `NATIVE_COMMAND_BAR_OPEN` is written from the no-visibility predicate while `NATIVE_COMMAND_BAR_CLICK_FRAME` is written from the with-visibility one, so the monitor can hold a stored frame while `open` reads false. A click inside the bar's rectangle then requests a dismiss, because `request_native_command_bar_dismiss_for_mouse_down` checks neither `open`, nor which button, nor press versus release.

### The general shape

The command bar is the acute case, not a special case. The same pattern recurs:

Three systems independently resolve keyboard focus. `sync_keyboard_target` assigns `CefKeyboardTarget`, `compute_host_focus_intent` assigns AppKit first-responder, and `resolve_terminal_input_targets` picks PTY recipients. They filter `Header` and `SideSheet` differently, treat Player mode differently, and read the bookmark markers off different components. `resolve_terminal_input_targets` can select a terminal that carries no `CefKeyboardTarget` at all.

The chord grammar is implemented three times: `native_keyboard::decide`, `shortcut::process_key_input`, and `resolve_terminal_web_shortcut`. Each keeps its own pending-prefix store; two of them gate direct bindings on ctrl/alt/super and one does not. When the `CGEventTap` is installed and passes a key through, the `NSEvent` monitor classifies the same physical key a second time and mutates the same `PENDING_PREFIX` static again.

The pane rectangle test is written out four times, in `pane.rs` three times and in `background_lifecycle.rs` once, with no shared helper.

Cursor position is obtained five ways, from three different sources of scale factor — `backingScaleFactor`, `Window::resolution::scale_factor()`, and `1.0 / ComputedNode::inverse_scale_factor` — with the flipped-view convention handled in some paths and not others.

`CefSuppressKeyboardInput` is written by six systems across three schedules; the `PostUpdate` writer always wins, and the value is read in the following frame's `PreUpdate`. `CefSuppressPointerInput` is read in thirteen places and written in none.

### Why it is unfixable in place

The state that routing decisions depend on crosses a thread boundary as roughly fifteen unrelated atomics and mutexes. The AppKit event thread samples them at arbitrary points relative to the Bevy frame that wrote them. Any two of those values can be observed in a combination that no frame ever actually produced.

Within Bevy, `sync_keyboard_target` writes `CefKeyboardTarget` through `Commands`, so `compute_host_focus_intent` — later in the same `PostUpdate` chain — reads pre-change values. The two systems that most need to agree are structurally guaranteed to disagree on the frame where it matters.

Fixing any individual symptom means finding all six predicates, or all three resolvers, or all four rectangle tests, and changing them consistently. That has been attempted and has not held.

## Design

### One resolver, one value, many appliers

A single system computes an `InputRoute` from ECS state each frame. Nothing else decides anything.

```
InputRoute {
    generation: u64,
    keyboard: KeyboardSink,
    responder: ResponderOwner,
    surfaces: [PointerSurface],   // z-ordered, topmost first
}

KeyboardSink   = CommandBar(Entity) | BookmarkInput(Entity)
               | Webview(Entity) | Terminal(Entity) | Bevy | None

ResponderOwner = WinitHost | NativeView(Entity) | Unmanaged

PointerSurface { id: SurfaceId, rect: PhysRect, z: i32, policy: HitPolicy }
```

`CefKeyboardTarget` stops being an input. It becomes an output, written by one applier system that reconciles the world against `route.keyboard`. The six command-bar predicates collapse to `matches!(route.keyboard, KeyboardSink::CommandBar(_))`. `HostFocusIntent` becomes `route.responder`. `resolve_terminal_input_targets` becomes a match on `KeyboardSink::Terminal`.

Resolve-then-apply also removes the `Commands` staleness hazard: the resolver reads, the appliers write, and no applier reads another applier's output.

### A z-ordered surface stack

The resolver builds one ordered list of hit-testable surfaces per frame — command bar at z 200, layout chrome at z 100, windowed pages at z 0, the 3D scene at the bottom — and exposes a single `hit_test(point) -> SurfaceId`.

That one call replaces the monitor's swallow decision, both command-bar dismiss predicates, `refresh_layout_cef_hover`, `refresh_active_windowed_hover`, and the four pane rectangle loops. One rectangle list, one containment function, one coordinate space, one z-order.

Surfaces carry an explicit `HitPolicy` rather than relying on the accident of whether something is an `NSView` (in the hit-test chain) or a `CALayer` (not in it). The layout overlay's synthetic pointer-injection machinery exists only because that distinction is currently implicit.

### One snapshot across the thread boundary

The resolver publishes an immutable, generation-stamped `RouteSnapshot`. The `NSEvent` monitor and the `CGEventTap` read that and nothing else. One consistent read per event replaces the current practice of sampling eight statics that were written at unrelated points in the frame.

Inconsistent pairs — `open` true with no frame, a frame with `open` false — stop being reachable, because there is only one value.

### One coordinate space

Pointer samples normalize to window-physical pixels, top-left origin, at ingress. Conversion to each sink's space happens once, at egress. Newtypes (`PhysPx`, `LogicalPx`, `CefDip`, `SurfaceLocal`) make a missing or doubled conversion a compile error rather than an off-by-a-scale-factor bug that only appears on a non-Retina display or during a DPI change.

One scale-factor source, obtained from the window, replaces the current three.

### One shortcut engine

A single chord engine owns the pending-prefix state and the keymap layers. It is invoked once per physical key and returns a disposition: consume with an optional `AppCommand`, route to a sink, or pass through. The `CGEventTap` and the `NSEvent` monitor become two ingress paths into the same engine rather than two engines racing on one static.

## Migration

Each step lands on its own and leaves the tree working.

1. **One command-bar predicate.** Delete the other five. Derive `NATIVE_COMMAND_BAR_OPEN` and `NATIVE_COMMAND_BAR_CLICK_FRAME` from it. This alone is expected to fix the current command-bar failure.
2. **One route snapshot.** Publish a single generation-stamped value; the AppKit monitor reads only that.
3. **One focus resolver.** Merge the three resolvers into `InputRoute`; `CefKeyboardTarget` becomes write-only output.
4. **One surface stack.** Collapse the hit tests and the hover refreshers onto it.
5. **Coordinate newtypes.** Normalize at ingress, convert at egress.
6. **One shortcut engine.** Collapse the three chord implementations.

## Testing

The routing decision is pure: ECS state in, `InputRoute` out. That makes the interesting cases table-driven unit tests rather than integration tests against CEF.

The reveal window is the regression that matters most. A test asserts that a modal in `display: Flex` with `Visibility::Hidden` and `CefKeyboardTarget` produces exactly one verdict, and that the keyboard sink, the responder owner, and the published snapshot all agree with it.

Hit testing gets a table of overlapping surfaces asserting topmost-wins by z, and a case where the command bar overlaps a windowed page.

Coordinate conversion gets round-trip tests at scale factors 1 and 2.

No test asserts class strings, and no test reads source with `include_str!` beyond the existing packaging invariants.
