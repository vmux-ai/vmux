# VMX-89: Ask to Terminate Terminal Process

## Summary

Show a native macOS confirmation dialog when the user attempts to close a tab, pane, or quit the app while terminal processes are still running. Configurable via `confirm_close` setting (default: `true`).

## Requirements

- **Tab close** (Cmd+W, header tab X button): if the tab's terminal has a live shell, show a confirmation dialog before closing.
- **Pane close** (`<leader> x`): same behavior as tab close.
- **App quit** (Cmd+Q, window close button): if any terminal across all tabs has a live shell, show a single confirmation dialog listing the count of running terminals.
- **Setting**: `terminal.confirm_close: bool` (default `true`). When `false`, all close/quit operations behave as they do today (immediate, no prompt).
- **PtyExited**: if the shell has already exited (`PtyExited` component present), skip confirmation and close immediately regardless of setting.
- **SIGINT** (Ctrl+C): unchanged -- raw `_exit(0)`, no dialog.

## Approach

Use the `rfd` crate for native macOS `NSAlert` dialogs. Async dialogs via `rfd::AsyncMessageDialog` to avoid blocking the Bevy main thread.

## Components

### New File: `crates/vmux_desktop/src/confirm_close.rs`

Contains all confirmation logic as a Bevy plugin.

#### Types

| Type | Kind | Purpose |
|------|------|---------|
| `ConfirmClose(Entity)` | Event | Confirmed: proceed with tab/pane despawn |
| `ConfirmQuit` | Event | Confirmed: proceed with app exit |
| `PendingCloseConfirmation` | Component | Marks entity awaiting dialog response (prevents double-dialog) |
| `PendingQuitConfirmation` | Resource | Marks app-level quit dialog in progress |

#### Systems

| System | Trigger | Behavior |
|--------|---------|----------|
| `intercept_tab_close` | `TabCommand::Close` / JS `"close_tab"` | Check `confirm_close` + `PtyExited`. If confirmation needed, insert `PendingCloseConfirmation`, spawn async dialog task. Otherwise despawn immediately. |
| `intercept_pane_close` | `PaneCommand::Close` | Same logic as tab close. |
| `intercept_quit` | Custom quit menu item event | Count live terminals. If 0 or `confirm_close == false`, send `AppExit`. Otherwise show dialog with count. |
| `handle_window_close` | `WindowCloseRequested` observer | Deny close, delegate to `intercept_quit` flow. |
| `process_confirmed_close` | `ConfirmClose` event | Despawn the entity (existing tab close logic). |
| `process_confirmed_quit` | `ConfirmQuit` event | Send `AppExit`. |

### Modified Files

| File | Change |
|------|--------|
| `crates/vmux_desktop/Cargo.toml` | Add `rfd = "0.15"` |
| `crates/vmux_desktop/src/settings.rs` | Add `confirm_close: bool` to `TerminalSettings` |
| `crates/vmux_desktop/src/settings.ron` | Add `confirm_close: true` default |
| `crates/vmux_desktop/src/layout/tab.rs` | `TabCommand::Close` delegates to confirmation flow instead of immediate despawn |
| `crates/vmux_desktop/src/layout/pane.rs` | `PaneCommand::Close` delegates to confirmation flow |
| `crates/vmux_desktop/src/browser.rs` | `"close_tab"` message delegates to confirmation flow |
| `crates/vmux_macro/src/lib.rs` | Replace `PredefinedMenuItem::quit(None)` with custom `MenuItem` using same Cmd+Q accelerator |
| `crates/vmux_desktop/src/main.rs` | Register `ConfirmClosePlugin`, add `WindowCloseRequested` observer |

## Dialog Details

### Tab/Pane Close Dialog

- **Style**: `MessageLevel::Warning`
- **Title**: "Close Terminal?"
- **Message**: "A process is still running in this terminal. Close anyway?"
- **Buttons**: "Close" (confirm), "Cancel" (deny)

### App Quit Dialog

- **Style**: `MessageLevel::Warning`
- **Title**: "Quit Vmux?"
- **Message**: "N terminal(s) are still running. Quit anyway?"
- **Buttons**: "Quit" (confirm), "Cancel" (deny)

## Async Dialog Flow

```
user action → check confirm_close setting
  → false: proceed immediately
  → true: check PtyExited / count live terminals
    → none alive: proceed immediately
    → alive: insert PendingCloseConfirmation / PendingQuitConfirmation
      → spawn IoTaskPool future:
          let confirmed = rfd::AsyncMessageDialog::new()
              .set_level(MessageLevel::Warning)
              .set_title(...)
              .set_description(...)
              .set_buttons(OkCancel)
              .show()
              .await;
          if confirmed { send ConfirmClose/ConfirmQuit event }
          remove PendingCloseConfirmation/PendingQuitConfirmation
```

`PendingCloseConfirmation` on the entity and `PendingQuitConfirmation` as a resource prevent duplicate dialogs if the user triggers close/quit again while a dialog is open.

## Edge Cases

- **Double Cmd+W**: `PendingCloseConfirmation` component check prevents second dialog.
- **Double Cmd+Q**: `PendingQuitConfirmation` resource check prevents second dialog.
- **Tab close during quit dialog**: quit dialog takes precedence; individual tab close is no-op while quit pending.
- **Shell exits while dialog is open**: dialog result still applies -- if user confirms, entity is despawned (may already be despawned by `PtyExited` auto-close, which is fine since `ConfirmClose` handler checks entity existence).
- **`confirm_close` changed at runtime**: hot-reload via existing `reload_settings_on_change` system. New value applies immediately to subsequent close attempts.
