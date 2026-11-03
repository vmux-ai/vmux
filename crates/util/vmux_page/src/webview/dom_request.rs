use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dioxus_core::ElementId;
use dioxus_html::geometry::PixelsVector2D;
use dioxus_html::{ScrollBehavior, ScrollLogicalPosition, ScrollToOptions};
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum DomRequest {
    Remount,
    AnimateHeight {
        element: String,
        duration_ms: u32,
    },
    AnimateLayout {
        element: String,
        duration_ms: u32,
    },
    Focus {
        element: String,
    },
    ShowPopover {
        element: String,
    },
    ScrollIntoView {
        element: String,
    },
    ScrollIntoViewVertical {
        element: String,
    },
    ScrollTo {
        element: String,
        top: f64,
        behavior: &'static str,
    },
    PreserveScroll,
    SelectAll {
        element: String,
    },
    OfferText {
        element: String,
    },
    ToggleMedia {
        element: String,
    },
    PlaceCaret {
        element: String,
        byte: usize,
    },
    CaretToEnd {
        element: String,
    },
    RunScript {
        script: String,
    },
    RevealElement {
        elements: Vec<String>,
        block: &'static str,
    },
    TextOffsetAtPoint {
        element: String,
        token: u64,
        x: f64,
        y: f64,
    },
    SectionAtTop {
        element: String,
        token: u64,
    },
    ScrollSection {
        container: String,
        element: String,
        behavior: &'static str,
    },
    FocusNode {
        node: usize,
        focus: bool,
    },
    ScrollNode {
        node: usize,
        x: f64,
        y: f64,
        behavior: &'static str,
    },
    RevealNode {
        node: usize,
        behavior: &'static str,
        block: &'static str,
        inline: &'static str,
    },
    MeasureNode {
        node: usize,
        token: u64,
        what: Measure,
    },
}

#[derive(Clone, Serialize)]
pub(crate) struct QueuedDomRequest {
    epoch: u64,
    revision: u64,
    #[serde(flatten)]
    request: DomRequest,
}

impl QueuedDomRequest {
    #[cfg(test)]
    pub(crate) fn request(&self) -> &DomRequest {
        &self.request
    }

    pub(crate) fn retryable(&self) -> bool {
        !matches!(
            self.request,
            DomRequest::ToggleMedia { .. } | DomRequest::RunScript { .. }
        )
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Measure {
    Rect,
    ScrollSize,
    ScrollOffset,
}

impl DomRequest {
    pub(crate) fn focus_node(node: ElementId, focus: bool) -> Self {
        Self::FocusNode {
            node: node.0,
            focus,
        }
    }

    pub(crate) fn scroll_node(
        node: ElementId,
        to: PixelsVector2D,
        behavior: ScrollBehavior,
    ) -> Self {
        Self::ScrollNode {
            node: node.0,
            x: to.x,
            y: to.y,
            behavior: Self::behaviour(behavior),
        }
    }

    pub(crate) fn reveal_node(node: ElementId, options: ScrollToOptions) -> Self {
        Self::RevealNode {
            node: node.0,
            behavior: Self::behaviour(options.behavior),
            block: Self::alignment(options.vertical),
            inline: Self::alignment(options.horizontal),
        }
    }

    pub(crate) fn measure_node(node: ElementId, token: u64, what: Measure) -> Self {
        Self::MeasureNode {
            node: node.0,
            token,
            what,
        }
    }

    fn behaviour(behavior: ScrollBehavior) -> &'static str {
        match behavior {
            ScrollBehavior::Instant => "instant",
            ScrollBehavior::Smooth => "smooth",
        }
    }

    fn alignment(position: ScrollLogicalPosition) -> &'static str {
        match position {
            ScrollLogicalPosition::Start => "start",
            ScrollLogicalPosition::Center => "center",
            ScrollLogicalPosition::End => "end",
            ScrollLogicalPosition::Nearest => "nearest",
        }
    }
}

#[derive(Clone)]
pub(crate) struct RequestQueue {
    queued: Rc<RefCell<Vec<QueuedDomRequest>>>,
    ipc_epoch: Rc<Cell<u64>>,
    navigation_revision: Rc<Cell<u64>>,
}

impl Default for RequestQueue {
    fn default() -> Self {
        Self::new(Rc::new(Cell::new(1)), Rc::new(Cell::new(1)))
    }
}

impl RequestQueue {
    pub(crate) fn new(ipc_epoch: Rc<Cell<u64>>, navigation_revision: Rc<Cell<u64>>) -> Self {
        Self {
            queued: Rc::default(),
            ipc_epoch,
            navigation_revision,
        }
    }

    pub(crate) fn stamp(&self, request: DomRequest) -> QueuedDomRequest {
        QueuedDomRequest {
            epoch: self.ipc_epoch.get(),
            revision: self.navigation_revision.get(),
            request,
        }
    }

    pub(crate) fn push(&self, request: DomRequest) {
        let Ok(mut queued) = self.queued.try_borrow_mut() else {
            return;
        };

        queued.push(self.stamp(request));
    }

    pub(crate) fn take(&self) -> Vec<QueuedDomRequest> {
        let Ok(mut queued) = self.queued.try_borrow_mut() else {
            return Vec::new();
        };

        std::mem::take(&mut *queued)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_request_kind_is_a_case_the_shim_handles() {
        let requests = [
            DomRequest::AnimateHeight {
                element: "e".into(),
                duration_ms: 160,
            },
            DomRequest::Remount,
            DomRequest::Focus {
                element: "e".into(),
            },
            DomRequest::ShowPopover {
                element: "e".into(),
            },
            DomRequest::ScrollIntoView {
                element: "e".into(),
            },
            DomRequest::ScrollIntoViewVertical {
                element: "e".into(),
            },
            DomRequest::ScrollTo {
                element: "e".into(),
                top: 0.0,
                behavior: "instant",
            },
            DomRequest::PreserveScroll,
            DomRequest::SelectAll {
                element: "e".into(),
            },
            DomRequest::OfferText {
                element: "e".into(),
            },
            DomRequest::ToggleMedia {
                element: "e".into(),
            },
            DomRequest::PlaceCaret {
                element: "e".into(),
                byte: 0,
            },
            DomRequest::RunScript {
                script: "return;".into(),
            },
        ];

        for request in requests {
            let json = serde_json::to_value(&request).expect("a request serializes");
            let kind = json["kind"].as_str().expect("every request is tagged");
            assert!(
                super::super::shim::WRY_HOST_SHIM.contains(&format!("case '{kind}':")),
                "the shim has no case for `{kind}`, so the page ignores it"
            );
        }
    }

    #[test]
    fn queued_request_carries_document_identity() {
        let queue = RequestQueue::new(Rc::new(Cell::new(7)), Rc::new(Cell::new(11)));
        let request = queue.stamp(DomRequest::Remount);

        let json = serde_json::to_value(&request).unwrap();

        assert_eq!(json["epoch"], 7);
        assert_eq!(json["revision"], 11);
    }
}
