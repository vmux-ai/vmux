use bevy_app::{App, Startup};
use bevy_ecs::prelude::*;
use vmux_feature::http::{Endpoint, HttpPlugin};
use vmux_feature::{FeaturePlugin, Provider};

pub(crate) struct ReleaseFlagsPlugin;

impl bevy_app::Plugin for ReleaseFlagsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FeaturePlugin, HttpPlugin))
            .add_systems(Startup, configure_release);
    }
}

fn configure_release(mut commands: Commands) {
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

#[cfg(feature = "feature-flags")]
pub(crate) struct FeatureFlagsPlugin;

#[cfg(feature = "feature-flags")]
impl bevy_app::Plugin for FeatureFlagsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_feature::page::FeaturePagePlugin);
        #[cfg(target_os = "macos")]
        app.add_plugins(vmux_feature::http::HttpPlugin)
            .add_systems(bevy_app::Startup, configure_administration);
    }
}

#[cfg(all(feature = "feature-flags", target_os = "macos"))]
fn configure_administration(mut commands: bevy_ecs::prelude::Commands) {
    let public = vmux_feature::page::Endpoint::new("https://api.vmux.ai/", "production", None)
        .expect("static public endpoint");
    let snapshot = vmux_feature::http::Endpoint::new("https://api.vmux.ai/v1/flags/snapshot")
        .expect("static snapshot endpoint");
    commands.spawn((
        vmux_feature::Provider::bundle("production"),
        public,
        snapshot,
    ));
    if !matches!(vmux_profile::build_profile(), "dev" | "local") {
        return;
    }
    let Ok(url) = std::env::var("VMUX_FEATURE_ADMIN_ENDPOINT") else {
        return;
    };
    let credential = std::env::var("VMUX_FEATURE_ADMIN_TOKEN").ok();
    let environment =
        std::env::var("VMUX_FEATURE_ADMIN_ENVIRONMENT").unwrap_or_else(|_| "development".into());
    let endpoint = match vmux_feature::page::Endpoint::new(&url, &environment, credential) {
        Ok(endpoint) if endpoint.is_loopback() && environment != "production" => endpoint,
        _ => {
            tracing::warn!("local flag administration endpoint rejected");
            return;
        }
    };
    let snapshot_url = format!("{}/v1/flags/snapshot", url.trim_end_matches('/'));
    let snapshot = match vmux_feature::http::Endpoint::new(&snapshot_url) {
        Ok(endpoint) => endpoint,
        Err(_) => {
            tracing::warn!("local flag snapshot endpoint rejected");
            return;
        }
    };
    commands.spawn((
        vmux_feature::Provider::bundle(&environment),
        endpoint,
        snapshot,
    ));
}
