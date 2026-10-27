use bevy::prelude::*;
use vmux_feature::AppFeatureExt;

pub use connection::{
    AuthSet, ConnectRequest, Connected, Connection, ConnectionFailure, ConnectionState,
    DisconnectRequest, Disconnected, ProfilePersistence,
};
pub use credential::{CredentialBroker, CredentialIssuer, CredentialLease};
pub use provider::AuthProviderDefinition;
pub use vmux_api::Id;
pub use vmux_connector::{CanAccess, ProviderKey};

mod connection;
mod credential;
mod provider;

pub struct AuthPlugin;

impl Plugin for AuthPlugin {
    fn build(&self, app: &mut App) {
        app.register_feature::<Feature>()
            .add_plugins((provider::ProviderPlugin, connection::ConnectionPlugin));
    }
}

struct Feature;

impl vmux_feature::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}
