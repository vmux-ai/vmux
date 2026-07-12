# Windowed focus recovery

Date: 2026-07-12
Status: Design — pending review

## Problem

When the last active page is closed and vmux automatically creates `vmux://start/`, the launcher is visible but does not accept typing or page-local keyboard shortcuts until clicked. Opening the same page with Cmd+T works immediately.

The defect is generic to a windowed CEF page replacing another windowed CEF page. The start page is the reliable reproduction because closing the final page immediately spawns it.

## Root cause

Windowed pages require their native CEF `NSView` to own the macOS first responder. `apply_windowed_host_focus` currently calls `set_windowed_focus(true)` once when the active webview changes or first becomes available.

Commit `3e9b03f9` changed this from a per-frame assertion to a one-shot assertion to preserve native text selection. During replacement, vmux can focus the new page before asynchronous teardown of the old CEF view finishes. The old view then resigns or clears native focus. Because the active webview entity has not changed, the one-shot cache suppresses further focus calls. Clicking the new page restores first responder manually.

Cmd+T does not close the previous page, so the teardown race does not occur.

## Approaches

### Chosen: verify native focus and recover only when missing

Expose a macOS-only focus query from the patched CEF browser registry. Each frame, compare the active windowed webview with the window's actual first responder. Call `set_windowed_focus(true)` only when the active webview does not own native focus.

This restores focus after delayed teardown while avoiding the repeated focus calls that previously disturbed text selection.

### Rejected: bounded focus retries

Retry focus for a fixed number of frames after an active-page transition. This is simpler but timing-dependent and can still disturb selection during the retry window.

### Rejected: explicit refocus after close or reveal

Send a focus request from page-close and start-page reveal paths. This directly covers the reported reproduction but couples focus correctness to individual lifecycle paths and misses other native focus-loss races.

## Design

### Native focus inspection

Add a `Browsers` query that reports whether a windowed browser owns native focus.

On macOS, obtain the browser's native `NSView`, read its `NSWindow.firstResponder`, and treat the browser as focused when the responder is the browser view or a descendant of it. Missing browsers, non-windowed browsers, null handles, and detached views report no native focus without panicking.

The AppKit implementation is `#[cfg(target_os = "macos")]`. Other platforms retain transition-based behavior.

### Focus application

Replace the entity-only local cache in `apply_windowed_host_focus` with a decision that considers:

- current `HostFocusIntent`;
- whether the target browser exists;
- whether native focus inspection is available;
- whether the target already owns native focus.

For a macOS `Windowed(webview)` intent, focus when the browser exists and native focus is absent. Do nothing while the browser already owns focus. If the browser is still being created, retry naturally on later frames.

For `WinitHost` and `Unmanaged`, preserve current behavior. Linux keeps the existing one-shot entity cache because native windowed focus inspection is macOS-specific.

### `vmux://start/` flow

After the final page closes:

1. Layout creates the replacement stack and opens `vmux://start/`.
2. Start-page handling attaches a warm spare or creates a cold browser.
3. Host focus resolves to the new start-page webview.
4. If teardown of the old page clears first responder, the next focus pass observes the mismatch and focuses the start page again.
5. Existing start-page JavaScript focuses `#command-bar-input`, so typing and page shortcuts work without a click.

Cmd+T follows the same focus verification and remains immediately usable.

## Testing

Add unit coverage around the focus decision:

- available target without native focus requests focus;
- target already holding native focus does not request focus;
- delayed browser creation requests focus once available;
- changing targets requests focus for the new target;
- unmanaged and winit-host intents do not request windowed focus.

Add patched CEF coverage for macOS focus inspection structure and platform gating where direct AppKit state is not practical in a unit test.

Run targeted tests for `vmux_browser` and the patched `bevy_cef_core` crate. Manual verification on macOS closes all pages until `vmux://start/` auto-spawns, then types and uses launcher shortcuts without clicking. Also verify Cmd+T and native text selection.

## Risks

- CEF may place first responder on a descendant rather than its root view. The query must recognize the full native view subtree.
- Reactive scheduling must receive a wake after asynchronous teardown. CEF lifecycle and browser creation already wake the event loop; manual verification confirms recovery timing.
- A false negative would reintroduce repeated focus calls and selection loss. Descendant detection and the already-focused test guard this behavior.
