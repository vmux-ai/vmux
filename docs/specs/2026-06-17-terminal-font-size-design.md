# Terminal Font Size: Global, Persisted, Live

## Problem

`cmd+`/`cmd-`/`cmd0` on a terminal (or terminal-backed page like a vibe agent) currently
adjusts a per-terminal, in-memory `TerminalFontScale` on the focused webview only. The
change is not shared across other open terminals and is lost on restart.

We want one font-size knob that:

- Applies to **all** terminals and terminal-backed pages (vibe agent included).
- Is written **directly to `settings.ron`**, so it persists across restarts and stays
  consistent with the rest of the configuration.

## Background

- `cmd+`/`cmd-`/`cmd0` reach the app via the macOS menu as
  `BrowserViewCommand::ZoomIn` / `ZoomOut` / `ZoomReset`.
- `handle_browser_commands` (`crates/vmux_browser/src/lib.rs`) reads `AppCommand::Browser`
  and, for terminals (`is_terminal`), inserts `TerminalFontScale` on the focused webview.
  For real browser pages it adjusts `ZoomLevel` (unchanged by this work).
- Terminal appearance is driven by `sync_terminal_theme`
  (`crates/vmux_terminal/src/plugin.rs`). Every terminal resolves the **single**
  `terminal.default_theme`; there is no per-terminal theme selection. The system emits a
  `TermThemeEvent` carrying `font_size` (currently `theme.font_size * scale`).
- Settings persistence already has a clean, reusable path:
  `apply_settings_update(settings, path, value)` mutates the in-memory `AppSettings`
  resource **and** returns minimal `ron_bytes`; emitting `SettingsWriteRequest { ron_bytes }`
  writes `settings.ron`. Used today by `on_settings_command` (`plugin/view.rs`) and the
  agent plugin.

Because all terminals resolve the same default theme, editing that theme's `font_size` is
exactly "change it across all terminals".

## Behavior

- **Step:** `cmd+` → `+1.0`, `cmd-` → `-1.0`, clamped to `[6.0, 40.0]`.
- **Reset:** `cmd0` → `14.0` (the embedded default `font_size`).
- **Scope:** edits `terminal.themes[<default>].font_size` in `settings.ron`; applies live to
  every open terminal and vibe agent page, and to future ones on launch.
- Browser (non-terminal) zoom is unchanged.

## Design

### 1. New typed message (`vmux_terminal`)

```rust
#[derive(Message, Clone, Copy, Debug)]
pub enum TerminalFontSizeCommand {
    Increase,
    Decrease,
    Reset,
}
```

Registered via `add_message::<TerminalFontSizeCommand>()` in the terminal plugin. This
replaces the per-terminal scale path and keeps cross-module behavior message-driven
(per AGENTS.md).

### 2. `vmux_browser` emits the message instead of mutating state

In `handle_browser_commands`, the `is_terminal` arms of `ZoomIn`/`ZoomOut`/`ZoomReset` write
`TerminalFontSizeCommand::{Increase,Decrease,Reset}` via a `MessageWriter`. Remove the
`term_scale_q` param and the three `TerminalFontScale` inserts. The non-terminal
`ZoomLevel` arms are untouched.

### 3. Handler system (`vmux_terminal`) — owns the persist

```rust
fn handle_terminal_font_size(
    mut reader: MessageReader<TerminalFontSizeCommand>,
    mut settings: ResMut<AppSettings>,
    mut writes: MessageWriter<SettingsWriteRequest>,
) { ... }
```

For each command:

1. Read `settings.terminal`; find the index of the theme whose `name == default_theme`.
2. Compute the new size from the **current** `themes[idx].font_size`:
   - `Increase` → `(cur + 1.0).min(40.0)`
   - `Decrease` → `(cur - 1.0).max(6.0)`
   - `Reset`    → `14.0`
3. `apply_settings_update(settings.as_mut(), "terminal.themes[{idx}].font_size", json!(new))`
   → mutates the in-memory resource (triggers change detection) and returns `ron_bytes`.
4. Emit `SettingsWriteRequest { ron_bytes }` to persist.

Compute `idx` and `new` while holding the immutable borrow, then call `apply_settings_update`
(which needs `&mut settings`).

**Edge case:** if `default_theme` is not present in `themes` (the synthesized-fallback case
in `resolve_theme`), log a warning and skip — there is no concrete entry to persist. The
embedded `settings.ron` always contains the default theme, so the normal path is `idx == 0`.

### 4. Propagate to all running terminals (`sync_terminal_theme`)

Today the system's change-detection hash covers terminal **colors** only, so a `font_size`
edit would not re-emit. Fold the typographic fields into the signature so any theme change
re-emits `TermThemeEvent` to **every** terminal:

- Extract a pure `theme_signature(theme: &TerminalTheme, colors: &TermColors) -> u64` that
  folds the color bytes **and** `font_size`, `line_height`, `padding`, `font_family`,
  `cursor_style`, `cursor_blink`.
- `theme_changed = signature != *last_hash` drives "emit to all"; new/newly-ready terminals
  still emit on add.
- Remove the `* scale` multiply (emit `theme.font_size` directly) and the
  `changed_scale` / `scale_q` parameters.

### 5. Remove `TerminalFontScale`

Delete the component, its `Default` impl, and all references (only `sync_terminal_theme` and
the three `vmux_browser` insert sites use it).

## Data Flow

```
cmd+/-/0 (macOS menu)
  -> BrowserViewCommand::ZoomIn/ZoomOut/ZoomReset
  -> handle_browser_commands (is_terminal)
  -> TerminalFontSizeCommand::{Increase,Decrease,Reset}
  -> handle_terminal_font_size
       -> apply_settings_update("terminal.themes[idx].font_size", new)  [mutates AppSettings]
       -> SettingsWriteRequest { ron_bytes }                            [writes settings.ron]
  -> AppSettings changed
  -> sync_terminal_theme (signature changed) -> TermThemeEvent to ALL terminals
```

The on-disk write fires the settings file watcher, but `LastSelfWriteHash` already guards
against reloading our own write (existing mechanism, no change needed).

## Testing (TDD)

Unit tests in `vmux_terminal`:

- `handle_terminal_font_size`: build an `App` with `AppSettings` (default theme `font_size = 14`),
  send `Increase` → `themes[0].font_size == 15.0` **and** exactly one `SettingsWriteRequest`
  emitted.
- Clamp: at `40.0`, `Increase` stays `40.0`; at `6.0`, `Decrease` stays `6.0`.
- `Reset`: from `20.0` → `14.0`.
- `theme_signature`: changing only `font_size` flips the returned hash (proves all-terminal
  re-emit triggers).

On-screen re-render (the actual `TermThemeEvent` reaching webviews) depends on host-emit
readiness, which is not unit-testable; verify at runtime.

## Out of Scope

- Per-terminal / per-tab font overrides.
- Browser (web page) zoom behavior.
- A settings-UI control for font size (the RON edit + existing settings view already cover it).
