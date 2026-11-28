#[cfg(host)]
pub use driver::{Frontmatter, KnowledgeVault, RawNote, RawSnapshot};
pub use driver::{MarkdownMetadata, WikiLink};
#[cfg(host)]
pub use graph::{
    KnowledgeBacklink, KnowledgeBrokenLink, KnowledgeGraph, KnowledgeGraphPlugin, KnowledgeIndex,
    KnowledgeReady, KnowledgeResolvedLink, KnowledgeSearchHit, LiveKnowledge, MaterializeKnowledge,
    RetireKnowledge,
};
#[cfg(all(host, feature = "host"))]
pub use host::KnowledgePlugin;
#[cfg(all(host, feature = "host"))]
pub use rename::{
    ApplyKnowledgeRename, KnowledgeRenameComplete, KnowledgeRenamePlugin, KnowledgeRenamePrepared,
    PrepareKnowledgeRename,
};
#[cfg(all(host, feature = "host"))]
pub use tool::KnowledgeToolPlugin;
pub use vmux_api::knowledge::{KnowledgeProperty, KnowledgePropertyKind, KnowledgeReference};

pub struct Feature;

impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

mod driver;
#[cfg(all(host, feature = "host"))]
mod host;
#[cfg(all(host, feature = "host"))]
mod tool;

#[cfg(host)]
mod graph;
#[cfg(all(host, feature = "host"))]
mod rename;
#[cfg(all(host, feature = "host"))]
mod rename_io;
