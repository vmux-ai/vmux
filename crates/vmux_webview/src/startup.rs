//! Startup: drain embedded HTTP base URLs before pane spawn so history/status UIs load like normal pages.

use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_cef::prelude::WebviewExtendStandardMaterial;
use vmux_core::{SessionSavePath, SessionSaveQueue};
use vmux_ui_native::hosted::history::{HistoryUiBaseUrl, HistoryUiUrlReceiver};
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
    const WAIT_TOTAL: Duration = Duration::from_millis(350);
    let t0 = Instant::now();
    let deadline = t0 + WAIT_TOTAL;
    drain_channels_until_deadline(
        &mut status_ready.0,
        status_rx.0.as_ref(),
        &mut hist_ready.0,
        hist_rx.0.as_ref(),
        deadline,
    );
    info!(
        "vmux: embedded status/history UI base URLs drained in {:?}",
        t0.elapsed()
    );
}

fn drain_channels_until_deadline(
    status_ready: &mut Option<String>,
    status_rx: Option<&crossbeam_channel::Receiver<String>>,
    hist_ready: &mut Option<String>,
    hist_rx: Option<&crossbeam_channel::Receiver<String>>,
    deadline: Instant,
) {
    loop {
        let mut progressed = false;
        if status_ready.is_none()
            && let Some(rx) = status_rx
            && let Ok(u) = rx.try_recv()
        {
            *status_ready = Some(u);
            progressed = true;
        }
        if hist_ready.is_none()
            && let Some(rx) = hist_rx
            && let Ok(u) = rx.try_recv()
        {
            *hist_ready = Some(u);
            progressed = true;
        }
        if (status_ready.is_some() || status_rx.is_none())
            && (hist_ready.is_some() || hist_rx.is_none())
        {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        if !progressed {
            std::thread::sleep((deadline - now).min(Duration::from_millis(10)));
        }
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
