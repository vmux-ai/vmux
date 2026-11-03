use std::cell::Cell;
use std::rc::Rc;

use vmux_ui::hooks::EventListenerError;

pub struct Embedding {
    pub outbox: Rc<dyn Outbox>,
    pub assets: Rc<dyn Assets>,
    pub waker: Rc<dyn Wake>,
    pub lifecycle: Rc<dyn LifecycleSink>,
    pub navigation_revision: Rc<Cell<u64>>,
    pub ipc_epoch: Rc<Cell<u64>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleUpdate {
    Mounted,
    Ready(u64),
    Remounted,
    Navigated(u64),
    FramePending,
    FrameDelivered(u64),
    FrameApplied(u64),
    FrameFailed(u64),
    UiEventHandled,
    HostEventDelivered,
    ListenerRegistered,
    ListenersCleared,
    DropdownOpened,
    PageInteracted,
    MeasurementStarted,
    MeasurementFinished,
    MeasurementsCleared,
}

pub trait LifecycleSink {
    fn update(&self, update: LifecycleUpdate);
}

pub trait Wake {
    fn wake(&self);

    fn task_waker(&self) -> std::task::Waker {
        std::task::Waker::noop().clone()
    }
}

pub trait Assets {
    fn fetch(&self, url: &str, reply: AssetReply);
}

pub trait Outbox {
    fn send(
        &self,
        id: &str,
        bytes: &[u8],
        navigation_revision: u64,
        ipc_epoch: u64,
    ) -> Result<(), EventListenerError>;

    fn set_page(&self, _url: &str) {}

    fn set_title(&self, _title: &str) {}

    fn set_favicon(&self, _url: &str) {}
}

pub struct AssetReply(wry::RequestAsyncResponder);

impl From<wry::RequestAsyncResponder> for AssetReply {
    fn from(responder: wry::RequestAsyncResponder) -> Self {
        Self(responder)
    }
}

impl AssetReply {
    pub fn respond(self, status: u16, mime: &str, body: Vec<u8>) {
        let built = wry::http::Response::builder()
            .status(status)
            .header(wry::http::header::CONTENT_TYPE, mime)
            .body(body);
        match built {
            Ok(response) => self.0.respond(response),
            Err(error) => {
                tracing::error!("vmux_page: an asset response would not build: {error}");
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
