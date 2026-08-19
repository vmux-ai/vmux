//! One page's `VirtualDom`, run here rather than compiled into a wasm bundle.
//!
//! [`PageSurface`](crate::PageSurface) owns the webview; this owns what fills it. The webview is
//! handed a document carrying nothing but the interpreter, and every element it displays arrives
//! as a batch of edits the page asks for and applies itself.
//!
//! Two things share one `Rc`, both on the main thread:
//!
//! - the host's render call, which hands over a batch when the page is waiting for one, and
//! - the `vmux://` handler, which answers `__events` while the page blocks on the reply, and holds
//!   the page's standing request for `__edits`.
//!
//! wry's asynchronous protocol closure carries no `Send` bound, so the compiler holds both to the
//! same thread without a thread-local or an `unsafe`. The IPC handler is not among them: what a
//! page still says over `window.ipc` is its console, which nothing here has to be consulted about.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use tracing::{error, warn};
use vmux_ui::hooks::EventListenerError;
use vmux_ui::transport::{BytesListener, HostScope, PageHost, TextOffsetAnswer};

use crate::dom_request::{DomRequest, RequestQueue};
use crate::embed::{Embedding, Outbox, Wake};
use crate::event_selection::EventSelection;
use crate::frame::PageFrame;
use crate::measurement::PendingReads;
use crate::surface_element::SurfaceElement;
use crate::{EventOutcome, EventRequest, PageDom};

/// What a page needs from the host, and what the host needs back.
#[derive(Clone)]
pub(crate) struct SurfaceDom {
    page: Rc<RefCell<PageDom>>,
    host: Rc<dyn PageHost>,
    reactor: Rc<tokio::runtime::Runtime>,
    waker: Rc<dyn Wake>,
    /// What the event being handled found selected, for as long as it is being handled.
    selection: Rc<RefCell<EventSelection>>,
    listeners: Listeners,
    requests: RequestQueue,
    /// The questions mounted components have asked about their elements.
    reads: PendingReads,
    /// The first batch has been sent.
    mounted: Rc<Cell<bool>>,
    /// The page's standing request for the next batch, waiting for a render to produce one.
    parked: Rc<RefCell<Option<wry::RequestAsyncResponder>>>,
}

/// Set on a request that is also the acknowledgement of the frame before it.
const FRAME_APPLIED: &str = "x-vmux-applied";

/// Host-to-page callbacks, by event id.
///
/// A `RefCell` rather than a channel because a listener runs inside the dom's own runtime: it is
/// the page reacting, not a message crossing a thread.
type Listeners = Rc<RefCell<HashMap<String, Vec<BytesListener>>>>;

impl SurfaceDom {
    /// Mount a page and build the transport its components reach the host through.
    ///
    /// The transport is entered as a [`HostScope`] around every entry into the dom rather than
    /// installed for the thread, because the thread will eventually run more than one page and a
    /// single installed host would leave all but the last talking to the wrong one.
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

    /// The reactor for the futures pages spawn, shared by every page on this thread.
    ///
    /// `vmux_ui::platform::sleep_ms` is `tokio::time::sleep` off the web, and a page has plenty of
    /// reasons to wait — the palette debounces its host search, the layout defers work by a turn.
    /// Dioxus polls those tasks on this thread, which is the host's, and a host with no reactor of
    /// its own would panic on the first timer rather than failing anywhere a caller could see.
    ///
    /// One worker, and it exists to drive timers rather than to run work: a current-thread runtime
    /// would let a sleep register and then never wake it, because nothing would be driving it.
    ///
    /// Shared, because one per page is a worker thread per page and a pane is a page. They can only
    /// ever want the same timers driven: every page runs on this thread. It also outlives them,
    /// which closing a pane wants — dropping a runtime blocks until its workers stop.
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

    /// The questions in flight, for whatever hears the page answer them.
    pub(crate) fn reads(&self) -> PendingReads {
        self.reads.clone()
    }

    /// The page applied the batch it was last given.
    ///
    /// No wake. The ack used to arrive over IPC, separately from the request it always accompanied,
    /// so the host had to wake itself to notice that a render was released; it rides the request
    /// now, and the flush that follows happens in the same call.
    fn page_flushed(&self) {
        let _host = HostScope::enter(self.host.clone());
        if let Ok(mut page) = self.page.try_borrow_mut() {
            page.flushed();
        }
    }

    /// The next batch, if there is one.
    ///
    /// Nothing checks that the page can take one, because reaching here means it asked: the only
    /// caller returns unless a request is parked, and the shell starts the pump once the
    /// interpreter holds a root.
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

    /// Answer the page's standing request for the next batch, or hold it until there is one.
    ///
    /// The page asks rather than being handed a script, which is what keeps the interpreter's bytes
    /// out of a string literal — and out of base64, which inflated every batch by a third and made
    /// the page decode it a character at a time.
    ///
    /// Only one request is ever held. A second means the first belongs to a page that has gone —
    /// reloaded, or timed its fetch out — so it is answered empty rather than left to rot.
    ///
    /// The request is also the acknowledgement of the frame before it, which is what releases the
    /// next render. Reading it here rather than over IPC is what makes the ordering deterministic:
    /// the two arrived on separate queues, so a request handled before its own ack found the page
    /// still unflushed and produced nothing.
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

    /// Hand the page whatever is waiting for it, if it is waiting to be handed something.
    ///
    /// A batch is not the only reason to answer. A component can ask for the caret without giving
    /// the page anything new to draw, and a request nobody collects is a keystroke that lands in
    /// the wrong field — so an empty batch still goes out when there are requests behind it.
    ///
    /// The requests travel in the same body rather than being collected afterwards. The host knows
    /// at this point whether there are any, so telling the page to come back and ask spends a
    /// blocking round trip on something already decided.
    pub(crate) fn flush_to_page(&self) {
        if self.parked.borrow().is_none() {
            return;
        }
        // Rendering first, because a component asks to focus an element from the render that
        // produces it — draining before would leave the request behind for a frame.
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

    /// Answer the synchronous request the page is blocked on.
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

    /// Whether this request says the page applied the frame before it.
    ///
    /// A header rather than a body: WebKit does not hand a scheme handler the body of a `fetch`,
    /// which is why the interpreter's own event payload travels as a header too.
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

    /// Run one event through the page and produce the reply it is blocked on.
    ///
    /// The borrow can fail: this arrives on the main run loop, which an app spins inside modal
    /// dialogs and menu tracking, so it can land while a render holds the dom. Letting the browser
    /// act is the only safe answer there — the alternative is re-entering the runtime mid-render,
    /// which panics.
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

        // Held for the handler and no longer, so a component reading it from anywhere else finds
        // nothing rather than a stale answer wearing the face of a current one.
        *self.selection.borrow_mut() = selection;
        let element = SurfaceElement::new(event.element, self.requests.clone(), self.reads.clone());
        let outcome = page.handle(event, element);
        *self.selection.borrow_mut() = EventSelection::default();
        drop(page);
        // A handler almost always wrote a signal, and the click that ran it reached the webview
        // rather than the host's event loop, so nothing else knows a render is due.
        self.waker.wake();

        outcome
    }

    /// Deliver a host event to whatever the page registered for it.
    pub(crate) fn deliver(&self, id: &str, payload: &[u8]) {
        // A listener body is page code: it writes the page's signals and may emit back.
        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        // Taken out rather than borrowed across the call: a listener that emits back would
        // otherwise find the map still borrowed, and its event was dropped with a warning. While
        // they are out this id looks unregistered, which is what stops an emit to itself
        // recursing.
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

/// A page's half of [`PageHost`], with no browser in between.
///
/// A page in a wasm bundle reaches the host by base64-ing an envelope through `window.ipc`, which
/// the IPC handler decodes back. Running in this process the payload is already bytes, so it goes
/// straight to the host's [`Outbox`]. Every other capability here is the document's, and this
/// answers for it without asking anyone.
struct SurfaceHost {
    outbox: Rc<dyn Outbox>,
    listeners: Listeners,
    requests: RequestQueue,
    reads: PendingReads,
    /// Only ever read while [`SurfaceDom::handle_event`] holds it, which is the only time a page
    /// can meaningfully ask.
    selection: Rc<RefCell<EventSelection>>,
}

impl SurfaceHost {
    /// Queue something to do to an element the page rendered, for the page to collect.
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

    fn reveal_first_rendered(&self, element_ids: &[&str], centered: bool) {
        self.request(DomRequest::RevealElement {
            elements: element_ids.iter().map(|id| id.to_string()).collect(),
            block: if centered { "center" } else { "nearest" },
        });
    }

    /// Unlike every other capability here, this one waits: the answer is a number the page has to
    /// go and read, so it comes back over IPC against a token the way a measurement does.
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

    /// Straight to the pasteboard rather than through a [`DomRequest`], because this is the one
    /// capability here that is not about the document. Asking the page to run
    /// `navigator.clipboard` would also need `vmux://` to be a secure context, which is a property
    /// of how wry registered the scheme rather than anything this crate decides.
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
}
