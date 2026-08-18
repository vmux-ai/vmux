//! A page's view: the webview that paints it, and the dom that fills it.

use tracing::{error, warn};

use crate::dom::SurfaceDom;
use crate::embed::Embedding;
use crate::page::NativePage;
use crate::report::PageMessage;
use crate::route::PageRoutes;
use crate::shim::WRY_HOST_SHIM;

/// What a view's `prefers-color-scheme` should answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Appearance {
    Light,
    Dark,
    System,
}

/// One page running in this process, painted by a webview of its own.
pub struct PageSurface {
    webview: wry::WebView,
    dom: SurfaceDom,
}

impl PageSurface {
    /// Build the view for a page, as a child of a window the host already has.
    pub fn build(
        page: &'static NativePage,
        window: &impl wry::raw_window_handle::HasWindowHandle,
        bounds: wry::Rect,
        embed: Embedding,
        instance: crate::Instance,
    ) -> Result<Self, wry::Error> {
        let dom = SurfaceDom::mount(page.component, instance, &embed);
        let message = PageMessage::new(page, embed.outbox);
        let routes = PageRoutes::new(page, dom.clone(), embed.assets);
        let webview = wry::WebViewBuilder::new()
            .with_transparent(page.transparent)
            .with_initialization_script(WRY_HOST_SHIM)
            .with_asynchronous_custom_protocol("vmux".into(), move |_id, request, responder| {
                routes.serve(request, responder);
            })
            .with_ipc_handler(move |request| message.receive(request.body()))
            .with_url(page.url)
            .with_bounds(bounds)
            .build_as_child(window)?;

        Ok(Self { webview, dom })
    }

    pub fn set_bounds(&self, bounds: wry::Rect) {
        if let Err(error) = self.webview.set_bounds(bounds) {
            error!("vmux_native: set_bounds failed: {error}");
        }
    }

    /// Whether the view is on screen at all.
    ///
    /// It has to be said explicitly: a host that leaves a hidden page out of its placement pass
    /// rather than giving it an empty rectangle would otherwise leave the view at whatever
    /// rectangle it last had.
    pub fn set_visible(&self, visible: bool) {
        if let Err(error) = self.webview.set_visible(visible) {
            error!("vmux_native: set_visible failed: {error}");
        }
    }

    /// Hand the page whatever this frame produced, if it is waiting for it.
    ///
    /// Nothing is evaluated. The page holds a standing request for its next batch and this answers
    /// it, so the interpreter's bytes travel as bytes and the only thing reaching the document is
    /// what the document asked for.
    pub fn render(&self) {
        self.dom.flush_to_page();
    }

    /// Hand the page an event the host raised, for whatever it registered against that id.
    pub fn deliver(&self, id: &str, payload: &[u8]) {
        self.dom.deliver(id, payload);
    }

    /// Put the view last in its parent's subview array, so clicks land on it.
    ///
    /// `hitTest:` walks siblings back to front and knows nothing of `zPosition`, so a view
    /// painting above another is not the same as that view receiving the pointer. Anything the
    /// host adds to the same parent afterwards lands in front — visibly on top, and taking every
    /// click aimed at what is drawn over it.
    ///
    /// Reasserted rather than done once, because the next sibling to arrive undoes it again.
    pub fn raise_above_siblings(&self) {
        use objc2_app_kit::{NSView, NSWindowOrderingMode};
        use wry::WebViewExtMacOS;

        let wk = self.webview.webview();
        let view: &NSView = &wk;
        // `superview` is unsafe only because it hands out a reference AppKit could invalidate; it
        // is read and dropped inside this call, on the thread that owns the hierarchy.
        let Some(parent) = (unsafe { view.superview() }) else {
            return;
        };
        let subviews = parent.subviews();
        let frontmost = subviews.lastObject();
        if frontmost.is_some_and(|front| std::ptr::eq(&*front, view)) {
            return;
        }

        parent.addSubview_positioned_relativeTo(view, NSWindowOrderingMode::Above, None);
    }

    /// Outrank the layers of whatever else is drawn in this window.
    ///
    /// A sibling's `CALayer` can carry a `zPosition`, and subview order cannot outrank one — only
    /// another `zPosition` can. This is that other one.
    ///
    /// It buys painting and nothing else. A layer's `zPosition` is invisible to `hitTest:`, which
    /// walks the subview array back to front, so this does not move a single click;
    /// [`Self::raise_above_siblings`] is what does.
    pub fn raise_above_layers(&self) {
        use objc2_app_kit::NSView;
        use wry::WebViewExtMacOS;

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
        use objc2_app_kit::{
            NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua,
            NSAppearanceNameDarkAqua, NSView,
        };
        use wry::WebViewExtMacOS;

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
        use objc2_app_kit::NSView;
        use wry::WebViewExtMacOS;

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
