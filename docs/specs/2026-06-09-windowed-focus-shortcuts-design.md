# Windowed focus + shortcut prioritization

Date: 2026-06-09
Status: Design — pending review

## Problem

In User (browse) mode on macOS, content pages render as native windowed CEF child views (PR #56). Two focus bugs:

1. **Shortcuts die when a page is focused.** Once a vibe/web page or terminal is focused (e.g. after clicking it), *no* shortcuts fire — not ⌘K, not ⌘W, not ctrl+hjkl, not leader chords.
2. **New pages aren't typeable until clicked.** Opening a vibe/terminal (new tab, new stack) leaves it un-typeable; the user must click it to give it focus.

Desired end state: **a newly opened page is focused automatically (typeable without a click), and shortcuts always take priority — even while a page is focused.**

## Root cause

CEF windowed views are created as **child `NSView`s of winit's window** (`browsers.rs` `set_as_child`). On macOS, CEF couples two things for a windowed browser:

- **Renderer/logical focus** — needed for a web page to accept typed input and show a caret.
- **`NSView` first-responder** — set by `host.set_focus(true)`.

There is no way (in windowed mode) to give a web page renderer focus *without* making its `NSView` the macOS first-responder.

Empirically (confirmed against the running app): **while a CEF page holds first-responder, the host process receives no key events at all** — neither winit's `ButtonInput<KeyCode>` (so `process_key_input` dies) nor the app-level `addLocalMonitorForEventsMatchingMask` in `native_keyboard.rs` (so even ⌘K's `Consume` never happens). CEF-focus == total host-keyboard blackout.

PR #56 tried to avoid this with a `FocusCanceler` (`client_handler.rs` `on_set_focus` returns `1` for *every* focus request) so winit always keeps first-responder and keys forward via `CefKeyboardTarget`. The realized behavior falls short:

- The blunt cancel also kills renderer focus, so forwarded keys land nowhere → page not typeable without a click (bug 2).
- A click still ends with the CEF `NSView` in first-responder → host-keyboard blackout → shortcuts die (bug 1).

## Constraints

- **Terminals cannot be native-focused.** Terminal keystrokes reach the shell only via the Bevy path: `handle_terminal_keyboard` reads `KeyboardInput` → `ServiceClient` → PTY. CEF forwarding is suppressed for terminals (`sync_keyboard_target` sets `suppress.0 = terminal_q.contains(...)`). xterm.js in the terminal page has no PTY pipe of its own. So a terminal needs **winit** to be first-responder.
- **Web pages need native first-responder** to type in windowed mode (the coupling above).
- ⇒ Web pages and terminals require *opposite* first-responder states.
- The app-level local monitor cannot rescue shortcuts while a CEF page is focused (the blackout). Interception must sit **below the app**.

## Chosen approach

Keep windowed rendering (preserve the CPU win). Add a session-level event tap as the universal shortcut layer, and manage first-responder per page type.

### 1. Universal shortcut layer — `CGEventTap`

- Install a session-level `CGEventTap` (`kCGSessionEventTap`, `kCGHeadInsertEventTap`, `kCGEventTapOptionDefault`) for `keyDown` + `flagsChanged`, macOS-only, in `vmux_desktop`.
- The callback runs the **same `decide()` logic** as `native_keyboard.rs`: `Consume` modifier shortcuts and chord leaders/sequences (return `NULL` to drop, queue the `AppCommand` into the existing `PENDING_COMMANDS`); `PassThrough` everything else (return the event).
- A session tap fires **before** the event reaches the app, so it works even while a CEF page is first-responder (unlike the local monitor).
- **Frontmost gate (safety):** the callback must only `Consume`/queue when vmux is the active application (`NSApp.isActive` / frontmost check). When vmux is not frontmost, always pass through — never hijack other apps' keys.
- **Tap re-enable:** handle `kCGEventTapDisabledByTimeout` / `…ByUserInput` by calling `CGEventTapEnable(true)` from the callback.
- **Run loop:** add the tap source to the main `CFRunLoop`; the callback queues commands drained by the existing `process_monitored_keys` system.

### 2. Local monitor stays as the no-permission fallback

- Keep `addLocalMonitorForEventsMatchingMask`. No de-duplication needed: the session tap consumes shortcut keydowns *before* they reach the app, so the app-level monitor only ever sees pass-through keys (which produce no command). The two cannot double-fire.
- When the tap is not granted (see §4), the local monitor still handles shortcuts whenever winit is first-responder (terminal / layout focus). Only "shortcuts while a web page is focused" is lost until the grant.

### 3. Per-page-type first-responder (reuse `allow_native_focus`)

The `WebviewWindowedNativeFocus` marker → `allow_native_focus` → `cancel_native_focus = !allow_native_focus` → whether `FocusCanceler` is attached. Today only the command bar sets the marker; content pages get the canceler.

- **Web / vibe / browse pages** → add `WebviewWindowedNativeFocus` (so `allow_native_focus = true`, **no** `FocusCanceler`) → they can hold first-responder and type natively, like an ordinary browser.
- **Terminal pages** → no marker (keep `FocusCanceler`) → their CEF view never steals first-responder → winit keeps it → the Bevy key path keeps working.
- Decision is by page kind at attach time (terminal URL ⇒ withhold marker; otherwise grant it).

### 4. Auto-focus on open / active-page change

A User-mode focus-sync system sets the correct first-responder for the active page whenever the active page changes (covers new tab / new stack / switch):

- Active page is **web/vibe** → `set_windowed_focus(active, true)` → CEF `NSView` becomes first-responder → typeable immediately.
- Active page is **terminal** → reclaim winit first-responder via AppKit (`[[ns_view window] makeFirstResponder: ns_view]`, objc2) so `KeyboardInput` flows → Bevy path delivers; ensure `CefKeyboardTarget` is on the terminal browser for routing.

This removes the need for a manual click.

### 5. Permission UX — prompt at launch

- The tap needs **Input Monitoring**. Create it at startup; macOS shows the Input Monitoring prompt on first run. Input Monitoring grants typically require an app restart to take effect.
- Graceful degradation if not (yet) granted: typing + auto-focus still work; shortcuts fire whenever winit/terminal is focused (local-monitor fallback); only "shortcuts while a web page is focused" is unavailable until granted + restart.

## Keyboard data-flow (User mode, tap granted, vmux frontmost)

| Focused page | Shortcut key (⌘K, ctrl+h, leader…) | Normal key (typing) |
|---|---|---|
| Web / vibe | tap `Consume` → `AppCommand` (page never sees it) | tap pass-through → CEF `NSView` (first-responder) types natively |
| Terminal | tap `Consume` → `AppCommand` | tap pass-through → winit (first-responder) → `KeyboardInput` → `handle_terminal_keyboard` → service → PTY |
| Layout / none | tap `Consume` → `AppCommand` | tap pass-through → winit |

`CefKeyboardTarget` forwarding (`keyboard.rs send_key_event`) remains for Player/OSR mode; it is inert for native-focused windowed web pages (winit gets no `KeyboardInput` to forward) and stays suppressed for terminals.

## Components & files

New (vmux_desktop):
- `crates/vmux_desktop/src/event_tap.rs` — `CGEventTap` install, callback (frontmost gate + `decide()` + re-enable), run-loop wiring; reuses `ShortcutMap` / `PENDING_COMMANDS` / `decide()` from `native_keyboard`.
- Register in `ShortcutPlugin` (`shortcut.rs`) on macOS (Startup install, `process_monitored_keys` already drains the queue).

Changed:
- `crates/vmux_desktop/src/native_keyboard.rs` — expose `decide`/`PENDING_COMMANDS`/translate helpers for reuse; keep the local monitor as fallback.
- `crates/vmux_browser/src/lib.rs` — add `WebviewWindowedNativeFocus` to web/agent (non-terminal) content browsers at attach; new User-mode focus-sync system (web → `set_windowed_focus(true)`; terminal → reclaim winit first-responder) keyed on `FocusedStack` changes.
- First-responder reclaim helper (AppKit `makeFirstResponder`, objc2) — in vmux_desktop or patched `bevy_cef_core` where the window handle is available.

Unchanged: the `FocusCanceler` mechanism itself (now applied only to terminals + non-native modals among content); `set_windowed_focus` gate; OSR focus path.

## Testing

- `decide()` parity test: the tap path and local-monitor path classify identically (extend `native_keyboard` tests; share the fn).
- Frontmost-gate unit: callback passes through (no queue) when not frontmost.
- Per-type native-focus selection: web/agent browsers receive `WebviewWindowedNativeFocus`; terminals do not (component-level test on attach).
- Focus-sync system: on active-page change, web → `set_windowed_focus(true)` issued; terminal → winit-reclaim issued (assert via messages/calls, per Bevy message-integration rule).
- Manual (macOS, requires grant + restart): open new vibe/terminal → typeable without click; ⌘K / ⌘W / ctrl+hjkl / leader chord fire while a web page is focused and while a terminal is focused; switching web↔terminal hands off first-responder correctly; other apps' shortcuts unaffected when vmux is backgrounded.

## Risks / open questions

- **Input Monitoring friction.** A keystroke tap is a sensitive permission; the prompt is alarming and needs a restart. Mitigated by graceful fallback, but it is the main cost of staying windowed.
- **CGEvent → KeyCombo mapping.** The tap sees `CGEventFlags` + keycode; `decide()` was built for `NSEventModifierFlags`. Need a small, tested converter (or normalize both to the shared `Modifiers`).
- **First-responder reclaim timing.** Reclaiming winit first-responder for terminals must not fight CEF's own focus handling on the same frame; may need to run after CEF view attach/raise.
- **Web page blur semantics.** With web pages now genuinely first-responder, `document.hasFocus()` is true (good); confirm clipboard ⌘C/⌘V still behave (the tap consumes them as shortcuts only if bound — they are not vmux shortcuts, so they pass through to the page natively).
- **Non-macOS.** Tap + native-focus changes are macOS-gated; Linux/OSR path unchanged.
