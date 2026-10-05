#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

#[cfg(host)]
pub use host::{
    AgentOpenBeside, AgentPaneDirection, Browser, BrowserGoBackRequest, BrowserGoForwardRequest,
    BrowserNavigateRequest, CloseRequiresConfirmation, CloseStackReason, CloseStackRequest,
    ContributedCommandChosen, ErrorPage, Header, LauncherDismissRequest, LayoutCef,
    LayoutCefPlugin, LayoutCefStateSet, LayoutContractPlugin, LayoutPersistenceSet, LayoutPlugin,
    LayoutStartupSet, LayoutUiStateUpdates, Loading, NavigationState, NewTabRequest, Open,
    OpenBesideRequest, OpenInNewStackRequest, PendingWebviewReveal, ReloadRevision,
    TabLayoutSpawnContent, TabLayoutSpawnRequest, TerminalLayoutSpawnRequest, UpdateState, active,
    active_pane, apply, archive, bookmark, cef, contract, hosted_page, overlay, page_context, pane,
    pending_stack, placement, plugin, profile, projection, settings, side_sheet, snapshot, space,
    stack, tab, target, toggle, tool, unit, warm_page, window, workspace_snapshot,
    workspace_snapshot_publish, worktree,
};

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod event;
pub mod protocol;
pub mod reconcile;
pub mod state;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
pub mod host;
