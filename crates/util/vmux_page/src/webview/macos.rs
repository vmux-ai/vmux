use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSAutoresizingMaskOptions, NSColor, NSView, NSWindowOrderingMode,
};
use objc2_quartz_core::CACornerMask;
use objc2_web_kit::WKWebViewConfiguration;
use tracing::{error, warn};
use wry::WebViewExtMacOS;

use super::{Appearance, SiblingOrder, WebView};

struct SurfaceBackground;

impl SurfaceBackground {
    fn apply(webview: &wry::WebView, appearance: Appearance, transparent: bool) {
        let wk = webview.webview();
        let (red, green, blue, alpha) = appearance.surface_rgba(transparent);
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(
            red as f64 / 255.0,
            green as f64 / 255.0,
            blue as f64 / 255.0,
            alpha as f64 / 255.0,
        );
        unsafe {
            wk.setUnderPageBackgroundColor(Some(&color));
        }
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            layer.setOpaque(!transparent);
            layer.setBackgroundColor(Some(&color.CGColor()));
        }
    }
}

impl WebView {
    pub fn presentation_settled(&self, expected_opacity: f32) -> bool {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        if view.window().is_none() || view.isHidden() {
            return false;
        }
        let Some(layer) = view.layer() else {
            return false;
        };
        let Some(presented) = (unsafe { layer.presentationLayer() }) else {
            return false;
        };
        let model = layer.frame();
        let actual = presented.frame();
        PresentedLayer {
            frame: [
                actual.origin.x,
                actual.origin.y,
                actual.size.width,
                actual.size.height,
            ],
            opacity: presented.opacity(),
        }
        .matches(PresentedLayer {
            frame: [
                model.origin.x,
                model.origin.y,
                model.size.width,
                model.size.height,
            ],
            opacity: expected_opacity,
        })
    }

    pub(super) fn anchor_native_frame(&self) {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        let Some(parent) = (unsafe { view.superview() }) else {
            return;
        };
        let mask = if parent.isFlipped() {
            NSAutoresizingMaskOptions::ViewMaxYMargin
        } else {
            NSAutoresizingMaskOptions::ViewMinYMargin
        };
        view.setAutoresizingMask(mask);
    }

    pub(crate) fn platform_bounds(&self) -> Option<wry::Rect> {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        let parent = unsafe { view.superview() }?;
        let frame = view.frame();
        let top = if parent.isFlipped() {
            frame.origin.y
        } else {
            parent.frame().size.height - frame.origin.y - frame.size.height
        };
        Some(wry::Rect {
            position: wry::dpi::LogicalPosition::new(frame.origin.x, top).into(),
            size: wry::dpi::LogicalSize::new(frame.size.width, frame.size.height).into(),
        })
    }

    pub fn set_opacity(&self, opacity: f32) {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            return;
        };
        layer.setOpacity(opacity.clamp(0.0, 1.0));
    }

    pub fn order_among_siblings(&self, order: SiblingOrder) {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        let Some(parent) = (unsafe { view.superview() }) else {
            return;
        };
        let subviews = parent.subviews();
        let occupant = match order {
            SiblingOrder::Front => subviews.lastObject(),
            SiblingOrder::Back => subviews.firstObject(),
        };
        if occupant.is_some_and(|held| std::ptr::eq(&*held, view)) {
            return;
        }
        let mode = match order {
            SiblingOrder::Front => NSWindowOrderingMode::Above,
            SiblingOrder::Back => NSWindowOrderingMode::Below,
        };

        parent.addSubview_positioned_relativeTo(view, mode, None);
    }

    pub fn raise_above_layers(&self) {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            error!("vmux_page: the view has no layer, it will paint under its siblings");
            return;
        };
        layer.setZPosition(500.0);
    }

    pub fn set_corner_radius(&self, radius: f64, all_corners: bool) {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            warn!("vmux_page: the view has no layer, its corners will stay square");
            return;
        };
        let all = CACornerMask::LayerMinXMinYCorner
            | CACornerMask::LayerMaxXMinYCorner
            | CACornerMask::LayerMinXMaxYCorner
            | CACornerMask::LayerMaxXMaxYCorner;
        let bottom = if view.isFlipped() {
            CACornerMask::LayerMinXMaxYCorner | CACornerMask::LayerMaxXMaxYCorner
        } else {
            CACornerMask::LayerMinXMinYCorner | CACornerMask::LayerMaxXMinYCorner
        };
        layer.setCornerRadius(radius.max(0.0));
        layer.setMasksToBounds(true);
        layer.setMaskedCorners(if all_corners { all } else { bottom });
    }
    pub fn set_focus_ring(&self, width: f64, color_rgb: [f32; 3]) {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            warn!("vmux_page: the view has no layer, it cannot show a focus ring");
            return;
        };
        layer.setBorderWidth(width.max(0.0));
        if width <= 0.0 {
            return;
        }
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(
            color_rgb[0].clamp(0.0, 1.0) as f64,
            color_rgb[1].clamp(0.0, 1.0) as f64,
            color_rgb[2].clamp(0.0, 1.0) as f64,
            1.0,
        );
        layer.setBorderColor(Some(&color.CGColor()));
    }

    pub fn set_appearance(&self, appearance: Appearance) {
        self.appearance.set(appearance);
        let named = match appearance {
            Appearance::Light => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }),
            Appearance::Dark => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }),
            Appearance::System => None,
        };
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setAppearance(named.as_deref());
        SurfaceBackground::apply(&self.webview, appearance, self.page.get().transparent);
    }

    pub fn take_first_responder(&self) -> bool {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        let Some(window) = view.window() else {
            return false;
        };
        let holds_it = window
            .firstResponder()
            .is_some_and(|current| std::ptr::eq(&*current as *const _ as *const NSView, view));
        if holds_it {
            return true;
        }
        let accepted = window.makeFirstResponder(Some(view));
        if !accepted {
            warn!("vmux_page: the window refused first responder, this page cannot be typed in");
        }
        accepted
    }
}

struct PresentedLayer {
    frame: [f64; 4],
    opacity: f32,
}

impl PresentedLayer {
    fn matches(&self, expected: Self) -> bool {
        for (actual, expected) in self.frame.iter().zip(expected.frame) {
            if (*actual - expected).abs() > 1.0 || !actual.is_finite() {
                return false;
            }
        }
        (self.opacity - expected.opacity).abs() <= 0.001
    }
}

#[cfg(test)]
mod presentation_tests {
    use super::*;

    #[test]
    fn native_presentation_must_match_the_requested_frame_and_fade_phase() {
        for (actual, expected, settled) in [
            (0.0, 1.0, false),
            (0.0, 0.0, true),
            (1.0, 1.0, true),
            (0.5, 1.0, false),
        ] {
            assert_eq!(
                PresentedLayer {
                    frame: [0.0, 0.0, 800.0, 600.0],
                    opacity: actual
                }
                .matches(PresentedLayer {
                    frame: [0.0, 0.0, 800.0, 600.0],
                    opacity: expected
                }),
                settled
            );
        }
        assert!(
            !PresentedLayer {
                frame: [0.0, 0.0, 800.0, 600.0],
                opacity: 1.0
            }
            .matches(PresentedLayer {
                frame: [20.0, 0.0, 800.0, 600.0],
                opacity: 1.0
            })
        );
    }
}

pub struct LinkPreviewPolicy;

impl LinkPreviewPolicy {
    pub fn forbid(webview: &wry::WebView) {
        unsafe { webview.webview().setAllowsLinkPreview(false) };
    }
}

pub struct SharedWebProcess;

impl SharedWebProcess {
    pub fn configuration() -> Option<Retained<WKWebViewConfiguration>> {
        let marker = MainThreadMarker::new()?;
        let config = unsafe { WKWebViewConfiguration::new(marker) };
        pool::SharedPool::attach_to(&config, marker);
        Some(config)
    }
}

#[allow(deprecated)]
mod pool {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_web_kit::{WKProcessPool, WKWebViewConfiguration};
    use std::cell::OnceCell;

    pub struct SharedPool;

    impl SharedPool {
        pub fn attach_to(config: &WKWebViewConfiguration, marker: MainThreadMarker) {
            POOL.with(|pool| {
                let shared = pool.get_or_init(|| unsafe { WKProcessPool::new(marker) });
                unsafe { config.setProcessPool(shared) };
            });
        }
    }

    thread_local! {
        static POOL: OnceCell<Retained<WKProcessPool>> = const { OnceCell::new() };
    }
}
