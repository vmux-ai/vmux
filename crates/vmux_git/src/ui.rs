mod branches;
mod changes;
mod command_log;
mod dashboard;
mod diff;
mod diff_projection;
mod empty;
mod history;
mod model;
mod panel;
mod root;
mod shared;
mod shortcuts;
mod state;
mod status;
mod workspace;

pub use diff_projection::{DiffViewRow, EditorDiffMarker};
pub use root::Page;
pub use shared::{DiffView, GitFooter, GitStatusFeed};

#[vmux_native::page(
    url = crate::GIT_PAGE_URL,
    title = "Git",
    component = Page,
    document_url = crate::GIT_DOCUMENT_URL,
    subtree
)]
pub(crate) struct GitPage;

#[vmux_native::page(
    url = crate::GIT_DOCUMENT_URL,
    title = "Git",
    component = Page
)]
pub(crate) struct LegacyGitPage;
