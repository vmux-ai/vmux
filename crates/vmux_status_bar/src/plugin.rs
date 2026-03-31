//! Bevy plugin: embedded HTTP for the Dioxus status bundle + [`PaneChromeStrip`](vmux_layout::PaneChromeStrip) wiring.

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use serde::Serialize;
use vmux_layout::{
    Active, Pane, PaneChromeNeedsUrl, PaneChromeOwner, PaneChromeStrip, PaneLastUrl,
    VmuxHostedWebPlugin, VmuxWebviewSurface,
};
use vmux_server::{EmbeddedServeDirRequest, EmbeddedServeDirStartup, PendingEmbeddedServeDir};

/// After this many seconds without a base URL from the embedded server, pane chrome uses [`STATUS_CHROME_UNAVAILABLE_HTML`].
const STATUS_UI_EMBEDDED_WAIT_SECS: f32 = 5.0;

/// Visible fallback when `dist/index.html` is missing, the loopback server never reports a port, or startup is stuck.
const STATUS_CHROME_UNAVAILABLE_HTML: &str = r#"<!DOCTYPE html><html><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width"/><style>html,body{margin:0;background:#1a1a1a;color:#9aa0a6;font:12px system-ui,-apple-system,sans-serif;height:100%;}body{display:flex;align-items:center;justify-content:center;text-align:center;padding:8px 12px;}p{margin:0;line-height:1.4;}small{display:block;margin-top:6px;opacity:.75;font-size:11px;}</style></head><body><div><p>Status bar UI did not load.</p><small>Run <code style="color:#bdc1c6">cargo build -p vmux_status_bar</code> (build.rs refreshes <code style="color:#bdc1c6">dist/</code>) or set <code style="color:#bdc1c6">VMUX_STATUS_UI_URL</code>.</small></div></body></html>"#;

#[derive(Resource, Default)]
pub struct StatusUiBaseUrl(pub Option<String>);

#[derive(Resource, Default)]
pub struct StatusUiUrlReceiver(pub Option<crossbeam_channel::Receiver<String>>);

/// When true, chrome strips use inline HTML instead of waiting on a loopback URL (missing dist, server failure, or timeout).
#[derive(Resource, Default)]
struct StatusUiChromeUnavailable(pub bool);

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
        commands.insert_resource(StatusUiChromeUnavailable(true));
        return;
    }

    let (tx, rx) = crossbeam_channel::bounded::<String>(1);
    let flag = Arc::new(Mutex::new(false));
    pending.0.push(EmbeddedServeDirRequest {
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

fn timeout_status_embedded(
    time: Res<Time>,
    mut wait_started: Local<Option<f32>>,
    ready: Res<StatusUiBaseUrl>,
    mut unavailable: ResMut<StatusUiChromeUnavailable>,
    rx: Res<StatusUiUrlReceiver>,
) {
    if unavailable.0 || ready.0.is_some() {
        *wait_started = None;
        return;
    }
    if rx.0.is_none() {
        *wait_started = None;
        return;
    }
    let now = time.elapsed_secs();
    let start = wait_started.get_or_insert(now);
    if now - *start >= STATUS_UI_EMBEDDED_WAIT_SECS {
        bevy::log::warn!(
            "vmux status bar: embedded HTTP server did not report a URL within {}s; using inline fallback",
            STATUS_UI_EMBEDDED_WAIT_SECS
        );
        unavailable.0 = true;
        *wait_started = None;
    }
}

fn apply_status_url_to_chrome(
    ready: Res<StatusUiBaseUrl>,
    unavailable: Res<StatusUiChromeUnavailable>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut WebviewSource), With<PaneChromeNeedsUrl>>,
) {
    if unavailable.0 {
        for (e, mut src) in &mut q {
            *src = WebviewSource::inline(STATUS_CHROME_UNAVAILABLE_HTML);
            commands.entity(e).remove::<PaneChromeNeedsUrl>();
        }
        return;
    }
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
            .init_resource::<StatusUiChromeUnavailable>()
            .init_resource::<StatusUiEmitState>()
            .add_systems(
                Startup,
                startup_status_server.in_set(EmbeddedServeDirStartup::FillPending),
            )
            .add_systems(
                Update,
                (
                    poll_status_url,
                    timeout_status_embedded.after(poll_status_url),
                    apply_status_url_to_chrome
                        .after(poll_status_url)
                        .after(timeout_status_embedded),
                    emit_status_to_active_chrome.after(apply_status_url_to_chrome),
                ),
            );
    }
}

impl VmuxHostedWebPlugin for StatusBarHostedPlugin {
    const SURFACE: VmuxWebviewSurface = VmuxWebviewSurface::PaneChrome;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_bar_plugin_registers_in_app() {
        let mut app = App::new();
        app.add_plugins(StatusBarHostedPlugin);
    }
}
