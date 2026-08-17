//! One page's `VirtualDom`, run here rather than compiled into a wasm bundle.
//!
//! [`PageSurface`](crate::PageSurface) owns the webview; this owns what fills it. The webview is
//! handed a document carrying nothing but the interpreter, and every element it displays arrives
//! as a batch of edits evaluated into it.
//!
//! Three things share one `Rc`, all on the main thread:
//!
//! - the host's render call, which asks the dom for a batch each frame,
//! - the `vmux://` handler, which answers `__events` while the page blocks on the reply,
//! - and the IPC handler, which hears `initialize` and `flushed` back from the page.
//!
//! wry's asynchronous protocol closure carries no `Send` bound, so the compiler holds all three to
//! the same thread without a thread-local or an `unsafe`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use tracing::warn;
use vmux_ui::hooks::EventListenerError;
use vmux_ui::transport::{BytesListener, HostScope, PageHost};

use crate::embed::{Embedding, Outbox, Wake};
use crate::{EditScript, EventOutcome, EventRequest, PageDom};

/// What a page needs from the host, and what the host needs back.
#[derive(Clone)]
pub(crate) struct SurfaceDom {
    page: Rc<RefCell<PageDom>>,
    host: Rc<dyn PageHost>,
    reactor: Rc<tokio::runtime::Runtime>,
    waker: Rc<dyn Wake>,
    caret: CaretMirror,
    listeners: Listeners,
    pending_scripts: PendingScripts,
    /// The page has an interpreter and a root, so a batch can be evaluated into it.
    ready: Rc<Cell<bool>>,
    /// The first batch has been sent.
    mounted: Rc<Cell<bool>>,
}

/// Host-to-page callbacks, by event id.
///
/// A `RefCell` rather than a channel because a listener runs inside the dom's own runtime: it is
/// the page reacting, not a message crossing a thread.
type Listeners = Rc<RefCell<HashMap<String, Vec<BytesListener>>>>;

/// Script the page's components asked the host to run, waiting for the next render.
///
/// Queued rather than evaluated on the spot because a component asks while it has no handle to the
/// webview at all — and because a call made during a render would reach the document before the
/// edits that render produced.
type PendingScripts = Rc<RefCell<Vec<String>>>;

impl SurfaceDom {
    /// Mount a page and build the transport its components reach the host through.
    ///
    /// The transport is entered as a [`HostScope`] around every entry into the dom rather than
    /// installed for the thread, because the thread will eventually run more than one page and a
    /// single installed host would leave all but the last talking to the wrong one.
    pub(crate) fn mount(component: crate::PageComponent, embed: &Embedding) -> Self {
        let listeners: Listeners = Rc::new(RefCell::new(HashMap::new()));
        let pending_scripts: PendingScripts = Rc::new(RefCell::new(Vec::new()));
        let caret = CaretMirror::default();
        let host: Rc<dyn PageHost> = Rc::new(SurfaceHost {
            outbox: embed.outbox.clone(),
            listeners: listeners.clone(),
            pending_scripts: pending_scripts.clone(),
            caret: caret.clone(),
        });

        Self {
            page: Rc::new(RefCell::new(PageDom::mount(component))),
            host,
            reactor: Rc::new(Self::reactor()),
            waker: embed.waker.clone(),
            caret,
            listeners,
            pending_scripts,
            ready: Rc::new(Cell::new(false)),
            mounted: Rc::new(Cell::new(false)),
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
    /// Waking is the whole point of hearing about it. A render is withheld until the last batch
    /// lands, and the ack arrives over IPC rather than through the host's event loop, so without
    /// this the frame that would send the next batch waits for a reactive timer. Anything needing
    /// more than one pass then opens in stages.
    pub(crate) fn page_flushed(&self) {
        let _host = HostScope::enter(self.host.clone());
        if let Ok(mut page) = self.page.try_borrow_mut() {
            page.flushed();
        }
        self.waker.wake();
    }

    /// The next batch to evaluate, if there is one and the page can take it.
    pub(crate) fn next_batch(&self) -> Option<EditScript> {
        if !self.ready.get() {
            return None;
        }

        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        let mut page = self.page.try_borrow_mut().ok()?;
        let edits = if self.mounted.get() {
            page.render()?
        } else {
            self.mounted.set(true);
            page.rebuild()
        };

        Some(EditScript::of(&edits))
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
    pub(crate) fn report_caret(&self, element_id: &str, byte: usize) {
        self.caret.report(element_id, byte);
    }

    /// Script the page asked for since this was last called.
    ///
    /// Drained after a batch is evaluated, never before: a component that asks to focus an element
    /// is asking about the element the same render just produced.
    pub(crate) fn take_pending_scripts(&self) -> Vec<String> {
        let Ok(mut pending) = self.pending_scripts.try_borrow_mut() else {
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
    pending_scripts: PendingScripts,
    caret: CaretMirror,
}

/// Where the caret last was, as the document itself reported it.
///
/// Every other capability here is an instruction, which a queued script delivers fine. Reading the
/// caret is a question, and there is nobody to answer it: `evaluate_script` returns nothing to a
/// caller standing in a Dioxus event handler. So the document volunteers the answer on every
/// selection change instead, and a reader takes the last one.
///
/// Stale only if the caret moved without the document saying so, which nothing does — it moves on
/// input, and the report is posted before the next key can arrive.
#[derive(Clone, Default)]
struct CaretMirror(Rc<RefCell<Option<(String, usize)>>>);

impl CaretMirror {
    fn report(&self, element_id: &str, byte: usize) {
        let Ok(mut reported) = self.0.try_borrow_mut() else {
            return;
        };
        *reported = Some((element_id.to_string(), byte));
    }

    /// The caret in this field, or zero if the last report was about another one.
    fn position_in(&self, element_id: &str) -> usize {
        let Ok(reported) = self.0.try_borrow() else {
            return 0;
        };
        match reported.as_ref() {
            Some((id, byte)) if id == element_id => *byte,
            _ => 0,
        }
    }
}

impl SurfaceHost {
    /// Queue a statement to run against an element the page rendered, if it is still there.
    ///
    /// The id goes through `serde_json` rather than interpolation, because it reaches this from
    /// page code and a quote in one would otherwise close the string literal it lands in.
    fn on_element(&self, element_id: &str, statement: &str) {
        let Ok(id) = serde_json::to_string(element_id) else {
            return;
        };
        let Ok(mut pending) = self.pending_scripts.try_borrow_mut() else {
            return;
        };

        pending.push(format!(
            "(function(){{const el=document.getElementById({id});if(el){statement};}})();"
        ));
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
        self.on_element(element_id, "el.focus()");
    }

    fn scroll_element_into_view(&self, element_id: &str) {
        self.on_element(
            element_id,
            r#"el.scrollIntoView({block:"nearest",inline:"nearest"})"#,
        );
    }

    fn select_element_text(&self, element_id: &str) {
        self.on_element(element_id, "el.setSelectionRange(0,el.value.length)");
    }

    /// A frame later than the rest, because focusing an input may move the selection itself and
    /// the focus this follows was queued as its own script.
    fn offer_element_text(&self, element_id: &str) {
        self.on_element(
            element_id,
            "requestAnimationFrame(function(){\
             el.focus();el.setSelectionRange(0,el.value.length);el.scrollLeft=0;})",
        );
    }

    fn caret_position(&self, element_id: &str) -> usize {
        self.caret.position_in(element_id)
    }

    /// The offset is in UTF-8 bytes and `setSelectionRange` counts UTF-16 units, so the value is
    /// re-encoded and cut where the caller cut it. The cut is on a character boundary already —
    /// `TextCaret::place` floors it — so the decode cannot land mid-character.
    fn place_caret(&self, element_id: &str, byte: usize) {
        self.on_element(
            element_id,
            &format!(
                "var b=new TextEncoder().encode(el.value).slice(0,{byte});\
                 var i=new TextDecoder().decode(b).length;el.setSelectionRange(i,i)"
            ),
        );
    }
}
