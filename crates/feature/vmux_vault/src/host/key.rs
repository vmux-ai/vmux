use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_ecs::{
    AtGeneration, Cwd, EntityTarget, Generation, GenerationPlugin, Operation, OperationRetained,
    OperationTarget,
};
use zeroize::Zeroizing;

use crate::storage::VaultStorage;
use crate::storage::keys::{LOCKED, NativeKeyAccess};

use super::key_io::{GenerateTask, NativeCall, ObserveTask, ReadTask, StoreTask};
pub(crate) use crate::storage::keys::port::KeyPort;
use crate::storage::keys::port::{KeyIngress, KeyOrigin};

pub(super) struct KeyAuthorityPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct KeyAuthoritySet;

impl Plugin for KeyAuthorityPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<GenerationPlugin<VaultNamespace>>() {
            app.add_plugins(GenerationPlugin::<VaultNamespace>::default());
        }
        if !app.is_plugin_added::<GenerationPlugin<VaultKey>>() {
            app.add_plugins(GenerationPlugin::<VaultKey>::default());
        }
        app.add_message::<LockVault>().add_systems(
            Update,
            (
                (
                    lock,
                    retire_namespaces,
                    retire_requests,
                    receive_load,
                    receive_observe,
                    materialize,
                    admit_reads,
                    admit_creates,
                    admit_stores,
                )
                    .chain(),
                (
                    submit_read,
                    submit_observe,
                    submit_generate,
                    submit_store,
                    drain_read,
                    drain_observe,
                    drain_generate,
                    drain_store,
                    adopt_jobs,
                    deliver_replies,
                    cleanup,
                )
                    .chain(),
            )
                .chain()
                .in_set(KeyAuthoritySet)
                .after(super::RuntimeSet)
                .before(super::OperationSet),
        );
    }
}

#[derive(Component)]
pub(crate) struct KeyNamespace(pub(super) NativeKeyAccess);

impl KeyNamespace {
    pub(super) fn port(
        &self,
        namespace: Entity,
        generation: u64,
        admission: &mut KeyAdmission,
        operation: Entity,
        wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
    ) -> (KeyPort, KeyIngress) {
        admission.0 = admission.0.wrapping_add(1).max(1);
        KeyPort::new(
            KeyOrigin {
                operation,
                namespace,
                generation,
                admission: admission.0,
            },
            wake,
        )
    }
}

#[derive(Clone, Copy)]
pub(crate) struct VaultNamespace;

#[derive(Component, Default)]
pub(super) struct KeyAdmission(pub(super) u64);

#[derive(Component, Clone)]
pub(super) struct AcceptedStorage(pub(super) VaultStorage);

#[derive(Component)]
pub(super) struct RepositoryDirectory(pub(super) PathBuf);

#[derive(Component, Clone)]
pub(super) struct KeyCanceled(pub(super) Arc<AtomicBool>);

#[derive(Component, Clone, Copy)]
pub(super) struct KeyPermissions {
    pub(super) load: bool,
    pub(super) observe: bool,
    pub(super) create: bool,
    pub(super) store: bool,
}

#[derive(Component)]
pub(super) struct KeyManifest(pub(super) String);

#[derive(Component)]
pub(super) struct VaultKey;

#[derive(Component)]
pub(super) struct VaultId(pub(super) String);

#[derive(Component)]
struct UnlockedKey(Zeroizing<Vec<u8>>);

#[derive(Component)]
struct Locked;

#[derive(Component)]
struct LockCutoff(u64);

#[derive(Message)]
pub struct LockVault(pub Entity);

#[derive(Component)]
pub(super) struct KeyRequester(pub(super) KeyOrigin);

#[derive(Component)]
pub(super) struct EcsKeyRequest;

#[derive(Component)]
pub(super) struct StoreRequest;

#[derive(Component)]
pub(super) struct KeyOutcome {
    pub(super) result: Result<Zeroizing<Vec<u8>>, String>,
    pub(super) owner: Option<Entity>,
    pub(super) generation: Option<u64>,
}

#[derive(Component)]
struct KeyReply(mpsc::Sender<Result<Zeroizing<Vec<u8>>, String>>);

#[derive(Component)]
pub(super) struct LoadKey;

#[derive(Component)]
struct ObserveKey;

#[derive(Component)]
pub(super) struct CreateKey;

#[derive(Component)]
pub(super) struct StoreKey(pub(super) Zeroizing<Vec<u8>>);

#[derive(Component)]
struct KeyWaiting(Entity);

#[derive(Component)]
struct KeyResult(Result<Zeroizing<Vec<u8>>, String>);

impl KeyResult {
    fn locked() -> Self {
        Self(Err(LOCKED.to_string()))
    }
}

#[derive(Component)]
struct KeyLoading(Entity);

#[derive(Component, Clone)]
struct KeyIo {
    namespace: Entity,
    namespace_generation: u64,
    owner: Entity,
    generation: u64,
    vault_id: String,
    access: NativeKeyAccess,
    path: PathBuf,
    wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
}

impl KeyIo {
    fn notify(&self) {
        if let Some(wake) = &self.wake {
            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
    }
}

#[derive(Component)]
struct ReadReady;

#[derive(Component)]
struct ObserveReady;

#[derive(Component)]
struct GenerateReady;

#[derive(Component)]
struct StoreReady;

#[derive(Component)]
struct StoreBuffer(Zeroizing<Vec<u8>>);

#[derive(SystemParam)]
struct KeyAuthority<'w, 's> {
    ports: Query<
        'w,
        's,
        (
            &'static KeyPort,
            &'static KeyPermissions,
            Option<&'static KeyCanceled>,
        ),
    >,
    namespaces: Query<'w, 's, (&'static KeyNamespace, &'static Generation<VaultNamespace>)>,
    owners: Query<
        'w,
        's,
        (
            &'static VaultId,
            &'static Generation<VaultKey>,
            Option<&'static LockCutoff>,
        ),
        With<VaultKey>,
    >,
    requests: Query<
        'w,
        's,
        (
            Entity,
            &'static KeyRequester,
            Option<&'static EntityTarget<VaultKey>>,
            Option<&'static KeyWaiting>,
            Has<LoadKey>,
            Has<ObserveKey>,
            Has<CreateKey>,
            Has<StoreRequest>,
        ),
    >,
    ios: Query<'w, 's, &'static KeyIo>,
}

impl KeyAuthority<'_, '_> {
    fn origin_valid(&self, origin: KeyOrigin) -> bool {
        let Ok((port, _, canceled)) = self.ports.get(origin.operation) else {
            return false;
        };
        if port.origin != origin || canceled.is_some_and(|c| c.0.load(Ordering::Acquire)) {
            return false;
        }
        self.namespaces
            .get(origin.namespace)
            .is_ok_and(|(_, generation)| generation.value() == origin.generation)
    }

    fn request_valid(&self, request: Entity) -> bool {
        let Ok((_, requester, target, _, is_load, is_observe, is_create, is_store)) =
            self.requests.get(request)
        else {
            return false;
        };
        if !self.origin_valid(requester.0) {
            return false;
        }
        let Ok((_, permissions, _)) = self.ports.get(requester.0.operation) else {
            return false;
        };
        let allowed = (is_load && permissions.load)
            || (is_observe && permissions.observe)
            || (is_create && permissions.create)
            || (is_store && permissions.store);
        if !allowed {
            return false;
        }
        if let Some(target) = target {
            let Ok((_, _, cutoff)) = self.owners.get(target.entity()) else {
                return false;
            };
            if cutoff.is_some_and(|cutoff| requester.0.admission <= cutoff.0) {
                return false;
            }
        }
        true
    }

    fn io_current(&self, io: &KeyIo) -> bool {
        self.namespaces
            .get(io.namespace)
            .is_ok_and(|(_, generation)| generation.value() == io.namespace_generation)
            && self
                .owners
                .get(io.owner)
                .is_ok_and(|(_, generation, _)| generation.value() == io.generation)
    }

    fn io_accepted(&self, io: &KeyIo, job: Entity) -> bool {
        if !self.io_current(io) {
            return false;
        }
        self.requests
            .iter()
            .any(|(entity, _, _, waiting, _, _, _, _)| {
                waiting.is_some_and(|w| w.0 == job) && self.request_valid(entity)
            })
    }

    fn io_accepted_write(&self, io: &KeyIo, job: Entity) -> bool {
        if !self.io_current(io) {
            return false;
        }
        for (entity, requester, _, waiting, _, _, is_create, is_store) in &self.requests {
            if waiting.is_none_or(|w| w.0 != job) || !self.request_valid(entity) {
                continue;
            }
            let Ok((_, permissions, _)) = self.ports.get(requester.0.operation) else {
                continue;
            };
            if (is_create && permissions.create) || (is_store && permissions.store) {
                return true;
            }
        }
        false
    }

    fn io_capture(&self, origin: KeyOrigin, owner: Entity) -> Option<KeyIo> {
        let (access, _) = self.namespaces.get(origin.namespace).ok()?;
        let (vault_id, generation, _) = self.owners.get(owner).ok()?;
        let (port, _, _) = self.ports.get(origin.operation).ok()?;
        Some(KeyIo {
            namespace: origin.namespace,
            namespace_generation: origin.generation,
            owner,
            generation: generation.value(),
            path: access.0.path(&vault_id.0),
            vault_id: vault_id.0.clone(),
            access: access.0.clone(),
            wake: port.wake.clone(),
        })
    }

    fn io_claimed(&self, path: &Path) -> bool {
        self.ios.iter().any(|io| io.path == path)
    }
}

fn lock(
    mut requests: MessageReader<LockVault>,
    mut owners: Query<
        (
            &EntityTarget<KeyNamespace>,
            &mut Generation<VaultKey>,
            Option<&LockCutoff>,
        ),
        With<VaultKey>,
    >,
    mut namespaces: Query<(&KeyAdmission, Option<&mut super::VaultRegistry>), With<KeyNamespace>>,
    mut commands: Commands,
) {
    for LockVault(owner) in requests.read() {
        let Ok((target, mut generation, existing_cutoff)) = owners.get_mut(*owner) else {
            continue;
        };
        let namespace = target.entity();
        let admission = if let Ok((admission, registry)) = namespaces.get_mut(namespace) {
            if let Some(mut registry) = registry {
                registry.invalidate();
            }
            commands
                .entity(namespace)
                .remove::<super::VaultStatusProjection>();
            admission.0
        } else {
            0
        };
        let cutoff = existing_cutoff.map_or(admission, |existing| existing.0.max(admission));
        generation.advance();
        commands
            .entity(*owner)
            .remove::<UnlockedKey>()
            .insert((Locked, LockCutoff(cutoff)));
    }
}

fn retire_namespaces(
    mut namespaces: Query<(Entity, Ref<KeyNamespace>, &mut Generation<VaultNamespace>)>,
    manifests: Query<(Entity, Ref<KeyManifest>)>,
    keys: Query<
        (
            Entity,
            &EntityTarget<KeyNamespace>,
            &VaultId,
            &AtGeneration<VaultNamespace>,
        ),
        With<VaultKey>,
    >,
    mut commands: Commands,
) {
    for (_, access, mut generation) in &mut namespaces {
        if access.is_changed() && !access.is_added() {
            generation.advance();
        }
    }
    for (namespace, manifest) in &manifests {
        if !manifest.is_changed() {
            continue;
        }
        for (entity, target, vault_id, _) in &keys {
            if target.entity() == namespace && vault_id.0 != manifest.0 {
                commands.entity(entity).despawn();
            }
        }
    }
    for (entity, target, _, accepted) in &keys {
        let alive = namespaces
            .get(target.entity())
            .is_ok_and(|(_, _, generation)| accepted.matches(*generation));
        if !alive {
            commands.entity(entity).despawn();
        }
    }
}

fn retire_requests(
    authority: KeyAuthority,
    ports: Query<(&KeyPort, &KeyCanceled)>,
    requests: Query<Entity, (With<KeyRequester>, Without<KeyResult>, Without<KeyOutcome>)>,
    mut commands: Commands,
) {
    for (port, canceled) in &ports {
        if !authority.origin_valid(port.origin) {
            canceled.0.store(true, Ordering::Release);
        }
    }
    for entity in &requests {
        if !authority.request_valid(entity) {
            commands
                .entity(entity)
                .insert(KeyResult(Err("Vault key operation retired".to_string())));
        }
    }
}

fn receive_load(ingress: Query<&KeyIngress>, mut commands: Commands) {
    for ingress in &ingress {
        for request in ingress.load.lock().try_iter() {
            commands.spawn((
                Operation::bundle("vault.key.load").unwrap().running(),
                OperationRetained,
                KeyRequester(request.origin),
                VaultId(request.vault_id),
                KeyReply(request.reply),
                LoadKey,
            ));
        }
    }
}

fn receive_observe(ingress: Query<&KeyIngress>, mut commands: Commands) {
    for ingress in &ingress {
        for request in ingress.observe.lock().try_iter() {
            commands.spawn((
                Operation::bundle("vault.key.observe").unwrap().running(),
                OperationRetained,
                KeyRequester(request.origin),
                VaultId(request.vault_id),
                KeyReply(request.reply),
                ObserveKey,
            ));
        }
    }
}

fn materialize(
    authority: KeyAuthority,
    requests: Query<
        (Entity, &KeyRequester, &VaultId),
        (Without<EntityTarget<VaultKey>>, Without<KeyResult>),
    >,
    existing_owners: Query<
        (
            Entity,
            &VaultId,
            &EntityTarget<KeyNamespace>,
            Option<&LockCutoff>,
        ),
        With<VaultKey>,
    >,
    namespace_dirs: Query<(Option<&Cwd>, Option<&RepositoryDirectory>), With<KeyNamespace>>,
    mut commands: Commands,
) {
    let mut created: BTreeMap<(Entity, String), Entity> = BTreeMap::new();
    for (entity, requester, vault_id) in &requests {
        let origin = requester.0;
        if vault_id.0.is_empty() || !authority.request_valid(entity) {
            commands
                .entity(entity)
                .insert(KeyResult(Err("Vault key request refused".to_string())));
            continue;
        }
        let existing = existing_owners
            .iter()
            .find(|(_, id, target, _)| id.0 == vault_id.0 && target.entity() == origin.namespace);
        let (owner, cutoff) = match existing {
            Some((owner, _, _, cutoff)) => (owner, cutoff.map(|c| c.0)),
            None => {
                let key = (origin.namespace, vault_id.0.clone());
                let owner = match created.get(&key) {
                    Some(&owner) => owner,
                    None => {
                        let (cwd, repository) = namespace_dirs
                            .get(origin.namespace)
                            .map(|(cwd, repository)| {
                                (
                                    cwd.map(|c| c.0.clone()).unwrap_or_default(),
                                    repository.map(|r| r.0.clone()).unwrap_or_default(),
                                )
                            })
                            .unwrap_or_default();
                        let owner = commands
                            .spawn((
                                Name::new("Vault key"),
                                VaultKey,
                                VaultId(vault_id.0.clone()),
                                Cwd(cwd),
                                RepositoryDirectory(repository),
                                EntityTarget::<KeyNamespace>::new(origin.namespace),
                                AtGeneration::<VaultNamespace>::new(origin.generation),
                                Generation::<VaultKey>::default(),
                            ))
                            .id();
                        created.insert(key, owner);
                        owner
                    }
                };
                (owner, None)
            }
        };
        if cutoff.is_some_and(|cutoff| origin.admission <= cutoff) {
            commands
                .entity(entity)
                .insert(KeyResult(Err(LOCKED.to_string())));
            continue;
        }
        commands
            .entity(entity)
            .insert(EntityTarget::<VaultKey>::new(owner));
    }
}

fn admit_reads(
    authority: KeyAuthority,
    requests: Query<
        (
            Entity,
            &KeyRequester,
            &EntityTarget<VaultKey>,
            Has<ObserveKey>,
        ),
        (
            Or<(With<LoadKey>, With<ObserveKey>)>,
            Without<KeyWaiting>,
            Without<KeyResult>,
        ),
    >,
    owners: Query<(Option<&UnlockedKey>, Has<Locked>, Option<&KeyLoading>), With<VaultKey>>,
    mut commands: Commands,
) {
    let mut spawned_jobs: BTreeMap<Entity, Entity> = BTreeMap::new();
    let mut claimed_paths: BTreeSet<PathBuf> = BTreeSet::new();
    for (entity, requester, target, observing) in &requests {
        let owner = target.entity();
        let Ok((unlocked, locked, loading)) = owners.get(owner) else {
            continue;
        };
        if let Some(key) = unlocked {
            commands
                .entity(entity)
                .insert(KeyResult(Ok(Zeroizing::new(key.0.to_vec()))));
            continue;
        }
        if observing && locked {
            commands.entity(entity).insert(KeyResult::locked());
            continue;
        }
        if let Some(&job) = spawned_jobs.get(&owner) {
            commands.entity(entity).insert(KeyWaiting(job));
            continue;
        }
        if let Some(loading) = loading {
            let job = loading.0;
            if authority
                .ios
                .get(job)
                .is_ok_and(|io| authority.io_current(io))
            {
                commands.entity(entity).insert(KeyWaiting(job));
            }
            continue;
        }
        let Some(io) = authority.io_capture(requester.0, owner) else {
            continue;
        };
        if authority.io_claimed(&io.path) || claimed_paths.contains(&io.path) {
            continue;
        }
        claimed_paths.insert(io.path.clone());
        let job = commands
            .spawn((
                Operation::bundle("vault.key.read").unwrap().running(),
                OperationRetained,
                OperationTarget::new(owner),
                io,
            ))
            .id();
        if observing {
            commands.entity(job).insert(ObserveReady);
        } else {
            commands.entity(job).insert(ReadReady);
        }
        spawned_jobs.insert(owner, job);
        commands.entity(owner).insert(KeyLoading(job));
        commands.entity(entity).insert(KeyWaiting(job));
    }
}

fn admit_creates(
    authority: KeyAuthority,
    requests: Query<
        (Entity, &KeyRequester, &EntityTarget<VaultKey>),
        (With<CreateKey>, Without<KeyWaiting>, Without<KeyResult>),
    >,
    owners: Query<Option<&KeyLoading>, With<VaultKey>>,
    create_jobs: Query<(), Or<(With<GenerateReady>, With<GenerateTask>, With<StoreBuffer>)>>,
    mut commands: Commands,
) {
    let mut spawned_jobs: BTreeMap<Entity, Entity> = BTreeMap::new();
    let mut claimed_paths: BTreeSet<PathBuf> = BTreeSet::new();
    for (entity, requester, target) in &requests {
        let owner = target.entity();
        if let Some(&job) = spawned_jobs.get(&owner) {
            commands.entity(entity).insert(KeyWaiting(job));
            continue;
        }
        if let Ok(Some(loading)) = owners.get(owner) {
            if create_jobs.get(loading.0).is_ok()
                && authority
                    .ios
                    .get(loading.0)
                    .is_ok_and(|io| authority.io_current(io))
            {
                commands.entity(entity).insert(KeyWaiting(loading.0));
            }
            continue;
        }
        let Some(mut io) = authority.io_capture(requester.0, owner) else {
            continue;
        };
        if authority.io_claimed(&io.path) || claimed_paths.contains(&io.path) {
            continue;
        }
        claimed_paths.insert(io.path.clone());
        io.generation = io.generation.saturating_add(1);
        commands
            .entity(owner)
            .remove::<UnlockedKey>()
            .insert(Generation::<VaultKey>::new(io.generation));
        let job = commands
            .spawn((
                Operation::bundle("vault.key.generate").unwrap().running(),
                OperationRetained,
                OperationTarget::new(owner),
                io,
                GenerateReady,
            ))
            .id();
        spawned_jobs.insert(owner, job);
        commands.entity(owner).insert(KeyLoading(job));
        commands.entity(entity).insert(KeyWaiting(job));
    }
}

fn admit_stores(
    authority: KeyAuthority,
    requests: Query<
        (Entity, &KeyRequester, &EntityTarget<VaultKey>, &StoreKey),
        (Without<KeyWaiting>, Without<KeyResult>),
    >,
    mut commands: Commands,
) {
    let mut claimed_paths: BTreeSet<PathBuf> = BTreeSet::new();
    for (entity, requester, target, store_key) in &requests {
        let owner = target.entity();
        let Some(mut io) = authority.io_capture(requester.0, owner) else {
            continue;
        };
        if authority.io_claimed(&io.path) || claimed_paths.contains(&io.path) {
            continue;
        }
        claimed_paths.insert(io.path.clone());
        io.generation = io.generation.saturating_add(1);
        commands
            .entity(owner)
            .remove::<UnlockedKey>()
            .insert(Generation::<VaultKey>::new(io.generation));
        let job = commands
            .spawn((
                Operation::bundle("vault.key.write").unwrap().running(),
                OperationRetained,
                OperationTarget::new(owner),
                io,
                StoreBuffer(Zeroizing::new(store_key.0.to_vec())),
                StoreReady,
            ))
            .id();
        commands.entity(owner).insert(KeyLoading(job));
        commands
            .entity(entity)
            .remove::<StoreKey>()
            .insert(KeyWaiting(job));
    }
}

fn submit_read(
    authority: KeyAuthority,
    jobs: Query<(Entity, &KeyIo), With<ReadReady>>,
    mut commands: Commands,
) {
    for (entity, io) in &jobs {
        commands.entity(entity).remove::<ReadReady>();
        if !authority.io_accepted(io, entity) {
            commands
                .entity(entity)
                .insert(KeyResult(Err("Vault key operation retired".to_string())));
            continue;
        }
        let io = io.clone();
        let wake = io.wake.clone();
        let call = NativeCall::spawn(wake, move || io.access.read(&io.vault_id));
        commands.entity(entity).insert(ReadTask(call));
    }
}

fn submit_observe(
    authority: KeyAuthority,
    jobs: Query<(Entity, &KeyIo), With<ObserveReady>>,
    mut commands: Commands,
) {
    for (entity, io) in &jobs {
        commands.entity(entity).remove::<ObserveReady>();
        if !authority.io_accepted(io, entity) {
            commands
                .entity(entity)
                .insert(KeyResult(Err("Vault key operation retired".to_string())));
            continue;
        }
        let io = io.clone();
        let wake = io.wake.clone();
        let call = NativeCall::spawn(wake, move || io.access.observe(&io.vault_id));
        commands.entity(entity).insert(ObserveTask(call));
    }
}

fn submit_generate(
    authority: KeyAuthority,
    jobs: Query<(Entity, &KeyIo), With<GenerateReady>>,
    mut commands: Commands,
) {
    for (entity, io) in &jobs {
        commands.entity(entity).remove::<GenerateReady>();
        if !authority.io_accepted_write(io, entity) {
            commands
                .entity(entity)
                .insert(KeyResult(Err("Vault key operation retired".to_string())));
            continue;
        }
        let io = io.clone();
        let wake = io.wake.clone();
        let call = NativeCall::spawn(wake, move || io.access.generate());
        commands.entity(entity).insert(GenerateTask(call));
    }
}

fn submit_store(
    authority: KeyAuthority,
    jobs: Query<(Entity, &KeyIo, Option<&StoreBuffer>), With<StoreReady>>,
    mut commands: Commands,
) {
    for (entity, io, buffer) in &jobs {
        commands.entity(entity).remove::<StoreReady>();
        if !authority.io_accepted_write(io, entity) {
            commands
                .entity(entity)
                .insert(KeyResult(Err("Vault key operation retired".to_string())));
            continue;
        }
        let Some(buffer) = buffer else {
            continue;
        };
        let key = Zeroizing::new(buffer.0.to_vec());
        let io = io.clone();
        let wake = io.wake.clone();
        let call = NativeCall::spawn(wake, move || io.access.write(&io.vault_id, &key));
        commands.entity(entity).insert(StoreTask(call));
    }
}

fn drain_read(mut jobs: Query<(Entity, &mut ReadTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut jobs {
        let Some(result) = task.0.poll() else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<ReadTask>()
            .insert(KeyResult(result));
    }
}

fn drain_observe(
    mut jobs: Query<(Entity, &KeyIo, &mut ObserveTask)>,
    waiting_loads: Query<(Entity, &KeyWaiting), With<LoadKey>>,
    mut commands: Commands,
) {
    for (entity, io, mut task) in &mut jobs {
        let Some(result) = task.0.poll() else {
            continue;
        };
        commands.entity(entity).remove::<ObserveTask>();
        let result = match result {
            Ok(Some(key)) => Ok(key),
            Ok(None) => Err(LOCKED.to_string()),
            Err(e) => Err(e),
        };
        if result.is_err() {
            for (request, waiting) in &waiting_loads {
                if waiting.0 == entity {
                    commands.entity(request).remove::<KeyWaiting>();
                    io.notify();
                }
            }
        }
        commands.entity(entity).insert(KeyResult(result));
    }
}

fn drain_generate(mut jobs: Query<(Entity, &KeyIo, &mut GenerateTask)>, mut commands: Commands) {
    for (entity, io, mut task) in &mut jobs {
        let Some(result) = task.0.poll() else {
            continue;
        };
        commands.entity(entity).remove::<GenerateTask>();
        match result {
            Ok(key) => {
                commands
                    .entity(entity)
                    .insert((StoreBuffer(key), StoreReady));
                io.notify();
            }
            Err(error) => {
                commands.entity(entity).insert(KeyResult(Err(error)));
            }
        }
    }
}

fn drain_store(
    authority: KeyAuthority,
    mut jobs: Query<(Entity, &KeyIo, Option<&StoreBuffer>, &mut StoreTask)>,
    mut commands: Commands,
) {
    for (entity, io, buffer, mut task) in &mut jobs {
        let Some(result) = task.0.poll() else {
            continue;
        };
        commands.entity(entity).remove::<(StoreTask, StoreBuffer)>();
        let result = match result {
            Ok(receipt) => {
                if receipt.created && !authority.io_current(io) {
                    warn!("Retired Vault key creation retained its durable file");
                }
                match buffer {
                    Some(buffer) => Ok(Zeroizing::new(buffer.0.to_vec())),
                    None => Err("Vault key write lost its accepted input".to_string()),
                }
            }
            Err(error) => Err(error),
        };
        commands.entity(entity).insert(KeyResult(result));
    }
}

fn adopt_jobs(
    authority: KeyAuthority,
    jobs: Query<(Entity, &KeyIo, &KeyResult)>,
    owners: Query<Option<&KeyLoading>, With<VaultKey>>,
    waiting: Query<(Entity, &KeyWaiting)>,
    pending: Query<(), (With<KeyRequester>, Without<KeyWaiting>, Without<KeyResult>)>,
    mut commands: Commands,
) {
    let has_pending = !pending.is_empty();
    for (job, io, result) in &jobs {
        let active = authority.io_accepted(io, job);
        let outcome = if active {
            match &result.0 {
                Ok(key) => Ok(Zeroizing::new(key.to_vec())),
                Err(error) => Err(error.clone()),
            }
        } else {
            Err("Vault key operation retired".to_string())
        };
        if let Ok(key) = &outcome {
            commands
                .entity(io.owner)
                .remove::<Locked>()
                .insert(UnlockedKey(Zeroizing::new(key.to_vec())));
        }
        if owners
            .get(io.owner)
            .ok()
            .flatten()
            .is_some_and(|loading| loading.0 == job)
        {
            commands.entity(io.owner).remove::<KeyLoading>();
        }
        for (entity, link) in &waiting {
            if link.0 != job {
                continue;
            }
            let reply = match &outcome {
                Ok(key) => Ok(Zeroizing::new(key.to_vec())),
                Err(error) => Err(error.clone()),
            };
            commands.entity(entity).insert(KeyResult(reply));
        }
        commands.entity(job).despawn();
        if has_pending {
            io.notify();
        }
    }
}

fn deliver_replies(
    authority: KeyAuthority,
    finished: Query<
        (
            Entity,
            &KeyResult,
            Option<&EntityTarget<VaultKey>>,
            Option<&KeyReply>,
            Has<EcsKeyRequest>,
        ),
        (Without<KeyIo>, Without<KeyOutcome>),
    >,
    mut commands: Commands,
) {
    for (entity, result, target, key_reply, ecs_request) in &finished {
        let valid = authority.request_valid(entity);
        let outcome = if valid {
            match &result.0 {
                Ok(key) => Ok(Zeroizing::new(key.to_vec())),
                Err(error) => Err(error.clone()),
            }
        } else {
            Err("Vault key operation retired".to_string())
        };
        if let Some(reply) = key_reply {
            let _ = reply.0.send(outcome);
            commands.entity(entity).despawn();
        } else if ecs_request {
            let owner = target.map(|t| t.entity());
            let generation =
                owner.and_then(|o| authority.owners.get(o).ok().map(|(_, g, _)| g.value()));
            commands
                .entity(entity)
                .remove::<KeyResult>()
                .insert(KeyOutcome {
                    result: outcome,
                    owner,
                    generation,
                });
        } else {
            commands.entity(entity).despawn();
        }
    }
}

fn cleanup(
    waiting: Query<
        (Entity, &KeyWaiting),
        (With<KeyRequester>, Without<KeyResult>, Without<KeyOutcome>),
    >,
    ios: Query<(), With<KeyIo>>,
    mut commands: Commands,
) {
    for (entity, link) in &waiting {
        if ios.get(link.0).is_err() {
            commands
                .entity(entity)
                .insert(KeyResult(Err("Vault key operation retired".to_string())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::keys::{KeyWriteReceipt, NativeKeyFixture};
    use parking_lot::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    struct KeyFiles {
        directory: PathBuf,
        reads: Arc<AtomicUsize>,
        writes: Arc<AtomicUsize>,
        generates: Arc<AtomicUsize>,
        read_gate: Mutex<Option<mpsc::Receiver<()>>>,
        generate_gate: Mutex<Option<mpsc::Receiver<()>>>,
        write_gate: Mutex<Option<mpsc::Receiver<()>>>,
        fail_write: AtomicBool,
    }

    impl KeyFiles {
        fn new(directory: PathBuf) -> Self {
            Self {
                directory,
                reads: Arc::new(AtomicUsize::new(0)),
                writes: Arc::new(AtomicUsize::new(0)),
                generates: Arc::new(AtomicUsize::new(0)),
                read_gate: Mutex::new(None),
                generate_gate: Mutex::new(None),
                write_gate: Mutex::new(None),
                fail_write: AtomicBool::new(false),
            }
        }

        fn wait(gate: &Mutex<Option<mpsc::Receiver<()>>>) -> Result<(), String> {
            let gate = gate.lock().take();
            if let Some(gate) = gate {
                gate.recv_timeout(Duration::from_secs(3))
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        }
    }

    impl NativeKeyFixture for KeyFiles {
        fn path(&self, vault_id: &str) -> PathBuf {
            self.directory.join(vault_id)
        }

        fn read(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Self::wait(&self.read_gate)?;
            std::fs::read(self.directory.join(vault_id))
                .map(Zeroizing::new)
                .map_err(|e| e.to_string())
        }

        fn observe(&self, vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
            match std::fs::read(self.directory.join(vault_id)) {
                Ok(bytes) => Ok(Some(Zeroizing::new(bytes))),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        }

        fn generate(&self) -> Result<Zeroizing<Vec<u8>>, String> {
            self.generates.fetch_add(1, Ordering::SeqCst);
            Self::wait(&self.generate_gate)?;
            Ok(Zeroizing::new(vec![9; 32]))
        }

        fn write(&self, vault_id: &str, key: &[u8]) -> Result<KeyWriteReceipt, String> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            Self::wait(&self.write_gate)?;
            if self.fail_write.load(Ordering::SeqCst) {
                return Err("isolated durable write refused".into());
            }
            let path = self.directory.join(vault_id);
            let created = !path.exists();
            std::fs::write(path, key).map_err(|e| e.to_string())?;
            Ok(KeyWriteReceipt { created })
        }
    }

    struct Scenario {
        app: App,
        namespace: Entity,
        _temp: tempfile::TempDir,
        callers: Vec<std::thread::JoinHandle<()>>,
    }

    impl Scenario {
        fn new(files: Arc<KeyFiles>) -> Self {
            let temp = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins(KeyAuthorityPlugin);
            let namespace = app
                .world_mut()
                .spawn((
                    Name::new("Isolated key namespace"),
                    Cwd(temp.path().to_path_buf()),
                    RepositoryDirectory(temp.path().join("repository")),
                    KeyNamespace(NativeKeyAccess::fixture(files.directory.clone(), files)),
                    Generation::<VaultNamespace>::default(),
                    KeyAdmission::default(),
                ))
                .id();
            Self {
                app,
                namespace,
                _temp: temp,
                callers: Vec::new(),
            }
        }

        fn caller(&mut self) -> KeyPort {
            let operation = self
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("test.vault.caller").unwrap().running(),
                    OperationRetained,
                    KeyCanceled(Arc::new(AtomicBool::new(false))),
                    KeyPermissions {
                        load: true,
                        observe: true,
                        create: true,
                        store: true,
                    },
                ))
                .id();
            let world = self.app.world_mut();
            let mut query = world.query::<(
                &KeyNamespace,
                &Generation<VaultNamespace>,
                &mut KeyAdmission,
            )>();
            let (access, generation, mut admission) = query.get_mut(world, self.namespace).unwrap();
            let (port, ingress) = access.port(
                self.namespace,
                generation.value(),
                &mut admission,
                operation,
                None,
            );
            world.entity_mut(operation).insert((port.clone(), ingress));
            port
        }

        fn request<T: Send + 'static>(
            &mut self,
            port: KeyPort,
            call: impl FnOnce(KeyPort) -> Result<T, String> + Send + 'static,
        ) -> mpsc::Receiver<Result<T, String>> {
            let (sender, receiver) = mpsc::channel();
            self.callers.push(std::thread::spawn(move || {
                let _ = sender.send(call(port));
            }));
            receiver
        }

        fn create_request(&mut self, origin: KeyOrigin, vault_id: &str) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    Operation::bundle("vault.key.create").unwrap().running(),
                    OperationRetained,
                    KeyRequester(origin),
                    VaultId(vault_id.to_string()),
                    CreateKey,
                    EcsKeyRequest,
                ))
                .id()
        }

        fn store_request(&mut self, origin: KeyOrigin, vault_id: &str, key: &[u8]) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    Operation::bundle("vault.key.store").unwrap().running(),
                    OperationRetained,
                    KeyRequester(origin),
                    VaultId(vault_id.to_string()),
                    StoreKey(Zeroizing::new(key.to_vec())),
                    StoreRequest,
                    EcsKeyRequest,
                ))
                .id()
        }

        fn ecs_result(&mut self, request: Entity) -> Result<Zeroizing<Vec<u8>>, String> {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if let Some(outcome) = self
                    .app
                    .world_mut()
                    .entity_mut(request)
                    .take::<KeyOutcome>()
                {
                    self.app.world_mut().despawn(request);
                    return outcome.result;
                }
                assert!(
                    Instant::now() < deadline,
                    "canonical key response did not complete"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn result<T>(&mut self, receiver: &mpsc::Receiver<T>) -> T {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                match receiver.try_recv() {
                    Ok(result) => return result,
                    Err(mpsc::TryRecvError::Disconnected) => panic!("native caller disappeared"),
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                assert!(
                    Instant::now() < deadline,
                    "canonical key response did not complete"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn entered(&mut self, counter: &AtomicUsize) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while counter.load(Ordering::SeqCst) == 0 {
                self.app.update();
                assert!(
                    Instant::now() < deadline,
                    "native request was not submitted"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn owner(&mut self) -> Entity {
            let world = self.app.world_mut();
            world
                .query_filtered::<Entity, With<VaultKey>>()
                .single(world)
                .unwrap()
        }
    }

    impl Drop for Scenario {
        fn drop(&mut self) {
            self.app.world_mut().clear_entities();
            for caller in self.callers.drain(..) {
                let _ = caller.join();
            }
        }
    }

    #[test]
    fn concurrent_accepted_loads_share_one_native_read() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("vault"), vec![7; 32]).unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let reads = files.reads.clone();
        let (release, gate) = mpsc::channel();
        *files.read_gate.lock() = Some(gate);
        let mut scenario = Scenario::new(files);
        let mut replies = Vec::new();
        for _ in 0..8 {
            let port = scenario.caller();
            replies.push(scenario.request(port, |p| p.load("vault")));
        }
        scenario.entered(&reads);
        for _ in 0..3 {
            scenario.app.update();
        }
        release.send(()).unwrap();
        for reply in replies {
            assert_eq!(*scenario.result(&reply).unwrap(), vec![7; 32]);
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn explicit_lock_refuses_silent_reopening_and_preserves_accepted_loans() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("vault"), vec![7; 32]).unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let reads = files.reads.clone();
        let mut scenario = Scenario::new(files);
        let port = scenario.caller();
        let reply = scenario.request(port, |p| p.load("vault"));
        let loan = scenario.result(&reply).unwrap();
        let owner = scenario.owner();
        scenario.app.world_mut().write_message(LockVault(owner));
        scenario.app.update();
        let port = scenario.caller();
        let reply = scenario.request(port, |p| p.observe("vault"));
        assert!(scenario.result(&reply).is_err());
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert_eq!(*loan, vec![7; 32]);
        let port = scenario.caller();
        let reply = scenario.request(port, |p| p.load("vault"));
        assert_eq!(*scenario.result(&reply).unwrap(), vec![7; 32]);
        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn canceled_before_admission_never_reads_native_storage() {
        let temp = tempfile::tempdir().unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let reads = files.reads.clone();
        let mut scenario = Scenario::new(files);
        let port = scenario.caller();
        scenario
            .app
            .world()
            .get::<KeyCanceled>(port.origin.operation)
            .unwrap()
            .0
            .store(true, Ordering::Release);
        let reply = scenario.request(port, |p| p.load("vault"));
        assert!(scenario.result(&reply).is_err());
        assert_eq!(reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn retired_namespace_drains_native_context_before_release() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("vault"), vec![7; 32]).unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let reads = files.reads.clone();
        let weak = Arc::downgrade(&files);
        let (release, gate) = mpsc::channel();
        *files.read_gate.lock() = Some(gate);
        let mut scenario = Scenario::new(files);
        let port = scenario.caller();
        let reply = scenario.request(port, |p| p.load("vault"));
        scenario.entered(&reads);
        scenario.app.world_mut().despawn(scenario.namespace);
        assert!(scenario.result(&reply).is_err());
        assert!(weak.upgrade().is_some());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while weak.upgrade().is_some() {
            scenario.app.update();
            assert!(
                Instant::now() < deadline,
                "native context remained after drain"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            scenario
                .app
                .world_mut()
                .query_filtered::<Entity, With<UnlockedKey>>()
                .iter(scenario.app.world())
                .next()
                .is_none()
        );
    }

    #[test]
    fn failed_durable_creation_never_grants_an_unlocked_key() {
        let temp = tempfile::tempdir().unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        files.fail_write.store(true, Ordering::SeqCst);
        let mut scenario = Scenario::new(files);
        let port = scenario.caller();
        let request = scenario.create_request(port.origin, "vault");
        assert!(scenario.ecs_result(request).is_err());
        assert!(!temp.path().join("vault").exists());
        let port = scenario.caller();
        let reply = scenario.request(port, |p| p.observe("vault"));
        assert!(scenario.result(&reply).is_err());
    }

    #[test]
    fn generated_key_is_published_after_durable_write_only() {
        let temp = tempfile::tempdir().unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let writes = files.writes.clone();
        let (release, gate) = mpsc::channel();
        *files.write_gate.lock() = Some(gate);
        let mut scenario = Scenario::new(files);
        let port = scenario.caller();
        let request = scenario.create_request(port.origin, "vault");
        scenario.entered(&writes);
        let owner = scenario.owner();
        assert!(scenario.app.world().get::<UnlockedKey>(owner).is_none());
        release.send(()).unwrap();
        let key = scenario.ecs_result(request).unwrap();
        assert_eq!(
            std::fs::read(temp.path().join("vault")).unwrap(),
            key.to_vec()
        );
        let port = scenario.caller();
        let reply = scenario.request(port, |p| p.observe("vault"));
        assert_eq!(*scenario.result(&reply).unwrap(), *key);
    }

    #[test]
    fn namespace_replacement_serializes_the_physical_key_path_until_drain() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("vault"), vec![7; 32]).unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let reads = files.reads.clone();
        let (release, gate) = mpsc::channel();
        *files.read_gate.lock() = Some(gate);
        let mut scenario = Scenario::new(files);
        let port = scenario.caller();
        let first = scenario.request(port, |p| p.load("vault"));
        scenario.entered(&reads);
        scenario
            .app
            .world_mut()
            .get_mut::<Generation<VaultNamespace>>(scenario.namespace)
            .unwrap()
            .advance();
        let port = scenario.caller();
        let replacement = scenario.request(port, |p| p.load("vault"));
        assert!(scenario.result(&first).is_err());
        for _ in 0..3 {
            scenario.app.update();
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        release.send(()).unwrap();
        assert_eq!(*scenario.result(&replacement).unwrap(), vec![7; 32]);
        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_revoked_store_permission_refuses_without_native_write() {
        let temp = tempfile::tempdir().unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let writes = files.writes.clone();
        let mut scenario = Scenario::new(files);
        let port = scenario.caller();
        scenario
            .app
            .world_mut()
            .get_mut::<KeyPermissions>(port.origin.operation)
            .unwrap()
            .store = false;
        let request = scenario.store_request(port.origin, "vault", &[8; 32]);
        assert!(scenario.ecs_result(request).is_err());
        assert_eq!(writes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn locking_during_native_read_discards_the_late_key() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("vault"), vec![7; 32]).unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let reads = files.reads.clone();
        let (release, gate) = mpsc::channel();
        *files.read_gate.lock() = Some(gate);
        let mut scenario = Scenario::new(files);
        let port = scenario.caller();
        let reply = scenario.request(port, |p| p.load("vault"));
        scenario.entered(&reads);
        let owner = scenario.owner();
        scenario.app.world_mut().write_message(LockVault(owner));
        assert!(scenario.result(&reply).is_err());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while scenario.app.world().get::<KeyLoading>(owner).is_some() {
            scenario.app.update();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        let port = scenario.caller();
        let reply = scenario.request(port, |p| p.observe("vault"));
        assert!(scenario.result(&reply).is_err());
        assert!(scenario.app.world().get::<UnlockedKey>(owner).is_none());
    }

    #[test]
    fn a_fresh_manifest_identity_cannot_receive_an_old_native_completion() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("vault"), vec![7; 32]).unwrap();
        std::fs::write(temp.path().join("replacement"), vec![8; 32]).unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let reads = files.reads.clone();
        let (release, gate) = mpsc::channel();
        *files.read_gate.lock() = Some(gate);
        let mut scenario = Scenario::new(files);
        let port = scenario.caller();
        let old = scenario.request(port, |p| p.load("vault"));
        scenario.entered(&reads);
        scenario
            .app
            .world_mut()
            .entity_mut(scenario.namespace)
            .insert(KeyManifest("replacement".into()));
        assert!(scenario.result(&old).is_err());
        let port = scenario.caller();
        let current = scenario.request(port, |p| p.load("replacement"));
        assert_eq!(*scenario.result(&current).unwrap(), vec![8; 32]);
        release.send(()).unwrap();
        for _ in 0..20 {
            scenario.app.update();
            std::thread::sleep(Duration::from_millis(1));
        }
        let owner = scenario.owner();
        assert_eq!(
            scenario.app.world().get::<VaultId>(owner).unwrap().0,
            "replacement"
        );
        let port = scenario.caller();
        let current = scenario.request(port, |p| p.observe("replacement"));
        assert_eq!(*scenario.result(&current).unwrap(), vec![8; 32]);
    }

    #[test]
    fn lock_cutoff_refuses_pre_lock_port_even_after_later_unlock() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("vault"), vec![7; 32]).unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let mut scenario = Scenario::new(files);
        let first = scenario.caller();
        let pre_lock_idle = scenario.caller();
        let reply = scenario.request(first, |p| p.load("vault"));
        assert_eq!(*scenario.result(&reply).unwrap(), vec![7; 32]);
        let owner = scenario.owner();
        scenario.app.world_mut().write_message(LockVault(owner));
        scenario.app.update();
        let post_lock = scenario.caller();
        let unlocked = scenario.request(post_lock, |p| p.load("vault"));
        assert_eq!(*scenario.result(&unlocked).unwrap(), vec![7; 32]);
        let stale = scenario.request(pre_lock_idle, |p| p.load("vault"));
        assert!(scenario.result(&stale).is_err());
    }

    #[test]
    fn fresh_load_after_lock_waits_for_stale_read_drain_and_succeeds() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("vault"), vec![7; 32]).unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let reads = files.reads.clone();
        let (release, gate) = mpsc::channel();
        *files.read_gate.lock() = Some(gate);
        let mut scenario = Scenario::new(files);
        let old_port = scenario.caller();
        let old_reply = scenario.request(old_port, |p| p.load("vault"));
        scenario.entered(&reads);
        let owner = scenario.owner();
        scenario.app.world_mut().write_message(LockVault(owner));
        assert!(scenario.result(&old_reply).is_err());
        let fresh_port = scenario.caller();
        let fresh_reply = scenario.request(fresh_port, |p| p.load("vault"));
        for _ in 0..3 {
            scenario.app.update();
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        release.send(()).unwrap();
        assert_eq!(*scenario.result(&fresh_reply).unwrap(), vec![7; 32]);
        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn canceled_creator_with_load_only_waiter_refuses_native_write() {
        let temp = tempfile::tempdir().unwrap();
        let files = Arc::new(KeyFiles::new(temp.path().to_path_buf()));
        let generates = files.generates.clone();
        let writes = files.writes.clone();
        let (release, gate) = mpsc::channel();
        *files.generate_gate.lock() = Some(gate);
        let mut scenario = Scenario::new(files);
        let creator = scenario.caller();
        let creator_op = creator.origin.operation;
        let create_req = scenario.create_request(creator.origin, "vault");
        scenario.entered(&generates);
        let reader = scenario.caller();
        scenario
            .app
            .world_mut()
            .get_mut::<KeyPermissions>(reader.origin.operation)
            .unwrap()
            .create = false;
        scenario
            .app
            .world_mut()
            .get_mut::<KeyPermissions>(reader.origin.operation)
            .unwrap()
            .store = false;
        let load_reply = scenario.request(reader, |p| p.load("vault"));
        for _ in 0..3 {
            scenario.app.update();
        }
        scenario
            .app
            .world()
            .get::<KeyCanceled>(creator_op)
            .unwrap()
            .0
            .store(true, Ordering::Release);
        release.send(()).unwrap();
        assert!(scenario.ecs_result(create_req).is_err());
        assert!(scenario.result(&load_reply).is_err());
        assert_eq!(writes.load(Ordering::SeqCst), 0);
        assert!(!temp.path().join("vault").exists());
    }
}
