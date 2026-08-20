//! The half of this crate that needs a webview to exist.
//!
//! Gated once, here, rather than an attribute per module: everything below wants the same thing —
//! a window to parent a `wry` webview into — so what decides whether it is compiled is one fact,
//! and it should be written once. The page half above this is plain Rust and builds everywhere.

mod dom;
mod dom_request;
mod embed;
mod event_selection;
mod frame;
mod measurement;
mod report;
mod route;
mod shim;
mod surface;
mod surface_element;

pub use embed::{AssetReply, Assets, Embedding, Outbox, Wake};
pub use surface::{Appearance, PageSurface, SiblingOrder};

// wry calls `objc2::exception::catch`, whose C shim ships as a static archive built by
// `objc2-exception-helper`. Cargo puts that archive's directory on the link path but its `-l`
// never reaches the binary, so the reference resolves to nothing. Naming the library here is what
// pulls it in.
#[link(name = "objc2_exception_helper_0_1", kind = "static")]
unsafe extern "C" {}
