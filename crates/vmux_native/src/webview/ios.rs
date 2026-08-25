use objc2_ui_kit::{UIUserInterfaceStyle, UIView};
use tracing::warn;
use wry::WebViewExtIOS;

use super::{Appearance, SiblingOrder, WebView};

impl WebView {
    pub fn order_among_siblings(&self, order: SiblingOrder) {
        let wk = self.webview.webview();
        let view: &UIView = &wk;
        let Some(parent) = view.superview() else {
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
        match order {
            SiblingOrder::Front => parent.bringSubviewToFront(view),
            SiblingOrder::Back => parent.sendSubviewToBack(view),
        }
    }

    pub fn raise_above_layers(&self) {
        let wk = self.webview.webview();
        let view: &UIView = &wk;
        view.layer().setZPosition(500.0);
    }

    pub fn set_appearance(&self, appearance: Appearance) {
        let style = match appearance {
            Appearance::Light => UIUserInterfaceStyle::Light,
            Appearance::Dark => UIUserInterfaceStyle::Dark,
            Appearance::System => UIUserInterfaceStyle::Unspecified,
        };
        let wk = self.webview.webview();
        let view: &UIView = &wk;
        view.setOverrideUserInterfaceStyle(style);
    }

    pub fn take_first_responder(&self) {
        let wk = self.webview.webview();
        let view: &UIView = &wk;
        if view.isFirstResponder() {
            return;
        }
        if !view.becomeFirstResponder() {
            warn!("vmux_native: the view refused first responder, this page cannot be typed in");
        }
    }
}
