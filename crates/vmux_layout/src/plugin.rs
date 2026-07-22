use bevy::prelude::*;
use bevy_cef::prelude::BinEventEmitterPlugin;
use vmux_core::page::{PAGE_READY_BIN_EVENT_ID, PageReady, mark_webview_page_ready};

use crate::active_panes::ActivePanesPlugin;
use crate::archive::ArchivePlugin;
use crate::bookmark::BookmarkPlugin;
use crate::command_bar::handler::CommandBarInputPlugin;
#[cfg(feature = "player-mode")]
use crate::focus_ring::FocusRingPlugin;
use crate::header::HeaderLayoutPlugin;
use crate::pane::PanePlugin;
use crate::profile::ProfilePlugin;
use crate::scene::ScenePlugin;
use crate::side_sheet::SideSheetLayoutPlugin;
use crate::space::SpacePlugin;
use crate::stack::StackPlugin;
use crate::tab::TabPlugin;
use crate::toggle::TogglePlugin;
use crate::warm_page::PrewarmPagesPlugin;
use crate::webview_reveal::WebviewRevealPlugin;
use crate::window::WindowPlugin;
use crate::worktree::WorktreePlugin;
use crate::{
    BrowserGoBackRequest, BrowserGoForwardRequest, BrowserNavigateRequest, ExtensionInstallRequest,
    LayoutSpawnRequest, LayoutStartupSet, NewStackContext, Open, OpenInNewStackRequest,
    TabLayoutSpawnRequest, reconcile, settings,
};

/// Wires the layout shell: spaces, tabs, panes, stacks, focus ring, header/side-sheet,
/// command-bar input, and layout apply/snapshot, aggregating the per-area sub-plugins.
pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Open>()
            .init_resource::<NewStackContext>()
            .init_resource::<settings::ConfirmCloseSettings>()
            .init_resource::<settings::ResolvedLocale>()
            .init_resource::<crate::UpdateState>()
            .add_message::<LayoutSpawnRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_message::<vmux_core::PageOpenRequest>()
            .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
            .add_message::<vmux_core::agent::RestartAgentPty>()
            .add_message::<BrowserNavigateRequest>()
            .add_message::<BrowserGoBackRequest>()
            .add_message::<BrowserGoForwardRequest>()
            .add_message::<OpenInNewStackRequest>()
            .add_message::<ExtensionInstallRequest>()
            .add_message::<reconcile::LayoutApplyRequest>()
            .add_message::<reconcile::LayoutApplyResponse>()
            .add_message::<reconcile::LayoutSnapshotRequest>()
            .add_message::<reconcile::LayoutSnapshotResponse>()
            .configure_sets(
                Startup,
                (
                    LayoutStartupSet::Window,
                    LayoutStartupSet::Persistence,
                    LayoutStartupSet::DefaultTab,
                    LayoutStartupSet::Post,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    reconcile::apply_layout_requests,
                    reconcile::serve_snapshot_requests,
                ),
            )
            .add_systems(
                Update,
                crate::debug::handle_debug_page_open
                    .in_set(vmux_core::PageOpenSet::HandleKnownPages),
            )
            .add_plugins(BinEventEmitterPlugin::<(PageReady,)>::with_id(
                PAGE_READY_BIN_EVENT_ID,
            ))
            .add_observer(mark_webview_page_ready)
            .add_plugins((
                ProfilePlugin,
                SpacePlugin,
                ScenePlugin,
                WindowPlugin,
                TabPlugin,
                PanePlugin,
                StackPlugin,
                ActivePanesPlugin,
                SideSheetLayoutPlugin,
                HeaderLayoutPlugin,
                WorktreePlugin,
            ))
            .add_plugins((
                CommandBarInputPlugin,
                TogglePlugin,
                WebviewRevealPlugin,
                ArchivePlugin,
                PrewarmPagesPlugin,
                BookmarkPlugin,
            ));
        #[cfg(feature = "player-mode")]
        app.add_plugins(FocusRingPlugin);
    }
}
