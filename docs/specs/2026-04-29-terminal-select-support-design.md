# Terminal Select Support — Design

VMX-101. Brings text selection to terminal panes via mouse and keyboard, with copy-to-clipboard.

## Goals

- Browser-like mouse selection: drag selects characters, double-click selects word, triple-click selects line, Shift+drag/click extends selection.
- tmux-style keyboard copy mode entered via `<leader>[` for keyboard-only selection and copy.
- `Cmd+C` copies the selection to the system clipboard.
- Selection state lives in the daemon and propagates via the existing `ViewportPatch.selection` field, so the renderer needs no new IPC channel for display.
- Respect terminal-app mouse capture mode (vim, htop, less, tmux): plain drag still forwards to the PTY, only Shift+drag selects locally.

## Non-Goals (v0.1)

- Rectangular (block) selection. The wire field `is_block` is preserved but always `false`. Block selection lands later if there is demand.
- Multi-pane selection (selection that spans across panes).
- Search-then-select / regex selection in copy mode.
- Cross-platform clipboard. macOS-only via `pbcopy`. Linux gets a TODO comment.
- Auto-copy on mouse-up. Explicit `Cmd+C` only.
- Scrollback selection (selecting text that has scrolled off the viewport). Selection ranges use viewport coordinates only.

## Data Model

`TermSelectionRange` already exists in `vmux_terminal::event` and is already serialized through `ViewportPatch.selection`. No new wire types needed for rendering.

### Daemon: per-session state

Add to `Session` in `crates/vmux_daemon/src/session.rs`:

```rust
selection: Option<TermSelectionRange>,
copy_mode: Option<CopyModeState>,
```

```rust
struct CopyModeState {
    cursor: (u16, u16),       // selection cursor position
    anchor: Option<(u16, u16)>, // when Some, selection extends from anchor → cursor
}
```

When `copy_mode` is `Some`, normal PTY input is suppressed; keyboard events go through `CopyModeKey` handling instead. When `copy_mode` is `None`, the session is in "normal" mode with optional mouse-driven selection.

### Daemon: VTE mode tracking

Alacritty's `Term` exposes `mode()` which includes mouse-capture flags. On every `sync_viewport`, derive `mouse_capture: bool` from term mode and broadcast `TerminalMode { session_id, mouse_capture, copy_mode: copy_mode.is_some() }` only when changed. Desktop reads this to decide whether plain drag should select or forward to PTY.

## Protocol Additions

In `crates/vmux_daemon/src/protocol.rs`:

```rust
pub enum ClientMessage {
    // ...existing
    SetSelection { session_id: SessionId, range: Option<TermSelectionRange> },
    SelectWordAt { session_id: SessionId, col: u16, row: u16 },
    SelectLineAt { session_id: SessionId, row: u16 },
    ExtendSelectionTo { session_id: SessionId, col: u16, row: u16 },
    GetSelectionText { session_id: SessionId },
    EnterCopyMode { session_id: SessionId },
    ExitCopyMode { session_id: SessionId },
    CopyModeKey { session_id: SessionId, key: CopyModeKey },
}

pub enum CopyModeKey {
    Left, Right, Up, Down,           // h/j/k/l + arrows
    LineStart, LineEnd,              // 0, $
    PageUp, PageDown,                // C-u, C-d
    StartSelection,                  // v
    Copy,                            // y
    Exit,                            // q, esc
}

pub enum DaemonMessage {
    // ...existing
    SelectionText { session_id: SessionId, text: String },
    TerminalMode { session_id: SessionId, mouse_capture: bool, copy_mode: bool },
}
```

## Daemon Behavior

### Selection propagation

`sync_viewport` already sends `selection: None` in every `ViewportPatch`. Replace with `selection: self.selection.clone()`. Treat selection changes the same as cursor-only changes: include selection-changed in the early-return guard so the patch fires when selection is set/cleared without line content changing (similar to the recent cursor-tracking fix).

### Selection text extraction

New helper `Session::selection_text(&self) -> Option<String>`:

- Walk `start_row..=end_row` in the term grid.
- For each row, take cells `start_col..=end_col` (clamped per-row for line-wrapped selection — only the first row uses `start_col` from the start, intermediate rows take col 0, last row stops at `end_col`).
- Join with `\n`.
- Strip trailing whitespace per line; trailing newline preserved only if selection ends past last char.

### Word/line boundary

`SelectWordAt` walks left and right from `(col, row)` while the cell character is in a word-character class (`[A-Za-z0-9_-/.]` initial set, configurable later). `SelectLineAt` selects entire row from col 0 to last non-blank cell.

### Mouse-capture flag

After every `processor.advance(...)`, compare `term.mode()` mouse flags to last known. If changed, broadcast `TerminalMode`.

### Copy mode

- `EnterCopyMode`: set `copy_mode = Some(CopyModeState { cursor: term.cursor, anchor: None })`. Broadcast `TerminalMode`. Set selection to `None` initially (single-cell highlight rendered via copy-mode cursor instead).
- `CopyModeKey::StartSelection`: set `anchor = Some(cursor)`. Selection range = `(anchor, cursor)`.
- `CopyModeKey::Left/Right/Up/Down/...`: move cursor, if anchor is set, update selection range.
- `CopyModeKey::Copy`: extract text from selection (or single cell if no anchor), send `SelectionText`, exit copy mode.
- `CopyModeKey::Exit`: clear `copy_mode`, clear selection, broadcast.

While `copy_mode.is_some()`, ignore `SessionInput` messages (PTY input suppressed).

## Desktop Behavior

### Mouse pipeline

`on_term_mouse` (`crates/vmux_desktop/src/terminal.rs:838`) currently translates every `TermMouseEvent` to an SGR mouse sequence and writes it to PTY. Rewrite as:

```
on_term_mouse:
    1. Read latest `mouse_capture` flag from per-session state (cached from TerminalMode broadcasts).
    2. Compute should_select = !mouse_capture || event.modifiers & MOD_SHIFT.
    3. If should_select:
        - On press (button=0, !moving):
            - Track click_count via timestamp + position window (300ms, 5px).
            - count==1 → SetSelection { range: zero-length at (col, row) }, store anchor.
            - count==2 → SelectWordAt
            - count==3 → SelectLineAt
            - With Shift: ExtendSelectionTo (extend from existing anchor)
        - On drag (moving=true): ExtendSelectionTo { col, row }
        - On release (button=0, !pressed): no-op (selection persists).
        - On press without Shift outside existing selection: SetSelection { None } first, then start fresh.
    4. Else: existing SGR forwarding to PTY.
```

`MouseSelectionState` resource (currently empty struct) stores `last_click: Option<(Instant, u16, u16, u8)>` (timestamp, col, row, count) for click-count detection.

### Cmd+C

Existing `KeyCode::KeyC` handler with `super_key` modifier in `handle_terminal_keyboard` is currently a `// TODO: implement copy via daemon snapshot` stub. Replace with `daemon.send(GetSelectionText { session_id })`. New `DaemonMessage::SelectionText` handler in `poll_daemon_messages` writes text to clipboard via `pbcopy`.

### `<leader>[` keybind

Add to `crates/vmux_desktop/src/settings.ron`:

```ron
(command: "terminal_copy_mode", binding: Leader((key: "["))),
```

Add `AppCommand::EnterTerminalCopyMode` variant. Handler in terminal layer sends `EnterCopyMode { session_id }` for the focused terminal.

### Copy mode keyboard

When `TerminalMode.copy_mode == true` for the focused terminal, `handle_terminal_keyboard` switches into copy-mode key mapping instead of forwarding bytes to PTY:

| Key | CopyModeKey |
|-----|-------------|
| `h`, `Left` | Left |
| `j`, `Down` | Down |
| `k`, `Up` | Up |
| `l`, `Right` | Right |
| `0` | LineStart |
| `$` | LineEnd |
| `Ctrl+u` | PageUp |
| `Ctrl+d` | PageDown |
| `v` | StartSelection |
| `y`, `Enter` | Copy |
| `q`, `Escape` | Exit |

All other keys ignored. After Copy, daemon sends `SelectionText` → desktop writes to clipboard.

### Visual indicator (copy mode)

Out of scope for v0.1 backend. Frontend can show a status overlay later by reading `TerminalMode.copy_mode`. The selection highlight overlay already renders via existing CSS.

## Renderer (vmux_terminal Dioxus app)

No new code needed for v0.1. Existing `row_selection_cols` / overlay in `crates/vmux_terminal/src/app.rs` already paints the selection highlight from `vp.selection`.

Optional polish: subtle copy-mode indicator (small "COPY" badge in corner) — punted to a follow-up issue.

## Error Handling

- `GetSelectionText` with no active selection → daemon sends `SelectionText { text: "" }`. Desktop skips clipboard write on empty.
- Selection coordinates outside grid bounds → daemon clamps silently.
- `pbcopy` failure → log via `eprintln!`, do not error the user.
- Unknown `CopyModeKey` value (forward-compat) → daemon ignores.

## Testing

- Daemon unit tests for `selection_text()` covering: single-line, multi-line, end-of-line whitespace stripping, empty selection, out-of-bounds clamping, word/line boundary detection.
- Daemon test: `EnterCopyMode → CopyModeKey::Right ×3 → StartSelection → CopyModeKey::Right ×2 → Copy` produces expected text.
- Desktop has no automated terminal mouse tests (manual). Add one integration smoke test: mock daemon receives expected `SetSelection` after a synthetic `TermMouseEvent` press+drag sequence.

## Migration

None. New protocol fields are additive; daemon and desktop versions are bumped together.

## Implementation Order

1. Wire `Session.selection` into `ViewportPatch` (no behavior yet, just plumbing).
2. `SetSelection` + `SelectWordAt` + `SelectLineAt` + `ExtendSelectionTo` daemon-side, with text extraction and tests.
3. `TerminalMode` broadcast + per-session cache in desktop.
4. Mouse pipeline rewrite in `on_term_mouse` + `MouseSelectionState`.
5. `GetSelectionText` + `SelectionText` + Cmd+C → `pbcopy`.
6. Copy-mode protocol (`EnterCopyMode`, `CopyModeKey`, `ExitCopyMode`).
7. `<leader>[` keybind + `AppCommand::EnterTerminalCopyMode`.
8. Copy-mode keymap in `handle_terminal_keyboard`.

Each step ships a working slice; mouse selection alone is shippable after step 5.
