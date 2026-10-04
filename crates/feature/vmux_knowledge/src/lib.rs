#[cfg(host)]
pub use graph::{KnowledgeIndex, KnowledgeRenamePlan, KnowledgeResolvedLink, KnowledgeSearchHit};
#[cfg(host)]
pub use host::{ExpandedKnowledgeDirs, KnowledgePlugin};
pub use model::{MarkdownMetadata, WikiLink};
#[cfg(host)]
pub use tool::KnowledgeToolPlugin;
#[cfg(host)]
pub use vault::{AgentPrompt, Frontmatter, KnowledgeVault, MemoriesDir, SkillsDir};
pub use vmux_api::knowledge::{KnowledgeProperty, KnowledgePropertyKind, KnowledgeReference};

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(host)]
mod agent_config;
#[cfg(host)]
mod graph;
#[cfg(host)]
mod host;
mod model;
#[cfg(host)]
mod tool;
#[cfg(host)]
mod vault;
