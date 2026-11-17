#![allow(non_snake_case)]

pub(crate) use input::EditorFocus;
pub use workspace::Page;
pub(super) use workspace::{HOVER_DELAY_MS, Mode, SCROLL_ID};

mod address;
mod diagnostic;
mod directory;
mod dom;
mod editor;
mod explorer;
mod input;
mod key;
mod markdown;
mod menu;
mod note;
mod note_text;
mod outline_navigation;
mod project;
mod project_form;
mod project_manager;
mod sidebar;
mod state;
mod status;
mod text_geometry;
mod text_style;
mod toolbar;
mod workspace;

#[vmux_page::page(
    component = Page,
    document_url = "vmux://files/",
    states = [vmux_api::address::PageAddressUiState, vmux_ui::theme::ThemeUiState, vmux_ecs::event::FileUiState, vmux_ecs::event::ProjectNavigation, vmux_api::input::KeyClaimsUiState, vmux_api::page_location::PageLocationUiState],
    dom_group = "editor",
    subtree,
    reactive_takes,
    takes = vmux_ecs::WorkspaceKind
)]
pub(crate) struct FilePage;

#[vmux_page::page(
    page = "projects",
    component = Page,
    states = [vmux_api::address::PageAddressUiState, vmux_ui::theme::ThemeUiState, vmux_ecs::event::FileUiState, vmux_project::contract::ProjectUiState, vmux_project::contract::ProjectFormUiState, vmux_session::state::SessionScopeUiState, vmux_git::event::RepositorySummaryUiState, vmux_git::state::GitUiState, vmux_api::input::KeyClaimsUiState, vmux_api::page_location::PageLocationUiState],
    dom_group = "editor",
    subtree,
    takes = vmux_ecs::PageMetadata
)]
pub(crate) struct ProjectsPage;

#[vmux_page::page(
    owner = vmux_knowledge::Feature,
    component = Page,
    states = [vmux_api::address::PageAddressUiState, vmux_ui::theme::ThemeUiState, vmux_ecs::event::FileUiState, vmux_ecs::event::ProjectNavigation, vmux_api::input::KeyClaimsUiState, vmux_api::page_location::PageLocationUiState],
    dom_group = "editor",
    subtree,
    reactive_takes,
    takes = vmux_ecs::WorkspaceKind
)]
pub(crate) struct KnowledgePage;
