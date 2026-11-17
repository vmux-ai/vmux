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
    commands.spawn((
        Provider::bundle("production"),
        Endpoint::new("https://api.vmux.ai/v1/flags/snapshot").expect("compiled release endpoint"),
    ));
}
