//! The half of this crate that needs a webview to exist, and the webview itself.
//!
//! Gated once, here, rather than an attribute per module: everything below wants the same thing —
//! a window to parent a `wry` webview into — so what decides whether it is compiled is one fact,
//! and it should be written once. The page half above this is plain Rust and builds everywhere.
//!
//! Everything a page asks of its webview is here. What that webview *is* — an `NSView` or a
//! `UIView`, ordered and focused through different selectors — lives in the two platform
//! siblings, so nothing in this file has to say which platform it is on.

mod dom;
mod dom_request;
mod element;
mod embed;
mod event_selection;
mod frame;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "macos")]
mod macos;
mod measurement;
mod report;
mod route;
mod shim;

pub use embed::{AssetReply, Assets, Embedding, Outbox, Wake};

use tracing::error;

use crate::page::NativePage;
use crate::webview::dom::Dom;
use crate::webview::report::PageMessage;
use crate::webview::route::PageRoutes;
use crate::webview::shim::WRY_HOST_SHIM;

/// What a webview's `prefers-color-scheme` should answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Appearance {
    Light,
    Dark,
    System,
}

/// Which end of its parent's subview array a webview sits at.
///
/// Not where it is drawn. Hit testing walks the array back to front and knows nothing of
/// `zPosition`, so a view's place in it decides who the platform hands a click to and nothing
/// else — [`WebView::raise_above_layers`] is what decides what is painted over what.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SiblingOrder {
    /// Last, and so the first asked for any point it covers.
    Front,
    /// First, and so asked only for the points no sibling covers.
    Back,
}

/// One page running in this process, painted by a webview of its own.
pub struct WebView {
    webview: wry::WebView,
    dom: Dom,
}

impl WebView {
    /// Build the webview for a page, as a child of a window the host already has.
    pub fn build(
        page: &'static NativePage,
        window: &impl wry::raw_window_handle::HasWindowHandle,
        bounds: wry::Rect,
        embed: Embedding,
        instance: crate::Instance,
    ) -> Result<Self, wry::Error> {
        let dom = Dom::mount(page.component, instance, &embed);
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

    /// Whether the webview is on screen at all.
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

// wry calls `objc2::exception::catch`, whose C shim ships as a static archive built by
// `objc2-exception-helper`. Cargo puts that archive's directory on the link path but its `-l`
// never reaches the binary, so the reference resolves to nothing. Naming the library here is what
// pulls it in.
#[link(name = "objc2_exception_helper_0_1", kind = "static")]
unsafe extern "C" {}
