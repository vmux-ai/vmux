#[cfg(host)]
pub use driver::{
    Frontmatter, KnowledgeBacklink, KnowledgeBrokenLink, KnowledgeIndex, KnowledgeRenamePlan,
    KnowledgeResolvedLink, KnowledgeSearchHit, KnowledgeVault,
};
pub use driver::{MarkdownMetadata, WikiLink};
#[cfg(host)]
pub use host::KnowledgePlugin;
#[cfg(host)]
pub use tool::KnowledgeToolPlugin;
pub use vmux_api::knowledge::{KnowledgeProperty, KnowledgePropertyKind, KnowledgeReference};

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

mod driver;
#[cfg(host)]
mod host;
#[cfg(host)]
mod tool;
