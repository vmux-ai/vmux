# VMX-73: Cmd+L Navigate In-Tab Instead of New Tab

## Problem

When using Cmd+L (FocusAddressBar) and navigating to a terminal or filesystem path, the current behavior either does nothing or creates a new tab. Users expect Cmd+L to change content within the current tab, reserving new-tab creation for Cmd+T.

### Current Behavior

| Input | Cmd+L (normal mode) | Cmd+T (new tab mode) |
|---|---|---|
| URL | Changes `WebviewSource` on current browser | Spawns browser in empty tab |
| Filesystem path | **Does nothing** (only handled in new-tab mode) | Spawns terminal in empty tab |
| `vmux://terminal` | **Fires `TerminalCommand::New`** (creates new tab) | Spawns terminal in empty tab |
| "terminal" action | **Creates new tab** with terminal | Spawns terminal in empty tab |

### Desired Behavior

Cmd+L replaces current tab content when the current tab is a browser. When the current tab is a terminal, Cmd+L creates a new tab (to preserve the running terminal session).

## Design

### Event Schema Change

Add `new_tab: bool` to `CommandBarActionEvent` to make navigation intent explicit:

```rust
// crates/vmux_command_bar/src/event.rs
pub struct CommandBarActionEvent {
    pub action: String,
    pub value: String,
    pub new_tab: bool,  // true = Cmd+T mode, false = Cmd+L mode
}
```

The WASM command bar already receives `new_tab` via `CommandBarOpenEvent`. The `emit_action` helper propagates this flag into every action event.

Flow:
- Cmd+L -> `CommandBarOpenEvent { new_tab: false }` -> user action -> `CommandBarActionEvent { new_tab: false }`
- Cmd+T -> `CommandBarOpenEvent { new_tab: true }` -> user action -> `CommandBarActionEvent { new_tab: true }`

### Rust Handler Logic

`on_command_bar_action` in `command_bar.rs` uses `evt.new_tab` to determine behavior instead of checking `NewTabContext.tab`.

#### `navigate` action

| `new_tab` | Input type | Current tab | Behavior |
|---|---|---|---|
| `true` | URL | any | Spawn browser in empty tab (existing) |
| `true` | Path | any | Spawn terminal in empty tab (existing) |
| `true` | `vmux://terminal` | any | Spawn terminal in empty tab (existing) |
| `false` | URL | Browser | Change `WebviewSource` (existing, works) |
| `false` | URL | Terminal | Create new tab with browser |
| `false` | Path | Browser | Despawn browser, spawn terminal in current tab |
| `false` | Path | Terminal | Create new tab with terminal |
| `false` | `vmux://terminal` | Browser | Despawn browser, spawn terminal in current tab |
| `false` | `vmux://terminal` | Terminal | Create new tab with terminal |

#### `terminal` action

| `new_tab` | Current tab | Behavior |
|---|---|---|
| `true` | any | Spawn terminal in empty tab (existing) |
| `false` | Browser | Despawn browser, spawn terminal in current tab |
| `false` | Terminal | Create new tab with terminal |

#### `dismiss` action

When `new_tab: true`, despawn empty tab from `NewTabContext` and restore keyboard to previous tab (existing behavior). `NewTabContext` still tracks the empty tab entity for cleanup purposes.

### Replace-in-Tab Procedure

When replacing a browser tab's content with a terminal:

1. Find the current active tab entity via `focused_tab()`
2. Find the `Browser` child entity of that tab
3. Despawn the browser entity
4. Spawn new `Terminal` entity as child of the same tab
5. Update `PageMetadata` on the tab entity
6. Set `CefKeyboardTarget` on the new terminal entity

When replacing with a new URL (browser -> browser), the existing `WebviewSource` change is sufficient.

### Files Changed

| File | Change |
|---|---|
| `crates/vmux_command_bar/src/event.rs` | Add `new_tab: bool` field to `CommandBarActionEvent` |
| `crates/vmux_command_bar/src/app.rs` | Update `emit_action` to accept `new_tab` param, pass `new_tab()` signal at all call sites |
| `crates/vmux_desktop/src/command_bar.rs` | Rewrite `on_command_bar_action` to branch on `evt.new_tab` + current tab type detection |

### What Does NOT Change

- Command bar UI/UX (no visual changes)
- `CommandBarOpenEvent` (already has `new_tab` field)
- Keybinding definitions
- Tab/Pane/Space entity hierarchy
- `NewTabContext` resource (still used for empty tab cleanup)

## Testing

Manual testing matrix:
1. Cmd+L from browser tab -> type URL -> should navigate in current tab
2. Cmd+L from browser tab -> type path -> should replace browser with terminal in current tab
3. Cmd+L from browser tab -> select Terminal -> should replace browser with terminal in current tab
4. Cmd+L from terminal tab -> type URL -> should create new tab with browser
5. Cmd+L from terminal tab -> type path -> should create new tab with terminal
6. Cmd+T -> type URL -> should create new tab with browser (unchanged)
7. Cmd+T -> select Terminal -> should create new tab with terminal (unchanged)
8. Cmd+T -> Escape -> should despawn empty tab (unchanged)
