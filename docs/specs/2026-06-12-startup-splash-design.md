# Native Startup Splash Design

## Overview

On macOS the primary window is created hidden (`visible: false`, `crates/vmux_desktop/src/lib.rs:52`) and only revealed once the layout web page finishes loading and pings back `PageReady` over CEF's IPC channel (`reveal_window_after_layout_ready`, `crates/vmux_desktop/src/glass.rs:131`). `PageReady` originates in the page's JS (`mark_webview_page_ready`, `crates/vmux_core/src/page.rs:60`), so the blank period spans the full CEF subprocess spawn + layout SPA load + first-paint round-trip. On a cold start that is 1-3s of nothing on screen, which reads as "the app didn't launch."

Show a native AppKit splash window the moment the app starts, and dismiss it with a short fade when the real window reveals. The splash gives immediate, animated "it's starting" feedback and a seamless visual handoff into the Liquid Glass window.

## Scope

**In scope:**
- A native macOS splash window (`NSPanel`) created at the earliest Bevy `Startup`, before CEF/layout boot.
- A frosted material matching the app: `NSGlassEffectView` (Liquid Glass) on macOS 26+, `NSVisualEffectView` behind-window blur on macOS 13-25.
- Centered app logo (`NSImageView`) + animated spinner (`NSProgressIndicator`).
- Dismiss via ~150ms `alphaValue` fade once the primary window becomes visible; safety-timeout fallback if reveal never happens.
- macOS-only; cfg-gated so it is a no-op on Linux.

**Out of scope:**
- Any cross-platform / Linux splash.
- Splash on window re-show from the tray or app reactivation (startup-only).
- Progress text, version label, or "Starting…" copy (logo + spinner only).
- Theming / configurable splash appearance.
- Changing the existing `PageReady` reveal logic itself.

## Behavior

1. Process launches. At the first Bevy `Startup`, `show_splash` builds the splash `NSPanel`, centers it on the main screen, starts the spinner, and orders it front. The macOS run loop is already alive at `Startup`, so the spinner animates and the panel paints.
2. CEF spawns and the layout page loads in the background, exactly as today. The window stays hidden.
3. When the layout page fires `PageReady`, `reveal_window_after_layout_ready` sets the primary window `visible = true` (unchanged).
4. `dismiss_splash` observes `visible == true`, fades the splash `alphaValue` to 0 over ~150ms on top of the now-visible window (no flash of desktop), then closes and releases it.
5. Safety net: if the window has not revealed within `SPLASH_TIMEOUT` (20s) the splash force-fades anyway (with a `warn!`) so the spinner cannot spin forever.

Non-macOS builds never add `SplashPlugin` and behave exactly as today.

## Data Flow

```
Bevy Startup ──▶ show_splash ──▶ NSPanel (glass/blur) + logo + spinner, ordered front
                                        │
CEF spawn + layout load (background, unchanged)
                                        ▼
PageReady (JS) ─▶ reveal_window_after_layout_ready ─▶ Window.visible = true
                                        │
Last: dismiss_splash reads Window.visible ─▶ splash_decision(visible, dismissed, elapsed)
   │                                              │
   ├── Fade  ─▶ NSAnimationContext alpha→0 (150ms) ─▶ completion: panel.close()
   └── Force ─▶ same fade after 20s timeout (warn!)
```

## Components & Changes

### 1. `crates/vmux_desktop/src/splash.rs` (new, `#[cfg(target_os = "macos")]`)

- `pub(crate) struct SplashPlugin` — registers the `SplashState` non-send resource and the two systems:
  - `show_splash` in `Startup`.
  - `dismiss_splash` in `Last`.
- `SplashState` (NonSend, mirrors `GlassState`): `window: Option<Retained<NSPanel>>`, `shown: bool`, `dismissed: bool`, `created_at: Option<Instant>`.
- `show_splash`:
  - Guard: run once (`shown`); require `MainThreadMarker` and `NSScreen::mainScreen` (skip silently if absent — no crash).
  - Build borderless, non-activating `NSPanel` (`NSWindowStyleMask::Borderless | NonactivatingPanel`), `opaque = false`, clear background, `hasShadow = true`, floating level, sized ~280×280, centered on the main screen's `visibleFrame`.
  - **Material (version-detected, reusing glass.rs's `AnyClass::get(c"NSGlassEffectView")` check):**
    - macOS 26+: `NSGlassEffectView` (style `Clear`, faint dark tint for logo legibility), as the content/background view — same construction pattern as `install_window_glass`.
    - macOS 13-25: `NSVisualEffectView`, `blendingMode = .behindWindow`, `state = .active`, a HUD/popover material; corner radius via its layer (`wantsLayer`, `cornerRadius`, `masksToBounds`).
  - Add subviews on top of the effect view: centered `NSImageView` with the logo, and an `NSProgressIndicator` (style `.spinning`, `startAnimation`) beneath it.
  - Logo: `include_bytes!("../../../packaging/macos/vmux-icon.png")` → `NSData` → `NSImage`. Works in `cargo run` (unbundled) and in the packaged app. `NSImageView` scales the 1097×1097 source down.
  - `orderFrontRegardless`; set `shown = true`, `created_at = Some(Instant::now())`, store the `Retained<NSPanel>`.
- `dismiss_splash`:
  - Query the primary window's `visible`; compute `splash_decision(visible, state.dismissed, state.created_at.elapsed())`.
  - On `Fade` / `Force`: set `dismissed = true`; run an `NSAnimationContext` grouped animation, `panel.animator().setAlphaValue(0.0)`, duration ~0.15s, with a `block2` completion handler that closes the panel. `Force` additionally logs `warn!`. Capture a `Retained<NSPanel>` clone in the completion block so AppKit keeps the panel alive through the animation; the resource's own `Option` may be cleared immediately.
- `splash_decision(visible: bool, dismissed: bool, elapsed: Duration) -> SplashAction` — **pure**, no AppKit. Returns `None` while `!visible && elapsed < TIMEOUT`, `Fade` when `visible && !dismissed`, `Force` when `!visible && elapsed >= TIMEOUT && !dismissed`, `None` when `dismissed`. This is the unit-tested core.

### 2. `crates/vmux_desktop/src/lib.rs`

- Add `#[cfg(target_os = "macos")] mod splash;`.
- In the existing `#[cfg(target_os = "macos")]` block, add `SplashPlugin` (alongside `GlassPlugin`).

### 3. `crates/vmux_desktop/Cargo.toml`

- `objc2-app-kit` features: add `NSVisualEffectView` (transitively enables `NSGlassEffectView`, already listed), `NSImage`, `NSImageView`, `NSProgressIndicator`, `NSScreen`.
- `objc2-foundation` features: add `NSData`.

## Testing

AppKit window construction cannot be exercised headlessly, so follow the existing `glass.rs` convention: pure-logic unit tests plus source-assertion tests, with the visual result verified by running the app.

- **Pure logic (TDD, write first):** `splash_decision` —
  - hidden + within timeout → `None`
  - visible + not dismissed → `Fade`
  - hidden + past timeout + not dismissed → `Force`
  - already dismissed → `None` (idempotent)
- **Source-assertion (mirror glass.rs tests):**
  - `lib.rs` registers `SplashPlugin` under a macOS cfg.
  - `splash.rs` uses `NSProgressIndicator` and detects the material via `AnyClass::get(c"NSGlassEffectView")`.
  - `splash.rs` embeds the logo via `include_bytes!`.
  - `Cargo.toml` enables the new `objc2-app-kit` / `objc2-foundation` features.
- **Manual verification:** cold-launch the app and confirm the splash appears immediately with a spinning indicator, then fades out as the window reveals; confirm no flash of desktop between splash and content; confirm it does not steal focus from the terminal.

## Edge Cases

- **Linux / non-macOS:** plugin not added; no-op.
- **macOS < 26:** Liquid Glass unavailable; splash uses the `NSVisualEffectView` blur path. Independent of the window's own glass fallback.
- **No main screen** (`NSScreen::mainScreen` is `None`): skip creating the splash; no crash, app proceeds without it.
- **Very fast `PageReady`:** splash shows briefly then fades — acceptable; no minimum-display time (YAGNI).
- **Tray re-show / reactivation:** `show_splash` is `Startup`-once, so re-showing the window later never re-triggers the splash.
- **Re-entrancy:** the `dismissed` flag makes `dismiss_splash` idempotent across frames and during the fade.
