pub use markdown::{MarkdownMetadata, WikiLink};
#[cfg(host)]
pub use snapshot::{RawNote, RawSnapshot};
#[cfg(host)]
pub use vault::{Frontmatter, KnowledgeVault};

#[cfg(host)]
mod agent_config;
mod markdown;
#[cfg(host)]
mod snapshot;
#[cfg(host)]
mod vault;
