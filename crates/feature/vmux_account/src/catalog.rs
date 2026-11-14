use crate::account::Set;
use crate::mutations::{EnsureDirectory, Initializing, Target, Writing};
use crate::profile::{Account, Failure, Owner, Profile};
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use uuid::Uuid;
use vmux_ecs::record::{
    CaptureRequest, Graph, Persisted, Record, RestoreRequest, Restored, Scope, StoreOwner,
};
use vmux_ecs::{
    Active, Operation, OperationCompleted, Order, ProfileId, ProfileLabel, Ready, Revision,
    Uuid as Id,
};
use vmux_profile::Profile as Directory;

pub(crate) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ReadRequest>()
            .add_message::<ReadCompleted>()
            .add_message::<RetryRequest>()
            .add_systems(Update, (retry, load, receive).chain().in_set(Set::Receive))
            .add_systems(Update, seed.in_set(Set::Seed))
            .add_systems(Update, adopt.in_set(Set::Adopt));
    }
}

#[derive(Component)]
pub struct Root;

impl Root {
    pub fn at(data: impl Into<std::path::PathBuf>, active: &str) -> impl Bundle {
        (
            Self,
            Name::new("Account store"),
            Location(data.into()),
            Selection(Directory::named(active).into_id()),
            Revision(0),
            Order(0),
        )
    }
}

#[derive(Component)]
#[component(immutable)]
pub struct Location(pub std::path::PathBuf);

#[derive(Component)]
#[component(immutable)]
pub struct Selection(pub String);

#[derive(Component)]
pub struct Error(pub Failure);

#[derive(Message)]
pub struct RetryRequest(pub Entity);

#[derive(Message)]
pub struct ReadRequest {
    pub entity: Entity,
    pub root: Entity,
}

#[derive(Message)]
pub struct ReadCompleted {
    pub entity: Entity,
    pub result: Option<Result<Loaded, String>>,
}

pub struct Loaded {
    pub generation: u64,
    pub records: Vec<Record>,
    pub directories: Vec<DirectoryObservation>,
}

pub struct DirectoryObservation {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Component)]
pub(crate) struct Loading;

#[derive(Component)]
struct Load;

#[derive(Component)]
struct Fresh;

#[derive(Component)]
struct Observed;

#[derive(Component)]
struct ReadGeneration(u64);

type AccountRows<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Id,
        &'static Scope,
        Option<&'static ChildOf>,
    ),
    (With<Account>, Without<Root>),
>;

type ProfileRows<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Id,
        &'static ProfileId,
        &'static Name,
        &'static Owner,
        &'static Revision,
        &'static Scope,
    ),
    (With<Profile>, Without<Root>),
>;

#[derive(SystemParam)]
pub(crate) struct Catalog<'w, 's> {
    accounts: AccountRows<'w, 's>,
    pub(crate) profiles: ProfileRows<'w, 's>,
    pub(crate) selections: Query<'w, 's, &'static Selection, With<Root>>,
}

impl Catalog<'_, '_> {
    pub(crate) fn account(&self, root: Entity) -> Option<(Entity, Uuid)> {
        for (entity, id, _, parent) in &self.accounts {
            if parent.is_some_and(|parent| parent.parent() == root) {
                return Some((entity, id.0.parse().ok()?));
            }
        }
        None
    }

    pub(crate) fn entities(&self, root: Entity) -> Vec<Entity> {
        let Some((account, _)) = self.account(root) else {
            return Vec::new();
        };
        let mut entities = vec![account];
        for (entity, _, _, _, owner, _, _) in &self.profiles {
            if owner.0 == account {
                entities.push(entity);
            }
        }
        entities
    }

    pub(crate) fn profile(&self, root: Entity, id: Uuid) -> Option<Entity> {
        let (account, _) = self.account(root)?;
        for (entity, identity, _, _, owner, _, _) in &self.profiles {
            if owner.0 == account && identity.0 == id.to_string() {
                return Some(entity);
            }
        }
        None
    }

    pub(crate) fn directory(&self, account: Entity, id: &str) -> Option<Entity> {
        for (entity, _, directory, _, owner, _, _) in &self.profiles {
            if owner.0 == account && directory.0 == id {
                return Some(entity);
            }
        }
        None
    }

    pub(crate) fn validate(&self, entities: &[Entity]) -> Result<(Entity, Uuid), Failure> {
        let mut account = None;
        for entity in entities {
            if let Ok((entity, id, scope, _)) = self.accounts.get(*entity) {
                let id: Uuid = id.0.parse().map_err(|_| Failure::InvalidStore)?;
                if account.is_some() || scope.0 != id {
                    return Err(Failure::InvalidStore);
                }
                account = Some((entity, id));
            }
        }
        let (account, scope) = account.ok_or(Failure::InvalidStore)?;
        let mut directories = std::collections::HashSet::new();
        for entity in entities {
            if *entity == account {
                continue;
            }
            let Ok((_, _, directory, _, owner, _, stored_scope)) = self.profiles.get(*entity)
            else {
                return Err(Failure::InvalidStore);
            };
            if owner.0 != account
                || stored_scope.0 != scope
                || Directory::named(&directory.0).id() != directory.0
                || !directories.insert(&directory.0)
            {
                return Err(Failure::InvalidStore);
            }
        }
        Ok((account, scope))
    }
}

fn retry(mut requests: MessageReader<RetryRequest>, failed: FailedRoots, mut commands: Commands) {
    for RetryRequest(root) in requests.read() {
        if failed.contains(*root) {
            commands
                .entity(*root)
                .remove::<Error>()
                .remove::<Ready>()
                .remove::<Loading>();
        }
    }
}

fn load(roots: UnloadedRoots, mut requests: MessageWriter<ReadRequest>, mut commands: Commands) {
    for root in &roots {
        let entity = commands
            .spawn((
                Operation::bundle("account.load").unwrap(),
                Load,
                ChildOf(root),
            ))
            .id();
        commands.entity(root).insert(Loading);
        requests.write(ReadRequest { entity, root });
    }
}

fn receive(
    mut results: MessageMutator<ReadCompleted>,
    active: Query<&ChildOf, With<Load>>,
    roots: Query<(), With<Root>>,
    mut commands: Commands,
) {
    for result in results.read() {
        let Some(value) = result.result.take() else {
            continue;
        };
        let Ok(parent) = active.get(result.entity) else {
            continue;
        };
        let root = parent.parent();
        if !roots.contains(root) {
            continue;
        }
        let loaded = match value {
            Ok(loaded) => loaded,
            Err(error) => {
                commands
                    .entity(root)
                    .insert(Error(Failure::Storage(error.clone())))
                    .remove::<Loading>();
                commands.trigger(OperationCompleted {
                    entity: result.entity,
                    result: Err(error),
                });
                continue;
            }
        };
        if loaded.generation > 0 && loaded.records.is_empty() {
            commands
                .entity(root)
                .insert(Error(Failure::InvalidStore))
                .remove::<Loading>();
            commands.entity(result.entity).despawn();
            continue;
        }
        for directory in loaded.directories {
            let mut observation =
                commands.spawn((Observed, ProfileId(directory.id), ChildOf(result.entity)));
            if let Some(name) = directory.name {
                observation.insert(Name::new(name));
            }
        }
        commands
            .entity(result.entity)
            .insert(ReadGeneration(loaded.generation));
        if loaded.records.is_empty() {
            commands.entity(result.entity).insert(Fresh);
        } else {
            commands
                .entity(result.entity)
                .insert(RestoreRequest(Graph(loaded.records)));
        }
    }
}

fn seed(
    pending: Seeds,
    observations: Query<(&ChildOf, &ProfileId, Option<&Name>), With<Observed>>,
    selections: Query<&Selection, With<Root>>,
    mut commands: Commands,
) {
    for (operation, parent) in &pending {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        let account = Uuid::new_v4();
        let mut records = vec![Account::record(account)];
        let mut selected = false;
        for (owner, directory, name) in &observations {
            if owner.parent() != operation {
                continue;
            }
            let label = match name {
                Some(name) if !name.as_str().trim().is_empty() => name.as_str().trim().to_string(),
                _ => Profile::label(&directory.0),
            };
            selected |= directory.0 == selection.0;
            records.push(Profile::record(
                account,
                Uuid::new_v4(),
                &directory.0,
                &label,
            ));
        }
        if !selected {
            records.push(Profile::record(
                account,
                Uuid::new_v4(),
                &selection.0,
                &Profile::label(&selection.0),
            ));
            commands
                .entity(operation)
                .insert(EnsureDirectory(selection.0.clone()));
        }
        commands
            .entity(operation)
            .insert(RestoreRequest(Graph(records)));
    }
}

fn adopt(
    pending: Restores,
    observations: Query<(&ChildOf, &ProfileId, Option<&Name>), With<Observed>>,
    catalog: Catalog,
    mut generations: Generations,
    mut commands: Commands,
) {
    for (operation, parent, restored, generation, fresh) in &pending {
        let root = parent.parent();
        let entities = match &restored.0 {
            Ok(entities) => entities,
            Err(_) => {
                commands
                    .entity(root)
                    .insert(Error(Failure::InvalidStore))
                    .remove::<Loading>();
                commands.entity(operation).despawn();
                continue;
            }
        };
        let (account, scope) = match catalog.validate(entities) {
            Ok(account) => account,
            Err(error) => {
                commands
                    .entity(root)
                    .insert(Error(error))
                    .remove::<Loading>();
                commands.entity(operation).despawn();
                continue;
            }
        };
        let Ok(selection) = catalog.selections.get(root) else {
            continue;
        };
        let old = catalog.account(root);
        let mut captured = entities.clone();
        let mut changed = fresh;
        let mut selected = catalog.directory(account, &selection.0).is_some();
        for (owner, directory, name) in &observations {
            if owner.parent() != operation {
                continue;
            }
            if Directory::named(&directory.0).id() != directory.0 {
                commands
                    .entity(root)
                    .insert(Error(Failure::InvalidStore))
                    .remove::<Loading>();
                commands.entity(operation).despawn();
                captured.clear();
                break;
            }
            if catalog.directory(account, &directory.0).is_some() {
                continue;
            }
            let label = match name {
                Some(name) if !name.as_str().trim().is_empty() => name.as_str().trim().to_string(),
                _ => Profile::label(&directory.0),
            };
            let entity = commands
                .spawn((
                    Profile,
                    Id(Uuid::new_v4().to_string()),
                    directory.clone(),
                    Name::new(label),
                    Scope(scope),
                    Revision(1),
                    Persisted(0),
                    Owner(account),
                ))
                .id();
            captured.push(entity);
            selected |= directory.0 == selection.0;
            changed = true;
        }
        if captured.is_empty() {
            continue;
        }
        if !selected {
            let entity = commands
                .spawn((
                    Profile,
                    Id(Uuid::new_v4().to_string()),
                    ProfileId(selection.0.clone()),
                    Name::new(Profile::label(&selection.0)),
                    Scope(scope),
                    Revision(1),
                    Persisted(0),
                    Owner(account),
                ))
                .id();
            captured.push(entity);
            commands
                .entity(operation)
                .insert(EnsureDirectory(selection.0.clone()));
            changed = true;
        }
        commands
            .entity(account)
            .remove::<StoreOwner>()
            .insert(ChildOf(root));
        if let Some((old, _)) = old
            && old != account
        {
            commands.entity(old).despawn();
        }
        if let Ok(mut revision) = generations.get_mut(root) {
            revision.0 = generation.0;
        }
        commands
            .entity(operation)
            .remove::<Restored>()
            .remove::<Load>();
        if changed {
            if fresh {
                for entity in &captured {
                    commands.entity(*entity).insert(Persisted(0));
                }
            }
            commands.entity(root).insert(Writing);
            commands.entity(operation).insert((
                Initializing,
                Target(scope),
                CaptureRequest {
                    scope_id: scope,
                    entities: captured,
                },
            ));
        } else {
            for entity in &captured {
                if let Ok((_, _, directory, _, _, _, _)) = catalog.profiles.get(*entity) {
                    let mut profile = commands.entity(*entity);
                    profile.insert(ProfileLabel);
                    if directory.0 == selection.0 {
                        profile.insert(Active);
                    }
                }
            }
            commands
                .entity(root)
                .insert(Ready)
                .remove::<Loading>()
                .remove::<Error>();
            commands.entity(operation).despawn();
        }
    }
}

type FailedRoots<'w, 's> = Query<'w, 's, (), (With<Root>, With<Error>, Without<Writing>)>;

type UnloadedRoots<'w, 's> =
    Query<'w, 's, Entity, (With<Root>, Without<Ready>, Without<Loading>, Without<Error>)>;

type Seeds<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static ChildOf),
    (With<Fresh>, Without<RestoreRequest>, Without<Restored>),
>;

type Restores<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Restored,
        &'static ReadGeneration,
        Has<Fresh>,
    ),
    With<Load>,
>;

type Generations<'w, 's> =
    Query<'w, 's, &'static mut Revision, (With<Root>, Without<Profile>, Without<Account>)>;
