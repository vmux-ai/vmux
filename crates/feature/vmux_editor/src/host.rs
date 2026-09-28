use bevy::prelude::*;

#[vmux_native::page]
pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins((
            crate::lsp_page::LspPage::plugin(),
            crate::ui::FilePage::plugin(),
            crate::ui::ProjectsPage::plugin(),
            crate::ui::KnowledgePage::plugin(),
        ));
        app.add_plugins((
            contract::ContractPlugin,
            tool::FileToolPlugin,
            lsp::LspPlugin,
            app_key::KeyPlugin,
            search::SearchPlugin,
            directory::DirectoryPlugin,
        ))
        .add_plugins(panel::PanelPlugin)
        .add_plugins((
            page_open::PageOpenPlugin,
            file_lifecycle::FileLifecyclePlugin,
            workspace_edit::WorkspaceEditPlugin,
            status::StatusPlugin,
            viewport::ViewportPlugin,
            media::MediaPlugin,
            note::NotePlugin,
            editing::EditorPlugin,
            language::LanguagePlugin,
            shape::ShapePlugin,
            encoding::EncodingPlugin,
            navigation::NavigationPlugin,
            history::HistoryPlugin,
            explorer::ExplorerPlugin,
            vmux_core::host::UiStatePlugin::<vmux_core::event::FileUiState>::default(),
        ))
        .add_plugins((
            Self::MANIFEST
                .plugin()
                .route(vmux_core::HostSpawnRoute::scheme("file")),
            ProjectsPage::MANIFEST.plugin(),
        ));
    }
}

#[vmux_native::page(file = "src/projects.ron")]
struct ProjectsPage;

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
pub mod tool;

pub(crate) mod app_key;
pub(crate) mod directory;
pub(crate) mod editing;
pub(crate) mod editor;
pub(crate) mod explorer;
pub(crate) mod file_lifecycle;
pub(crate) mod history;
pub(crate) mod language;
pub(crate) mod media;
pub(crate) mod navigation;
pub(crate) mod note;
pub(crate) mod page_open;
pub(crate) mod panel;
pub(crate) mod preview;
pub(crate) mod search;
pub(crate) mod status;
pub(crate) mod viewport;
pub(crate) mod workspace_edit;
pub(crate) mod wrap;

pub use contract::ContractPlugin;
pub use editor::FileView;
pub use explorer::{GlobalSearchRequest, StackExplorerVisibility};
pub use lsp::LspPlugin;
pub use page_open::restore_file_view_bundle;
pub use status::FileViewModeRequest;
pub use tool::FileToolPlugin;
