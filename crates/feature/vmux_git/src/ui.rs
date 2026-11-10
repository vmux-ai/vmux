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
mod review;
mod root;
mod shared;
mod shortcuts;
mod state;
mod status;

#[vmux_page::page(
    component = Page,
    document_url = "git://git/",
    states = ["PageAddressUiState", "ThemeUiState", "GitUiState", "GitReviewUiState", "KeyClaimsUiState", "PageLocationUiState"],
    subtree
)]
pub(crate) struct GitPage;
