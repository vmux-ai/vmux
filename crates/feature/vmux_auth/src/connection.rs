use std::collections::HashSet;

use bevy::prelude::*;
use moonshine_save::prelude::Save;
use vmux_api::Id;
use vmux_connector::{CanAccess, ProviderKey};
use vmux_ecs::persistence::PersistenceAppExt;

use crate::credential::CredentialVault;
use crate::provider::AuthProviderDefinition;

pub(crate) struct ConnectionPlugin;

impl Plugin for ConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.register_persistence_type::<ProfilePersistence, Connection>()
            .register_persistence_type::<ProfilePersistence, Id<Connection>>()
            .register_persistence_type::<ProfilePersistence, Name>()
            .register_persistence_type::<ProfilePersistence, ProviderKey>()
            .register_persistence_type::<ProfilePersistence, CanAccess>()
            .register_type::<ConnectionState>()
            .init_resource::<CredentialVault>()
            .add_plugins(vmux_ecs::IdentityIndexPlugin::<Connection>::default())
            .add_message::<ConnectRequest>()
            .add_message::<DisconnectRequest>()
            .configure_sets(
                Update,
                (
                    AuthSet::Receive,
                    AuthSet::Apply,
                    AuthSet::Resolve,
                    AuthSet::Publish,
                    AuthSet::Cleanup,
                )
                    .chain(),
            )
            .add_systems(Update, connect.in_set(AuthSet::Receive))
            .add_systems(Update, ApplyDeferred.in_set(AuthSet::Apply))
            .add_systems(Update, rebuild.in_set(AuthSet::Resolve))
            .add_systems(Update, disconnect.in_set(AuthSet::Publish));
    }
}

pub enum ProfilePersistence {}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuthSet {
    Receive,
    Apply,
    Resolve,
    Publish,
    Cleanup,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[require(Save, ConnectionState)]
#[type_path = "vmux_auth"]
pub struct Connection;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_auth"]
pub enum ConnectionState {
    Connecting,
    AwaitingUser {
        verification_uri: String,
        user_code: String,
        expires_at: vmux_ecs::UnixMillis,
    },
    Connected,
    Failed(ConnectionFailure),
    #[default]
    Disconnected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[type_path = "vmux_auth"]
pub enum ConnectionFailure {
    AuthorizationDenied,
    Expired,
    Network,
    ProviderUnavailable,
    StorageUnavailable,
}

#[derive(Message, Clone, Debug)]
pub struct ConnectRequest {
    id: Id<Connection>,
    provider: ProviderKey,
    name: String,
}

impl ConnectRequest {
    pub fn new(
        id: Id<Connection>,
        provider: ProviderKey,
        name: impl Into<String>,
    ) -> Result<Self, String> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("connection name must not be empty".to_string());
        }
        Ok(Self { id, provider, name })
    }
}

#[derive(Message, Clone, Debug)]
pub struct DisconnectRequest {
    id: Id<Connection>,
}

impl DisconnectRequest {
    pub fn new(id: Id<Connection>) -> Self {
        Self { id }
    }
}

#[derive(EntityEvent, Clone, Debug)]
pub struct Connected {
    #[event_target]
    pub entity: Entity,
    pub id: Id<Connection>,
    pub provider: ProviderKey,
}

#[derive(EntityEvent, Clone, Debug)]
pub struct Disconnected {
    #[event_target]
    pub entity: Entity,
    pub id: Id<Connection>,
    pub provider: ProviderKey,
}

fn connect(
    mut requests: MessageReader<ConnectRequest>,
    index: Res<vmux_ecs::IdentityIndex<Connection>>,
    providers: Query<&ProviderKey, With<AuthProviderDefinition>>,
    mut commands: Commands,
) {
    let mut pending = HashSet::new();
    for request in requests.read() {
        if index.entity(&request.id).ok().flatten().is_some() || !pending.insert(request.id.clone())
        {
            continue;
        }
        if !providers
            .iter()
            .any(|provider| provider == &request.provider)
        {
            continue;
        }
        commands.spawn((
            Connection,
            request.id.clone(),
            Name::new(request.name.clone()),
            request.provider.clone(),
            CanAccess::default(),
            ConnectionState::Connecting,
        ));
    }
}

fn rebuild(mut index: vmux_ecs::IdentityIndexWriter<Connection>) {
    index.rebuild();
}

fn disconnect(
    mut requests: MessageReader<DisconnectRequest>,
    index: Res<vmux_ecs::IdentityIndex<Connection>>,
    mut connections: Query<(&Id<Connection>, &ProviderKey, &mut ConnectionState), With<Connection>>,
    mut vault: ResMut<CredentialVault>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(Some(entity)) = index.entity(&request.id) else {
            continue;
        };
        let Ok((id, provider, mut state)) = connections.get_mut(entity) else {
            continue;
        };
        vault.invalidate(provider, id);
        *state = ConnectionState::Disconnected;
        commands.trigger(Disconnected {
            entity,
            id: id.clone(),
            provider: provider.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use vmux_feature::{AppFeatureExt, FeatureManifestSource};

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
    fn connection_identity_is_indexed_and_disconnect_revokes_its_lease() {
        let mut app = App::new();
        app.register_feature::<ProviderFeature>()
            .add_plugins((crate::provider::ProviderPlugin, ConnectionPlugin));
        app.finish();
        app.update();
        let id = Id::<Connection>::new("connection-1");
        let provider = ProviderKey::new("example").unwrap();
        app.world_mut()
            .resource_mut::<Messages<ConnectRequest>>()
            .write(ConnectRequest::new(id.clone(), provider.clone(), "Example").unwrap());

        app.update();

        let entity = app
            .world()
            .resource::<vmux_ecs::IdentityIndex<Connection>>()
            .entity(&id)
            .unwrap()
            .unwrap();
        app.world_mut()
            .entity_mut(entity)
            .insert(ConnectionState::Connected);
        app.world_mut()
            .resource_mut::<CredentialVault>()
            .install(provider.clone(), id.clone(), "secret")
            .unwrap();
        let lease = app
            .world()
            .resource::<CredentialVault>()
            .lease(&provider, &id)
            .unwrap();
        app.world_mut()
            .resource_mut::<Messages<DisconnectRequest>>()
            .write(DisconnectRequest::new(id));

        app.update();

        assert_eq!(
            app.world().get::<ConnectionState>(entity),
            Some(&ConnectionState::Disconnected)
        );
        assert!(!lease.is_active());
    }

    #[test]
    fn restored_connection_defaults_to_disconnected() {
        let mut app = App::new();
        app.add_plugins(ConnectionPlugin);
        let connection = app.world_mut().spawn(Connection).id();

        assert_eq!(
            app.world().get::<ConnectionState>(connection),
            Some(&ConnectionState::Disconnected)
        );
    }
}
