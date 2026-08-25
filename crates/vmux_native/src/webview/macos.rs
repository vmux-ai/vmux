use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSView, NSWindowOrderingMode,
};
use tracing::{error, warn};
use wry::WebViewExtMacOS;

use super::{Appearance, SiblingOrder, WebView};

impl WebView {
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
