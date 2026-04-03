//! Bevy wiring for the UI library: drain embedded HTTP URL into [`VmuxUiLibraryBaseUrl`].

use std::time::{Duration, Instant};

use bevy::prelude::*;
use vmux_core::VmuxUiLibraryBaseUrl;
use vmux_server::EmbeddedServeDirStartup;

use super::ui_library_server::UiLibraryUrlReceiver;
use super::{UiPlugin, register_ui_plugin_dioxus_warmup};

/// Same idea as status/history drains in `vmux_webview`: wait briefly for the tokio `ServeDir` task
/// to send the loopback base so [`VmuxUiLibraryBaseUrl`] is ready before the first [`Update`] frame.
fn startup_drain_ui_library_url(
    mut ready: ResMut<VmuxUiLibraryBaseUrl>,
    rx: ResMut<UiLibraryUrlReceiver>,
) {
    if ready.0.is_some() {
        return;
    }
    let Some(ref r) = rx.0 else {
        return;
    };
    const WAIT_TOTAL: Duration = Duration::from_millis(350);
    let deadline = Instant::now() + WAIT_TOTAL;
    loop {
        if let Ok(u) = r.try_recv() {
            bevy::log::info!("vmux_ui_native: UI library base URL ready ({u})");
            ready.0 = Some(u);
            return;
        }
        if Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn poll_ui_library_url(mut ready: ResMut<VmuxUiLibraryBaseUrl>, rx: ResMut<UiLibraryUrlReceiver>) {
    if ready.0.is_some() {
        return;
    }
    let Some(ref r) = rx.0 else {
        return;
    };
    if let Ok(u) = r.try_recv() {
        bevy::log::info!("vmux_ui_native: UI library base URL ready ({u})");
        ready.0 = Some(u);
    }
}

#[derive(Default)]
pub struct UiLibraryUiPlugin;

impl UiPlugin for UiLibraryUiPlugin {}

impl Plugin for UiLibraryUiPlugin {
    fn build(&self, app: &mut App) {
        register_ui_plugin_dioxus_warmup::<Self>(app);
        app.add_systems(
            Startup,
            startup_drain_ui_library_url.in_set(EmbeddedServeDirStartup::DrainChannels),
        );
        app.add_systems(Update, poll_ui_library_url);
    }
}
