//! Session persistence with [moonshine-save]: hierarchical layout snapshot + legacy URL resource.

use std::path::PathBuf;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_core::WebviewDocumentUrlEmit;
use vmux_core::{SessionSavePath, SessionSaveQueue};
use vmux_input::tmux_prefix_commands;
use vmux_layout::{
    LayoutTree, PaneLastUrl, Root, SessionLayoutSnapshot, allowed_navigation_url, setup_vmux_panes,
    split_active_pane,
};
use vmux_settings::{VmuxAppSettings, VmuxCacheDir, VmuxCacheDirInitSet};
use vmux_webview::rebuild_session_snapshot;

const SAVE_FILENAME: &str = "last_session.ron";

fn session_save_path(cache: &VmuxCacheDir) -> PathBuf {
    cache
        .0
        .clone()
        .map(|d| d.join(SAVE_FILENAME))
        .unwrap_or_else(|| std::env::temp_dir().join("vmux_last_session.ron"))
}

fn init_session_save_path(mut commands: Commands, cache: Res<VmuxCacheDir>) {
    commands.insert_resource(SessionSavePath(session_save_path(&cache)));
}

fn load_session(mut commands: Commands, path: PathBuf) {
    if path.is_file() {
        commands.trigger_load(LoadWorld::default_from_file(path));
    }
}

fn load_session_from_resource(commands: Commands, path: Res<SessionSavePath>) {
    load_session(commands, path.0.clone());
}

/// Writes [`SessionLayoutSnapshot`] and [`VmuxAppSettings`] to `path` (same file moonshine loads in `vmux`).
pub fn save_session_snapshot_to_file(commands: &mut Commands, path: PathBuf) {
    commands.trigger_save(
        SaveWorld::default_into_file(path)
            .include_resource::<SessionLayoutSnapshot>()
            .include_resource::<VmuxAppSettings>(),
    );
}

fn drain_session_save_queue_inner(queue: &mut SessionSaveQueue, commands: &mut Commands) {
    let paths = std::mem::take(&mut queue.0);
    for path in paths {
        save_session_snapshot_to_file(commands, path);
    }
}

fn drain_session_save_queue_startup(mut queue: ResMut<SessionSaveQueue>, mut commands: Commands) {
    drain_session_save_queue_inner(&mut queue, &mut commands);
}

fn drain_session_save_queue_update(mut queue: ResMut<SessionSaveQueue>, mut commands: Commands) {
    drain_session_save_queue_inner(&mut queue, &mut commands);
}

fn on_webview_document_url(
    trigger: On<Receive<WebviewDocumentUrlEmit>>,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    mut pane_queries: ParamSet<(Query<&mut PaneLastUrl>, Query<&PaneLastUrl>)>,
    layout_q: Query<&LayoutTree, With<Root>>,
    webview_src: Query<&WebviewSource>,
    (path, settings): (Res<SessionSavePath>, Res<VmuxAppSettings>),
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let url = trigger.url.trim();
    if url.is_empty() || !allowed_navigation_url(url) {
        return;
    }
    {
        let mut q = pane_queries.p0();
        let Ok(mut pl) = q.get_mut(webview) else {
            return;
        };
        if pl.0.as_str() == url {
            return;
        }
        pl.0 = url.to_string();
    }
    let Ok(tree) = layout_q.single() else {
        return;
    };
    *snapshot = rebuild_session_snapshot(
        tree,
        &pane_queries.p1(),
        &webview_src,
        settings.default_webview_url.as_str(),
    );
    save_session_snapshot_to_file(&mut commands, path.0.clone());
}

fn save_session_on_app_exit(
    path: Res<SessionSavePath>,
    mut exits: MessageReader<AppExit>,
    mut commands: Commands,
) {
    for _ in exits.read() {
        save_session_snapshot_to_file(&mut commands, path.0.clone());
    }
}

/// Registers layout snapshot, legacy URL resource, session path, moonshine load/save, and URL observer.
#[derive(Default)]
pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SessionLayoutSnapshot>()
            .init_resource::<SessionSaveQueue>()
            .add_observer(moonshine_save::prelude::save_on_default_event)
            .add_observer(moonshine_save::prelude::load_on_default_event)
            .add_observer(on_webview_document_url)
            .add_systems(
                PreStartup,
                (
                    init_session_save_path.after(VmuxCacheDirInitSet),
                    load_session_from_resource.after(init_session_save_path),
                ),
            )
            .add_systems(
                Startup,
                drain_session_save_queue_startup.after(setup_vmux_panes),
            )
            .add_systems(
                Update,
                drain_session_save_queue_update
                    .after(tmux_prefix_commands)
                    .after(split_active_pane),
            )
            .add_systems(Last, save_session_on_app_exit);
    }
}
