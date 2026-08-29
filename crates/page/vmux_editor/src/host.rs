pub mod contract;
pub mod edit;
pub mod encoding;
pub mod explorer_model;
pub mod fold;
pub mod fold_store;
pub mod highlight;
pub mod keymap;
pub mod lsp;
pub mod markdown;
pub mod palette;
pub mod shape;

pub(crate) mod app_key;
pub(crate) mod dir;
pub(crate) mod explorer_fs;
pub(crate) mod preview;
pub(crate) mod search;
pub(crate) mod wrap;

mod plugin;

pub use contract::EditorContractPlugin;
pub use lsp::LspPlugin;
pub use plugin::{
    EditorPlugin, FileView, FileViewModeRequest, GlobalSearchRequest, StackExplorerVisibility,
    restore_file_view_bundle,
};
