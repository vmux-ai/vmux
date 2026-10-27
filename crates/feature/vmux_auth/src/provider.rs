use bevy::prelude::*;
use vmux_connector::ProviderKey;
use vmux_feature::{AuthProvider, FeatureRegistry};

pub(crate) struct ProviderPlugin;

impl Plugin for ProviderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register);
    }
}

#[derive(Component, Clone, Debug)]
pub struct AuthProviderDefinition(AuthProvider);

impl AuthProviderDefinition {
    pub fn provider(&self) -> &AuthProvider {
        &self.0
    }
}

fn register(registry: Res<FeatureRegistry>, mut commands: Commands) {
    for manifest in registry.manifests() {
        for provider in &manifest.auth_providers {
            let mut provider = provider.clone();
            provider.configure_client_id(|name| std::env::var(name).ok());
            if !provider.configured() {
                bevy::log::warn!(provider = %provider.id, "auth provider is not configured");
                continue;
            }
            let key = ProviderKey::new(provider.id.clone())
                .expect("validated auth provider IDs are non-empty");
            commands.spawn((
                Name::new(provider.name.clone()),
                key,
                AuthProviderDefinition(provider),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use vmux_feature::{AppFeatureExt, AuthFlow, DeviceCodeFlow, FeatureManifestSource};

    use super::*;

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
    fn providers_are_projected_from_the_frozen_feature_registry() {
        let mut app = App::new();
        app.register_feature::<ProviderFeature>()
            .add_plugins(ProviderPlugin);
        app.finish();

        app.update();

        let mut providers = app
            .world_mut()
            .query::<(&ProviderKey, &AuthProviderDefinition)>();
        let (key, provider) = providers.single(app.world()).unwrap();
        assert_eq!(key.as_str(), "example");
        assert!(matches!(
            &provider.provider().flow,
            AuthFlow::DeviceCode(DeviceCodeFlow { client_id, .. }) if client_id == "client"
        ));
    }
}
