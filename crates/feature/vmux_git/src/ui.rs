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

#[vmux_page::page(
    component = Page,
    states = ["ThemeUiState", "GitUiState", "KeyClaimsUiState"],
    subtree
)]
pub(crate) struct GitPage;

#[vmux_page::page(
    page = "document",
    component = Page,
    states = ["ThemeUiState", "GitUiState", "KeyClaimsUiState"]
)]
pub(crate) struct LegacyGitPage;
