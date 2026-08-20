//! Run a vmux page in this process instead of compiling it to wasm.
//!
//! The page's components and its `VirtualDom` execute here, in Rust, while a browser engine owns
//! the document and does the layout, styling and painting. On macOS and iOS that engine is a
//! `wry` webview this crate builds and drives — [`PageSurface`] — which makes this a small
//! `dioxus_desktop` cut to what vmux needs: no windowing stack of its own, no event loop, and a
//! page described by a [`NativePage`] const rather than assembled by a builder.
//!
//! Two directions, and they are not symmetric:
//!
//! - **Out.** A render produces a batch of bytes in the interpreter's binary protocol, which the
//!   page applies with `run_from_bytes`. [`PageDom::render`] yields the batch, and the page asks
//!   for it over `__edits` rather than having it evaluated in.
//! - **In.** An event arrives as base64 JSON in a `dioxus-data` request header, is run through the
//!   `VirtualDom`, and is answered *synchronously* with whether the browser should still take its
//!   default action. [`EventRequest`] decodes it and [`EventOutcome`] encodes the answer.
//!
//! The synchrony of that second leg is the whole design constraint: the page issues a blocking
//! `XMLHttpRequest` and only calls `preventDefault()` once it has read the reply, so an answer
//! that arrives a frame later is not late, it is useless — the browser has already acted.
//!
//! Nothing here knows what is embedding it. A host answers for the three things this cannot know —
//! a frame, an asset, and where a page's bytes go — through [`Embedding`], and that is what keeps
//! an ECS, a window and an asset server on the other side of the boundary.
//!
//! `onmounted` works here too, which it does not by default: dioxus hands a mounted component
//! `MountedData::new(())`, whose every method answers `NotSupported`. Focus and scroll are
//! instructions and resolve as soon as the next frame carries them; the three questions — rect,
//! scroll size, scroll offset — go out node-addressed with a token and come back against it, so
//! two in flight cannot take each other's answer.

mod event_request;
mod instance;
mod page;
mod page_dom;
mod shell;

pub use event_request::{EventOutcome, EventRequest, EventRequestError};
pub use instance::{Instance, PageScope};
pub use page::NativePage;
pub use page_dom::{PageComponent, PageDom};
pub use shell::InterpreterShell;

#[cfg(ui)]
mod dom;
#[cfg(ui)]
mod dom_request;
#[cfg(ui)]
mod embed;
#[cfg(ui)]
mod event_selection;
#[cfg(ui)]
mod frame;
#[cfg(ui)]
mod measurement;
#[cfg(ui)]
mod report;
#[cfg(ui)]
mod route;
#[cfg(ui)]
mod shim;
#[cfg(ui)]
mod surface;
#[cfg(ui)]
mod surface_element;

#[cfg(ui)]
pub use embed::{AssetReply, Assets, Embedding, Outbox, Wake};
#[cfg(ui)]
pub use surface::{Appearance, PageSurface, SiblingOrder};

// wry calls `objc2::exception::catch`, whose C shim ships as a static archive built by
// `objc2-exception-helper`. Cargo puts that archive's directory on the link path but its `-l`
// never reaches the binary, so the reference resolves to nothing. Naming the library here is what
// pulls it in.
#[cfg(ui)]
#[link(name = "objc2_exception_helper_0_1", kind = "static")]
unsafe extern "C" {}
