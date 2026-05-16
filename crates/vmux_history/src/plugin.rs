use std::path::PathBuf;
use std::time::Duration;

use bevy::time::common_conditions::on_timer;
use vmux_webview_app::{WebviewAppConfig, WebviewAppRegistry};

use crate::prune::prune_history;
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
        app.add_systems(
            Update,
            prune_history.run_if(on_timer(Duration::from_secs(3600))),
        );
        app.add_systems(Startup, prune_history);
    }
}
