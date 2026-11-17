use bevy_app::{App, Startup};
use bevy_ecs::prelude::*;
use vmux_feature::http::{Endpoint, HttpPlugin};
use vmux_feature::{FeaturePlugin, Provider};

pub(crate) struct ReleaseFlagsPlugin;

impl bevy_app::Plugin for ReleaseFlagsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FeaturePlugin, HttpPlugin))
            .add_systems(Startup, configure);
    }
}

fn configure(mut commands: Commands) {
    if matches!(vmux_profile::build_profile(), "dev" | "local") {
        if let Ok(url) = std::env::var("VMUX_RELEASE_FLAGS_ENDPOINT") {
            let environment = std::env::var("VMUX_RELEASE_FLAGS_ENVIRONMENT")
                .unwrap_or_else(|_| "development".into());
            let provider = commands.spawn(Provider::bundle(&environment)).id();
            if url.starts_with("http://") {
                if let Ok(endpoint) = Endpoint::new(&url) {
                    commands.entity(provider).insert(endpoint);
                }
            }
            return;
        }
    }
    commands.spawn((
        Provider::bundle("production"),
        Endpoint::new("https://api.vmux.ai/v1/flags/snapshot").expect("compiled release endpoint"),
    ));
}
