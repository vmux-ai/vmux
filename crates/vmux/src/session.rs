//! Session persistence with [moonshine-save]: hierarchical layout snapshot + legacy URL resource.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_core::{LastVisitedUrl, SessionSavePath, WebviewDocumentUrlEmit, allowed_navigation_url};
use vmux_layout::{LayoutTree, PaneLastUrl, Root, SessionLayoutSnapshot};
use vmux_webview::rebuild_session_snapshot;

const SAVE_FILENAME: &str = "last_session.ron";

/// Directory for vmux app cache (parent of the CEF cache folder).
pub fn vmux_cache_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let p = if cfg!(target_os = "macos") {
        PathBuf::from(home).join("Library/Caches/vmux")
    } else {
        PathBuf::from(home).join(".cache/vmux")
    };
    Some(p)
}

pub(crate) fn session_save_path() -> PathBuf {
    vmux_cache_dir()
        .map(|d| d.join(SAVE_FILENAME))
        .unwrap_or_else(|| std::env::temp_dir().join("vmux_last_session.ron"))
}

pub(crate) fn load_session(mut commands: Commands, path: PathBuf) {
    if path.is_file() {
        commands.trigger_load(LoadWorld::default_from_file(path));
    }
}

pub(crate) fn load_session_from_resource(commands: Commands, path: Res<SessionSavePath>) {
    load_session(commands, path.0.clone());
}

pub(crate) fn on_webview_document_url(
    trigger: On<Receive<WebviewDocumentUrlEmit>>,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    mut pane_queries: ParamSet<(Query<&mut PaneLastUrl>, Query<&PaneLastUrl>)>,
    layout_q: Query<&LayoutTree, With<Root>>,
    webview_src: Query<&WebviewSource>,
    path: Res<SessionSavePath>,
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
    *snapshot = rebuild_session_snapshot(tree, &pane_queries.p1(), &webview_src);
    commands.trigger_save(
        SaveWorld::default_into_file(path.0.clone()).include_resource::<SessionLayoutSnapshot>(),
    );
}

/// Registers layout snapshot, legacy URL resource, session path, moonshine load/save, and URL observer.
#[derive(Default)]
pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<LastVisitedUrl>()
            .init_resource::<LastVisitedUrl>()
            .init_resource::<SessionLayoutSnapshot>()
            .insert_resource(SessionSavePath(session_save_path()))
            .add_observer(moonshine_save::prelude::save_on_default_event)
            .add_observer(moonshine_save::prelude::load_on_default_event)
            .add_observer(on_webview_document_url)
            .add_systems(PreStartup, load_session_from_resource);
    }
}
