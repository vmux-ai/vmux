use bevy::prelude::*;

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(FILES_PAGE_MANIFEST);
        app.world_mut().spawn(PROJECTS_PAGE_MANIFEST);
        app.add_plugins((
            contract::ContractPlugin,
            lsp::LspPlugin,
            app_key::KeyPlugin,
            search::SearchPlugin,
        ))
        .add_plugins((
            page_open::PageOpenPlugin,
            file_lifecycle::FileLifecyclePlugin,
            workspace_edit::WorkspaceEditPlugin,
            status::StatusPlugin,
            viewport::ViewportPlugin,
            media::MediaPlugin,
            note::NotePlugin,
            editing::EditingPlugin,
            language::LanguagePlugin,
            shape::ShapePlugin,
            encoding::EncodingPlugin,
            navigation::NavigationPlugin,
            history::HistoryPlugin,
            explorer::ExplorerPlugin,
            ui_state::UiStatePlugin,
        ));
    }
}

const FILES_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "files",
    title: "Files",
    title_message_id: None,
    replaces_command: None,
    keywords: &["file", "open"],
    icon: Some(vmux_core::BuiltinIcon::Files),
    command_bar: true,
};

const PROJECTS_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "projects",
    title: "Projects",
    title_message_id: Some("layout-projects"),
    replaces_command: None,
    keywords: &["project", "files", "folder", "open"],
    icon: Some(vmux_core::BuiltinIcon::Project),
    command_bar: true,
};

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
pub(crate) mod preview;
pub(crate) mod search;
pub(crate) mod status;
pub(crate) mod ui_state;
pub(crate) mod viewport;
pub(crate) mod workspace_edit;
pub(crate) mod wrap;

pub use contract::ContractPlugin;
pub use editor::FileView;
pub use explorer::{GlobalSearchRequest, StackExplorerVisibility};
pub use lsp::LspPlugin;
pub use page_open::restore_file_view_bundle;
pub use status::FileViewModeRequest;
