use crate::account::Set;
use crate::catalog::{Catalog, Error, Loading, Root};
use crate::profile::{Account, Failure, Owner, Profile};
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use uuid::Uuid;
use vmux_ecs::record::{CaptureRequest, Captured, Confirmed, Persisted, Receipt, Record, Scope};
use vmux_ecs::{
    Active, Operation, OperationCompleted, Order, ProfileId, ProfileLabel, Ready, Revision,
    Uuid as Id,
};
use vmux_profile::Profile as Directory;

pub(crate) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CreateRequest>()
            .add_message::<RenameRequest>()
            .add_message::<MutationCompleted>()
            .add_message::<WriteRequest>()
            .add_message::<WriteCompleted>()
            .add_systems(Update, (create, rename).chain().in_set(Set::Receive))
            .add_systems(Update, prepare.in_set(Set::Prepare))
            .add_systems(Update, write.in_set(Set::Write))
            .add_systems(Update, apply.in_set(Set::Apply))
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
pub struct MutationCompleted {
    pub root: Entity,
    pub operation_id: Uuid,
    pub result: Result<Receipt, Failure>,
    pub directory_id: Option<String>,
    pub created: bool,
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
pub(crate) struct Target(pub(crate) Uuid);

#[derive(Component)]
struct Prepared;

#[derive(Component)]
struct Sent;

#[derive(Component)]
pub(crate) struct Writing;

#[derive(Component)]
pub(crate) struct EnsureDirectory(pub(crate) String);

#[derive(Component)]
struct AwaitReload(Commit);

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
            return Err(Failure::InvalidName);
        }
        for (operation, owner) in &self.operations {
            if owner.parent() == root && operation.0 == id.to_string() {
                return Err(Failure::Conflict);
            }
        }
        let mut order = self.roots.get_mut(root).map_err(|_| Failure::NotReady)?;
        order.0 = order.0.checked_add(1).ok_or(Failure::NotReady)?;
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

type Pending<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Name,
        &'static Order,
        Option<&'static Target>,
        Has<Creation>,
    ),
    (
        With<Operation>,
        Without<Prepared>,
        Without<Accepted>,
        Without<AwaitReload>,
    ),
>;

fn prepare(
    pending: Pending,
    catalog: Catalog,
    available: AvailableRoots,
    mut replies: MessageWriter<MutationCompleted>,
    identities: Query<&Id>,
    mut commands: Commands,
) {
    let mut ordered: Vec<_> = pending.iter().collect();
    ordered.sort_by_key(|(_, _, _, order, _, _)| order.0);
    let mut busy = std::collections::HashSet::new();
    for (operation, parent, name, _, target, creation) in ordered {
        let root = parent.parent();
        if !available.contains(root) || !busy.insert(root) {
            continue;
        }
        let Some((account, scope)) = catalog.account(root) else {
            continue;
        };
        let (target_id, directory, revision) = if creation {
            let base = Directory::named(name.as_str()).into_id();
            let mut directory = base.clone();
            for suffix in 2usize.. {
                if catalog.directory(account, &directory).is_none() {
                    break;
                }
                directory = format!("{base}-{suffix}");
            }
            (Uuid::new_v4(), directory, 1)
        } else {
            let target_id = target.unwrap().0;
            let Some(entity) = catalog.profile(root, target_id) else {
                replies.write(MutationCompleted {
                    root,
                    operation_id: identities.get(operation).unwrap().0.parse().unwrap(),
                    result: Err(Failure::UnknownProfile),
                    directory_id: None,
                    created: false,
                });
                commands.entity(operation).despawn();
                continue;
            };
            let (_, _, directory, _, _, revision, _) = catalog.profiles.get(entity).unwrap();
            let Some(revision) = revision
                .0
                .checked_add(1)
                .filter(|revision| *revision <= i64::MAX as u64)
            else {
                replies.write(MutationCompleted {
                    root,
                    operation_id: identities.get(operation).unwrap().0.parse().unwrap(),
                    result: Err(Failure::NotReady),
                    directory_id: None,
                    created: false,
                });
                commands.entity(operation).despawn();
                continue;
            };
            (target_id, directory.0.clone(), revision)
        };
        commands.entity(root).insert(Writing);
        commands.entity(operation).insert((
            Prepared,
            Target(target_id),
            ProfileId(directory),
            Revision(revision),
            CaptureRequest {
                scope_id: scope,
                entities: catalog.entities(root),
            },
        ));
    }
}

type Captures<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Id,
        &'static Target,
        &'static mut Captured,
        Option<&'static Name>,
        Option<&'static ProfileId>,
        Option<&'static Revision>,
        Option<&'static EnsureDirectory>,
        Has<Creation>,
        Has<Initializing>,
    ),
    Without<Sent>,
>;

fn write(
    mut captures: Captures,
    generations: Query<&Revision, With<Root>>,
    mut requests: MessageWriter<WriteRequest>,
    mut replies: MessageWriter<MutationCompleted>,
    mut commands: Commands,
) {
    for (
        entity,
        parent,
        id,
        target,
        mut captured,
        name,
        directory,
        revision,
        ensure,
        creation,
        initializing,
    ) in &mut captures
    {
        let root = parent.parent();
        let Ok(generation) = generations.get(root) else {
            continue;
        };
        let value = std::mem::replace(&mut captured.0, Err("capture consumed".into()));
        let mut records = match value {
            Ok(records) => records,
            Err(error) => {
                let failure = Failure::Storage(error);
                if initializing {
                    commands.entity(root).insert(Error(failure));
                } else {
                    replies.write(MutationCompleted {
                        root,
                        operation_id: id.0.parse().unwrap(),
                        result: Err(failure),
                        directory_id: None,
                        created: creation,
                    });
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
            records.push(Profile::record(
                records[0].scope_id,
                target.0,
                &directory.unwrap().0,
                name,
            ));
            serde_json::json!({"create": name}).to_string()
        } else {
            let name = name.unwrap().as_str();
            for record in &mut records {
                if record.id != target.0 {
                    continue;
                }
                record.revision = revision.unwrap().0;
                for component in &mut record.components {
                    if component.key == "name" {
                        component.value = serde_json::json!(name);
                    }
                }
            }
            serde_json::json!({"rename": target.0, "name": name}).to_string()
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
                Some(directory.unwrap().0.clone())
            } else {
                ensure.map(|directory| directory.0.clone())
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
        Option<&'static Name>,
        Option<&'static ProfileId>,
        Has<Creation>,
        Has<Initializing>,
    ),
    With<Sent>,
>;

fn apply(
    mut events: MessageMutator<WriteCompleted>,
    active: Writes,
    catalog: Catalog,
    mut generations: Generations,
    mut commands: Commands,
    mut replies: MessageWriter<MutationCompleted>,
) {
    for event in events.read() {
        let Some(result) = event.result.take() else {
            continue;
        };
        let Ok((parent, id, name, directory, creation, initializing)) = active.get(event.entity)
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
                    WriteFailure::Conflict => Failure::Conflict,
                    WriteFailure::Storage(error) => Failure::Storage(error),
                };
                if !initializing {
                    replies.write(MutationCompleted {
                        root,
                        operation_id: id.0.parse().unwrap(),
                        result: Err(failure.clone()),
                        directory_id: None,
                        created: creation,
                    });
                }
                commands.entity(root).remove::<Writing>();
                if failure == Failure::Conflict {
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
        if commit.receipt.operation_id.to_string() != id.0 || commit.receipt.revision == 0 {
            if !initializing {
                replies.write(MutationCompleted {
                    root,
                    operation_id: id.0.parse().unwrap(),
                    result: Err(Failure::InvalidStore),
                    directory_id: None,
                    created: creation,
                });
            }
            commands
                .entity(root)
                .insert(Error(Failure::InvalidStore))
                .remove::<Writing>()
                .remove::<Loading>();
            commands.entity(event.entity).despawn();
            continue;
        }
        if commit.replayed {
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
        let Some((account, scope)) = catalog.account(root) else {
            continue;
        };
        let mut confirmations = Vec::new();
        if initializing {
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
                .entity(event.entity)
                .insert((Confirmed(confirmations), Accepted(commit)))
                .remove::<Sent>();
        } else {
            let target = if creation {
                commands
                    .spawn((
                        Profile,
                        ProfileLabel,
                        Id(commit.receipt.entity_id.to_string()),
                        directory.unwrap().clone(),
                        Name::new(name.unwrap().as_str().to_string()),
                        Scope(scope),
                        Revision(commit.receipt.revision),
                        Persisted(0),
                        Owner(account),
                    ))
                    .id()
            } else {
                let Some(entity) = catalog.profile(root, commit.receipt.entity_id) else {
                    continue;
                };
                commands.entity(entity).insert((
                    Name::new(name.unwrap().as_str().to_string()),
                    Revision(commit.receipt.revision),
                ));
                entity
            };
            commands
                .entity(event.entity)
                .insert((
                    Confirmed(vec![(target, commit.receipt.clone())]),
                    Accepted(commit),
                ))
                .remove::<Sent>();
            commands.entity(root).remove::<Writing>().remove::<Error>();
        }
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
        if let Some(entity) = catalog.profile(root, commit.0.receipt.entity_id) {
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
