#![allow(non_snake_case)]

pub(crate) use input::EditorFocus;
pub(crate) use lsp::LspPage;
pub use page::Page;
pub(super) use page::{HOVER_DELAY_MS, Mode, SCROLL_ID, diff_tone};

mod breadcrumb;
mod diagnostic;
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
mod note_text;
mod page;
mod sidebar;
mod state;
mod status;
mod text_geometry;
mod text_style;
mod toolbar;

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
