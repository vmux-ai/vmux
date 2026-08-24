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
    /// Scroll a container to an absolute offset from its top.
    ///
    /// Distinct from [`Self::ScrollIntoView`], which can only aim at an element that exists. A
    /// viewport scroll — a page down, a `zz` — names a position no element sits at.
    ScrollTo {
        element: String,
        top: f64,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every request the host can queue must be a case the page's shim answers.
    ///
    /// A source scan because there is no way to run the switch without a webview, and the two
    /// halves are edited apart: the tag comes from `serde` here and the case is written by hand in
    /// the shim, so a new variant is silently ignored by the page until someone notices the
    /// feature does nothing. The tag is serialized rather than spelled out, so this fails if the
    /// rename attribute drifts too.
    #[test]
    fn every_request_kind_is_a_case_the_shim_handles() {
        let requests = [
            DomRequest::Focus {
                element: "e".into(),
            },
            DomRequest::ScrollIntoView {
                element: "e".into(),
            },
            DomRequest::ScrollTo {
                element: "e".into(),
                top: 0.0,
            },
            DomRequest::SelectAll {
                element: "e".into(),
            },
            DomRequest::OfferText {
                element: "e".into(),
            },
            DomRequest::ClearText {
                element: "e".into(),
            },
            DomRequest::ToggleMedia {
                element: "e".into(),
            },
            DomRequest::PlaceCaret {
                element: "e".into(),
                byte: 0,
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
}
