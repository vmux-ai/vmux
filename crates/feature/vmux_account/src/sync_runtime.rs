use std::path::PathBuf;
use std::time::Duration;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryData;
use bevy_ecs::system::SystemParam;
use uuid::Uuid;
use vmux_ecs::record::{Deleted, Record};
use vmux_ecs::{JobWake, Operation, Revision, Uuid as Id};
use vmux_feature::{Current, Definition};
use vmux_job::{Job, JobOwner, JobPlugin, JobRequest, JobResult};

use crate::cloud::transport::Reply;
use crate::mutations::{
    ImportProfileRequest, MutationCompleted, NoUpload, RemoteRenameRequest, Target, WriteRequest,
};
use crate::sync::{Checkpoint, CheckpointChange, ExpectedScope, Origin, Pending};
use crate::sync::{ListRequest, UploadRequest};
use crate::sync_store::{Loaded, Store};
use crate::wire::{ProfileChange, Response};
use crate::{
    Account, Binding, CloudAvailable, Connection, Location, Owner, Phase, Profile, Root, Set,
    SyncEnabled,
};

pub(crate) struct ProfileSyncPlugin;

impl bevy_app::Plugin for ProfileSyncPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_feature::FeaturePlugin>() {
            app.add_plugins(vmux_feature::FeaturePlugin);
        }
        app.add_plugins((
            JobPlugin::<Load>::default(),
            JobPlugin::<Pause>::default(),
            JobPlugin::<Seal>::default(),
            JobPlugin::<Acknowledge>::default(),
            JobPlugin::<Replace>::default(),
            JobPlugin::<SaveConflict>::default(),
        ))
        .add_observer(wake::<Load>)
        .add_observer(wake::<Pause>)
        .add_observer(wake::<Seal>)
        .add_observer(wake::<Acknowledge>)
        .add_observer(wake::<Replace>)
        .add_observer(wake::<SaveConflict>)
        .add_message::<ImportRequest>()
        .add_message::<KeepLocalRequest>()
        .add_message::<UseRemoteRequest>()
        .add_systems(Update, define.before(vmux_feature::Set::Accept))
        .add_systems(Update, outbox.after(Set::Write).before(Set::Dispatch))
        .add_systems(
            Update,
            (
                (
                    resume,
                    reset,
                    receive_ack,
                    receive_conflict,
                    retry_conflict,
                    resolve_local,
                    resolve_remote,
                    import_remote,
                    receive_resolution,
                    finish_adoption,
                )
                    .chain(),
                (
                    refresh,
                    load,
                    receive_load,
                    restore_outbound,
                    restore_checkpoints,
                    finish_load,
                )
                    .chain(),
                (
                    seed,
                    enumerate,
                    receive_list,
                    reconcile,
                    seal,
                    receive_seal,
                    receive_upload,
                    delay,
                )
                    .chain(),
            )
                .chain()
                .after(Set::Publish)
                .after(vmux_feature::Set::Publish),
        );
    }
}

#[derive(Component)]
pub struct ProfileSyncFlag;

#[derive(Component, Clone, Copy, Default)]
pub struct RemoteRevision(pub u64);

#[derive(Component, Clone, Copy, Default)]
pub struct SyncedRevision(pub u64);

#[derive(Component)]
pub struct RemoteDeleted;

#[derive(Component)]
pub struct RemoteProfile;

#[derive(Component)]
pub struct Conflict(pub Option<Record>);

#[derive(Message)]
pub struct ImportRequest {
    pub root: Entity,
    pub profile_id: Uuid,
    pub binding_revision: u64,
    pub remote_revision: u64,
}

#[derive(Message)]
pub struct KeepLocalRequest {
    pub root: Entity,
    pub profile_id: Uuid,
    pub binding_revision: u64,
    pub local_revision: u64,
    pub remote_revision: u64,
    pub conflict_id: Uuid,
}

#[derive(Message)]
pub struct UseRemoteRequest {
    pub root: Entity,
    pub profile_id: Uuid,
    pub binding_revision: u64,
    pub local_revision: u64,
    pub remote_revision: u64,
    pub conflict_id: Uuid,
}

#[derive(Component, Clone)]
struct Pause {
    duration: Duration,
}

impl Job for Pause {
    type Output = ();
    fn run(self) {
        std::thread::sleep(self.duration);
    }
}

#[derive(Component, Default)]
struct Attempts(u32);

impl Attempts {
    fn delay(&self) -> Duration {
        Duration::from_millis(250u64.saturating_mul(1u64 << self.0.min(3)))
    }
}

fn wake<J: Job>(_input: On<Add, JobRequest<J>>, wake: Option<Res<JobWake>>) {
    if let Some(wake) = wake {
        wake.0.wake_by_ref();
    }
}

fn delay(pending: Query<(Entity, &Pause), Without<Delayed>>, mut commands: Commands) {
    for (target, pause) in &pending {
        commands.spawn((
            Operation::bundle("account.profile-delay").unwrap(),
            ChildOf(target),
            JobRequest(pause.clone()),
        ));
        commands.entity(target).insert(Delayed).remove::<Pause>();
    }
}

fn resume(mut results: Results<Pause>, mut commands: Commands) {
    for (entity, _, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        if let Some(parent) = parent {
            commands
                .entity(parent.parent())
                .remove::<Delayed>()
                .remove::<Enumerated>();
        }
        commands.entity(entity).despawn();
    }
}

#[derive(Component)]
struct Adopting(Uuid);

#[derive(Component)]
struct Delayed;

#[derive(Component)]
struct OutboxLoading;

#[derive(Component, Clone, Copy)]
struct Inventory {
    id: Uuid,
    after: u32,
    through: Option<u32>,
}

#[derive(Component)]
struct Seen(Uuid);

#[derive(Component)]
struct Refresh;

#[derive(Component)]
struct Seed;

#[derive(Component)]
pub(crate) struct Outbound;

#[derive(Component)]
struct BaseRevision(Option<u64>);

#[derive(Component)]
struct SentOn {
    connection: Entity,
    binding_revision: u64,
}

#[derive(Component)]
struct Enumerated(Entity);

#[derive(Component)]
struct PageCursor(Option<Uuid>);

#[derive(Component)]
struct Reading;

#[derive(Component)]
struct Incoming(Record);

#[derive(Component)]
struct Sending;

#[derive(Component)]
struct Busy;

type MutationOwners<'w, 's> =
    Query<'w, 's, (&'static ChildOf, &'static Target), (With<Operation>, Without<Outbound>)>;

type BusyOwners<'w, 's> =
    Query<'w, 's, (&'static ChildOf, &'static Origin), (With<Outbound>, With<Busy>)>;

type ConflictOwners<'w, 's> = Query<
    'w,
    's,
    (&'static ChildOf, &'static Target, &'static Origin),
    (With<Outbound>, With<Conflict>),
>;

type LoadRoots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Location,
        Option<&'static Origin>,
        Has<Refresh>,
        Option<&'static Inventory>,
    ),
    (With<Root>, Without<OutboxLoading>, Without<Delayed>),
>;

type ReceiveLoadProfiles<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Id,
        &'static Owner,
        Option<&'static Origin>,
        Option<&'static RemoteRevision>,
        Option<&'static SyncedRevision>,
    ),
    With<Profile>,
>;

type EnumerateRoots<'w, 's> =
    Query<'w, 's, (Entity, Option<&'static Enumerated>), (With<Root>, Without<Delayed>)>;

type SeedRoots<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<Root>,
        With<Seed>,
        Without<OutboxLoading>,
        Without<Refresh>,
    ),
>;

type ReceiveListPending<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Origin,
        &'static SentOn,
        &'static mut PageCursor,
        &'static mut Reply,
    ),
    With<Reading>,
>;

type ReconcilePending<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Target,
        &'static Origin,
        &'static Id,
        Has<Busy>,
    ),
    With<Outbound>,
>;

type SealPending<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Id,
        &'static Target,
        &'static Origin,
        &'static vmux_ecs::Order,
    ),
    (
        With<Outbound>,
        Without<Busy>,
        Without<Conflict>,
        Without<Delayed>,
    ),
>;

type ReceiveUploadPending<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Id,
        &'static Target,
        &'static Origin,
        &'static SentOn,
        &'static BaseRevision,
        &'static mut Reply,
    ),
    With<Sending>,
>;

type ResolveLocalConflicts<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Target,
        &'static Origin,
        &'static Conflict,
        &'static Id,
        Has<Busy>,
    ),
    With<Outbound>,
>;

type ResolveRemoteConflicts<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Target,
        &'static Origin,
        &'static Conflict,
        &'static Id,
        Has<Busy>,
    ),
    With<Outbound>,
>;

type ImportRemoteRemote<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static Id,
        &'static Name,
        &'static Revision,
        &'static Origin,
    ),
    (With<RemoteProfile>, Without<Deleted>),
>;

type AdmissionRoots<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<Root>,
        With<CloudAvailable>,
        Without<crate::Disconnecting>,
    ),
>;

#[derive(SystemParam)]
pub(crate) struct Context<'w, 's> {
    accounts: Query<
        'w,
        's,
        (
            &'static ChildOf,
            &'static Id,
            &'static Binding,
            &'static Revision,
        ),
        With<Account>,
    >,
    connections: Query<'w, 's, ConnectedContext, With<Connection>>,
    roots: AdmissionRoots<'w, 's>,
    flags: Query<'w, 's, &'static Current, With<ProfileSyncFlag>>,
    profiles: Query<'w, 's, ProfileContext, (With<Profile>, Without<crate::mutations::Proposal>)>,
    dispatches: Query<'w, 's, DispatchContext, With<Outbound>>,
    selections:
        Query<'w, 's, (&'static ChildOf, &'static Target, &'static SyncEnabled), With<Operation>>,
}

#[derive(QueryData)]
struct ConnectedContext {
    entity: Entity,
    parent: &'static ChildOf,
    phase: &'static Phase,
    server: &'static crate::Server,
    account: &'static crate::connection::RemoteAccount,
    generation: &'static Revision,
}

#[derive(QueryData)]
struct ProfileContext {
    entity: Entity,
    id: &'static Id,
    owner: &'static Owner,
    enabled: Option<&'static SyncEnabled>,
    origin: Option<&'static Origin>,
    remote: Option<&'static RemoteRevision>,
    deleted: Has<RemoteDeleted>,
    name: &'static Name,
    revision: &'static Revision,
    synced: Option<&'static SyncedRevision>,
}

#[derive(QueryData)]
struct DispatchContext {
    parent: &'static ChildOf,
    id: &'static Id,
    target: &'static Target,
    origin: &'static Origin,
    sent: &'static SentOn,
    base: &'static BaseRevision,
    name: Option<&'static Name>,
    deleted: Has<Deleted>,
}

impl Context<'_, '_> {
    fn origin(&self, root: Entity) -> Option<(Origin, u64)> {
        for (owner, _, binding, revision) in &self.accounts {
            if owner.parent() == root && binding.is_linked() {
                let account = binding.account_id.as_deref()?.parse().ok()?;
                return Some((
                    Origin {
                        server: binding.server.clone(),
                        account,
                    },
                    revision.0,
                ));
            }
        }
        None
    }

    fn scope(&self, root: Entity) -> Option<ExpectedScope> {
        let (origin, revision) = self.origin(root)?;
        for (parent, id, _, _) in &self.accounts {
            if parent.parent() == root {
                return Some(ExpectedScope {
                    account_id: id.0.parse().ok()?,
                    binding_revision: revision,
                    origin,
                });
            }
        }
        None
    }

    pub(crate) fn accepts_scope(&self, root: Entity, scope: &ExpectedScope) -> bool {
        self.connection(root).is_some_and(|(_, origin, revision)| {
            origin == scope.origin && revision == scope.binding_revision
        }) && self.accounts.iter().any(|(parent, id, binding, revision)| {
            parent.parent() == root && scope.matches(id, Some(binding), revision.0)
        })
    }

    fn connection(&self, root: Entity) -> Option<(Entity, Origin, u64)> {
        if !self.roots.contains(root) || !self.flags.single().is_ok_and(|flag| flag.0) {
            return None;
        }
        let (origin, revision) = self.origin(root)?;
        for connection in &self.connections {
            if connection.parent.parent() == root
                && *connection.phase == Phase::Connected
                && connection.server.authority() == origin.server
                && connection.account.0 == origin.account
                && self.accounts.iter().any(|(parent, _, binding, _)| {
                    parent.parent() == root
                        && binding.credential_generation == connection.generation.0
                })
            {
                return Some((connection.entity, origin, revision));
            }
        }
        None
    }

    fn profile(&self, root: Entity, target: Uuid, origin: &Origin) -> Option<(Entity, u64)> {
        if self.selections.iter().any(|(parent, profile, enabled)| {
            parent.parent() == root && profile.0 == target && !enabled.0
        }) {
            return None;
        }
        for profile in &self.profiles {
            if profile.id.0 != target.to_string()
                || !profile.enabled.is_some_and(|enabled| enabled.0)
                || !self
                    .accounts
                    .get(profile.owner.0)
                    .is_ok_and(|(parent, _, _, _)| parent.parent() == root)
            {
                continue;
            }
            if profile.origin == Some(origin) {
                if profile.deleted {
                    return None;
                }
                return Some((profile.entity, profile.remote.map_or(0, |remote| remote.0)));
            }
            return Some((profile.entity, 0));
        }
        None
    }

    fn replica(
        &self,
        root: Entity,
        target: Uuid,
        origin: &Origin,
    ) -> Option<(Entity, String, u64, u64, u64)> {
        let (entity, remote) = self.profile(root, target, origin)?;
        let profile = self.profiles.get(entity).ok()?;
        let synced = if profile.origin == Some(origin) {
            profile.synced.map_or(0, |value| value.0)
        } else {
            0
        };
        Some((
            entity,
            profile.name.as_str().into(),
            profile.revision.0,
            synced,
            remote,
        ))
    }

    pub(crate) fn permits(
        &self,
        entity: Entity,
        connection: Entity,
        change: &ProfileChange,
    ) -> bool {
        let Ok(pending) = self.dispatches.get(entity) else {
            return false;
        };
        let root = pending.parent.parent();
        if pending.id.0 != change.operation_id.to_string()
            || pending.target.0 != change.id
            || pending.base.0 != Some(change.base_revision)
            || pending.sent.connection != connection
            || pending.deleted != change.name.is_none()
            || !pending.deleted && pending.name.map(|name| name.as_str()) != change.name.as_deref()
        {
            return false;
        }
        self.connection(root)
            .is_some_and(|(current, origin, revision)| {
                current == connection
                    && origin == *pending.origin
                    && revision == pending.sent.binding_revision
                    && self.profile(root, change.id, &origin).is_some()
            })
    }
}

fn reset(
    roots: Query<Entity, With<Root>>,
    context: Context,
    profiles: Query<(Entity, &Owner, &Origin), With<Profile>>,
    accounts: Query<&ChildOf, With<Account>>,
    mut commands: Commands,
) {
    for root in &roots {
        let current = context.origin(root);
        for (entity, owner, origin) in &profiles {
            if !accounts
                .get(owner.0)
                .is_ok_and(|parent| parent.parent() == root)
                || current
                    .as_ref()
                    .is_some_and(|(current, _)| current == origin)
            {
                continue;
            }
            commands
                .entity(entity)
                .remove::<Origin>()
                .remove::<RemoteRevision>()
                .remove::<SyncedRevision>()
                .remove::<RemoteDeleted>();
        }
        if current.is_none() {
            commands.entity(root).remove::<Origin>();
        }
    }
}

fn define(
    providers: Query<Entity, With<vmux_feature::ReleaseProvider>>,
    scopes: Query<Entity, With<vmux_feature::InstallationScope>>,
    sources: Query<
        (Entity, &vmux_feature::manifest::FeatureManifest),
        With<vmux_feature::manifest::FeatureManifestOwner<crate::Feature>>,
    >,
    flags: Query<(), With<ProfileSyncFlag>>,
    mut commands: Commands,
) {
    if !flags.is_empty() {
        return;
    }
    let Ok(provider) = providers.single() else {
        return;
    };
    let Ok(scope) = scopes.single() else {
        return;
    };
    let Ok((source, manifest)) = sources.single() else {
        return;
    };
    let (index, declaration) = manifest
        .flag("profile.sync")
        .expect("Profile sync declaration");
    commands.spawn((
        ProfileSyncFlag,
        Definition::bundle(provider, source, index, scope, declaration),
    ));
}

fn outbox(
    mut requests: MessageMutator<WriteRequest>,
    context: Context,
    profiles: Query<(&Id, &Owner, Option<&SyncEnabled>), With<Profile>>,
    accounts: Query<&ChildOf, With<Account>>,
    excluded: Query<(), With<NoUpload>>,
) {
    for request in requests.read() {
        if excluded.contains(request.entity) {
            continue;
        }
        let Some((origin, _)) = context.origin(request.root) else {
            continue;
        };
        let Some(record) = request
            .records
            .iter()
            .find(|record| record.id == request.target_id)
        else {
            continue;
        };
        if !record
            .components
            .iter()
            .any(|component| component.key == "profile")
        {
            continue;
        }
        let enabled = record
            .components
            .iter()
            .find(|component| component.key == "profile_sync")
            .and_then(|component| component.value.as_bool());
        let mut consent = enabled.unwrap_or(false);
        for (id, owner, sync) in &profiles {
            if id.0 == request.target_id.to_string()
                && accounts
                    .get(owner.0)
                    .is_ok_and(|account| account.parent() == request.root)
            {
                consent = enabled.unwrap_or(sync.is_some_and(|sync| sync.0));
            }
        }
        if !consent {
            continue;
        }
        let name = record
            .components
            .iter()
            .find(|component| component.key == "name")
            .and_then(|component| component.value.as_str())
            .map(str::to_string);
        if !record.deleted
            && name
                .as_ref()
                .is_none_or(|name| name.len() > 256 || name.trim().is_empty())
        {
            continue;
        }
        request.outbound = Some(Pending {
            operation_id: request.operation_id,
            profile_id: record.id,
            origin,
            local_revision: record.revision,
            name,
            base_revision: None,
            conflict: None,
            conflicted: false,
        });
    }
}

fn refresh(mut completions: MessageReader<MutationCompleted>, mut commands: Commands) {
    for completion in completions.read() {
        if completion.result.is_ok() {
            commands.entity(completion.root).insert(Refresh);
        }
    }
}

#[derive(Component, Clone)]
struct Load {
    data: PathBuf,
    origin: Origin,
    inventory: Inventory,
}

impl Job for Load {
    type Output = Result<Loaded, String>;
    fn run(self) -> Self::Output {
        Store::load(
            &self.data,
            &self.origin,
            self.inventory.after,
            self.inventory.through,
        )
    }
}

fn load(roots: LoadRoots<'_, '_>, context: Context, mut commands: Commands) {
    for (root, path, previous, refresh, inventory) in &roots {
        let Some((origin, _)) = context.origin(root) else {
            continue;
        };
        if previous == Some(&origin) && !refresh && inventory.is_none() {
            continue;
        }
        let continuing = previous == Some(&origin) && inventory.is_some();
        let inventory = if continuing {
            *inventory.unwrap()
        } else {
            Inventory {
                id: Uuid::new_v4(),
                after: 0,
                through: None,
            }
        };
        let request = Load {
            data: path.0.clone(),
            origin: origin.clone(),
            inventory,
        };
        commands.spawn((
            Operation::bundle("account.profile-outbox-read").unwrap(),
            ChildOf(root),
            request.clone(),
            JobRequest(request),
        ));
        let mut root = commands.entity(root);
        root.insert((origin, OutboxLoading, inventory));
        if !continuing {
            root.remove::<Refresh>().remove::<Seed>();
        }
    }
}

fn receive_load(
    mut results: Results<Load>,
    requests: Query<&Load>,
    context: Context,
    inventories: Query<&Inventory, With<Root>>,
    mut commands: Commands,
) {
    for (operation, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let Some(parent) = parent else {
            continue;
        };
        let root = parent.parent();
        let Ok(request) = requests.get(operation) else {
            continue;
        };
        let origin = &request.origin;
        let current = inventories.get(root).is_ok_and(|current| {
            current.id == request.inventory.id && current.after == request.inventory.after
        });
        if !current {
            commands.entity(operation).despawn();
            continue;
        }
        if context.origin(root).as_ref().map(|value| &value.0) != Some(origin) {
            commands
                .entity(root)
                .remove::<OutboxLoading>()
                .remove::<Inventory>();
            commands.entity(operation).despawn();
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("consumed".into()));
        if let Ok(Ok(loaded)) = value {
            let loaded: Loaded = loaded;
            commands
                .entity(operation)
                .insert(loaded)
                .remove::<JobResult<Load>>()
                .remove::<JobRequest<Load>>();
        } else {
            commands.entity(root).remove::<OutboxLoading>().insert((
                Refresh,
                Pause {
                    duration: Duration::from_secs(1),
                },
            ));
            commands.entity(operation).despawn();
        }
    }
}

fn restore_outbound(
    pages: Query<(&Load, &Loaded, &ChildOf)>,
    outbound: Query<(Entity, &Id, &ChildOf), With<Outbound>>,
    mut commands: Commands,
) {
    for (request, page, parent) in &pages {
        let root = parent.parent();
        for (order, entry) in &page.pending {
            if let Some((entity, _, _)) = outbound.iter().find(|(_, id, parent)| {
                parent.parent() == root && id.0 == entry.operation_id.to_string()
            }) {
                commands.entity(entity).insert(Seen(request.inventory.id));
                continue;
            }
            let mut entity = commands.spawn((
                Operation::bundle("account.profile-transfer").unwrap(),
                Outbound,
                Target(entry.profile_id),
                entry.origin.clone(),
                Revision(entry.local_revision),
                BaseRevision(entry.base_revision),
                ChildOf(root),
                vmux_ecs::Order(*order),
                Seen(request.inventory.id),
            ));
            entity.insert(Id(entry.operation_id.to_string()));
            if let Some(name) = &entry.name {
                entity.insert(Name::new(name.clone()));
            } else {
                entity.insert((Deleted, Name::new("Deleted profile")));
            }
            if entry.conflicted {
                entity.insert(Conflict(entry.conflict.clone()));
            }
        }
    }
}

fn restore_checkpoints(
    pages: Query<(&Load, &Loaded, &ChildOf)>,
    profiles: ReceiveLoadProfiles<'_, '_>,
    accounts: Query<&ChildOf, With<Account>>,
    mut commands: Commands,
) {
    for (request, page, parent) in &pages {
        let root = parent.parent();
        let origin = &request.origin;
        for checkpoint in &page.checkpoints {
            for (entity, id, owner, previous, remote, synced) in &profiles {
                if id.0 != checkpoint.profile_id.to_string()
                    || !accounts
                        .get(owner.0)
                        .is_ok_and(|account| account.parent() == root)
                {
                    continue;
                }
                if previous == Some(origin)
                    && (remote.is_some_and(|remote| remote.0 > checkpoint.remote_revision)
                        || synced.is_some_and(|synced| synced.0 > checkpoint.local_revision))
                {
                    continue;
                }
                let mut entity = commands.entity(entity);
                entity.insert((
                    origin.clone(),
                    RemoteRevision(checkpoint.remote_revision),
                    SyncedRevision(checkpoint.local_revision),
                ));
                if checkpoint.deleted {
                    entity.insert(RemoteDeleted);
                } else {
                    entity.remove::<RemoteDeleted>();
                }
            }
        }
    }
}

fn finish_load(
    pages: Query<(Entity, &Load, &Loaded, &ChildOf)>,
    outbound: FinishLoadOutbound<'_, '_>,
    mut commands: Commands,
) {
    for (operation, request, page, parent) in &pages {
        let root = parent.parent();
        commands.entity(root).remove::<OutboxLoading>();
        if let Some(after) = page.next {
            commands.entity(root).insert(Inventory {
                id: request.inventory.id,
                after,
                through: Some(page.through),
            });
        } else {
            for (entity, parent, busy, seen) in &outbound {
                if parent.parent() == root
                    && !busy
                    && seen.is_none_or(|seen| seen.0 != request.inventory.id)
                {
                    commands.entity(entity).despawn();
                }
            }
            commands.entity(root).remove::<Inventory>().insert(Seed);
        }
        commands.entity(operation).despawn();
    }
}

type FinishLoadOutbound<'w, 's> =
    Query<'w, 's, (Entity, &'static ChildOf, Has<Busy>, Option<&'static Seen>), With<Outbound>>;

fn seed(
    roots: SeedRoots<'_, '_>,
    context: Context,
    pending: Query<(&ChildOf, &Target, &Origin), With<Outbound>>,
    mutations: MutationOwners<'_, '_>,
    mut requests: MessageWriter<crate::SyncRequest>,
    mut commands: Commands,
    wake: Option<Res<JobWake>>,
) {
    for root in &roots {
        if mutations.iter().any(|(parent, _)| parent.parent() == root) {
            continue;
        }
        let Some(scope) = context.scope(root) else {
            continue;
        };
        if !context.accepts_scope(root, &scope) {
            continue;
        }
        for profile in &context.profiles {
            let Ok(id) = profile.id.0.parse() else {
                continue;
            };
            let Some((_, _, revision, synced, _)) = context.replica(root, id, &scope.origin) else {
                continue;
            };
            if revision <= synced
                || pending.iter().any(|(parent, target, origin)| {
                    parent.parent() == root && target.0 == id && *origin == scope.origin
                })
            {
                continue;
            }
            requests.write(crate::SyncRequest {
                root,
                operation_id: Uuid::new_v4(),
                account_id: scope.account_id,
                binding_revision: scope.binding_revision,
                profile_id: id,
                enabled: true,
            });
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
        }
        commands.entity(root).remove::<Seed>();
    }
}

fn enumerate(
    roots: EnumerateRoots<'_, '_>,
    context: Context,
    mut requests: MessageWriter<ListRequest>,
    mut commands: Commands,
    reading: Query<(&ChildOf, &Origin), With<Reading>>,
    wake: Option<Res<JobWake>>,
) {
    for (root, previous) in &roots {
        let Some((connection, origin, revision)) = context.connection(root) else {
            if previous.is_some() {
                commands.entity(root).remove::<Enumerated>();
            }
            continue;
        };
        if previous.is_some_and(|previous| previous.0 == connection) {
            continue;
        }
        if reading
            .iter()
            .any(|(parent, current)| parent.parent() == root && *current == origin)
        {
            continue;
        }
        let entity = commands
            .spawn((
                Operation::bundle("account.profile-list").unwrap(),
                Reading,
                origin,
                SentOn {
                    connection,
                    binding_revision: revision,
                },
                PageCursor(None),
                ChildOf(root),
            ))
            .id();
        requests.write(ListRequest {
            entity,
            connection,
            after: None,
        });
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
        commands.entity(root).insert(Enumerated(connection));
    }
}

impl RemoteProfile {
    fn accepts_page(
        records: &[Record],
        after: Option<Uuid>,
        next: Option<Uuid>,
        scope: Uuid,
    ) -> bool {
        if records.len() > 16 {
            return false;
        }
        let mut previous = after;
        for record in records {
            if previous.is_some_and(|previous| record.id <= previous)
                || Self::name(record, scope).is_err()
            {
                return false;
            }
            previous = Some(record.id);
        }
        next.is_none() || records.len() == 16 && next == previous
    }

    fn name(record: &Record, scope: Uuid) -> Result<Option<String>, String> {
        if !record.is_valid()
            || record.scope_id != scope
            || record.components.len() > 2
            || record.relationships.len() != 1
        {
            return Err("invalid remote Profile record".into());
        }
        if !record.components.iter().any(|component| {
            component.key == "profile" && component.version == 1 && component.value.is_null()
        }) {
            return Err("missing remote Profile marker".into());
        }
        let relationship = &record.relationships[0];
        if relationship.key != "account"
            || relationship.version != 1
            || relationship.target != scope
            || relationship.position != 0
            || !relationship.owned
        {
            return Err("invalid remote Profile owner".into());
        }
        let name = record
            .components
            .iter()
            .find(|component| component.key == "name" && component.version == 1)
            .and_then(|component| component.value.as_str());
        if record.deleted && record.components.len() == 1 {
            return Ok(None);
        }
        let name = name.ok_or("invalid remote Profile name")?;
        if name.trim().is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
            return Err("invalid remote Profile name".into());
        }
        if record.deleted {
            return Ok(None);
        }
        Ok(Some(name.to_string()))
    }
}

fn receive_list(
    mut pending: ReceiveListPending<'_, '_>,
    context: Context,
    remote: Query<(Entity, &Id, &ChildOf, &Origin, &Revision), With<RemoteProfile>>,
    mut requests: MessageWriter<ListRequest>,
    mut commands: Commands,
    wake: Option<Res<JobWake>>,
    enumerated: Query<&Enumerated, With<Root>>,
) {
    for (entity, parent, origin, sent, mut cursor, mut reply) in &mut pending {
        let root = parent.parent();
        if !context
            .connection(root)
            .is_some_and(|(connection, current, revision)| {
                connection == sent.connection
                    && current == *origin
                    && revision == sent.binding_revision
            })
        {
            if enumerated
                .get(root)
                .is_ok_and(|current| current.0 == sent.connection)
            {
                commands.entity(root).remove::<Enumerated>();
            }
            commands.entity(entity).despawn();
            continue;
        }
        let Some(Ok(Response::Entities { entities, next })) = reply.0.take() else {
            commands.entity(entity).despawn();
            commands.entity(root).remove::<Enumerated>().insert(Pause {
                duration: Duration::from_secs(1),
            });
            continue;
        };
        if !RemoteProfile::accepts_page(&entities, cursor.0, next, origin.account) {
            commands.entity(entity).despawn();
            commands.entity(root).remove::<Enumerated>().insert(Pause {
                duration: Duration::from_secs(1),
            });
            continue;
        }
        for record in entities {
            let name = RemoteProfile::name(&record, origin.account).unwrap();
            let existing = remote
                .iter()
                .find(|(_, id, parent, _, _)| {
                    parent.parent() == root && id.0 == record.id.to_string()
                })
                .map(|value| value.0);
            if existing.is_some_and(|entity| {
                remote
                    .get(entity)
                    .is_ok_and(|(_, _, _, previous, revision)| {
                        *previous == *origin && revision.0 > record.revision
                    })
            }) {
                continue;
            }
            let target = match existing {
                Some(entity) => entity,
                None => commands
                    .spawn((RemoteProfile, Id(record.id.to_string()), ChildOf(root)))
                    .id(),
            };
            let mut profile = commands.entity(target);
            profile.insert((Revision(record.revision), origin.clone(), Incoming(record)));
            if let Some(name) = name {
                profile.insert(Name::new(name)).remove::<Deleted>();
            } else {
                profile.insert((Name::new("Deleted profile"), Deleted));
            }
        }
        if let Some(next) = next {
            cursor.0 = Some(next);
            requests.write(ListRequest {
                entity,
                connection: sent.connection,
                after: Some(next),
            });
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
            commands.entity(entity).remove::<Reply>();
        } else {
            commands.entity(entity).despawn();
            commands.entity(root).insert(Pause {
                duration: Duration::from_secs(5),
            });
        }
    }
}

#[derive(SystemParam)]
struct Adoption<'w> {
    requests: MessageWriter<'w, RemoteRenameRequest>,
    wake: Option<Res<'w, JobWake>>,
}

impl Adoption<'_> {
    fn write(&mut self, request: RemoteRenameRequest) {
        self.requests.write(request);
        if let Some(wake) = &self.wake {
            wake.0.wake_by_ref();
        }
    }
}

fn reconcile(
    incoming: Query<(Entity, &ChildOf, &Origin, &Incoming), With<RemoteProfile>>,
    context: Context,
    mutations: MutationOwners<'_, '_>,
    pending: ReconcilePending<'_, '_>,
    locations: Query<&Location, With<Root>>,
    mut requests: Adoption,
    mut commands: Commands,
) {
    for (entity, parent, origin, record) in &incoming {
        let root = parent.parent();
        let Some(scope) = context.scope(root) else {
            commands.entity(entity).remove::<Incoming>();
            continue;
        };
        if scope.origin != *origin || !context.accepts_scope(root, &scope) {
            commands.entity(entity).remove::<Incoming>();
            continue;
        }
        let Some((_, local_name, local_revision, synced, remote)) =
            context.replica(root, record.0.id, origin)
        else {
            commands.entity(entity).remove::<Incoming>();
            continue;
        };
        if record.0.revision <= remote {
            commands.entity(entity).remove::<Incoming>();
            continue;
        }
        if mutations
            .iter()
            .any(|(parent, target)| parent.parent() == root && target.0 == record.0.id)
            || pending.iter().any(|(_, parent, target, current, _, busy)| {
                parent.parent() == root && target.0 == record.0.id && *current == *origin && busy
            })
        {
            continue;
        }
        if local_revision > synced {
            let Some((outbound, _, _, _, id, _)) =
                pending.iter().find(|(_, parent, target, current, _, _)| {
                    parent.parent() == root && target.0 == record.0.id && **current == *origin
                })
            else {
                commands.entity(root).insert(Refresh);
                continue;
            };
            let Ok(location) = locations.get(root) else {
                continue;
            };
            let request = SaveConflict {
                data: location.0.clone(),
                origin: origin.clone(),
                operation: id.0.parse().unwrap(),
                current: Some(record.0.clone()),
            };
            commands.entity(outbound).insert((
                Busy,
                Conflict(Some(record.0.clone())),
                request.clone(),
                JobRequest(request),
            ));
        } else {
            let Ok(name) = RemoteProfile::name(&record.0, origin.account) else {
                commands.entity(entity).remove::<Incoming>();
                continue;
            };
            let deleted = name.is_none();
            requests.write(RemoteRenameRequest {
                root,
                operation_id: Uuid::new_v4(),
                profile_id: record.0.id,
                name: name.unwrap_or(local_name),
                scope,
                checkpoint: CheckpointChange {
                    remote_revision: record.0.revision,
                    through_local_revision: local_revision,
                    deleted,
                    resolves: None,
                },
            });
        }
        commands.entity(entity).remove::<Incoming>();
    }
}

#[derive(Clone)]
struct Seal {
    data: PathBuf,
    origin: Origin,
    operation: Uuid,
    revision: u64,
}
impl Job for Seal {
    type Output = Result<Pending, String>;
    fn run(self) -> Self::Output {
        Store::seal(&self.data, &self.origin, self.operation, self.revision)
    }
}

fn seal(
    pending: SealPending<'_, '_>,
    busy: BusyOwners<'_, '_>,
    context: Context,
    roots: Query<&Location, With<Root>>,
    mut commands: Commands,
    conflicts: ConflictOwners<'_, '_>,
) {
    let mut ordered: Vec<_> = pending.iter().collect();
    ordered.sort_by_key(|item| item.5.0);
    let mut active = std::collections::HashSet::new();
    for (owner, origin) in &busy {
        if context
            .origin(owner.parent())
            .is_some_and(|(current, _)| current == *origin)
        {
            active.insert(owner.parent());
        }
    }
    for (entity, parent, id, target, origin, _) in ordered {
        let root = parent.parent();
        if active.contains(&root) {
            continue;
        }
        let Some((connection, current, binding_revision)) = context.connection(root) else {
            continue;
        };
        if current != *origin {
            continue;
        }
        if conflicts.iter().any(|(parent, conflicting, current)| {
            parent.parent() == root && conflicting.0 == target.0 && *current == *origin
        }) {
            continue;
        }
        let Some((_, revision)) = context.profile(root, target.0, origin) else {
            continue;
        };
        let Ok(path) = roots.get(root) else {
            continue;
        };
        commands.entity(entity).insert((
            Busy,
            SentOn {
                connection,
                binding_revision,
            },
            JobRequest(Seal {
                data: path.0.clone(),
                origin: origin.clone(),
                operation: id.0.parse().unwrap(),
                revision,
            }),
        ));
        active.insert(root);
    }
}

fn receive_seal(
    mut results: Results<Seal>,
    context: Context,
    sent: Query<(&ChildOf, &SentOn, &Origin)>,
    mut requests: MessageWriter<UploadRequest>,
    mut commands: Commands,
    attempts: Query<&Attempts>,
    wake: Option<Res<JobWake>>,
) {
    for (entity, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("consumed".into()));
        let Ok((parent, sent, origin)) = sent.get(entity) else {
            continue;
        };
        if let Ok(Ok(entry)) = value {
            if context
                .connection(parent.parent())
                .is_some_and(|(connection, current, revision)| {
                    connection == sent.connection
                        && current == *origin
                        && revision == sent.binding_revision
                        && context
                            .profile(parent.parent(), entry.profile_id, origin)
                            .is_some()
                })
            {
                requests.write(UploadRequest {
                    entity,
                    connection: sent.connection,
                    change: ProfileChange {
                        operation_id: entry.operation_id,
                        id: entry.profile_id,
                        base_revision: entry.base_revision.unwrap(),
                        name: entry.name,
                    },
                });
                if let Some(wake) = &wake {
                    wake.0.wake_by_ref();
                }
                commands
                    .entity(entity)
                    .insert((Sending, BaseRevision(entry.base_revision)));
            } else {
                let count = Attempts(
                    attempts
                        .get(entity)
                        .map_or(0, |value| value.0.saturating_add(1)),
                );
                let delay = count.delay();
                commands
                    .entity(entity)
                    .remove::<Busy>()
                    .insert((count, Pause { duration: delay }));
                commands.entity(parent.parent()).insert(Refresh);
            }
        } else {
            let count = Attempts(
                attempts
                    .get(entity)
                    .map_or(0, |value| value.0.saturating_add(1)),
            );
            let delay = count.delay();
            commands
                .entity(entity)
                .remove::<Busy>()
                .insert((count, Pause { duration: delay }));
            commands.entity(parent.parent()).insert(Refresh);
        }
        commands
            .entity(entity)
            .remove::<JobResult<Seal>>()
            .remove::<JobRequest<Seal>>();
    }
}

#[derive(Clone)]
struct Acknowledge {
    data: PathBuf,
    origin: Origin,
    receipt: vmux_ecs::record::Receipt,
}
impl Job for Acknowledge {
    type Output = Result<Checkpoint, String>;
    fn run(self) -> Self::Output {
        Store::acknowledge(&self.data, &self.origin, &self.receipt)
    }
}

#[derive(Component, Clone)]
struct SaveConflict {
    data: PathBuf,
    origin: Origin,
    operation: Uuid,
    current: Option<Record>,
}
impl Job for SaveConflict {
    type Output = Result<(), String>;
    fn run(self) -> Self::Output {
        Store::conflicted(&self.data, &self.origin, self.operation, self.current)
    }
}

fn receive_upload(
    mut pending: ReceiveUploadPending<'_, '_>,
    context: Context,
    roots: Query<&Location, With<Root>>,
    mut commands: Commands,
    attempts: Query<&Attempts>,
) {
    for (entity, parent, id, target, origin, sent, base, mut reply) in &mut pending {
        let root = parent.parent();
        let current = context
            .connection(root)
            .is_some_and(|(connection, current, revision)| {
                connection == sent.connection
                    && current == *origin
                    && revision == sent.binding_revision
            });
        let value = reply.0.take();
        if current {
            let Ok(path) = roots.get(root) else {
                continue;
            };
            match value {
                Some(Ok(Response::Committed(receipt)))
                    if receipt.operation_id.to_string() == id.0
                        && receipt.entity_id == target.0
                        && base.0.and_then(|revision| revision.checked_add(1))
                            == Some(receipt.revision) =>
                {
                    commands.entity(entity).insert(JobRequest(Acknowledge {
                        data: path.0.clone(),
                        origin: origin.clone(),
                        receipt,
                    }));
                }
                Some(Ok(Response::Conflict { current })) => {
                    if current.as_ref().is_none_or(|record| {
                        record.id == target.0 && RemoteProfile::name(record, origin.account).is_ok()
                    }) {
                        let request = SaveConflict {
                            data: path.0.clone(),
                            origin: origin.clone(),
                            operation: id.0.parse().unwrap(),
                            current: current.clone(),
                        };
                        commands.entity(entity).insert((
                            Conflict(current.clone()),
                            request.clone(),
                            JobRequest(request),
                        ));
                    } else {
                        let count = Attempts(
                            attempts
                                .get(entity)
                                .map_or(0, |value| value.0.saturating_add(1)),
                        );
                        let delay = count.delay();
                        commands
                            .entity(entity)
                            .remove::<Busy>()
                            .insert((count, Pause { duration: delay }));
                    }
                }
                _ => {
                    let count = Attempts(
                        attempts
                            .get(entity)
                            .map_or(0, |value| value.0.saturating_add(1)),
                    );
                    let delay = count.delay();
                    commands
                        .entity(entity)
                        .remove::<Busy>()
                        .insert((count, Pause { duration: delay }));
                }
            }
        } else {
            let count = Attempts(
                attempts
                    .get(entity)
                    .map_or(0, |value| value.0.saturating_add(1)),
            );
            let delay = count.delay();
            commands
                .entity(entity)
                .remove::<Busy>()
                .insert((count, Pause { duration: delay }));
        }
        commands
            .entity(entity)
            .remove::<Reply>()
            .remove::<Sending>();
    }
}

fn receive_ack(
    mut results: Results<Acknowledge>,
    requests: Query<&Origin, With<Outbound>>,
    context: Context,
    profiles: Query<(Entity, &Id, &Owner), With<Profile>>,
    accounts: Query<&ChildOf, With<Account>>,
    mut commands: Commands,
) {
    for (entity, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("consumed".into()));
        if let Ok(Ok(checkpoint)) = value {
            if let Some(parent) = parent {
                let root = parent.parent();
                if let Ok(request) = requests.get(entity)
                    && context
                        .origin(root)
                        .is_some_and(|(origin, _)| origin == *request)
                {
                    for (profile, id, owner) in &profiles {
                        if id.0 == checkpoint.profile_id.to_string()
                            && accounts
                                .get(owner.0)
                                .is_ok_and(|parent| parent.parent() == root)
                        {
                            let mut profile = commands.entity(profile);
                            profile.insert((
                                request.clone(),
                                RemoteRevision(checkpoint.remote_revision),
                                SyncedRevision(checkpoint.local_revision),
                            ));
                            if checkpoint.deleted {
                                profile.insert(RemoteDeleted);
                            } else {
                                profile.remove::<RemoteDeleted>();
                            }
                        }
                    }
                }
                commands.entity(root).insert(Refresh);
            }
            commands.entity(entity).despawn();
        } else {
            commands
                .entity(entity)
                .remove::<Busy>()
                .remove::<JobResult<Acknowledge>>()
                .remove::<JobRequest<Acknowledge>>();
        }
    }
}

fn receive_conflict(
    mut results: Results<SaveConflict>,
    attempts: Query<&Attempts>,
    mut commands: Commands,
) {
    for (entity, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("consumed".into()));
        let mut target = commands.entity(entity);
        target
            .remove::<JobResult<SaveConflict>>()
            .remove::<JobRequest<SaveConflict>>();
        if matches!(value, Ok(Ok(()))) {
            target
                .remove::<Busy>()
                .remove::<Attempts>()
                .remove::<SaveConflict>();
        } else {
            let count = Attempts(
                attempts
                    .get(entity)
                    .map_or(1, |value| value.0.saturating_add(1)),
            );
            let duration = count.delay();
            target.insert((Busy, count, Pause { duration }));
        }
    }
}

fn retry_conflict(pending: ConflictRetries<'_, '_>, mut commands: Commands) {
    for (entity, request) in &pending {
        commands.entity(entity).insert(JobRequest(request.clone()));
    }
}

type ConflictRetries<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static SaveConflict),
    (
        With<Busy>,
        Without<JobRequest<SaveConflict>>,
        Without<Delayed>,
        Without<Pause>,
    ),
>;

type Results<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut JobResult<J>,
        &'static JobOwner<J>,
        Option<&'static ChildOf>,
    ),
>;

#[derive(Clone)]
struct Replace {
    data: PathBuf,
    pending: Pending,
    scope: ExpectedScope,
    conflict_id: Uuid,
}

impl Job for Replace {
    type Output = Result<(), String>;
    fn run(self) -> Self::Output {
        Store::replace_pending(&self.data, &self.pending, &self.scope, self.conflict_id)
    }
}

fn resolve_local(
    mut requests: MessageReader<KeepLocalRequest>,
    context: Context,
    roots: Query<&Location, With<Root>>,
    conflicts: ResolveLocalConflicts<'_, '_>,
    profiles: Query<(&Id, &Name, &Revision, &Owner), With<Profile>>,
    accounts: Query<&ChildOf, With<Account>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Some(scope) = context.scope(request.root) else {
            continue;
        };
        if scope.binding_revision != request.binding_revision
            || !context.accepts_scope(request.root, &scope)
            || context
                .profile(request.root, request.profile_id, &scope.origin)
                .is_none()
        {
            continue;
        }
        let origin = &scope.origin;
        let Ok(path) = roots.get(request.root) else {
            continue;
        };
        let Some((conflict_entity, _, _, _, conflict, _, _)) =
            conflicts
                .iter()
                .find(|(_, parent, target, current, _, id, busy)| {
                    parent.parent() == request.root
                        && target.0 == request.profile_id
                        && *current == origin
                        && id.0 == request.conflict_id.to_string()
                        && !busy
                })
        else {
            continue;
        };
        if conflict.0.as_ref().is_some_and(|record| record.deleted) {
            continue;
        }
        let base = conflict.0.as_ref().map_or(0, |record| record.revision);
        if base != request.remote_revision {
            continue;
        }
        for (id, name, revision, owner) in &profiles {
            if id.0 != request.profile_id.to_string()
                || !accounts
                    .get(owner.0)
                    .is_ok_and(|account| account.parent() == request.root)
                || name.as_str().len() > 256
                || revision.0 != request.local_revision
            {
                continue;
            }
            let pending = Pending {
                operation_id: Uuid::new_v4(),
                profile_id: request.profile_id,
                origin: origin.clone(),
                local_revision: revision.0,
                name: Some(name.as_str().into()),
                base_revision: Some(base),
                conflict: None,
                conflicted: false,
            };
            commands.entity(conflict_entity).insert((
                Busy,
                JobRequest(Replace {
                    data: path.0.clone(),
                    pending,
                    scope: scope.clone(),
                    conflict_id: request.conflict_id,
                }),
            ));
        }
    }
}

fn resolve_remote(
    mut requests: MessageReader<UseRemoteRequest>,
    context: Context,
    conflicts: ResolveRemoteConflicts<'_, '_>,
    profiles: Query<(&Id, &Name, &Revision, &Owner), With<Profile>>,
    accounts: Query<&ChildOf, With<Account>>,
    mut rename: Adoption,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Some(scope) = context.scope(request.root) else {
            continue;
        };
        if scope.binding_revision != request.binding_revision
            || !context.accepts_scope(request.root, &scope)
            || context
                .profile(request.root, request.profile_id, &scope.origin)
                .is_none()
        {
            continue;
        }
        let Some((conflict_entity, _, _, _, conflict, _, _)) =
            conflicts
                .iter()
                .find(|(_, parent, target, current, _, id, busy)| {
                    parent.parent() == request.root
                        && target.0 == request.profile_id
                        && **current == scope.origin
                        && id.0 == request.conflict_id.to_string()
                        && !busy
                })
        else {
            continue;
        };
        let Some((_, local_name, local_revision, _)) = profiles.iter().find(|(id, _, _, owner)| {
            id.0 == request.profile_id.to_string()
                && accounts
                    .get(owner.0)
                    .is_ok_and(|parent| parent.parent() == request.root)
        }) else {
            continue;
        };
        let (remote, name) = match &conflict.0 {
            Some(record) => match RemoteProfile::name(record, scope.origin.account) {
                Ok(name) => (record.revision, name),
                Err(_) => continue,
            },
            None => (0, None),
        };
        if remote != request.remote_revision || local_revision.0 != request.local_revision {
            continue;
        }
        let deleted = name.is_none();
        let operation = Uuid::new_v4();
        commands
            .entity(conflict_entity)
            .insert((Busy, Adopting(operation)));
        rename.write(RemoteRenameRequest {
            root: request.root,
            operation_id: operation,
            profile_id: request.profile_id,
            name: name.unwrap_or_else(|| local_name.as_str().into()),
            scope,
            checkpoint: CheckpointChange {
                remote_revision: remote,
                through_local_revision: local_revision.0,
                deleted,
                resolves: Some(request.conflict_id),
            },
        });
    }
}

fn import_remote(
    mut requests: MessageReader<ImportRequest>,
    context: Context,
    remote: ImportRemoteRemote<'_, '_>,
    mut imports: MessageWriter<ImportProfileRequest>,
    wake: Option<Res<JobWake>>,
) {
    for request in requests.read() {
        let Some(scope) = context.scope(request.root) else {
            continue;
        };
        if scope.binding_revision != request.binding_revision
            || !context.accepts_scope(request.root, &scope)
        {
            continue;
        }
        for (parent, id, name, revision, current) in &remote {
            if parent.parent() != request.root
                || id.0 != request.profile_id.to_string()
                || *current != scope.origin
                || revision.0 != request.remote_revision
            {
                continue;
            }
            imports.write(ImportProfileRequest {
                root: request.root,
                operation_id: Uuid::new_v4(),
                profile_id: request.profile_id,
                name: name.as_str().into(),
                scope: scope.clone(),
                checkpoint: CheckpointChange {
                    remote_revision: revision.0,
                    through_local_revision: 0,
                    deleted: false,
                    resolves: None,
                },
            });
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
        }
    }
}

fn receive_resolution(mut replaced: Results<Replace>, mut commands: Commands) {
    for (entity, result, owner, parent) in &mut replaced {
        if !owner.accepts(parent) {
            continue;
        }
        if let Some(parent) = parent {
            commands.entity(parent.parent()).insert(Refresh);
        }
        if result.0.as_ref().is_ok_and(|result| result.is_ok()) {
            commands.entity(entity).despawn();
        } else {
            commands
                .entity(entity)
                .remove::<Busy>()
                .remove::<JobResult<Replace>>()
                .remove::<JobRequest<Replace>>();
        }
    }
}

fn finish_adoption(
    mut completions: MessageReader<MutationCompleted>,
    pending: Query<(Entity, &ChildOf, &Adopting)>,
    mut commands: Commands,
) {
    for completion in completions.read() {
        for (entity, parent, adopting) in &pending {
            if parent.parent() != completion.root || adopting.0 != completion.operation_id {
                continue;
            }
            commands.entity(completion.root).insert(Refresh);
            if completion.result.is_ok() {
                commands.entity(entity).despawn();
            } else {
                commands
                    .entity(entity)
                    .remove::<Adopting>()
                    .remove::<Busy>();
            }
        }
    }
}

type Activity<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static Target,
        &'static Origin,
        Has<Busy>,
        Option<&'static Conflict>,
        &'static Id,
    ),
    With<Outbound>,
>;

#[derive(SystemParam)]
pub struct SyncProjection<'w, 's> {
    pending: Activity<'w, 's>,
    context: Context<'w, 's>,
    remote: Query<'w, 's, (&'static ChildOf, &'static Id, &'static Origin), With<RemoteProfile>>,
}

impl SyncProjection<'_, '_> {
    pub fn activity(&self, root: Entity, profile: Uuid) -> (bool, bool, bool) {
        let Some((current, _)) = self.context.origin(root) else {
            return (false, false, false);
        };
        let mut pending = false;
        let mut sending = false;
        let mut conflicted = false;
        for (parent, target, origin, busy, conflict, _) in &self.pending {
            if parent.parent() == root && target.0 == profile && *origin == current {
                pending = true;
                sending |= busy;
                conflicted |= conflict.is_some();
            }
        }
        (pending, sending, conflicted)
    }

    pub fn visible(&self, root: Entity, profile: &str) -> bool {
        let Some((current, _)) = self.context.origin(root) else {
            return false;
        };
        self.remote.iter().any(|(parent, id, origin)| {
            parent.parent() == root && id.0 == profile && *origin == current
        })
    }

    pub fn available(&self, root: Entity) -> bool {
        self.context.connection(root).is_some()
    }

    pub fn conflict(
        &self,
        root: Entity,
        profile: Uuid,
    ) -> Option<(Uuid, u64, bool, Option<String>)> {
        let (current, _) = self.context.origin(root)?;
        for (parent, target, origin, _, conflict, id) in &self.pending {
            if parent.parent() != root || target.0 != profile || *origin != current {
                continue;
            }
            let Some(conflict) = conflict else {
                continue;
            };
            let revision = conflict.0.as_ref().map_or(0, |record| record.revision);
            let deleted = conflict.0.as_ref().is_some_and(|record| record.deleted);
            let name = match &conflict.0 {
                Some(record) => RemoteProfile::name(record, current.account).ok().flatten(),
                None => None,
            };
            return Some((id.0.parse().ok()?, revision, deleted, name));
        }
        None
    }
}
