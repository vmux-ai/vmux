use objc2::rc::Retained;
use objc2_ui_kit::{UIUserInterfaceStyle, UIView};
use tracing::warn;
use wry::WebViewExtIOS;

use super::{Appearance, SiblingOrder, WebView};

impl WebView {
    pub fn ui_view(&self) -> Retained<UIView> {
        Retained::into_super(Retained::into_super(self.webview.webview()))
    }

    pub fn fill_parent(&self) {
        use objc2_ui_kit::UIViewAutoresizing;

        let view = self.ui_view();
        let Some(parent) = view.superview() else {
            warn!("vmux_native: a view with no parent cannot fill one");
            return;
        };
        view.setAutoresizingMask(
            UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
        );
        view.setFrame(parent.bounds());
    }

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
