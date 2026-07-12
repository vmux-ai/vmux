# Windowed Focus Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore keyboard focus automatically when any active windowed CEF page, including an auto-spawned `vmux://start/`, loses macOS first-responder ownership after replacing a closed page.

**Architecture:** Add native-focus inspection to the patched CEF browser registry, then make vmux's host-focus system reassert focus only when inspection shows that the active browser lost it. Preserve the existing entity-cache fallback on platforms where native inspection is unavailable.

**Tech Stack:** Rust, Bevy ECS, patched `bevy_cef_core`, CEF windowed browsers, objc2/AppKit.

---

### Task 1: Inspect native first-responder ownership

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs:580-610`
- Test: `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs:2430-2470`

- [ ] **Step 1: Write the failing source-structure test**

Add this test beside the existing windowed native-focus tests:

```rust
#[test]
fn windowed_native_focus_detects_first_responder_subtree() {
    let implementation = include_str!("browsers.rs")
        .split("#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or_default();
    let focus_fn = implementation
        .split("pub fn windowed_has_native_focus")
        .nth(1)
        .and_then(|tail| tail.split("pub fn set_windowed_focus").next())
        .unwrap_or_default();

    assert!(focus_fn.contains("window.firstResponder()"));
    assert!(focus_fn.contains("downcast_ref::<NSView>()"));
    assert!(focus_fn.contains("isDescendantOf(view)"));
    assert!(implementation.contains("#[cfg(not(target_os = \"macos\"))]"));
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p bevy_cef_core windowed_native_focus_detects_first_responder_subtree
```

Expected: FAIL because `windowed_has_native_focus` does not exist and `focus_fn` is empty.

- [ ] **Step 3: Add the macOS native-focus query and non-macOS fallback**

Insert immediately before `set_windowed_focus`:

```rust
#[cfg(target_os = "macos")]
pub fn windowed_has_native_focus(&self, webview: &Entity) -> Option<bool> {
    use objc2_app_kit::NSView;

    let browser = self.browsers.get(webview)?;
    if !browser.windowed || !browser.allow_native_focus {
        return None;
    }
    let handle = browser.host.window_handle();
    if handle.is_null() {
        return Some(false);
    }
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    let Some(window) = view.window() else {
        return Some(false);
    };
    let Some(responder) = window.firstResponder() else {
        return Some(false);
    };
    let Some(responder_view) = responder.downcast_ref::<NSView>() else {
        return Some(false);
    };
    Some(core::ptr::eq(responder_view, view) || responder_view.isDescendantOf(view))
}

#[cfg(not(target_os = "macos"))]
pub fn windowed_has_native_focus(&self, _: &Entity) -> Option<bool> {
    None
}
```

- [ ] **Step 4: Format and verify GREEN**

Run:

```bash
cargo fmt -p bevy_cef_core
cargo test -p bevy_cef_core windowed_native_focus
```

Expected: formatting succeeds and all matching tests pass.

- [ ] **Step 5: Commit the CEF focus query**

```bash
git add patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs
git commit -m "fix(cef): expose windowed native focus state"
```

### Task 2: Recover active windowed focus without disturbing selection

**Files:**
- Modify: `crates/vmux_browser/src/host_focus.rs:75-110`
- Test: `crates/vmux_browser/src/host_focus.rs:160-205`

- [ ] **Step 1: Add failing focus-decision tests**

Change existing `windowed_focus_action` calls to pass `None` before the mutable cache, then add:

```rust
#[test]
fn windowed_focus_action_recovers_lost_native_focus() {
    let webview = Entity::from_bits(1);
    let mut focused = Some(webview);

    assert_eq!(
        windowed_focus_action(
            HostFocusIntent::Windowed(webview),
            true,
            Some(false),
            &mut focused,
        ),
        Some(webview)
    );
}

#[test]
fn windowed_focus_action_preserves_held_native_focus() {
    let webview = Entity::from_bits(1);
    let mut focused = Some(webview);

    assert_eq!(
        windowed_focus_action(
            HostFocusIntent::Windowed(webview),
            true,
            Some(true),
            &mut focused,
        ),
        None
    );
}

#[test]
fn windowed_focus_action_focuses_changed_target() {
    let previous = Entity::from_bits(1);
    let next = Entity::from_bits(2);
    let mut focused = Some(previous);

    assert_eq!(
        windowed_focus_action(
            HostFocusIntent::Windowed(next),
            true,
            Some(false),
            &mut focused,
        ),
        Some(next)
    );
    assert_eq!(focused, Some(next));
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
cargo test -p vmux_browser host_focus
```

Expected: compilation fails with `E0061` because `windowed_focus_action` does not yet accept native-focus state.

- [ ] **Step 3: Extend the focus decision**

Replace `windowed_focus_action` with:

```rust
fn windowed_focus_action(
    intent: HostFocusIntent,
    has_browser: bool,
    has_native_focus: Option<bool>,
    focused: &mut Option<Entity>,
) -> Option<Entity> {
    match intent {
        HostFocusIntent::Windowed(webview) if has_browser => {
            let should_focus = has_native_focus
                .map(|has_focus| !has_focus)
                .unwrap_or(*focused != Some(webview));
            *focused = Some(webview);
            should_focus.then_some(webview)
        }
        _ => {
            *focused = None;
            None
        }
    }
}
```

Update `apply_windowed_host_focus`:

```rust
pub(crate) fn apply_windowed_host_focus(
    intent: Res<HostFocusIntent>,
    browsers: NonSend<Browsers>,
    mut focused: Local<Option<Entity>>,
) {
    let (has_browser, has_native_focus) = match *intent {
        HostFocusIntent::Windowed(webview) => (
            browsers.has_browser(webview),
            browsers.windowed_has_native_focus(&webview),
        ),
        _ => (false, None),
    };
    if let Some(webview) = windowed_focus_action(
        *intent,
        has_browser,
        has_native_focus,
        &mut focused,
    ) {
        browsers.set_windowed_focus(&webview, true);
    }
}
```

- [ ] **Step 4: Format and verify GREEN**

Run:

```bash
cargo fmt -p vmux_browser
cargo test -p vmux_browser host_focus
cargo test -p bevy_cef_core windowed_native_focus
```

Expected: all matching tests pass. The lost-focus test requests focus again; the held-focus test does not.

- [ ] **Step 5: Commit focus recovery**

```bash
git add crates/vmux_browser/src/host_focus.rs
git commit -m "fix(browser): recover lost windowed focus"
```

### Task 3: Validate the reported start-page path

**Files:**
- Verify: `crates/vmux_browser/src/host_focus.rs`
- Verify: `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs`
- Delete: `docs/plans/2026-07-12-windowed-focus-recovery.md`

- [ ] **Step 1: Run targeted formatting, tests, and lint**

```bash
cargo fmt -p bevy_cef_core -p vmux_browser -- --check
cargo test -p bevy_cef_core windowed_native_focus
cargo test -p vmux_browser host_focus
cargo clippy -p bevy_cef_core -p vmux_browser --all-targets -- -D warnings
```

Expected: every command exits successfully with no warnings.

- [ ] **Step 2: Build the affected desktop path**

```bash
cargo build -p vmux_desktop --features dev
```

Expected: build succeeds using the modified patched CEF crate and `vmux_browser`.

- [ ] **Step 3: Verify macOS behavior in a separate profile**

Launch the dev app with a non-current profile:

```bash
make dev VMUX_PROFILE=focus-recovery
```

Verify:

1. Open `vmux://start/` with Cmd+T; type and use its launcher shortcuts immediately.
2. Close every page until `vmux://start/` auto-spawns; type and use the same shortcuts without clicking.
3. Type in a normal web text field, select text, and wait; selection must remain unchanged.
4. Switch between a terminal and a windowed web page; each accepts input immediately.

Expected: all four checks pass. Stop the separate-profile app afterward.

- [ ] **Step 4: Remove the completed implementation plan**

```bash
git rm docs/plans/2026-07-12-windowed-focus-recovery.md
git commit -m "chore: remove completed focus recovery plan"
```

- [ ] **Step 5: Confirm final worktree state**

```bash
git status --short --branch
git log --oneline -5
```

Expected: clean worktree on `fix/windowed-focus-recovery`; commits include the design, native focus query, focus recovery, and plan removal.
