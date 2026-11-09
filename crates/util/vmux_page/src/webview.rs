use crate::page::Page;
use crate::webview::dom::Dom;
use crate::webview::report::PageMessage;
use crate::webview::route::PageRoutes;
use crate::webview::shim::Shim;
pub use embed::{AssetReply, Assets, Embedding, LifecycleSink, LifecycleUpdate, Outbox, Wake};
pub(crate) use frame::InitialFrame;
use std::cell::Cell;
use std::rc::Rc;
use tracing::error;

#[cfg(target_os = "macos")]
use wry::WebViewBuilderExtMacos;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Appearance {
    Light,
    Dark,
    System,
}

impl Appearance {
    pub(crate) fn surface_rgba(self, transparent: bool) -> (u8, u8, u8, u8) {
        if transparent {
            return (0, 0, 0, 0);
        }
        match self {
            Self::Light => (240, 240, 240, 255),
            Self::Dark | Self::System => (20, 20, 20, 255),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SiblingOrder {
    Front,
    Back,
}

pub struct WebView {
    webview: wry::WebView,
    dom: Dom,
    page: Rc<Cell<&'static Page>>,
    appearance: Rc<Cell<Appearance>>,
    outbox: Rc<dyn Outbox>,
    blocked: Rc<Cell<bool>>,
    ipc_epoch: Rc<Cell<u64>>,
    interactive_sync_pending: Rc<Cell<bool>>,
    interactive_sync_attempted: Rc<Cell<bool>>,
    waker: Rc<dyn Wake>,
}

impl WebView {
    pub fn build(
        page: &'static Page,
        window: &impl wry::raw_window_handle::HasWindowHandle,
        bounds: wry::Rect,
        embed: Embedding,
        instance: crate::Instance,
        appearance: Appearance,
        interactive: bool,
    ) -> Result<Self, wry::Error> {
        let blocked = Rc::new(Cell::new(!interactive));
        let ipc_epoch = embed.ipc_epoch.clone();
        let dom = Dom::mount(page.component, instance, &embed, blocked.clone());
        let lifecycle = embed.lifecycle.clone();
        let current_page = Rc::new(Cell::new(page));
        let current_appearance = Rc::new(Cell::new(appearance));
        let outbox = embed.outbox.clone();
        let interactive_sync_pending = Rc::new(Cell::new(false));
        let interactive_sync_attempted = Rc::new(Cell::new(false));
        let loaded_sync = interactive_sync_pending.clone();
        let loaded_sync_attempted = interactive_sync_attempted.clone();
        let loaded_waker = embed.waker.clone();
        let waker = embed.waker.clone();
        let loaded_page = current_page.clone();
        let message = PageMessage::new(
            current_page.clone(),
            outbox.clone(),
            dom.navigation_revision(),
            ipc_epoch.clone(),
            blocked.clone(),
            dom.reads(),
            embed.waker,
            lifecycle,
        );
        let routes = Rc::new(PageRoutes::new(
            current_page.clone(),
            current_appearance.clone(),
            dom.clone(),
            embed.assets,
        ));
        let frame_capability_script = format!(
            "globalThis.__vmuxFrameCapability='{}';",
            dom.frame_capability()
        );
        let builder = wry::WebViewBuilder::new();
        #[cfg(target_os = "macos")]
        let builder = match macos::SharedWebProcess::configuration() {
            Some(config) => builder.with_webview_configuration(config),
            None => builder,
        };
        let navigation_page = current_page.clone();
        let mut builder = builder
            .with_transparent(page.transparent)
            .with_background_color(appearance.surface_rgba(page.transparent))
            .with_visible(false)
            .with_initialization_script(&frame_capability_script)
            .with_initialization_script(Shim::source())
            .with_navigation_handler(move |url| {
                url.starts_with("vmux://")
                    || PageRoutes::serves_document(&url, navigation_page.get())
            });
        for scheme in ["vmux", "git"] {
            let routes = routes.clone();
            builder = builder.with_asynchronous_custom_protocol(
                scheme.into(),
                move |_id, request, responder| {
                    routes.serve(request, responder);
                },
            );
        }
        let webview = builder
            .with_on_page_load_handler(move |event, url| {
                if matches!(event, wry::PageLoadEvent::Finished)
                    && PageRoutes::serves_document(&url, loaded_page.get())
                {
                    loaded_sync.set(true);
                    loaded_sync_attempted.set(false);
                    loaded_waker.wake();
                }
            })
            .with_ipc_handler(move |request| {
                message.receive(&request.uri().to_string(), request.body())
            })
            .with_url(page.document_url())
            .with_bounds(bounds)
            .build_as_child(window)?;
        #[cfg(target_os = "macos")]
        macos::LinkPreviewPolicy::forbid(&webview);
        let surface = Self {
            webview,
            dom,
            page: current_page,
            appearance: current_appearance,
            outbox,
            blocked,
            ipc_epoch,
            interactive_sync_pending,
            interactive_sync_attempted,
            waker,
        };
        #[cfg(target_os = "macos")]
        surface.anchor_native_frame();
        surface.set_appearance(appearance);
        Ok(surface)
    }

    pub fn navigate(&self, page: &'static Page, url: &str, instance: crate::Instance) -> bool {
        let current = self.page.get();
        let document_changed = current.document_url() != page.document_url();
        let remounted = !current.preserves_dom_for(page);
        self.page.set(page);
        if remounted {
            self.dom.remount(page.component, instance);
        } else {
            self.dom.update_instance(instance);
        }
        self.outbox.set_page(url);
        if document_changed && let Err(error) = self.webview.load_url(page.document_url()) {
            error!("vmux_page: navigation failed for {}: {error}", page.url);
        }
        remounted
    }

    pub fn navigation_revision(&self) -> u64 {
        self.dom.navigation_revision().get()
    }

    pub fn reload(&self) -> bool {
        match self.webview.reload() {
            Ok(()) => true,
            Err(error) => {
                error!(
                    "vmux_page: reload failed for {}: {error}",
                    self.page.get().url
                );
                false
            }
        }
    }

    pub fn set_url(&self, url: &str) {
        self.outbox.set_page(url);
    }

    pub fn set_bounds(&self, bounds: wry::Rect) {
        if let Err(error) = self.webview.set_bounds(bounds) {
            error!("vmux_page: set_bounds failed: {error}");
        }
    }

    pub fn bounds(&self) -> Option<wry::Rect> {
        #[cfg(target_os = "macos")]
        {
            self.platform_bounds()
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.webview.bounds().ok()
        }
    }

    pub fn set_page_scale(&self, scale: f64) {
        if let Err(error) = self.webview.zoom(scale) {
            error!("vmux_page: zoom failed: {error}");
        }
    }

    pub fn set_visible(&self, visible: bool) {
        if let Err(error) = self.webview.set_visible(visible) {
            error!("vmux_page: set_visible failed: {error}");
        }
    }

    pub fn render(&self) {
        self.sync_interactive_state();
        self.dom.flush_to_page();
    }

    pub fn evaluate_script(&self, script: &str) -> bool {
        match self.webview.evaluate_script(script) {
            Ok(()) => true,
            Err(error) => {
                error!("vmux_page: script evaluation failed: {error}");
                false
            }
        }
    }

    pub fn set_interactive(&self, interactive: bool) -> u64 {
        let blocked = !interactive;
        if self.blocked.replace(blocked) != blocked {
            if blocked {
                self.dom.cancel_pending_reads();
            }
            let previous_epoch = self.ipc_epoch.get();
            let epoch = previous_epoch.wrapping_add(1).max(1);
            self.ipc_epoch.set(epoch);
            self.dom.interaction_epoch_changed(previous_epoch);
        }
        self.interactive_sync_pending.set(true);
        self.interactive_sync_attempted.set(false);
        self.sync_interactive_state();
        self.ipc_epoch.get()
    }

    pub fn sync_interactive_state(&self) {
        if !self.interactive_sync_pending.get() {
            return;
        }
        if self.interactive_sync_attempted.replace(true) {
            return;
        }
        let inert = if self.blocked.get() { "true" } else { "false" };
        let script = format!(
            "(()=>{{globalThis.__vmuxIpcEpoch={};const apply=()=>{{if({inert}&&document.activeElement instanceof HTMLElement)document.activeElement.blur();if(document.body)document.body.inert={inert};}};if(document.body)apply();else document.addEventListener('DOMContentLoaded',apply,{{once:true}});}})();",
            self.ipc_epoch.get()
        );
        if self.evaluate_script(&script) {
            self.interactive_sync_pending.set(false);
        } else {
            self.interactive_sync_attempted.set(false);
            self.waker.wake();
        }
    }

    pub fn ipc_epoch(&self) -> u64 {
        self.ipc_epoch.get()
    }

    pub fn deliver(&self, id: &str, payload: &[u8]) -> bool {
        self.dom.deliver(id, payload)
    }
}

#[link(name = "objc2_exception_helper_0_1", kind = "static")]
unsafe extern "C" {}
