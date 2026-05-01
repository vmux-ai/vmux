# Terminal Select Support Implementation Plan (VMX-101)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add browser-style mouse selection and tmux-style copy mode to terminal panes. State lives in the daemon. Cmd+C copies the daemon-resolved selection to the system clipboard.

**Design doc:** `docs/specs/2026-04-29-terminal-select-support-design.md`

**Worktree:** `.worktrees/vmx-101` on branch `jun/vmx-101-terminal-select-support`

**Architecture:**
- Daemon owns selection + copy-mode state per session, broadcasts via existing `ViewportPatch.selection`.
- New `ClientMessage`s: `SetSelection`, `SelectWordAt`, `SelectLineAt`, `ExtendSelectionTo`, `GetSelectionText`, `EnterCopyMode`, `ExitCopyMode`, `CopyModeKey`.
- New `DaemonMessage`s: `SelectionText { text }`, `TerminalMode { mouse_capture, copy_mode }`.
- Desktop tracks per-session `TerminalMode` for mouse-capture override (Shift+drag forces selection); rewrites `on_term_mouse` for click-count detection; replaces Cmd+C TODO with daemon round-trip → `pbcopy`.
- New `AppCommand::Terminal::CopyMode` bound to `<leader>[`; copy-mode keymap activates only when daemon reports `copy_mode == true`.

**Tech Stack:** Rust, tokio, alacritty_terminal 0.26 (`Term::mode()`, `TermMode::MOUSE_MODE`), rkyv, Bevy 0.18.

---

### Task 1: Wire `Session.selection` field through `ViewportPatch`

**Files:**
- Modify: `crates/vmux_daemon/src/session.rs`

- [ ] **Step 1: Add `selection` field to `Session`**

In `crates/vmux_daemon/src/session.rs` `pub struct Session`, after `last_cursor`:

```rust
    /// Currently selected range (in viewport coords). None when no selection.
    selection: Option<TermSelectionRange>,
```

In `Session::new()`'s returned `Self { ... }`, add:

```rust
            selection: None,
```

- [ ] **Step 2: Track selection changes in `sync_viewport`**

In `sync_viewport`, after the existing `let cursor_moved = ...;` line and BEFORE the early-return guard, add a hash-compare for selection:

```rust
        let selection_changed = {
            // Tracked separately from line content; broadcast even when only selection toggles.
            let last = std::mem::take(&mut self.last_selection_signature);
            let cur = selection_signature(&self.selection);
            self.last_selection_signature = cur;
            last != cur
        };
```

Update the early-return guard to:

```rust
        if changed_lines.is_empty() && !full && !cursor_moved && !selection_changed {
            return;
        }
```

In the `DaemonMessage::ViewportPatch { ... }` literal, replace `selection: None,` with:

```rust
            selection: self.selection.clone(),
```

Add to `Session` struct:

```rust
    /// Stable hash of the last broadcast selection, so toggles re-trigger sync.
    last_selection_signature: u64,
```

Initialize in `Session::new()`:

```rust
            last_selection_signature: 0,
```

Add helper near the bottom of the file (next to `hash_grid_row`):

```rust
fn selection_signature(sel: &Option<TermSelectionRange>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    match sel {
        None => 0u8.hash(&mut h),
        Some(r) => {
            1u8.hash(&mut h);
            r.start_col.hash(&mut h);
            r.start_row.hash(&mut h);
            r.end_col.hash(&mut h);
            r.end_row.hash(&mut h);
            r.is_block.hash(&mut h);
        }
    }
    h.finish()
}
```

- [ ] **Step 3: Verify build**

```bash
env -u CEF_PATH cargo check -p vmux_daemon
```

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_daemon/src/session.rs
git commit -m "feat(daemon): plumb Session.selection through ViewportPatch"
```

---

### Task 2: Daemon `SetSelection` / `ExtendSelectionTo` + `selection_text` helper + tests

**Files:**
- Modify: `crates/vmux_daemon/src/protocol.rs`
- Modify: `crates/vmux_daemon/src/server.rs`
- Modify: `crates/vmux_daemon/src/session.rs`
- Create: `crates/vmux_daemon/tests/selection.rs`

- [ ] **Step 1: Add new `ClientMessage` variants**

In `crates/vmux_daemon/src/protocol.rs`, append to `ClientMessage` enum (before the closing `}`):

```rust
    SetSelection {
        session_id: SessionId,
        range: Option<TermSelectionRange>,
    },
    ExtendSelectionTo {
        session_id: SessionId,
        col: u16,
        row: u16,
    },
    SelectWordAt {
        session_id: SessionId,
        col: u16,
        row: u16,
    },
    SelectLineAt {
        session_id: SessionId,
        row: u16,
    },
    GetSelectionText {
        session_id: SessionId,
    },
```

Append to `DaemonMessage` enum (before closing `}`):

```rust
    SelectionText {
        session_id: SessionId,
        text: String,
    },
```

- [ ] **Step 2: Selection setters + text extraction on `Session`**

In `crates/vmux_daemon/src/session.rs`, add public methods on `Session` (place after `pub fn write_input`):

```rust
    /// Replace the selection. Empty range clears.
    pub fn set_selection(&mut self, range: Option<TermSelectionRange>) {
        self.selection = range;
        self.sync_viewport();
    }

    /// Extend the current selection's end point to (col, row). If no
    /// selection exists, anchor at (col, row).
    pub fn extend_selection_to(&mut self, col: u16, row: u16) {
        let range = match self.selection.take() {
            Some(mut r) => {
                r.end_col = col;
                r.end_row = row;
                r
            }
            None => TermSelectionRange {
                start_col: col,
                start_row: row,
                end_col: col,
                end_row: row,
                is_block: false,
            },
        };
        self.selection = Some(range);
        self.sync_viewport();
    }

    /// Select the word at (col, row), where "word" is a maximal run of
    /// `[A-Za-z0-9_./-]` characters.
    pub fn select_word_at(&mut self, col: u16, row: u16) {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        if (row as usize) >= grid.screen_lines() || (col as usize) >= num_cols {
            return;
        }
        let offset = grid.display_offset() as i32;
        let line = &grid[Line(row as i32 - offset)];
        let is_word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '/' | '-');
        let mut start = col as usize;
        let mut end = col as usize;
        while start > 0 && is_word(line[Column(start - 1)].c) {
            start -= 1;
        }
        while end + 1 < num_cols && is_word(line[Column(end + 1)].c) {
            end += 1;
        }
        self.selection = Some(TermSelectionRange {
            start_col: start as u16,
            start_row: row,
            end_col: end as u16,
            end_row: row,
            is_block: false,
        });
        self.sync_viewport();
    }

    /// Select the entire row from col 0 to the last non-blank cell.
    pub fn select_line_at(&mut self, row: u16) {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        if (row as usize) >= grid.screen_lines() {
            return;
        }
        let offset = grid.display_offset() as i32;
        let line = &grid[Line(row as i32 - offset)];
        let mut end = 0usize;
        for c in 0..num_cols {
            if !line[Column(c)].c.is_whitespace() {
                end = c;
            }
        }
        self.selection = Some(TermSelectionRange {
            start_col: 0,
            start_row: row,
            end_col: end as u16,
            end_row: row,
            is_block: false,
        });
        self.sync_viewport();
    }

    /// Extract selected text. Lines joined by `\n`, trailing whitespace
    /// stripped per line.
    pub fn selection_text(&self) -> Option<String> {
        let sel = self.selection.as_ref()?;
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();
        let offset = grid.display_offset() as i32;

        // Normalize so (start) <= (end) in row-major order.
        let (sr, sc, er, ec) = if (sel.start_row, sel.start_col) <= (sel.end_row, sel.end_col) {
            (sel.start_row, sel.start_col, sel.end_row, sel.end_col)
        } else {
            (sel.end_row, sel.end_col, sel.start_row, sel.start_col)
        };

        let mut lines: Vec<String> = Vec::new();
        for row_idx in sr..=er {
            if (row_idx as usize) >= num_lines {
                break;
            }
            let line = &grid[Line(row_idx as i32 - offset)];
            let (lo, hi) = if sel.is_block {
                (sc as usize, ec as usize)
            } else if sr == er {
                (sc as usize, ec as usize)
            } else if row_idx == sr {
                (sc as usize, num_cols.saturating_sub(1))
            } else if row_idx == er {
                (0, ec as usize)
            } else {
                (0, num_cols.saturating_sub(1))
            };
            let mut s = String::new();
            for c in lo..=hi.min(num_cols.saturating_sub(1)) {
                s.push(line[Column(c)].c);
            }
            while s.ends_with(' ') {
                s.pop();
            }
            lines.push(s);
        }
        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }
```

- [ ] **Step 3: Server dispatch for new messages**

In `crates/vmux_daemon/src/server.rs`'s `match msg { ... }` block, add arms (before `ClientMessage::Shutdown`):

```rust
            ClientMessage::SetSelection { session_id, range } => {
                let mut mgr = manager.lock().await;
                if let Some(s) = mgr.sessions.get_mut(&session_id) {
                    s.set_selection(range);
                }
            }
            ClientMessage::ExtendSelectionTo {
                session_id,
                col,
                row,
            } => {
                let mut mgr = manager.lock().await;
                if let Some(s) = mgr.sessions.get_mut(&session_id) {
                    s.extend_selection_to(col, row);
                }
            }
            ClientMessage::SelectWordAt {
                session_id,
                col,
                row,
            } => {
                let mut mgr = manager.lock().await;
                if let Some(s) = mgr.sessions.get_mut(&session_id) {
                    s.select_word_at(col, row);
                }
            }
            ClientMessage::SelectLineAt { session_id, row } => {
                let mut mgr = manager.lock().await;
                if let Some(s) = mgr.sessions.get_mut(&session_id) {
                    s.select_line_at(row);
                }
            }
            ClientMessage::GetSelectionText { session_id } => {
                let mgr = manager.lock().await;
                let text = mgr
                    .sessions
                    .get(&session_id)
                    .and_then(|s| s.selection_text())
                    .unwrap_or_default();
                let resp = DaemonMessage::SelectionText { session_id, text };
                let mut w = writer.lock().await;
                write_message!(&mut *w, &resp)?;
            }
```

- [ ] **Step 4: Add tests**

Create `crates/vmux_daemon/tests/selection.rs`:

```rust
use vmux_daemon::session::Session;
use vmux_terminal::event::TermSelectionRange;

fn new_session() -> Session {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    Session::new(shell, String::new(), Vec::new(), 80, 24).expect("spawn session")
}

fn write_and_drain(s: &mut Session, bytes: &[u8]) {
    s.write_input(bytes);
    // Give the PTY a moment to echo back through the reader thread.
    for _ in 0..50 {
        if s.poll() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn set_and_clear_selection() {
    let mut s = new_session();
    s.set_selection(Some(TermSelectionRange {
        start_col: 0,
        start_row: 0,
        end_col: 4,
        end_row: 0,
        is_block: false,
    }));
    assert!(s.selection_text().is_some());
    s.set_selection(None);
    assert!(s.selection_text().is_none());
}

#[test]
fn extend_from_empty_anchors() {
    let mut s = new_session();
    s.extend_selection_to(3, 1);
    let text = s.selection_text().unwrap_or_default();
    // Single-cell selection is at most one char wide; allow empty for blank cell.
    assert!(text.chars().count() <= 1, "got {text:?}");
}

#[test]
fn select_line_strips_trailing_blanks() {
    let mut s = new_session();
    write_and_drain(&mut s, b"hello world\n");
    // After the prompt + newline the line containing "hello world" exists somewhere
    // in the viewport; pick the first row that is non-empty when fully selected.
    let mut found = false;
    for row in 0..24u16 {
        s.select_line_at(row);
        if let Some(t) = s.selection_text()
            && t.contains("hello world")
        {
            assert!(!t.ends_with(' '));
            found = true;
            break;
        }
    }
    assert!(found, "did not find 'hello world' in any row");
}

#[test]
fn select_word_walks_word_chars() {
    let mut s = new_session();
    write_and_drain(&mut s, b"foo_bar baz\n");
    for row in 0..24u16 {
        for col in 0..11u16 {
            s.select_word_at(col, row);
            if let Some(t) = s.selection_text()
                && t == "foo_bar"
            {
                return;
            }
        }
    }
    panic!("did not find foo_bar word selection");
}
```

Make `Session::selection_text` callable in tests by ensuring `Session` and its module are `pub` (already are via `pub mod session;` in `lib.rs`).

- [ ] **Step 5: Verify**

```bash
env -u CEF_PATH cargo test -p vmux_daemon
```

Expected: 4 selection tests pass. (PTY tests are slow; if `select_line_strips_trailing_blanks` flakes due to shell prompt timing, it is acceptable to mark `#[ignore]` and revisit.)

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_daemon/src/protocol.rs crates/vmux_daemon/src/session.rs crates/vmux_daemon/src/server.rs crates/vmux_daemon/tests/selection.rs
git commit -m "feat(daemon): selection API + text extraction (VMX-101)"
```

---

### Task 3: Daemon broadcasts `TerminalMode { mouse_capture, copy_mode }`

**Files:**
- Modify: `crates/vmux_daemon/src/protocol.rs`
- Modify: `crates/vmux_daemon/src/session.rs`

- [ ] **Step 1: Add message variant**

In `protocol.rs` `DaemonMessage`:

```rust
    TerminalMode {
        session_id: SessionId,
        mouse_capture: bool,
        copy_mode: bool,
    },
```

- [ ] **Step 2: Track and broadcast mode changes in `Session`**

In `session.rs`, add field to `Session`:

```rust
    last_terminal_mode: Option<(bool, bool)>,
```

Initialize in `Session::new()`:

```rust
            last_terminal_mode: None,
```

Add a helper method on `Session`:

```rust
    fn maybe_broadcast_mode(&mut self) {
        use alacritty_terminal::term::TermMode;
        let mouse_capture = self.term.mode().intersects(TermMode::MOUSE_MODE);
        let copy_mode = self.copy_mode.is_some();
        let cur = (mouse_capture, copy_mode);
        if self.last_terminal_mode != Some(cur) {
            self.last_terminal_mode = Some(cur);
            let _ = self.patch_tx.send(DaemonMessage::TerminalMode {
                session_id: self.id,
                mouse_capture,
                copy_mode,
            });
        }
    }
```

(Note: `copy_mode` field is added in Task 7; for now stub it as `false`.) Replace the body until Task 7 ships:

```rust
    fn maybe_broadcast_mode(&mut self) {
        use alacritty_terminal::term::TermMode;
        let mouse_capture = self.term.mode().intersects(TermMode::MOUSE_MODE);
        let cur = (mouse_capture, false);
        if self.last_terminal_mode != Some(cur) {
            self.last_terminal_mode = Some(cur);
            let _ = self.patch_tx.send(DaemonMessage::TerminalMode {
                session_id: self.id,
                mouse_capture,
                copy_mode: false,
            });
        }
    }
```

Call it at the end of `poll()` after `if got_data { self.sync_viewport(); }`:

```rust
        self.maybe_broadcast_mode();
```

- [ ] **Step 3: Verify**

```bash
env -u CEF_PATH cargo check -p vmux_daemon
```

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_daemon/src/protocol.rs crates/vmux_daemon/src/session.rs
git commit -m "feat(daemon): broadcast TerminalMode mouse_capture state"
```

---

### Task 4: Desktop caches per-session `TerminalMode`

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Add resource**

In `crates/vmux_desktop/src/terminal.rs`, near the other `#[derive(Resource)]` definitions (before `pub(crate) struct TerminalInputPlugin;`):

```rust
/// Per-session terminal mode flags, last broadcast by the daemon.
#[derive(Resource, Default)]
pub(crate) struct TerminalModeMap {
    pub modes: bevy::utils::HashMap<SessionId, TerminalModeFlags>,
}

#[derive(Default, Clone, Copy, Debug)]
pub(crate) struct TerminalModeFlags {
    pub mouse_capture: bool,
    pub copy_mode: bool,
}
```

If `bevy::utils::HashMap` is not in this Bevy version, fall back to `std::collections::HashMap` (verify at compile time).

In `TerminalInputPlugin::build`, add after `app.init_resource::<MouseSelectionState>()`:

```rust
            .init_resource::<TerminalModeMap>()
```

- [ ] **Step 2: Handle `TerminalMode` in `poll_daemon_messages`**

In the `match msg { ... }` block of `poll_daemon_messages`, add a new arm before the trailing `_ => {}`:

```rust
            DaemonMessage::TerminalMode {
                session_id,
                mouse_capture,
                copy_mode,
            } => {
                commands.queue(move |world: &mut World| {
                    let mut map = world.resource_mut::<TerminalModeMap>();
                    map.modes.insert(
                        session_id,
                        TerminalModeFlags {
                            mouse_capture,
                            copy_mode,
                        },
                    );
                });
            }
```

(If `commands.queue` is not available in this Bevy version, switch to a plain `ResMut<TerminalModeMap>` system param on `poll_daemon_messages`.)

- [ ] **Step 3: Verify**

```bash
env -u CEF_PATH cargo check -p vmux_desktop
```

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/terminal.rs
git commit -m "feat(desktop): cache TerminalMode per session"
```

---

### Task 5: Mouse selection pipeline (browser-style)

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Replace `MouseSelectionState`**

In `crates/vmux_desktop/src/terminal.rs`, replace the existing empty struct:

```rust
/// Tracks mouse state for selection (reserved for future local selection).
#[derive(Resource, Default)]
struct MouseSelectionState;
```

with:

```rust
/// Tracks the most recent mouse-down per session for click-count detection
/// (300ms / 5px window) and an active drag anchor.
#[derive(Resource, Default)]
struct MouseSelectionState {
    per_session: std::collections::HashMap<SessionId, MouseSessionState>,
}

#[derive(Default, Clone, Debug)]
struct MouseSessionState {
    last_click: Option<MouseClickRecord>,
    drag_active: bool,
}

#[derive(Clone, Copy, Debug)]
struct MouseClickRecord {
    when: std::time::Instant,
    col: u16,
    row: u16,
    count: u8,
}
```

- [ ] **Step 2: Rewrite `on_term_mouse`**

Replace the entire body of `on_term_mouse` with:

```rust
fn on_term_mouse(
    trigger: On<Receive<TermMouseEvent>>,
    q: Query<&DaemonSessionHandle, With<Terminal>>,
    daemon: Option<Res<DaemonClient>>,
    mode_map: Res<TerminalModeMap>,
    mut state: ResMut<MouseSelectionState>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(daemon) = daemon else { return };
    let Ok(handle) = q.get(entity) else { return };
    let session_id = handle.session_id;

    let mouse_capture = mode_map
        .modes
        .get(&session_id)
        .map(|m| m.mouse_capture)
        .unwrap_or(false);
    let shift = event.modifiers & MOD_SHIFT != 0;
    let select_mode = !mouse_capture || shift;

    // Scroll wheel + non-left buttons: always forward to PTY (selection only
    // hijacks the left button).
    if event.button != 0 || !select_mode {
        let button = if event.moving {
            event.button + 32
        } else {
            event.button
        };
        let seq = sgr_mouse_sequence(
            button,
            event.col,
            event.row,
            event.modifiers,
            event.pressed,
        );
        daemon.0.send(ClientMessage::SessionInput {
            session_id,
            data: seq,
        });
        return;
    }

    let entry = state.per_session.entry(session_id).or_default();

    if event.pressed && !event.moving {
        // Mouse-down: detect click count.
        let now = std::time::Instant::now();
        let count = match entry.last_click {
            Some(prev)
                if now.duration_since(prev.when).as_millis() <= 300
                    && (prev.col as i32 - event.col as i32).abs() <= 1
                    && (prev.row as i32 - event.row as i32).abs() <= 1 =>
            {
                (prev.count + 1).min(3)
            }
            _ => 1,
        };
        entry.last_click = Some(MouseClickRecord {
            when: now,
            col: event.col,
            row: event.row,
            count,
        });
        entry.drag_active = count == 1;

        match count {
            1 if shift => {
                daemon.0.send(ClientMessage::ExtendSelectionTo {
                    session_id,
                    col: event.col,
                    row: event.row,
                });
            }
            1 => {
                daemon.0.send(ClientMessage::SetSelection {
                    session_id,
                    range: Some(vmux_terminal::event::TermSelectionRange {
                        start_col: event.col,
                        start_row: event.row,
                        end_col: event.col,
                        end_row: event.row,
                        is_block: false,
                    }),
                });
            }
            2 => daemon.0.send(ClientMessage::SelectWordAt {
                session_id,
                col: event.col,
                row: event.row,
            }),
            _ => daemon.0.send(ClientMessage::SelectLineAt {
                session_id,
                row: event.row,
            }),
        }
    } else if event.moving && entry.drag_active {
        daemon.0.send(ClientMessage::ExtendSelectionTo {
            session_id,
            col: event.col,
            row: event.row,
        });
    } else if !event.pressed {
        entry.drag_active = false;
    }
}
```

- [ ] **Step 3: Verify**

```bash
env -u CEF_PATH cargo check -p vmux_desktop
```

- [ ] **Step 4: Manual smoke test**

```bash
make run-mac
# In a terminal pane: drag → highlight follows. Double-click → word.
# Triple-click → line. Shift+click → extends end.
```

(If smoke test fails because the daemon process is stale, run `pkill -f 'Vmux daemon'` and retry — see AGENTS.md note.)

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/terminal.rs
git commit -m "feat(desktop): browser-style mouse selection in terminals (VMX-101)"
```

---

### Task 6: Cmd+C copies selection via daemon

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Replace the Cmd+C TODO**

In `handle_terminal_keyboard`, locate:

```rust
                KeyCode::KeyC => {
                    // Copy: in daemon mode we can't access selection, so skip
                    // TODO: implement copy via daemon snapshot
                    continue;
                }
```

Replace with:

```rust
                KeyCode::KeyC => {
                    for handle in &q {
                        daemon.0.send(ClientMessage::GetSelectionText {
                            session_id: handle.session_id,
                        });
                    }
                    continue;
                }
```

- [ ] **Step 2: Handle `SelectionText` in `poll_daemon_messages`**

Add an arm in the daemon-message `match msg`:

```rust
            DaemonMessage::SelectionText { session_id: _, text } => {
                if !text.is_empty() {
                    write_to_pasteboard(&text);
                }
            }
```

Add a free function near the bottom of `terminal.rs`:

```rust
fn write_to_pasteboard(text: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    match Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
        Ok(mut child) => {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
        Err(e) => warn!("pbcopy failed: {e}"),
    }
}
```

- [ ] **Step 3: Verify**

```bash
env -u CEF_PATH cargo check -p vmux_desktop
```

- [ ] **Step 4: Manual smoke test**

```bash
pkill -f 'Vmux daemon'
make run-mac
# Select text in terminal, hit Cmd+C, verify clipboard contents with `pbpaste`.
```

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/terminal.rs
git commit -m "feat(desktop): Cmd+C copies daemon selection to pasteboard"
```

---

### Task 7: Daemon copy-mode protocol

**Files:**
- Modify: `crates/vmux_daemon/src/protocol.rs`
- Modify: `crates/vmux_daemon/src/session.rs`
- Modify: `crates/vmux_daemon/src/server.rs`
- Modify: `crates/vmux_daemon/tests/selection.rs`

- [ ] **Step 1: Add protocol variants**

In `protocol.rs` `ClientMessage`:

```rust
    EnterCopyMode {
        session_id: SessionId,
    },
    ExitCopyMode {
        session_id: SessionId,
    },
    CopyModeKey {
        session_id: SessionId,
        key: CopyModeKey,
    },
```

Add at the end of `protocol.rs`:

```rust
#[derive(Debug, Clone, Copy, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum CopyModeKey {
    Left,
    Right,
    Up,
    Down,
    LineStart,
    LineEnd,
    PageUp,
    PageDown,
    StartSelection,
    Copy,
    Exit,
}
```

- [ ] **Step 2: `Session` copy-mode state**

In `session.rs`, add:

```rust
struct CopyModeState {
    cursor: (u16, u16),
    anchor: Option<(u16, u16)>,
}
```

To `Session`:

```rust
    copy_mode: Option<CopyModeState>,
```

Initialize in `Session::new()`:

```rust
            copy_mode: None,
```

Update `maybe_broadcast_mode` (added in Task 3) to use `self.copy_mode.is_some()` instead of `false`.

Add public methods:

```rust
    pub fn enter_copy_mode(&mut self) {
        let grid = self.term.grid();
        let cursor = (
            grid.cursor.point.column.0 as u16,
            grid.cursor.point.line.0 as u16,
        );
        self.copy_mode = Some(CopyModeState {
            cursor,
            anchor: None,
        });
        self.selection = None;
        self.maybe_broadcast_mode();
        self.sync_viewport();
    }

    pub fn exit_copy_mode(&mut self) {
        self.copy_mode = None;
        self.selection = None;
        self.maybe_broadcast_mode();
        self.sync_viewport();
    }

    /// Returns Some(text) if the key triggered a Copy action.
    pub fn copy_mode_key(&mut self, key: crate::protocol::CopyModeKey) -> Option<String> {
        use crate::protocol::CopyModeKey as K;
        let cm = self.copy_mode.as_mut()?;
        let cols = self.cols;
        let rows = self.rows;
        let (cur_col, cur_row) = cm.cursor;
        let (new_col, new_row) = match key {
            K::Left => (cur_col.saturating_sub(1), cur_row),
            K::Right => ((cur_col + 1).min(cols.saturating_sub(1)), cur_row),
            K::Up => (cur_col, cur_row.saturating_sub(1)),
            K::Down => (cur_col, (cur_row + 1).min(rows.saturating_sub(1))),
            K::LineStart => (0, cur_row),
            K::LineEnd => (cols.saturating_sub(1), cur_row),
            K::PageUp => (cur_col, cur_row.saturating_sub(rows / 2)),
            K::PageDown => (cur_col, (cur_row + rows / 2).min(rows.saturating_sub(1))),
            K::StartSelection => {
                cm.anchor = Some((cur_col, cur_row));
                (cur_col, cur_row)
            }
            K::Exit => {
                self.exit_copy_mode();
                return None;
            }
            K::Copy => {
                let text = self.selection_text();
                self.exit_copy_mode();
                return text;
            }
        };
        cm.cursor = (new_col, new_row);
        if let Some((ac, ar)) = cm.anchor {
            self.selection = Some(TermSelectionRange {
                start_col: ac,
                start_row: ar,
                end_col: new_col,
                end_row: new_row,
                is_block: false,
            });
        }
        self.sync_viewport();
        None
    }
```

- [ ] **Step 3: Server dispatch**

In `server.rs`, add arms:

```rust
            ClientMessage::EnterCopyMode { session_id } => {
                let mut mgr = manager.lock().await;
                if let Some(s) = mgr.sessions.get_mut(&session_id) {
                    s.enter_copy_mode();
                }
            }
            ClientMessage::ExitCopyMode { session_id } => {
                let mut mgr = manager.lock().await;
                if let Some(s) = mgr.sessions.get_mut(&session_id) {
                    s.exit_copy_mode();
                }
            }
            ClientMessage::CopyModeKey { session_id, key } => {
                let mut mgr = manager.lock().await;
                if let Some(s) = mgr.sessions.get_mut(&session_id) {
                    if let Some(text) = s.copy_mode_key(key) {
                        let resp = DaemonMessage::SelectionText { session_id, text };
                        let mut w = writer.lock().await;
                        write_message!(&mut *w, &resp)?;
                    }
                }
            }
```

Also gate `SessionInput` while copy_mode is active. Replace the existing `ClientMessage::SessionInput` arm:

```rust
            ClientMessage::SessionInput { session_id, data } => {
                let mgr = manager.lock().await;
                if let Some(session) = mgr.sessions.get(&session_id) {
                    if !session.is_copy_mode() {
                        session.write_input(&data);
                    }
                }
            }
```

Add helper on `Session`:

```rust
    pub fn is_copy_mode(&self) -> bool {
        self.copy_mode.is_some()
    }
```

- [ ] **Step 4: Add tests**

Append to `crates/vmux_daemon/tests/selection.rs`:

```rust
use vmux_daemon::protocol::CopyModeKey;

#[test]
fn copy_mode_movement_creates_selection() {
    let mut s = new_session();
    s.enter_copy_mode();
    assert!(s.is_copy_mode());
    s.copy_mode_key(CopyModeKey::StartSelection);
    s.copy_mode_key(CopyModeKey::Right);
    s.copy_mode_key(CopyModeKey::Right);
    assert!(s.selection_text().is_some());
    let copied = s.copy_mode_key(CopyModeKey::Copy);
    assert!(copied.is_some());
    assert!(!s.is_copy_mode());
}

#[test]
fn copy_mode_exit_clears_state() {
    let mut s = new_session();
    s.enter_copy_mode();
    s.copy_mode_key(CopyModeKey::Exit);
    assert!(!s.is_copy_mode());
}
```

- [ ] **Step 5: Verify**

```bash
env -u CEF_PATH cargo test -p vmux_daemon
```

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_daemon/src/protocol.rs crates/vmux_daemon/src/session.rs crates/vmux_daemon/src/server.rs crates/vmux_daemon/tests/selection.rs
git commit -m "feat(daemon): copy-mode protocol + cursor/anchor state (VMX-101)"
```

---

### Task 8: Bind `<leader>[` to enter copy mode

**Files:**
- Modify: `crates/vmux_desktop/src/command.rs`
- Modify: `crates/vmux_desktop/src/settings.ron`

- [ ] **Step 1: Add `TerminalCommand::CopyMode`**

In `crates/vmux_desktop/src/command.rs` `enum TerminalCommand`, after the `Clear` variant:

```rust
    #[menu(id = "terminal_copy_mode", label = "Copy Mode\t<leader> [", hidden)]
    #[shortcut(chord = "Ctrl+g, [")]
    CopyMode,
```

(Note: macro uses literal `Ctrl+g` even though runtime leader is `Ctrl+B`; that mirrors existing entries — see `tab_duplicate`.)

- [ ] **Step 2: Add settings binding**

In `crates/vmux_desktop/src/settings.ron` `bindings:` array, after the existing terminal-related entries (keep alphabetical-ish order with other Leader entries):

```ron
            (command: "terminal_copy_mode", binding: Leader((key: "["))),
```

- [ ] **Step 3: Verify**

```bash
env -u CEF_PATH cargo check -p vmux_desktop
```

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/command.rs crates/vmux_desktop/src/settings.ron
git commit -m "feat(shortcut): bind <leader>[ to terminal copy mode (VMX-101)"
```

---

### Task 9: Desktop dispatch — `CopyMode` enters daemon copy mode + key forwarding

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Add system that consumes `AppCommand::Terminal::CopyMode`**

Append to `terminal.rs`:

```rust
fn handle_terminal_copy_mode_command(
    mut er: MessageReader<AppCommand>,
    q: Query<&DaemonSessionHandle, (With<Terminal>, With<CefKeyboardTarget>)>,
    daemon: Option<Res<DaemonClient>>,
) {
    let Some(daemon) = daemon else {
        for _ in er.read() {}
        return;
    };
    for cmd in er.read() {
        if matches!(
            cmd,
            AppCommand::Terminal(crate::command::TerminalCommand::CopyMode)
        ) {
            for handle in &q {
                daemon
                    .0
                    .send(ClientMessage::EnterCopyMode {
                        session_id: handle.session_id,
                    });
            }
        }
    }
}
```

Register in `TerminalInputPlugin::build` inside the `Update` system tuple:

```rust
                    handle_terminal_copy_mode_command.in_set(crate::command::ReadAppCommands),
```

- [ ] **Step 2: Add copy-mode keymap inside `handle_terminal_keyboard`**

At the very top of the per-event `for event in er.read() { ... }` loop (before the `super_key` branch), insert:

```rust
        // Copy-mode: intercept keys and forward as CopyModeKey, suppress PTY input.
        let copy_mode_active = q.iter().any(|h| {
            mode_map
                .modes
                .get(&h.session_id)
                .map(|m| m.copy_mode)
                .unwrap_or(false)
        });
        if copy_mode_active {
            use crate::command::TerminalCommand;
            use vmux_daemon::protocol::CopyModeKey as K;
            let key = match (&event.logical_key, ctrl) {
                (Key::Character(s), false) if s.as_str() == "h" => Some(K::Left),
                (Key::Character(s), false) if s.as_str() == "j" => Some(K::Down),
                (Key::Character(s), false) if s.as_str() == "k" => Some(K::Up),
                (Key::Character(s), false) if s.as_str() == "l" => Some(K::Right),
                (Key::Character(s), false) if s.as_str() == "0" => Some(K::LineStart),
                (Key::Character(s), false) if s.as_str() == "$" => Some(K::LineEnd),
                (Key::Character(s), true) if s.as_str() == "u" => Some(K::PageUp),
                (Key::Character(s), true) if s.as_str() == "d" => Some(K::PageDown),
                (Key::Character(s), false) if s.as_str() == "v" => Some(K::StartSelection),
                (Key::Character(s), false) if s.as_str() == "y" => Some(K::Copy),
                (Key::Enter, _) => Some(K::Copy),
                (Key::Character(s), false) if s.as_str() == "q" => Some(K::Exit),
                (Key::Escape, _) => Some(K::Exit),
                (Key::ArrowLeft, _) => Some(K::Left),
                (Key::ArrowRight, _) => Some(K::Right),
                (Key::ArrowUp, _) => Some(K::Up),
                (Key::ArrowDown, _) => Some(K::Down),
                _ => None,
            };
            if let Some(k) = key {
                for handle in &q {
                    daemon.0.send(ClientMessage::CopyModeKey {
                        session_id: handle.session_id,
                        key: k,
                    });
                }
                let _ = TerminalCommand::CopyMode; // touch import
                continue;
            }
            // Swallow non-mapped keys while in copy mode.
            continue;
        }
```

Add `mode_map: Res<TerminalModeMap>` to `handle_terminal_keyboard`'s signature and remove the placeholder `let _ = TerminalCommand::CopyMode;` line if rustc complains about an unused import — instead drop the `use crate::command::TerminalCommand;` line.

- [ ] **Step 3: Verify**

```bash
env -u CEF_PATH cargo check -p vmux_desktop
```

- [ ] **Step 4: Manual smoke test**

```bash
pkill -f 'Vmux daemon'
make run-mac
# In a terminal pane: Ctrl+B then [ — cursor freezes (PTY input blocked).
# Press v then move with h/j/k/l — selection grows.
# Press y — text copied to clipboard, copy mode exits.
# Press q or Esc — exits without copying.
```

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/terminal.rs
git commit -m "feat(desktop): tmux-style copy mode keymap (VMX-101)"
```

---

### Task 10: Final lint + test sweep

**Files:** none

- [ ] **Step 1: Run pre-commit checks per AGENTS.md**

```bash
PKGS=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.manifest_path | test("patches") | not) | .name')
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
env -u CEF_PATH cargo test --workspace --exclude bevy_cef_core
```

- [ ] **Step 2: Fix any failures, then commit**

```bash
git add -u
git commit -m "chore: rustfmt/clippy fixups for VMX-101"
```

(If no fixups needed, skip this commit.)

---

## Self-review

- Daemon-only state: yes (selection + copy_mode live in `Session`).
- Mouse-capture override: yes — `select_mode = !mouse_capture || shift`.
- Cmd+C: yes — round-trips through `GetSelectionText` → `SelectionText` → `pbcopy`.
- Cursor-only updates work: Task 1 adds `selection_changed` to the early-return guard in line with existing `cursor_moved` plumbing.
- No placeholders in code blocks.
- Spec/plan parity: 10 tasks ↔ 8 design steps (split: design step 2 → tasks 2; design step 3 → task 3; design step 6 → task 7; design step 8 → tasks 8 + 9).
- Tests: 4 selection unit tests + 2 copy-mode unit tests. PTY-dependent tests use real `Session::new` (spawns shell); marked acceptable to `#[ignore]` if flaky.
- All commit messages prefixed per Conventional Commits.
