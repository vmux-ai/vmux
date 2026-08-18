//! What a page asked the host to do to an element, as data rather than as script.
//!
//! Each of these used to be a JavaScript statement composed in Rust and evaluated into the page.
//! The statements were fixed and their one interpolated value went through `serde_json`, so none of
//! them was injectable — but a host that builds script is a host whose vocabulary is whatever the
//! next `format!` says it is, and nothing declares what a page may ask for.
//!
//! So the vocabulary is this enum, it travels in the frame that carries the batch it follows, and
//! the shim applies each one from a fixed switch. The host evaluates no statement it composed.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus_core::ElementId;
use dioxus_html::geometry::PixelsVector2D;
use dioxus_html::{ScrollBehavior, ScrollLogicalPosition, ScrollToOptions};
use serde::Serialize;

/// One thing to do to an element the page rendered.
///
/// Two ways of naming an element, because there are two ways a page comes to hold one. A component
/// that rendered an `id` knows the element by that, and the shim looks it up. A component holding a
/// `MountedData` has no id at all — it has the node the renderer assigned — and what it wants done
/// are the interpreter's own methods, so those are addressed by node and handed straight to it.
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
    /// Focus a field and offer its value up to be overtyped: selected whole, rewound to the start.
    OfferText {
        element: String,
    },
    /// `byte` is a UTF-8 offset, which the page re-encodes: `setSelectionRange` counts UTF-16 units.
    PlaceCaret {
        element: String,
        byte: usize,
    },
    /// `interpreter.setFocus`.
    FocusNode {
        node: usize,
        focus: bool,
    },
    /// `interpreter.scroll`, which scrolls the element's own content.
    ScrollNode {
        node: usize,
        x: f64,
        y: f64,
        behavior: &'static str,
    },
    /// `interpreter.scrollTo`, which scrolls whatever contains the element until it is visible.
    RevealNode {
        node: usize,
        behavior: &'static str,
        block: &'static str,
        inline: &'static str,
    },
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

    /// The DOM calls the two axes `block` and `inline`; dioxus names them for the screen.
    pub(crate) fn reveal_node(node: ElementId, options: ScrollToOptions) -> Self {
        Self::RevealNode {
            node: node.0,
            behavior: Self::behaviour(options.behavior),
            block: Self::alignment(options.vertical),
            inline: Self::alignment(options.horizontal),
        }
    }

    /// Spelled as `scrollIntoView` and `scroll` read them. Written out rather than derived, because
    /// dioxus only carries these spellings under a serde feature this crate does not ask for.
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

/// What the page's components have asked for, waiting for the page to come and collect it.
///
/// Queued rather than done on the spot because a component asks while it holds no handle to the
/// view at all — and because a request answered during a render would reach the document before the
/// edits that render produced.
#[derive(Clone, Default)]
pub(crate) struct RequestQueue(Rc<RefCell<Vec<DomRequest>>>);

impl RequestQueue {
    pub(crate) fn push(&self, request: DomRequest) {
        let Ok(mut queued) = self.0.try_borrow_mut() else {
            return;
        };

        queued.push(request);
    }

    /// Everything asked for since the last frame, which is drained into the next one.
    pub(crate) fn take(&self) -> Vec<DomRequest> {
        let Ok(mut queued) = self.0.try_borrow_mut() else {
            return Vec::new();
        };

        std::mem::take(&mut *queued)
    }
}
