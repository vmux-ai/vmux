use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use ring::rand::SecureRandom;
use uuid::Uuid;
use vmux_ecs::JobWake;
use vmux_job::{BlockingCancellation, Job, JobOwner, JobPlugin, JobRequest, JobResult, JobSet};
use zeroize::Zeroizing;

use crate::availability::SecureCredentials;
use crate::cloud::transport::NativePeer;
use crate::connection::{DeviceKey, KeyPrepared, KeyRequest, Rejected};
use crate::control::{StoredKeyPrepared, StoredKeyRequest};
use crate::{Connection, Failure, Root, Set};

pub(crate) struct CredentialsPlugin;

impl bevy_app::Plugin for CredentialsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JobPlugin::<ReadKey>::default(),
            JobPlugin::<ReadStoredKey>::default(),
        ))
        .add_systems(Update, (read, read_stored).in_set(Set::Dispatch))
        .add_systems(Update, offer.in_set(Set::Publish))
        .add_systems(Update, (receive, receive_stored).in_set(JobSet::Apply));
    }
}

fn offer(roots: Query<Entity, (With<Root>, Without<SecureCredentials>)>, mut commands: Commands) {
    for root in &roots {
        commands.entity(root).insert(SecureCredentials);
    }
}

#[derive(Clone, Component)]
#[component(immutable)]
struct ReadKey {
    data: std::path::PathBuf,
    scope: String,
}

#[derive(Clone, Component)]
#[component(immutable)]
struct ReadStoredKey {
    data: std::path::PathBuf,
    scope: String,
}

impl Job for ReadStoredKey {
    type Output = Result<Option<DeviceKey>, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("account credential read canceled".into());
        }
        let file = vmux_profile::account_credentials::AccountCredentials::at(self.data, self.scope);
        let Some(bytes) = file.load_existing()? else {
            return Ok(None);
        };
        let id = Uuid::from_slice(&bytes[..16]).map_err(|_| "invalid account device identity")?;
        if id.is_nil() {
            return Err("invalid account device identity".into());
        }
        let mut seed = Zeroizing::new([0u8; 32]);
        seed.copy_from_slice(&bytes[16..]);
        Ok(Some(DeviceKey {
            id,
            seed,
            reused: true,
        }))
    }
}

fn read_stored(
    mut requests: MessageReader<StoredKeyRequest>,
    active: Query<(), With<NativePeer>>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !active.contains(request.entity) {
            continue;
        }
        let operation = ReadStoredKey {
            data: request.data.clone(),
            scope: request.scope.clone(),
        };
        commands
            .entity(request.entity)
            .insert((operation.clone(), JobRequest(operation)));
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

type StoredResults<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut JobResult<ReadStoredKey>,
        &'static JobOwner<ReadStoredKey>,
        Option<&'static ChildOf>,
    ),
>;

fn receive_stored(mut results: StoredResults, mut commands: Commands) {
    for (entity, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let value = match std::mem::replace(&mut result.0, Err("credential result consumed".into()))
        {
            Ok(value) => value,
            Err(error) => Err(error),
        };
        commands
            .entity(entity)
            .insert(StoredKeyPrepared(Some(value)))
            .remove::<JobResult<ReadStoredKey>>()
            .remove::<JobRequest<ReadStoredKey>>()
            .remove::<ReadStoredKey>();
    }
}

impl Job for ReadKey {
    type Output = Result<DeviceKey, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("account credential read canceled".into());
        }
        let mut proposed = Zeroizing::new([0u8; 48]);
        proposed[..16].copy_from_slice(Uuid::new_v4().as_bytes());
        ring::rand::SystemRandom::new()
            .fill(&mut proposed[16..])
            .map_err(|_| "device key generation failed")?;
        let file = vmux_profile::account_credentials::AccountCredentials::at(self.data, self.scope);
        let (stored, reused) = file.load_or_create(proposed.as_ref())?;
        let id = Uuid::from_slice(&stored[..16]).map_err(|_| "invalid account device identity")?;
        if id.is_nil() {
            return Err("invalid account device identity".into());
        }
        let mut seed = Zeroizing::new([0u8; 32]);
        seed.copy_from_slice(&stored[16..]);
        Ok(DeviceKey { id, seed, reused })
    }
}

fn read(
    mut requests: MessageReader<KeyRequest>,
    active: Query<(), With<Connection>>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !active.contains(request.entity) {
            continue;
        }
        let operation = ReadKey {
            data: request.data.clone(),
            scope: request.scope.clone(),
        };
        commands
            .entity(request.entity)
            .insert((operation.clone(), JobRequest(operation)));
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn receive(mut ready: Results, mut commands: Commands) {
    for (entity, mut result, owner, parent) in &mut ready {
        if !owner.accepts(parent) {
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("credential result consumed".into()));
        match value {
            Ok(key) => {
                commands.entity(entity).insert(KeyPrepared(Some(key)));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Rejected(Failure::storage(error)));
            }
        }
        commands
            .entity(entity)
            .remove::<JobResult<ReadKey>>()
            .remove::<JobRequest<ReadKey>>()
            .remove::<ReadKey>();
    }
}

type Results<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut JobResult<ReadKey>,
        &'static JobOwner<ReadKey>,
        Option<&'static ChildOf>,
    ),
>;
