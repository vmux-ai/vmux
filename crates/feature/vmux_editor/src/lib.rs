#[cfg(host)]
pub use host::{
    ContractPlugin, EditorPlugin, FileToolPlugin, FileView, FileViewModeRequest,
    GlobalSearchRequest, LspPlugin, StackExplorerVisibility, contract, edit, encoding, fold,
    fold_store, keymap, lsp, markdown, palette, shape, tool,
};

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub(crate) mod event;
mod text;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
pub mod host;
