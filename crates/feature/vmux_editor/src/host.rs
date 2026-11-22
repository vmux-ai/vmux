use bevy::prelude::*;

pub use contract::ContractPlugin;
pub use editor::FileView;
pub use explorer::{GlobalSearchRequest, StackExplorerVisibility};
pub use lsp::LspPlugin;
pub use status::FileViewModeRequest;
pub use tool::FileToolPlugin;

#[vmux_page::page]
pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins((
            crate::ui::FilePage::plugin(),
            crate::ui::ProjectsPage::plugin(),
            crate::ui::KnowledgePage::plugin(),
        ));
        app.add_plugins((
            ContractPlugin,
            FileToolPlugin,
            LspPlugin,
            app_key::KeyPlugin,
            search::SearchPlugin,
            directory::DirectoryPlugin,
            command_bar::CommandBarPlugin,
        ))
        .add_plugins(panel::PanelPlugin)
        .add_plugins((
            page_open::PageOpenPlugin,
            project::ProjectPagePlugin,
            file_lifecycle::FileLifecyclePlugin,
            fold_store::FoldStorePlugin,
            workspace_edit::WorkspaceEditPlugin,
            feedback::FeedbackPlugin,
            status::StatusPlugin,
            address::AddressPlugin,
            viewport::ViewportPlugin,
            media::MediaPlugin,
            note::NotePlugin,
            editing::EditorPlugin,
            language::LanguagePlugin,
            shape::ShapePlugin,
            encoding::EncodingPlugin,
        ))
        .add_plugins((
            navigation::NavigationPlugin,
            history::HistoryPlugin,
            explorer::ExplorerPlugin,
            vmux_ecs::UiStatePlugin::<vmux_ecs::event::FileUiState>::default(),
        ))
        .add_plugins((
            Self::manifest()
                .plugin()
                .route(vmux_ecs::host_spawn::HostSpawnRoute::scheme("file")),
            ProjectsPage::manifest()
                .plugin()
                .hosted(vmux_ecs::page::HostedPage::subtree(
                    ProjectsPage::url(),
                    ProjectsPage::manifest().title,
                )),
        ));
    }
}

#[vmux_page::page(page = "projects")]
struct ProjectsPage;

pub mod contract;
pub mod edit;
pub mod encoding;
pub mod fold;
mod fold_io;
mod fold_preferences;
mod fold_store;
pub(crate) mod highlight;
pub mod keymap;
pub mod lsp;
pub mod markdown;
pub mod shape;
pub mod tool;

mod address;
pub(crate) mod app_key;
mod command_bar;
mod delivery;
pub(crate) mod directory;
pub(crate) mod editing;
pub(crate) mod editor;
pub(crate) mod explorer;
pub(crate) mod feedback;
pub(crate) mod file_lifecycle;
mod file_loading;
mod file_loading_io;
mod file_watch;
mod file_watch_io;
mod file_write_io;
mod file_writes;
pub(crate) mod history;
pub(crate) mod language;
pub(crate) mod media;
pub(crate) mod navigation;
pub(crate) mod note;
pub(crate) mod page_open;
pub(crate) mod panel;
mod parked_edits;
pub(crate) mod picker_driver;
pub(crate) mod preview;
pub(crate) mod project;
mod project_form;
pub(crate) mod project_store;
pub(crate) mod search;
pub(crate) mod status;
pub(crate) mod viewport;
pub(crate) mod workspace_edit;
pub(crate) mod wrap;
