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
    status_label: Option<Retained<objc2_app_kit::NSTextField>>,
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
            .add_systems(Last, (update_splash_text, dismiss_splash).chain());
    }
}

fn show_splash(mut state: NonSendMut<SplashState>) {
    use objc2::{ClassType, MainThreadMarker, MainThreadOnly, runtime::AnyClass};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSFont, NSGlassEffectView,
        NSGlassEffectViewStyle, NSProgressIndicator, NSProgressIndicatorStyle, NSScreen,
        NSTextAlignment, NSTextField, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowCollectionBehavior,
        NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

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
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );

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

    const TITLE_H: f64 = 40.0;
    let title = NSTextField::labelWithString(&NSString::from_str("Vmux"), mtm);
    title.setFrame(NSRect::new(
        NSPoint::new(0.0, (H - TITLE_H) / 2.0 + 40.0),
        NSSize::new(W, TITLE_H),
    ));
    title.setAlignment(NSTextAlignment::Center);
    title.setFont(Some(&NSFont::boldSystemFontOfSize(28.0)));
    title.setTextColor(Some(&NSColor::labelColor()));
    container.addSubview(&title);

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

    const STATUS_H: f64 = 20.0;
    let status = NSTextField::labelWithString(&NSString::from_str("Starting..."), mtm);
    status.setFrame(NSRect::new(
        NSPoint::new(0.0, (H - STATUS_H) / 2.0 - 96.0),
        NSSize::new(W, STATUS_H),
    ));
    status.setAlignment(NSTextAlignment::Center);
    status.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    status.setTextColor(Some(&NSColor::secondaryLabelColor()));
    container.addSubview(&status);
    state.status_label = Some(status);

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

fn update_splash_text(state: NonSend<SplashState>, status: Res<crate::boot_status::SplashStatus>) {
    use objc2_foundation::NSString;
    if let Some(label) = &state.status_label {
        label.setStringValue(&NSString::from_str(&status.phase.display()));
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
    fn splash_shows_title_and_status_label() {
        let source = include_str!("splash.rs");
        assert!(source.contains("NSTextField"));
        assert!(source.contains("\"Vmux\""));
        assert!(source.contains("SplashStatus"));
        assert!(source.contains("update_splash_text"));
    }

    #[test]
    fn splash_panel_is_fullscreen_auxiliary() {
        let source = include_str!("splash.rs");
        assert!(source.contains("setCollectionBehavior"));
        assert!(source.contains("FullScreenAuxiliary"));
        assert!(source.contains("CanJoinAllSpaces"));
    }

    #[test]
    fn desktop_enables_splash_appkit_features() {
        let manifest = include_str!("../Cargo.toml");
        assert!(manifest.contains("\"objc2-app-kit/NSProgressIndicator\""));
        assert!(manifest.contains("\"objc2-app-kit/NSVisualEffectView\""));
        assert!(manifest.contains("\"objc2-app-kit/NSTextField\""));
        assert!(manifest.contains("\"objc2-app-kit/NSFont\""));
    }
}
