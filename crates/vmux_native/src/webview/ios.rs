//! The four things a page's view can only be asked of a `UIView`.

use objc2_ui_kit::{UIUserInterfaceStyle, UIView};
use tracing::warn;
use wry::WebViewExtIOS;

use super::{Appearance, SiblingOrder, WebView};

impl WebView {
    /// Put the view at one end of its parent's subview array, so UIKit asks it first or last.
    ///
    /// `hitTest:` walks siblings back to front and knows nothing of `zPosition`, so a view
    /// painting above another is not the same as that view receiving the touch. A view at the
    /// front takes every touch aimed at what it is drawn over, whether or not its document has
    /// anything there to receive one; a view at the back is asked only where no sibling answers.
    ///
    /// Reasserted rather than done once, because every sibling the host adds afterwards lands in
    /// front and undoes it.
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

    /// Outrank the layers of whatever else is drawn in this window.
    ///
    /// A sibling's `CALayer` can carry a `zPosition`, and subview order cannot outrank one — only
    /// another `zPosition` can. This is that other one.
    ///
    /// It buys painting and nothing else. A layer's `zPosition` is invisible to `hitTest:`, which
    /// walks the subview array back to front, so this does not move a single touch;
    /// [`WebView::order_among_siblings`] is what does.
    ///
    /// No `setWantsLayer` and no absent-layer path, unlike AppKit: every `UIView` is layer-backed
    /// from birth.
    pub fn raise_above_layers(&self) {
        let wk = self.webview.webview();
        let view: &UIView = &wk;
        view.layer().setZPosition(500.0);
    }

    /// Make `prefers-color-scheme` inside the view answer with something other than the system's.
    ///
    /// A `WKWebView` has no colour-scheme override of its own and takes the style it inherits, so
    /// left alone it renders dark on a dark phone whatever the app has been set to.
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

    /// Give this view UIKit first responder, so its DOM receives keys from a hardware keyboard.
    ///
    /// Unlike AppKit there is no window to ask: a `UIResponder` is told to become first responder
    /// and answers whether it did.
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
