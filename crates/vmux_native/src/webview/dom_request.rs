use std::cell::RefCell;
use std::rc::Rc;

use dioxus_core::ElementId;
use dioxus_html::geometry::PixelsVector2D;
use dioxus_html::{ScrollBehavior, ScrollLogicalPosition, ScrollToOptions};
use serde::Serialize;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum DomRequest {
    Focus {
        element: String,
    },
    ScrollIntoView {
        element: String,
    },
    SelectAll {
        element: String,
    },
    OfferText {
        element: String,
    },
    ClearText {
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

#[derive(Clone, Default)]
pub(crate) struct RequestQueue(Rc<RefCell<Vec<DomRequest>>>);

impl RequestQueue {
    pub(crate) fn push(&self, request: DomRequest) {
        let Ok(mut queued) = self.0.try_borrow_mut() else {
            return;
        };

        queued.push(request);
    }

    pub(crate) fn take(&self) -> Vec<DomRequest> {
        let Ok(mut queued) = self.0.try_borrow_mut() else {
            return Vec::new();
        };

        std::mem::take(&mut *queued)
    }
}
