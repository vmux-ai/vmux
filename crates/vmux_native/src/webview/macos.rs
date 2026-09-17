use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSView, NSWindowOrderingMode,
};
use objc2_web_kit::WKWebViewConfiguration;
use tracing::{error, warn};
use wry::WebViewExtMacOS;

use super::{Appearance, SiblingOrder, WebView};

impl WebView {
    pub fn order_among_siblings(&self, order: SiblingOrder) {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            layer.setZPosition(sibling_z_position(order));
        }
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
            error!("vmux_native: the view has no layer, it will paint under its siblings");
            return;
        };
        layer.setZPosition(500.0);
    }

    pub fn set_corner_radius(&self, radius: f64, all_corners: bool) {
        use objc2_quartz_core::CACornerMask;
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            warn!("vmux_native: the view has no layer, its corners will stay square");
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
        use objc2_app_kit::NSColor;
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            warn!("vmux_native: the view has no layer, it cannot show a focus ring");
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
        let named = match appearance {
            Appearance::Light => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }),
            Appearance::Dark => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }),
            Appearance::System => None,
        };
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setAppearance(named.as_deref());
    }

    pub fn take_first_responder(&self) {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        let Some(window) = view.window() else {
            return;
        };
        let holds_it = window
            .firstResponder()
            .is_some_and(|current| std::ptr::eq(&*current as *const _ as *const NSView, view));
        if holds_it {
            return;
        }
        if !window.makeFirstResponder(Some(view)) {
            warn!("vmux_native: the window refused first responder, this page cannot be typed in");
        }
    }
}

fn sibling_z_position(order: SiblingOrder) -> f64 {
    match order {
        SiblingOrder::Front => 500.0,
        SiblingOrder::Back => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_order_removes_the_layout_layer_override() {
        assert_eq!(sibling_z_position(SiblingOrder::Back), 0.0);
        assert_eq!(sibling_z_position(SiblingOrder::Front), 500.0);
    }
}

pub struct ImmediateAction;

impl ImmediateAction {
    pub fn forbid(webview: &wry::WebView) {
        use wry::WebViewExtMacOS;

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
