use bevy::prelude::*;

pub use connection::{
    ConnectRequest, Connected, Connection, ConnectionName, ConnectionState, DisconnectRequest,
    Disconnected,
};
pub use credential::{CredentialBroker, CredentialLease, CredentialSource};
pub use provider::AuthProviderDefinition;
pub use vmux_api::Id;
pub use vmux_connector::{AccountExternalId, CanAccess, ProviderKey};

mod connection;
mod credential;
mod flow;
mod provider;

pub struct AuthPlugin;

impl Plugin for AuthPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_ecs::manifest::FeaturePlugin::<Feature>::default())
            .add_plugins(vmux_connector::ConnectorPlugin)
            .add_plugins(connection::ConnectionPlugin)
            .add_plugins(provider::ProviderPlugin);
    }
}

struct Feature;

impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = "()";
}
