use std::time::{Duration, Instant};

use bevy::prelude::*;
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, rc::Retained, runtime::AnyClass};
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSFont,
    NSGlassEffectView, NSGlassEffectViewStyle, NSPanel, NSProgressIndicator,
    NSProgressIndicatorStyle, NSScreen, NSTextAlignment, NSTextField, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use vmux_ecs::time::{Clock, Deadline, DeadlinePlugin};

use crate::boot_status::{
    Closed, Dismissal, PageCounts, PresentedAt, SplashRequestSet, SplashStatus,
};

pub(crate) struct SplashPlugin;

struct SplashFade;

impl Plugin for SplashPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DeadlinePlugin::<SplashFade>::default())
            .init_non_send::<SplashState>()
            .add_systems(PostStartup, show)
            .add_systems(
                Last,
                (update_text, dismiss, drain)
                    .chain()
                    .after(SplashRequestSet),
            );
    }
}

#[derive(Default)]
struct SplashState {
    window: Option<Retained<NSPanel>>,
    status_label: Option<Retained<NSTextField>>,
    owner: Option<Entity>,
    fade_started: Option<Instant>,
}

impl SplashState {
    fn build_panel(mtm: MainThreadMarker, frame: NSRect) -> Retained<NSPanel> {
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
        panel
    }

    fn attach_backdrop(mtm: MainThreadMarker, container: &NSView, bounds: NSRect) {
        let resize = NSAutoresizingMaskOptions::ViewWidthSizable
            | NSAutoresizingMaskOptions::ViewHeightSizable;
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
    }

    fn show(&mut self, owner: Entity, boot: &SplashStatus, pages: &PageCounts) -> bool {
        if self.window.is_some() {
            return false;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let Some(screen) = NSScreen::mainScreen(mtm) else {
            return false;
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

        let panel = Self::build_panel(mtm, frame);
        let window: &NSWindow = panel.as_super();

        let bounds = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(W, H));
        let container = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        container.setWantsLayer(true);
        if let Some(layer) = container.layer() {
            layer.setCornerRadius(20.0);
            layer.setMasksToBounds(true);
        }
        Self::attach_backdrop(mtm, &container, bounds);

        const TITLE_H: f64 = 40.0;
        let title =
            NSTextField::labelWithString(&NSString::from_str(vmux_profile::Product::name()), mtm);
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
        let text = boot.display(pages);
        let status = NSTextField::labelWithString(&NSString::from_str(&text), mtm);
        status.setFrame(NSRect::new(
            NSPoint::new(0.0, (H - STATUS_H) / 2.0 - 96.0),
            NSSize::new(W, STATUS_H),
        ));
        status.setAlignment(NSTextAlignment::Center);
        status.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        status.setTextColor(Some(&NSColor::secondaryLabelColor()));
        container.addSubview(&status);
        self.status_label = Some(status);

        window.setContentView(Some(&container));
        window.orderFrontRegardless();

        self.window = Some(panel);
        self.owner = Some(owner);
        true
    }

    fn dismiss(&mut self, owner: Entity, clock: &Clock) -> bool {
        if self.owner != Some(owner) || self.fade_started.is_some() {
            return false;
        }
        let Some(panel) = self.window.as_ref() else {
            return false;
        };
        let window: &NSWindow = panel.as_super();
        window.animator().setAlphaValue(0.0);
        self.fade_started = Some(clock.now());
        true
    }

    fn drain(&mut self, cancelled: bool, clock: Option<&Clock>) -> Option<Entity> {
        if !cancelled
            && self.fade_started.is_none_or(|started| {
                clock.map_or_else(|| started.elapsed(), |clock| clock.elapsed_since(started))
                    < Duration::from_millis(280)
            })
        {
            return None;
        }
        if let Some(panel) = self.window.take() {
            let window: &NSWindow = panel.as_super();
            window.close();
        }
        self.status_label = None;
        self.fade_started = None;
        self.owner.take()
    }

    fn update_text(&self, text: &str) {
        if let Some(label) = &self.status_label {
            label.setStringValue(&NSString::from_str(text));
        }
    }
}

impl Drop for SplashState {
    fn drop(&mut self) {
        let _ = self.drain(true, None);
    }
}

fn show(
    mut state: NonSendMut<SplashState>,
    boot: Single<(Entity, &SplashStatus, &PageCounts)>,
    clock: Res<Clock>,
    mut commands: Commands,
) {
    let (owner, boot, pages) = *boot;
    if state.show(owner, boot, pages) {
        commands.entity(owner).insert(PresentedAt(clock.now()));
    }
}

fn dismiss(
    mut state: NonSendMut<SplashState>,
    requests: Query<Entity, With<Dismissal>>,
    clock: Res<Clock>,
    mut commands: Commands,
) {
    for owner in &requests {
        if state.dismiss(owner, &clock) {
            commands.entity(owner).insert(Deadline::<SplashFade>::after(
                &clock,
                Duration::from_millis(280),
            ));
        }
    }
}

fn drain(
    mut state: NonSendMut<SplashState>,
    owners: Query<(), With<SplashStatus>>,
    clock: Res<Clock>,
    mut commands: Commands,
) {
    let Some(owner) = state.owner else {
        return;
    };
    let current = owners.contains(owner);
    if let Some(owner) = state.drain(!current, Some(&clock))
        && current
    {
        commands
            .entity(owner)
            .try_insert(Closed)
            .try_remove::<Deadline<SplashFade>>();
    }
}

fn update_text(state: NonSend<SplashState>, owners: Query<(&SplashStatus, &PageCounts)>) {
    let Some(owner) = state.owner else {
        return;
    };
    if let Ok((status, pages)) = owners.get(owner) {
        state.update_text(&status.display(pages));
    }
}
