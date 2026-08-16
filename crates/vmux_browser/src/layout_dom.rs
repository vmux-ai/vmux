//! The layout page's `VirtualDom`, run here rather than compiled into the wasm bundle.
//!
//! [`layout_view`](crate::layout_view) owns the `WKWebView`; this owns what fills it. The webview
//! is handed a document carrying nothing but the interpreter, and every element it displays
//! arrives as a batch of edits evaluated into it.
//!
//! Three things share one `Rc`, all on the main thread:
//!
//! - the render system, which asks the dom for a batch each frame,
//! - the `vmux://` handler, which answers `__events` while the page blocks on the reply,
//! - and the IPC handler, which hears `initialize` and `flushed` back from the page.
//!
//! wry's asynchronous protocol closure carries no `Send` bound, so the compiler holds all three to
//! the same thread without a thread-local or an `unsafe`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use bevy::prelude::*;
use bevy_cef_core::prelude::BinIpcEventRaw;
use vmux_dioxus::{EditScript, EventOutcome, EventRequest, PageDom};
use vmux_ui::hooks::EventListenerError;
use vmux_ui::transport::{BytesListener, PageHost, install_host};

/// What the layout page needs from the host, and what the host needs back.
#[derive(Clone)]
pub(crate) struct LayoutDom {
    page: Rc<RefCell<PageDom>>,
    listeners: Listeners,
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

impl LayoutDom {
    /// Mount the layout page and install the transport its components reach the host through.
    ///
    /// `install_host` has to happen before the first render, because a component asks for the
    /// installed host as it mounts and the native fallback is deliberately `None`.
    pub(crate) fn mount(
        bin_ipc: async_channel::Sender<BinIpcEventRaw>,
        webview: Entity,
        host: String,
    ) -> Self {
        let listeners: Listeners = Rc::new(RefCell::new(HashMap::new()));

        install_host(Rc::new(LayoutPageHost {
            bin_ipc,
            webview,
            host,
            listeners: listeners.clone(),
        }));

        Self {
            page: Rc::new(RefCell::new(PageDom::mount(vmux_layout::page::Page))),
            listeners,
            ready: Rc::new(Cell::new(false)),
            mounted: Rc::new(Cell::new(false)),
        }
    }

    /// The page reported that its interpreter is initialized and holding a root.
    pub(crate) fn page_is_ready(&self) {
        self.ready.set(true);
    }

    /// The page applied the batch it was last given.
    pub(crate) fn page_flushed(&self) {
        if let Ok(mut page) = self.page.try_borrow_mut() {
            page.flushed();
        }
    }

    /// The next batch to evaluate, if there is one and the page can take it.
    pub(crate) fn next_batch(&self) -> Option<EditScript> {
        if !self.ready.get() {
            return None;
        }

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
    /// The borrow can fail: this arrives on the main run loop, which the app spins inside modal
    /// dialogs and menu tracking, so it can land while a render holds the dom. Letting the browser
    /// act is the only safe answer there — the alternative is re-entering the runtime mid-render,
    /// which panics.
    pub(crate) fn handle_event(&self, header: &str) -> EventOutcome {
        let event = match EventRequest::from_header(header) {
            Ok(event) => event.into_event(),
            Err(error) => {
                warn!("layout_dom: {error}");
                return EventOutcome::unreadable();
            }
        };

        let Ok(mut page) = self.page.try_borrow_mut() else {
            warn!("layout_dom: an event arrived while the page was rendering");
            return EventOutcome::unreadable();
        };

        page.handle(event)
    }

    /// Deliver a host event to whatever the page registered for it.
    pub(crate) fn deliver(&self, id: &str, payload: &[u8]) {
        let Ok(mut listeners) = self.listeners.try_borrow_mut() else {
            warn!("layout_dom: a host emit arrived while the page was registering listeners");
            return;
        };
        let Some(registered) = listeners.get_mut(id) else {
            return;
        };

        for listener in registered {
            listener(payload);
        }
    }
}

/// The layout page's half of [`PageHost`], with no browser in between.
///
/// A page in the wasm bundle reaches the host by base64-ing an envelope through `window.ipc`,
/// which the IPC handler decodes back into a [`BinIpcEventRaw`]. Running in this process, the
/// payload is already bytes and the entity is already known, so it goes straight onto the channel
/// every existing `BinReceive` observer already reads.
struct LayoutPageHost {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    webview: Entity,
    host: String,
    listeners: Listeners,
}

impl PageHost for LayoutPageHost {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        // Unbounded, so this never blocks — which is what lets an event handler call it while the
        // page waits on a synchronous reply.
        self.bin_ipc
            .send_blocking(BinIpcEventRaw {
                webview: self.webview,
                host: self.host.clone(),
                id: id.to_string(),
                payload: bytes.to_vec(),
            })
            .map_err(|_| EventListenerError::Unsupported)
    }

    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
        let mut listeners = self
            .listeners
            .try_borrow_mut()
            .map_err(|_| EventListenerError::Unsupported)?;
        listeners.entry(id.to_string()).or_default().push(on_bytes);

        Ok(())
    }
}
