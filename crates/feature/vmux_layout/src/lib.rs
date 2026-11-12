#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

#[cfg(feature = "geometry")]
pub use geometry::PaneSplitGaps;
pub use hierarchy::Hierarchy;
#[cfg(feature = "store")]
pub use pane_store::{Schema as PaneStore, StorePlugin};
pub use topology::{
    LayoutPlugin, Pane, PaneId, PaneSet, PaneSize, PaneSplit, PaneSplitDirection, SelectRequest,
    SelectionChanged, SpawnSeq,
};

pub const PANE_GAP_PX: f32 = 4.0;

#[cfg(feature = "geometry")]
mod geometry;
mod hierarchy;
#[cfg(feature = "store")]
mod pane_store;
mod topology;

#[cfg(all(host, feature = "host"))]
pub use host::{
    AgentOpenBeside, AgentPaneDirection, Browser, BrowserGoBackRequest, BrowserGoForwardRequest,
    BrowserNavigateRequest, CloseRequiresConfirmation, CloseStackReason, CloseStackRequest,
    ContributedCommandChosen, ErrorPage, Header, LauncherDismissRequest, LayoutCef,
    LayoutCefPlugin, LayoutCefStateSet, LayoutContractPlugin, LayoutRestoreCommitted,
    LayoutRestoreRequest, LayoutRuntimePlugin, LayoutStartupSet, LayoutStoreSet,
    LayoutUiStateUpdates, Loading, NavigationState, NewTabRequest, Open, OpenBesideRequest,
    OpenInNewStackRequest, PendingWebviewReveal, ReloadRevision, TabLayoutSpawnContent,
    TabLayoutSpawnRequest, TerminalLayoutSpawnRequest, UpdateState, active, active_pane, apply,
    archive, bookmark, cef, contract, hosted_page, overlay, page_context, page_slot, pane,
    placement, plugin, presentation, profile, projection, settings, side_sheet, snapshot, space,
    stack, tab, target, toggle, tool, unit, warm_page, window, workspace_snapshot,
    workspace_snapshot_publish, worktree,
};

#[cfg(all(host, feature = "host"))]
pub(crate) struct Feature;

#[cfg(all(host, feature = "host"))]
impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(feature = "contracts")]
pub mod event;
#[cfg(feature = "contracts")]
pub mod protocol;
#[cfg(feature = "contracts")]
pub mod reconcile;
#[cfg(feature = "contracts")]
pub mod state;

#[cfg(all(ui, feature = "host"))]
pub mod ui;

#[cfg(all(host, feature = "host"))]
pub mod host;
