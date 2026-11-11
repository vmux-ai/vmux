use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_ecs::Cwd;
use vmux_ecs::profile::ProjectsRoot;

use crate::project::{ProjectId, ProjectRoute};

pub(super) struct ProjectPersistencePlugin;

impl Plugin for ProjectPersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<WriteCompleted>()
            .add_message::<ProjectSaved>()
            .configure_sets(
                Update,
                (PersistenceSet::Load, PersistenceSet::Write).chain(),
            )
            .add_systems(Update, (load, restore).chain().in_set(PersistenceSet::Load))
            .add_systems(
                Update,
                (finish, write).chain().in_set(PersistenceSet::Write),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum PersistenceSet {
    Load,
    Write,
}

#[derive(Component)]
pub(super) struct Registered;

#[derive(Component)]
pub(super) struct Revision(pub(super) u64);

#[derive(Component)]
pub(super) struct Repository(pub(super) Option<String>);

#[derive(Component)]
struct Hidden;

#[derive(Component)]
struct Loaded;

#[derive(Component)]
struct LoadError(String);

#[derive(Component)]
struct LoadTask(Task<Result<Snapshot, String>>);

type UnloadedRoots = (Without<Loaded>, Without<LoadTask>, Without<LoadError>);
type IdleRoots = (With<Loaded>, Without<WriteTask>);

#[derive(Component)]
pub(super) struct WriteRequest {
    pub(super) root: PathBuf,
    pub(super) mutation: Mutation,
}

#[derive(Clone)]
pub(super) enum Mutation {
    Save {
        project: ProjectSaved,
        expected: u64,
    },
    Remove(String),
}

#[derive(Component)]
struct WriteTask {
    requester: Entity,
    mutation: Mutation,
    task: Task<Result<(), String>>,
}

#[derive(Message, Clone)]
pub(super) struct ProjectSaved {
    pub(super) root: PathBuf,
    pub(super) id: ProjectId,
    pub(super) directory: PathBuf,
    pub(super) name: String,
    pub(super) repository: Option<String>,
    pub(super) revision: u64,
}

#[derive(Message)]
pub(super) struct WriteCompleted {
    pub(super) requester: Entity,
    pub(super) result: Result<Option<ProjectId>, String>,
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
pub(super) struct Registrations<'w, 's> {
    roots: Query<
        'w,
        's,
        (
            Entity,
            &'static ProjectsRoot,
            Has<Loaded>,
            Option<&'static LoadError>,
        ),
    >,
    records: Records<'w, 's>,
    hidden: Query<'w, 's, (&'static ChildOf, &'static ProjectId), With<Hidden>>,
}

impl Registrations<'_, '_> {
    pub(super) fn ready(&self, root: &Path) -> Result<bool, String> {
        for (_, directory, loaded, error) in &self.roots {
            if directory.0.path() != root
                && directory.0.path() != vmux_path::PathIdentity::resolve(root).as_path()
            {
                continue;
            }
            if let Some(error) = error {
                return Err(error.0.clone());
            }
            return Ok(loaded);
        }
        Ok(false)
    }

    pub(super) fn snapshot(&self, root: &Path) -> Snapshot {
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
        snapshot
    }

    pub(super) fn get(
        &self,
        root: &Path,
        url: &str,
    ) -> Option<(&Name, &Cwd, &Repository, &Revision)> {
        for (_, parent, id, name, directory, repository, revision) in &self.records {
            if id.0 == url && self.owns(parent.parent(), root) {
                return Some((name, directory, repository, revision));
            }
        }
        None
    }

    pub(super) fn resolve(&self, url: &str, root: &Path) -> Option<Result<PathBuf, String>> {
        if !ProjectRoute::is_page(url) {
            return None;
        }
        if let Err(error) = ProjectRoute::route_path(url, root)? {
            return Some(Err(error));
        }
        if let Err(error) = self.ready(root) {
            return Some(Err(error));
        }
        for (parent, id) in &self.hidden {
            if id.0 == url && self.owns(parent.parent(), root) {
                return Some(Err(vmux_ui::i18n::translate("error-page-not-found")));
            }
        }
        if let Some((_, directory, _, _)) = self.get(root, url) {
            if !directory.0.is_dir() {
                return Some(Err(vmux_ui::i18n::translate("error-page-not-found")));
            }
            return Some(Ok(directory.0.clone()));
        }
        ProjectRoute::resolve(url, root)
    }

    pub(super) fn url_for_directory(&self, root: &Path, directory: &Path) -> Option<String> {
        let directory = vmux_path::PathIdentity::resolve(directory);
        let mut selected = None;
        for (_, parent, id, _, cwd, _, _) in &self.records {
            if !self.owns(parent.parent(), root) || !directory.as_path().starts_with(&cwd.0) {
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
        self.roots.get(entity).is_ok_and(|(_, directory, _, _)| {
            directory.0.path() == root
                || directory.0.path() == vmux_path::PathIdentity::resolve(root).as_path()
        })
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Snapshot {
    #[serde(default)]
    pub(super) projects: BTreeMap<String, Record>,
    #[serde(default)]
    pub(super) hidden: BTreeSet<String>,
}

impl Snapshot {
    fn read(root: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(root.join(".projects.ron")) {
            Ok(source) => ron::from_str(&source).map_err(|error| error.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn write(&self, root: &Path) -> Result<(), String> {
        let source = ron::ser::to_string(self).map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(root.join(".projects.ron"), source.as_bytes())
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Record {
    pub(super) name: String,
    pub(super) directory: PathBuf,
    pub(super) repository: Option<String>,
    pub(super) revision: u64,
}

fn load(
    roots: Query<(Entity, &ProjectsRoot), UnloadedRoots>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, directory) in &roots {
        let root = directory.0.path().to_path_buf();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = Snapshot::read(&root);
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        commands.entity(entity).insert(LoadTask(task));
    }
}

fn restore(mut tasks: Query<(Entity, &mut LoadTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<LoadTask>();
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                commands.entity(entity).insert(LoadError(error));
                continue;
            }
        };
        for (url, record) in snapshot.projects {
            commands
                .spawn((
                    Registered,
                    ProjectId(url),
                    Name::new(record.name),
                    Cwd(record.directory),
                    Repository(record.repository),
                    Revision(record.revision),
                    ChildOf(entity),
                ))
                .remove::<Save>();
        }
        for url in snapshot.hidden {
            commands.spawn((Hidden, ProjectId(url), ChildOf(entity)));
        }
        commands.entity(entity).insert(Loaded);
    }
}

fn write(
    roots: Query<(Entity, &ProjectsRoot), IdleRoots>,
    requests: Query<(Entity, &WriteRequest)>,
    registrations: Registrations,
    mut completed: MessageWriter<WriteCompleted>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, directory) in &roots {
        let root = directory.0.path();
        for (requester, request) in &requests {
            if request.root != root {
                continue;
            }
            let mut snapshot = registrations.snapshot(root);
            let mut mutation = request.mutation.clone();
            commands.entity(requester).remove::<WriteRequest>();
            match &mut mutation {
                Mutation::Save { project, expected } => {
                    let current = snapshot
                        .projects
                        .get(&project.id.0)
                        .map_or(0, |record| record.revision);
                    let revision = match current.checked_add(1) {
                        Some(revision) if current == *expected => revision,
                        _ => {
                            completed.write(WriteCompleted {
                                requester,
                                result: Err(vmux_ui::i18n::translate("project-edit-conflict")),
                            });
                            continue;
                        }
                    };
                    if snapshot.projects.iter().any(|(url, record)| {
                        url != &project.id.0 && record.directory == project.directory
                    }) {
                        completed.write(WriteCompleted {
                            requester,
                            result: Err(vmux_ui::i18n::translate("project-already-exists")),
                        });
                        continue;
                    }
                    project.revision = revision;
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
                }
                Mutation::Remove(url) => {
                    if let Some(record) = snapshot.projects.remove(url) {
                        if let Some(source) = ProjectRoute::from_path(root, &record.directory)
                            && !snapshot.projects.contains_key(&source)
                        {
                            snapshot.hidden.insert(source);
                        }
                        snapshot.hidden.insert(url.clone());
                    }
                }
            }
            let root = root.to_path_buf();
            let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
            let task = IoTaskPool::get().spawn(async move {
                let result = snapshot.write(&root);
                if let Some(wake) = wake {
                    let _ = wake.send_event(WinitUserEvent::WakeUp);
                }
                result
            });
            commands.entity(entity).insert(WriteTask {
                requester,
                mutation,
                task,
            });
            break;
        }
    }
}

fn finish(
    mut tasks: Query<(Entity, &ProjectsRoot, &mut WriteTask)>,
    records: Records,
    hidden: Query<(Entity, &ChildOf, &ProjectId), With<Hidden>>,
    mut completed: MessageWriter<WriteCompleted>,
    mut saved: MessageWriter<ProjectSaved>,
    mut commands: Commands,
) {
    for (entity, root, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).remove::<WriteTask>();
        if result.is_ok() {
            match &task.mutation {
                Mutation::Save { project, .. } => {
                    let existing = records.iter().find(|(_, parent, id, ..)| {
                        parent.parent() == entity && id.0 == project.id.0
                    });
                    let bundle = (
                        Registered,
                        project.id.clone(),
                        Name::new(project.name.clone()),
                        Cwd(project.directory.clone()),
                        Repository(project.repository.clone()),
                        Revision(project.revision),
                        ChildOf(entity),
                    );
                    if let Some((record, ..)) = existing {
                        commands.entity(record).insert(bundle).remove::<Save>();
                    } else {
                        commands.spawn(bundle).remove::<Save>();
                    }
                    for (hidden, parent, id) in &hidden {
                        if parent.parent() == entity && id == &project.id {
                            commands.entity(hidden).despawn();
                        }
                    }
                    saved.write(project.clone());
                }
                Mutation::Remove(url) => {
                    let existing = records
                        .iter()
                        .find(|(_, parent, id, ..)| parent.parent() == entity && &id.0 == url);
                    if let Some((record, _, _, _, directory, _, _)) = existing {
                        if let Some(source) = ProjectRoute::from_path(root.0.path(), &directory.0)
                            && !records.iter().any(|(_, parent, id, ..)| {
                                parent.parent() == entity && id.0 == source && &id.0 != url
                            })
                            && !hidden
                                .iter()
                                .any(|(_, parent, id)| parent.parent() == entity && id.0 == source)
                        {
                            commands.spawn((Hidden, ProjectId(source), ChildOf(entity)));
                        }
                        commands.entity(record).despawn();
                        if !hidden
                            .iter()
                            .any(|(_, parent, id)| parent.parent() == entity && &id.0 == url)
                        {
                            commands.spawn((Hidden, ProjectId(url.clone()), ChildOf(entity)));
                        }
                    }
                }
            }
        }
        let result = result.map(|_| match &task.mutation {
            Mutation::Save { project, .. } => Some(project.id.clone()),
            Mutation::Remove(_) => None,
        });
        completed.write(WriteCompleted {
            requester: task.requester,
            result,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Store {
        app: App,
        root: PathBuf,
        owner: Entity,
    }

    impl Store {
        fn at(root: &Path) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ProjectPersistencePlugin));
            let directory =
                vmux_ecs::profile::ProjectsDirectory::ensure_at(root.to_path_buf()).unwrap();
            let root = directory.path().to_path_buf();
            let owner = app.world_mut().spawn(ProjectsRoot(directory)).id();
            let mut store = Self { app, root, owner };
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                store.app.update();
                if store.app.world().get::<Loaded>(owner).is_some()
                    || store.app.world().get::<LoadError>(owner).is_some()
                {
                    return store;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "registrations did not load"
                );
                std::thread::yield_now();
            }
        }

        fn queue(&mut self, url: &str, revision: u64, name: &str, directory: &Path) -> Entity {
            self.app
                .world_mut()
                .spawn(WriteRequest {
                    root: self.root.clone(),
                    mutation: Mutation::Save {
                        expected: revision,
                        project: ProjectSaved {
                            root: self.root.clone(),
                            id: ProjectId(url.into()),
                            directory: directory.canonicalize().unwrap(),
                            name: name.into(),
                            repository: None,
                            revision: 0,
                        },
                    },
                })
                .id()
        }

        fn complete(&mut self, count: usize) -> Vec<WriteCompleted> {
            let mut results = Vec::new();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                self.app.update();
                results.extend(
                    self.app
                        .world_mut()
                        .resource_mut::<Messages<WriteCompleted>>()
                        .drain(),
                );
                if results.len() == count {
                    return results;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "writes did not complete"
                );
                std::thread::yield_now();
            }
        }

        fn save(&mut self, url: &str, revision: u64, name: &str, directory: &Path) {
            self.queue(url, revision, name, directory);
            assert!(self.complete(1)[0].result.is_ok());
        }

        fn entity(&mut self, url: &str) -> Entity {
            self.app
                .world_mut()
                .query_filtered::<(Entity, &ProjectId), With<Registered>>()
                .iter(self.app.world())
                .find(|(_, id)| id.0 == url)
                .unwrap()
                .0
        }

        fn snapshot(&mut self) -> Snapshot {
            let mut state =
                bevy::ecs::system::SystemState::<Registrations>::new(self.app.world_mut());
            state.get(self.app.world()).unwrap().snapshot(&self.root)
        }
    }

    #[test]
    fn existing_ron_registrations_restore_into_project_components() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("repo");
        std::fs::create_dir_all(directory.join(".git")).unwrap();
        let url = "vmux://projects/stable";
        let remote = "https://github.com/example/repo.git";
        let source = format!(
            "(projects:{{{url:?}:(name:\"Legacy\",directory:{:?},repository:Some({remote:?}),revision:7)}},hidden:[\"vmux://projects/removed\"])",
            directory.canonicalize().unwrap().to_str().unwrap(),
        );
        std::fs::write(root.path().join(".projects.ron"), source).unwrap();
        let mut store = Store::at(root.path());
        let entity = store.entity(url);
        assert_eq!(
            store.app.world().get::<Name>(entity).unwrap().as_str(),
            "Legacy"
        );
        assert_eq!(store.app.world().get::<Revision>(entity).unwrap().0, 7);
        assert_eq!(
            store
                .app
                .world()
                .get::<Repository>(entity)
                .unwrap()
                .0
                .as_deref(),
            Some(remote)
        );
        assert!(store.snapshot().hidden.contains("vmux://projects/removed"));
    }

    #[test]
    fn writes_and_queries_stay_with_their_own_projects_root() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let first_project = first.path().join("repo");
        let second_project = second.path().join("repo");
        std::fs::create_dir_all(first_project.join(".git")).unwrap();
        std::fs::create_dir_all(second_project.join(".git")).unwrap();
        let mut store = Store::at(first.path());
        let directory =
            vmux_ecs::profile::ProjectsDirectory::ensure_at(second.path().to_path_buf()).unwrap();
        let second_root = directory.path().to_path_buf();
        store.app.world_mut().spawn(ProjectsRoot(directory));
        store.queue("vmux://projects/same", 0, "First root", &first_project);
        store.app.world_mut().spawn(WriteRequest {
            root: second_root.clone(),
            mutation: Mutation::Save {
                expected: 0,
                project: ProjectSaved {
                    root: second_root.clone(),
                    id: ProjectId("vmux://projects/same".into()),
                    directory: second_project.canonicalize().unwrap(),
                    name: "Second root".into(),
                    repository: None,
                    revision: 0,
                },
            },
        });
        assert!(
            store
                .complete(2)
                .iter()
                .all(|completed| completed.result.is_ok())
        );
        let first_snapshot = store.snapshot();
        assert_eq!(first_snapshot.projects.len(), 1);
        assert_eq!(
            first_snapshot.projects["vmux://projects/same"].name,
            "First root"
        );
        let mut state = bevy::ecs::system::SystemState::<Registrations>::new(store.app.world_mut());
        let second_snapshot = state.get(store.app.world()).unwrap().snapshot(&second_root);
        assert_eq!(second_snapshot.projects.len(), 1);
        assert_eq!(
            second_snapshot.projects["vmux://projects/same"].name,
            "Second root"
        );
        assert_eq!(
            Snapshot::read(&second_root).unwrap().projects["vmux://projects/same"].name,
            "Second root"
        );
    }

    #[test]
    fn source_edits_preserve_the_entity_and_restore_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir_all(first.join(".git")).unwrap();
        std::fs::create_dir_all(second.join(".git")).unwrap();
        std::fs::write(first.join("work.txt"), "keep").unwrap();
        let url = "vmux://projects/stable";
        let mut store = Store::at(root.path());
        store.save(url, 0, "First", &first);
        let entity = store.entity(url);
        store.save(url, 1, "Renamed", &second);
        assert_eq!(store.entity(url), entity);
        assert!(store.app.world().get::<Save>(entity).is_none());
        assert_eq!(
            store.app.world().get::<Name>(entity).unwrap().as_str(),
            "Renamed"
        );
        assert_eq!(
            store.app.world().get::<Cwd>(entity).unwrap().0,
            second.canonicalize().unwrap()
        );
        drop(store);
        let mut restored = Store::at(root.path());
        let entity = restored.entity(url);
        assert!(restored.app.world().get::<Save>(entity).is_none());
        assert_eq!(restored.app.world().get::<Revision>(entity).unwrap().0, 2);
        let mut state =
            bevy::ecs::system::SystemState::<Registrations>::new(restored.app.world_mut());
        let registrations = state.get(restored.app.world()).unwrap();
        assert_eq!(
            registrations.resolve(url, &restored.root).unwrap().unwrap(),
            second.canonicalize().unwrap()
        );
        assert_eq!(
            registrations.url_for_directory(&restored.root, &second.join("src")),
            Some(url.into())
        );
        assert_eq!(
            std::fs::read_to_string(first.join("work.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn concurrent_writes_reject_stale_edits_and_keep_independent_projects() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir_all(first.join(".git")).unwrap();
        std::fs::create_dir_all(second.join(".git")).unwrap();
        let url = "vmux://projects/first";
        let mut store = Store::at(root.path());
        store.save(url, 0, "First", &first);
        store.queue(url, 1, "Winner", &first);
        store.queue(url, 1, "Other edit", &first);
        store.queue("vmux://projects/second", 0, "Second", &second);
        let results = store.complete(3);
        assert_eq!(
            results
                .iter()
                .filter(|result| result.result.is_ok())
                .count(),
            2
        );
        let snapshot = store.snapshot();
        assert_eq!(snapshot.projects.len(), 2);
        assert_eq!(snapshot.projects[url].revision, 2);
        assert_eq!(Snapshot::read(&store.root).unwrap().projects.len(), 2);
        store.queue("vmux://projects/duplicate", 0, "Duplicate", &first);
        assert!(store.complete(1)[0].result.is_err());
        assert_eq!(store.snapshot().projects.len(), 2);
    }

    #[test]
    fn failed_persistence_keeps_the_committed_entity() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("repo");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        let url = "vmux://projects/repo";
        let mut store = Store::at(root.path());
        store.save(url, 0, "Original", &project);
        let file = store.root.join(".projects.ron");
        let source = std::fs::read_to_string(&file).unwrap();
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        store.queue(url, 1, "Rejected", &project);
        assert!(store.complete(1)[0].result.is_err());
        let entity = store.entity(url);
        assert_eq!(
            store.app.world().get::<Name>(entity).unwrap().as_str(),
            "Original"
        );
        assert_eq!(store.app.world().get::<Revision>(entity).unwrap().0, 1);
        std::fs::remove_dir(&file).unwrap();
        std::fs::write(&file, source).unwrap();
        assert_eq!(
            Snapshot::read(&store.root).unwrap().projects[url].name,
            "Original"
        );
    }

    #[test]
    fn removal_restores_hidden_urls_without_deleting_checkout_files() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("repo");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::write(project.join("work.txt"), "keep").unwrap();
        let url = "vmux://projects/stable";
        let mut store = Store::at(root.path());
        store.save(url, 0, "Project", &project);
        store.app.world_mut().spawn(WriteRequest {
            root: store.root.clone(),
            mutation: Mutation::Remove(url.into()),
        });
        assert!(store.complete(1)[0].result.is_ok());
        drop(store);
        let mut restored = Store::at(root.path());
        assert!(restored.snapshot().projects.is_empty());
        let mut state =
            bevy::ecs::system::SystemState::<Registrations>::new(restored.app.world_mut());
        let registrations = state.get(restored.app.world()).unwrap();
        for url in [url, "vmux://projects/repo"] {
            assert!(registrations.resolve(url, &restored.root).unwrap().is_err());
        }
        assert_eq!(
            std::fs::read_to_string(project.join("work.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn corrupt_registrations_are_reported_without_overwriting_them() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join(".projects.ron");
        std::fs::write(&file, "invalid").unwrap();
        let store = Store::at(root.path());
        assert!(store.app.world().get::<LoadError>(store.owner).is_some());
        assert!(store.app.world().get::<Loaded>(store.owner).is_none());
        assert_eq!(std::fs::read_to_string(file).unwrap(), "invalid");
    }
}
