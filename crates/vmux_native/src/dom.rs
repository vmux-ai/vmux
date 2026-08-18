//! One page's `VirtualDom`, run here rather than compiled into a wasm bundle.
//!
//! [`PageSurface`](crate::PageSurface) owns the webview; this owns what fills it. The webview is
//! handed a document carrying nothing but the interpreter, and every element it displays arrives
//! as a batch of edits the page asks for and applies itself.
//!
//! Three things share one `Rc`, all on the main thread:
//!
//! - the host's render call, which hands over a batch when the page is waiting for one,
//! - the `vmux://` handler, which answers `__events` while the page blocks on the reply, and
//!   holds the page's standing request for `__edits`,
//! - and the IPC handler, which hears `initialize` back from the page.
//!
//! wry's asynchronous protocol closure carries no `Send` bound, so the compiler holds all three to
//! the same thread without a thread-local or an `unsafe`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use tracing::{error, warn};
use vmux_ui::hooks::EventListenerError;
use vmux_ui::transport::{BytesListener, HostScope, PageHost};

use crate::dom_request::DomRequest;
use crate::embed::{Embedding, Outbox, Wake};
use crate::frame::PageFrame;
use crate::{EventOutcome, EventRequest, PageDom};

/// What a page needs from the host, and what the host needs back.
#[derive(Clone)]
pub(crate) struct SurfaceDom {
    page: Rc<RefCell<PageDom>>,
    host: Rc<dyn PageHost>,
    reactor: Rc<tokio::runtime::Runtime>,
    waker: Rc<dyn Wake>,
    caret: CaretMirror,
    listeners: Listeners,
    pending_requests: PendingRequests,
    /// The page has an interpreter and a root, so a batch can be evaluated into it.
    ready: Rc<Cell<bool>>,
    /// The first batch has been sent.
    mounted: Rc<Cell<bool>>,
    /// The page's standing request for the next batch, waiting for a render to produce one.
    parked: Rc<RefCell<Option<wry::RequestAsyncResponder>>>,
}

/// Set on a request that is also the acknowledgement of the frame before it.
const FRAME_APPLIED: &str = "x-vmux-applied";

/// What the page's components asked the host to do to their elements, waiting for the page to ask.
///
/// Queued rather than done on the spot because a component asks while it has no handle to the view
/// at all — and because a request answered during a render would reach the document before the
/// edits that render produced.
type PendingRequests = Rc<RefCell<Vec<DomRequest>>>;

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
        let pending_requests: PendingRequests = Rc::new(RefCell::new(Vec::new()));
        let caret = CaretMirror::default();
        let host: Rc<dyn PageHost> = Rc::new(SurfaceHost {
            outbox: embed.outbox.clone(),
            listeners: listeners.clone(),
            pending_requests: pending_requests.clone(),
            caret: caret.clone(),
        });

        Self {
            page: Rc::new(RefCell::new(PageDom::mount(component, instance))),
            host,
            reactor: Rc::new(Self::reactor()),
            waker: embed.waker.clone(),
            caret,
            listeners,
            pending_requests,
            ready: Rc::new(Cell::new(false)),
            mounted: Rc::new(Cell::new(false)),
            parked: Rc::new(RefCell::new(None)),
        }
    }

    /// A reactor for the futures the page spawns.
    ///
    /// `vmux_ui::platform::sleep_ms` is `tokio::time::sleep` off the web, and a page has plenty of
    /// reasons to wait — the palette debounces its host search, the layout defers work by a turn.
    /// Dioxus polls those tasks on this thread, which is the host's, and a host with no reactor of
    /// its own would panic on the first timer rather than failing anywhere a caller could see.
    ///
    /// One worker, and it exists to drive timers rather than to run work: a current-thread runtime
    /// would let a sleep register and then never wake it, because nothing would be driving it.
    fn reactor() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_time()
            .thread_name("vmux-page")
            .build()
            .expect("a reactor for a page's timers")
    }

    /// The page reported that its interpreter is initialized and holding a root.
    pub(crate) fn page_is_ready(&self) {
        self.ready.set(true);
        self.waker.wake();
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

    /// The next batch, if there is one and the page can take it.
    fn next_batch(&self) -> Option<Vec<u8>> {
        if !self.ready.get() {
            return None;
        }

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
        let requests = self.take_pending_requests();
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
        let header = request
            .headers()
            .get("dioxus-data")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let body = self.handle_event(header).response_bytes();
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
    pub(crate) fn handle_event(&self, header: &str) -> EventOutcome {
        let event = match EventRequest::from_header(header) {
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

        let outcome = page.handle(event);
        drop(page);
        // A handler almost always wrote a signal, and the click that ran it reached the webview
        // rather than the host's event loop, so nothing else knows a render is due.
        self.waker.wake();

        outcome
    }

    /// The document reported where its caret is.
    pub(crate) fn report_caret(&self, element_id: &str, start: usize, end: usize) {
        self.caret.report(element_id, start, end);
    }

    /// The document reported whether anything in it is selected.
    pub(crate) fn report_document_selection(&self, selected: bool) {
        self.caret.report_document_selection(selected);
    }

    /// What the page asked for since the last frame.
    ///
    /// Drained into the frame that carries the batch they follow: a component that asks to focus an
    /// element is asking about the element the same render just produced.
    fn take_pending_requests(&self) -> Vec<DomRequest> {
        let Ok(mut pending) = self.pending_requests.try_borrow_mut() else {
            return Vec::new();
        };

        std::mem::take(&mut *pending)
    }

    /// Deliver a host event to whatever the page registered for it.
    pub(crate) fn deliver(&self, id: &str, payload: &[u8]) {
        // A listener body is page code: it writes the page's signals and may emit back.
        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        let Ok(mut listeners) = self.listeners.try_borrow_mut() else {
            warn!("vmux_native: a host emit arrived while the page was registering listeners");
            return;
        };
        let Some(registered) = listeners.get_mut(id) else {
            return;
        };

        for listener in registered {
            listener(payload);
        }
        drop(listeners);
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
    pending_requests: PendingRequests,
    caret: CaretMirror,
}

/// Where the caret last was, as the document itself reported it.
///
/// Every other capability here is an instruction, which a queued script delivers fine. Reading the
/// caret is a question, and nothing carries an answer back: the host hands the page a batch and
/// the page applies it. So the document volunteers the answer on every selection change instead,
/// and a reader takes the last one.
///
/// Stale only if the caret moved without the document saying so, which nothing does — it moves on
/// input, and the report is posted before the next key can arrive.
#[derive(Clone, Default)]
struct CaretMirror {
    field: Rc<RefCell<Option<(String, usize, usize)>>>,
    document_selected: Rc<Cell<bool>>,
}

impl CaretMirror {
    fn report(&self, element_id: &str, start: usize, end: usize) {
        let Ok(mut reported) = self.field.try_borrow_mut() else {
            return;
        };
        *reported = Some((element_id.to_string(), start, end));
    }

    fn report_document_selection(&self, selected: bool) {
        self.document_selected.set(selected);
    }

    /// The caret range in this field, collapsed at zero if the last report was about another one.
    fn selection_in(&self, element_id: &str) -> (usize, usize) {
        let Ok(reported) = self.field.try_borrow() else {
            return (0, 0);
        };
        match reported.as_ref() {
            Some((id, start, end)) if id == element_id => (*start, *end),
            _ => (0, 0),
        }
    }
}

impl SurfaceHost {
    /// Queue something to do to an element the page rendered, for the page to collect.
    fn request(&self, request: DomRequest) {
        let Ok(mut pending) = self.pending_requests.try_borrow_mut() else {
            return;
        };

        pending.push(request);
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

    fn select_element_text(&self, element_id: &str) {
        self.request(DomRequest::SelectAll {
            element: element_id.to_string(),
        });
    }

    fn offer_element_text(&self, element_id: &str) {
        self.request(DomRequest::OfferText {
            element: element_id.to_string(),
        });
    }

    fn caret_selection(&self, element_id: &str) -> (usize, usize) {
        self.caret.selection_in(element_id)
    }

    fn has_text_selection(&self) -> bool {
        self.caret.document_selected.get()
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
