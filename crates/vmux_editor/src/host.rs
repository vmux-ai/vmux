//! Everything that only exists on a desktop: the filesystem, the language servers, and the
//! Bevy plugin that drives them.
//!
//! One gate for the lot, rather than an attribute on each declaration. The crate's public paths
//! are unchanged: `lib.rs` re-exports this module's contents, so `vmux_editor::EditorPlugin`
//! still resolves from outside and `crate::edit` still resolves from within.

pub mod contract;
pub mod edit;
pub mod explorer_model;
pub mod fold;
pub mod fold_store;
pub mod highlight;
pub mod keymap;
pub mod lsp;
pub mod markdown;

pub(crate) mod app_key;
pub(crate) mod dir;
pub(crate) mod explorer_fs;
pub(crate) mod preview;
pub(crate) mod wrap;

mod plugin;

pub use contract::EditorContractPlugin;
pub use lsp::LspPlugin;
pub use plugin::{
    EditorPlugin, FileView, FileViewModeRequest, GlobalSearchRequest, StackExplorerVisibility,
    restore_file_view_bundle,
};
