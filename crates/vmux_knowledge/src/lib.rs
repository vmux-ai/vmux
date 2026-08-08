//! Local-first Markdown knowledge tree and agent context.

#[cfg(not(web))]
mod plugin;
#[cfg(not(web))]
pub mod store;

#[cfg(not(web))]
pub use plugin::KnowledgePlugin;
