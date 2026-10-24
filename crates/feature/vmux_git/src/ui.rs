pub use root::Page;
pub use shared::{DiffView, GitFooter};

mod branches;
mod changes;
mod dashboard;
mod diff;
mod empty;
mod history;
mod model;
mod operation;
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
