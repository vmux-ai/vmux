pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");

#[cfg(host)]
mod host;
#[cfg(host)]
pub mod store;
#[cfg(host)]
mod tool;
#[cfg(host)]
pub use host::{ExpandedKnowledgeDirs, KnowledgePlugin};
#[cfg(host)]
pub use tool::KnowledgeToolPlugin;
