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

pub use embed::{AssetReply, Assets, Embedding, HostLayer, Outbox, Wake};

use tracing::error;

use crate::page::NativePage;
use crate::webview::dom::Dom;
use crate::webview::report::PageMessage;
use crate::webview::route::PageRoutes;
use crate::webview::shim::WRY_HOST_SHIM;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Appearance {
    Light,
    Dark,
    System,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SiblingOrder {
    Front,
    Back,
}

pub struct WebView {
    webview: wry::WebView,
    dom: Dom,
}

impl WebView {
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
        let mut builder = wry::WebViewBuilder::new();
        if let Some(colour) = page.background {
            builder = builder.with_background_color(colour);
        }
        let webview = builder
            .with_transparent(page.transparent)
            .with_initialization_script(WRY_HOST_SHIM)
            .with_asynchronous_custom_protocol("vmux".into(), move |_id, request, responder| {
                routes.serve(request, responder);
            })
            .with_ipc_handler(move |request| message.receive(request.body()))
            .with_url(page.document_url())
            .with_bounds(bounds)
            .build_as_child(window)?;

        Ok(Self { webview, dom })
    }

    pub fn set_bounds(&self, bounds: wry::Rect) {
        if let Err(error) = self.webview.set_bounds(bounds) {
            error!("vmux_native: set_bounds failed: {error}");
        }
    }

    pub fn set_page_scale(&self, scale: f64) {
        if let Err(error) = self.webview.zoom(scale) {
            error!("vmux_native: zoom failed: {error}");
        }
    }
    pub fn paint(&self, colour: (u8, u8, u8, u8)) {
        if let Err(error) = self.webview.set_background_color(colour) {
            error!("vmux_native: set_background_color failed: {error}");
        }
    }

    pub fn set_visible(&self, visible: bool) {
        if let Err(error) = self.webview.set_visible(visible) {
            error!("vmux_native: set_visible failed: {error}");
        }
    }

    pub fn render(&self) {
        self.dom.flush_to_page();
    }

    pub fn deliver(&self, id: &str, payload: &[u8]) {
        self.dom.deliver(id, payload);
    }
}

#[link(name = "objc2_exception_helper_0_1", kind = "static")]
unsafe extern "C" {}
