use bevy::prelude::*;
use bevy_cef::prelude::BinEventEmitterPlugin;
use vmux_server::{PAGE_READY_BIN_EVENT_ID, PageReady, mark_webview_page_ready_on_js_emit};

use crate::command_bar::handler::CommandBarInputPlugin;
use crate::focus_ring::FocusRingPlugin;
use crate::glass::GlassMaterialPlugin;
use crate::header::HeaderLayoutPlugin;
use crate::pane::PanePlugin;
use crate::profile::ProfilePlugin;
use crate::scene::ScenePlugin;
use crate::side_sheet::SideSheetLayoutPlugin;
use crate::space::SpacePlugin;
use crate::stack::StackPlugin;
use crate::tab::TabPlugin;
use crate::toggle::TogglePlugin;
use crate::webview_reveal::WebviewRevealPlugin;
use crate::window::WindowPlugin;
use crate::{
    BrowserGoBackRequest, BrowserGoForwardRequest, BrowserNavigateRequest, LayoutSpawnRequest,
    LayoutStartupSet, NewStackContext, Open, OpenInNewStackRequest, TabLayoutSpawnRequest,
    reconcile, settings,
};

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Open>()
            .init_resource::<NewStackContext>()
            .init_resource::<settings::ConfirmCloseSettings>()
            .add_message::<LayoutSpawnRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_message::<vmux_core::PageOpenRequest>()
            .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
            .add_message::<vmux_core::agent::RestartAgentPty>()
            .add_message::<BrowserNavigateRequest>()
            .add_message::<BrowserGoBackRequest>()
            .add_message::<BrowserGoForwardRequest>()
            .add_message::<OpenInNewStackRequest>()
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
            .add_plugins(BinEventEmitterPlugin::<(PageReady,)>::with_id(
                PAGE_READY_BIN_EVENT_ID,
            ))
            .add_observer(mark_webview_page_ready_on_js_emit)
            .add_plugins((
                ProfilePlugin,
                SpacePlugin,
                ScenePlugin,
                WindowPlugin,
                TabPlugin,
                PanePlugin,
                StackPlugin,
                FocusRingPlugin,
                GlassMaterialPlugin,
                SideSheetLayoutPlugin,
                HeaderLayoutPlugin,
            ))
            .add_plugins((CommandBarInputPlugin, TogglePlugin, WebviewRevealPlugin));
    }
}
