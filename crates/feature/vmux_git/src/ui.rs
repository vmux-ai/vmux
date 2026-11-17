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
    document_url = "git://git/",
    states = [vmux_api::address::PageAddressUiState, vmux_ui::theme::ThemeUiState, crate::state::GitUiState, vmux_api::input::KeyClaimsUiState, vmux_api::page_location::PageLocationUiState],
    subtree
)]
pub(crate) struct GitPage;
