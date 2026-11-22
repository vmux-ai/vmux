use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use vmux_ecs::{Cwd, EntityTarget, Operation, OperationRetained, OperationTarget};
use zeroize::Zeroizing;

use crate::storage::VaultStorage;
use crate::storage::keys::{LOCKED, NativeKeyAccess};

use super::key_io::{GenerateTask, NativeCall, ObserveTask, ReadTask, StoreTask};
pub(crate) use crate::storage::keys::port::KeyPort;
use crate::storage::keys::port::{KeyIngress, KeyOrigin};

pub(super) struct KeyAuthorityPlugin;

impl Plugin for KeyAuthorityPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LockVault>().add_systems(
            Update,
            (
                lock,
                invalidate,
                receive_load,
                receive_observe,
                receive_create,
                receive_store,
                materialize,
                admit_reads,
                admit_creates,
                admit_stores,
                submit_read,
                submit_observe,
                submit_generate,
                submit_store,
                drain_read,
                drain_observe,
                drain_generate,
                drain_store,
                respond,
                cleanup,
            )
                .chain()
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

#[derive(Component, Default)]
pub(crate) struct NamespaceGeneration(pub(crate) u64);

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
struct VaultKey;

#[derive(Component)]
struct VaultId(String);

#[derive(Component)]
struct KeyGeneration(u64);

#[derive(Component)]
struct AcceptedNamespaceGeneration(u64);

#[derive(Component)]
struct UnlockedKey(Zeroizing<Vec<u8>>);

#[derive(Component)]
struct LockedSince(u64);

#[derive(Message)]
pub struct LockVault(pub Entity);

#[derive(Component)]
struct KeyRequester(KeyOrigin);

impl KeyOrigin {
    fn valid(&self, world: &World) -> bool {
        let Some(port) = world.get::<KeyPort>(self.operation) else {
            return false;
        };
        port.origin == *self
            && world
                .get::<NamespaceGeneration>(self.namespace)
                .is_some_and(|g| g.0 == self.generation)
            && world.get::<KeyNamespace>(self.namespace).is_some()
            && !world
                .get::<KeyCanceled>(self.operation)
                .is_some_and(|c| c.0.load(Ordering::Acquire))
    }
}

impl KeyRequester {
    fn valid(&self, world: &World, request: Entity) -> bool {
        if !self.0.valid(world) {
            return false;
        }
        let Some(permissions) = world.get::<KeyPermissions>(self.0.operation) else {
            return false;
        };
        let allowed = (world.get::<LoadKey>(request).is_some() && permissions.load)
            || (world.get::<ObserveKey>(request).is_some() && permissions.observe)
            || (world.get::<CreateKey>(request).is_some() && permissions.create)
            || (world.get::<StoreReply>(request).is_some() && permissions.store);
        if !allowed {
            return false;
        }
        if let Some(target) = world.get::<EntityTarget<VaultKey>>(request) {
            if world.get::<KeyGeneration>(target.entity()).is_none() {
                return false;
            }
            if world
                .get::<LockedSince>(target.entity())
                .is_some_and(|l| self.0.admission <= l.0)
            {
                return false;
            }
        }
        true
    }
}

#[derive(Component)]
struct KeyReply(mpsc::Sender<Result<Zeroizing<Vec<u8>>, String>>);

#[derive(Component)]
struct StoreReply(mpsc::Sender<Result<(), String>>);

#[derive(Component)]
struct LoadKey;

#[derive(Component)]
struct ObserveKey;

#[derive(Component)]
struct CreateKey;

#[derive(Component)]
struct StoreKey(Zeroizing<Vec<u8>>);

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
    fn current(&self, world: &World) -> bool {
        world
            .get::<NamespaceGeneration>(self.namespace)
            .is_some_and(|g| g.0 == self.namespace_generation)
            && world.get::<KeyNamespace>(self.namespace).is_some()
            && world
                .get::<KeyGeneration>(self.owner)
                .is_some_and(|g| g.0 == self.generation)
    }

    fn accepted(&self, world: &mut World, job: Entity) -> bool {
        if !self.current(world) {
            return false;
        }
        let mut query = world.query::<(Entity, &KeyWaiting, &KeyRequester)>();
        for (entity, waiting, requester) in query.iter(world) {
            if waiting.0 == job && requester.valid(world, entity) {
                return true;
            }
        }
        false
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

fn lock(world: &mut World) {
    let requests: Vec<_> = world
        .resource_mut::<Messages<LockVault>>()
        .drain()
        .collect();
    for LockVault(owner) in requests {
        let Some(target) = world.get::<EntityTarget<KeyNamespace>>(owner) else {
            continue;
        };
        let admission = world
            .get::<KeyAdmission>(target.entity())
            .map_or(0, |a| a.0);
        let Some(mut generation) = world.get_mut::<KeyGeneration>(owner) else {
            continue;
        };
        generation.0 = generation.0.wrapping_add(1);
        world
            .entity_mut(owner)
            .remove::<UnlockedKey>()
            .insert(LockedSince(admission));
    }
}

fn invalidate(world: &mut World) {
    let mut namespaces = world.query::<(Entity, Ref<KeyNamespace>)>();
    let changed: Vec<_> = namespaces
        .iter(world)
        .filter(|(_, n)| n.is_changed() && !n.is_added())
        .map(|(e, _)| e)
        .collect();
    for entity in changed {
        if let Some(mut generation) = world.get_mut::<NamespaceGeneration>(entity) {
            generation.0 = generation.0.wrapping_add(1);
        }
    }
    let mut manifests = world.query::<(Entity, Ref<KeyManifest>)>();
    let replaced: Vec<_> = manifests
        .iter(world)
        .filter(|(_, m)| m.is_changed())
        .map(|(e, m)| (e, m.0.clone()))
        .collect();
    for (namespace, vault_id) in replaced {
        let mut keys = world
            .query_filtered::<(Entity, &EntityTarget<KeyNamespace>, &VaultId), With<VaultKey>>();
        let old: Vec<_> = keys
            .iter(world)
            .filter(|(_, t, id)| t.entity() == namespace && id.0 != vault_id)
            .map(|(e, _, _)| e)
            .collect();
        for entity in old {
            world.despawn(entity);
        }
    }
    let mut owners = world.query::<(
        Entity,
        &EntityTarget<KeyNamespace>,
        &AcceptedNamespaceGeneration,
    )>();
    let retired: Vec<_> = owners
        .iter(world)
        .filter(|(_, target, generation)| {
            world.get::<KeyNamespace>(target.entity()).is_none()
                || world
                    .get::<NamespaceGeneration>(target.entity())
                    .is_none_or(|g| g.0 != generation.0)
        })
        .map(|(e, _, _)| e)
        .collect();
    for entity in retired {
        world.despawn(entity);
    }
    let mut ports = world.query::<(&KeyPort, &KeyCanceled)>();
    for (port, canceled) in ports.iter(world) {
        if !port.origin.valid(world) {
            canceled.0.store(true, Ordering::Release);
        }
    }
    let mut requests = world.query::<(Entity, &KeyRequester)>();
    let refused: Vec<_> = requests
        .iter(world)
        .filter(|(e, r)| !r.valid(world, *e))
        .map(|(e, _)| e)
        .collect();
    for entity in refused {
        world
            .entity_mut(entity)
            .insert(KeyResult(Err("Vault key operation retired".to_string())));
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

fn receive_create(ingress: Query<&KeyIngress>, mut commands: Commands) {
    for ingress in &ingress {
        for request in ingress.create.lock().try_iter() {
            commands.spawn((
                Operation::bundle("vault.key.create").unwrap().running(),
                OperationRetained,
                KeyRequester(request.origin),
                VaultId(request.vault_id),
                KeyReply(request.reply),
                CreateKey,
            ));
        }
    }
}

fn receive_store(ingress: Query<&KeyIngress>, mut commands: Commands) {
    for ingress in &ingress {
        for request in ingress.store.lock().try_iter() {
            commands.spawn((
                Operation::bundle("vault.key.store").unwrap().running(),
                OperationRetained,
                KeyRequester(request.origin),
                VaultId(request.vault_id),
                StoreReply(request.reply),
                StoreKey(request.key),
            ));
        }
    }
}

fn materialize(world: &mut World) {
    let mut query = world
        .query_filtered::<(Entity, &KeyRequester, &VaultId), Without<EntityTarget<VaultKey>>>();
    let requests: Vec<_> = query
        .iter(world)
        .map(|(e, r, id)| (e, r.0, id.0.clone()))
        .collect();
    for (entity, origin, vault_id) in requests {
        let permissions = world.get::<KeyPermissions>(origin.operation);
        let allowed = permissions.is_some_and(|p| {
            (world.get::<LoadKey>(entity).is_some() && p.load)
                || (world.get::<ObserveKey>(entity).is_some() && p.observe)
                || (world.get::<CreateKey>(entity).is_some() && p.create)
                || (world.get::<StoreKey>(entity).is_some() && p.store)
        });
        if !allowed || !origin.valid(world) || vault_id.is_empty() {
            world
                .entity_mut(entity)
                .insert(KeyResult(Err("Vault key request refused".to_string())));
            continue;
        }
        let mut owners = world
            .query_filtered::<(Entity, &VaultId, &EntityTarget<KeyNamespace>), With<VaultKey>>();
        let existing = owners
            .iter(world)
            .find(|(_, id, target)| id.0 == vault_id && target.entity() == origin.namespace)
            .map(|(e, _, _)| e);
        let owner = match existing {
            Some(owner) => owner,
            None => {
                let cwd = world
                    .get::<Cwd>(origin.namespace)
                    .map(|c| c.0.clone())
                    .unwrap_or_default();
                let repository = world
                    .get::<RepositoryDirectory>(origin.namespace)
                    .map(|r| r.0.clone())
                    .unwrap_or_default();
                world
                    .spawn((
                        Name::new("Vault key"),
                        VaultKey,
                        VaultId(vault_id),
                        Cwd(cwd),
                        RepositoryDirectory(repository),
                        EntityTarget::<KeyNamespace>::new(origin.namespace),
                        AcceptedNamespaceGeneration(origin.generation),
                        KeyGeneration(0),
                    ))
                    .id()
            }
        };
        if world
            .get::<LockedSince>(owner)
            .is_some_and(|l| origin.admission <= l.0)
        {
            world
                .entity_mut(entity)
                .insert(KeyResult(Err(LOCKED.to_string())));
            continue;
        }
        world
            .entity_mut(entity)
            .insert(EntityTarget::<VaultKey>::new(owner));
    }
}

fn admit_reads(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &KeyRequester, &EntityTarget<VaultKey>), (Without<KeyWaiting>, Without<KeyResult>)>();
    let requests: Vec<_> = query
        .iter(world)
        .filter(|(e, _, _)| {
            world.get::<LoadKey>(*e).is_some() || world.get::<ObserveKey>(*e).is_some()
        })
        .map(|(e, r, t)| (e, r.0, t.entity()))
        .collect();
    for (entity, origin, owner) in requests {
        if let Some(key) = world.get::<UnlockedKey>(owner) {
            let result = Zeroizing::new(key.0.to_vec());
            world.entity_mut(entity).insert(KeyResult(Ok(result)));
            continue;
        }
        let observing = world.get::<ObserveKey>(entity).is_some();
        if observing && world.get::<LockedSince>(owner).is_some() {
            world.entity_mut(entity).insert(KeyResult::locked());
            continue;
        }
        if let Some(loading) = world.get::<KeyLoading>(owner) {
            let job = loading.0;
            world.entity_mut(entity).insert(KeyWaiting(job));
            continue;
        }
        let Some(io) = KeyIo::capture(world, origin, owner) else {
            continue;
        };
        if io.claimed(world) {
            continue;
        }
        let job = world
            .spawn((
                Operation::bundle("vault.key.read").unwrap().running(),
                OperationRetained,
                OperationTarget::new(owner),
                io,
            ))
            .id();
        if observing {
            world.entity_mut(job).insert(ObserveReady);
        } else {
            world.entity_mut(job).insert(ReadReady);
        }
        world.entity_mut(owner).insert(KeyLoading(job));
        world.entity_mut(entity).insert(KeyWaiting(job));
    }
}

impl KeyIo {
    fn capture(world: &World, origin: KeyOrigin, owner: Entity) -> Option<Self> {
        let access = world.get::<KeyNamespace>(origin.namespace)?.0.clone();
        let vault_id = world.get::<VaultId>(owner)?.0.clone();
        Some(Self {
            namespace: origin.namespace,
            namespace_generation: origin.generation,
            owner,
            generation: world.get::<KeyGeneration>(owner)?.0,
            path: access.path(&vault_id),
            vault_id,
            access,
            wake: world.get::<KeyPort>(origin.operation)?.wake.clone(),
        })
    }

    fn claimed(&self, world: &mut World) -> bool {
        let mut query = world.query::<&KeyIo>();
        query.iter(world).any(|io| io.path == self.path)
    }

    fn notify(&self) {
        if let Some(wake) = &self.wake {
            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
    }
}

fn admit_creates(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &KeyRequester, &EntityTarget<VaultKey>), (With<CreateKey>, Without<KeyWaiting>, Without<KeyResult>)>();
    let requests: Vec<_> = query
        .iter(world)
        .map(|(e, r, t)| (e, r.0, t.entity()))
        .collect();
    for (entity, origin, owner) in requests {
        if let Some(loading) = world.get::<KeyLoading>(owner) {
            if world.get::<GenerateReady>(loading.0).is_some()
                || world.get::<GenerateTask>(loading.0).is_some()
                || world.get::<StoreBuffer>(loading.0).is_some()
            {
                let job = loading.0;
                world.entity_mut(entity).insert(KeyWaiting(job));
            }
            continue;
        }
        let Some(mut io) = KeyIo::capture(world, origin, owner) else {
            continue;
        };
        if io.claimed(world) {
            continue;
        }
        let Some(mut generation) = world.get_mut::<KeyGeneration>(owner) else {
            continue;
        };
        generation.0 = generation.0.wrapping_add(1);
        io.generation = generation.0;
        world.entity_mut(owner).remove::<UnlockedKey>();
        let job = world
            .spawn((
                Operation::bundle("vault.key.generate").unwrap().running(),
                OperationRetained,
                OperationTarget::new(owner),
                io,
                GenerateReady,
            ))
            .id();
        world.entity_mut(owner).insert(KeyLoading(job));
        world.entity_mut(entity).insert(KeyWaiting(job));
    }
}

fn admit_stores(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &KeyRequester, &EntityTarget<VaultKey>), (With<StoreKey>, Without<KeyWaiting>, Without<KeyResult>)>();
    let requests: Vec<_> = query
        .iter(world)
        .map(|(e, r, t)| (e, r.0, t.entity()))
        .collect();
    for (entity, origin, owner) in requests {
        let Some(mut io) = KeyIo::capture(world, origin, owner) else {
            continue;
        };
        if io.claimed(world) {
            continue;
        }
        let Some(key) = world.entity_mut(entity).take::<StoreKey>() else {
            continue;
        };
        let Some(mut generation) = world.get_mut::<KeyGeneration>(owner) else {
            continue;
        };
        generation.0 = generation.0.wrapping_add(1);
        io.generation = generation.0;
        world.entity_mut(owner).remove::<UnlockedKey>();
        let job = world
            .spawn((
                Operation::bundle("vault.key.write").unwrap().running(),
                OperationRetained,
                OperationTarget::new(owner),
                io,
                StoreBuffer(key.0),
                StoreReady,
            ))
            .id();
        world.entity_mut(owner).insert(KeyLoading(job));
        world.entity_mut(entity).insert(KeyWaiting(job));
    }
}

fn submit_read(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &KeyIo), With<ReadReady>>();
    let jobs: Vec<_> = query.iter(world).map(|(e, io)| (e, io.clone())).collect();
    for (entity, io) in jobs {
        world.entity_mut(entity).remove::<ReadReady>();
        if !io.accepted(world, entity) {
            world
                .entity_mut(entity)
                .insert(KeyResult(Err("Vault key operation retired".to_string())));
            continue;
        }
        let wake = io.wake.clone();
        let call = NativeCall::spawn(wake, move || io.access.read(&io.vault_id));
        world.entity_mut(entity).insert(ReadTask(call));
    }
}

fn submit_observe(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &KeyIo), With<ObserveReady>>();
    let jobs: Vec<_> = query.iter(world).map(|(e, io)| (e, io.clone())).collect();
    for (entity, io) in jobs {
        world.entity_mut(entity).remove::<ObserveReady>();
        if !io.accepted(world, entity) {
            world
                .entity_mut(entity)
                .insert(KeyResult(Err("Vault key operation retired".to_string())));
            continue;
        }
        let wake = io.wake.clone();
        let call = NativeCall::spawn(wake, move || io.access.observe(&io.vault_id));
        world.entity_mut(entity).insert(ObserveTask(call));
    }
}

fn submit_generate(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &KeyIo), With<GenerateReady>>();
    let jobs: Vec<_> = query.iter(world).map(|(e, io)| (e, io.clone())).collect();
    for (entity, io) in jobs {
        world.entity_mut(entity).remove::<GenerateReady>();
        if !io.accepted(world, entity) {
            world
                .entity_mut(entity)
                .insert(KeyResult(Err("Vault key operation retired".to_string())));
            continue;
        }
        let wake = io.wake.clone();
        let call = NativeCall::spawn(wake, move || io.access.generate());
        world.entity_mut(entity).insert(GenerateTask(call));
    }
}

fn submit_store(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &KeyIo), With<StoreReady>>();
    let jobs: Vec<_> = query.iter(world).map(|(e, io)| (e, io.clone())).collect();
    for (entity, io) in jobs {
        world.entity_mut(entity).remove::<StoreReady>();
        if !io.accepted(world, entity) {
            world
                .entity_mut(entity)
                .insert(KeyResult(Err("Vault key operation retired".to_string())));
            continue;
        }
        let Some(buffer) = world.get::<StoreBuffer>(entity) else {
            continue;
        };
        let key = Zeroizing::new(buffer.0.to_vec());
        let wake = io.wake.clone();
        let call = NativeCall::spawn(wake, move || io.access.write(&io.vault_id, &key));
        world.entity_mut(entity).insert(StoreTask(call));
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

fn drain_observe(world: &mut World) {
    let mut query = world.query::<(Entity, &mut ObserveTask)>();
    let mut results = Vec::new();
    for (entity, mut task) in query.iter_mut(world) {
        if let Some(result) = task.0.poll() {
            results.push((entity, result));
        }
    }
    for (entity, result) in results {
        world.entity_mut(entity).remove::<ObserveTask>();
        let result = match result {
            Ok(Some(key)) => Ok(key),
            Ok(None) => Err(LOCKED.to_string()),
            Err(e) => Err(e),
        };
        if result.is_err() {
            let mut waiting = world.query_filtered::<(Entity, &KeyWaiting), With<LoadKey>>();
            let retry: Vec<_> = waiting
                .iter(world)
                .filter(|(_, w)| w.0 == entity)
                .map(|(e, _)| e)
                .collect();
            for request in retry {
                world.entity_mut(request).remove::<KeyWaiting>();
                if let Some(io) = world.get::<KeyIo>(entity) {
                    io.notify();
                }
            }
        }
        world.entity_mut(entity).insert(KeyResult(result));
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

fn drain_store(world: &mut World) {
    let mut query = world.query::<(Entity, &mut StoreTask)>();
    let mut results = Vec::new();
    for (entity, mut task) in query.iter_mut(world) {
        if let Some(result) = task.0.poll() {
            results.push((entity, result));
        }
    }
    for (entity, result) in results {
        world.entity_mut(entity).remove::<StoreTask>();
        let result = match result {
            Ok(receipt) => {
                let io = world.get::<KeyIo>(entity).cloned();
                if receipt.created && io.is_some_and(|io| !io.current(world)) {
                    warn!("Retired Vault key creation retained its durable file");
                }
                match world.entity_mut(entity).take::<StoreBuffer>() {
                    Some(buffer) => Ok(buffer.0),
                    None => Err("Vault key write lost its accepted input".to_string()),
                }
            }
            Err(error) => Err(error),
        };
        world.entity_mut(entity).insert(KeyResult(result));
    }
}

fn respond(world: &mut World) {
    let mut query = world.query::<(Entity, &KeyIo)>();
    let completed: Vec<_> = query
        .iter(world)
        .filter(|(e, _)| world.get::<KeyResult>(*e).is_some())
        .map(|(e, io)| (e, io.clone()))
        .collect();
    for (job, io) in completed {
        let Some(result) = world.entity_mut(job).take::<KeyResult>() else {
            continue;
        };
        let active = io.accepted(world, job);
        let result = if active {
            result.0
        } else {
            Err("Vault key operation retired".to_string())
        };
        if let Ok(key) = &result {
            world
                .entity_mut(io.owner)
                .remove::<LockedSince>()
                .insert(UnlockedKey(Zeroizing::new(key.to_vec())));
        }
        if world
            .get::<KeyLoading>(io.owner)
            .is_some_and(|l| l.0 == job)
        {
            world.entity_mut(io.owner).remove::<KeyLoading>();
        }
        let mut query = world.query::<(Entity, &KeyWaiting)>();
        let waiting: Vec<_> = query
            .iter(world)
            .filter(|(_, w)| w.0 == job)
            .map(|(e, _)| e)
            .collect();
        for entity in waiting {
            let reply = match &result {
                Ok(key) => Ok(Zeroizing::new(key.to_vec())),
                Err(error) => Err(error.clone()),
            };
            world.entity_mut(entity).insert(KeyResult(reply));
        }
        world.despawn(job);
        let mut pending = world
            .query_filtered::<Entity, (With<KeyRequester>, Without<KeyWaiting>, Without<KeyResult>)>(
            );
        if pending.iter(world).next().is_some() {
            io.notify();
        }
    }
    let mut query = world.query_filtered::<Entity, (With<KeyResult>, Without<KeyIo>)>();
    let finished: Vec<_> = query.iter(world).collect();
    for entity in finished {
        let Some(result) = world.entity_mut(entity).take::<KeyResult>() else {
            continue;
        };
        let valid = world
            .get::<KeyRequester>(entity)
            .is_some_and(|r| r.valid(world, entity));
        let result = if valid {
            result.0
        } else {
            Err("Vault key operation retired".to_string())
        };
        if let Some(reply) = world.entity_mut(entity).take::<KeyReply>() {
            let _ = reply.0.send(result);
        } else if let Some(reply) = world.entity_mut(entity).take::<StoreReply>() {
            let _ = reply.0.send(result.map(|_| ()));
        }
        world.despawn(entity);
    }
}

fn cleanup(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &KeyWaiting), With<KeyRequester>>();
    let abandoned: Vec<_> = query
        .iter(world)
        .filter(|(_, w)| world.get::<KeyIo>(w.0).is_none())
        .map(|(e, _)| e)
        .collect();
    for entity in abandoned {
        world
            .entity_mut(entity)
            .insert(KeyResult(Err("Vault key operation retired".to_string())));
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
        read_gate: Mutex<Option<mpsc::Receiver<()>>>,
        write_gate: Mutex<Option<mpsc::Receiver<()>>>,
        fail_write: AtomicBool,
    }

    impl KeyFiles {
        fn new(directory: PathBuf) -> Self {
            Self {
                directory,
                reads: Arc::new(AtomicUsize::new(0)),
                writes: Arc::new(AtomicUsize::new(0)),
                read_gate: Mutex::new(None),
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
                    NamespaceGeneration::default(),
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
            let mut query =
                world.query::<(&KeyNamespace, &NamespaceGeneration, &mut KeyAdmission)>();
            let (access, generation, mut admission) = query.get_mut(world, self.namespace).unwrap();
            let (port, ingress) = access.port(
                self.namespace,
                generation.0,
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
        let reply = scenario.request(port, |p| p.create("vault"));
        assert!(scenario.result(&reply).is_err());
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
        let reply = scenario.request(port, |p| p.create("vault"));
        scenario.entered(&writes);
        let owner = scenario.owner();
        assert!(scenario.app.world().get::<UnlockedKey>(owner).is_none());
        release.send(()).unwrap();
        let key = scenario.result(&reply).unwrap();
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
            .get_mut::<NamespaceGeneration>(scenario.namespace)
            .unwrap()
            .0 += 1;
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
        let reply = scenario.request(port, |p| p.store("vault", &[8; 32]));
        assert!(scenario.result(&reply).is_err());
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
}
