use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::query::Changed as ComponentChanged;
use bevy_ecs::system::{Commands as WorldCommands, SystemParam};
use vmux_ecs::{
    Cwd, EntityTarget, IdentityIndexPlugin, Operation, OperationCompleted, OperationPlugin,
    OperationState, OperationTarget, Revision,
};

use crate::{ProjectId, Record, Snapshot};

pub struct ProjectPlugin;

impl Plugin for ProjectPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.add_plugins(IdentityIndexPlugin::<ProjectId>::default())
            .init_resource::<Clock>()
            .add_message::<LoadRequest>()
            .add_message::<LoadCompleted>()
            .add_message::<WriteTaskRequest>()
            .add_message::<WriteResult>()
            .add_message::<AliasRequest>()
            .add_message::<AliasCompleted>()
            .add_message::<WriteCompleted>()
            .add_message::<ProjectSaved>()
            .add_message::<ProjectChanged>()
            .configure_sets(Update, (StoreSet::Load, StoreSet::Write).chain())
            .add_systems(
                Update,
                (cancel, reset, load, restore)
                    .chain()
                    .in_set(StoreSet::Load),
            )
            .add_systems(
                Update,
                (aliases, finish, write, dispatch, prune)
                    .chain()
                    .in_set(StoreSet::Write),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StoreSet {
    Load,
    Write,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Root(pub PathBuf);

#[derive(Component)]
pub struct Project;

#[derive(Component)]
pub struct Registered;

#[derive(Component)]
pub struct Hidden;

#[derive(Component)]
pub struct Archived;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Repository(pub Option<String>);

#[derive(Component)]
pub struct Loaded;

#[derive(Component)]
struct LoadedDirectory(PathBuf);

#[derive(Component)]
struct Accepted {
    operation: Entity,
    owner: Entity,
    directory: PathBuf,
}

#[derive(Component)]
pub struct LoadError(pub Error);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Conflict,
    Duplicate,
    InvalidName,
    NotFound,
    Unavailable(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict => formatter.write_str("project revision conflict"),
            Self::Duplicate => formatter.write_str("project already exists"),
            Self::InvalidName => formatter.write_str("invalid project name"),
            Self::NotFound => formatter.write_str("project is unavailable"),
            Self::Unavailable(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub owner: Entity,
    pub directory: PathBuf,
    pub generation: u64,
}

#[derive(Resource, Default)]
struct Clock(u64);

impl Clock {
    fn ticket(&mut self, owner: Entity, directory: &Path) -> Ticket {
        self.0 = self
            .0
            .checked_add(1)
            .expect("project store generation exhausted");
        Ticket {
            owner,
            directory: directory.to_path_buf(),
            generation: self.0,
        }
    }
}

#[derive(Component)]
struct Loading(Ticket);

#[derive(Component)]
struct Writing {
    ticket: Ticket,
    operation: Entity,
}

#[derive(Component)]
struct Prepared {
    ticket: Ticket,
    requester: Entity,
}

#[derive(Component)]
struct Proposal;

#[derive(Component)]
struct RequestedProject(ProjectId);

#[derive(Component)]
struct SaveIntent;

#[derive(Component)]
struct RemoveIntent;

#[derive(Component)]
struct Archive(bool);

#[derive(Component)]
struct Dispatched;

#[derive(Component)]
struct Aliasing;

#[derive(Component)]
struct AliasesReady;

#[derive(Component)]
struct HideAlias;

#[derive(Message, Clone)]
pub struct LoadRequest {
    pub ticket: Ticket,
}

#[derive(Message)]
pub struct LoadCompleted {
    pub ticket: Ticket,
    pub result: Result<Snapshot, Error>,
}

#[derive(Component)]
pub struct WriteRequest {
    pub root: PathBuf,
    pub mutation: Mutation,
}

#[derive(Clone)]
pub enum Mutation {
    Save {
        project: ProjectSaved,
        expected: u64,
    },
    Remove(String),
    Archive {
        url: String,
        archived: bool,
    },
}

#[derive(Message, Clone)]
pub struct ProjectSaved {
    pub root: PathBuf,
    pub id: ProjectId,
    pub directory: PathBuf,
    pub name: String,
    pub repository: Option<String>,
    pub revision: u64,
}

#[derive(Message)]
pub struct ProjectChanged {
    pub root: PathBuf,
}

#[derive(Message)]
pub struct WriteCompleted {
    pub requester: Entity,
    pub result: Result<Option<ProjectId>, Error>,
}

#[derive(Message)]
pub struct WriteTaskRequest {
    pub ticket: Ticket,
    pub snapshot: Snapshot,
}

#[derive(Message)]
pub struct AliasRequest {
    pub ticket: Ticket,
    pub directory: PathBuf,
}

#[derive(Message)]
pub struct AliasCompleted {
    pub ticket: Ticket,
    pub result: Result<Vec<PathBuf>, Error>,
}

#[derive(Message)]
pub struct WriteResult {
    pub ticket: Ticket,
    pub result: Result<BTreeSet<String>, Error>,
}

type Proposals<'w, 's> = Query<
    'w,
    's,
    (
        &'static RequestedProject,
        &'static ChildOf,
        Option<&'static Name>,
        Option<&'static Cwd>,
        Option<&'static Repository>,
        Option<&'static Revision>,
        Has<SaveIntent>,
        Has<RemoveIntent>,
        Option<&'static Archive>,
        Option<&'static EntityTarget<Project>>,
    ),
    With<Proposal>,
>;

type Records<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static ProjectId,
        &'static Name,
        &'static Cwd,
        &'static Repository,
        &'static Revision,
    ),
    With<Registered>,
>;

#[derive(SystemParam)]
pub struct Registrations<'w, 's> {
    roots: RootStatus<'w, 's>,
    records: Records<'w, 's>,
    projects: Query<'w, 's, (Entity, &'static ChildOf, &'static ProjectId), With<Project>>,
    hidden: Query<'w, 's, (&'static ChildOf, &'static ProjectId), With<Hidden>>,
    archived: Query<'w, 's, (&'static ChildOf, &'static ProjectId), With<Archived>>,
    unused: Query<'w, 's, Entity, UnusedProjects>,
}

type RootStatus<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        Has<Loaded>,
        Option<&'static LoadError>,
    ),
>;
type ChangedRoots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        Option<&'static LoadedDirectory>,
        Option<&'static Loading>,
        Option<&'static Writing>,
    ),
    ComponentChanged<Root>,
>;
type UnloadedRoots = (Without<Loaded>, Without<Loading>, Without<LoadError>);
type IdleRoots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        Has<Loaded>,
        Option<&'static LoadError>,
    ),
    Without<Writing>,
>;
type UnusedProjects = (
    With<Project>,
    Without<Registered>,
    Without<Hidden>,
    Without<Archived>,
);

#[derive(SystemParam)]
struct LifecycleRoots<'w, 's> {
    changed: ChangedRoots<'w, 's>,
    unloaded: Query<'w, 's, (Entity, &'static Root), UnloadedRoots>,
    all: Query<'w, 's, (Entity, &'static Root)>,
}

impl Registrations<'_, '_> {
    pub fn ready(&self, root: &Path) -> Result<bool, Error> {
        let mut found = None;
        for (_, directory, loaded, error) in &self.roots {
            if directory.0 != root {
                continue;
            }
            if found.is_some() {
                return Err(Error::Duplicate);
            }
            if let Some(error) = error {
                return Err(error.0.clone());
            }
            found = Some(loaded);
        }
        Ok(found.unwrap_or(false))
    }

    pub fn snapshot(&self, root: &Path) -> Snapshot {
        let mut snapshot = Snapshot::default();
        for (_, parent, id, name, directory, repository, revision) in &self.records {
            if !self.owns(parent.parent(), root) {
                continue;
            }
            snapshot.projects.insert(
                id.0.clone(),
                Record::capture(name, directory, repository, revision),
            );
        }
        for (parent, id) in &self.hidden {
            if self.owns(parent.parent(), root) {
                snapshot.hidden.insert(id.0.clone());
            }
        }
        for (parent, id) in &self.archived {
            if self.owns(parent.parent(), root) {
                snapshot.archived.insert(id.0.clone());
            }
        }
        snapshot
    }

    pub fn get(&self, root: &Path, url: &str) -> Option<(&Name, &Cwd, &Repository, &Revision)> {
        for (_, parent, id, name, directory, repository, revision) in &self.records {
            if id.0 == url && self.owns(parent.parent(), root) {
                return Some((name, directory, repository, revision));
            }
        }
        None
    }

    pub fn directory(&self, root: &Path, url: &str) -> Result<Option<PathBuf>, Error> {
        self.ready(root)?;
        for (parent, id) in &self.hidden {
            if id.0 == url && self.owns(parent.parent(), root) {
                return Err(Error::NotFound);
            }
        }
        Ok(self
            .get(root, url)
            .map(|(_, directory, _, _)| directory.0.clone()))
    }

    pub fn url_for_directory(&self, root: &Path, directory: &Path) -> Option<String> {
        let mut selected = None;
        for (_, parent, id, _, cwd, _, _) in &self.records {
            if !self.owns(parent.parent(), root) || !directory.starts_with(&cwd.0) {
                continue;
            }
            let depth = cwd.0.components().count();
            if selected
                .as_ref()
                .is_none_or(|(_, previous)| depth > *previous)
            {
                selected = Some((id.0.clone(), depth));
            }
        }
        selected.map(|(url, _)| url)
    }

    fn entity(&self, owner: Entity, url: &str) -> Result<Option<Entity>, Error> {
        let mut selected = None;
        for (entity, parent, id) in &self.projects {
            if parent.parent() != owner || id.0 != url {
                continue;
            }
            if selected.is_some() {
                return Err(Error::Duplicate);
            }
            selected = Some(entity);
        }
        Ok(selected)
    }

    fn save_target(
        &self,
        owner: Entity,
        project: &ProjectSaved,
        expected: u64,
    ) -> Result<Option<Entity>, Error> {
        let target = self.entity(owner, &project.id.0)?;
        let (_, root, _, _) = self.roots.get(owner).map_err(|_| Error::NotFound)?;
        let name = project.name.trim();
        if name.is_empty() || name.len() > 256 || name.contains(['\0', '\n', '\r']) {
            return Err(Error::InvalidName);
        }
        let current = self
            .get(&root.0, &project.id.0)
            .map_or(Revision(0), |(_, _, _, revision)| *revision);
        if project.root != root.0 || current.0 != expected || current.next().is_none() {
            return Err(Error::Conflict);
        }
        if self.duplicate_directory(owner, &project.id.0, &project.directory) {
            return Err(Error::Duplicate);
        }
        Ok(target)
    }

    fn duplicate_directory(&self, owner: Entity, url: &str, directory: &Path) -> bool {
        for (_, parent, id, _, current, _, _) in &self.records {
            if parent.parent() == owner && id.0 != url && current.0 == directory {
                return true;
            }
        }
        false
    }

    fn owns(&self, entity: Entity, root: &Path) -> bool {
        self.roots
            .get(entity)
            .is_ok_and(|(_, directory, _, _)| directory.0 == root)
    }
}

fn cancel(
    accepted: Query<(Entity, &Accepted)>,
    prepared: Query<(Entity, &Prepared)>,
    roots: Query<&Root>,
    mut completed: MessageWriter<WriteCompleted>,
    mut commands: WorldCommands,
) {
    for (entity, request) in &accepted {
        if roots
            .get(request.owner)
            .is_ok_and(|root| root.0 == request.directory)
        {
            continue;
        }
        let error = Error::Unavailable("project owner was removed or replaced".to_string());
        commands.entity(entity).remove::<Accepted>();
        commands.trigger(OperationCompleted {
            entity,
            result: Err(error.to_string()),
        });
        completed.write(WriteCompleted {
            requester: entity,
            result: Err(error),
        });
    }
    for (entity, pending) in &prepared {
        if !roots
            .get(pending.ticket.owner)
            .is_ok_and(|root| root.0 == pending.ticket.directory)
        {
            commands.trigger(OperationCompleted {
                entity,
                result: Err("project owner was removed or replaced".into()),
            });
        }
    }
}

fn reset(
    roots: LifecycleRoots,
    children: Query<(Entity, &ChildOf), With<Project>>,
    mut commands: WorldCommands,
) {
    for (entity, root, loaded, loading, writing) in &roots.changed {
        let changed = loaded.is_some_and(|directory| directory.0 != root.0)
            || loading.is_some_and(|pending| pending.0.directory != root.0)
            || writing.is_some_and(|pending| pending.ticket.directory != root.0);
        if !changed {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Loaded, LoadedDirectory, LoadError, Loading, Writing)>();
        for (child, parent) in &children {
            if parent.parent() == entity {
                commands.entity(child).despawn();
            }
        }
    }
}

fn load(
    roots: LifecycleRoots,
    mut clock: ResMut<Clock>,
    mut requests: MessageWriter<LoadRequest>,
    mut commands: WorldCommands,
) {
    for (owner, root) in &roots.unloaded {
        if roots
            .all
            .iter()
            .any(|(other, directory)| other != owner && directory.0 == root.0)
        {
            commands
                .entity(owner)
                .insert((LoadError(Error::Duplicate), LoadedDirectory(root.0.clone())));
            continue;
        }
        let ticket = clock.ticket(owner, &root.0);
        requests.write(LoadRequest {
            ticket: ticket.clone(),
        });
        commands.entity(owner).insert(Loading(ticket));
    }
}

fn restore(
    mut results: MessageReader<LoadCompleted>,
    roots: Query<(&Root, &Loading)>,
    mut commands: WorldCommands,
) {
    for completed in results.read() {
        let Ok((root, pending)) = roots.get(completed.ticket.owner) else {
            continue;
        };
        if pending.0 != completed.ticket || root.0 != completed.ticket.directory {
            continue;
        }
        commands.entity(completed.ticket.owner).remove::<Loading>();
        let snapshot = match &completed.result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                commands
                    .entity(completed.ticket.owner)
                    .insert((LoadError(error.clone()), LoadedDirectory(root.0.clone())));
                continue;
            }
        };
        let mut ids = BTreeSet::new();
        ids.extend(snapshot.projects.keys().cloned());
        ids.extend(snapshot.hidden.iter().cloned());
        ids.extend(snapshot.archived.iter().cloned());
        for url in ids {
            let mut project = commands.spawn((
                Project,
                ProjectId(url.clone()),
                ChildOf(completed.ticket.owner),
            ));
            if let Some(record) = snapshot.projects.get(&url) {
                project.insert((
                    Registered,
                    Name::new(record.name.clone()),
                    Cwd(record.directory.clone()),
                    Repository(record.repository.clone()),
                    Revision(record.revision),
                ));
            }
            if snapshot.hidden.contains(&url) {
                project.insert(Hidden);
            }
            if snapshot.archived.contains(&url) {
                project.insert(Archived);
            }
        }
        commands
            .entity(completed.ticket.owner)
            .insert((Loaded, LoadedDirectory(root.0.clone())));
    }
}

fn write(
    roots: IdleRoots,
    requests: Query<(Entity, &WriteRequest, Has<Operation>)>,
    registrations: Registrations,
    mut clock: ResMut<Clock>,
    mut completed: MessageWriter<WriteCompleted>,
    mut commands: WorldCommands,
) {
    let mut handled = HashSet::new();
    for (owner, root, loaded, error) in &roots {
        if !loaded && error.is_none() {
            continue;
        }
        for (requester, request, operation) in &requests {
            if request.root != root.0 || !handled.insert(requester) {
                continue;
            }
            commands.entity(requester).remove::<WriteRequest>();
            let ready = registrations.ready(&root.0);
            let validation = match ready {
                Err(error) => Err(error),
                Ok(_) => match &request.mutation {
                    Mutation::Save { project, expected } => {
                        registrations.save_target(owner, project, *expected)
                    }
                    Mutation::Remove(url) | Mutation::Archive { url, .. } => {
                        registrations.entity(owner, url)
                    }
                },
            };
            let target = match validation {
                Ok(target) => target,
                Err(error) => {
                    commands.trigger(OperationCompleted {
                        entity: requester,
                        result: Err(error.to_string()),
                    });
                    completed.write(WriteCompleted {
                        requester,
                        result: Err(error),
                    });
                    continue;
                }
            };
            let ticket = clock.ticket(owner, &root.0);
            let accepted = commands
                .spawn((
                    Operation::bundle("project.write").unwrap().running(),
                    OperationTarget::new(owner),
                    ChildOf(owner),
                ))
                .id();
            let id = match &request.mutation {
                Mutation::Save { project, .. } => project.id.clone(),
                Mutation::Remove(url) | Mutation::Archive { url, .. } => ProjectId(url.clone()),
            };
            let mut proposal = commands.entity(accepted);
            proposal.insert((Proposal, RequestedProject(id)));
            if let Some(target) = target {
                proposal.insert(EntityTarget::<Project>::new(target));
            }
            match &request.mutation {
                Mutation::Save { project, .. } => {
                    let revision = registrations
                        .get(&root.0, &project.id.0)
                        .map_or(Revision(0), |(_, _, _, revision)| *revision)
                        .next()
                        .unwrap();
                    proposal.insert((
                        SaveIntent,
                        Name::new(project.name.trim().to_string()),
                        Cwd(project.directory.clone()),
                        Repository(project.repository.clone()),
                        revision,
                    ));
                }
                Mutation::Remove(url) => {
                    proposal.insert(RemoveIntent);
                    if let Some((_, directory, _, _)) = registrations.get(&root.0, url) {
                        proposal.insert(directory.clone());
                    }
                }
                Mutation::Archive { archived, .. } => {
                    proposal.insert(Archive(*archived));
                }
            }
            commands.entity(accepted).insert(Prepared {
                ticket: ticket.clone(),
                requester,
            });
            commands.entity(requester).insert(Accepted {
                operation: accepted,
                owner,
                directory: root.0.clone(),
            });
            if operation {
                commands.entity(requester).insert(OperationState::Running);
            }
            commands.entity(owner).insert(Writing {
                ticket,
                operation: accepted,
            });
            break;
        }
    }
}

fn aliases(
    mut results: MessageReader<AliasCompleted>,
    roots: Query<(&Root, &Writing)>,
    pending: Query<&Prepared, With<Aliasing>>,
    registrations: Registrations,
    mut failed: MessageWriter<WriteResult>,
    mut commands: WorldCommands,
) {
    let mut handled = HashSet::new();
    for result in results.read() {
        let Ok((root, writing)) = roots.get(result.ticket.owner) else {
            continue;
        };
        if writing.ticket != result.ticket || root.0 != result.ticket.directory {
            continue;
        }
        let Ok(prepared) = pending.get(writing.operation) else {
            continue;
        };
        if prepared.ticket != result.ticket {
            continue;
        }
        if !handled.insert(writing.operation) {
            continue;
        }
        let paths = match &result.result {
            Ok(paths) => paths,
            Err(error) => {
                failed.write(WriteResult {
                    ticket: result.ticket.clone(),
                    result: Err(error.clone()),
                });
                continue;
            }
        };
        let mut registered = HashSet::new();
        for (_, parent, id, ..) in &registrations.records {
            if parent.parent() == result.ticket.owner {
                registered.insert(id.0.as_str());
            }
        }
        let mut selected = BTreeSet::new();
        for path in paths {
            if path.components().any(|part| {
                !matches!(
                    part,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            }) {
                continue;
            }
            let Some(url) = crate::ProjectRoute::from_path(&root.0, &root.0.join(path)) else {
                continue;
            };
            if registered.contains(url.as_str()) || !selected.insert(url.clone()) {
                continue;
            }
            commands.spawn((
                HideAlias,
                RequestedProject(ProjectId(url)),
                ChildOf(writing.operation),
            ));
        }
        commands
            .entity(writing.operation)
            .remove::<Aliasing>()
            .insert(AliasesReady);
    }
}

type PendingWrites<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Prepared, Has<Aliasing>, Has<AliasesReady>),
    Without<Dispatched>,
>;

#[derive(SystemParam)]
struct PreparedWrites<'w, 's> {
    pending: PendingWrites<'w, 's>,
    proposals: Proposals<'w, 's>,
    aliases: Query<'w, 's, (&'static RequestedProject, &'static ChildOf), With<HideAlias>>,
}

fn dispatch(
    prepared_writes: PreparedWrites,
    roots: Query<(&Root, &Writing)>,
    registrations: Registrations,
    mut io: MessageWriter<WriteTaskRequest>,
    mut aliases: MessageWriter<AliasRequest>,
    mut commands: WorldCommands,
) {
    for (operation, prepared, aliasing, aliases_ready) in &prepared_writes.pending {
        let owner = prepared.ticket.owner;
        let Ok((root, writing)) = roots.get(owner) else {
            continue;
        };
        if writing.operation != operation
            || writing.ticket != prepared.ticket
            || root.0 != prepared.ticket.directory
        {
            continue;
        }
        let Ok((id, parent, name, directory, repository, revision, save, remove, archive, _)) =
            prepared_writes.proposals.get(operation)
        else {
            continue;
        };
        if parent.parent() != owner {
            continue;
        }
        if remove
            && !aliases_ready
            && let Some(directory) = directory
        {
            if !aliasing {
                aliases.write(AliasRequest {
                    ticket: prepared.ticket.clone(),
                    directory: directory.0.clone(),
                });
                commands.entity(operation).insert(Aliasing);
            }
            continue;
        }
        let id = &id.0;
        let mut snapshot = Snapshot::default();
        for (_, parent, current, name, directory, repository, revision) in &registrations.records {
            if parent.parent() != owner || ((save || remove) && *current == *id) {
                continue;
            }
            snapshot.projects.insert(
                current.0.clone(),
                Record::capture(name, directory, repository, revision),
            );
        }
        for (parent, current) in &registrations.hidden {
            if parent.parent() == owner && (!save || *current != *id) {
                snapshot.hidden.insert(current.0.clone());
            }
        }
        for (parent, current) in &registrations.archived {
            if parent.parent() == owner && (archive.is_none() || *current != *id) {
                snapshot.archived.insert(current.0.clone());
            }
        }
        if save {
            let (Some(name), Some(directory), Some(repository), Some(revision)) =
                (name, directory, repository, revision)
            else {
                continue;
            };
            snapshot.projects.insert(
                id.0.clone(),
                Record::capture(name, directory, repository, revision),
            );
        }
        if remove && directory.is_some() {
            snapshot.hidden.insert(id.0.clone());
        }
        if archive.is_some_and(|archive| archive.0) {
            snapshot.archived.insert(id.0.clone());
        }
        for (alias, parent) in &prepared_writes.aliases {
            if parent.parent() == operation {
                snapshot.hidden.insert(alias.0.0.clone());
            }
        }
        io.write(snapshot.write_request(prepared.ticket.clone()));
        commands.entity(operation).insert(Dispatched);
    }
}

fn prune(
    registrations: Registrations,
    operations: Query<(Entity, &OperationState, &Prepared)>,
    roots: Query<(Entity, &Writing)>,
    accepted: Query<(Entity, &Accepted)>,
    aliases: Query<(Entity, &ChildOf), With<HideAlias>>,
    mut completed: MessageWriter<WriteCompleted>,
    mut commands: WorldCommands,
) {
    for entity in &registrations.unused {
        commands.entity(entity).despawn();
    }
    for (owner, writing) in &roots {
        if operations.get(writing.operation).is_ok() {
            continue;
        }
        commands.entity(owner).remove::<Writing>();
        for (requester, pending) in &accepted {
            if pending.operation != writing.operation {
                continue;
            }
            let error = Error::Unavailable("accepted project write was removed".into());
            commands.entity(requester).remove::<Accepted>();
            commands.trigger(OperationCompleted {
                entity: requester,
                result: Err(error.to_string()),
            });
            completed.write(WriteCompleted {
                requester,
                result: Err(error),
            });
        }
    }
    for (operation, state, prepared) in &operations {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if let Ok((owner, writing)) = roots.get(prepared.ticket.owner)
            && writing.operation == operation
            && writing.ticket == prepared.ticket
        {
            commands.entity(owner).remove::<Writing>();
        }
        for (requester, pending) in &accepted {
            if pending.operation != operation {
                continue;
            }
            let error =
                Error::Unavailable("accepted project write ended before persistence".into());
            commands.entity(requester).remove::<Accepted>();
            commands.trigger(OperationCompleted {
                entity: requester,
                result: Err(error.to_string()),
            });
            completed.write(WriteCompleted {
                requester,
                result: Err(error),
            });
        }
        for (entity, parent) in &aliases {
            if parent.parent() == operation {
                commands.entity(entity).despawn();
            }
        }
        commands.entity(operation).remove::<(
            Prepared,
            Proposal,
            RequestedProject,
            SaveIntent,
            RemoveIntent,
            Archive,
            Dispatched,
            Aliasing,
            AliasesReady,
            EntityTarget<Project>,
            Name,
            Cwd,
            Repository,
            Revision,
        )>();
    }
}

#[derive(SystemParam)]
struct WriteEffects<'w> {
    completed: MessageWriter<'w, WriteCompleted>,
    saved: MessageWriter<'w, ProjectSaved>,
    changed: MessageWriter<'w, ProjectChanged>,
}

fn finish(
    mut results: MessageReader<WriteResult>,
    roots: Query<(&Root, &Writing)>,
    pending: Query<&Prepared>,
    proposals: Proposals,
    projects: Query<(Entity, &ChildOf, &ProjectId), With<Project>>,
    mut effects: WriteEffects,
    mut commands: WorldCommands,
) {
    let mut handled = HashSet::new();
    for result in results.read() {
        let Ok((root, writing)) = roots.get(result.ticket.owner) else {
            continue;
        };
        if writing.ticket != result.ticket || root.0 != result.ticket.directory {
            continue;
        }
        let Ok(prepared) = pending.get(writing.operation) else {
            continue;
        };
        if prepared.ticket != result.ticket {
            continue;
        }
        let Ok((id, parent, name, directory, repository, revision, save, remove, archive, target)) =
            proposals.get(writing.operation)
        else {
            continue;
        };
        if parent.parent() != result.ticket.owner {
            continue;
        }
        if !handled.insert(writing.operation) {
            continue;
        }
        let id = &id.0;
        commands.entity(result.ticket.owner).remove::<Writing>();
        let mut outcome = result.result.clone().map(|_| None);
        if let Ok(hidden) = &result.result {
            let existing = target.and_then(|target| projects.get(target.entity()).ok());
            let existing = existing
                .filter(|(_, parent, current)| {
                    parent.parent() == result.ticket.owner && *current == id
                })
                .or_else(|| {
                    projects.iter().find(|(_, parent, current)| {
                        parent.parent() == result.ticket.owner && *current == id
                    })
                });
            let entity = match existing {
                Some((entity, ..)) => entity,
                None => commands
                    .spawn((Project, id.clone(), ChildOf(result.ticket.owner)))
                    .id(),
            };
            if save {
                let (Some(name), Some(directory), Some(repository), Some(revision)) =
                    (name, directory, repository, revision)
                else {
                    continue;
                };
                commands
                    .entity(entity)
                    .insert((
                        Registered,
                        name.clone(),
                        directory.clone(),
                        repository.clone(),
                        *revision,
                    ))
                    .remove::<Hidden>();
                effects.saved.write(ProjectSaved {
                    root: root.0.clone(),
                    id: id.clone(),
                    directory: directory.0.clone(),
                    name: name.as_str().to_string(),
                    repository: repository.0.clone(),
                    revision: revision.0,
                });
                outcome = Ok(Some(id.clone()));
            }
            if remove {
                commands
                    .entity(entity)
                    .remove::<(Registered, Name, Cwd, Repository, Revision)>();
            }
            if let Some(archive) = archive {
                if archive.0 {
                    commands.entity(entity).insert(Archived);
                } else {
                    commands.entity(entity).remove::<Archived>();
                }
            }
            for url in hidden {
                if url == &id.0 {
                    commands.entity(entity).insert(Hidden);
                } else if let Some((entity, ..)) = projects
                    .iter()
                    .find(|(_, parent, id)| parent.parent() == result.ticket.owner && &id.0 == url)
                {
                    commands.entity(entity).insert(Hidden);
                } else {
                    commands.spawn((
                        Project,
                        Hidden,
                        ProjectId(url.clone()),
                        ChildOf(result.ticket.owner),
                    ));
                }
            }
            effects.changed.write(ProjectChanged {
                root: root.0.clone(),
            });
        }
        commands.trigger(OperationCompleted {
            entity: writing.operation,
            result: outcome
                .clone()
                .map(|_| ())
                .map_err(|error| error.to_string()),
        });
        if let Ok(mut requester) = commands.get_entity(prepared.requester) {
            requester.remove::<Accepted>();
            commands.trigger(OperationCompleted {
                entity: prepared.requester,
                result: outcome
                    .clone()
                    .map(|_| ())
                    .map_err(|error| error.to_string()),
            });
        }
        effects.completed.write(WriteCompleted {
            requester: prepared.requester,
            result: outcome,
        });
    }
}
