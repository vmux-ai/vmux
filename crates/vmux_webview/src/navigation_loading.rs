//! Pane loading bar driven by CEF `CefLoadHandler::on_loading_state_change` (wired in `bevy_cef_core`).

use bevy::prelude::*;
use bevy_cef::prelude::WebviewLoadingStateReceiver;
use vmux_layout::PendingNavigationLoads;

/// If CEF never reports `is_loading == false`, drop the entry so the bar cannot stick forever.
const PENDING_LOAD_TIMEOUT_SECS: f32 = 8.0;

pub(crate) fn apply_cef_webview_loading_state(
    mut pending: ResMut<PendingNavigationLoads>,
    receiver: Res<WebviewLoadingStateReceiver>,
    time: Res<Time>,
) {
    while let Ok(ev) = receiver.0.try_recv() {
        if ev.is_loading {
            pending.0.insert(ev.webview, time.elapsed_secs());
        } else {
            pending.0.remove(&ev.webview);
        }
    }
    let now = time.elapsed_secs();
    pending
        .0
        .retain(|_, started| now <= *started + PENDING_LOAD_TIMEOUT_SECS);
}

pub(crate) fn register(app: &mut App) {
    app.add_systems(Update, apply_cef_webview_loading_state);
}
