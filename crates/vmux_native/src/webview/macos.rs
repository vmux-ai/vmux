//! The four things a page's view can only be asked of an `NSView`.

use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSView, NSWindowOrderingMode,
};
use tracing::{error, warn};
use wry::WebViewExtMacOS;

use super::{Appearance, SiblingOrder, WebView};

impl WebView {
    /// Put the view at one end of its parent's subview array, so AppKit asks it first or last.
    ///
    /// `hitTest:` walks siblings back to front and knows nothing of `zPosition`, so a view
    /// painting above another is not the same as that view receiving the pointer. A view at the
    /// front takes every click aimed at what it is drawn over, whether or not its document has
    /// anything there to receive one; a view at the back is asked only where no sibling answers.
    ///
    /// Reasserted rather than done once, because every sibling the host adds afterwards lands in
    /// front and undoes it.
    pub fn order_among_siblings(&self, order: SiblingOrder) {
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        // `superview` is unsafe only because it hands out a reference AppKit could invalidate; it
        // is read and dropped inside this call, on the thread that owns the hierarchy.
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

    /// Outrank the layers of whatever else is drawn in this window.
    ///
    /// A sibling's `CALayer` can carry a `zPosition`, and subview order cannot outrank one — only
    /// another `zPosition` can. This is that other one.
    ///
    /// It buys painting and nothing else. A layer's `zPosition` is invisible to `hitTest:`, which
    /// walks the subview array back to front, so this does not move a single click;
    /// [`WebView::order_among_siblings`] is what does. That independence is the point: a page
    /// can paint over every pane and still let their clicks through.
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

    /// Make `prefers-color-scheme` inside the view answer with something other than the system's.
    ///
    /// A `WKWebView` has no colour-scheme override and inherits its `NSAppearance` from the
    /// window, so left alone it renders dark on a dark desktop whatever the app has been set to.
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

    /// Give this view AppKit first responder, so its DOM receives keys.
    ///
    /// A host cannot do this from the outside. Its own focus routes address a browser it owns or
    /// the window itself, and neither can reach a `WKWebView` standing in for a page.
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
