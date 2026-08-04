# Command Bar as a Layout Panel

## Context

The command bar is a separate CEF browser. Because it is a separate surface, none of what a modal
normally gets for free is free: its content arrives over IPC behind a ready/rendered/painted
handshake with a retry loop and a timeout, its size is a round-trip between host and page, its
keyboard focus is negotiated across three systems, its clickable area is a rectangle published to a
static and hit-tested on the AppKit thread, and its stacking is split between `zPosition` for
painting and subview order for input.

An attempt to convert it to a native windowed `NSView` failed for a reason worth recording: a
windowed CEF child view inside the winit-owned window never receives DOM input here. Both delivery
paths were tested with the view frontmost, hit-tested, correctly sized, not `WasHidden`,
Chromium-focused and holding first responder — `send_key_event` forwarding produces no DOM key
events because it is a windowless API, and real `NSEvent`s produce no `keydown` in the renderer
either. The surfaces that work today (editor, terminal) do so because they consume keys in Rust,
never as DOM input.

Separately, the existing command bar does not open at all on `main` as of `e4599d2c`: the
`OpenCommandBar` command fires and nothing follows it. This design replaces that path rather than
repairing it.

The requirement has also changed: the bar should float, and the user should be able to drag it
anywhere and resize it.

## Decision

The command bar becomes a component inside the existing layout page (`vmux://layout/`), alongside
the header and side sheet. It stops being its own browser.

The decisive precedent is the bookmark URL field: a DOM text input that already lives in the layout
page and already accepts typing. `BookmarkTextInputActive` makes `sync_keyboard_target` hand
`CefKeyboardTarget` to the layout shell, and keys flow winit → Bevy → `send_key_event` → the focused
DOM element. A command bar in the same document needs no new input mechanism.

Dragging and resizing — the hard part in every other option — become pointer events on a
`position: fixed` panel. Position and size are four numbers in settings. There is no host geometry,
no measurement round-trip, and no coordinate-space conversion.

## What this deletes

Not refactored — removed:

- the open handshake: `COMMAND_BAR_OPEN_EVENT` delivery to a separate browser, `open_id`,
  `CommandBarReadyEvent`, `CommandBarRenderedEvent`, `PendingCommandBarReveal`, the reveal frame
  counters, `COMMAND_BAR_OPEN_RETRY_INTERVAL`, `COMMAND_BAR_NATIVE_REVEAL_TIMEOUT`, and
  `command_bar_should_recreate_renderer`
- the size round-trip: `CommandBarSizeEvent`, `CommandBarNativeSize`, `install_command_bar_size_observer`,
  `command_bar_results_extra_height`, and `command_bar/size.rs`
- the focus negotiation for the modal: its `CefKeyboardTarget` handling, the `HostFocusIntent::Windowed`
  arm for a modal, and the palette's `focus_command_bar_input_retry` loop
- the native surface plumbing: `sync_windowed_command_bar`, `command_bar_windowed_frame`, the
  `NATIVE_COMMAND_BAR_*` statics and published click rectangle, `flush_native_command_bar_pointer_events`,
  `dismiss_windowed_command_bar_on_outside_click`, `dismiss_command_bar_from_native_monitor`,
  and `sync_command_bar_overlay` plus `CommandBarOverlay` in `glass.rs`
- the `Modal` webview spawn in `window.rs` and every backend branch that special-cases it

Opening becomes setting a signal. The lifecycle is a `div`.

## Keeping the door open for an OS-level window

The panel must stay re-hostable in a standalone `NSPanel` later without a rewrite. Three rules:

`CommandPalette` stays host-agnostic. It already is — the start page renders the same component
through `PaletteVariant`, and `build_command_bar_open_payload` and `gather_command_bar_tabs` are
already shared. Nothing in the panel may reach into layout-page internals.

Data continues to arrive as one typed payload rather than being read from layout state, so the same
payload can be pushed to a different browser later.

Commit and dismiss stay events (`CommandBarActionEvent`), never direct calls into layout code.

Re-hosting then means pointing the existing component at a different surface, and re-adding only the
geometry the OS window needs.

## Integration points

**Pointer regions.** `cef_pointer_hit_rect` currently marks a layout node interactive only when it
is the header or the side sheet:

```rust
let interactive = (header.is_some() || side_sheet.is_some()) && open && ...
```

The panel needs a third marker component so its rect joins the injected regions. Without this,
clicks over the panel are not forwarded into the layout OSR browser.

**Keyboard ownership.** Generalise the bookmark marker into one "the layout page owns the keyboard"
condition covering the bookmark field and the panel, so `sync_keyboard_target` has a single rule
rather than a second special case.

**Layering.** The layout overlay composites at `zPosition` 100, above the windowed pages — this is
why the sidebar and header already paint over page content. The panel inherits that for free and
needs no `zPosition` of its own.

**Dismiss.** Outside-click becomes a DOM listener in the same document, which is genuinely simpler
than the windowed case, where clicks outside the bar never reached the page at all.

**Persistence.** Panel position and size live in `AppSettings` under the editor/layout settings,
clamped to the window on restore so a panel saved on a larger display stays reachable.

## Testing

The open/dismiss decision, the clamp-on-restore logic, and the drag/resize geometry are pure
functions and get table-driven unit tests.

One integration test covers the regression that motivated this: typing immediately after the bar
opens reaches the query state, with no reveal window in which keystrokes can be lost.

The pointer-region contract gets a test asserting the panel's rect appears in the injected regions
when open and not when closed — the failure mode there is silent, and it is the one piece of host
plumbing the panel depends on.

Styling stays untested per repository policy.

## Sequencing

Build the panel behind the existing `OpenCommandBar` command and delete the old surface in the same
change. Leaving both alive would mean two command bars racing for the same keyboard, which is the
class of bug this design exists to remove.
