#![allow(non_snake_case)]

pub(crate) use input::EditorFocus;
pub use workspace::Page;
pub(super) use workspace::{HOVER_DELAY_MS, Mode, SCROLL_ID};

mod breadcrumb;
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
mod sidebar;
mod state;
mod status;
mod text_geometry;
mod text_style;
mod toolbar;
mod workspace;

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
