//! Startup: drain embedded HTTP base URLs before pane spawn so history/status UIs load like normal pages.

use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_cef::prelude::WebviewExtendStandardMaterial;
use vmux_core::{SessionSavePath, SessionSaveQueue};
use vmux_history::{HistoryUiBaseUrl, HistoryUiUrlReceiver};
use vmux_layout::LastVisitedUrl;
use vmux_layout::{setup_vmux_panes, LoadingBarMaterial, SessionLayoutSnapshot};
use vmux_server::EmbeddedServeDirStartup;
use vmux_settings::VmuxAppSettings;
use vmux_status_bar::{StatusUiBaseUrl, StatusUiUrlReceiver};

/// Wait for each embedded server thread to send `http://127.0.0.1:…/` so [`setup_vmux_panes_startup`]
/// can open history (and status chrome in the same frame) on the real URL — no `about:blank` hop.
pub fn startup_drain_embedded_ui_urls(
    mut status_ready: ResMut<StatusUiBaseUrl>,
    status_rx: Res<StatusUiUrlReceiver>,
    mut hist_ready: ResMut<HistoryUiBaseUrl>,
    hist_rx: Res<HistoryUiUrlReceiver>,
) {
    const WAIT: Duration = Duration::from_secs(3);
    let t0 = Instant::now();
    drain_channel(&mut status_ready.0, status_rx.0.as_ref(), WAIT);
    drain_channel(&mut hist_ready.0, hist_rx.0.as_ref(), WAIT);
    info!(
        "vmux: embedded status/history UI base URLs drained in {:?}",
        t0.elapsed()
    );
}

fn drain_channel(
    ready: &mut Option<String>,
    rx: Option<&crossbeam_channel::Receiver<String>>,
    wait: Duration,
) {
    if ready.is_some() {
        return;
    }
    let Some(rx) = rx else {
        return;
    };
    if let Ok(u) = rx.recv_timeout(wait) {
        *ready = Some(u);
    }
}

pub fn setup_vmux_panes_startup(
    commands: Commands,
    snapshot: ResMut<SessionLayoutSnapshot>,
    last: Option<Res<LastVisitedUrl>>,
    meshes: ResMut<Assets<Mesh>>,
    materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    loading_bar_materials: ResMut<Assets<LoadingBarMaterial>>,
    path: Option<Res<SessionSavePath>>,
    session_queue: ResMut<SessionSaveQueue>,
    settings: Res<VmuxAppSettings>,
    hist_url: Res<HistoryUiBaseUrl>,
) {
    let h = hist_url.0.as_deref();
    setup_vmux_panes(
        commands,
        snapshot,
        last,
        meshes,
        materials,
        loading_bar_materials,
        path,
        session_queue,
        settings,
        h,
    );
}

pub(super) fn register(app: &mut App) {
    app.add_systems(
        Startup,
        (
            startup_drain_embedded_ui_urls.in_set(EmbeddedServeDirStartup::DrainChannels),
            setup_vmux_panes_startup.after(startup_drain_embedded_ui_urls),
        ),
    );
}
