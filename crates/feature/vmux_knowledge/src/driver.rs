#[cfg(host)]
pub use graph::{
    KnowledgeBacklink, KnowledgeBrokenLink, KnowledgeIndex, KnowledgeRenamePlan,
    KnowledgeResolvedLink, KnowledgeSearchHit,
};
pub use markdown::{MarkdownMetadata, WikiLink};
#[cfg(host)]
pub use vault::{Frontmatter, KnowledgeVault};

#[cfg(host)]
mod agent_config;
#[cfg(host)]
mod graph;
mod markdown;
#[cfg(host)]
mod vault;
