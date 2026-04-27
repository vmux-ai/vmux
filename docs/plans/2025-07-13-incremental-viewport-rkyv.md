# Incremental Viewport Updates with rkyv Serialization

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce terminal input-to-display latency by sending only changed lines via rkyv binary serialization instead of full RON-serialized viewport every frame.

**Architecture:** Add per-line hashing to detect dirty rows. Replace RON text serialization with rkyv binary (base64-encoded for CEF IPC transport). WASM side maintains full viewport state and applies incremental patches. Full sync on resize/spawn.

**Tech Stack:** rkyv 0.8, base64 (Rust), alacritty_terminal 0.26, Dioxus 0.7, bevy_cef

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `Cargo.toml` (workspace) | Modify | Add rkyv + base64 workspace deps |
| `crates/vmux_terminal/Cargo.toml` | Modify | Add rkyv dep |
| `crates/vmux_desktop/Cargo.toml` | Modify | Add rkyv + base64 deps, drop ron |
| `crates/vmux_ui/Cargo.toml` | Modify | Add rkyv + base64 deps (wasm target) |
| `crates/vmux_terminal/src/event.rs` | Modify | Add rkyv derives, add `TermViewportPatch` type |
| `crates/vmux_desktop/src/terminal.rs` | Modify | Line hashing, incremental sync, rkyv+base64 serialize |
| `crates/vmux_ui/src/hooks/event_listener.rs` | Modify | Add rkyv+base64 decode path |
| `crates/vmux_terminal/src/app.rs` | Modify | Maintain full viewport state, apply patches |
| `patches/bevy_cef-0.5.2/src/common/ipc/host_emit.rs` | Modify | Add `new_raw` constructor |

---

### Task 1: Add dependencies

**Files:**
- Modify: `Cargo.toml` (workspace)
- Modify: `crates/vmux_terminal/Cargo.toml`
- Modify: `crates/vmux_desktop/Cargo.toml`
- Modify: `crates/vmux_ui/Cargo.toml`

- [ ] **Step 1: Add rkyv and base64 to workspace dependencies**

In `Cargo.toml` (workspace root), add to `[workspace.dependencies]`:

```toml
rkyv = { version = "0.8", features = ["alloc"] }
base64 = "0.22"
```

- [ ] **Step 2: Add rkyv to vmux_terminal**

In `crates/vmux_terminal/Cargo.toml`, add under `[dependencies]`:

```toml
rkyv = { workspace = true }
```

- [ ] **Step 3: Add rkyv and base64 to vmux_desktop**

In `crates/vmux_desktop/Cargo.toml`, add under `[dependencies]`:

```toml
rkyv = { workspace = true }
base64 = { workspace = true }
```

- [ ] **Step 4: Add rkyv and base64 to vmux_ui (wasm target)**

In `crates/vmux_ui/Cargo.toml`, add under `[target.'cfg(target_arch = "wasm32")'.dependencies]`:

```toml
rkyv = { workspace = true }
base64 = { workspace = true }
```

- [ ] **Step 5: Verify workspace resolves**

Run: `cargo check -p vmux_terminal -p vmux_desktop -p vmux_ui 2>&1 | head -5`
Expected: no dependency resolution errors (compilation errors from unused imports are fine at this stage)

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "chore: add rkyv and base64 workspace dependencies"
```

---

### Task 2: Add rkyv derives and TermViewportPatch to event types

**Files:**
- Modify: `crates/vmux_terminal/src/event.rs`

- [ ] **Step 1: Add rkyv derives to all viewport-related types**

Add `rkyv::Archive, rkyv::Serialize, rkyv::Deserialize` derives to: `TermColor`, `TermSpan`, `TermLine`, `TermCursor`, `CursorShape`, `TermSelectionRange`, `TermViewportEvent`.

The file should look like this after changes (showing only modified type definitions — leave all other code unchanged):

```rust
use serde::{Deserialize, Serialize};

// ... existing constants unchanged ...

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum TermColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

// TermThemeEvent — NO rkyv derives (still uses RON)

#[derive(Debug, Clone, Serialize, Deserialize, Default, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TermViewportEvent {
    pub lines: Vec<TermLine>,
    pub cursor: TermCursor,
    pub cols: u16,
    pub rows: u16,
    pub title: Option<String>,
    #[serde(default)]
    pub selection: Option<TermSelectionRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TermSelectionRange {
    pub start_col: u16,
    pub start_row: u16,
    pub end_col: u16,
    pub end_row: u16,
    pub is_block: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TermLine {
    pub spans: Vec<TermSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TermSpan {
    pub text: String,
    pub fg: TermColor,
    pub bg: TermColor,
    pub flags: u16,
    #[serde(default)]
    pub col: u16,
    #[serde(default)]
    pub grid_cols: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TermCursor {
    pub col: u16,
    pub row: u16,
    pub shape: CursorShape,
    pub visible: bool,
    #[serde(default)]
    pub ch: String,
}

// Default impl for TermCursor — unchanged

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum CursorShape {
    Block,
    Beam,
    Underline,
}
```

- [ ] **Step 2: Add TermViewportPatch type**

Add at the end of `event.rs`, before the `TermResizeEvent` struct:

```rust
/// Incremental viewport update. Contains only changed lines plus cursor/selection.
/// When `full` is true, `changed_lines` contains ALL lines (used on resize/spawn).
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TermViewportPatch {
    /// (row_index, line) pairs for rows that changed since last sync.
    pub changed_lines: Vec<(u16, TermLine)>,
    pub cursor: TermCursor,
    pub cols: u16,
    pub rows: u16,
    pub selection: Option<TermSelectionRange>,
    /// When true, changed_lines contains every row (full viewport rebuild).
    pub full: bool,
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p vmux_terminal`
Expected: compiles successfully

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(terminal): add rkyv derives to viewport types and TermViewportPatch"
```

---

### Task 3: Add HostEmitEvent::new_raw constructor

**Files:**
- Modify: `patches/bevy_cef-0.5.2/src/common/ipc/host_emit.rs`

- [ ] **Step 1: Add new_raw method**

Add a second constructor to `HostEmitEvent` that accepts a pre-serialized payload string (bypasses `serde_json::to_string` wrapping):

```rust
impl HostEmitEvent {
    /// Creates a new `HostEmitEvent` with the given id and payload.
    pub fn new(webview: Entity, id: impl Into<String>, payload: &impl Serialize) -> Self {
        Self {
            webview,
            id: id.into(),
            payload: serde_json::to_string(payload).unwrap_or_default(),
        }
    }

    /// Creates a new `HostEmitEvent` with a pre-encoded payload string.
    /// The payload is sent as-is through CEF IPC without additional JSON wrapping.
    /// Use this for binary-encoded payloads (e.g. base64-encoded rkyv bytes).
    pub fn new_raw(webview: Entity, id: impl Into<String>, raw_payload: String) -> Self {
        Self {
            webview,
            id: id.into(),
            payload: raw_payload,
        }
    }
}
```

Note: `emit_event_raw_json` in bevy_cef_core sends the payload string directly into a CEF ProcessMessage. When the payload is a JSON-encoded string (from `new`), the JS side receives a JSON value. When using `new_raw`, the payload must be a valid JSON value — a quoted string like `"\"base64data\""` works.

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p bevy_cef`
Expected: compiles successfully

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat(bevy_cef): add HostEmitEvent::new_raw for pre-encoded payloads"
```

---

### Task 4: Implement dirty-line tracking and incremental sync on the native side

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Add imports**

At the top of `terminal.rs`, add:

```rust
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rkyv;
```

- [ ] **Step 2: Extend TerminalState with line tracking fields**

Change the `TerminalState` struct (around line 48) from:

```rust
pub(crate) struct TerminalState {
    term: Term<VmuxEventProxy>,
    processor: Processor,
    dirty: bool,
    /// Moving end of keyboard-driven selection (distinct from anchor).
    /// Tracked separately because `Selection::to_range()` normalizes order,
    /// making it impossible to know which end the user is extending.
    selection_cursor: Option<Point>,
}
```

to:

```rust
pub(crate) struct TerminalState {
    term: Term<VmuxEventProxy>,
    processor: Processor,
    dirty: bool,
    /// Moving end of keyboard-driven selection (distinct from anchor).
    /// Tracked separately because `Selection::to_range()` normalizes order,
    /// making it impossible to know which end the user is extending.
    selection_cursor: Option<Point>,
    /// Per-row hash of the last synced viewport. Used to detect which lines
    /// changed and need to be re-sent to the webview.
    line_hashes: Vec<u64>,
    /// True when the entire viewport must be re-sent (resize, first frame).
    full_sync_needed: bool,
}
```

- [ ] **Step 3: Update TerminalState initialization**

Find where `TerminalState` is constructed (the `Terminal::new` or spawn site). It should be around where `TerminalState { term, processor, dirty: true, selection_cursor: None }` is written. Add the new fields:

```rust
TerminalState {
    term,
    processor,
    dirty: true,
    selection_cursor: None,
    line_hashes: Vec::new(),
    full_sync_needed: true,
}
```

- [ ] **Step 4: Add line hashing function**

Add this function somewhere near `build_viewport` (around line 460):

```rust
/// Compute a fast hash of a single grid row's visible content.
/// Hashes character, fg, bg, and flags for each cell.
fn hash_grid_row<T: TermEventListener>(term: &Term<T>, row_idx: usize, offset: i32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let grid = term.grid();
    let num_cols = grid.columns();
    let row = &grid[Line(row_idx as i32 - offset)];
    for col_idx in 0..num_cols {
        let cell = &row[Column(col_idx)];
        cell.c.hash(&mut hasher);
        std::mem::discriminant(&cell.fg).hash(&mut hasher);
        match &cell.fg {
            Color::Named(c) => (*c as u8).hash(&mut hasher),
            Color::Spec(rgb) => { rgb.r.hash(&mut hasher); rgb.g.hash(&mut hasher); rgb.b.hash(&mut hasher); },
            Color::Indexed(i) => i.hash(&mut hasher),
        }
        std::mem::discriminant(&cell.bg).hash(&mut hasher);
        match &cell.bg {
            Color::Named(c) => (*c as u8).hash(&mut hasher),
            Color::Spec(rgb) => { rgb.r.hash(&mut hasher); rgb.g.hash(&mut hasher); rgb.b.hash(&mut hasher); },
            Color::Indexed(i) => i.hash(&mut hasher),
        }
        cell.flags.bits().hash(&mut hasher);
    }
    hasher.finish()
}
```

- [ ] **Step 5: Add function to build a single line**

Add this function near `build_viewport`:

```rust
/// Build a TermLine for a single grid row, using the same span-coalescing
/// logic as build_viewport.
fn build_line<T: TermEventListener>(term: &Term<T>, row_idx: usize, offset: i32) -> TermLine {
    let grid = term.grid();
    let num_cols = grid.columns();
    let row = &grid[Line(row_idx as i32 - offset)];
    let mut spans = Vec::new();
    let mut text = String::new();
    let mut cur_fg: TermColor = TermColor::Default;
    let mut cur_bg: TermColor = TermColor::Default;
    let mut cur_flags: u16 = 0;
    let mut span_col_start: u16 = 0;
    let mut span_grid_cols: u16 = 0;

    for col_idx in 0..num_cols {
        let cell = &row[Column(col_idx)];

        if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
            span_grid_cols += 1;
            continue;
        }

        let fg = color_to_term_color(&cell.fg);
        let bg = color_to_term_color(&cell.bg);
        let flags = cell_flags_to_u16(cell.flags);

        if fg != cur_fg || bg != cur_bg || flags != cur_flags {
            if !text.is_empty() {
                spans.push(TermSpan {
                    text: std::mem::take(&mut text),
                    fg: cur_fg,
                    bg: cur_bg,
                    flags: cur_flags,
                    col: span_col_start,
                    grid_cols: span_grid_cols,
                });
                span_col_start = col_idx as u16;
                span_grid_cols = 0;
            }
            cur_fg = fg;
            cur_bg = bg;
            cur_flags = flags;
        }
        text.push(cell.c);
        span_grid_cols += 1;
    }
    if !text.is_empty() {
        spans.push(TermSpan {
            text,
            fg: cur_fg,
            bg: cur_bg,
            flags: cur_flags,
            col: span_col_start,
            grid_cols: span_grid_cols,
        });
    }

    TermLine { spans }
}
```

- [ ] **Step 6: Replace sync_terminal_viewport with incremental version**

Replace the `sync_terminal_viewport` function (starting around line 341) with:

```rust
/// Serialize visible viewport diff and send to webview via rkyv + base64.
fn sync_terminal_viewport(
    mut q: Query<(Entity, &mut TerminalState), With<Terminal>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, mut state) in &mut q {
        if !state.dirty {
            continue;
        }
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        state.dirty = false;

        let grid = state.term.grid();
        let num_lines = grid.screen_lines();
        let num_cols = grid.columns();
        let offset = grid.display_offset() as i32;

        let full = state.full_sync_needed
            || state.line_hashes.len() != num_lines;

        // Resize hash cache if needed.
        if state.line_hashes.len() != num_lines {
            state.line_hashes.resize(num_lines, 0);
        }

        let mut changed_lines = Vec::new();

        for row_idx in 0..num_lines {
            let hash = hash_grid_row(&state.term, row_idx, offset);
            if full || hash != state.line_hashes[row_idx] {
                state.line_hashes[row_idx] = hash;
                changed_lines.push((row_idx as u16, build_line(&state.term, row_idx, offset)));
            }
        }

        state.full_sync_needed = false;

        // Build cursor.
        let cursor_point = grid.cursor.point;
        let scrolled_back = offset > 0;
        let cursor_char = {
            let cursor_row = &grid[cursor_point.line];
            let cell = &cursor_row[cursor_point.column];
            cell.c.to_string()
        };

        // Convert alacritty selection to viewport-relative coordinates.
        let selection = state
            .term
            .selection
            .as_ref()
            .and_then(|sel| sel.to_range(&state.term))
            .map(|range| {
                let start_row = (range.start.line.0 + offset) as u16;
                let end_row = (range.end.line.0 + offset) as u16;
                TermSelectionRange {
                    start_col: range.start.column.0 as u16,
                    start_row,
                    end_col: range.end.column.0 as u16,
                    end_row,
                    is_block: range.is_block,
                }
            });

        let patch = TermViewportPatch {
            changed_lines,
            cursor: TermCursor {
                col: cursor_point.column.0 as u16,
                row: cursor_point.line.0 as u16,
                shape: CursorShape::Block,
                visible: !scrolled_back,
                ch: cursor_char,
            },
            cols: num_cols as u16,
            rows: num_lines as u16,
            selection,
            full,
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&patch).unwrap();
        let b64 = BASE64.encode(&bytes);
        // Wrap in JSON string so emit_event_raw_json delivers it correctly.
        let json_payload = serde_json::to_string(&b64).unwrap_or_default();
        commands.trigger(HostEmitEvent::new_raw(entity, TERM_VIEWPORT_EVENT, json_payload));
    }
}
```

- [ ] **Step 7: Set full_sync_needed on resize**

In `on_term_resize` (around line 1128), after `state.term.resize(dims)`, add:

```rust
state.full_sync_needed = true;
```

So the block becomes:

```rust
    if let Ok(mut state) = state_q.get_mut(entity) {
        let dims = PtyDimensions { cols, rows };
        state.term.resize(dims);
        state.dirty = true;
        state.full_sync_needed = true;
    }
```

- [ ] **Step 8: Remove the old build_viewport function**

Delete the `build_viewport` function (approximately lines 361-459) since it is fully replaced by `build_line` + inline cursor/selection building in `sync_terminal_viewport`.

- [ ] **Step 9: Verify compilation**

Run: `cargo check -p vmux_desktop`
Expected: compiles successfully (may warn about unused `ron` import which we'll clean up)

- [ ] **Step 10: Remove unused ron import from terminal.rs**

If `ron` is no longer used in `terminal.rs` for the viewport path, check if it's still needed for the theme sync. The theme sync at line 1189 still uses `ron::ser::to_string(&event)` for `TermThemeEvent`, so `ron` stays in the desktop crate dependencies but may need its import verified.

- [ ] **Step 11: Commit**

```bash
git add -A && git commit -m "feat(terminal): dirty-line tracking + incremental rkyv viewport sync"
```

---

### Task 5: Add rkyv+base64 decode path on the WASM side

**Files:**
- Modify: `crates/vmux_ui/src/hooks/event_listener.rs`

- [ ] **Step 1: Add rkyv+base64 decode function**

Add imports and a new decode function to `event_listener.rs`:

```rust
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
```

Add this function after `decode_host_emit_js`:

```rust
/// Decode a base64-encoded rkyv payload from a CEF host-emit event.
/// The JS value is expected to be a string containing base64-encoded rkyv bytes.
pub fn decode_rkyv_host_emit<T>(e: &JsValue) -> Option<T>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::de::Pool>,
{
    let s = e.as_string()?;
    let bytes = BASE64.decode(s.as_bytes()).ok()?;
    rkyv::from_bytes::<T, rkyv::rancor::Error>(&bytes).ok()
}
```

- [ ] **Step 2: Add a variant of use_event_listener for rkyv**

Add a new public function after `use_event_listener`:

```rust
/// Like [`use_event_listener`] but decodes the payload using rkyv + base64
/// instead of RON/JSON serde.
pub fn use_rkyv_event_listener<T, F>(name: &'static str, on_event: F) -> BevyState
where
    T: rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::de::Pool>,
    F: FnMut(T) + 'static,
{
    let on_event = Rc::new(RefCell::new(on_event));
    let mut is_loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    use_hook(move || {
        let on_event = Rc::clone(&on_event);
        let Some(rt) = Runtime::try_current() else {
            is_loading.set(false);
            error.set(Some(
                "use_rkyv_event_listener: no Dioxus runtime (internal error)".into(),
            ));
            return;
        };
        let scope = current_scope_id();

        let result = (|| -> Result<(), EventListenerError> {
            let cef = window_cef()?;
            let Ok(listen) = js_sys::Reflect::get(&cef, &JsValue::from_str("listen")) else {
                return Err(EventListenerError::NoListenMethod);
            };
            let Ok(listen_fn) = listen.dyn_into::<Function>() else {
                return Err(EventListenerError::ListenNotCallable);
            };

            let on_event_inner = Rc::clone(&on_event);
            let closure = Closure::wrap(Box::new(move |e: JsValue| {
                if let Some(msg) = decode_rkyv_host_emit::<T>(&e) {
                    let on_event = Rc::clone(&on_event_inner);
                    rt.in_scope(scope, || {
                        on_event.borrow_mut()(msg);
                    });
                }
            }) as Box<dyn FnMut(JsValue)>);

            let cb = closure.as_ref().unchecked_ref();
            let _ = listen_fn.call2(&cef, &JsValue::from_str(name), cb);
            closure.forget();
            Ok(())
        })();

        match result {
            Ok(()) => {
                is_loading.set(false);
                match try_emit_ui_ready() {
                    Ok(()) => {}
                    Err(e) => error.set(Some(format!("cef.emit failed: {e}"))),
                }
            }
            Err(e) => {
                is_loading.set(false);
                error.set(Some(format!("cef.listen failed: {e}")));
            }
        }
    });

    BevyState { is_loading, error }
}
```

- [ ] **Step 3: Export from hooks module**

Ensure `use_rkyv_event_listener` and `decode_rkyv_host_emit` are accessible. Check `crates/vmux_ui/src/hooks/mod.rs` and add the re-export if needed.

- [ ] **Step 4: Verify compilation (wasm target)**

Run: `cargo check -p vmux_ui --target wasm32-unknown-unknown`
Expected: compiles successfully

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(vmux_ui): add rkyv+base64 event listener for viewport patches"
```

---

### Task 6: Update WASM terminal app to apply incremental patches

**Files:**
- Modify: `crates/vmux_terminal/src/app.rs`

- [ ] **Step 1: Add import for the new event listener and patch type**

Replace:
```rust
use vmux_ui::hooks::{use_event_listener, use_theme};
```

with:
```rust
use vmux_ui::hooks::{use_rkyv_event_listener, use_event_listener, use_theme};
```

Add import for the patch type:
```rust
use vmux_terminal::event::TermViewportPatch;
```

- [ ] **Step 2: Change viewport signal to store full state built from patches**

In the `App` component, replace:

```rust
    let mut viewport = use_signal(TermViewportEvent::default);
```

with:

```rust
    let mut viewport = use_signal(TermViewportEvent::default);
```

(The signal type stays `TermViewportEvent` — we accumulate patches into it.)

Replace the viewport event listener:

```rust
    let _listener = use_event_listener::<TermViewportEvent, _>(TERM_VIEWPORT_EVENT, move |data| {
        viewport.set(data);
    });
```

with:

```rust
    let _listener = use_rkyv_event_listener::<TermViewportPatch, _>(TERM_VIEWPORT_EVENT, move |patch| {
        viewport.with_mut(|vp| {
            // On full sync or dimension change, rebuild entire viewport.
            if patch.full || vp.cols != patch.cols || vp.rows != patch.rows {
                vp.lines.clear();
                vp.lines.resize(patch.rows as usize, TermLine::default());
            }

            // Ensure lines vec is large enough.
            if vp.lines.len() < patch.rows as usize {
                vp.lines.resize(patch.rows as usize, TermLine::default());
            }

            // Apply changed lines.
            for (row_idx, line) in patch.changed_lines {
                let idx = row_idx as usize;
                if idx < vp.lines.len() {
                    vp.lines[idx] = line;
                }
            }

            vp.cursor = patch.cursor;
            vp.cols = patch.cols;
            vp.rows = patch.rows;
            vp.selection = patch.selection;
        });
    });
```

- [ ] **Step 3: Verify the rest of the rendering code still works**

The rendering code reads `vp.lines`, `vp.cursor`, `vp.selection`, `vp.cols` — all of which are still populated. No other changes needed in the rendering section.

- [ ] **Step 4: Build the WASM binary**

Run: `cargo build -p vmux_terminal --target wasm32-unknown-unknown`
Expected: compiles successfully

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(terminal): apply incremental viewport patches on WASM side"
```

---

### Task 7: Full build verification

- [ ] **Step 1: Build native (desktop)**

Run: `cargo build -p vmux_desktop`
Expected: compiles successfully

- [ ] **Step 2: Build WASM (terminal webview)**

Run: `cargo build -p vmux_terminal --target wasm32-unknown-unknown`
Expected: compiles successfully

- [ ] **Step 3: Build all WASM crates**

Run: `cargo build -p vmux_ui --target wasm32-unknown-unknown`
Expected: compiles successfully

- [ ] **Step 4: Run the app and test**

Launch the app manually and verify:
1. Terminal renders correctly on first load (full sync)
2. Typing characters shows immediate echo
3. Running `ls -la` or similar fills the screen correctly
4. Resizing the window re-renders the full viewport
5. Scrolling through output works
6. Text selection works
7. Copy/paste works

- [ ] **Step 5: Commit (if any final fixes were needed)**

```bash
git add -A && git commit -m "fix: final adjustments for incremental viewport"
```
