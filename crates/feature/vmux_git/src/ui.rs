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
    states = [
        vmux_ui::theme::UiTheme,
        crate::state::UiGit,
        vmux_api::input::UiKeyClaims,
    ],
    subtree
)]
pub(crate) struct GitPage;

#[vmux_page::page(
    page = "document",
    component = Page,
    states = [
        vmux_ui::theme::UiTheme,
        crate::state::UiGit,
        vmux_api::input::UiKeyClaims,
    ]
)]
pub(crate) struct LegacyGitPage;
