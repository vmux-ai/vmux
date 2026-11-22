use std::collections::HashMap;
use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_transport::DeviceId;

pub(crate) use super::authorization_driver::RemoteAuthorizations;
pub use super::authorization_driver::{
    AccountRelayCredential, AuthorizationOutcome, AuthorizedDevice, RemoteAuthorizationStore,
};
use super::authorization_driver::{
    AuthenticateDevice, AuthorizationFile, AuthorizationInbox, PairDevice, ResetAuthorization,
    RevalidateAuthorization, RevokeAuthorization, StoredAuthorizations, StoredDevice, Token,
    TokenHash,
};

pub struct RemoteAuthorizationPlugin;

impl Plugin for RemoteAuthorizationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                receive_requests,
                ApplyDeferred,
                sync_authority,
                ApplyDeferred,
                process_authentications,
                ApplyDeferred,
                process_revalidations,
                process_mutations,
            )
                .chain(),
        );
    }
}

#[derive(Component, Debug)]
pub(super) struct AuthorizationAuthority {
    path: PathBuf,
    generation: u64,
    disk_fingerprint: Option<u64>,
    initialized: bool,
    last_sync_error: Option<String>,
}

impl AuthorizationAuthority {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path,
            generation: 0,
            disk_fingerprint: None,
            initialized: false,
            last_sync_error: None,
        }
    }

    #[cfg(test)]
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }
}

#[derive(Component, Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct PairingCredential(String);

impl PairingCredential {
    fn matches(&self, candidate: &str) -> bool {
        !self.0.is_empty() && Token::secure_eq(candidate, &self.0)
    }
}

#[derive(Component)]
struct AuthorizedDeviceEntity;

#[derive(Component, Clone, Debug, Eq, PartialEq)]
struct AuthorizedDeviceId(DeviceId);

#[derive(Component, Clone, Debug, Eq, PartialEq)]
struct DeviceTokenHash(TokenHash);

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
struct AuthorizedAt(i64);

#[derive(SystemParam)]
struct AuthorizedDevices<'w, 's> {
    devices: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static AuthorizedDeviceId,
            &'static mut DeviceTokenHash,
            &'static mut AuthorizedAt,
        ),
        With<AuthorizedDeviceEntity>,
    >,
    commands: Commands<'w, 's>,
}

impl AuthorizedDevices<'_, '_> {
    fn matches(&self, authority: Entity, client_id: &DeviceId, token: &str) -> bool {
        self.devices.iter().any(|(_, parent, id, hash, _)| {
            parent.parent() == authority && id.0 == *client_id && hash.0.matches(token)
        })
    }

    fn reconcile(&mut self, authority: Entity, stored_devices: Vec<StoredDevice>) {
        let mut incoming: HashMap<DeviceId, StoredDevice> =
            HashMap::with_capacity(stored_devices.len());
        for device in stored_devices {
            incoming.insert(device.id.clone(), device);
        }
        for (entity, parent, id, mut hash, mut authorized_at) in &mut self.devices {
            if parent.parent() != authority {
                continue;
            }
            if let Some(updated) = incoming.remove(&id.0) {
                if hash.0 != updated.token_hash {
                    hash.0 = updated.token_hash;
                }
                if authorized_at.0 != updated.authorized_at_unix {
                    authorized_at.0 = updated.authorized_at_unix;
                }
            } else {
                self.commands.entity(entity).despawn();
            }
        }
        for device in incoming.into_values() {
            self.commands.spawn((
                AuthorizedDeviceEntity,
                AuthorizedDeviceId(device.id),
                DeviceTokenHash(device.token_hash),
                AuthorizedAt(device.authorized_at_unix),
                ChildOf(authority),
            ));
        }
    }

    fn pair(
        &mut self,
        authority_entity: Entity,
        authority: &mut AuthorizationAuthority,
        pairing: &mut PairingCredential,
        client_id: &DeviceId,
    ) -> std::io::Result<Option<AuthorizationOutcome>> {
        let device_token = Token::generate();
        let token_hash = TokenHash::of(&device_token);
        let authorized_at_unix = chrono::Utc::now().timestamp();
        let next_pairing_token = Token::generate();
        let mut snapshot_devices = Vec::new();
        let mut existing_entity = None;
        for (entity, parent, id, hash, authorized_at) in &self.devices {
            if parent.parent() != authority_entity {
                continue;
            }
            if id.0 == *client_id {
                existing_entity = Some(entity);
                continue;
            }
            snapshot_devices.push(StoredDevice {
                id: id.0.clone(),
                token_hash: hash.0.clone(),
                authorized_at_unix: authorized_at.0,
            });
        }
        snapshot_devices.push(StoredDevice {
            id: client_id.clone(),
            token_hash: token_hash.clone(),
            authorized_at_unix,
        });
        snapshot_devices.sort_by(|left, right| left.id.cmp(&right.id));
        let stored = StoredAuthorizations {
            version: 1,
            pairing_token: next_pairing_token.clone(),
            devices: snapshot_devices,
        };
        let fingerprint = AuthorizationFile::write(&authority.path, &stored)?;
        if let Some(entity) = existing_entity
            && let Ok((_, _, _, mut hash, mut authorized_at)) = self.devices.get_mut(entity)
        {
            hash.0 = token_hash;
            authorized_at.0 = authorized_at_unix;
        } else {
            self.commands.spawn((
                AuthorizedDeviceEntity,
                AuthorizedDeviceId(client_id.clone()),
                DeviceTokenHash(token_hash),
                AuthorizedAt(authorized_at_unix),
                ChildOf(authority_entity),
            ));
        }
        pairing.0 = next_pairing_token;
        authority.disk_fingerprint = Some(fingerprint);
        authority.generation = authority.generation.saturating_add(1);
        Ok(Some(AuthorizationOutcome::Paired { device_token }))
    }

    fn revoke(
        &mut self,
        authority_entity: Entity,
        authority: &mut AuthorizationAuthority,
        pairing: &PairingCredential,
        client_id: &DeviceId,
    ) -> std::io::Result<bool> {
        let mut target_entity = None;
        let mut remaining = Vec::new();
        for (entity, parent, id, hash, authorized_at) in &self.devices {
            if parent.parent() != authority_entity {
                continue;
            }
            if id.0 == *client_id {
                target_entity = Some(entity);
                continue;
            }
            remaining.push(StoredDevice {
                id: id.0.clone(),
                token_hash: hash.0.clone(),
                authorized_at_unix: authorized_at.0,
            });
        }
        let Some(target) = target_entity else {
            return Ok(false);
        };
        remaining.sort_by(|left, right| left.id.cmp(&right.id));
        let stored = StoredAuthorizations {
            version: 1,
            pairing_token: pairing.0.clone(),
            devices: remaining,
        };
        let fingerprint = AuthorizationFile::write(&authority.path, &stored)?;
        self.commands.entity(target).despawn();
        authority.disk_fingerprint = Some(fingerprint);
        authority.generation = authority.generation.saturating_add(1);
        Ok(true)
    }

    fn reset(
        &mut self,
        authority_entity: Entity,
        authority: &mut AuthorizationAuthority,
        pairing: &mut PairingCredential,
    ) -> std::io::Result<()> {
        AuthorizationFile::remove(&authority.path)?;
        for (entity, parent, _, _, _) in &self.devices {
            if parent.parent() == authority_entity {
                self.commands.entity(entity).despawn();
            }
        }
        pairing.0.clear();
        authority.disk_fingerprint = None;
        authority.initialized = false;
        authority.generation = authority.generation.saturating_add(1);
        Ok(())
    }
}

fn receive_requests(mut inbox: Single<&mut AuthorizationInbox>, mut commands: Commands) {
    while let Ok(request) = inbox.0.authenticate_device.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.pair_device.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.revalidate.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.revoke.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.reset.try_recv() {
        commands.spawn(request);
    }
}

fn sync_authority(
    mut authorities: Query<(Entity, &mut AuthorizationAuthority, &mut PairingCredential)>,
    mut devices: AuthorizedDevices,
    pending_auth: Query<(), With<AuthenticateDevice>>,
    pending_pair: Query<(), With<PairDevice>>,
    pending_reval: Query<(), With<RevalidateAuthorization>>,
    pending_revoke: Query<(), With<RevokeAuthorization>>,
    pending_reset: Query<(), With<ResetAuthorization>>,
) {
    let has_pending = !pending_auth.is_empty()
        || !pending_pair.is_empty()
        || !pending_reval.is_empty()
        || !pending_revoke.is_empty()
        || !pending_reset.is_empty();
    for (authority_entity, mut authority, mut pairing) in &mut authorities {
        if authority.initialized && !has_pending {
            continue;
        }
        authority.last_sync_error = None;
        let loaded = match AuthorizationFile::read(&authority.path) {
            Ok(Some((fingerprint, stored))) => {
                if authority.initialized && authority.disk_fingerprint == Some(fingerprint) {
                    continue;
                }
                (fingerprint, stored)
            }
            Ok(None) => {
                let stored = StoredAuthorizations::new();
                match AuthorizationFile::write(&authority.path, &stored) {
                    Ok(fingerprint) => (fingerprint, stored),
                    Err(error) => {
                        authority.last_sync_error = Some(error.to_string());
                        continue;
                    }
                }
            }
            Err(error) => {
                authority.last_sync_error = Some(error.to_string());
                continue;
            }
        };
        let (fingerprint, stored) = loaded;
        pairing.0 = stored.pairing_token;
        devices.reconcile(authority_entity, stored.devices);
        authority.disk_fingerprint = Some(fingerprint);
        authority.initialized = true;
        authority.generation = authority.generation.saturating_add(1);
    }
}

fn process_authentications(
    mut authorities: Single<(Entity, &mut AuthorizationAuthority, &mut PairingCredential)>,
    mut devices: AuthorizedDevices,
    mut device_auths: Query<(Entity, &mut AuthenticateDevice)>,
    mut pairings: Query<(Entity, &mut PairDevice)>,
    mut commands: Commands,
) {
    let (authority_entity, authority, pairing) = &mut *authorities;
    for (request_entity, mut request) in &mut device_auths {
        let outcome = if let Some(error) = &authority.last_sync_error {
            Err(std::io::Error::other(error.clone()))
        } else {
            Ok(devices
                .matches(*authority_entity, &request.client_id, &request.device_token)
                .then_some(AuthorizationOutcome::Accepted))
        };
        if let Some(response) = request.response.take() {
            let _ = response.send(outcome);
        }
        commands.entity(request_entity).despawn();
    }
    for (request_entity, mut request) in &mut pairings {
        let outcome = if let Some(error) = &authority.last_sync_error {
            Err(std::io::Error::other(error.clone()))
        } else if !pairing.matches(&request.pairing_token) {
            Ok(None)
        } else {
            devices.pair(*authority_entity, authority, pairing, &request.client_id)
        };
        if let Some(response) = request.response.take() {
            let _ = response.send(outcome);
        }
        commands.entity(request_entity).despawn();
    }
}

fn process_revalidations(
    authority: Single<(Entity, &AuthorizationAuthority)>,
    devices: AuthorizedDevices,
    mut revalidations: Query<(Entity, &mut RevalidateAuthorization)>,
    mut commands: Commands,
) {
    let (authority_entity, authority) = *authority;
    for (request_entity, mut request) in &mut revalidations {
        let outcome = if let Some(error) = &authority.last_sync_error {
            Err(std::io::Error::other(error.clone()))
        } else {
            Ok(devices.matches(authority_entity, &request.client_id, &request.device_token))
        };
        if let Some(response) = request.response.take() {
            let _ = response.send(outcome);
        }
        commands.entity(request_entity).despawn();
    }
}

fn process_mutations(
    mut authorities: Single<(Entity, &mut AuthorizationAuthority, &mut PairingCredential)>,
    mut devices: AuthorizedDevices,
    mut revokes: Query<(Entity, &mut RevokeAuthorization)>,
    mut resets: Query<(Entity, &mut ResetAuthorization)>,
    mut commands: Commands,
) {
    let (authority_entity, authority, pairing) = &mut *authorities;
    for (request_entity, mut request) in &mut revokes {
        let outcome = if let Some(error) = &authority.last_sync_error {
            Err(std::io::Error::other(error.clone()))
        } else {
            devices.revoke(*authority_entity, authority, pairing, &request.client_id)
        };
        if let Some(response) = request.response.take() {
            let _ = response.send(outcome);
        }
        commands.entity(request_entity).despawn();
    }
    for (request_entity, mut request) in &mut resets {
        let outcome = devices.reset(*authority_entity, authority, pairing);
        if let Some(response) = request.response.take() {
            let _ = response.send(outcome);
        }
        commands.entity(request_entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::{mpsc, oneshot};

    use super::*;

    struct AuthorityFixture {
        _directory: tempfile::TempDir,
        store: RemoteAuthorizationStore,
        app: App,
        authority: Entity,
    }

    impl AuthorityFixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let store = RemoteAuthorizationStore::new(directory.path().join("authorizations.json"));
            let (wake, _) = mpsc::unbounded_channel();
            let (_client, bundle) = RemoteAuthorizations::with_store(store.clone(), wake);
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, RemoteAuthorizationPlugin));
            let authority = app.world_mut().spawn(bundle).id();
            app.update();
            Self {
                _directory: directory,
                store,
                app,
                authority,
            }
        }

        fn pairing_token(&self) -> String {
            self.app
                .world()
                .get::<PairingCredential>(self.authority)
                .unwrap()
                .0
                .clone()
        }

        fn generation(&self) -> u64 {
            self.app
                .world()
                .get::<AuthorizationAuthority>(self.authority)
                .unwrap()
                .generation()
        }

        fn device_ids(&mut self) -> Vec<DeviceId> {
            let authority = self.authority;
            let mut query = self
                .app
                .world_mut()
                .query_filtered::<(&ChildOf, &AuthorizedDeviceId), With<AuthorizedDeviceEntity>>();
            let mut ids = Vec::new();
            for (parent, id) in query.iter(self.app.world()) {
                if parent.parent() == authority {
                    ids.push(id.0.clone());
                }
            }
            ids.sort();
            ids
        }

        fn pair(
            &mut self,
            client_id: DeviceId,
            pairing_token: String,
        ) -> std::io::Result<Option<AuthorizationOutcome>> {
            let (response, mut receiver) = oneshot::channel();
            self.app.world_mut().spawn(PairDevice {
                client_id,
                pairing_token,
                response: Some(response),
            });
            self.app.update();
            receiver.try_recv().unwrap()
        }

        fn authorizes(&mut self, client_id: DeviceId, device_token: &str) -> std::io::Result<bool> {
            let (response, mut receiver) = oneshot::channel();
            self.app.world_mut().spawn(RevalidateAuthorization {
                client_id,
                device_token: device_token.to_string(),
                response: Some(response),
            });
            self.app.update();
            receiver.try_recv().unwrap()
        }

        fn revoke(&mut self, client_id: DeviceId) -> std::io::Result<bool> {
            let (response, mut receiver) = oneshot::channel();
            self.app.world_mut().spawn(RevokeAuthorization {
                client_id,
                response: Some(response),
            });
            self.app.update();
            receiver.try_recv().unwrap()
        }
    }

    #[test]
    fn pairing_issues_a_device_token_and_rotates_the_pairing_token() {
        let mut fixture = AuthorityFixture::new();
        let pairing_token = fixture.pairing_token();
        let initial_generation = fixture.generation();
        let client = DeviceId::new("phone-a");
        let outcome = fixture
            .pair(client.clone(), pairing_token.clone())
            .unwrap()
            .unwrap();
        let AuthorizationOutcome::Paired { device_token } = outcome else {
            panic!("pairing did not issue a device token")
        };

        assert!(fixture.authorizes(client.clone(), &device_token).unwrap());
        assert_eq!(fixture.device_ids(), vec![client]);
        assert_ne!(fixture.pairing_token(), pairing_token);
        assert!(fixture.generation() > initial_generation);
        assert_eq!(
            fixture
                .pair(DeviceId::new("phone-b"), pairing_token)
                .unwrap(),
            None
        );
    }

    #[test]
    fn each_device_has_an_independent_revocable_token() {
        let mut fixture = AuthorityFixture::new();
        let first = DeviceId::new("phone-a");
        let second = DeviceId::new("phone-b");
        let first_pairing = fixture.pairing_token();
        let AuthorizationOutcome::Paired {
            device_token: first_token,
        } = fixture.pair(first.clone(), first_pairing).unwrap().unwrap()
        else {
            panic!("first pairing failed")
        };
        let second_pairing = fixture.pairing_token();
        let AuthorizationOutcome::Paired {
            device_token: second_token,
        } = fixture
            .pair(second.clone(), second_pairing)
            .unwrap()
            .unwrap()
        else {
            panic!("second pairing failed")
        };

        assert!(fixture.revoke(first.clone()).unwrap());
        assert!(!fixture.authorizes(first, &first_token).unwrap());
        assert!(fixture.authorizes(second.clone(), &second_token).unwrap());
        assert_eq!(fixture.device_ids(), vec![second]);
    }

    #[test]
    fn external_store_revocation_reconciles_canonical_entities() {
        let mut fixture = AuthorityFixture::new();
        let client = DeviceId::new("phone-a");
        let pairing = fixture.pairing_token();
        let AuthorizationOutcome::Paired { device_token } =
            fixture.pair(client.clone(), pairing).unwrap().unwrap()
        else {
            panic!("pairing failed")
        };
        let generation_before = fixture.generation();

        assert!(fixture.store.revoke(&client).unwrap());
        assert!(!fixture.authorizes(client, &device_token).unwrap());
        assert!(fixture.device_ids().is_empty());
        assert!(fixture.generation() > generation_before);
    }
}
