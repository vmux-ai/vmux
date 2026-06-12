# Native Startup Splash Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a native macOS splash window (logo + spinner, Liquid Glass / blur) the instant Vmux launches, dismissed with a fade when the real window reveals, so cold start no longer looks frozen.

**Architecture:** A macOS-only `SplashPlugin` in `crates/vmux_desktop`. `show_splash` runs at Bevy `Startup` and builds an `NSPanel` (frosted material + centered `NSImageView` logo + spinning `NSProgressIndicator`). `dismiss_splash` runs at `Last`, watches the primary window's `visible` flag, and fades + closes the panel via a pure `splash_decision` helper. Mirrors the existing `glass.rs` AppKit patterns.

**Tech Stack:** Rust, Bevy 0.19, `objc2` / `objc2-app-kit` / `objc2-foundation` / `objc2-quartz-core`.

---

## Context for the implementer

- All work happens in the existing worktree `.worktrees/startup-splash` (branch `feat/startup-splash`). Do not edit the main checkout.
- Run shell commands through bash: `bash -c "cd .worktrees/startup-splash && <cmd>"`.
- **No code comments** — this codebase forbids them. The code blocks below are comment-free on purpose; keep them that way.
- The crate `vmux_desktop` is macOS-primary. Everything here is gated with `#[cfg(target_os = "macos")]`.
- Reference file for AppKit idioms: `crates/vmux_desktop/src/glass.rs` (NonSend state, `MainThreadMarker`, `NSPanel` construction, `AnyClass::get(c"NSGlassEffectView")` version check).
- `cargo test -p vmux_desktop` is the targeted test command. The first build pulls CEF and is slow; later builds are incremental.

## File Structure

- **Create** `crates/vmux_desktop/src/splash.rs` — the whole feature: `SplashPlugin`, `SplashState`, `show_splash`, `dismiss_splash`, `splash_decision`, tests.
- **Modify** `crates/vmux_desktop/src/lib.rs` — register the module + plugin (macOS cfg block).
- **Modify** `crates/vmux_desktop/Cargo.toml` — enable the new `objc2-app-kit` / `objc2-foundation` features.

---

## Task 1: Enable AppKit features

**Files:**
- Modify: `crates/vmux_desktop/Cargo.toml`

- [ ] **Step 1: Add the `objc2-app-kit` features**

In `crates/vmux_desktop/Cargo.toml`, replace the `objc2-app-kit` dependency feature list (currently lines ~57-67) so it reads:

```toml
objc2-app-kit = { version = "0.3", features = [
    "NSApplication",
    "NSEvent",
    "NSView",
    "NSWindow",
    "NSResponder",
    "NSGraphics",
    "NSColor",
    "NSGlassEffectView",
    "NSPanel",
    "NSVisualEffectView",
    "NSImage",
    "NSImageView",
    "NSProgressIndicator",
    "NSScreen",
    "NSAnimation",
] }
```

- [ ] **Step 2: Add the `objc2-foundation` `NSData` feature**

In the same file, change the `objc2-foundation` line to:

```toml
objc2-foundation = { version = "0.3", features = ["NSGeometry", "NSData"] }
```

- [ ] **Step 3: Verify it still builds**

Run: `bash -c "cd .worktrees/startup-splash && cargo build -p vmux_desktop 2>&1 | tail -5"`
Expected: build completes (added features are unused for now; that is fine — no errors).

- [ ] **Step 4: Commit**

```bash
bash -c "cd .worktrees/startup-splash && git add crates/vmux_desktop/Cargo.toml && git commit -m 'chore(splash): enable AppKit features for startup splash'"
```

---

## Task 2: Pure `splash_decision` helper (TDD)

This is the only unit-testable core (AppKit window code is verified manually). Build it first.

**Files:**
- Create: `crates/vmux_desktop/src/splash.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`

- [ ] **Step 1: Write the failing test (create the file with only the helper + tests)**

Create `crates/vmux_desktop/src/splash.rs`:

```rust
use std::time::Duration;

const SPLASH_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplashAction {
    None,
    Fade,
    Force,
}

fn splash_decision(visible: bool, dismissed: bool, elapsed: Duration) -> SplashAction {
    if dismissed {
        return SplashAction::None;
    }
    if visible {
        return SplashAction::Fade;
    }
    if elapsed >= SPLASH_TIMEOUT {
        return SplashAction::Force;
    }
    SplashAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_within_timeout_does_nothing() {
        assert_eq!(
            splash_decision(false, false, Duration::from_secs(1)),
            SplashAction::None
        );
    }

    #[test]
    fn visible_triggers_fade() {
        assert_eq!(
            splash_decision(true, false, Duration::from_secs(1)),
            SplashAction::Fade
        );
    }

    #[test]
    fn hidden_past_timeout_forces_dismiss() {
        assert_eq!(
            splash_decision(false, false, Duration::from_secs(20)),
            SplashAction::Force
        );
    }

    #[test]
    fn dismissed_is_idempotent() {
        assert_eq!(
            splash_decision(true, true, Duration::from_secs(1)),
            SplashAction::None
        );
        assert_eq!(
            splash_decision(false, true, Duration::from_secs(99)),
            SplashAction::None
        );
    }
}
```

- [ ] **Step 2: Wire the module into `lib.rs`**

In `crates/vmux_desktop/src/lib.rs`, next to the other macOS modules (after `mod glass;`, around line 14), add:

```rust
#[cfg(target_os = "macos")]
mod splash;
```

- [ ] **Step 3: Run the test to verify it passes**

Run: `bash -c "cd .worktrees/startup-splash && cargo test -p vmux_desktop splash:: 2>&1 | tail -15"`
Expected: 4 tests pass. (They are simple enough to pass immediately — that's fine; the value is locking the decision table.)

Note: a `dead_code` warning for `SplashAction`/`splash_decision` is expected until Task 4 uses them. CI clippy runs on the final state, not here.

- [ ] **Step 4: Commit**

```bash
bash -c "cd .worktrees/startup-splash && git add crates/vmux_desktop/src/splash.rs crates/vmux_desktop/src/lib.rs && git commit -m 'feat(splash): add splash_decision dismissal helper'"
```

---

## Task 3: Build and show the splash panel

**Files:**
- Modify: `crates/vmux_desktop/src/splash.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`

- [ ] **Step 1: Add imports, state, plugin, and `show_splash`**

At the **top** of `crates/vmux_desktop/src/splash.rs`, above the `const SPLASH_TIMEOUT` line, add:

```rust
use std::time::Instant;

use bevy::prelude::*;
use objc2::rc::Retained;
use objc2_app_kit::NSPanel;
```

Then, **below** the `splash_decision` function (before the `#[cfg(test)]` module), add:

```rust
#[derive(Default)]
struct SplashState {
    window: Option<Retained<NSPanel>>,
    shown: bool,
    dismissed: bool,
    created_at: Option<Instant>,
    fade_started: Option<Instant>,
}

pub(crate) struct SplashPlugin;

impl Plugin for SplashPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<SplashState>()
            .add_systems(Startup, show_splash);
    }
}

fn show_splash(mut state: NonSendMut<SplashState>) {
    use objc2::{runtime::AnyClass, MainThreadMarker};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSGlassEffectView,
        NSGlassEffectViewStyle, NSImage, NSImageScaling, NSImageView, NSProgressIndicator,
        NSProgressIndicatorStyle, NSScreen, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowStyleMask,
    };
    use objc2_foundation::{NSData, NSPoint, NSRect, NSSize};

    if state.shown {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    state.shown = true;
    let Some(screen) = NSScreen::mainScreen(mtm) else {
        return;
    };

    const W: f64 = 280.0;
    const H: f64 = 280.0;
    let vf = screen.visibleFrame();
    let frame = NSRect::new(
        NSPoint::new(
            vf.origin.x + (vf.size.width - W) / 2.0,
            vf.origin.y + (vf.size.height - H) / 2.0,
        ),
        NSSize::new(W, H),
    );

    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        frame,
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    let window: &NSWindow = panel.as_super();
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHasShadow(true);
    unsafe { window.setReleasedWhenClosed(false) };
    panel.setBecomesKeyOnlyIfNeeded(true);

    let bounds = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(W, H));
    let resize =
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable;

    let container = NSView::initWithFrame(NSView::alloc(mtm), bounds);
    container.setWantsLayer(true);
    if let Some(layer) = container.layer() {
        layer.setCornerRadius(20.0);
        layer.setMasksToBounds(true);
    }

    if AnyClass::get(c"NSGlassEffectView").is_some() {
        let glass = NSGlassEffectView::new(mtm);
        glass.setStyle(NSGlassEffectViewStyle::Clear);
        glass.setTintColor(Some(&NSColor::clearColor()));
        let view: &NSView = &glass;
        view.setFrame(bounds);
        view.setAutoresizingMask(resize);
        container.addSubview(view);
    } else {
        let blur = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), bounds);
        blur.setMaterial(NSVisualEffectMaterial::HUDWindow);
        blur.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        blur.setState(NSVisualEffectState::Active);
        let view: &NSView = &blur;
        view.setAutoresizingMask(resize);
        container.addSubview(view);
    }

    let bytes: &[u8] = include_bytes!("../../../packaging/macos/vmux-icon.png");
    let data = NSData::with_bytes(bytes);
    if let Some(image) = NSImage::initWithData(NSImage::alloc(mtm), &data) {
        const LOGO: f64 = 96.0;
        let logo = NSImageView::imageViewWithImage(&image, mtm);
        logo.setFrame(NSRect::new(
            NSPoint::new((W - LOGO) / 2.0, (H - LOGO) / 2.0 + 24.0),
            NSSize::new(LOGO, LOGO),
        ));
        logo.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        container.addSubview(&logo);
    }

    const SPIN: f64 = 32.0;
    let spinner = NSProgressIndicator::initWithFrame(
        NSProgressIndicator::alloc(mtm),
        NSRect::new(
            NSPoint::new((W - SPIN) / 2.0, (H - SPIN) / 2.0 - 56.0),
            NSSize::new(SPIN, SPIN),
        ),
    );
    spinner.setStyle(NSProgressIndicatorStyle::Spinning);
    spinner.setIndeterminate(true);
    unsafe { spinner.startAnimation(None) };
    container.addSubview(&spinner);

    window.setContentView(Some(&container));
    window.orderFrontRegardless();

    state.window = Some(panel);
    state.created_at = Some(Instant::now());
}
```

- [ ] **Step 2: Register the plugin in `lib.rs`**

In `crates/vmux_desktop/src/lib.rs`, find the macOS block at the end of `impl Plugin for VmuxPlugin` (currently):

```rust
        #[cfg(target_os = "macos")]
        app.add_plugins(glass::GlassPlugin)
            .add_systems(Last, focus_native::apply_winit_host_focus);
```

Replace it with (bundling the plugins in one tuple per project style):

```rust
        #[cfg(target_os = "macos")]
        app.add_plugins((glass::GlassPlugin, splash::SplashPlugin))
            .add_systems(Last, focus_native::apply_winit_host_focus);
```

- [ ] **Step 3: Build**

Run: `bash -c "cd .worktrees/startup-splash && cargo build -p vmux_desktop 2>&1 | tail -15"`
Expected: compiles. If a deref coercion error appears on `container.addSubview(&logo)` / `&spinner`, wrap as `let v: &NSView = &logo; container.addSubview(v);` (same for the spinner).

- [ ] **Step 4: Commit**

```bash
bash -c "cd .worktrees/startup-splash && git add crates/vmux_desktop/src/splash.rs crates/vmux_desktop/src/lib.rs && git commit -m 'feat(splash): show native splash window at startup'"
```

---

## Task 4: Dismiss the splash on window reveal

**Files:**
- Modify: `crates/vmux_desktop/src/splash.rs`

- [ ] **Step 1: Register the dismiss system**

In `SplashPlugin::build`, extend the chain to add the `Last` system:

```rust
impl Plugin for SplashPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<SplashState>()
            .add_systems(Startup, show_splash)
            .add_systems(Last, dismiss_splash);
    }
}
```

- [ ] **Step 2: Implement `dismiss_splash`**

Add below `show_splash` (above the test module):

```rust
fn dismiss_splash(
    mut state: NonSendMut<SplashState>,
    window_q: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    use objc2_app_kit::{NSAnimatablePropertyContainer, NSWindow};

    if state.window.is_none() {
        return;
    }
    let visible = window_q.single().map(|w| w.visible).unwrap_or(false);
    let elapsed = state.created_at.map(|t| t.elapsed()).unwrap_or_default();
    let action = splash_decision(visible, state.dismissed, elapsed);

    match action {
        SplashAction::None => {
            let close = state
                .fade_started
                .is_some_and(|t| t.elapsed() >= std::time::Duration::from_millis(280));
            if close {
                if let Some(panel) = state.window.take() {
                    let window: &NSWindow = panel.as_super();
                    window.close();
                }
            }
        }
        SplashAction::Fade | SplashAction::Force => {
            if action == SplashAction::Force {
                warn!("splash: window did not reveal within timeout; dismissing splash");
            }
            if let Some(panel) = state.window.as_ref() {
                let window: &NSWindow = panel.as_super();
                window.animator().setAlphaValue(0.0);
            }
            state.dismissed = true;
            state.fade_started = Some(Instant::now());
        }
    }
}
```

- [ ] **Step 3: Build**

Run: `bash -c "cd .worktrees/startup-splash && cargo build -p vmux_desktop 2>&1 | tail -15"`
Expected: compiles, no `dead_code` warnings for `SplashAction` / `splash_decision` anymore.

- [ ] **Step 4: Commit**

```bash
bash -c "cd .worktrees/startup-splash && git add crates/vmux_desktop/src/splash.rs && git commit -m 'feat(splash): fade out splash when window reveals'"
```

---

## Task 5: Guard tests, lint, and manual verification

**Files:**
- Modify: `crates/vmux_desktop/src/splash.rs`

- [ ] **Step 1: Add source-assertion guard tests**

These mirror `glass.rs`'s style and prevent regressions (wiring, material detection, asset embedding, features). Add them inside the existing `#[cfg(test)] mod tests` block in `splash.rs`:

```rust
    #[test]
    fn splash_plugin_registered_in_lib() {
        let source = include_str!("lib.rs");
        assert!(source.contains("splash::SplashPlugin"));
        assert!(source.contains("mod splash;"));
    }

    #[test]
    fn splash_uses_spinner_and_version_detected_material() {
        let source = include_str!("splash.rs");
        assert!(source.contains("NSProgressIndicator"));
        assert!(source.contains("AnyClass::get(c\"NSGlassEffectView\")"));
        assert!(source.contains("NSVisualEffectView"));
    }

    #[test]
    fn splash_embeds_logo() {
        let source = include_str!("splash.rs");
        assert!(source.contains("include_bytes!"));
        assert!(source.contains("vmux-icon.png"));
    }

    #[test]
    fn desktop_enables_splash_appkit_features() {
        let manifest = include_str!("../Cargo.toml");
        assert!(manifest.contains("\"NSProgressIndicator\""));
        assert!(manifest.contains("\"NSVisualEffectView\""));
        assert!(manifest.contains("\"NSImageView\""));
        assert!(manifest.contains("\"NSData\""));
    }
```

- [ ] **Step 2: Run the full crate test + lint**

Run: `bash -c "cd .worktrees/startup-splash && cargo test -p vmux_desktop splash:: 2>&1 | tail -20"`
Expected: all `splash::` tests pass (4 logic + 4 guard = 8).

Run: `bash -c "cd .worktrees/startup-splash && cargo fmt -p vmux_desktop && cargo clippy -p vmux_desktop 2>&1 | tail -20"`
Expected: no warnings/errors in `splash.rs`.

- [ ] **Step 3: Manual verification (required — AppKit visuals can't be unit-tested)**

Run the app from the worktree: `bash -c "cd .worktrees/startup-splash && cargo run -p vmux_desktop"`

Confirm:
- Splash panel appears **immediately** at launch (well before the main window), centered, frosted, with the logo and a **spinning** indicator.
- When the main window reveals, the splash **fades out** (~150-280ms) over the now-visible window — no flash of bare desktop in between.
- The splash does **not** steal keyboard focus from the terminal.
- Quit and relaunch once to confirm it's reproducible.

If the splash never appears, check the run log for the `glass:` / panel path and that `NSScreen::mainScreen` returned a screen.

- [ ] **Step 4: Commit**

```bash
bash -c "cd .worktrees/startup-splash && git add crates/vmux_desktop/src/splash.rs && git commit -m 'test(splash): add guard tests for splash wiring and assets'"
```

- [ ] **Step 5: Clean up the plan**

Per project convention, delete this plan file once implemented:

```bash
bash -c "cd .worktrees/startup-splash && git rm docs/plans/2026-06-12-startup-splash.md && git commit -m 'chore(splash): remove implemented plan'"
```

---

## Self-Review notes (author)

- **Spec coverage:** material version-detection (Task 3 glass/blur branches), logo+spinner (Task 3), Startup creation (Task 3), fade dismissal + timeout fallback (Task 4 via `splash_decision`), no-main-screen guard (Task 3 early return), macOS-only (cfg gates + plugin only added in macOS block), tests (Task 2 logic + Task 5 guards + manual). All spec sections mapped.
- **Type consistency:** `SplashAction` / `splash_decision` / `SplashState` field names (`window`, `shown`, `dismissed`, `created_at`, `fade_started`) used identically across Tasks 2-4.
- **Deviation from spec:** dismissal uses `animator().setAlphaValue(0.0)` (default-context implicit animation) + a deferred `close()` once `fade_started` elapses ~280ms, instead of an `NSAnimationContext` completion handler. Same visual result, no `block2` plumbing, simpler and lower-risk. `setReleasedWhenClosed(false)` added so the `Retained<NSPanel>` owns the lifetime safely across `close()`.
