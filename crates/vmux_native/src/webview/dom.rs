use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use tracing::{error, warn};
use vmux_ui::hooks::EventListenerError;
use vmux_ui::transport::{BytesListener, HostScope, PageHost, TextOffsetAnswer};

use crate::webview::dom_request::{DomRequest, RequestQueue};
use crate::webview::element::Element;
use crate::webview::embed::{Embedding, Outbox, Wake};
use crate::webview::event_selection::EventSelection;
use crate::webview::frame::PageFrame;
use crate::webview::measurement::PendingReads;
use crate::{EventOutcome, EventRequest, PageDom};

#[derive(Clone)]
pub(crate) struct Dom {
    page: Rc<RefCell<PageDom>>,
    host: Rc<dyn PageHost>,
    reactor: Rc<tokio::runtime::Runtime>,
    waker: Rc<dyn Wake>,
    selection: Rc<RefCell<EventSelection>>,
    listeners: Listeners,
    requests: RequestQueue,
    reads: PendingReads,
    mounted: Rc<Cell<bool>>,
    parked: Rc<RefCell<Option<wry::RequestAsyncResponder>>>,
}

const FRAME_APPLIED: &str = "x-vmux-applied";

type Listeners = Rc<RefCell<HashMap<String, Vec<BytesListener>>>>;

impl Dom {
    pub(crate) fn mount(
        component: crate::PageComponent,
        instance: crate::Instance,
        embed: &Embedding,
    ) -> Self {
        let listeners: Listeners = Rc::new(RefCell::new(HashMap::new()));
        let requests = RequestQueue::default();
        let reads = PendingReads::default();
        let selection: Rc<RefCell<EventSelection>> = Rc::default();
        let host: Rc<dyn PageHost> = Rc::new(SurfaceHost {
            outbox: embed.outbox.clone(),
            listeners: listeners.clone(),
            requests: requests.clone(),
            reads: reads.clone(),
            selection: selection.clone(),
        });

        Self {
            page: Rc::new(RefCell::new(PageDom::mount(component, instance))),
            host,
            reactor: Self::reactor(),
            waker: embed.waker.clone(),
            selection,
            listeners,
            requests,
            reads,
            mounted: Rc::new(Cell::new(false)),
            parked: Rc::new(RefCell::new(None)),
        }
    }

    fn reactor() -> Rc<tokio::runtime::Runtime> {
        thread_local! {
            static REACTOR: Rc<tokio::runtime::Runtime> = Rc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_time()
                    .thread_name("vmux-page")
                    .build()
                    .expect("a reactor for a page's timers"),
            );
        }

        REACTOR.with(Rc::clone)
    }

    pub(crate) fn reads(&self) -> PendingReads {
        self.reads.clone()
    }

    fn page_flushed(&self) {
        let _host = HostScope::enter(self.host.clone());
        if let Ok(mut page) = self.page.try_borrow_mut() {
            page.flushed();
        }
    }

    fn next_batch(&self) -> Option<Vec<u8>> {
        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        let mut page = self.page.try_borrow_mut().ok()?;
        if self.mounted.get() {
            page.render()
        } else {
            self.mounted.set(true);
            Some(page.rebuild())
        }
    }

    pub(crate) fn serve_edits(
        &self,
        request: &wry::http::Request<Vec<u8>>,
        responder: wry::RequestAsyncResponder,
    ) {
        if Self::acknowledges_last_frame(request) {
            self.page_flushed();
        }
        if let Some(stale) = self.parked.borrow_mut().take() {
            Self::respond(stale, PageFrame::new(Vec::new(), Vec::new()));
        }
        *self.parked.borrow_mut() = Some(responder);
        self.flush_to_page();
    }

    pub(crate) fn flush_to_page(&self) {
        if self.parked.borrow().is_none() {
            return;
        }
        let edits = self.next_batch();
        let requests = self.requests.take();
        if edits.is_none() && requests.is_empty() {
            return;
        }
        let Some(responder) = self.parked.borrow_mut().take() else {
            return;
        };

        Self::respond(
            responder,
            PageFrame::new(requests, edits.unwrap_or_default()),
        );
    }

    pub(crate) fn answer_event(
        &self,
        request: &wry::http::Request<Vec<u8>>,
        responder: wry::RequestAsyncResponder,
    ) {
        let headers = request.headers();
        let payload = headers
            .get(EventRequest::HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let body = self
            .handle_event(payload, EventSelection::of(headers))
            .response_bytes();
        let response = wry::http::Response::builder()
            .header(wry::http::header::CONTENT_TYPE, "application/json")
            .body(body)
            .unwrap_or_else(|_| wry::http::Response::new(Vec::new()));

        responder.respond(response);
    }

    fn acknowledges_last_frame(request: &wry::http::Request<Vec<u8>>) -> bool {
        request
            .headers()
            .get(FRAME_APPLIED)
            .is_some_and(|value| value == "1")
    }

    fn respond(responder: wry::RequestAsyncResponder, frame: PageFrame) {
        let built = wry::http::Response::builder()
            .header(wry::http::header::CONTENT_TYPE, "application/octet-stream")
            .body(frame.into_body());
        match built {
            Ok(response) => responder.respond(response),
            Err(error) => {
                error!("vmux_native: a frame would not build a response: {error}");
                responder.respond(wry::http::Response::new(Vec::new()));
            }
        }
    }

    fn handle_event(&self, payload: &str, selection: EventSelection) -> EventOutcome {
        let event = match EventRequest::from_header(payload) {
            Ok(event) => event.into_event(),
            Err(error) => {
                warn!("vmux_native: {error}");
                return EventOutcome::unreadable();
            }
        };

        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        let Ok(mut page) = self.page.try_borrow_mut() else {
            warn!("vmux_native: an event arrived while the page was rendering");
            return EventOutcome::unreadable();
        };

        *self.selection.borrow_mut() = selection;
        let element = Element::new(event.element, self.requests.clone(), self.reads.clone());
        let outcome = page.handle(event, element);
        *self.selection.borrow_mut() = EventSelection::default();
        drop(page);
        self.waker.wake();

        outcome
    }

    pub(crate) fn deliver(&self, id: &str, payload: &[u8]) {
        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        let Ok(mut borrowed) = self.listeners.try_borrow_mut() else {
            warn!("vmux_native: a host emit arrived while the page was registering listeners");
            return;
        };
        let Some(mut registered) = borrowed.get_mut(id).map(std::mem::take) else {
            return;
        };
        drop(borrowed);

        for listener in registered.iter_mut() {
            listener(payload);
        }

        if let Ok(mut borrowed) = self.listeners.try_borrow_mut()
            && let Some(slot) = borrowed.get_mut(id)
        {
            registered.append(slot);
            *slot = registered;
        }
        self.waker.wake();
    }
}

struct SurfaceHost {
    outbox: Rc<dyn Outbox>,
    listeners: Listeners,
    requests: RequestQueue,
    reads: PendingReads,
    selection: Rc<RefCell<EventSelection>>,
}

impl SurfaceHost {
    fn request(&self, request: DomRequest) {
        self.requests.push(request);
    }
}

impl PageHost for SurfaceHost {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        self.outbox.send(id, bytes)
    }

    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
        let mut listeners = self
            .listeners
            .try_borrow_mut()
            .map_err(|_| EventListenerError::Unsupported)?;
        listeners.entry(id.to_string()).or_default().push(on_bytes);

        Ok(())
    }

    fn focus_element(&self, element_id: &str) {
        self.request(DomRequest::Focus {
            element: element_id.to_string(),
        });
    }

    fn scroll_element_into_view(&self, element_id: &str) {
        self.request(DomRequest::ScrollIntoView {
            element: element_id.to_string(),
        });
    }

    fn scroll_element_to(&self, element_id: &str, top: f64) {
        self.request(DomRequest::ScrollTo {
            element: element_id.to_string(),
            top,
        });
    }

    fn reveal_first_rendered(&self, element_ids: &[&str], centered: bool) {
        self.request(DomRequest::RevealElement {
            elements: element_ids.iter().map(|id| id.to_string()).collect(),
            block: if centered { "center" } else { "nearest" },
        });
    }

    fn text_offset_at(&self, element_id: &str, x: f64, y: f64) -> TextOffsetAnswer {
        let measurement = self.reads.ask();
        self.request(DomRequest::TextOffsetAtPoint {
            element: element_id.to_string(),
            token: measurement.token(),
            x,
            y,
        });

        Box::pin(async move {
            let [offset, _, _, _] = measurement.await.ok()?;

            Some(offset.max(0.0) as u32)
        })
    }

    fn select_element_text(&self, element_id: &str) {
        self.request(DomRequest::SelectAll {
            element: element_id.to_string(),
        });
    }

    fn clear_element_text(&self, element_id: &str) {
        self.request(DomRequest::ClearText {
            element: element_id.to_string(),
        });
    }

    fn toggle_media(&self, element_id: &str) {
        self.request(DomRequest::ToggleMedia {
            element: element_id.to_string(),
        });
    }

    fn offer_element_text(&self, element_id: &str) {
        self.request(DomRequest::OfferText {
            element: element_id.to_string(),
        });
    }

    fn write_to_clipboard(&self, text: &str) {
        vmux_clipboard::write(text.to_string());
    }

    fn event_field_selection(&self, element_id: &str) -> (usize, usize) {
        self.selection.borrow().in_field(element_id)
    }

    fn event_document_has_selection(&self) -> bool {
        self.selection.borrow().in_document()
    }

    fn resolves_keys(&self) -> bool {
        true
    }

    fn place_caret(&self, element_id: &str, byte: usize) {
        self.request(DomRequest::PlaceCaret {
            element: element_id.to_string(),
            byte,
        });
    }

    fn caret_to_end(&self, element_id: &str) {
        self.request(DomRequest::CaretToEnd {
            element: element_id.to_string(),
        });
    }
}
