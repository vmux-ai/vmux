use crate::account::Set;
use crate::mutations::{EnsureDirectory, Initializing, Proposal, Target, Writing};
use crate::profile::{Account, Binding, Failure, Owner, Profile, SyncEnabled};
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use uuid::Uuid;
use vmux_ecs::record::{
    CaptureRequest, Graph, Persisted, Record, RestoreRequest, Restored, Scope, StoreOwner,
};
use vmux_ecs::{
    Active, EntityTarget, Operation, OperationCompleted, Order, ProfileId, ProfileLabel, Ready,
    Revision, Uuid as Id,
};
use vmux_profile::Profile as Directory;

pub(crate) struct CatalogPlugin;

impl bevy_app::Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ReadRequest>()
            .add_message::<ReadCompleted>()
            .add_message::<RetryRequest>()
            .add_systems(Update, (retry, load, receive).chain().in_set(Set::Receive))
            .add_systems(Update, seed.in_set(Set::Seed))
            .add_systems(
                Update,
                (validate, reconcile, adopt).chain().in_set(Set::Adopt),
            );
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
    (With<Profile>, Without<Root>, Without<Proposal>),
>;

#[derive(SystemParam)]
pub(crate) struct Catalog<'w, 's> {
    accounts: AccountRows<'w, 's>,
    pub(crate) profiles: ProfileRows<'w, 's>,
    pub(crate) selections: Query<'w, 's, &'static Selection, With<Root>>,
    bindings: Query<'w, 's, &'static Binding, With<Account>>,
    consent: Query<'w, 's, &'static SyncEnabled, With<Profile>>,
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

    pub(crate) fn available_directory(&self, account: Entity, name: &str) -> String {
        let base = Directory::named(name).into_id();
        let mut directory = base.clone();
        for suffix in 2usize.. {
            if self.directory(account, &directory).is_none() {
                break;
            }
            directory = format!("{base}-{suffix}");
        }
        directory
    }

    pub(crate) fn validate(&self, entities: &[Entity]) -> Result<(Entity, Uuid), Failure> {
        let mut account = None;
        for entity in entities {
            if let Ok((entity, id, scope, _)) = self.accounts.get(*entity) {
                let id: Uuid = id.0.parse().map_err(|_| Failure::invalid_store())?;
                if account.is_some() || scope.0 != id {
                    return Err(Failure::invalid_store());
                }
                account = Some((entity, id));
            }
        }
        let (account, scope) = account.ok_or(Failure::invalid_store())?;
        let binding = self.bindings.get(account).ok();
        if binding.is_some_and(|binding| !binding.is_valid()) {
            return Err(Failure::invalid_store());
        }
        let mut directories = std::collections::HashSet::new();
        for entity in entities {
            if *entity == account {
                continue;
            }
            let Ok((_, _, directory, _, owner, _, stored_scope)) = self.profiles.get(*entity)
            else {
                return Err(Failure::invalid_store());
            };
            if owner.0 != account
                || (self.consent.get(*entity).is_ok_and(|consent| consent.0)
                    && !binding.is_some_and(Binding::is_linked))
                || stored_scope.0 != scope
                || Directory::named(&directory.0).id() != directory.0
                || !directories.insert(&directory.0)
            {
                return Err(Failure::invalid_store());
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
                    .insert(Error(Failure::storage(error.clone())))
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
                .insert(Error(Failure::invalid_store()))
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
        let scope = Uuid::new_v4();
        let account = commands
            .spawn((Account::bundle(scope), StoreOwner(operation)))
            .id();
        let mut entities = vec![account];
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
            entities.push(
                commands
                    .spawn(Profile::bundle(
                        scope,
                        Uuid::new_v4(),
                        &directory.0,
                        &label,
                        account,
                    ))
                    .id(),
            );
        }
        if !selected {
            entities.push(
                commands
                    .spawn(Profile::bundle(
                        scope,
                        Uuid::new_v4(),
                        &selection.0,
                        &Profile::label(&selection.0),
                        account,
                    ))
                    .id(),
            );
            commands
                .entity(operation)
                .insert(EnsureDirectory(selection.0.clone()));
        }
        commands.entity(operation).insert(Restored(Ok(entities)));
    }
}

#[derive(Component)]
struct Validated;

fn validate(
    pending: Restores,
    observations: Query<(&ChildOf, &ProfileId, Option<&Name>), With<Observed>>,
    catalog: Catalog,
    mut commands: Commands,
) {
    for (operation, parent, restored, _, _) in &pending {
        let root = parent.parent();
        let entities = match &restored.0 {
            Ok(entities) => entities,
            Err(_) => {
                commands
                    .entity(root)
                    .insert(Error(Failure::invalid_store()))
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
        let mut valid = true;
        for (owner, directory, _) in &observations {
            if owner.parent() != operation {
                continue;
            }
            if Directory::named(&directory.0).id() != directory.0 {
                commands
                    .entity(root)
                    .insert(Error(Failure::invalid_store()))
                    .remove::<Loading>();
                commands.entity(operation).despawn();
                valid = false;
                break;
            }
        }
        if valid {
            commands.entity(operation).insert((
                EntityTarget::<Account>::new(account),
                Scope(scope),
                Validated,
            ));
        }
    }
}

#[derive(Component)]
struct CaptureRequired;

fn reconcile(
    mut pending: Query<(Entity, &ChildOf, &mut Restored, Has<Fresh>), With<Load>>,
    validated: Query<(&EntityTarget<Account>, &Scope), With<Validated>>,
    observations: Query<(&ChildOf, &ProfileId, Option<&Name>), With<Observed>>,
    catalog: Catalog,
    mut commands: Commands,
) {
    for (operation, parent, mut restored, fresh) in &mut pending {
        let root = parent.parent();
        let Ok((target, scope)) = validated.get(operation) else {
            continue;
        };
        let account = target.entity();
        let scope = scope.0;
        let Ok(selection) = catalog.selections.get(root) else {
            continue;
        };
        let entities = restored.0.as_ref().unwrap();
        let mut captured = entities.clone();
        let mut changed = fresh;
        let mut selected = catalog.directory(account, &selection.0).is_some();
        for (owner, directory, name) in &observations {
            if owner.parent() != operation {
                continue;
            }
            if catalog.directory(account, &directory.0).is_some() {
                continue;
            }
            let label = match name {
                Some(name) if !name.as_str().trim().is_empty() => name.as_str().trim().to_string(),
                _ => Profile::label(&directory.0),
            };
            let entity = commands
                .spawn(Profile::bundle(
                    scope,
                    Uuid::new_v4(),
                    &directory.0,
                    &label,
                    account,
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
                .spawn(Profile::bundle(
                    scope,
                    Uuid::new_v4(),
                    &selection.0,
                    &Profile::label(&selection.0),
                    account,
                ))
                .id();
            captured.push(entity);
            commands
                .entity(operation)
                .insert(EnsureDirectory(selection.0.clone()));
            changed = true;
        }
        restored.0 = Ok(captured);
        if changed {
            commands.entity(operation).insert(CaptureRequired);
        }
    }
}

fn adopt(
    pending: Restores,
    validated: Query<(&EntityTarget<Account>, &Scope), With<Validated>>,
    required: Query<(), With<CaptureRequired>>,
    catalog: Catalog,
    mut generations: Generations,
    mut commands: Commands,
) {
    for (operation, parent, restored, generation, fresh) in &pending {
        let root = parent.parent();
        let Ok((target, scope)) = validated.get(operation) else {
            continue;
        };
        let entities = restored.0.as_ref().unwrap();
        let account = target.entity();
        let scope = scope.0;
        let Ok(selection) = catalog.selections.get(root) else {
            continue;
        };
        let old = catalog.account(root);
        let captured = entities;
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
            .remove::<Validated>()
            .remove::<EntityTarget<Account>>()
            .remove::<Scope>()
            .remove::<Restored>()
            .remove::<Load>();
        if required.contains(operation) {
            if fresh {
                for entity in captured {
                    commands.entity(*entity).insert(Persisted(0));
                }
            }
            commands.entity(root).insert(Writing);
            commands.entity(operation).insert((
                Initializing,
                Target(scope),
                CaptureRequest {
                    scope_id: scope,
                    entities: captured.clone(),
                },
            ));
        } else {
            for entity in captured {
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
    (
        With<Fresh>,
        With<Load>,
        Without<RestoreRequest>,
        Without<Restored>,
    ),
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
