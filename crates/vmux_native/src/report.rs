//! What a page says back, over wry's string IPC.
//!
//! The other direction from [`route`](crate::route): that is the page asking the host for
//! something, this is the page telling it something. Five kinds, and the wire is text because
//! that is all `window.ipc` carries.

use std::rc::Rc;

use tracing::{error, info, warn};

use crate::dom::SurfaceDom;
use crate::embed::Outbox;
use crate::page::NativePage;

/// One page-to-host message, decoded.
enum PageReport<'a> {
    /// The interpreter is up and holding a root, so the page can take a batch at all.
    Initialized,
    /// The last batch was applied, which is what releases the next render.
    Flushed,
    /// Where the caret is, volunteered because nothing can ask for it.
    Caret {
        element: &'a str,
        byte: usize,
    },
    Console {
        level: &'a str,
        text: &'a str,
    },
    /// A payload a page emitted, still base64 as the shim sent it.
    Emitted(&'a str),
}

impl<'a> PageReport<'a> {
    fn of(body: &'a str) -> Self {
        // The interpreter's own two, which it sends as `{"method":..}` through `sendIpcMessage`
        // rather than through the shim, so they arrive as JSON and everything else does not.
        if body.contains(r#""method":"initialize""#) {
            return Self::Initialized;
        }
        if body.contains(r#""method":"flushed""#) {
            return Self::Flushed;
        }
        if let Some(rest) = body.strip_prefix("caret:")
            && let Some((element, byte)) = rest.rsplit_once(':')
            && let Ok(byte) = byte.parse::<usize>()
        {
            return Self::Caret { element, byte };
        }
        if let Some(rest) = body.strip_prefix("log:") {
            let (level, text) = rest.split_once(':').unwrap_or(("log", rest));
            return Self::Console { level, text };
        }

        Self::Emitted(body)
    }
}

/// The page's messages, delivered to whatever answers for each.
///
/// A page emits through an `ArrayBuffer`; wry's IPC carries text, so the shim base64s the same
/// `BinIpcEnvelope` bytes and this undoes it. The envelope framing is left alone, because the host
/// matches its id with `bin_ipc_event_id::<E>()` and that has to keep agreeing.
pub(crate) struct PageMessage {
    outbox: Rc<dyn Outbox>,
    name: &'static str,
    dom: SurfaceDom,
}

impl PageMessage {
    pub(crate) fn new(page: &NativePage, outbox: Rc<dyn Outbox>, dom: SurfaceDom) -> Self {
        Self {
            outbox,
            name: page.url,
            dom,
        }
    }

    pub(crate) fn receive(&self, body: &str) {
        match PageReport::of(body) {
            PageReport::Initialized => self.dom.page_is_ready(),
            PageReport::Flushed => self.dom.page_flushed(),
            PageReport::Caret { element, byte } => self.dom.report_caret(element, byte),
            PageReport::Console { level, text } => self.log(level, text),
            PageReport::Emitted(payload) => self.emit(payload),
        }
    }

    /// The page's console, under the name of the page that wrote it.
    fn log(&self, level: &str, text: &str) {
        let name = self.name;
        match level {
            "error" | "reject" => error!("{name}: {text}"),
            "warn" => warn!("{name}: {text}"),
            _ => info!("{name}: {text}"),
        }
    }

    fn emit(&self, payload: &str) {
        use base64::Engine;
        use vmux_ui::transport::bin_ipc_envelope::BinIpcEnvelope;

        let bytes = match base64::engine::general_purpose::STANDARD.decode(payload) {
            Ok(bytes) => bytes,
            Err(error) => {
                error!("vmux_native: ipc payload was not base64: {error}");
                return;
            }
        };
        let Some((id, payload)) = BinIpcEnvelope::decode(&bytes) else {
            error!(
                "vmux_native: ipc payload was not a bin ipc envelope, {} bytes",
                bytes.len()
            );
            return;
        };
        if self.outbox.send(&id, &payload).is_err() {
            error!("vmux_native: the host outbox is closed");
        }
    }
}
