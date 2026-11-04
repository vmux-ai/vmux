#[cfg(host)]
pub use host::{
    ContractPlugin, EditorPlugin, FileToolPlugin, FileView, FileViewModeRequest,
    GlobalSearchRequest, LspPlugin, StackExplorerVisibility, contract, edit, encoding, fold,
    fold_store, keymap, lsp, markdown, shape, tool,
};
#[cfg(host)]
pub use project::ProjectRoute;
pub use project::{
    OwnedProjects, Project, ProjectBranch, ProjectOwner, ProjectPath, ProjectSource,
    ProjectSources, ProjectTarget,
};

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_feature::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub(crate) mod event;
mod picker;
pub(crate) mod project;
mod text;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
pub mod host;
