use bevy::prelude::*;
use vmux_core::host::persistence::PersistenceAppExt;
use vmux_flex::prelude::*;

use super::agent::LayoutAgentPlugin;
use super::command::LayoutRequestPlugin;
use super::persistence::LayoutPersistencePlugin;
use super::projection::LayoutUiProjectionPlugin;
use crate::active_pane::ActivePanePlugin;
use crate::archive::ArchivePlugin;
use crate::bookmark::BookmarkPlugin;
use crate::contract::LayoutContractPlugin;
use crate::event::CEF_RESERVED_HEIGHT_PX;
use crate::host::webview_reveal::WebviewRevealPlugin;
use crate::native_open::NativeOpenPlugin;
use crate::overlay::LayoutOverlayPlugin;
use crate::page_context::PageContextPlugin;
use crate::pane::PanePlugin;
use crate::side_sheet::SideSheetLayoutPlugin;
use crate::space::SpaceLayoutPlugin;
use crate::stack::StackPlugin;
use crate::tab::TabPlugin;
use crate::toggle::TogglePlugin;
use crate::warm_page::PrewarmPagesPlugin;
use crate::window::WindowLayoutPlugin;
use crate::worktree::WorktreePlugin;
use crate::{Header, LayoutStartupSet, Open, TerminalLayoutSpawnRequest, apply, settings};

#[vmux_native::page]
pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        {
            app.add_plugins((
                crate::ui::LayoutPage::plugin(),
                crate::ui::ErrorPage::plugin(),
            ));
        }
        app.add_systems(Startup, spawn_update_state)
            .add_systems(
                PostUpdate,
                sync_header_visibility.before(LayoutSystems::Layout),
            )
            .add_plugins((LayoutContractPlugin, LayoutRequestPlugin, LayoutAgentPlugin))
            .register_persisted::<Open>()
            .register_persisted::<crate::profile::Profile>()
            .init_resource::<settings::ConfirmCloseSettings>()
            .init_resource::<settings::ResolvedLocale>()
            .add_message::<TerminalLayoutSpawnRequest>()
            .add_message::<vmux_core::PageOpenRequest>()
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
            .add_plugins((
                apply::LayoutApplyPlugin,
                crate::tool::LayoutToolPlugin,
                LayoutUiProjectionPlugin,
                LayoutOverlayPlugin,
                SpaceLayoutPlugin,
                WindowLayoutPlugin,
                TabPlugin,
                PanePlugin,
                StackPlugin,
                ActivePanePlugin,
                SideSheetLayoutPlugin,
                WorktreePlugin,
            ))
            .add_plugins((
                Self::MANIFEST.plugin(),
                ErrorPage::MANIFEST.plugin(),
                LayoutPersistencePlugin,
                PageContextPlugin,
                TogglePlugin,
                WebviewRevealPlugin,
                ArchivePlugin,
                PrewarmPagesPlugin,
                NativeOpenPlugin,
                BookmarkPlugin,
                vmux_core::host::UiStatePlugin::<crate::state::LayoutUiState>::default(),
                crate::workspace_snapshot_publish::SnapshotPlugin,
                crate::pending_stack::PendingStackPlugin,
            ));
    }
}

fn spawn_update_state(mut commands: Commands) {
    commands.spawn((Name::new("Update state"), crate::UpdateState::default()));
}

fn sync_header_visibility(
    mut headers: Query<(&mut Visibility, &mut Node), With<Header>>,
    added: Query<Entity, (With<Header>, Added<Open>)>,
    mut removed: RemovedComponents<Open>,
) {
    for entity in &added {
        if let Ok((mut visibility, mut node)) = headers.get_mut(entity) {
            *visibility = Visibility::Visible;
            node.display = Display::Flex;
            node.height = Val::Px(CEF_RESERVED_HEIGHT_PX);
        }
    }

    for entity in removed.read() {
        if let Ok((mut visibility, mut node)) = headers.get_mut(entity) {
            *visibility = Visibility::Hidden;
            node.display = Display::None;
            node.height = Val::Px(0.0);
        }
    }
}

#[vmux_native::page(page = "error")]
pub struct ErrorPage;
