//! Embedded HTTP for the UI library Dioxus bundle (`dist/` / `VMUX_UI_LIBRARY_URL`, legacy `VMUX_UI_SHOWCASE_URL`).

use std::path::PathBuf;

use bevy::prelude::*;
use vmux_core::VmuxUiLibraryBaseUrl;
use vmux_server::{
    EmbeddedServeDirStartup, PendingEmbeddedServeDir, ServePlugin, push_pending_embedded_serve_dir,
    register_serve_plugin_dioxus_warmup,
};

/// Channel from embedded `ServeDir` until [`UiLibraryUiPlugin`](crate::hosted::ui_library_ui::UiLibraryUiPlugin) drains the loopback base URL.
#[derive(Resource, Default)]
pub struct UiLibraryUrlReceiver(pub Option<crossbeam_channel::Receiver<String>>);

fn ui_library_dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(crate::UI_LIBRARY_DIST_DIR_NAME)
}

fn startup_ui_library_server(mut commands: Commands, mut pending: ResMut<PendingEmbeddedServeDir>) {
    let override_url = std::env::var("VMUX_UI_LIBRARY_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("VMUX_UI_SHOWCASE_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
        });
    if let Some(u) = override_url {
        commands.insert_resource(VmuxUiLibraryBaseUrl(Some(u.trim().to_string())));
        return;
    }

    let dist = ui_library_dist_dir();
    if !dist.join("index.html").is_file() {
        bevy::log::info!(
            "vmux_ui_native: no {}; run `cargo build -p vmux_ui` (debug) or set VMUX_UI_LIBRARY_URL (or legacy VMUX_UI_SHOWCASE_URL)",
            dist.display()
        );
        return;
    }

    let rx = push_pending_embedded_serve_dir(&mut pending, dist);
    commands.insert_resource(UiLibraryUrlReceiver(Some(rx)));
}

/// Serves the UI library bundle from `dist/` (debug native builds; see [`crate::UiLibraryPlugin`]).
#[derive(Default)]
pub struct UiLibraryServerPlugin;

impl Plugin for UiLibraryServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VmuxUiLibraryBaseUrl>()
            .init_resource::<UiLibraryUrlReceiver>()
            .add_systems(
                Startup,
                startup_ui_library_server.in_set(EmbeddedServeDirStartup::FillPending),
            );
        register_serve_plugin_dioxus_warmup::<Self>(app);
    }
}

impl ServePlugin for UiLibraryServerPlugin {}
