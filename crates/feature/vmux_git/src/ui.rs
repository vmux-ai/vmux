pub use root::Page;
pub use shared::{DiffView, GitFooter};

mod branches;
mod changes;
mod command_log;
mod dashboard;
mod diff;
mod empty;
mod history;
mod model;
mod panel;
mod root;
mod shared;
mod shortcuts;
mod state;
mod status;

#[vmux_native::page(
    component = Page,
    subtree
)]
pub(crate) struct GitPage;

#[vmux_native::page(
    page = "document",
    component = Page
)]
pub(crate) struct LegacyGitPage;
