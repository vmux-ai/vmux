//! One element a page rendered, as the component that rendered it gets to hold it.
//!
//! Dioxus hands a mounted component `MountedData::new(())`, and `()` answers `NotSupported` to
//! everything — so `onmounted` is inert until a renderer supplies a backing. This is that backing.
//!
//! Instructions resolve as soon as they are queued: the next frame carries them, and nothing the
//! page could say afterwards would change the answer. The three questions go out the same way and
//! wait, because an answer has to come back — see [`measurement`](crate::webview::measurement).

use std::future::Future;
use std::pin::Pin;

use dioxus_core::ElementId;
use dioxus_html::geometry::{PixelsRect, PixelsSize, PixelsVector2D};
use dioxus_html::{MountedResult, RenderedElementBacking, ScrollBehavior, ScrollToOptions};

use crate::webview::dom_request::{DomRequest, Measure, RequestQueue};
use crate::webview::measurement::{Measurement, PendingReads};

type Answer<T> = Pin<Box<dyn Future<Output = MountedResult<T>>>>;

/// The node the renderer assigned, and the two queues that reach the page holding it.
pub(crate) struct Element {
    node: ElementId,
    requests: RequestQueue,
    reads: PendingReads,
}

impl Element {
    pub(crate) fn new(node: ElementId, requests: RequestQueue, reads: PendingReads) -> Self {
        Self {
            node,
            requests,
            reads,
        }
    }

    /// Queue one instruction and report it done.
    ///
    /// Done rather than pending, because a future that waited for the page to confirm would be a
    /// round trip bought for a `()`.
    fn queued(&self, request: DomRequest) -> Answer<()> {
        self.requests.push(request);

        Box::pin(std::future::ready(Ok(())))
    }

    /// Queue one question, and hand back what will hold its answer.
    fn asked(&self, what: Measure) -> Measurement {
        let measurement = self.reads.ask();
        self.requests.push(DomRequest::measure_node(
            self.node,
            measurement.token(),
            what,
        ));

        measurement
    }
}

impl RenderedElementBacking for Element {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_focus(&self, focus: bool) -> Answer<()> {
        self.queued(DomRequest::focus_node(self.node, focus))
    }

    fn scroll(&self, coordinates: PixelsVector2D, behavior: ScrollBehavior) -> Answer<()> {
        self.queued(DomRequest::scroll_node(self.node, coordinates, behavior))
    }

    fn scroll_to(&self, options: ScrollToOptions) -> Answer<()> {
        self.queued(DomRequest::reveal_node(self.node, options))
    }

    fn get_client_rect(&self) -> Answer<PixelsRect> {
        let measured = self.asked(Measure::Rect);

        Box::pin(async move {
            let [x, y, width, height] = measured.await?;

            Ok(PixelsRect::new(
                (x, y).into(),
                PixelsSize::new(width, height),
            ))
        })
    }

    fn get_scroll_size(&self) -> Answer<PixelsSize> {
        let measured = self.asked(Measure::ScrollSize);

        Box::pin(async move {
            let [width, height, _, _] = measured.await?;

            Ok(PixelsSize::new(width, height))
        })
    }

    fn get_scroll_offset(&self) -> Answer<PixelsVector2D> {
        let measured = self.asked(Measure::ScrollOffset);

        Box::pin(async move {
            let [x, y, _, _] = measured.await?;

            Ok(PixelsVector2D::new(x, y))
        })
    }
}
