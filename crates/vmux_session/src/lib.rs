//! Session persistence with [moonshine-save]: hierarchical layout snapshot + legacy URL resource.

use std::path::PathBuf;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_core::{NavigationHistory, NavigationHistoryFile, WebviewDocumentUrlEmit};
pub use vmux_core::{
    NavigationHistoryPath, NavigationHistorySaveQueue, SessionSavePath, SessionSaveQueue,
};
use vmux_layout::{
    History, Layout, Pane, PaneLastUrl, SessionLayoutSnapshot, Webview,
    allowed_navigation_url,
};
use vmux_settings::{VmuxAppSettings, VmuxCacheDir, VmuxCacheDirInitSet};
use vmux_webview::{rebuild_session_snapshot, setup_vmux_panes_startup};

const SAVE_FILENAME: &str = "last_session.ron";
const NAV_HISTORY_FILENAME: &str = "navigation_history.ron";

fn session_save_path(cache: &VmuxCacheDir) -> PathBuf {
    cache
        .0
        .clone()
        .map(|d| d.join(SAVE_FILENAME))
        .unwrap_or_else(|| std::env::temp_dir().join("vmux_last_session.ron"))
}

fn navigation_history_path(cache: &VmuxCacheDir) -> PathBuf {
    cache
        .0
        .clone()
        .map(|d| d.join(NAV_HISTORY_FILENAME))
        .unwrap_or_else(|| std::env::temp_dir().join("vmux_navigation_history.ron"))
}

fn init_session_save_path(mut commands: Commands, cache: Res<VmuxCacheDir>) {
    commands.insert_resource(SessionSavePath(session_save_path(&cache)));
    commands.insert_resource(NavigationHistoryPath(navigation_history_path(&cache)));
}

fn load_navigation_history_from_disk(mut commands: Commands, path: Res<NavigationHistoryPath>) {
    let p = &path.0;
    let hist = if p.is_file() {
        match std::fs::read_to_string(p) {
            Ok(s) => match ron::from_str::<NavigationHistoryFile>(&s) {
                Ok(f) => NavigationHistory::from(f),
                Err(e) => {
                    warn!("vmux_session: bad navigation_history.ron {:?}: {e}", p);
                    NavigationHistory::default()
                }
            },
            Err(e) => {
                warn!("vmux_session: read navigation_history.ron {:?}: {e}", p);
                NavigationHistory::default()
            }
        }
    } else {
        NavigationHistory::default()
    };
    commands.insert_resource(hist);
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

pub fn save_navigation_history_to_disk(path: &PathBuf, hist: &NavigationHistory) {
    let file = NavigationHistoryFile::from(hist);
    let Ok(s) = ron::to_string(&file) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(path, s.as_bytes()) {
        warn!("vmux_session: write navigation_history {:?}: {e}", path);
    }
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

fn drain_navigation_history_save_queue(
    mut queue: ResMut<NavigationHistorySaveQueue>,
    path: Res<NavigationHistoryPath>,
    hist: Res<NavigationHistory>,
) {
    let paths = std::mem::take(&mut queue.0);
    if paths.is_empty() {
        return;
    }
    save_navigation_history_to_disk(&path.0, &hist);
}

fn on_webview_document_url(
    trigger: On<Receive<WebviewDocumentUrlEmit>>,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    mut pane_queries: ParamSet<(Query<&mut PaneLastUrl>, Query<&PaneLastUrl>)>,
    layout_q: Query<&Layout, With<vmux_layout::Window>>,
    webview_src: Query<&WebviewSource>,
    history_overlay: Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    (path, settings): (Res<SessionSavePath>, Res<VmuxAppSettings>),
    mut session_queue: ResMut<SessionSaveQueue>,
    mut nav_hist: ResMut<NavigationHistory>,
    nav_path: Res<NavigationHistoryPath>,
    mut nav_queue: ResMut<NavigationHistorySaveQueue>,
) {
    let ev = trigger.event();
    let webview = ev.webview;
    if history_overlay.contains(webview) {
        return;
    }
    let Some(url) = ev.url.as_deref() else {
        return;
    };
    let url = url.trim();
    if url.is_empty() || !allowed_navigation_url(url) {
        return;
    }
    // `PaneLastUrl` is seeded with the spawn URL; the preload script's first `{ url }` emit matches it.
    // We must still record that visit in `NavigationHistory` (deduped inside `push_visit`).
    let url_changed = {
        let mut q = pane_queries.p0();
        let Ok(mut pl) = q.get_mut(webview) else {
            return;
        };
        if pl.0.as_str() != url {
            pl.0 = url.to_string();
            true
        } else {
            false
        }
    };
    if nav_hist.push_visit(url.to_string()) {
        nav_queue.0.push(nav_path.0.clone());
    }
    if url_changed {
        let Ok(tree) = layout_q.single() else {
            return;
        };
        *snapshot = rebuild_session_snapshot(
            tree,
            &pane_queries.p1(),
            &webview_src,
            &history_overlay,
            settings.browser.default_webview_url.as_str(),
        );
        session_queue.0.push(path.0.clone());
    }
}

/// Flushes session to disk on shutdown. `AppExit` uses Bevy’s message bus, not the ECS observer
/// `Event` system, so `add_observer(On<AppExit>)` is not applicable; `MessageReader` in `Last` is correct.
fn save_session_on_app_exit(
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    layout_q: Query<&Layout, With<vmux_layout::Window>>,
    pane_last: Query<&PaneLastUrl>,
    webview_src: Query<&WebviewSource>,
    history_panes: Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    settings: Res<VmuxAppSettings>,
    path: Res<SessionSavePath>,
    nav_path: Res<NavigationHistoryPath>,
    hist: Res<NavigationHistory>,
    mut exits: MessageReader<AppExit>,
    mut commands: Commands,
) {
    for _ in exits.read() {
        match layout_q.single() {
            Ok(tree) => {
                *snapshot = rebuild_session_snapshot(
                    tree,
                    &pane_last,
                    &webview_src,
                    &history_panes,
                    settings.browser.default_webview_url.as_str(),
                );
            }
            Err(e) => {
                warn!("vmux_session: skip layout snapshot rebuild on exit (no layout Window / Layout?): {e}");
            }
        }
        save_session_snapshot_to_file(&mut commands, path.0.clone());
        save_navigation_history_to_disk(&nav_path.0, &hist);
    }
}

/// Registers layout snapshot, legacy URL resource, session path, moonshine load/save, and URL observer.
#[derive(Default)]
pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SessionLayoutSnapshot>()
            .init_resource::<SessionSaveQueue>()
            .init_resource::<NavigationHistorySaveQueue>()
            .add_observer(moonshine_save::prelude::save_on_default_event)
            .add_observer(moonshine_save::prelude::load_on_default_event)
            .add_observer(on_webview_document_url)
            .add_systems(
                PreStartup,
                (
                    init_session_save_path.after(VmuxCacheDirInitSet),
                    load_navigation_history_from_disk.after(init_session_save_path),
                    load_session_from_resource.after(init_session_save_path),
                ),
            )
            .add_systems(
                Startup,
                drain_session_save_queue_startup.after(setup_vmux_panes_startup),
            )
            // After all `Update` systems and observers (including URL `Receive`), so enqueued paths flush same frame.
            .add_systems(
                PostUpdate,
                (
                    drain_session_save_queue_update,
                    drain_navigation_history_save_queue.after(drain_session_save_queue_update),
                ),
            )
            .add_systems(Last, save_session_on_app_exit);
    }
}

#[cfg(test)]
mod ecs_tests {
    use super::*;

    #[test]
    fn session_plugin_registers_in_app() {
        let mut app = App::new();
        app.add_plugins(SessionPlugin);
    }
}
