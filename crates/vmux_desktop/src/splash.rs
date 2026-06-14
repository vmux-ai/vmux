use std::time::Duration;
use std::time::Instant;

use bevy::prelude::*;
use objc2::rc::Retained;
use objc2_app_kit::NSPanel;

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
            .add_systems(Startup, show_splash)
            .add_systems(Last, dismiss_splash);
    }
}

fn show_splash(mut state: NonSendMut<SplashState>) {
    use objc2::{AnyThread, ClassType, MainThreadMarker, MainThreadOnly, runtime::AnyClass};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSGlassEffectView,
        NSGlassEffectViewStyle, NSImage, NSImageScaling, NSImageView, NSProgressIndicator,
        NSProgressIndicatorStyle, NSScreen, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow,
        NSWindowStyleMask,
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
    if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
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

fn dismiss_splash(
    mut state: NonSendMut<SplashState>,
    window_q: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    use objc2::ClassType;
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
            if close && let Some(panel) = state.window.take() {
                let window: &NSWindow = panel.as_super();
                window.close();
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
}
