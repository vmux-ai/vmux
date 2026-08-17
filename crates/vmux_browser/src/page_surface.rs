//! A page whose components run in this process, painted by a `WKWebView` of its own.
//!
//! The layout was the first, and is no longer the only one: a start pane is the same arrangement in
//! a smaller rectangle. What differs between two native pages is a URL, a root component and the
//! document chrome they render into — [`SurfacePage`] — so that is all a caller supplies.
//!
//! What does *not* differ, and lives here: the view, the `vmux://` protocol that answers `__events`
//! and serves the shell, the IPC handler that hears the page back, and the `VirtualDom` in
//! [`dom`](self::dom).

use bevy::prelude::*;
use bevy_cef_core::prelude::{BinIpcEventRaw, Requester, embedded_page_host_of};

use self::dom::{PageWaker, SurfaceDom};
use self::protocol::{PageMessage, VmuxProtocol, WRY_HOST_SHIM};

pub(crate) mod dom;
mod protocol;

/// Everything that distinguishes one natively-hosted page from another.
///
/// A `const` per page, because a page names itself: the alternative is a registry the pages have to
/// be looked up in, which is one more thing to keep in agreement with them.
pub(crate) struct SurfacePage {
    pub(crate) url: &'static str,
    pub(crate) component: vmux_dioxus::PageComponent,
    /// The element the interpreter renders into, and its classes.
    pub(crate) root_id: &'static str,
    pub(crate) root_class: &'static str,
    /// Everything inside `<head>` — stylesheets, `<base>`, inline rules.
    pub(crate) head: &'static str,
    pub(crate) html_attributes: &'static str,
    pub(crate) body_class: &'static str,
    /// A page drawn over other content wants to see through itself; one filling a pane does not.
    pub(crate) transparent: bool,
}

/// One page's view and the dom that fills it.
pub(crate) struct PageSurface {
    webview: wry::WebView,
    dom: SurfaceDom,
}

impl PageSurface {
    /// Build the view for a page, as a child of the app's window.
    ///
    /// Returns `None` when the window is not up yet, which is a state the caller retries out of
    /// rather than an error.
    pub(crate) fn build(
        page: &'static SurfacePage,
        window: &impl wry::raw_window_handle::HasWindowHandle,
        entity: Entity,
        bounds: wry::Rect,
        bin_ipc: async_channel::Sender<BinIpcEventRaw>,
        requester: Requester,
        waker: PageWaker,
    ) -> Result<Self, wry::Error> {
        let dom = SurfaceDom::mount(
            page.component,
            bin_ipc.clone(),
            entity,
            embedded_page_host_of(page.url).unwrap_or_default(),
            waker,
        );
        let message = PageMessage::new(page, bin_ipc, entity, dom.clone());
        let serve = dom.clone();
        let webview = wry::WebViewBuilder::new()
            .with_transparent(page.transparent)
            .with_initialization_script(WRY_HOST_SHIM)
            .with_asynchronous_custom_protocol("vmux".into(), move |_id, request, responder| {
                VmuxProtocol::serve(page, &serve, &requester, request, responder);
            })
            .with_ipc_handler(move |request| message.receive(request.body()))
            .with_url(page.url)
            .with_bounds(bounds)
            .build_as_child(window)?;

        Ok(Self { webview, dom })
    }

    pub(crate) fn dom(&self) -> &SurfaceDom {
        &self.dom
    }

    pub(crate) fn set_bounds(&self, bounds: wry::Rect) {
        if let Err(error) = self.webview.set_bounds(bounds) {
            error!("page_surface: set_bounds failed: {error}");
        }
    }

    /// Evaluate the next batch of edits, then whatever scripts the page asked for.
    ///
    /// The scripts go after the batch, so an element a component just asked to focus exists to be
    /// found.
    pub(crate) fn render(&self) {
        if let Some(script) = self.dom.next_batch()
            && let Err(error) = self.webview.evaluate_script(script.as_str())
        {
            error!("page_surface: applying an edit batch failed: {error}");
        }
        for script in self.dom.take_pending_scripts() {
            if let Err(error) = self.webview.evaluate_script(&script) {
                error!("page_surface: a page script failed: {error}");
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn webview(&self) -> &wry::WebView {
        &self.webview
    }
}
