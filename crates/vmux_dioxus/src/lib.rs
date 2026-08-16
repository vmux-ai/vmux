//! Run a Dioxus page in the host process instead of compiling it to wasm.
//!
//! The page's components and its `VirtualDom` execute here, in Rust. A browser engine somewhere
//! else still owns the document and does the layout, styling and painting — this crate only says
//! what the document should contain, and reads back what the user did to it.
//!
//! Two directions, and they are not symmetric:
//!
//! - **Out.** A render produces a batch of bytes in the interpreter's binary protocol, which the
//!   page applies with `run_from_bytes`. [`PageDom::render`] yields the batch; [`EditScript`]
//!   wraps it for a host that delivers edits by evaluating a script.
//! - **In.** An event arrives as base64 JSON in a `dioxus-data` request header, is run through the
//!   `VirtualDom`, and is answered *synchronously* with whether the browser should still take its
//!   default action. [`EventRequest`] decodes it and [`EventOutcome`] encodes the answer.
//!
//! The synchrony of that second leg is the whole design constraint: the page issues a blocking
//! `XMLHttpRequest` and only calls `preventDefault()` once it has read the reply, so an answer
//! that arrives a frame later is not late, it is useless — the browser has already acted.
//!
//! This crate is deliberately ignorant of both Bevy and wry. It never opens a socket, never binds
//! a port, and holds no handle to a window.
//!
//! Not here yet: an element backing for `onmounted`. Dioxus substitutes `MountedData::new(())` for
//! a mounted event, whose methods all answer `NotSupported`, so `MountedData::set_focus` and
//! friends are inert until a host provides one.

mod edit_script;
mod event_request;
mod page_dom;

pub use edit_script::EditScript;
pub use event_request::{EventOutcome, EventRequest, EventRequestError};
pub use page_dom::PageDom;
