use std::path::PathBuf;

use vmux_webview_app::{WebviewAppConfig, WebviewAppRegistry};

use crate::spawn::spawn_visits;

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<WebviewAppRegistry>()
            .register(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                &WebviewAppConfig::with_custom_host("history"),
            );
        app.add_systems(Update, spawn_visits);
    }
}
