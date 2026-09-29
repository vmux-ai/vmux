#![allow(non_snake_case)]

mod breadcrumb;
mod directory;
mod dom;
mod editor;
mod explorer;
mod input;
mod key;
mod lsp;
mod markdown;
mod menu;
mod note;
mod page;
mod sidebar;
mod state;
mod status;
mod text_geometry;
mod toolbar;

pub(crate) use input::{FIND_INPUT_ID, focus_file_input, focus_find_input};
pub(super) use input::{INPUT_ID, focus_container};
pub(crate) use lsp::LspPage;
pub use page::Page;
pub(super) use page::{
    HOVER_DELAY_MS, Mode, SCROLL_ID, diff_marker_row_class, diff_marker_sign,
    diff_marker_text_class, diff_tone,
};
pub(crate) use sidebar::ExplorerPane;

#[vmux_native::page(
    component = Page,
    dom_group = "editor",
    subtree
)]
pub(crate) struct FilePage;

#[vmux_native::page(
    page = "projects",
    component = Page,
    dom_group = "editor",
    subtree
)]
pub(crate) struct ProjectsPage;

#[vmux_native::page(
    file = "../vmux_knowledge/src/feature.ron",
    component = Page,
    dom_group = "editor",
    subtree
)]
pub(crate) struct KnowledgePage;
