//! Local-first Markdown knowledge tree and agent context.

#[cfg(not(target_arch = "wasm32"))]
mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod store;

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::KnowledgePlugin;
