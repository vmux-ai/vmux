//! Shared component types and reflection registration used across all vmux crates.

pub mod agent_setup;
pub mod dom_snapshot;
pub mod editor;
pub mod event;
pub mod icon;
pub mod input;
pub mod knowledge;
pub mod media;
pub mod page_metadata;
pub mod process_id;
pub mod scroll;
pub mod tools;
pub mod vault;
pub use editor::{CursorPos, EditMode, KeymapKind, SelSpan};
pub use icon::{BuiltinIcon, PageIcon};
pub use input::{KeyModifiers, KeyStroke};
pub use page_metadata::PageMetadata;
pub use process_id::ProcessId;

#[cfg(not(web))]
pub mod host;
#[cfg(not(web))]
pub use host::*;
