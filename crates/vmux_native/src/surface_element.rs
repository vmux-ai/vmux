//! One element a page rendered, as the component that rendered it gets to hold it.
//!
//! Dioxus hands a mounted component `MountedData::new(())`, and `()` answers `NotSupported` to
//! everything — so `onmounted` is inert until a renderer supplies a backing. This is that backing.
//!
//! Only the instructions are here. The four questions a component can ask an element — its rect,
//! its scroll size, its scroll offset — need an answer to travel back, which this transport has no
//! channel for yet, so they stay refusals rather than lies.

use dioxus_html::geometry::PixelsVector2D;
use dioxus_html::{MountedResult, RenderedElementBacking, ScrollBehavior, ScrollToOptions};

use crate::dom_request::{DomRequest, RequestQueue};

/// The node the renderer assigned, and the queue the host drains into the next frame.
pub(crate) struct SurfaceElement {
    node: dioxus_core::ElementId,
    requests: RequestQueue,
}

impl SurfaceElement {
    pub(crate) fn new(node: dioxus_core::ElementId, requests: RequestQueue) -> Self {
        Self { node, requests }
    }

    /// Queue one instruction and report it done.
    ///
    /// Done rather than pending: the queue is drained by the very next frame, and nothing the page
    /// can say afterwards would change the answer. A future that waited for the page to confirm
    /// would be a round trip bought for a `()`.
    fn queued(&self, request: DomRequest) -> ReadyResult {
        self.requests.push(request);

        Box::pin(std::future::ready(Ok(())))
    }
}

type ReadyResult = std::pin::Pin<Box<dyn std::future::Future<Output = MountedResult<()>>>>;

impl RenderedElementBacking for SurfaceElement {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_focus(&self, focus: bool) -> ReadyResult {
        self.queued(DomRequest::focus_node(self.node, focus))
    }

    fn scroll(&self, coordinates: PixelsVector2D, behavior: ScrollBehavior) -> ReadyResult {
        self.queued(DomRequest::scroll_node(self.node, coordinates, behavior))
    }

    fn scroll_to(&self, options: ScrollToOptions) -> ReadyResult {
        self.queued(DomRequest::reveal_node(self.node, options))
    }
}
