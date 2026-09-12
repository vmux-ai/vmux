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

use std::cell::Cell;
use std::rc::Rc;
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
    page: Rc<Cell<&'static NativePage>>,
    outbox: Rc<dyn Outbox>,
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
        let current_page = Rc::new(Cell::new(page));
        let outbox = embed.outbox.clone();
        let message = PageMessage::new(
            current_page.clone(),
            outbox.clone(),
            dom.reads(),
            embed.waker,
        );
        let routes = PageRoutes::new(current_page.clone(), dom.clone(), embed.assets);
        let title_outbox = outbox.clone();
        let title_page = current_page.clone();
        let builder = wry::WebViewBuilder::new();
        #[cfg(target_os = "macos")]
        let builder = match macos::SharedWebProcess::configuration() {
            Some(config) => {
                use wry::WebViewBuilderExtMacos;
                builder.with_webview_configuration(config)
            }
            None => builder,
        };
        let webview = builder
            .with_transparent(page.transparent)
            .with_initialization_script(WRY_HOST_SHIM)
            .with_asynchronous_custom_protocol("vmux".into(), move |_id, request, responder| {
                routes.serve(request, responder);
            })
            .with_document_title_changed_handler(move |title| {
                if title_page.get().reports_title {
                    title_outbox.set_title(&title);
                }
            })
            .with_ipc_handler(move |request| message.receive(request.body()))
            .with_url(page.document_url())
            .with_bounds(bounds)
            .build_as_child(window)?;
        #[cfg(target_os = "macos")]
        macos::ImmediateAction::forbid(&webview);
        Ok(Self {
            webview,
            dom,
            page: current_page,
            outbox,
        })
    }

    pub fn navigate(&self, page: &'static NativePage, instance: crate::Instance) {
        let document_changed = self.page.get().document_url() != page.document_url();
        self.page.set(page);
        self.outbox.set_page(page.url);
        self.dom.remount(page.component, instance);
        if document_changed && let Err(error) = self.webview.load_url(page.document_url()) {
            error!("vmux_native: navigation failed for {}: {error}", page.url);
        }
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
