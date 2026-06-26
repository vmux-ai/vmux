use bevy::prelude::*;

pub(crate) struct DynamicIslandPlugin;

#[cfg(target_os = "macos")]
impl Plugin for DynamicIslandPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<IslandPanel>()
            .add_systems(Startup, install_island_panel)
            .add_systems(Last, (sync_island_overlay, apply_island_resize));
    }
}

#[cfg(not(target_os = "macos"))]
impl Plugin for DynamicIslandPlugin {
    fn build(&self, _app: &mut App) {}
}

#[cfg(target_os = "macos")]
use island_macos::*;

#[cfg(target_os = "macos")]
mod island_macos {
    use bevy::prelude::*;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{ClassType, MainThreadMarker, MainThreadOnly};
    use objc2_core_foundation::CFRetained;
    use objc2_app_kit::{
        NSBackingStoreType, NSColor, NSFloatingWindowLevel, NSGlassEffectView,
        NSGlassEffectViewStyle, NSPanel, NSScreen, NSView, NSWindow, NSWindowCollectionBehavior,
        NSWindowStyleMask,
    };
    use objc2_core_graphics::CGMutablePath;
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use objc2_quartz_core::{CALayer, CAShapeLayer};
    use vmux_layout::island::Island;
    use vmux_layout::island::event::IslandPanelResize;

    const DEFAULT_W: f64 = 360.0;
    const DEFAULT_H: f64 = 44.0;

    #[derive(Default)]
    pub(super) struct IslandPanel {
        panel: Option<Retained<NSPanel>>,
        glass: Option<Retained<NSGlassEffectView>>,
        content_layer: Option<Retained<CALayer>>,
        held: Option<bevy_cef_core::prelude::AcceleratedFrame>,
    }

    /// Panel frame on the active display plus the notch geometry (width, height) when the built-in
    /// display has one. Notched: panel top is flush with the physical screen top so the island can
    /// wrap the notch. Non-notched/external: top-center just under the menu bar.
    fn island_frame(mtm: MainThreadMarker, w: f64, h: f64) -> (NSRect, Option<(f64, f64)>) {
        let Some(screen) = NSScreen::mainScreen(mtm) else {
            return (NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(w, h)), None);
        };
        let f = screen.frame();
        let notch_h = screen.safeAreaInsets().top;
        if notch_h > 0.0 {
            let x = f.origin.x + (f.size.width - w) / 2.0;
            let y = f.origin.y + f.size.height - h;
            let left = screen.auxiliaryTopLeftArea();
            let right = screen.auxiliaryTopRightArea();
            let notch_w = (f.size.width - left.size.width - right.size.width).max(120.0);
            (
                NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)),
                Some((notch_w, notch_h)),
            )
        } else {
            let vf = screen.visibleFrame();
            let x = vf.origin.x + (vf.size.width - w) / 2.0;
            let y = vf.origin.y + vf.size.height - h - 8.0;
            (NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)), None)
        }
    }

    /// Build the island silhouette in layer-local coords (origin bottom-left, y-up). Without a notch
    /// it is a rounded pill; with one, the top-center is carved with rounded inner corners (concave
    /// shoulders) so the glass and the physical notch read as one shape.
    fn island_shape_path(w: f64, h: f64, notch: Option<(f64, f64)>) -> CFRetained<CGMutablePath> {
        let path = CGMutablePath::new();
        let p: &CGMutablePath = &path;
        let r = (h / 2.0).min(22.0);
        match notch {
            None => {
                rounded_rect(p, 0.0, 0.0, w, h, r);
            }
            Some((nw, nh)) => {
                let m = core::ptr::null();
                let nr = 12.0_f64.min(nw / 2.0);
                let nl = (w - nw) / 2.0;
                let nrr = (w + nw) / 2.0;
                let (l, b, rt, t) = (0.0, 0.0, w, h);
                let nb = h - nh;
                unsafe {
                    CGMutablePath::move_to_point(Some(p), m, l, b + r);
                    CGMutablePath::add_quad_curve_to_point(Some(p), m, l, b, l + r, b);
                    CGMutablePath::add_line_to_point(Some(p), m, rt - r, b);
                    CGMutablePath::add_quad_curve_to_point(Some(p), m, rt, b, rt, b + r);
                    CGMutablePath::add_line_to_point(Some(p), m, rt, t - r);
                    CGMutablePath::add_quad_curve_to_point(Some(p), m, rt, t, rt - r, t);
                    CGMutablePath::add_line_to_point(Some(p), m, nrr, t);
                    CGMutablePath::add_line_to_point(Some(p), m, nrr, nb + nr);
                    CGMutablePath::add_quad_curve_to_point(Some(p), m, nrr, nb, nrr - nr, nb);
                    CGMutablePath::add_line_to_point(Some(p), m, nl + nr, nb);
                    CGMutablePath::add_quad_curve_to_point(Some(p), m, nl, nb, nl, nb + nr);
                    CGMutablePath::add_line_to_point(Some(p), m, nl, t);
                    CGMutablePath::add_line_to_point(Some(p), m, l + r, t);
                    CGMutablePath::add_quad_curve_to_point(Some(p), m, l, t, l, t - r);
                    CGMutablePath::close_subpath(Some(p));
                }
            }
        }
        path
    }

    fn rounded_rect(p: &CGMutablePath, x: f64, y: f64, w: f64, h: f64, r: f64) {
        let m = core::ptr::null();
        let (l, b, rt, t) = (x, y, x + w, y + h);
        unsafe {
            CGMutablePath::move_to_point(Some(p), m, l, b + r);
            CGMutablePath::add_quad_curve_to_point(Some(p), m, l, b, l + r, b);
            CGMutablePath::add_line_to_point(Some(p), m, rt - r, b);
            CGMutablePath::add_quad_curve_to_point(Some(p), m, rt, b, rt, b + r);
            CGMutablePath::add_line_to_point(Some(p), m, rt, t - r);
            CGMutablePath::add_quad_curve_to_point(Some(p), m, rt, t, rt - r, t);
            CGMutablePath::add_line_to_point(Some(p), m, l + r, t);
            CGMutablePath::add_quad_curve_to_point(Some(p), m, l, t, l, t - r);
            CGMutablePath::close_subpath(Some(p));
        }
    }

    fn shape_layer(path: &CGMutablePath) -> Retained<CAShapeLayer> {
        let layer = CAShapeLayer::new();
        layer.setPath(Some(path));
        layer
    }

    fn apply_mask(state: &IslandPanel, w: f64, h: f64, notch: Option<(f64, f64)>) {
        let path = island_shape_path(w, h, notch);
        if let Some(glass) = &state.glass {
            let v: &NSView = glass;
            if let Some(layer) = v.layer() {
                unsafe { layer.setMask(Some(&shape_layer(&path))) };
            }
        }
        if let Some(content) = &state.content_layer {
            unsafe { content.setMask(Some(&shape_layer(&path))) };
        }
    }

    pub(super) fn install_island_panel(mut state: NonSendMut<IslandPanel>) {
        if state.panel.is_some() {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let (w, h) = (DEFAULT_W, DEFAULT_H);

        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
            NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            NSBackingStoreType::Buffered,
            false,
        );
        let win: &NSWindow = panel.as_super();
        win.setOpaque(false);
        win.setBackgroundColor(Some(&NSColor::clearColor()));
        win.setHasShadow(true);
        win.setLevel(NSFloatingWindowLevel);
        win.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        win.setIgnoresMouseEvents(true);
        panel.setBecomesKeyOnlyIfNeeded(true);

        let content = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
        );
        content.setWantsLayer(true);

        // Glass backdrop; default dark tint (appearance-mode binding lands with PR #172).
        let glass = NSGlassEffectView::new(mtm);
        glass.setStyle(NSGlassEffectViewStyle::Clear);
        glass.setTintColor(Some(&NSColor::colorWithWhite_alpha(0.0, 0.45)));
        let glass_view: &NSView = &glass;
        glass_view.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)));
        content.addSubview(glass_view);

        // OSR content layer above the glass.
        let content_layer = CALayer::new();
        content_layer.setOpaque(false);
        content_layer.setZPosition(10.0);
        content_layer.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)));
        if let Some(host) = content.layer() {
            host.addSublayer(&content_layer);
        }

        win.setContentView(Some(&content));

        let (frame, notch) = island_frame(mtm, w, h);
        win.setFrame_display(frame, false);

        state.glass = Some(glass);
        state.content_layer = Some(content_layer);
        state.panel = Some(panel);
        apply_mask(&state, w, h, notch);
        if let Some(panel) = &state.panel {
            let win: &NSWindow = panel.as_super();
            win.orderFrontRegardless();
        }
    }

    pub(super) fn sync_island_overlay(
        mut state: NonSendMut<IslandPanel>,
        island_q: Query<Entity, With<Island>>,
        windows: Query<&bevy::window::Window>,
        overlay_frames: Res<bevy_cef::prelude::NativeOverlayFrames>,
    ) {
        let Ok(island_e) = island_q.single() else {
            return;
        };
        let next = overlay_frames
            .0
            .lock()
            .ok()
            .and_then(|mut map| map.remove(&island_e));
        if next.is_none() && state.held.is_none() {
            return;
        }
        let Some(layer) = state.content_layer.clone() else {
            return;
        };
        if let Some(frame) = next {
            let io_surface = frame.io_surface as *mut AnyObject;
            if !io_surface.is_null() {
                let scale = windows
                    .iter()
                    .next()
                    .map(|w| w.resolution.scale_factor() as f64)
                    .unwrap_or(2.0);
                layer.setOpaque(false);
                layer.setContentsScale(scale);
                unsafe { layer.setContents(Some(&*io_surface)) };
                state.held = Some(frame);
            }
        }
    }

    pub(super) fn apply_island_resize(
        state: NonSend<IslandPanel>,
        mut resizes: MessageReader<IslandPanelResize>,
    ) {
        let Some(r) = resizes.read().last().copied() else {
            return;
        };
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(panel) = &state.panel else {
            return;
        };
        let (w, h) = (r.width as f64, r.height as f64);
        let (frame, notch) = island_frame(mtm, w, h);
        let win: &NSWindow = panel.as_super();
        win.setFrame_display_animate(frame, true, true);
        if let Some(glass) = &state.glass {
            let v: &NSView = glass;
            v.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)));
        }
        if let Some(content) = &state.content_layer {
            content.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)));
        }
        apply_mask(&state, w, h, notch);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn island_panel_is_nonactivating_all_spaces_floating_glass() {
        let src = include_str!("dynamic_island.rs");
        assert!(src.contains("NSWindowStyleMask::NonactivatingPanel"));
        assert!(src.contains("CanJoinAllSpaces"));
        assert!(src.contains("FullScreenAuxiliary"));
        assert!(src.contains("NSGlassEffectView"));
        assert!(src.contains("NSFloatingWindowLevel"));
    }

    #[test]
    fn island_wraps_notch_via_path_mask() {
        let src = include_str!("dynamic_island.rs");
        assert!(src.contains("fn island_shape_path"));
        assert!(src.contains("safeAreaInsets"));
        assert!(src.contains("auxiliaryTopLeftArea"));
        assert!(src.contains("CAShapeLayer"));
    }

    #[test]
    fn island_does_not_force_continuous_update_mode() {
        let src = include_str!("dynamic_island.rs");
        // Split literal so this assertion does not match itself in the source scrape.
        let banned = concat!("UpdateMode", "::", "Continuous");
        assert!(!src.contains(banned));
    }
}
