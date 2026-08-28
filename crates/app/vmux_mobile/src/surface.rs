#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use bevy_winit::{EventLoopProxy, EventLoopProxyWrapper, WINIT_WINDOWS, WinitUserEvent};
use vmux_native::{AssetReply, Assets, Instance, NativePage, Outbox, Wake, WebView};
use vmux_ui::hooks::EventListenerError;
use vmux_ui::hooks::transport::PageHost;
use wry::Rect;

const INDEX_CSS: &str = include_str!("../assets/tailwind.out.css");
const THEME_CSS: &str = include_str!("../../../vmux_ui/assets/theme.css");

#[derive(Clone)]
pub(crate) struct PageWaker(Option<EventLoopProxy<WinitUserEvent>>);

impl PageWaker {
    pub(crate) fn of(proxy: Option<&EventLoopProxyWrapper>) -> Self {
        Self(proxy.map(|proxy| (*proxy).clone()))
    }
}

impl Wake for PageWaker {
    fn wake(&self) {
        let Some(proxy) = self.0.as_ref() else {
            return;
        };
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

pub(crate) struct HostOutbox;

impl Outbox for HostOutbox {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        let Some(host) = crate::page_host::installed() else {
            return Err(EventListenerError::Unsupported);
        };
        host.send(id, bytes)
    }
}

pub(crate) struct BundledAssets;

impl Assets for BundledAssets {
    fn fetch(&self, url: &str, reply: AssetReply) {
        let body = match url.rsplit('/').next() {
            Some("index.css") => INDEX_CSS,
            Some("theme.css") => THEME_CSS,
            _ => {
                tracing::warn!(%url, "surface: no bundled asset");
                reply.fail("no such asset");
                return;
            }
        };
        reply.respond(200, "text/css", body.as_bytes().to_vec());
    }
}

pub(crate) struct PhoneLayer;

impl vmux_native::HostLayer for PhoneLayer {
    fn wrap(&self, inner: Rc<dyn PageHost>) -> Rc<dyn PageHost> {
        Rc::new(Routed { inner })
    }
}

struct Routed {
    inner: Rc<dyn PageHost>,
}

impl PageHost for Routed {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        let Some(host) = crate::page_host::installed() else {
            return Err(EventListenerError::Unsupported);
        };
        host.send(id, bytes)
    }

    fn listen(
        &self,
        id: &str,
        on_bytes: vmux_ui::hooks::transport::BytesListener,
    ) -> Result<(), EventListenerError> {
        let Some(host) = crate::page_host::installed() else {
            tracing::warn!(
                id,
                "surface: a page subscribed before the host was installed"
            );
            return Err(EventListenerError::Unsupported);
        };
        host.listen(id, on_bytes)
    }

    fn focus_element(&self, element_id: &str) {
        self.inner.focus_element(element_id);
    }

    fn scroll_element_into_view(&self, element_id: &str) {
        self.inner.scroll_element_into_view(element_id);
    }

    fn reveal_first_rendered(&self, element_ids: &[&str], centered: bool) {
        self.inner.reveal_first_rendered(element_ids, centered);
    }

    fn text_offset_at(
        &self,
        element_id: &str,
        x: f64,
        y: f64,
    ) -> vmux_ui::hooks::transport::TextOffsetAnswer {
        self.inner.text_offset_at(element_id, x, y)
    }

    fn select_element_text(&self, element_id: &str) {
        self.inner.select_element_text(element_id);
    }

    fn clear_element_text(&self, element_id: &str) {
        self.inner.clear_element_text(element_id);
    }

    fn toggle_media(&self, element_id: &str) {
        self.inner.toggle_media(element_id);
    }

    fn offer_element_text(&self, element_id: &str) {
        self.inner.offer_element_text(element_id);
    }

    fn place_caret(&self, element_id: &str, byte: usize) {
        self.inner.place_caret(element_id, byte);
    }

    fn event_field_selection(&self, element_id: &str) -> (usize, usize) {
        self.inner.event_field_selection(element_id)
    }

    fn event_document_has_selection(&self) -> bool {
        self.inner.event_document_has_selection()
    }

    fn write_to_clipboard(&self, text: &str) {
        self.inner.write_to_clipboard(text);
    }
}

pub(crate) fn embedding(waker: PageWaker) -> vmux_native::Embedding {
    vmux_native::Embedding {
        outbox: Rc::new(HostOutbox),
        assets: Rc::new(BundledAssets),
        waker: Rc::new(waker),
        layer: Some(Rc::new(PhoneLayer)),
    }
}

pub(crate) static START: NativePage =
    NativePage::pane("vmux://start/", vmux_start::page::StartPage);
pub(crate) static TEAM: NativePage = NativePage::pane("vmux://team/", vmux_team::page::Page);
pub(crate) static AGENT: NativePage =
    NativePage::pane("vmux://agent/", vmux_chat::page::Page).owning_subtree();

thread_local! {
    static WAKER: RefCell<Option<PageWaker>> = const { RefCell::new(None) };
}

pub(crate) struct Surfaces;

impl Surfaces {
    pub(crate) fn wake_with(waker: PageWaker) {
        WAKER.with_borrow_mut(|slot| *slot = Some(waker));
    }

    pub(crate) fn build(page: &'static NativePage, seat: Instance) -> Option<WebView> {
        let waker = WAKER.with_borrow(Clone::clone)?;
        let built = WINIT_WINDOWS.with(|windows| {
            let windows = windows.borrow();
            let window = windows.windows.values().next()?;
            Some(WebView::build(
                page,
                &**window,
                Rect::default(),
                embedding(waker),
                seat,
            ))
        });
        match built {
            Some(Ok(surface)) => Some(surface),
            Some(Err(error)) => {
                tracing::error!(%error, url = page.url(), "surface: a page would not mount");
                None
            }
            None => {
                tracing::warn!(url = page.url(), "surface: no window to mount into yet");
                None
            }
        }
    }
}
