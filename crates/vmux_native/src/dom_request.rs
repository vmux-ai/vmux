//! What a page asked the host to do to an element, as data rather than as script.
//!
//! Each of these used to be a JavaScript statement composed in Rust and evaluated into the page.
//! The statements were fixed and their one interpolated value went through `serde_json`, so none of
//! them was injectable — but a host that builds script is a host whose vocabulary is whatever the
//! next `format!` says it is, and nothing declares what a page may ask for.
//!
//! So the vocabulary is this enum, the page pulls the queue over `__dom` once a batch has landed,
//! and the shim applies each one from a fixed switch. The host evaluates no statement it composed.

use serde::Serialize;

/// One thing to do to an element the page rendered.
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
}
