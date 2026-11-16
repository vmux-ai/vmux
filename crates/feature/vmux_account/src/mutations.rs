use crate::account::Set;
use crate::catalog::{Catalog, Error, Loading, Root};
use crate::profile::{Account, Binding, Failure, Profile, SyncEnabled};
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryData;
use bevy_ecs::reflect::AppTypeRegistry;
use bevy_ecs::system::SystemParam;
use uuid::Uuid;
use vmux_ecs::record::{CaptureRequest, Captured, Confirmed, Receipt, Record};
use vmux_ecs::{
    Active, Operation, OperationCompleted, Order, ProfileId, ProfileLabel, Ready, Revision,
    Uuid as Id,
};

pub(crate) struct MutationsPlugin;

impl bevy_app::Plugin for MutationsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CreateRequest>()
            .add_message::<RenameRequest>()
            .add_message::<LinkRequest>()
            .add_message::<SyncRequest>()
            .add_message::<MutationCompleted>()
            .add_message::<WriteRequest>()
            .add_message::<WriteCompleted>()
            .add_systems(
                Update,
                (create, rename, link, select_sync)
                    .chain()
                    .in_set(Set::Receive),
            )
            .add_systems(Update, (select, prepare).chain().in_set(Set::Prepare))
            .add_systems(Update, write.in_set(Set::Write))
            .add_systems(
                Update,
                (collect, initialize, apply).chain().in_set(Set::Apply),
            )
            .add_systems(Update, (replayed, reply).chain().in_set(Set::Reply));
    }
}

#[derive(Message)]
pub struct CreateRequest {
    pub root: Entity,
    pub operation_id: Uuid,
    pub name: String,
}

#[derive(Message)]
pub struct RenameRequest {
    pub root: Entity,
    pub operation_id: Uuid,
    pub profile_id: Uuid,
    pub name: String,
}

#[derive(Message)]
pub struct LinkRequest {
    pub root: Entity,
    pub operation_id: Uuid,
    pub account_id: Uuid,
    pub binding: Binding,
}

#[derive(Message)]
pub struct SyncRequest {
    pub root: Entity,
    pub operation_id: Uuid,
    pub account_id: Uuid,
    pub binding_revision: u64,
    pub profile_id: Uuid,
    pub enabled: bool,
}

#[derive(Message)]
pub struct MutationCompleted {
    pub root: Entity,
    pub operation_id: Uuid,
    pub result: Result<Receipt, Failure>,
    pub directory_id: Option<String>,
    pub created: bool,
}

impl MutationCompleted {
    fn failed(root: Entity, id: &Id, failure: Failure, created: bool) -> Self {
        Self {
            root,
            operation_id: id.0.parse().unwrap(),
            result: Err(failure),
            directory_id: None,
            created,
        }
    }
}

#[derive(Message, Clone)]
pub struct WriteRequest {
    pub entity: Entity,
    pub root: Entity,
    pub generation: u64,
    pub operation_id: Uuid,
    pub intent: String,
    pub target_id: Uuid,
    pub records: Vec<Record>,
    pub directory: Option<String>,
    pub exclusive: bool,
}

#[derive(Message)]
pub struct WriteCompleted {
    pub entity: Entity,
    pub result: Option<Result<Commit, WriteFailure>>,
}

#[derive(Clone, Debug)]
pub struct Commit {
    pub generation: u64,
    pub receipt: Receipt,
    pub replayed: bool,
}

pub enum WriteFailure {
    Conflict,
    Storage(String),
}

#[derive(Component)]
pub(crate) struct Initializing;

#[derive(Component)]
struct Creation;

#[derive(Component)]
pub(crate) struct Proposal;

#[derive(Component)]
pub(crate) struct Target(pub(crate) Uuid);

#[derive(Component)]
struct Selected;

#[derive(Component)]
struct Prepared;

#[derive(Component)]
struct Sent;

#[derive(Component)]
pub(crate) struct Writing;

#[derive(Component)]
struct ExpectedBinding(u64);

#[derive(Component)]
pub(crate) struct EnsureDirectory(pub(crate) String);

#[derive(Component)]
struct AwaitReload(Commit);

#[derive(Component)]
struct Committed(Commit);

#[derive(Component)]
struct Accepted(Commit);

#[derive(SystemParam)]
struct Queue<'w, 's> {
    roots: WritableRoots<'w, 's>,
    operations: Query<'w, 's, (&'static Id, &'static ChildOf), With<Operation>>,
}

impl Queue<'_, '_> {
    fn reserve(&mut self, root: Entity, id: Uuid) -> Result<Order, Failure> {
        if id.is_nil() {
            return Err(Failure::invalid_name());
        }
        for (operation, owner) in &self.operations {
            if owner.parent() == root && operation.0 == id.to_string() {
                return Err(Failure::conflict());
            }
        }
        let mut order = self.roots.get_mut(root).map_err(|_| Failure::not_ready())?;
        order.0 = order.0.checked_add(1).ok_or(Failure::not_ready())?;
        Ok(Order(order.0))
    }
}

fn create(
    mut requests: MessageReader<CreateRequest>,
    mut queue: Queue,
    mut replies: MessageWriter<MutationCompleted>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let name = Profile::name(&request.name);
        let order = queue.reserve(request.root, request.operation_id);
        let (name, order) = match (name, order) {
            (Ok(name), Ok(order)) => (name, order),
            (Err(error), _) | (_, Err(error)) => {
                replies.write(MutationCompleted {
                    root: request.root,
                    operation_id: request.operation_id,
                    result: Err(error),
                    directory_id: None,
                    created: true,
                });
                continue;
            }
        };
        commands
            .spawn((
                Operation::bundle("account.create").unwrap(),
                Creation,
                ChildOf(request.root),
                Name::new(name),
                order,
            ))
            .insert(Id(request.operation_id.to_string()));
    }
}

fn rename(
    mut requests: MessageReader<RenameRequest>,
    mut queue: Queue,
    mut replies: MessageWriter<MutationCompleted>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let name = Profile::name(&request.name);
        let order = queue.reserve(request.root, request.operation_id);
        let (name, order) = match (name, order) {
            (Ok(name), Ok(order)) => (name, order),
            (Err(error), _) | (_, Err(error)) => {
                replies.write(MutationCompleted {
                    root: request.root,
                    operation_id: request.operation_id,
                    result: Err(error),
                    directory_id: None,
                    created: false,
                });
                continue;
            }
        };
        commands
            .spawn((
                Operation::bundle("account.rename").unwrap(),
                Target(request.profile_id),
                ChildOf(request.root),
                Name::new(name),
                order,
            ))
            .insert(Id(request.operation_id.to_string()));
    }
}

fn link(
    mut requests: MessageReader<LinkRequest>,
    mut queue: Queue,
    catalog: Catalog,
    bindings: Query<&Binding, With<Account>>,
    mut replies: MessageWriter<MutationCompleted>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let result = match catalog.account(request.root) {
            Some((account, id)) if id == request.account_id && request.binding.is_valid() => {
                let current = bindings.get(account).ok();
                if request.binding.is_linked()
                    && current
                        .is_some_and(|current| current.is_linked() && current != &request.binding)
                {
                    Err(Failure::account_mismatch())
                } else {
                    queue.reserve(request.root, request.operation_id)
                }
            }
            _ => Err(Failure::account_mismatch()),
        };
        let order = match result {
            Ok(order) => order,
            Err(error) => {
                replies.write(MutationCompleted {
                    root: request.root,
                    operation_id: request.operation_id,
                    result: Err(error),
                    directory_id: None,
                    created: false,
                });
                continue;
            }
        };
        commands
            .spawn((
                Operation::bundle("account.link").unwrap(),
                ChildOf(request.root),
                request.binding.clone(),
                order,
            ))
            .insert(Id(request.operation_id.to_string()));
    }
}

fn select_sync(
    mut requests: MessageReader<SyncRequest>,
    mut queue: Queue,
    catalog: Catalog,
    available: Query<(), With<crate::CloudAvailable>>,
    states: Query<(&Revision, Option<&Binding>), With<Account>>,
    mut replies: MessageWriter<MutationCompleted>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let result = match catalog.account(request.root) {
            Some((account, id))
                if id == request.account_id
                    && states
                        .get(account)
                        .is_ok_and(|(revision, _)| revision.0 == request.binding_revision) =>
            {
                if request.enabled && !available.contains(request.root) {
                    Err(Failure::access_denied())
                } else if request.enabled
                    && !states
                        .get(account)
                        .is_ok_and(|(_, binding)| binding.is_some_and(Binding::is_linked))
                {
                    Err(Failure::not_linked())
                } else {
                    queue.reserve(request.root, request.operation_id)
                }
            }
            _ => Err(Failure::account_mismatch()),
        };
        let order = match result {
            Ok(order) => order,
            Err(error) => {
                replies.write(MutationCompleted {
                    root: request.root,
                    operation_id: request.operation_id,
                    result: Err(error),
                    directory_id: None,
                    created: false,
                });
                continue;
            }
        };
        commands
            .spawn((
                Operation::bundle("account.sync-preference").unwrap(),
                Target(request.profile_id),
                ChildOf(request.root),
                SyncEnabled(request.enabled),
                ExpectedBinding(request.binding_revision),
                order,
            ))
            .insert(Id(request.operation_id.to_string()));
    }
}

#[derive(QueryData)]
struct PendingData {
    entity: Entity,
    parent: &'static ChildOf,
    id: &'static Id,
    name: Option<&'static Name>,
    order: &'static Order,
    target: Option<&'static Target>,
    creation: Has<Creation>,
    binding: Option<&'static Binding>,
    sync: Option<&'static SyncEnabled>,
    expected: Option<&'static ExpectedBinding>,
    selected: Has<Selected>,
}

type Pending<'w, 's> = Query<
    'w,
    's,
    PendingData,
    (
        With<Operation>,
        Without<Prepared>,
        Without<Accepted>,
        Without<AwaitReload>,
        Or<(With<Creation>, With<Target>, With<Binding>)>,
    ),
>;

fn select(
    pending: Pending,
    reserved: Query<&ChildOf, With<Selected>>,
    catalog: Catalog,
    available: AvailableRoots,
    mut replies: MessageWriter<MutationCompleted>,
    account_state: Query<(&Revision, Option<&Binding>), With<Account>>,
    mut commands: Commands,
) {
    let mut ordered: Vec<_> = pending.iter().collect();
    ordered.sort_by_key(|input| input.order.0);
    let mut busy = std::collections::HashSet::new();
    for parent in &reserved {
        busy.insert(parent.parent());
    }
    for input in ordered {
        let operation = input.entity;
        let root = input.parent.parent();
        let (binding, sync, expected) = (input.binding, input.sync, input.expected);
        if !available.contains(root) || !busy.insert(root) {
            continue;
        }
        let Some((account, _)) = catalog.account(root) else {
            continue;
        };
        let (account_revision, current_binding) = account_state.get(account).unwrap();
        let id = input.id;
        if expected.is_some_and(|expected| expected.0 != account_revision.0)
            || binding.is_some_and(|binding| {
                binding.is_linked()
                    && current_binding
                        .is_some_and(|current| current.is_linked() && current != binding)
            })
        {
            replies.write(MutationCompleted::failed(
                root,
                id,
                Failure::account_mismatch(),
                false,
            ));
            commands.entity(operation).despawn();
            continue;
        }
        if binding.is_none()
            && sync.is_some_and(|sync| sync.0)
            && !current_binding.is_some_and(Binding::is_linked)
        {
            replies.write(MutationCompleted::failed(
                root,
                id,
                Failure::not_linked(),
                false,
            ));
            commands.entity(operation).despawn();
            continue;
        }
        commands.entity(operation).insert(Selected);
    }
}

fn prepare(
    pending: Pending,
    catalog: Catalog,
    available: AvailableRoots,
    mut replies: MessageWriter<MutationCompleted>,
    account_state: Query<(&Revision, Option<&Binding>), With<Account>>,
    mut commands: Commands,
) {
    let mut ordered: Vec<_> = pending.iter().collect();
    ordered.sort_by_key(|input| input.order.0);
    for input in ordered {
        let operation = input.entity;
        let root = input.parent.parent();
        let (name, target) = (input.name, input.target);
        let (creation, binding) = (input.creation, input.binding);
        if !input.selected {
            continue;
        }
        if !available.contains(root) {
            commands.entity(operation).remove::<Selected>();
            continue;
        }
        let Some((account, scope)) = catalog.account(root) else {
            commands.entity(operation).remove::<Selected>();
            continue;
        };
        let (account_revision, _) = account_state.get(account).unwrap();
        let id = input.id;
        if binding.is_some() {
            let revision = account_revision.0.checked_add(1);
            let Some(revision) = revision.filter(|revision| *revision <= i64::MAX as u64) else {
                let failure = Failure::not_ready();
                replies.write(MutationCompleted::failed(root, id, failure, false));
                commands.entity(operation).despawn();
                continue;
            };
            commands.entity(root).insert(Writing);
            commands.entity(operation).remove::<Selected>().insert((
                Prepared,
                Target(scope),
                Revision(revision),
                CaptureRequest {
                    scope_id: scope,
                    entities: catalog.entities(root),
                },
            ));
            continue;
        }
        let (target_id, directory, revision) = if creation {
            let directory = catalog.available_directory(account, name.unwrap().as_str());
            (Uuid::new_v4(), directory, 1)
        } else {
            let target_id = target.unwrap().0;
            let Some(entity) = catalog.profile(root, target_id) else {
                replies.write(MutationCompleted::failed(
                    root,
                    id,
                    Failure::unknown_profile(),
                    false,
                ));
                commands.entity(operation).despawn();
                continue;
            };
            let (_, _, directory, _, _, revision, _) = catalog.profiles.get(entity).unwrap();
            let Some(revision) = revision
                .0
                .checked_add(1)
                .filter(|revision| *revision <= i64::MAX as u64)
            else {
                replies.write(MutationCompleted::failed(
                    root,
                    id,
                    Failure::not_ready(),
                    false,
                ));
                commands.entity(operation).despawn();
                continue;
            };
            (target_id, directory.0.clone(), revision)
        };
        let mut entities = catalog.entities(root);
        if creation {
            let label = name.unwrap().as_str();
            let proposal = commands
                .spawn((
                    Profile::bundle(scope, target_id, &directory, label, account),
                    Proposal,
                    ChildOf(operation),
                ))
                .id();
            entities.push(proposal);
        }
        commands.entity(root).insert(Writing);
        commands.entity(operation).remove::<Selected>().insert((
            Prepared,
            Target(target_id),
            ProfileId(directory),
            Revision(revision),
            CaptureRequest {
                scope_id: scope,
                entities,
            },
        ));
    }
}

#[derive(QueryData)]
#[query_data(mutable)]
struct Capture {
    entity: Entity,
    parent: &'static ChildOf,
    id: &'static Id,
    target: &'static Target,
    captured: &'static mut Captured,
    name: Option<&'static Name>,
    directory: Option<&'static ProfileId>,
    revision: Option<&'static Revision>,
    ensure: Option<&'static EnsureDirectory>,
    creation: Has<Creation>,
    initializing: Has<Initializing>,
    binding: Option<&'static Binding>,
    sync: Option<&'static SyncEnabled>,
}

type Captures<'w, 's> = Query<'w, 's, Capture, Without<Sent>>;

fn write(
    mut captures: Captures,
    generations: Query<&Revision, With<Root>>,
    registry: Res<AppTypeRegistry>,
    mut requests: MessageWriter<WriteRequest>,
    mut replies: MessageWriter<MutationCompleted>,
    mut commands: Commands,
) {
    let registry = registry.read();
    for mut capture in &mut captures {
        let root = capture.parent.parent();
        let (entity, id, target) = (capture.entity, capture.id, capture.target);
        let (name, binding, sync) = (capture.name, capture.binding, capture.sync);
        let (creation, initializing) = (capture.creation, capture.initializing);
        let Ok(generation) = generations.get(root) else {
            continue;
        };
        let value = std::mem::replace(&mut capture.captured.0, Err("capture consumed".into()));
        let mut records = match value {
            Ok(records) => records,
            Err(error) => {
                let failure = Failure::storage(error);
                if initializing {
                    commands.entity(root).insert(Error(failure));
                } else {
                    replies.write(MutationCompleted::failed(root, id, failure, creation));
                }
                commands
                    .entity(root)
                    .remove::<Writing>()
                    .remove::<Loading>();
                commands.entity(entity).despawn();
                continue;
            }
        };
        let intent = if initializing {
            serde_json::json!({"initialize": target.0}).to_string()
        } else if creation {
            let name = name.unwrap().as_str();
            serde_json::json!({"create": name}).to_string()
        } else {
            let mut failed = None;
            for record in &mut records {
                if record.id != target.0 {
                    if binding.is_some_and(|binding| !binding.is_linked()) {
                        let Some(next) = record
                            .revision
                            .checked_add(1)
                            .filter(|revision| *revision <= i64::MAX as u64)
                        else {
                            failed = Some("profile revision exhausted".to_string());
                            break;
                        };
                        record.revision = next;
                        if let Err(error) = record.set(&registry, &SyncEnabled(false)) {
                            failed = Some(error);
                            break;
                        }
                    }
                    continue;
                }
                record.revision = capture.revision.unwrap().0;
                let result = if let Some(binding) = binding {
                    record.set(&registry, binding)
                } else if let Some(sync) = sync {
                    record.set(&registry, sync)
                } else {
                    record.set(&registry, name.unwrap())
                };
                if let Err(error) = result {
                    failed = Some(error);
                    break;
                }
            }
            if let Some(error) = failed {
                let failure = Failure::storage(error);
                replies.write(MutationCompleted::failed(root, id, failure, false));
                commands.entity(root).remove::<Writing>();
                commands.entity(entity).despawn();
                continue;
            }
            if let Some(binding) = binding {
                serde_json::json!({"link": target.0, "binding": binding}).to_string()
            } else if let Some(sync) = sync {
                serde_json::json!({"sync": target.0, "enabled": sync.0}).to_string()
            } else {
                serde_json::json!({"rename": target.0, "name": name.unwrap().as_str()}).to_string()
            }
        };
        requests.write(WriteRequest {
            entity,
            root,
            generation: generation.0,
            operation_id: id.0.parse().unwrap(),
            intent,
            target_id: target.0,
            records,
            directory: if creation {
                Some(capture.directory.unwrap().0.clone())
            } else {
                capture.ensure.map(|directory| directory.0.clone())
            },
            exclusive: creation,
        });
        commands.entity(entity).insert(Sent).remove::<Captured>();
    }
}

type Writes<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static Id,
        &'static Target,
        Option<&'static Name>,
        Has<Creation>,
        Has<Initializing>,
        Option<&'static Binding>,
        Option<&'static SyncEnabled>,
    ),
    With<Sent>,
>;

fn collect(
    mut events: MessageMutator<WriteCompleted>,
    active: Writes,
    catalog: Catalog,
    proposals: Query<(Entity, &ChildOf), With<Proposal>>,
    mut generations: Generations,
    mut commands: Commands,
    mut replies: MessageWriter<MutationCompleted>,
) {
    for event in events.read() {
        let Some(result) = event.result.take() else {
            continue;
        };
        let Ok((parent, id, target, _, creation, initializing, _, _)) = active.get(event.entity)
        else {
            continue;
        };
        let root = parent.parent();
        let Ok(mut generation) = generations.get_mut(root) else {
            continue;
        };
        let commit = match result {
            Ok(commit) => commit,
            Err(error) => {
                let failure = match error {
                    WriteFailure::Conflict => Failure::conflict(),
                    WriteFailure::Storage(error) => Failure::storage(error),
                };
                if !initializing {
                    replies.write(MutationCompleted::failed(
                        root,
                        id,
                        failure.clone(),
                        creation,
                    ));
                }
                commands.entity(root).remove::<Writing>();
                if failure == Failure::conflict() {
                    commands.entity(root).remove::<Ready>().remove::<Loading>();
                } else {
                    if initializing {
                        commands
                            .entity(root)
                            .insert(Error(failure))
                            .remove::<Loading>();
                    }
                }
                commands.entity(event.entity).despawn();
                continue;
            }
        };
        if commit.receipt.operation_id.to_string() != id.0
            || commit.receipt.revision == 0
            || commit.receipt.revision > i64::MAX as u64
            || (!commit.replayed && commit.receipt.entity_id != target.0)
        {
            if !initializing {
                let failure = Failure::invalid_store();
                replies.write(MutationCompleted::failed(root, id, failure, creation));
            }
            commands
                .entity(root)
                .insert(Error(Failure::invalid_store()))
                .remove::<Writing>()
                .remove::<Loading>();
            commands.entity(event.entity).despawn();
            continue;
        }
        if commit.replayed {
            for (entity, parent) in &proposals {
                if parent.parent() == event.entity {
                    commands.entity(entity).despawn();
                }
            }
            commands
                .entity(event.entity)
                .insert(AwaitReload(commit))
                .remove::<Sent>();
            commands
                .entity(root)
                .remove::<Ready>()
                .remove::<Loading>()
                .remove::<Writing>();
            continue;
        }
        generation.0 = generation.0.max(commit.generation);
        let Some(_) = catalog.account(root) else {
            continue;
        };
        commands.entity(event.entity).insert(Committed(commit));
    }
}

type Initializations<'w, 's> =
    Query<'w, 's, (Entity, &'static ChildOf, &'static Committed), (With<Sent>, With<Initializing>)>;

fn initialize(pending: Initializations, catalog: Catalog, mut commands: Commands) {
    for (operation, parent, committed) in &pending {
        let root = parent.parent();
        let commit = &committed.0;
        let mut confirmations = Vec::new();
        let selection = &catalog.selections.get(root).unwrap().0;
        for entity in catalog.entities(root) {
            if let Ok((_, _, directory, _, _, revision, _)) = catalog.profiles.get(entity) {
                let mut profile = commands.entity(entity);
                profile.insert(ProfileLabel);
                if directory.0 == *selection {
                    profile.insert(Active);
                }
                confirmations.push((
                    entity,
                    Receipt {
                        operation_id: commit.receipt.operation_id,
                        entity_id: catalog.profiles.get(entity).unwrap().1.0.parse().unwrap(),
                        revision: revision.0,
                    },
                ));
            } else {
                confirmations.push((entity, commit.receipt.clone()));
            }
        }
        commands
            .entity(root)
            .insert(Ready)
            .remove::<Loading>()
            .remove::<Error>()
            .remove::<Writing>();
        commands
            .entity(operation)
            .insert((Confirmed(confirmations), Accepted(commit.clone())))
            .remove::<Committed>()
            .remove::<Sent>();
    }
}

type Applications<'w, 's> =
    Query<'w, 's, (Entity, &'static Committed), (With<Sent>, Without<Initializing>)>;

fn apply(
    pending: Applications,
    active: Writes,
    catalog: Catalog,
    proposals: Query<(Entity, &ChildOf), With<Proposal>>,
    mut commands: Commands,
) {
    for (operation, committed) in &pending {
        let Ok((parent, _, _, name, creation, _, binding, sync)) = active.get(operation) else {
            continue;
        };
        let root = parent.parent();
        let commit = &committed.0;
        let mut confirmations = Vec::new();
        let target = if let Some(binding) = binding {
            let (account, _) = catalog.account(root).unwrap();
            commands
                .entity(account)
                .insert((binding.clone(), Revision(commit.receipt.revision)));
            if !binding.is_linked() {
                for (_, id, _, _, owner, revision, _) in &catalog.profiles {
                    if owner.0 != account {
                        continue;
                    }
                    let profile_id = id.0.parse().unwrap();
                    let profile = catalog.profile(root, profile_id).unwrap();
                    let revision = revision.0 + 1;
                    commands
                        .entity(profile)
                        .insert((SyncEnabled(false), Revision(revision)));
                    confirmations.push((
                        profile,
                        Receipt {
                            operation_id: commit.receipt.operation_id,
                            entity_id: profile_id,
                            revision,
                        },
                    ));
                }
            }
            account
        } else if creation {
            let Some((entity, _)) = proposals
                .iter()
                .find(|(_, parent)| parent.parent() == operation)
            else {
                continue;
            };
            commands
                .entity(entity)
                .remove::<Proposal>()
                .remove::<ChildOf>()
                .insert((ProfileLabel, Revision(commit.receipt.revision)));
            entity
        } else {
            let Some(entity) = catalog.profile(root, commit.receipt.entity_id) else {
                continue;
            };
            commands
                .entity(entity)
                .insert(Revision(commit.receipt.revision));
            if let Some(sync) = sync {
                commands.entity(entity).insert(*sync);
            } else {
                commands
                    .entity(entity)
                    .insert(Name::new(name.unwrap().as_str().to_string()));
            }
            entity
        };
        confirmations.push((target, commit.receipt.clone()));
        commands
            .entity(operation)
            .insert((Confirmed(confirmations), Accepted(commit.clone())))
            .remove::<Committed>()
            .remove::<Sent>();
        commands.entity(root).remove::<Writing>().remove::<Error>();
    }
}

fn replayed(
    pending: Query<(Entity, &ChildOf, &AwaitReload), With<Operation>>,
    ready: Query<(), (With<Root>, With<Ready>)>,
    catalog: Catalog,
    mut commands: Commands,
) {
    for (operation, parent, commit) in &pending {
        let root = parent.parent();
        if !ready.contains(root) {
            continue;
        }
        if catalog
            .account(root)
            .is_some_and(|(_, id)| id == commit.0.receipt.entity_id)
        {
            commands
                .entity(operation)
                .insert(Accepted(commit.0.clone()))
                .remove::<AwaitReload>();
        } else if let Some(entity) = catalog.profile(root, commit.0.receipt.entity_id) {
            let directory = catalog.profiles.get(entity).unwrap().2.clone();
            commands
                .entity(operation)
                .insert((directory, Accepted(commit.0.clone())))
                .remove::<AwaitReload>();
        }
    }
}

fn reply(
    pending: AcceptedWrites,
    mut replies: MessageWriter<MutationCompleted>,
    mut commands: Commands,
) {
    for (operation, parent, accepted, directory, creation, initializing) in &pending {
        if !initializing {
            replies.write(MutationCompleted {
                root: parent.parent(),
                operation_id: accepted.0.receipt.operation_id,
                result: Ok(accepted.0.receipt.clone()),
                directory_id: directory.map(|directory| directory.0.clone()),
                created: creation,
            });
        }
        commands.trigger(OperationCompleted {
            entity: operation,
            result: Ok(()),
        });
        commands.entity(operation).despawn();
    }
}

type WritableRoots<'w, 's> =
    Query<'w, 's, &'static mut Order, (With<Root>, With<Ready>, Without<Error>)>;

type AvailableRoots<'w, 's> =
    Query<'w, 's, (), (With<Root>, With<Ready>, Without<Writing>, Without<Error>)>;

type Generations<'w, 's> =
    Query<'w, 's, &'static mut Revision, (With<Root>, Without<Profile>, Without<Account>)>;

type AcceptedWrites<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Accepted,
        Option<&'static ProfileId>,
        Has<Creation>,
        Has<Initializing>,
    ),
    Without<Confirmed>,
>;
