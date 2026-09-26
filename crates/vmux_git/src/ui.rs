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

pub use diff_projection::DiffViewRow;
pub use root::Page;
pub use shared::{DiffView, GitFooter};

#[vmux_native::page(
    component = Page,
    document_url = crate::GIT_DOCUMENT_URL,
    subtree
)]
pub(crate) struct GitPage;

#[vmux_native::page(
    file = "src/document.ron",
    component = Page
)]
pub(crate) struct LegacyGitPage;
