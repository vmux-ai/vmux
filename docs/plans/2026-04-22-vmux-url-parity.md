# vmux:// URL Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `vmux://*` URLs first-class citizens -- terminal tabs participate in all browser systems (palette, metadata, history, zoom, keyboard focus).

**Architecture:** Add `Browser` marker to Terminal entities so all `With<Browser>` queries include them. Gate browser-only CEF navigation systems with `Without<Terminal>`. Add session URLs. Reload on terminal = PTY restart.

**Tech Stack:** Bevy ECS, portable-pty 0.9, bevy_cef

---

### Task 1: Add Browser marker and session URL to Terminal spawn

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs:91-200`

- [ ] **Step 1: Add Browser import**

In `terminal.rs`, add `Browser` to imports:

```rust
use crate::{
    browser::Browser,
    layout::window::WEBVIEW_MESH_DEPTH_BIAS,
    settings::AppSettings,
};
```

- [ ] **Step 2: Add Browser marker to Terminal::new() bundle**

In `Terminal::new()`, add `Browser` to the first tuple of the returned bundle (after `Self`):

```rust
        (
            (
                Self,
                Browser,
                TerminalState {
                    term,
                    processor,
                    dirty: true,
                },
```

- [ ] **Step 3: Add session URL with PTY PID**

In `Terminal::new()`, after spawning the child process (~line 122), capture the PID and use it in the PageMetadata URL:

```rust
        let child = pair.slave.spawn_command(cmd).expect("failed to spawn shell");
        let pid = child.process_id().unwrap_or(0);
```

Then update the `PageMetadata` in the bundle:

```rust
                PageMetadata {
                    title: format!("Terminal - {}", shell),
                    url: format!("{}?session={}", TERMINAL_WEBVIEW_URL, pid),
                    favicon_url: String::new(),
                },
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | head -20`

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/terminal.rs
git commit -m "feat: add Browser marker and session URL to Terminal entities"
```

---

### Task 2: Remove ContentFilter type alias

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`

- [ ] **Step 1: Remove the ContentFilter type alias**

Delete line 40-41 in `browser.rs`:

```rust
/// Filter that matches any content entity (Browser or Terminal) inside a tab.
type ContentFilter = Or<(With<Browser>, With<Terminal>)>;
```

- [ ] **Step 2: Replace all ContentFilter usages with With<Browser>**

In `browser.rs`, replace every occurrence of `ContentFilter` with `With<Browser>`:

- `sync_keyboard_target`: `content_q: Query<..., ContentFilter>` -> `With<Browser>`
- `sync_children_to_ui`: `browser_q: Query<..., ContentFilter>` -> `With<Browser>`
- `sync_cef_webview_resize_after_ui`: `webviews: Query<..., ContentFilter>` -> `With<Browser>`
- `sync_webview_pane_corner_clip`: `tabs: Query<..., ContentFilter>` -> `With<Browser>`
- `push_tabs_host_emit`: `browser_q: Query<..., ContentFilter>` -> `With<Browser>`
- `handle_browser_commands`: `browsers: Query<..., (ContentFilter, ...)>` -> `(With<Browser>, ...)`

- [ ] **Step 3: Keep Terminal import (needed for Task 4)**

Keep `use crate::terminal::Terminal;` in browser.rs -- it will be needed for `Without<Terminal>` and `terminal_q` in Task 4.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | head -20`

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/browser.rs
git commit -m "refactor: remove ContentFilter, use With<Browser> (Terminal now has Browser marker)"
```

---

### Task 3: Update command bar URL matching

**Files:**
- Modify: `crates/vmux_desktop/src/command_bar.rs`

- [ ] **Step 1: Update vmux://terminal URL matching to use starts_with**

In `on_command_bar_action`, change the navigate handler from:

```rust
            if url.trim_end_matches('/') == "vmux://terminal" {
```

to:

```rust
            if url.starts_with("vmux://terminal") {
```

This handles `vmux://terminal/`, `vmux://terminal/?session=123`, etc.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | head -20`

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/command_bar.rs
git commit -m "fix: command bar navigate handles vmux://terminal with session params"
```

---

### Task 4: Implement PTY restart and gate navigation commands

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`
- Modify: `crates/vmux_desktop/src/browser.rs`

- [ ] **Step 1: Define RestartPty event in terminal.rs**

Add after the `PtyHandle` struct:

```rust
#[derive(Event)]
pub(crate) struct RestartPty {
    pub entity: Entity,
}
```

- [ ] **Step 2: Implement on_restart_pty observer in terminal.rs**

```rust
fn on_restart_pty(
    trigger: On<RestartPty>,
    mut q: Query<(&mut TerminalState, &mut PtyHandle, &mut PageMetadata)>,
    settings: Res<AppSettings>,
) {
    let entity = trigger.event().entity;
    let Ok((mut state, mut pty, mut meta)) = q.get_mut(entity) else {
        return;
    };

    // Kill old PTY
    let _ = pty.child.lock().unwrap().kill();

    let shell = settings
        .terminal
        .as_ref()
        .map(|t| t.shell.clone())
        .unwrap_or_else(default_shell);

    // Spawn new PTY
    let pty_system = NativePtySystem::default();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("failed to open PTY");

    let mut cmd = CommandBuilder::new(&shell);
    cmd.env("TERM", "xterm-256color");

    let child = pair.slave.spawn_command(cmd).expect("failed to spawn shell");
    let pid = child.process_id().unwrap_or(0);
    let reader = pair
        .master
        .try_clone_reader()
        .expect("failed to clone PTY reader");
    let new_writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(
        pair.master
            .take_writer()
            .expect("failed to take PTY writer"),
    ));
    drop(pair.slave);

    // Spawn new reader thread
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("pty-reader".into())
        .spawn(move || {
            pty_reader_thread(reader, tx);
        })
        .expect("failed to spawn PTY reader thread");

    // Reset terminal state
    let event_proxy = VmuxEventProxy {
        pty_writer: Arc::clone(&new_writer),
    };
    let term_config = TermConfig::default();
    let dims = PtyDimensions { cols: 80, rows: 24 };
    let new_term = Term::new(term_config, &dims, event_proxy);

    state.term = new_term;
    state.processor = Processor::new();
    state.dirty = true;

    // Replace PtyHandle entirely
    *pty = PtyHandle {
        rx: Mutex::new(rx),
        writer: new_writer,
        master: Mutex::new(pair.master),
        child: Mutex::new(child),
    };

    // Update metadata
    meta.url = format!("{}?session={}", TERMINAL_WEBVIEW_URL, pid);
    meta.title = format!("Terminal - {}", shell);
}
```

- [ ] **Step 3: Register the observer in TerminalPlugin::build**

```rust
        app.add_observer(on_restart_pty);
```

- [ ] **Step 4: Update handle_browser_commands in browser.rs**

Add import:

```rust
use crate::terminal::{Terminal, RestartPty};
```

Add `terminal_q` parameter to `handle_browser_commands`:

```rust
    terminal_q: Query<(), With<Terminal>>,
```

Replace the match block with terminal-aware logic:

```rust
        match browser_cmd {
            BrowserCommand::PrevPage => {
                if !terminal_q.contains(webview) {
                    commands.trigger(RequestGoBack { webview });
                }
            }
            BrowserCommand::NextPage => {
                if !terminal_q.contains(webview) {
                    commands.trigger(RequestGoForward { webview });
                }
            }
            BrowserCommand::Reload => {
                if terminal_q.contains(webview) {
                    commands.trigger(RestartPty { entity: webview });
                } else {
                    commands.trigger(RequestReload { webview });
                }
            }
            BrowserCommand::HardReload => {
                if terminal_q.contains(webview) {
                    commands.trigger(RestartPty { entity: webview });
                } else {
                    commands.trigger(RequestReloadIgnoreCache { webview });
                }
            }
            BrowserCommand::Stop => {}
            BrowserCommand::FocusAddressBar => {}
            BrowserCommand::Find => {}
            BrowserCommand::ZoomIn => {
                if let Ok(mut z) = zoom_q.get_mut(webview) {
                    z.0 += 0.5;
                }
            }
            BrowserCommand::ZoomOut => {
                if let Ok(mut z) = zoom_q.get_mut(webview) {
                    z.0 -= 0.5;
                }
            }
            BrowserCommand::ZoomReset => {
                if let Ok(mut z) = zoom_q.get_mut(webview) {
                    z.0 = 0.0;
                }
            }
            BrowserCommand::DevTools => commands.trigger(RequestShowDevTool { webview }),
            BrowserCommand::ViewSource => {}
            BrowserCommand::Print => {}
        }
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | head -20`

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_desktop/src/terminal.rs crates/vmux_desktop/src/browser.rs
git commit -m "feat: PTY restart on Reload, disable Back/Forward on terminal tabs"
```

---

### Task 5: Update persistence URL matching

**Files:**
- Modify: `crates/vmux_desktop/src/persistence.rs`

- [ ] **Step 1: Update URL check to use starts_with**

Change in `persistence.rs` (~line 260):

```rust
            if meta.url.trim_end_matches('/') == TERMINAL_WEBVIEW_URL.trim_end_matches('/') {
```

to:

```rust
            if meta.url.starts_with(TERMINAL_WEBVIEW_URL.trim_end_matches('/')) {
```

This matches `vmux://terminal/?session=12345` as well as `vmux://terminal/`.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | head -20`

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/persistence.rs
git commit -m "fix: persistence matches vmux://terminal URLs with session params"
```

---

### Task 6: Full build verification

- [ ] **Step 1: Run full cargo check**

Run: `cargo check -p vmux_desktop 2>&1 | tail -5`

Expected: No errors.

- [ ] **Step 2: Verify no regressions**

Confirm these queries now automatically include Terminal (no code change needed, they work because Terminal has Browser marker):

- `sync_page_metadata_to_tab` -- `With<Browser>` includes Terminal
- `handle_open_command_bar` `browser_meta` -- `With<Browser>` includes Terminal
- `handle_open_command_bar` `content_browsers` -- `With<Browser>` includes Terminal
- `on_command_bar_action` `content_browsers` -- `With<Browser>` includes Terminal
- `zoom_q` in `handle_browser_commands` -- `With<Browser>` includes Terminal

- [ ] **Step 3: Run the app and manually verify**

Run: `make run-mac`

Manual verification checklist:
1. Open a terminal tab -- appears in command bar tab list
2. Open command bar over terminal, close it -- keyboard focus returns to terminal
3. URL bar shows `vmux://terminal/?session={pid}`
4. Press Reload -- terminal restarts with fresh prompt
5. Press Back/Forward -- no action (arrows should be disabled)
6. Zoom in/out -- text scales
7. Restart app -- terminal tabs restore correctly
8. Open multiple terminals -- history shows distinct entries
