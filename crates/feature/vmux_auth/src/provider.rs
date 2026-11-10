use bevy::prelude::*;
use std::collections::BTreeSet;
use vmux_connector::ProviderKey;
use vmux_ecs::manifest::{AuthProvider, FeatureManifest};

pub struct ProviderPlugin;

impl Plugin for ProviderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register);
    }
}

#[derive(Component, Clone, Debug)]
pub struct AuthProviderDefinition(pub AuthProvider);

fn register(manifests: Query<&FeatureManifest>, mut commands: Commands) {
    let mut ids = BTreeSet::new();
    for provider in manifests
        .iter()
        .flat_map(|manifest| &manifest.auth_providers)
    {
        let mut provider = provider.clone();
        provider.configure_client_id(|name| std::env::var(name).ok());
        assert!(
            !provider.id.trim().is_empty(),
            "auth provider ID must not be empty"
        );
        assert!(
            ids.insert(provider.id.clone()),
            "duplicate auth provider ID"
        );
        if !provider.configured() {
            bevy::log::warn!("auth provider {} is not configured", provider.id);
            continue;
        }
        commands.spawn((
            Name::new(provider.name.clone()),
            ProviderKey(provider.id.clone()),
            AuthProviderDefinition(provider),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_ecs::manifest::{AuthFlow, DeviceCodeFlow, FeatureManifestSource, FeaturePlugin};

    struct ProviderFeature;

    impl FeatureManifestSource for ProviderFeature {
        const SOURCE: &'static str = r#"(
            auth_providers: [(
                id: "example",
                name: "Example",
                flow: DeviceCode((
                    client_id: "client",
                    device_authorization_endpoint: "https://example.com/device",
                    token_endpoint: "https://example.com/token",
                )),
            )],
        )"#;
    }

    #[test]
    fn providers_are_discovered_from_feature_manifests() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            FeaturePlugin::<ProviderFeature>::default(),
            ProviderPlugin,
        ));
        app.update();

        let mut providers = app
            .world_mut()
            .query::<(&ProviderKey, &AuthProviderDefinition)>();
        let (key, provider) = providers.single(app.world()).unwrap();
        assert_eq!(key.0, "example");
        assert!(matches!(
            &provider.0.flow,
            AuthFlow::DeviceCode(DeviceCodeFlow { client_id, .. }) if client_id == "client"
        ));
    }
}
