# vmux:// URL Parity Design

## Overview

Make `vmux://*` scheme URLs first-class citizens alongside normal HTTP URLs. Terminal tabs should participate in all the same systems as browser tabs: command bar tab list, metadata sync, history, zoom, keyboard focus restore. Each terminal gets a unique session URL. Reload restarts the PTY. Back/Forward disabled on terminals.

## Scope

**In scope:**
- Terminal entities get `Browser` marker component
- Session URL: `vmux://terminal/?session={pty_pid}`
- Remove `ContentFilter` (redundant after Browser marker added)
- Gate browser-only systems with `Without<Terminal>`
- Palette navigate routes `vmux://` URLs to appropriate commands
- Reload on terminal = PTY restart
- Back/Forward disabled on terminal tabs (NavigationState reports false)
- Zoom works on terminal tabs via CEF zoom

**Out of scope:**
- Other `vmux://` apps beyond terminal (header, side-sheet, etc.)
- Custom vmux:// URL routing framework

## Gap Analysis (current state)

| Feature | Browser tab | Terminal tab | Root cause |
|---------|-------------|--------------|------------|
| Palette tab list | works | invisible | `With<Browser>` query excludes Terminal |
| Title propagation | works | broken | `With<Browser>` query excludes Terminal |
| Keyboard restore after palette | works | broken | `With<Browser>` query excludes Terminal |
| History visits | works | duplicate entries | No session ID, all terminals share `vmux://terminal/` |
| Zoom | works | silent fail | `With<Browser>` query excludes Terminal |
| Back/Forward/Reload | works | no-op fires | No Terminal-specific handling |
| Persistence | works | works (fragile) | URL string matching, no Terminal marker persisted |

## Fix: Terminal gets Browser marker

Terminal entities spawn with both `Browser` and `Terminal` components. This makes all existing `With<Browser>` queries automatically include terminals. No query changes needed for:
- `sync_page_metadata_to_tab` (command bar tab list, metadata sync)
- `handle_open_command_bar` / command bar tab listing
- Keyboard focus restore after palette close
- Zoom commands
- Any future `With<Browser>` queries

### ContentFilter removal

`ContentFilter` (`Or<(With<Browser>, With<Terminal>)>`) becomes redundant since `With<Browser>` now covers both. Remove the type alias and replace any remaining usage with `With<Browser>`.

### Browser-only system gating

Systems that should NOT run on Terminal entities get `Without<Terminal>` added to their query:
- CEF navigation state sync (URL change tracking from CEF) -- terminals manage their own URL
- `apply_chrome_state_from_cef` for URL updates -- terminal URL is set internally, not by CEF navigation

## Session URLs

Each terminal gets a unique URL derived from its PTY process ID:

```
vmux://terminal/?session={pty_pid}
```

Where `pty_pid` is the child process ID from `portable_pty::Child`. This is set on the terminal's `PageMetadata.url` at spawn time.

Benefits:
- History entries are distinct per terminal session
- Persistence can restore specific sessions (future work)
- URL bar shows meaningful information

On PTY restart (Reload), the session URL updates to the new PID.

## Navigation Behavior

### Palette navigate

When user types a URL in the palette:
- If URL starts with `vmux://terminal`, dispatch `TabCommand::NewTerminal`
- Other `vmux://` URLs: pass through to normal navigation (CEF scheme handlers handle them)
- Normal URLs: existing behavior (navigate active browser or create new tab)

### Back/Forward

Disabled on terminal tabs. The terminal's `NavigationState` component is set with `can_go_back: false, can_go_forward: false`. The header UI reads this to grey out / disable the arrow buttons.

### Reload

On terminal tabs, Reload triggers PTY restart:
1. Kill the existing PTY child process
2. Drop the old `PtyHandle` (writer + reader)
3. Spawn a new PTY with the same shell configuration
4. Reset `TerminalState` (create fresh `Term` instance)
5. Update `PageMetadata.url` with new session PID
6. Mark terminal as dirty to trigger viewport re-render

Implementation: In the system handling `AppCommand::Navigation(Reload)`, check if the entity has `Terminal` component. If yes, call a `restart_pty()` function instead of dispatching CEF reload. `restart_pty()` reads shell configuration from `Res<AppSettings>` (terminal settings / profile) to spawn the new PTY with the correct shell.

### Zoom

Works automatically. Terminal entities have `Browser` marker, so the existing `ZoomLevel` query (`With<Browser>`) includes them. CEF zoom level applies to the terminal webview, scaling the rendered HTML text.

## Persistence

No changes needed to persistence logic. The existing approach (check `meta.url` for `vmux://terminal/` prefix, spawn `Terminal::new()`) continues to work. The session query parameter is ignored during restore since the PTY is a new process.

The persistence check should be updated to use `starts_with("vmux://terminal")` instead of exact match to handle session URLs.

## Files Changed

| File | Change |
|------|--------|
| `crates/vmux_desktop/src/terminal.rs` | Add `Browser` marker to Terminal spawn bundle (same entity that gets `Terminal` component), set session URL on `PageMetadata.url` in the spawn system, add `restart_pty()` function |
| `crates/vmux_desktop/src/browser.rs` | Remove `ContentFilter` type alias, replace usage with `With<Browser>`. Add `Without<Terminal>` to CEF navigation sync systems. Handle Reload for Terminal. |
| `crates/vmux_desktop/src/command_bar.rs` | Update navigate to use `starts_with("vmux://terminal")`. Replace `content_browsers` query filter. |
| `crates/vmux_desktop/src/persistence.rs` | Update URL check to `starts_with("vmux://terminal")` |

## Testing

- Open terminal tab, verify it appears in command bar tab list
- Open terminal tab, verify title updates propagate (e.g. `cd /tmp` changes title)
- Open command bar over terminal tab, close palette, verify keyboard input goes to terminal
- Open multiple terminals, verify history has distinct entries with session IDs
- Press Reload on terminal tab, verify PTY restarts (fresh prompt)
- Verify Back/Forward arrows are greyed out on terminal tabs
- Zoom in/out on terminal tab, verify text scales
- Restart app, verify terminal tabs restore correctly
