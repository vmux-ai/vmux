# OSR-free desktop

Delete offscreen rendering. Every surface becomes a native window or view that Chromium paints and
hit-tests itself; the host stops injecting input; the page and the ECS exchange nothing but events.

> **Recovered 2026-08-14.** Written 2026-08-08 on `feat/osr-free`, which was never merged and is no
> longer on the remote. It is landed unedited for one section — *Correction: the windowed path was
> never broken* — which retracts a claim that is load-bearing for
> [`2026-08-04-command-bar-panel-design.md`](2026-08-04-command-bar-panel-design.md). The retraction
> was lost with the branch while the claim it retracts stayed in the tree and went on being cited.
>
> Read the rest as a dated record, not as current fact. Paths predate the move of crates into
> `app/`, `host/` and `page/` and the `vmux_server` → `vmux_page` rename. The prototype described
> under *Command bar on the child window: where it stands* exists only on that branch. Two of its
> premises have since become fact rather than plan: the 3D scene is gone (#366) and
> `InteractionMode` is collapsed (#369).

## Why this is possible now

Two decisions removed the only reasons OSR existed.

**Player mode is shelved.** Panes as 3D meshes in a Bevy scene cannot be native views, so the app
needed a rendering path that produced textures. Nothing else did.

**Liquid glass is dropped, opaque backgrounds accepted.** A windowed CEF view cannot be transparent,
so the translucent layout shell had to be composited as a `CALayer` above the panes. That single
constraint is what made the shell full-window, which made it swallow hit-testing, which is why the
host injects pointer and keyboard input at all.

Everything else in the input-routing mess descends from those two.

## Correction: the windowed path was never broken

`docs/specs/2026-08-04-command-bar-panel-design.md` states that a windowed CEF child view never
receives DOM input in this app, and `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs`
repeats it at the decision point. **That conclusion is wrong**, and it is load-bearing for the design
it justified, so it has to be retracted before anything is built on it.

The disproof is direct: opening `vmux://command-bar/` as an ordinary page in a pane produces a
windowed CEF view running the same page, and it accepts keyboard input normally. Browser creation
parameters are identical to a working editor pane —

```
command-bar  windowed=true  bg=None              allow_native_focus=true  fps=120
editor pane  windowed=true  bg=Some(4280361252)  allow_native_focus=true  fps=120
```

— so CEF, the `vmux://` scheme, and the page are all innocent. What failed was the `Modal`-specific
plumbing: `sync_windowed_command_bar` instead of the shared `sync_windowed_frames`, a `Modal` arm in
`compute_host_focus_intent` instead of the `FocusedStack` path, plus `set_windowed_corner_radius`,
`nudge_windowed_repaint` and `publish_native_command_bar_route`, none of which panes touch.

The earlier retest that appeared to confirm the failure exercised that same bespoke path, so it
reproduced the bug rather than testing the hypothesis.

## The rule: Bevy does not consume keyboard or mouse input

The host observes global shortcuts and nothing else. Every other keystroke and pointer event goes
where AppKit sends it — to whichever native view is under the cursor or holds first responder — and
Chromium handles it. The page then emits a command. The host never routes input to a surface.

Input injection only ever existed because an offscreen surface cannot receive native events. Once
every surface is native, the entire apparatus is dead weight, and worse than dead weight: holding
`CefKeyboardTarget` on a *windowed* browser makes bevy_cef forward the key through `send_key_event`,
a windowless API that produces no DOM events. The host swallows the keystroke into a call that does
nothing, and the surface looks focused while being deaf. That is the bug that consumed 2026-08-08.

What this deletes, beyond the OSR pipeline itself:

- `CefKeyboardTarget` and the keyboard arbitration in `sync_keyboard_target`
- `CefSuppressKeyboardInput` and every `suppress.0` assignment
- `CefPointerTarget`, the published pointer regions, and the move/click/wheel injection
- `HostFocusIntent` and `apply_windowed_host_focus`, once no surface needs focus handed to it

What the host keeps:

- The global shortcut monitor in `native_keyboard.rs`. `Cmd+K` has to fire whichever page holds
  focus, including a third-party site, so it must sit above the pages. It should *observe and
  consume specific chords only* — never route general input.
- Terminal input, which goes to a PTY through the service rather than to a DOM.

The test for any future input code: if the host is deciding *which surface* should receive a
keystroke, it is wrong. AppKit already knows.

## Target topology

**Bevy holds state and orchestrates. It does not render.** No wgpu frame per tick, no texture upload,
no compositing. It owns tabs, stacks, panes, spaces, agents, terminals, settings and persistence, and
it positions native surfaces.

**Chrome and panes are opaque sibling `NSView`s in disjoint rectangles.** Panes already are. The
header becomes a strip at the top and the side sheet a strip at the left. Because the rectangles do
not overlap, AppKit hit-testing resolves them with no help from us.

**Floating surfaces are child `NSWindow`s** — the command bar, context menus, dropdowns, tooltips. A
child window may be transparent, carries a real shadow, and can extend past the parent window's
bounds. That is what a menu needs and what an opaque sibling view cannot provide, and it is how
Chromium renders its own menus. It also means "windowed CEF cannot be transparent" stops mattering:
the surfaces that need transparency stop being views.

Dragging such a window should use AppKit's `performWindowDragWithEvent:`, which hands the drag to the
OS — no per-frame message, no host round trip.

## The contract already exists

`crates/vmux_ui/src/hooks/transport.rs:24` defines the whole boundary:

```rust
pub trait PageHost {
    fn emit(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError>;
    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError>;
}
```

`vmux_wire` holds the payload types, dual-derived for serde and rkyv, and `vmux_command` re-exports
them so Bevy-side code is unchanged. This work should add to that contract, never around it.

Two honest caveats about how finished it is. The mobile implementation (`vmux_mobile/src/page_host.rs`)
returns `Unsupported` from `emit` and handles exactly one listen id, so "the same UI on either
transport" is demonstrated for one read-only event, not established. And the id convention is
asymmetric — page→host emits under `std::any::type_name::<T>()` while host→page uses short constants.
Worth fixing while the surface area is small.

`vmux_wire` is also churning: 61 commits in seven days on `feat/mobile-remote`, eight of them touching
the crate. The churn is directional — types are being moved in to make them host-portable — but this
plan should assume the crate moves under it.

## What the contract still lacks

`vmux_wire` is roughly 95% domain data. Absent: focus ownership, keyboard ownership, pointer regions,
panel placement. Terminal input ids live in `vmux_core`, the Bevy-linked crate, rather than the
portable one.

The happy consequence of going native is that most of those never need to exist. Pointer regions and
keyboard ownership are artefacts of injection; delete injection and they go with it. What genuinely
remains is small: surface geometry (host→page on open, page→host when the user moves or resizes a
floating window) and a dismiss signal. `CommandBarSizeEvent` is already the shape of the first.

## What gets deleted

- `WebviewNativeOverlay`, `WebviewNativeDirectOverlay`, and the presenters in `vmux_desktop/src/glass.rs`
- Pointer-region publishing and all three injection paths — move, click, wheel — plus
  `NativeMouseMovePresenter::{send_wheel, send_click}`
- `CommandBarRoute`, `sync_windowed_command_bar`, `NATIVE_COMMAND_BAR_*`
- `LAYOUT_ACTIVE_FRAME_RATE`, `LAYOUT_IDLE_FRAME_RATE`, `sync_layout_cef_frame_rate`,
  `request_layout_frame_burst`, and the scroll-wake special case in `background_lifecycle.rs`
- The OSR texture pipeline: dirty rects, `RenderTextureMessage`, `texture_upload.rs`, and the
  mesh/sprite webview renderers
- `InteractionMode` and `sync_cef_backend_for_interaction_mode`, once Player is gone
- The coordinate-space conversions that the 2026-08-02 input-routing spec proposed to newtype

## Verified: CEF in a child `NSWindow` works

Prototyped 2026-08-08 and confirmed. A `Titled | FullSizeContentView` window with the titlebar hidden
— **not** `Borderless`, whose `canBecomeKeyWindow` is false, so `makeKeyWindow` silently does nothing
— hosting a reparented CEF view. It paints, carries a real shadow, becomes key, takes first
responder, and accepts keyboard input.

The result came from completing this matrix, which is what finally separated the two variables:

| surface | sibling view | child window |
| --- | --- | --- |
| ordinary pane | types | **types** |
| `Modal` command bar | dead | dead |

A pane keeps working when moved into a child window, so the child window is not the problem. The
command bar page keeps working when opened as an ordinary pane, so the page is not the problem
either. What fails is the `Modal` path specifically: `sync_windowed_command_bar` instead of the
shared `sync_windowed_frames`, and a `Modal` arm in `compute_host_focus_intent` instead of the
`FocusedStack` route.

The practical consequence is that the exact fault in that plumbing does not need finding. The command
bar should stop being a `Modal` and become an ordinary windowed surface in a child window, which is
the same shape every popover needs.

Three gotchas worth carrying forward:

- Borderless windows cannot become key. Use a titled window with a hidden titlebar.
- `standardWindowButton` needs both the `NSButton` and `NSControl` features on `objc2-app-kit`;
  without them the traffic lights show through a chromeless window.
- After reparenting, `view.window()` is the *child*. Using it as the frame of reference re-offsets
  the window every call and walks it off screen — take the parent from `child.parentWindow()`.

`Browsers::host_in_child_window` is kept as the working reference for the real implementation.

## Migration order

Each step ships on its own.

1. **Prototype the child window** with the command bar. Answers the only open question.
2. **Command bar off OSR.** Deletes `sync_windowed_command_bar`, `CommandBarRoute`, and the modal.
3. **Header and side sheet to sibling views.** Deletes the layout overlay, the pointer regions, and
   all three injection paths. The largest single reduction, and where the 20% goes.
4. **Popovers to child windows.** Bookmark context menu first, since it already has host-side state.
5. **Drop Player.** Removes `InteractionMode` and the backend switch.
6. **Stop rendering.** Bevy keeps the window and stops drawing into it; gaps become window background.

Steps 1–3 are worth doing even if the rest stalls: they remove the injection layer, which is where
the bugs live.

## Open questions

- Where does the window's visual treatment live once nothing paints the gaps — window background
  colour, per-view corner radius, or a background view?
- Does the traffic-light inset survive a header that is its own view rather than a full-window overlay?
- Does dropping Player let us drop the Bevy renderer entirely, or is a window still required to hold
  the view hierarchy?


## Command bar on the child window: where it stands

Started 2026-08-08 on `perf/layout-render`. It renders in a floating child window and does not yet
accept keyboard input. Everything below is committed so the next attempt starts from the state
rather than from scratch.

Done:

- `CommandBarSurface` replaces the `Modal` spawn — an ordinary windowed browser with the same
  components a pane gets (`WebviewWindowed`, `WebviewWindowedNativeFocus`,
  `WebviewOpaqueWindowedBackground`), excluded from `sync_windowed_frames`.
- `sync_command_bar_window` lifts it into a child window when open, orders it out when closed, and
  keeps the window between opens so reopening is a reposition.
- `handle_open_command_bar` toggles it and emits `COMMAND_BAR_OPEN_EVENT` to it directly.
- `compute_host_focus_intent` and `sync_keyboard_target` both special-case it, because neither knew
  the surface existed and both undid the focus call every frame.
- Tests updated to the surface contract, including that it is windowed and explicitly not a `Modal`.

Not working: **typing**. The palette renders but no keystroke reaches it.

Also still wrong: the web view does not fill its window — the page paints into roughly the
bottom-right two thirds with the opaque background showing around it. A pane in the same child
window fills it correctly, so this is specific to how the surface is sized, and is probably the same
root cause as the input failure rather than a separate bug.

### Abandon this approach — the surface should be a page, not a new component

`CommandBarSurface`, `sync_command_bar_window`, and the special cases added to
`compute_host_focus_intent` and `sync_keyboard_target` should be **deleted rather than debugged**.
They repeat the mistake they were meant to remove: a surface that is special, so every system needs
a branch for it. `Modal` was that, and the replacement grew two branches in an afternoon by
discovering them through failed builds.

**Spawn the command bar as an ordinary page in an ordinary pane.** No new component. It then
inherits, with no special casing anywhere:

- `FocusedStack` membership
- `sync_keyboard_target`'s active-stack branch, which grants `CefKeyboardTarget`
- `compute_host_focus_intent`'s normal `host_focus_intent(active, is_terminal)` route
- `apply_windowed_host_focus`, which calls `set_windowed_focus` on it

That exact combination is what typed correctly in the pane experiments. Both of them —
`vmux://command-bar/` opened as a page, and the agent page moved into a child window — accepted
keyboard input with no host-side routing at all.

The only custom part is placement: host that pane's view in a child window via
`Browsers::host_in_child_window`, called **once on open**, not per frame. Everything else about the
pane stays untouched.

The page owns its own input in Dioxus — typing, arrow keys, Enter, Escape, filtering — and emits
`CommandBarActionEvent` to the host. The host routes no keys. That is the point of the whole
direction, and it is what every other `vmux://` page already does.

Open questions for that design, none of them blocking:

- Which stack does the command bar pane belong to, and does it need to be excluded from the tab
  strip and from `sync_windowed_frames`' layout positioning while still counting as focusable?
- Dismissal: Escape and outside-click both currently route through `deferred_dismiss_modal`, which
  targets a `Modal` that no longer exists.
- Whether the pane should be spawned once at startup and reused, or created per open. Reuse avoids
  the browser teardown that the old reveal handshake existed to hide.

### What not to repeat

Native AppKit and CEF calls are not idempotent and must not be driven as per-frame state sync.
`raise_windowed_to_front` broke painting and first responder that way; a `setFrame` compared with
`!=` on floats carrying noise re-set the window every frame; and `sync_command_bar_window` called
`host_in_child_window`, `resize`, `set_windowed_focus` and `makeKeyWindow` sixty times a second,
which leaves the surface visible and focused and reporting `hasFocus=true` while accepting no input.
Guard every one of them on an actual change.

Diagnose differentially, not by iteration. Every real answer today came from comparing two
configurations that differed in one variable — a pane against the modal, a sibling view against a
child window. The rounds spent changing one thing and rebuilding produced nothing. If the page-in-a-
pane approach also fails to type, dump both browsers' state side by side at the same instant — CEF
view frame, content view bounds, `WebviewSize`, resize arguments, key window, first responder,
`CefKeyboardTarget` — and read the difference out of the table.

Finally: the DOM `keydown` probe used throughout this work was unreliable. It reported zero events in
a configuration the user confirmed was typing correctly, so its absence proves nothing. Trust the
observed behaviour over the probe.


## Rebuild the command bar: delete the surface, keep the palette

Attempted incrementally on 2026-08-08 and abandoned. Every change fought OSR-era scaffolding rather
than the problem, and the branch ended with a broken command bar and a damaged layout. Start the
surface again rather than patch it.

The code splits in two, and the split is what makes this tractable.

**Keep.** The start page imports these directly, and they work — the palette types correctly today
when the page is opened in an ordinary pane:

- `command_bar/palette.rs` (~2100 lines) — `CommandPalette`, `PaletteVariant`, `StartAgentTransition`
- `command_bar/results.rs` (~1200 lines)
- from `command_bar/handler.rs`: `build_command_bar_open_payload`, `gather_command_bar_tabs`,
  `on_command_bar_action`, the path-completion observers, the `WriteCommandBarSnapshots` resources
- `command_bar/work_snapshot.rs`, `command_bar/shortcut.rs`
- `vmux_wire::command_bar` payload types

**Delete.** Everything that positions, reveals, focuses or routes input to a surface — roughly 177
references in `vmux_browser/src/lib.rs` alone:

- the `Modal` spawn in `window.rs` and every `With<Modal>` query
- `command_bar/state.rs`, `command_bar/size.rs`, `command_bar/panel.rs`, `command_bar/page.rs`
- the reveal handshake in `handler.rs`: `PendingCommandBarReveal`, `CommandBarReady`,
  `CommandBarRenderedOpen`, `reveal_command_bar`, `retry_pending_command_bar_open`,
  `prewarm_command_bar_modal`
- `sync_windowed_command_bar`, `CommandBarRoute`, `NATIVE_COMMAND_BAR_*`,
  `dismiss_windowed_command_bar_on_outside_click`, the synthetic pointer queue
- `sync_command_bar_overlay` and `CommandBarOverlay` in `vmux_desktop/src/glass.rs`
- the layout-page panel added in #342, and its `LAYOUT_COMMAND_BAR_*` events

**Then build the new surface**, small enough to hold in your head:

1. `vmux://command-bar/` is an ordinary page spawned once at startup, hidden.
2. `Cmd+K` shows it and hosts its view in a child window via `Browsers::host_in_child_window` —
   called once on open, never per frame.
3. **The host must not take `CefKeyboardTarget` on it.** That is what breaks input: bevy_cef then
   forwards keys through `send_key_event`, a windowless API that produces no DOM events for a
   windowed browser, so the host swallows the keystroke into a call that does nothing. Set
   `CefSuppressKeyboardInput` while the bar is open and let AppKit deliver natively to the child
   window, exactly as it does for a pane the user clicks into.
4. The page owns typing, arrows, Enter and Escape in Dioxus and emits `CommandBarActionEvent`. The
   host routes no input.
5. Dismiss: Escape and outside click are page concerns. Drag uses `performWindowDragWithEvent:`.

The success criterion for step 3 is already demonstrated: `vmux://command-bar/` opened as a pane
types correctly, and an agent page moved into a child window types correctly. The new surface has to
reproduce those conditions, not invent new ones.
