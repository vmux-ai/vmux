# Terminal tab title from OSC — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make terminal tab titles update reactively from OSC 0/2 escape sequences emitted by the running shell/program, by routing them through the dioxus terminal app and CEF chrome state (treating the terminal webview like any other web page).

**Architecture:** alacritty's `TermEvent::Title` (already parsed from OSC) is forwarded by `ServiceEventProxy` over the existing `broadcast::Sender<ServiceMessage>` as a new `ServiceMessage::ProcessTitle` variant. The desktop emits a `TermTitleEvent` to the matching terminal webview via `BinHostEmitEvent`. The dioxus app's listener calls `document.set_title`. CEF surfaces the title back via `WebviewChromeStateReceiver`, and `apply_chrome_state_from_cef` writes it into `PageMetadata.title` (the existing `vmux://` URL gate is relaxed for title — URL stays gated to preserve the recent VMX-109 fix).

**Tech Stack:** Rust, alacritty_terminal, tokio broadcast channels, rkyv (binary IPC framing), Bevy ECS (vmux_desktop), Dioxus + wasm-bindgen + web-sys (vmux_terminal), bevy_cef.

**Spec:** `docs/specs/2026-05-11-terminal-title-from-osc-design.md`

---

## File Map

| File | Change | Responsibility |
|---|---|---|
| `crates/vmux_terminal/src/event.rs` | modify | Add `TERM_TITLE_EVENT` const + `TermTitleEvent` struct (rkyv-serializable wire format between native and dioxus webview) |
| `crates/vmux_service/src/protocol.rs` | modify | Add `ServiceMessage::ProcessTitle { process_id, title }` variant |
| `crates/vmux_service/src/process.rs` | modify | Extend `ServiceEventProxy` with `process_id` + `patch_tx`; match `TermEvent::Title` and broadcast `ProcessTitle`; flip construction order so `patch_tx` exists before the proxy |
| `crates/vmux_desktop/src/terminal.rs` | modify | New arm in service-message loop (~line 802 after `ViewportPatch`) routes `ProcessTitle` to the matching entity and emits `TermTitleEvent` via `BinHostEmitEvent` |
| `crates/vmux_terminal/src/app.rs` | modify | New `use_bin_event_listener::<TermTitleEvent>` listener calls `web_sys::window().document().set_title(...)` |
| `crates/vmux_layout/src/chrome.rs` | modify | Split the `vmux://` gate in `apply_chrome_state_from_cef`: keep gate for URL, drop gate for title |

Each file has one clear responsibility. Tasks 1–6 below produce one commit per file.

---

## Pre-commit checks (every commit)

After each task's commit step, run the changed-crate fmt + clippy + test loop from `AGENTS.md`. The crate set per task is listed in the commit step. Do not push or open a PR if any check fails.

---

## Task 1: Add `TermTitleEvent` wire format

**Files:**
- Modify: `crates/vmux_terminal/src/event.rs`

- [ ] **Step 1: Write the failing rkyv round-trip test**

Append to the existing `#[cfg(test)] mod tests { ... }` block in `crates/vmux_terminal/src/event.rs`:

```rust
    #[test]
    fn term_title_event_rkyv_roundtrip() {
        let original = TermTitleEvent {
            title: "hello-osc".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered = rkyv::from_bytes::<TermTitleEvent, rkyv::rancor::Error>(&bytes)
            .expect("deserialize");
        assert_eq!(original.title, recovered.title);
    }
```

- [ ] **Step 2: Run test, verify it fails (type does not exist)**

```bash
env -u CEF_PATH cargo test -p vmux_terminal event::tests::term_title_event_rkyv_roundtrip
```

Expected: compile error — `cannot find type 'TermTitleEvent' in this scope`.

- [ ] **Step 3: Add the const and struct**

Add near the existing event constants at the top of `crates/vmux_terminal/src/event.rs` (just below `TERM_THEME_EVENT`):

```rust
pub const TERM_TITLE_EVENT: &str = "term_title";
```

Add the struct after the `TermResizeEvent` definition (place it near the other small `Term*Event` structs, before `#[cfg(test)] mod tests`):

```rust
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TermTitleEvent {
    pub title: String,
}
```

- [ ] **Step 4: Run test, verify it passes**

```bash
env -u CEF_PATH cargo test -p vmux_terminal event::tests::term_title_event_rkyv_roundtrip
```

Expected: `1 passed`.

- [ ] **Step 5: Run pre-commit checks for `vmux_terminal`**

```bash
cargo fmt -p vmux_terminal -- --check
env -u CEF_PATH cargo clippy -p vmux_terminal --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_terminal
```

All three must pass.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_terminal/src/event.rs
git commit -m "feat(VMX-109): add TermTitleEvent wire format"
```

---

## Task 2: Add `ServiceMessage::ProcessTitle` variant

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs`

This task has no test of its own — it adds a protocol variant that Task 3 immediately exercises. The variant is additive; existing exhaustive matches in callers will be extended in Task 4.

- [ ] **Step 1: Add the variant**

In `crates/vmux_service/src/protocol.rs`, inside the `pub enum ServiceMessage { ... }` declaration (the one starting at line 354), add a new variant after `ProcessExited`:

```rust
    ProcessTitle {
        process_id: ProcessId,
        title: String,
    },
```

- [ ] **Step 2: Verify the crate still compiles**

```bash
env -u CEF_PATH cargo build -p vmux_service
```

Expected: clean build. Some downstream crates (vmux_desktop) will start emitting `non_exhaustive_patterns` warnings if they use exhaustive matches on `ServiceMessage`. That is expected — Task 4 fixes the desktop side. Do NOT silence the warnings here.

- [ ] **Step 3: Run pre-commit checks for `vmux_service`**

```bash
cargo fmt -p vmux_service -- --check
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_service
```

If clippy on `vmux_service` itself stays warning-free (it should — no consumer of `ServiceMessage` lives inside `vmux_service` other than constructors), proceed.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_service/src/protocol.rs
git commit -m "feat(VMX-109): add ServiceMessage::ProcessTitle variant"
```

---

## Task 3: Forward `TermEvent::Title` from `ServiceEventProxy`

**Files:**
- Modify: `crates/vmux_service/src/process.rs`

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `crates/vmux_service/src/process.rs` (before the closing `}` of the test module):

```rust
    #[test]
    fn proxy_broadcasts_process_title_on_term_title_event() {
        use std::io;

        let (tx, mut rx) = broadcast::channel::<ServiceMessage>(8);
        let writer: PtyInputWriter = Arc::new(Mutex::new(Box::new(io::sink())));
        let process_id = ProcessId::new();
        let proxy = ServiceEventProxy {
            process_id,
            pty_writer: writer,
            patch_tx: tx,
        };

        proxy.send_event(TermEvent::Title("hello-osc".into()));

        let msg = rx.try_recv().expect("ProcessTitle should be broadcast");
        match msg {
            ServiceMessage::ProcessTitle {
                process_id: got_id,
                title,
            } => {
                assert_eq!(got_id, process_id);
                assert_eq!(title, "hello-osc");
            }
            other => panic!("expected ProcessTitle, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run test, verify it fails**

```bash
env -u CEF_PATH cargo test -p vmux_service process::tests::proxy_broadcasts_process_title_on_term_title_event
```

Expected: compile error — `ServiceEventProxy` has no fields `process_id` or `patch_tx`.

- [ ] **Step 3: Extend `ServiceEventProxy` and handle `Title`**

In `crates/vmux_service/src/process.rs`, replace the existing `ServiceEventProxy` definition and impl (around line 25–38):

```rust
#[derive(Clone)]
struct ServiceEventProxy {
    process_id: ProcessId,
    pty_writer: PtyInputWriter,
    patch_tx: broadcast::Sender<ServiceMessage>,
}

impl TermEventListener for ServiceEventProxy {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::PtyWrite(text) => {
                if let Ok(mut writer) = self.pty_writer.lock() {
                    let _ = writer.write_all(text.as_bytes());
                }
            }
            TermEvent::Title(title) => {
                let _ = self.patch_tx.send(ServiceMessage::ProcessTitle {
                    process_id: self.process_id,
                    title,
                });
            }
            _ => {}
        }
    }
}
```

`ResetTitle` is intentionally not handled in v1 (see spec — out of scope; remains the existing bootstrap title until the next `Title(...)` arrives).

- [ ] **Step 4: Flip construction order in `Process::new` (or `new_with_wake`)**

Find the proxy construction site (around line 265 of `crates/vmux_service/src/process.rs`). Currently it is:

```rust
let event_proxy = ServiceEventProxy {
    pty_writer: Arc::clone(&writer),
};
let dims = PtyDimensions { cols, rows };
let term = Term::new(TermConfig::default(), &dims, event_proxy);
let (patch_tx, _) = broadcast::channel(256);
```

Change the order so `patch_tx` is created first, then the proxy is constructed with the new fields:

```rust
let (patch_tx, _) = broadcast::channel(256);
let event_proxy = ServiceEventProxy {
    process_id: id,
    pty_writer: Arc::clone(&writer),
    patch_tx: patch_tx.clone(),
};
let dims = PtyDimensions { cols, rows };
let term = Term::new(TermConfig::default(), &dims, event_proxy);
```

The `id` binding is the `ProcessId` already in scope (used a few lines later when populating `Self { id, ... }`). The `Self { ..., patch_tx, ... }` initialization at the end of `Process::new` continues to work because `patch_tx` is still in scope and `Sender::clone` is cheap.

- [ ] **Step 5: Run test, verify it passes**

```bash
env -u CEF_PATH cargo test -p vmux_service process::tests::proxy_broadcasts_process_title_on_term_title_event
```

Expected: `1 passed`.

- [ ] **Step 6: Run pre-commit checks for `vmux_service`**

```bash
cargo fmt -p vmux_service -- --check
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_service
```

All three must pass.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_service/src/process.rs
git commit -m "feat(VMX-109): broadcast ProcessTitle from ServiceEventProxy"
```

---

## Task 4: Route `ProcessTitle` to terminal webview in desktop

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Add the imports**

In `crates/vmux_desktop/src/terminal.rs`, locate the existing `use vmux_terminal::event::*;` (or equivalent imports of `TERM_VIEWPORT_EVENT`/`TermViewportPatch`). Confirm `TERM_TITLE_EVENT` and `TermTitleEvent` are reachable via the existing `vmux_terminal::event::*` glob — if the file uses explicit imports instead of a glob, add:

```rust
use vmux_terminal::event::{TERM_TITLE_EVENT, TermTitleEvent};
```

- [ ] **Step 2: Add the `ProcessTitle` arm in the service-message loop**

In `crates/vmux_desktop/src/terminal.rs`, find the `for msg in service.0.drain() { match msg { ... } }` block (starts around line 745). Add a new arm after the `ViewportPatch` arm (which ends around line 802):

```rust
            ServiceMessage::ProcessTitle { process_id, title } => {
                for (entity, handle, _) in &terminals {
                    if handle.process_id == process_id {
                        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                            break;
                        }
                        let evt = TermTitleEvent { title };
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            TERM_TITLE_EVENT,
                            &evt,
                        ));
                        break;
                    }
                }
            }
```

The `break` on the not-ready branch matches the spec decision: drop on the floor if the webview is not yet ready in v1. The next `Title` from the shell will succeed.

- [ ] **Step 3: Build and check the matcher is exhaustive**

```bash
env -u CEF_PATH cargo build -p vmux_desktop
```

Expected: clean build. If `non_exhaustive_patterns` warnings appear elsewhere on `ServiceMessage`, they belong to other matchers — investigate before suppressing.

- [ ] **Step 4: Run pre-commit checks for `vmux_desktop`**

```bash
cargo fmt -p vmux_desktop -- --check
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_desktop
```

All three must pass.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/terminal.rs
git commit -m "feat(VMX-109): route ProcessTitle to terminal webview"
```

---

## Task 5: Set `document.title` in the dioxus terminal app

**Files:**
- Modify: `crates/vmux_terminal/src/app.rs`

- [ ] **Step 1: Add the listener inside `App`**

In `crates/vmux_terminal/src/app.rs`, locate the existing `_theme_listener` block (around line 108). Add a new listener immediately after it:

```rust
    let _title_listener =
        use_bin_event_listener::<TermTitleEvent, _>(TERM_TITLE_EVENT, move |evt| {
            if let Some(window) = web_sys::window()
                && let Some(doc) = window.document()
            {
                doc.set_title(&evt.title);
            }
        });
```

The closure does not capture any reactive signals — no need for `let mut foo = use_signal(...)` plumbing. `TermTitleEvent` and `TERM_TITLE_EVENT` are already in scope via the existing `use vmux_terminal::event::*;` at the top of the file.

- [ ] **Step 2: Verify wasm build still compiles**

```bash
cd crates/vmux_terminal && env -u CEF_PATH cargo check --target wasm32-unknown-unknown
```

Expected: clean check.

- [ ] **Step 3: Run pre-commit checks for `vmux_terminal`**

(Note: `vmux_terminal` is host-runnable for unit tests via the existing `#[cfg(test)]` blocks — those test the host-side event types. The wasm build is checked above.)

```bash
cargo fmt -p vmux_terminal -- --check
env -u CEF_PATH cargo clippy -p vmux_terminal --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_terminal
```

All three must pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_terminal/src/app.rs
git commit -m "feat(VMX-109): set document.title from TermTitleEvent"
```

---

## Task 6: Let CEF title flow into `PageMetadata` for `vmux://` URLs

**Files:**
- Modify: `crates/vmux_layout/src/chrome.rs`

- [ ] **Step 1: Relax the gate**

In `crates/vmux_layout/src/chrome.rs`, replace the body of `apply_chrome_state_from_cef` (lines 41–65) with:

```rust
pub fn apply_chrome_state_from_cef(
    chrome_rx: Res<WebviewChromeStateReceiver>,
    mut browser_meta: Query<&mut vmux_core::PageMetadata>,
) {
    while let Ok(ev) = chrome_rx.0.try_recv() {
        let Ok(mut meta) = browser_meta.get_mut(ev.webview) else {
            continue;
        };
        let url_owned_by_native_view = meta.url.starts_with("vmux://");
        if let Some(url) = ev.url
            && !url_owned_by_native_view
        {
            meta.url = url;
            meta.favicon_url.clear();
        }
        if let Some(title) = ev.title {
            meta.title = title;
        }
        if let Some(favicon) = ev.favicon_url {
            meta.favicon_url = favicon;
        }
    }
}
```

The change is small but semantically important: title and favicon are no longer suppressed for `vmux://` URLs; the URL itself stays gated to preserve the VMX-109 fix that prevented CEF from overwriting `vmux://terminal/<pid>` and `vmux://vibe/<session>`.

- [ ] **Step 2: Build and check**

```bash
env -u CEF_PATH cargo build -p vmux_layout
```

Expected: clean build.

- [ ] **Step 3: Run pre-commit checks for `vmux_layout`**

```bash
cargo fmt -p vmux_layout -- --check
env -u CEF_PATH cargo clippy -p vmux_layout --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_layout
```

All three must pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/src/chrome.rs
git commit -m "fix(VMX-109): allow CEF title updates for vmux:// URLs"
```

---

## Task 7: Manual integration verification

No code changes. This task gates whether the work is shippable.

- [ ] **Step 1: Build the desktop app**

```bash
cargo build -p vmux_desktop
```

- [ ] **Step 2: Launch vmux**

```bash
cargo run -p vmux_desktop
```

- [ ] **Step 3: Verify title updates from common shells**

In a fresh terminal pane (default shell, e.g. `nu` or `zsh`):
- Confirm the tab title initially reads `Terminal (xxxxxxxx)` (bootstrap value).
- Run `printf '\e]0;custom-osc-title\a'` — the tab title MUST update to `custom-osc-title`.
- Run `printf '\e]2;another-title\a'` (OSC 2 — same family) — the tab title MUST update to `another-title`.

- [ ] **Step 4: Verify shell-driven titles**

In a `zsh` or `bash` shell with default config:
- Type `vim` (or any common interactive program) and confirm the tab title reflects the running program (depends on shell config; if `precmd`/`preexec` set titles, you should see them change).

- [ ] **Step 5: Regression — `vmux://` URL preservation**

After the title updates, inspect the tab's URL field (e.g. via address bar or by clicking on the tab) and confirm it still reads `vmux://terminal/<pid>` (or `vmux://vibe/<session>`). The URL must NOT have been overwritten by CEF.

- [ ] **Step 6: Regression — browser tabs**

Open a new browser tab to `https://example.com`. Title should still read "Example Domain" as before. Navigate to another page and confirm titles update normally.

- [ ] **Step 7: Open PR**

If all manual checks pass, open a PR from this branch using the `open-new-pr` skill (or per the project's PR workflow in AGENTS.md).

---

## Self-Review Notes

- **Spec coverage:** Title-only behavior, OSC source, dioxus app sets `document.title`, CEF-back flow, `vmux://` gate split, "Terminal (xxxx)" bootstrap kept, `ResetTitle` not handled, drop-on-floor when webview not ready, `RestartPty` unchanged → all addressed across Tasks 1–6 and verified in Task 7.
- **Type consistency:** `TermTitleEvent { title: String }` is referenced identically in Tasks 1, 3, 4, 5. `ServiceMessage::ProcessTitle { process_id, title }` is referenced identically in Tasks 2, 3, 4. `TERM_TITLE_EVENT` const used in Tasks 1, 4, 5.
- **No placeholders:** All steps contain executable commands and complete code blocks.
- **TDD ordering:** Tasks 1 and 3 begin with failing tests; Tasks 2, 4, 5, 6 are wiring whose verification belongs to the manual integration in Task 7 (no host-side test harness available for the dioxus app + CEF + tab bar pipeline). This is acknowledged explicitly in Task 7 rather than fabricating shallow tests for the wiring.
