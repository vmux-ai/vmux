//! A page's view: the webview that paints it, and the dom that fills it.

use std::rc::Rc;

use tracing::{error, warn};

use crate::dom::SurfaceDom;
use crate::embed::{Assets, Embedding};
use crate::page::NativePage;
use crate::protocol::{PageMessage, VmuxProtocol, WRY_HOST_SHIM};

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
    ) -> Result<Self, wry::Error> {
        let dom = SurfaceDom::mount(page.component, &embed);
        let message = PageMessage::new(page, embed.outbox, dom.clone());
        let assets: Rc<dyn Assets> = embed.assets;
        let serve = dom.clone();
        let webview = wry::WebViewBuilder::new()
            .with_transparent(page.transparent)
            .with_initialization_script(WRY_HOST_SHIM)
            .with_asynchronous_custom_protocol("vmux".into(), move |_id, request, responder| {
                VmuxProtocol::serve(page, &serve, assets.as_ref(), request, responder);
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

    /// Evaluate the next batch of edits, then whatever scripts the page asked for.
    ///
    /// The scripts go after the batch, so an element a component just asked to focus exists to be
    /// found.
    pub fn render(&self) {
        if let Some(script) = self.dom.next_batch()
            && let Err(error) = self.webview.evaluate_script(script.as_str())
        {
            error!("vmux_native: applying an edit batch failed: {error}");
        }
        for script in self.dom.take_pending_scripts() {
            if let Err(error) = self.webview.evaluate_script(&script) {
                error!("vmux_native: a page script failed: {error}");
            }
        }
    }

    /// Hand the page an event the host raised, for whatever it registered against that id.
    pub fn deliver(&self, id: &str, payload: &[u8]) {
        self.dom.deliver(id, payload);
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

    pub fn webview(&self) -> &wry::WebView {
        &self.webview
    }
}
