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
    let mut environment = "production".to_string();
    let mut endpoint = Some(
        Endpoint::new("https://api.vmux.ai/v1/flags/snapshot").expect("compiled release endpoint"),
    );
    if matches!(vmux_profile::build_profile(), "dev" | "local")
        && let Ok(url) = std::env::var("VMUX_RELEASE_FLAGS_ENDPOINT")
    {
        environment = std::env::var("VMUX_RELEASE_FLAGS_ENVIRONMENT")
            .unwrap_or_else(|_| "development".into());
        endpoint = None;
        if url.starts_with("http://") {
            endpoint = Endpoint::new(&url).ok();
        }
    }
    let provider = commands.spawn(Provider::bundle(&environment)).id();
    if let Some(endpoint) = endpoint {
        commands.entity(provider).insert(endpoint);
    }
    #[cfg(feature = "account-cloud")]
    commands.spawn((
        vmux_account::AccountCloudFlag,
        vmux_feature::Definition::bundle(provider, "account.cloud", false),
    ));
}

#[cfg(feature = "feature-flags")]
pub(crate) struct FeatureFlagsPlugin;

#[cfg(feature = "feature-flags")]
impl bevy_app::Plugin for FeatureFlagsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_feature::registration::FeaturePagePlugin);
        #[cfg(target_os = "macos")]
        app.add_systems(Startup, configure_administration.after(configure_release));
    }
}

#[cfg(all(feature = "feature-flags", target_os = "macos"))]
fn configure_administration(
    mut commands: Commands,
    providers: Query<(Entity, &Name, Option<&Endpoint>), With<Provider>>,
) {
    let public =
        vmux_feature::registration::Endpoint::new("https://api.vmux.ai/", "production", None)
            .expect("static public endpoint");
    let snapshot =
        Endpoint::new("https://api.vmux.ai/v1/flags/snapshot").expect("static snapshot endpoint");
    let mut production = None;
    for (entity, name, endpoint) in &providers {
        if name.as_str() == "production" && endpoint == Some(&snapshot) {
            production = Some(entity);
            break;
        }
    }
    match production {
        Some(entity) => {
            commands.entity(entity).insert(public);
        }
        None => {
            commands.spawn((Provider::bundle("production"), public, snapshot));
        }
    }
    if !matches!(vmux_profile::build_profile(), "dev" | "local") {
        return;
    }
    let Ok(url) = std::env::var("VMUX_FEATURE_ADMIN_ENDPOINT") else {
        return;
    };
    let credential = std::env::var("VMUX_FEATURE_ADMIN_TOKEN").ok();
    let environment =
        std::env::var("VMUX_FEATURE_ADMIN_ENVIRONMENT").unwrap_or_else(|_| "development".into());
    let endpoint = match vmux_feature::registration::Endpoint::new(&url, &environment, credential) {
        Ok(endpoint) if endpoint.is_loopback() && environment != "production" => endpoint,
        _ => {
            tracing::warn!("local flag administration endpoint rejected");
            return;
        }
    };
    let snapshot_url = format!("{}/v1/flags/snapshot", url.trim_end_matches('/'));
    let snapshot = match Endpoint::new(&snapshot_url) {
        Ok(endpoint) => endpoint,
        Err(_) => {
            tracing::warn!("local flag snapshot endpoint rejected");
            return;
        }
    };
    for (entity, name, existing) in &providers {
        if name.as_str() != environment {
            continue;
        }
        if existing != Some(&snapshot) {
            tracing::warn!("local flag administration differs from the release snapshot endpoint");
            return;
        }
        commands.entity(entity).insert(endpoint);
        return;
    }
    commands.spawn((Provider::bundle(&environment), endpoint, snapshot));
}
