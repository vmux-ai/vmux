use bevy::prelude::*;
use bevy_cef::prelude::UiEventPlugin;
use vmux_core::page::{PageReady, mark_webview_page_ready};

use super::command::LayoutRequestPlugin;
use super::projection::LayoutUiProjectionPlugin;
use crate::active_pane::ActivePanePlugin;
use crate::archive::ArchivePlugin;
use crate::bookmark::BookmarkPlugin;
use crate::cef::LayoutCefPlugin;
use crate::contract::LayoutContractPlugin;
use crate::host::header::HeaderLayoutPlugin;
use crate::host::webview_reveal::WebviewRevealPlugin;
use crate::native_open::NativeOpenPlugin;
use crate::overlay::LayoutOverlayPlugin;
use crate::page_context::PageContextPlugin;
use crate::pane::PanePlugin;
use crate::profile::ProfilePlugin;
use crate::side_sheet::SideSheetLayoutPlugin;
use crate::space::SpaceLayoutPlugin;
use crate::stack::StackPlugin;
use crate::tab::TabPlugin;
use crate::toggle::TogglePlugin;
use crate::warm_page::PrewarmPagesPlugin;
use crate::window::WindowLayoutPlugin;
use crate::worktree::WorktreePlugin;
use crate::{
    LayoutStartupSet, Open, TabLayoutSpawnRequest, TerminalLayoutSpawnRequest, apply, settings,
};

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins((
            crate::ui::LayoutPage::plugin(),
            crate::tool_page::ToolsPage::plugin(),
            crate::vault_page::VaultPage::plugin(),
            crate::extensions_page::ExtensionsPage::plugin(),
            crate::error_page::ErrorPage::plugin(),
        ));
        app.add_plugins((LayoutContractPlugin, LayoutRequestPlugin))
            .register_type::<Open>()
            .init_resource::<settings::ConfirmCloseSettings>()
            .init_resource::<settings::ResolvedLocale>()
            .init_resource::<crate::UpdateState>()
            .add_message::<TerminalLayoutSpawnRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_message::<vmux_core::PageOpenRequest>()
            .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
            .add_message::<vmux_core::agent::RestartAgentPty>()
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
                (apply::apply_layout_requests, apply::serve_snapshot_requests),
            )
            .add_plugins(UiEventPlugin::<(PageReady,)>::default())
            .add_observer(mark_webview_page_ready)
            .add_plugins((
                crate::tool::LayoutToolPlugin,
                crate::bookmark_tool::BookmarkToolPlugin,
                ProfilePlugin,
                LayoutUiProjectionPlugin,
                LayoutOverlayPlugin,
                SpaceLayoutPlugin,
                WindowLayoutPlugin,
                TabPlugin,
                PanePlugin,
                StackPlugin,
                ActivePanePlugin,
                SideSheetLayoutPlugin,
                HeaderLayoutPlugin,
                WorktreePlugin,
                PageContextPlugin,
            ))
            .add_plugins((
                TogglePlugin,
                WebviewRevealPlugin,
                ArchivePlugin,
                PrewarmPagesPlugin,
                NativeOpenPlugin,
                BookmarkPlugin,
                LayoutCefPlugin,
                vmux_core::host::UiStatePlugin::<crate::state::LayoutUiState>::default(),
                crate::workspace_snapshot_publish::SnapshotPlugin,
                crate::overlay_adopt::OverlayAdoptPlugin,
                crate::pending_stack::PendingStackPlugin,
            ));
    }
}
