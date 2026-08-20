//! A page's view: the webview that paints it, and the dom that fills it.
//!
//! Everything a page asks of its view is here. What that view *is* — an `NSView` or a `UIView`,
//! ordered and focused through different selectors — lives in the two siblings, so nothing in
//! this file has to say which platform it is on.

#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "macos")]
mod macos;

use tracing::error;

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

/// Which end of its parent's subview array a view sits at.
///
/// Not where it is drawn. Hit testing walks the array back to front and knows nothing of
/// `zPosition`, so a view's place in it decides who the platform hands a click to and nothing
/// else — [`PageSurface::raise_above_layers`] is what decides what is painted over what.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SiblingOrder {
    /// Last, and so the first asked for any point it covers.
    Front,
    /// First, and so asked only for the points no sibling covers.
    Back,
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
        let message = PageMessage::new(page, embed.outbox, dom.reads(), embed.waker);
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
}
