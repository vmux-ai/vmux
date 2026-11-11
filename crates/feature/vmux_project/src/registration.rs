use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use serde::{Deserialize, Serialize};
use vmux_ecs::{
    Cwd, IdentityIndexPlugin, Operation, OperationCompleted, OperationPlugin, OperationState,
    Revision,
};

use crate::ProjectId;

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
                (finish, write, prune).chain().in_set(StoreSet::Write),
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
    requester: Entity,
    mutation: Mutation,
}

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
    pub removed_directory: Option<PathBuf>,
}

#[derive(Message)]
pub struct WriteResult {
    pub ticket: Ticket,
    pub result: Result<BTreeSet<String>, Error>,
}

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
    hidden: Query<'w, 's, (&'static ChildOf, &'static ProjectId), With<Hidden>>,
    archived: Query<'w, 's, (&'static ChildOf, &'static ProjectId), With<Archived>>,
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
    Changed<Root>,
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
                Record {
                    name: name.as_str().to_string(),
                    directory: directory.0.clone(),
                    repository: repository.0.clone(),
                    revision: revision.0,
                },
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

    fn owns(&self, entity: Entity, root: &Path) -> bool {
        self.roots
            .get(entity)
            .is_ok_and(|(_, directory, _, _)| directory.0 == root)
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub projects: BTreeMap<String, Record>,
    #[serde(default)]
    pub hidden: BTreeSet<String>,
    #[serde(default)]
    pub archived: BTreeSet<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Record {
    pub name: String,
    pub directory: PathBuf,
    pub repository: Option<String>,
    pub revision: u64,
}

impl Record {
    fn bundle(&self) -> impl Bundle {
        (
            Registered,
            Name::new(self.name.clone()),
            Cwd(self.directory.clone()),
            Repository(self.repository.clone()),
            Revision(self.revision),
        )
    }
}

fn cancel(
    accepted: Query<(Entity, &Accepted)>,
    roots: Query<&Root>,
    mut completed: MessageWriter<WriteCompleted>,
    mut commands: Commands,
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
}

fn reset(
    roots: ChangedRoots,
    children: Query<(Entity, &ChildOf), With<Project>>,
    mut commands: Commands,
) {
    for (entity, root, loaded, loading, writing) in &roots {
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
    roots: Query<(Entity, &Root), UnloadedRoots>,
    all_roots: Query<(Entity, &Root)>,
    mut clock: ResMut<Clock>,
    mut requests: MessageWriter<LoadRequest>,
    mut commands: Commands,
) {
    for (owner, root) in &roots {
        if all_roots
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
    mut commands: Commands,
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
                project.insert(record.bundle());
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
    mut io: MessageWriter<WriteTaskRequest>,
    mut completed: MessageWriter<WriteCompleted>,
    mut commands: Commands,
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
            if let Err(error) = registrations.ready(&root.0) {
                commands.entity(requester).remove::<WriteRequest>();
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
            let mut snapshot = registrations.snapshot(&root.0);
            let mut mutation = request.mutation.clone();
            let mut removed_directory = None;
            commands.entity(requester).remove::<WriteRequest>();
            let validation = match &mut mutation {
                Mutation::Save { project, expected } => {
                    let current = snapshot
                        .projects
                        .get(&project.id.0)
                        .map_or(0, |record| record.revision);
                    let name = project.name.trim();
                    if name.is_empty() || name.len() > 256 || name.contains(['\0', '\n', '\r']) {
                        Err(Error::InvalidName)
                    } else if project.root != root.0
                        || current != *expected
                        || Revision(current).next().is_none()
                    {
                        Err(Error::Conflict)
                    } else if snapshot.projects.iter().any(|(url, record)| {
                        url != &project.id.0 && record.directory == project.directory
                    }) {
                        Err(Error::Duplicate)
                    } else {
                        project.name = name.to_string();
                        project.revision = Revision(current).next().unwrap().0;
                        snapshot.hidden.remove(&project.id.0);
                        snapshot.projects.insert(
                            project.id.0.clone(),
                            Record {
                                name: project.name.clone(),
                                directory: project.directory.clone(),
                                repository: project.repository.clone(),
                                revision: project.revision,
                            },
                        );
                        Ok(())
                    }
                }
                Mutation::Remove(url) => {
                    if let Some(record) = snapshot.projects.remove(url) {
                        removed_directory = Some(record.directory);
                        snapshot.hidden.insert(url.clone());
                    }
                    Ok(())
                }
                Mutation::Archive { url, archived } => {
                    if *archived {
                        snapshot.archived.insert(url.clone());
                    } else {
                        snapshot.archived.remove(url);
                    }
                    Ok(())
                }
            };
            if let Err(error) = validation {
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
            let ticket = clock.ticket(owner, &root.0);
            io.write(WriteTaskRequest {
                ticket: ticket.clone(),
                snapshot,
                removed_directory,
            });
            commands.entity(requester).insert(Accepted {
                owner,
                directory: root.0.clone(),
            });
            if operation {
                commands.entity(requester).insert(OperationState::Running);
            }
            commands.entity(owner).insert(Writing {
                ticket,
                requester,
                mutation,
            });
            break;
        }
    }
}

fn prune(projects: Query<Entity, UnusedProjects>, mut commands: Commands) {
    for entity in &projects {
        commands.entity(entity).despawn();
    }
}

fn finish(
    mut results: MessageReader<WriteResult>,
    roots: Query<(&Root, &Writing)>,
    projects: Query<(Entity, &ChildOf, &ProjectId), With<Project>>,
    mut completed: MessageWriter<WriteCompleted>,
    mut saved: MessageWriter<ProjectSaved>,
    mut changed: MessageWriter<ProjectChanged>,
    mut commands: Commands,
) {
    for result in results.read() {
        let Ok((root, pending)) = roots.get(result.ticket.owner) else {
            continue;
        };
        if pending.ticket != result.ticket || root.0 != result.ticket.directory {
            continue;
        }
        commands.entity(result.ticket.owner).remove::<Writing>();
        if let Ok(hidden) = &result.result {
            let url = match &pending.mutation {
                Mutation::Save { project, .. } => &project.id.0,
                Mutation::Remove(url) | Mutation::Archive { url, .. } => url,
            };
            let existing = projects
                .iter()
                .find(|(_, parent, id)| parent.parent() == result.ticket.owner && &id.0 == url);
            let entity = match existing {
                Some((entity, ..)) => entity,
                None => commands
                    .spawn((
                        Project,
                        ProjectId(url.clone()),
                        ChildOf(result.ticket.owner),
                    ))
                    .id(),
            };
            match &pending.mutation {
                Mutation::Save { project, .. } => {
                    commands
                        .entity(entity)
                        .insert((
                            Registered,
                            Name::new(project.name.clone()),
                            Cwd(project.directory.clone()),
                            Repository(project.repository.clone()),
                            Revision(project.revision),
                        ))
                        .remove::<Hidden>();
                    saved.write(project.clone());
                }
                Mutation::Remove(_) => {
                    commands
                        .entity(entity)
                        .remove::<(Registered, Name, Cwd, Repository, Revision)>();
                }
                Mutation::Archive { archived, .. } => {
                    if *archived {
                        commands.entity(entity).insert(Archived);
                    } else {
                        commands.entity(entity).remove::<Archived>();
                    }
                }
            }
            for url in hidden {
                if let Some((entity, ..)) = projects
                    .iter()
                    .find(|(_, parent, id)| parent.parent() == result.ticket.owner && &id.0 == url)
                {
                    commands.entity(entity).insert(Hidden);
                } else if let Mutation::Remove(removed) = &pending.mutation
                    && removed == url
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
            changed.write(ProjectChanged {
                root: root.0.clone(),
            });
        }
        let result = result.result.clone().map(|_| match &pending.mutation {
            Mutation::Save { project, .. } => Some(project.id.clone()),
            Mutation::Remove(_) | Mutation::Archive { .. } => None,
        });
        if let Ok(mut requester) = commands.get_entity(pending.requester) {
            requester.remove::<Accepted>();
            commands.trigger(OperationCompleted {
                entity: pending.requester,
                result: result
                    .clone()
                    .map(|_| ())
                    .map_err(|error| error.to_string()),
            });
        }
        completed.write(WriteCompleted {
            requester: pending.requester,
            result,
        });
    }
}
