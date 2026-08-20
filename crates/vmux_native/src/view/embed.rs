//! What a host answers for, so that this crate does not have to.
//!
//! A page hosted here needs three things from whatever embeds it, and all three are the same
//! shape: something this crate cannot know because it belongs to the app around it. Where a
//! rendered frame is asked for, where an asset comes from, and where a page's emitted bytes go.
//!
//! Keeping them traits is what keeps Bevy out. The desktop answers all three out of its ECS — a
//! winit proxy, the asset requester CEF already resolves through, and the channel every
//! `BinReceive` observer reads — and none of that is visible from here.

use std::rc::Rc;

use vmux_ui::hooks::EventListenerError;

/// The host's answers for one page, handed over when its surface is built.
pub struct Embedding {
    pub outbox: Rc<dyn Outbox>,
    pub assets: Rc<dyn Assets>,
    pub waker: Rc<dyn Wake>,
}

/// Ask for a frame, because the page just gave itself something to render.
///
/// An app that renders on demand cannot see a page's work: a batch acknowledged over IPC, an event
/// answered on the protocol thread, and a host emit running a listener all happen without its
/// event loop hearing anything. Each one calls this.
pub trait Wake {
    fn wake(&self);
}

/// Resolve something the page's document referenced.
///
/// The reply is handed over rather than returned, because it is answered off this thread: a host
/// resolving assets from its own loop would deadlock waiting for a reply that loop has to produce.
pub trait Assets {
    fn fetch(&self, url: &str, reply: AssetReply);
}

/// Where a page's emitted bytes go.
///
/// One method, because the other direction is not the host's: a page registers its own listeners
/// here and [`PageSurface::deliver`](crate::PageSurface::deliver) runs them.
pub trait Outbox {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError>;
}

/// The one answer an [`Assets`] request gets.
pub struct AssetReply(wry::RequestAsyncResponder);

impl AssetReply {
    pub(crate) fn of(responder: wry::RequestAsyncResponder) -> Self {
        Self(responder)
    }

    pub fn respond(self, status: u16, mime: &str, body: Vec<u8>) {
        let built = wry::http::Response::builder()
            .status(status)
            .header(wry::http::header::CONTENT_TYPE, mime)
            .body(body);
        match built {
            Ok(response) => self.0.respond(response),
            Err(error) => {
                tracing::error!("vmux_native: an asset response would not build: {error}");
                self.fail("response invalid");
            }
        }
    }

    pub fn fail(self, reason: &str) {
        let response = wry::http::Response::builder()
            .status(500)
            .header(wry::http::header::CONTENT_TYPE, "text/plain")
            .body(reason.as_bytes().to_vec())
            .expect("a literal status and body always build");
        self.0.respond(response);
    }
}
