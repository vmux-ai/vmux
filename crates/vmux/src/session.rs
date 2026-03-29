//! Persist [`LastVisitedUrl`](vmux_core::LastVisitedUrl) with [moonshine-save].
//!
//! [crates.io `bevy_save`](https://crates.io/crates/bevy_save) currently targets Bevy 0.16.x; vmux uses Bevy 0.18, so we use moonshine-save instead.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_core::{
    allowed_navigation_url, LastVisitedUrl, WebviewDocumentUrlEmit,
};

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
    mut last: ResMut<LastVisitedUrl>,
    mut commands: Commands,
    path: Res<SessionSavePath>,
) {
    let url = trigger.url.trim();
    if url.is_empty() || !allowed_navigation_url(url) {
        return;
    }
    if last.0.as_str() == url {
        return;
    }
    last.0 = url.to_string();
    commands.trigger_save(
        SaveWorld::default_into_file(path.0.clone()).include_resource::<LastVisitedUrl>(),
    );
}

#[derive(Resource, Clone, Debug)]
pub(crate) struct SessionSavePath(pub PathBuf);

/// Registers [`LastVisitedUrl`], session file path, moonshine-save load/save, and URL-change persistence.
#[derive(Default)]
pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<LastVisitedUrl>()
            .init_resource::<LastVisitedUrl>()
            .insert_resource(SessionSavePath(session_save_path()))
            .add_observer(moonshine_save::prelude::save_on_default_event)
            .add_observer(moonshine_save::prelude::load_on_default_event)
            .add_observer(on_webview_document_url)
            .add_systems(PreStartup, load_session_from_resource);
    }
}
