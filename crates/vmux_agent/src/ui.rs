//! The webview half: what the pages render.
//!
//! Gated once here rather than per module, so what ships to wasm and iOS is the contents of
//! this directory and nothing else.

pub mod agents;
pub mod chat;
