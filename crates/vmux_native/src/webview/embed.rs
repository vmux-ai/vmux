use std::rc::Rc;

use vmux_ui::hooks::EventListenerError;

pub struct Embedding {
    pub outbox: Rc<dyn Outbox>,
    pub assets: Rc<dyn Assets>,
    pub waker: Rc<dyn Wake>,
}

pub trait Wake {
    fn wake(&self);
}

pub trait Assets {
    fn fetch(&self, url: &str, reply: AssetReply);
}

pub trait Outbox {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError>;
}

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
