//! Bevy plugin: embedded HTTP for the Dioxus status bundle + [`PaneChromeStrip`](vmux_layout::PaneChromeStrip) wiring.

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use serde::Serialize;
use vmux_layout::{
    Active, Pane, PaneChromeNeedsUrl, PaneChromeOwner, PaneChromeStrip, PaneLastUrl,
    VmuxHostedWebPlugin, VmuxWebviewSurface, setup_vmux_panes,
};
use vmux_server::{EmbeddedServeDirRequest, EmbeddedServeDirStartup, PendingEmbeddedServeDir};

#[derive(Resource, Default)]
pub struct StatusUiBaseUrl(pub Option<String>);

#[derive(Resource, Default)]
pub struct StatusUiUrlReceiver(pub Option<crossbeam_channel::Receiver<String>>);

#[derive(Resource, Default)]
struct StatusUiEmitState {
    last_hash: Option<u64>,
}

#[derive(Serialize)]
struct VmuxStatusPayload {
    user: String,
    host: String,
    active_url: String,
}

fn status_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "?".to_string())
}

static STATUS_HOST: LazyLock<String> = LazyLock::new(|| {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| {
            hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "localhost".to_string())
        })
});

fn status_bar_dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dist")
}

fn startup_status_server(mut commands: Commands, mut pending: ResMut<PendingEmbeddedServeDir>) {
    if let Ok(u) = std::env::var("VMUX_STATUS_UI_URL") {
        let u = u.trim();
        if !u.is_empty() {
            commands.insert_resource(StatusUiBaseUrl(Some(u.to_string())));
            return;
        }
    }

    let dist = status_bar_dist_dir();
    if !dist.join("index.html").is_file() {
        bevy::log::warn!(
            "vmux status bar: missing {}; add `crates/vmux_status_bar/dist/` or set VMUX_STATUS_UI_URL",
            dist.display()
        );
    }

    let (tx, rx) = crossbeam_channel::bounded::<String>(1);
    let flag = Arc::new(Mutex::new(false));
    pending.0 = Some(EmbeddedServeDirRequest {
        root: dist,
        tx,
        shutdown: flag,
    });
    commands.insert_resource(StatusUiUrlReceiver(Some(rx)));
}

fn poll_status_url(mut ready: ResMut<StatusUiBaseUrl>, rx: ResMut<StatusUiUrlReceiver>) {
    if ready.0.is_some() {
        return;
    }
    let Some(ref r) = rx.0 else {
        return;
    };
    if let Ok(u) = r.try_recv() {
        ready.0 = Some(u);
    }
}

fn apply_status_url_to_chrome(
    ready: Res<StatusUiBaseUrl>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut WebviewSource), With<PaneChromeNeedsUrl>>,
) {
    let Some(url) = ready.0.as_ref() else {
        return;
    };
    for (e, mut src) in &mut q {
        *src = WebviewSource::new(url.clone());
        commands.entity(e).remove::<PaneChromeNeedsUrl>();
    }
}

fn emit_status_to_active_chrome(
    mut commands: Commands,
    mut state: ResMut<StatusUiEmitState>,
    chrome: Query<(Entity, &PaneChromeOwner), With<PaneChromeStrip>>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    panes: Query<Entity, With<Pane>>,
    pane_url: Query<&PaneLastUrl>,
) {
    let Some(active_ent) = active.iter().next().or_else(|| panes.iter().next()) else {
        return;
    };
    let Some((wv, _)) = chrome.iter().find(|(_, o)| o.0 == active_ent) else {
        return;
    };
    let url = pane_url
        .get(active_ent)
        .map(|p| p.0.clone())
        .unwrap_or_default();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    let h = hasher.finish();
    if state.last_hash == Some(h) {
        return;
    }
    state.last_hash = Some(h);

    let payload = VmuxStatusPayload {
        user: status_user(),
        host: STATUS_HOST.clone(),
        active_url: url,
    };
    commands.trigger(HostEmitEvent::new(wv, "vmux_status", &payload));
}

/// Serves the Dioxus status bundle from `dist/` and drives [`PaneChromeStrip`] webviews.
#[derive(Default)]
pub struct StatusBarHostedPlugin;

impl Plugin for StatusBarHostedPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StatusUiBaseUrl>()
            .init_resource::<StatusUiUrlReceiver>()
            .init_resource::<StatusUiEmitState>()
            .add_systems(
                Startup,
                startup_status_server
                    .in_set(EmbeddedServeDirStartup::FillPending)
                    .after(setup_vmux_panes),
            )
            .add_systems(
                Update,
                (
                    poll_status_url,
                    apply_status_url_to_chrome.after(poll_status_url),
                    emit_status_to_active_chrome.after(apply_status_url_to_chrome),
                ),
            );
    }
}

impl VmuxHostedWebPlugin for StatusBarHostedPlugin {
    const SURFACE: VmuxWebviewSurface = VmuxWebviewSurface::PaneChrome;
}
