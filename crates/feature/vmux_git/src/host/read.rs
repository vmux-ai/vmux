use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
use vmux_path::PathIdentity;

use super::job::RepositoryJob;
use super::job_runner::{GitJob, GitJobRunning, RepositoryOutput, RetirementSet};
use super::repository::GitRepository;
use super::runtime::{GitRuntime, GitTask};
use crate::event::{GitOperationError, GitRepositorySnapshot};

pub(super) struct ReadPlugin;

impl Plugin for ReadPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(request).add_systems(
            Update,
            (
                poll_discovery,
                join,
                poll_reads,
                distribute,
                finish_retired,
                refresh_aliases,
                evict,
            )
                .chain()
                .in_set(ReadSet)
                .in_set(super::GitUpdateSet::Jobs)
                .after(RetirementSet),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ReadSet;

#[derive(EntityEvent)]
pub(super) struct RepositoryReadRequest {
    #[event_target]
    pub(super) entity: Entity,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SourceIdentity {
    path: PathIdentity,
    directory: Option<(u64, u64)>,
    marker: Option<(u64, u64, u64, i64, i64)>,
}

impl SourceIdentity {
    pub fn capture(path: &Path) -> Self {
        let path = PathIdentity::resolve(path);
        Self {
            directory: Self::metadata(path.as_path()),
            marker: Self::marker(&path.as_path().join(".git")),
            path,
        }
    }

    fn metadata(path: &Path) -> Option<(u64, u64)> {
        let metadata = std::fs::metadata(path).ok()?;
        Some((metadata.dev(), metadata.ino()))
    }

    fn marker(path: &Path) -> Option<(u64, u64, u64, i64, i64)> {
        let metadata = std::fs::metadata(path).ok()?;
        if metadata.is_file() {
            return Some((
                metadata.dev(),
                metadata.ino(),
                metadata.len(),
                metadata.mtime(),
                metadata.mtime_nsec(),
            ));
        }
        Some((metadata.dev(), metadata.ino(), 0, 0, 0))
    }

    pub fn matches(&self, path: &Path) -> bool {
        *self == Self::capture(path)
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    fn valid(&self) -> bool {
        self.matches(self.path.as_path())
    }
}

#[derive(Component, Clone)]
pub(super) struct PageRepositorySource {
    requested: PathBuf,
    request: SourceIdentity,
    root: SourceIdentity,
}

impl PageRepositorySource {
    pub(super) fn matches(&self, path: &str) -> bool {
        let requested = PathIdentity::lexical(path).into_path_buf();
        if requested != self.requested && requested != self.root.path.as_path() {
            return false;
        }
        self.root.valid()
            && (self.request.matches(Path::new(path)) || self.root.matches(Path::new(path)))
    }

    pub(super) fn request_alias(&self) -> (PathBuf, SourceIdentity) {
        (self.requested.clone(), self.request.clone())
    }

    pub(super) fn source_at(&self, path: &str) -> SourceIdentity {
        if PathIdentity::lexical(path).into_path_buf() == self.requested {
            return self.request.clone();
        }
        self.root.clone()
    }

    pub(super) fn preserve_request(&mut self, previous: &Self) {
        if self.root == previous.root {
            self.requested.clone_from(&previous.requested);
            self.request.clone_from(&previous.request);
        }
    }
}

#[derive(Component, Clone)]
pub(super) struct RepositoryRequestIdentity {
    source: SourceIdentity,
    accepted: Instant,
}

impl RepositoryRequestIdentity {
    pub(super) fn capture(path: &Path) -> Self {
        Self {
            source: SourceIdentity::capture(path),
            accepted: Instant::now(),
        }
    }
}

#[derive(Component)]
struct DiscoveryTask(GitTask<Result<RepositoryDiscovery, GitOperationError>>);

#[derive(Component)]
struct RepositoryDiscovery {
    repository: GitRepository,
    is_worktree: bool,
    source: SourceIdentity,
}

#[derive(Component)]
struct RepositoryRead {
    source: SourceIdentity,
    started: Instant,
}

impl RepositoryRead {
    const CAPACITY: usize = 128;

    fn accepts(
        &self,
        identity: &RepositoryRequestIdentity,
        discovery: &RepositoryDiscovery,
    ) -> bool {
        self.started >= identity.accepted && self.source == discovery.source && self.source.valid()
    }
}

#[derive(Component)]
struct ReadTask(GitTask<RepositoryOutput>);

#[derive(Component)]
struct RepositorySubscriber(Entity);

#[derive(Component)]
struct RepositoryAlias {
    requested: PathBuf,
    source: SourceIdentity,
    read: Entity,
    touched: Instant,
}

impl RepositoryAlias {
    const CAPACITY: usize = RepositoryRead::CAPACITY * 4;
}

#[derive(SystemParam)]
pub(super) struct Repositories<'w, 's> {
    aliases: Query<'w, 's, &'static RepositoryAlias>,
    reads: Query<'w, 's, (&'static RepositoryRead, &'static RepositoryOutput)>,
}

impl Repositories<'_, '_> {
    pub(super) fn cached(&self, path: &Path) -> Option<GitRepositorySnapshot> {
        let requested = PathIdentity::lexical(path).into_path_buf();
        let mut latest = None;
        for alias in &self.aliases {
            if alias.requested != requested || !alias.source.matches(path) {
                continue;
            }
            let Ok((read, output)) = self.reads.get(alias.read) else {
                continue;
            };
            if !read.source.valid() {
                continue;
            }
            let Ok(summary) = &output.0 else {
                continue;
            };
            if latest.is_none_or(|(started, _)| read.started > started) {
                latest = Some((read.started, summary));
            }
        }
        for (read, output) in &self.reads {
            if !read.source.matches(path) {
                continue;
            }
            let Ok(summary) = &output.0 else {
                continue;
            };
            if latest.is_none_or(|(started, _)| read.started > started) {
                latest = Some((read.started, summary));
            }
        }
        let mut snapshot = latest?.1.snapshot.clone();
        snapshot.path = path.to_string_lossy().into_owned();
        Some(snapshot)
    }
}

fn request(
    trigger: On<RepositoryReadRequest>,
    jobs: Query<(&RepositoryJob, &RepositoryRequestIdentity)>,
    runtime: Res<GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok((job, identity)) = jobs.get(entity) else {
        return;
    };
    if !identity.source.matches(&job.path) {
        commands
            .entity(entity)
            .insert(RepositoryOutput(Err(GitOperationError {
                message: "Git repository source changed".into(),
            })));
        return;
    }
    let path = identity.source.path.clone().into_path_buf();
    let Some(task) = runtime.spawn(wake.as_deref().map(|wake| (**wake).clone()), move || {
        GitRepository::discover_with_worktree(&path)
            .map(|(repository, is_worktree)| RepositoryDiscovery {
                source: SourceIdentity::capture(repository.path()),
                repository,
                is_worktree,
            })
            .map_err(|error| GitOperationError { message: error.0 })
    }) else {
        return;
    };
    commands
        .entity(entity)
        .insert((GitJobRunning, DiscoveryTask(task)));
}

fn poll_discovery(
    mut tasks: Query<(
        Entity,
        &mut DiscoveryTask,
        &RepositoryJob,
        &RepositoryRequestIdentity,
        Has<GitJob>,
    )>,
    mut commands: Commands,
) {
    for (entity, mut task, job, identity, attached) in &mut tasks {
        let valid = identity.source.matches(&job.path);
        if !attached || !valid {
            task.0.cancel();
        }
        if !task.0.is_finished() {
            continue;
        }
        if !attached {
            commands.entity(entity).despawn();
            continue;
        }
        let result = if !valid {
            Err(GitOperationError {
                message: "Git repository source changed".into(),
            })
        } else {
            match task.0.poll() {
                Some(Ok(result)) => result,
                _ => Err(GitOperationError {
                    message: "Git repository discovery worker failed".into(),
                }),
            }
        };
        let mut entity = commands.entity(entity);
        entity.remove::<DiscoveryTask>();
        match result {
            Ok(discovery) => {
                entity.insert(discovery);
            }
            Err(error) => {
                entity
                    .remove::<GitJobRunning>()
                    .insert(RepositoryOutput(Err(error)));
            }
        }
    }
}

type DiscoveredJobs<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static RepositoryJob,
        &'static RepositoryDiscovery,
        &'static RepositoryRequestIdentity,
    ),
    (With<GitJob>, Without<RepositorySubscriber>),
>;

fn join(
    jobs: DiscoveredJobs,
    reads: Query<(Entity, &RepositoryRead, Has<ReadTask>)>,
    runtime: Res<GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let mut admitted: Vec<(Entity, SourceIdentity, Instant)> = Vec::new();
    for (entity, job, discovery, identity) in &jobs {
        if !identity.source.matches(&job.path) || !discovery.source.valid() {
            commands
                .entity(entity)
                .remove::<(GitJobRunning, RepositoryDiscovery)>()
                .insert(RepositoryOutput(Err(GitOperationError {
                    message: "Git repository source changed".into(),
                })));
            continue;
        }
        let mut selected = None;
        let mut waiting = false;
        for (read_entity, read, running) in &reads {
            if read.accepts(identity, discovery) {
                selected = Some(read_entity);
                break;
            }
            if running && read.source.path == PathIdentity::resolve(discovery.repository.path()) {
                waiting = true;
            }
        }
        for (read_entity, source, started) in &admitted {
            if *started >= identity.accepted && *source == discovery.source {
                selected = Some(*read_entity);
                break;
            }
        }
        if let Some(read) = selected {
            commands.entity(entity).insert(RepositorySubscriber(read));
            continue;
        }
        if waiting {
            continue;
        }
        let source = discovery.source.clone();
        let repository = GitRepository::at(source.path.clone().into_path_buf());
        let started = Instant::now();
        let is_worktree = discovery.is_worktree;
        let Some(task) = runtime.spawn(wake.as_deref().map(|wake| (**wake).clone()), move || {
            RepositoryOutput(
                GitRepositorySnapshot::load_discovered_summary(
                    repository.path(),
                    &repository,
                    is_worktree,
                )
                .map(Arc::new)
                .map_err(|error| GitOperationError { message: error.0 }),
            )
        }) else {
            continue;
        };
        let read = commands
            .spawn((
                Name::new("git.repository.read"),
                RepositoryRead {
                    source: source.clone(),
                    started,
                },
                ReadTask(task),
            ))
            .id();
        admitted.push((read, source, started));
        commands.entity(entity).insert(RepositorySubscriber(read));
    }
}

fn poll_reads(
    mut reads: Query<(Entity, &RepositoryRead, &mut ReadTask)>,
    subscribers: Query<&RepositorySubscriber, With<GitJob>>,
    mut commands: Commands,
) {
    for (entity, read, mut task) in &mut reads {
        let attached = subscribers.iter().any(|subscriber| subscriber.0 == entity);
        let valid = read.source.valid();
        if !attached || !valid {
            task.0.cancel();
        }
        if !task.0.is_finished() {
            continue;
        }
        if !attached {
            commands.entity(entity).despawn();
            continue;
        }
        let output = if !valid {
            RepositoryOutput(Err(GitOperationError {
                message: "Git repository source changed".into(),
            }))
        } else {
            match task.0.poll() {
                Some(Ok(output)) => output,
                _ => RepositoryOutput(Err(GitOperationError {
                    message: "Git repository load worker failed".into(),
                })),
            }
        };
        commands.entity(entity).remove::<ReadTask>().insert(output);
    }
}

fn distribute(
    jobs: Query<
        (
            Entity,
            &RepositoryJob,
            &RepositoryRequestIdentity,
            &RepositorySubscriber,
        ),
        With<GitJob>,
    >,
    reads: Query<(&RepositoryRead, &RepositoryOutput)>,
    aliases: Query<(Entity, &RepositoryAlias)>,
    mut commands: Commands,
) {
    for (entity, job, identity, subscriber) in &jobs {
        let Ok((read, output)) = reads.get(subscriber.0) else {
            continue;
        };
        let output = if identity.source.matches(&job.path) && read.source.valid() {
            output.clone()
        } else {
            RepositoryOutput(Err(GitOperationError {
                message: "Git repository source changed".into(),
            }))
        };
        if output.0.is_ok() {
            let requested = PathIdentity::lexical(&job.path).into_path_buf();
            for (alias_entity, alias) in &aliases {
                if alias.requested == requested {
                    commands.entity(alias_entity).despawn();
                }
            }
            commands.spawn((
                Name::new("git.repository.alias"),
                RepositoryAlias {
                    requested,
                    source: identity.source.clone(),
                    read: subscriber.0,
                    touched: Instant::now(),
                },
            ));
        }
        commands
            .entity(entity)
            .remove::<(GitJobRunning, RepositorySubscriber, RepositoryDiscovery)>()
            .insert((
                output,
                PageRepositorySource {
                    requested: PathIdentity::lexical(&job.path).into_path_buf(),
                    request: identity.source.clone(),
                    root: read.source.clone(),
                },
            ));
    }
}

fn finish_retired(
    jobs: Query<(Entity, &RepositorySubscriber), Without<GitJob>>,
    reads: Query<(), With<ReadTask>>,
    mut commands: Commands,
) {
    for (entity, subscriber) in &jobs {
        if !reads.contains(subscriber.0) {
            commands.entity(entity).despawn();
        }
    }
}

fn refresh_aliases(
    reads: Query<(Entity, &RepositoryRead), With<RepositoryOutput>>,
    mut aliases: Query<&mut RepositoryAlias>,
) {
    for mut alias in &mut aliases {
        let Ok((_, current)) = reads.get(alias.read) else {
            continue;
        };
        let mut selected = alias.read;
        let mut latest = current.started;
        for (entity, read) in &reads {
            if read.source == current.source && read.started > latest {
                latest = read.started;
                selected = entity;
            }
        }
        alias.read = selected;
    }
}

type CompletedReads<'w, 's> =
    Query<'w, 's, (Entity, &'static RepositoryRead), (With<RepositoryOutput>, Without<ReadTask>)>;

fn evict(
    reads: CompletedReads,
    live_reads: Query<(), With<RepositoryRead>>,
    subscribers: Query<&RepositorySubscriber>,
    aliases: Query<(Entity, &RepositoryAlias)>,
    mut commands: Commands,
) {
    let mut retained = Vec::new();
    for (entity, read) in &reads {
        if subscribers.iter().any(|subscriber| subscriber.0 == entity) {
            continue;
        }
        if reads
            .iter()
            .any(|(_, newer)| newer.source == read.source && newer.started > read.started)
        {
            commands.entity(entity).despawn();
        } else {
            retained.push((read.started, entity));
        }
    }
    retained.sort_by_key(|(started, _)| *started);
    let excess = retained.len().saturating_sub(RepositoryRead::CAPACITY);
    for (_, entity) in retained.into_iter().take(excess) {
        commands.entity(entity).despawn();
    }
    let mut retained = Vec::new();
    for (entity, alias) in &aliases {
        if !live_reads.contains(alias.read) {
            commands.entity(entity).despawn();
        } else {
            retained.push((alias.touched, entity, alias.requested.clone()));
        }
    }
    retained.sort_by_key(|(touched, _, _)| std::cmp::Reverse(*touched));
    let mut requested = Vec::new();
    for (_, entity, path) in retained {
        if requested.contains(&path) || requested.len() >= RepositoryAlias::CAPACITY {
            commands.entity(entity).despawn();
        } else {
            requested.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::event::GitRepositoryRequest;
    use crate::host::controller::ControllerPlugin;
    use crate::host::job_runner::JobPlugin;
    use crate::host::repository::test_repo;
    use crate::host::state::{GitState, StatePlugin};
    use crate::state::GitUiState;
    use bevy_cef::prelude::UiInput;

    struct ReadPages(App);

    impl ReadPages {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatePlugin, JobPlugin, ControllerPlugin));
            Self(app)
        }

        fn open(&mut self, path: &Path) -> Entity {
            let page = self
                .0
                .world_mut()
                .spawn((GitState::default(), vmux_ecs::PagePresented))
                .id();
            self.request(page, path);
            page
        }

        fn request(&mut self, page: Entity, path: &Path) {
            let request_id = self
                .0
                .world()
                .get::<GitState>(page)
                .unwrap()
                .snapshot
                .repository_request_id
                + 1;
            self.0.world_mut().trigger(UiInput {
                webview: page,
                payload: GitRepositoryRequest {
                    request_id,
                    ..GitRepositoryRequest::at(path.to_string_lossy().into_owned())
                },
            });
        }

        fn wait(&mut self, page: Entity) -> GitRepositorySnapshot {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.0.update();
                let state = self.0.world().get::<GitState>(page).unwrap();
                if !state.snapshot.loading {
                    return state.snapshot.repository.clone().unwrap();
                }
                assert!(Instant::now() < deadline, "{}", state.snapshot.message);
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn hold(&mut self, root: &Path) -> (Entity, mpsc::Sender<()>) {
            self.0.world_mut().flush();
            let jobs = self
                .0
                .world_mut()
                .query_filtered::<Entity, With<RepositoryJob>>()
                .iter(self.0.world())
                .collect::<Vec<_>>();
            let (started_tx, started_rx) = mpsc::sync_channel(0);
            let (release_tx, release_rx) = mpsc::channel();
            let source = SourceIdentity::capture(root);
            let path = root.to_path_buf();
            let task = self
                .0
                .world()
                .resource::<GitRuntime>()
                .spawn(None, move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    RepositoryOutput(
                        GitRepositorySnapshot::load_summary(&path)
                            .map(Arc::new)
                            .map_err(|error| GitOperationError { message: error.0 }),
                    )
                })
                .unwrap();
            started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            let entity = self
                .0
                .world_mut()
                .spawn((
                    RepositoryRead {
                        source,
                        started: Instant::now(),
                    },
                    ReadTask(task),
                ))
                .id();
            for job in jobs {
                self.0
                    .world_mut()
                    .entity_mut(job)
                    .insert((GitJobRunning, RepositorySubscriber(entity)));
            }
            (entity, release_tx)
        }

        fn subscribed(&mut self, read: Entity, count: usize) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.0.update();
                let mut query = self.0.world_mut().query::<&RepositorySubscriber>();
                if query
                    .iter(self.0.world())
                    .filter(|subscriber| subscriber.0 == read)
                    .count()
                    == count
                {
                    return;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn closing_one_repository_page_preserves_the_other_subscriber() {
        let repo = test_repo::init();
        let nested = repo.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        test_repo::write(repo.path(), "file.txt", "untracked\n");
        let mut pages = ReadPages::new();
        let first = pages.open(repo.path());
        let second = pages.open(&nested);
        let (read, release) = pages.hold(repo.path());
        pages.subscribed(read, 2);
        pages.0.world_mut().entity_mut(first).remove::<GitState>();
        pages.0.update();
        assert!(pages.0.world().get_entity(read).is_ok());
        release.send(()).unwrap();
        let repository = pages.wait(second);
        assert_eq!(repository.branch, "main");
        assert!(
            repository
                .files
                .iter()
                .any(|file| file.path_bytes == b"file.txt")
        );
        assert_eq!(
            PathIdentity::resolve(&repository.repo_root),
            PathIdentity::resolve(repo.path())
        );
    }

    #[test]
    fn the_last_subscriber_and_native_read_survive_until_drain() {
        let repo = test_repo::init();
        let mut pages = ReadPages::new();
        let page = pages.open(repo.path());
        let (read, release) = pages.hold(repo.path());
        pages.subscribed(read, 1);
        let job = pages
            .0
            .world_mut()
            .query::<(Entity, &RepositorySubscriber)>()
            .iter(pages.0.world())
            .find(|(_, subscriber)| subscriber.0 == read)
            .unwrap()
            .0;
        pages.0.world_mut().entity_mut(page).remove::<GitState>();
        pages.0.update();
        assert!(pages.0.world().get_entity(read).is_ok());
        assert!(pages.0.world().get_entity(job).is_ok());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while pages.0.world().get_entity(job).is_ok() || pages.0.world().get_entity(read).is_ok() {
            pages.0.update();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_newer_repository_request_waits_for_retired_io_to_drain() {
        let repo = test_repo::init();
        test_repo::run(repo.path(), &["commit", "--allow-empty", "-qm", "base"]);
        let mut pages = ReadPages::new();
        let page = pages.open(repo.path());
        let (read, release) = pages.hold(repo.path());
        pages.subscribed(read, 1);
        test_repo::run(repo.path(), &["checkout", "-qb", "new-branch"]);
        pages.request(page, repo.path());
        pages.0.update();
        assert!(pages.0.world().get_entity(read).is_ok());
        assert!(
            pages
                .0
                .world()
                .get::<GitState>(page)
                .unwrap()
                .snapshot
                .loading
        );
        release.send(()).unwrap();
        assert_eq!(pages.wait(page).branch, "new-branch");
        assert!(pages.0.world().get_entity(read).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_retargeted_requested_alias_cannot_publish_another_repository() {
        let first = test_repo::init();
        let second = test_repo::init();
        let parent = tempfile::tempdir().unwrap();
        let alias = parent.path().join("repository");
        std::os::unix::fs::symlink(first.path(), &alias).unwrap();
        let mut pages = ReadPages::new();
        let page = pages.open(&alias);
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(second.path(), &alias).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            pages.0.update();
            let state = pages.0.world().get::<GitState>(page).unwrap();
            if !state.snapshot.loading {
                assert_eq!(state.snapshot.message, "Git repository source changed");
                assert!(state.repository().is_none());
                break;
            }
            assert!(Instant::now() < deadline);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_loaded_page_context_refuses_a_retargeted_repository_alias() {
        let first = test_repo::init();
        let second = test_repo::init();
        test_repo::run(
            second.path(),
            &["symbolic-ref", "HEAD", "refs/heads/second"],
        );
        let parent = tempfile::tempdir().unwrap();
        let alias = parent.path().join("repository");
        std::os::unix::fs::symlink(first.path(), &alias).unwrap();
        let mut pages = ReadPages::new();
        let page = pages.open(&alias);
        assert_eq!(pages.wait(page).branch, "main");
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(second.path(), &alias).unwrap();
        pages
            .0
            .world_mut()
            .trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
                page,
                &crate::state::GitPageContext {
                    working_directory: alias.to_string_lossy().into_owned(),
                    page_url: crate::GIT_PAGE_URL.into(),
                },
            ));
        let repository = pages.wait(page);
        assert_eq!(repository.branch, "second");
        assert_eq!(
            PathIdentity::resolve(&repository.repo_root),
            PathIdentity::resolve(second.path())
        );
    }

    #[cfg(unix)]
    #[test]
    fn retention_uses_the_accepted_alias_identity_before_page_removal() {
        let first = test_repo::init();
        let second = test_repo::init();
        let parent = tempfile::tempdir().unwrap();
        let alias = parent.path().join("repository");
        std::os::unix::fs::symlink(first.path(), &alias).unwrap();
        let mut pages = ReadPages::new();
        let page = pages.open(&alias);
        assert_eq!(pages.wait(page).branch, "main");
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(second.path(), &alias).unwrap();
        pages.0.world_mut().entity_mut(page).remove::<GitState>();
        let mut context = bevy::ecs::system::SystemState::<super::super::state::RetainedPages>::new(
            pages.0.world_mut(),
        );
        assert!(
            context
                .get_mut(pages.0.world_mut())
                .unwrap()
                .restore(&alias.to_string_lossy())
                .is_none()
        );
    }

    #[test]
    fn linked_worktrees_keep_their_own_repository_observations() {
        let repo = test_repo::init();
        test_repo::run(repo.path(), &["commit", "--allow-empty", "-qm", "base"]);
        let temporary = tempfile::tempdir().unwrap();
        let linked = temporary.path().join("linked");
        test_repo::run(
            repo.path(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "linked",
                linked.to_str().unwrap(),
            ],
        );
        let mut pages = ReadPages::new();
        let main = pages.open(repo.path());
        let worktree = pages.open(&linked);
        assert_eq!(pages.wait(main).branch, "main");
        let linked = pages.wait(worktree);
        assert_eq!(linked.branch, "linked");
        assert!(linked.is_worktree);
    }

    #[test]
    fn a_nested_repository_read_primes_a_new_root_request() {
        let repo = test_repo::init();
        let nested = repo.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        test_repo::write(repo.path(), "file.txt", "untracked\n");
        let mut pages = ReadPages::new();
        let page = pages.open(&nested);
        pages.wait(page);
        pages.request(page, repo.path());
        pages.0.world_mut().flush();
        let state = pages.0.world().get::<GitState>(page).unwrap();
        assert!(state.snapshot.loading);
        let repository = state.repository().unwrap();
        assert_eq!(repository.branch, "main");
        assert!(
            repository
                .files
                .iter()
                .any(|file| file.path_bytes == b"file.txt")
        );
    }

    #[test]
    fn a_replaced_discovered_git_marker_refuses_read_admission() {
        let repo = test_repo::init();
        let nested = repo.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        let mut pages = ReadPages::new();
        let page = pages.open(&nested);
        pages.0.world_mut().flush();
        let job = pages
            .0
            .world_mut()
            .query_filtered::<Entity, With<RepositoryJob>>()
            .single(pages.0.world())
            .unwrap();
        let (repository, is_worktree) = GitRepository::discover_with_worktree(&nested).unwrap();
        let source = SourceIdentity::capture(repository.path());
        pages.0.world_mut().entity_mut(job).insert((
            GitJobRunning,
            RepositoryDiscovery {
                repository,
                is_worktree,
                source,
            },
        ));
        let saved = tempfile::tempdir().unwrap();
        std::fs::rename(repo.path().join(".git"), saved.path().join("old-git")).unwrap();
        test_repo::run(repo.path(), &["init", "-q", "--initial-branch=replacement"]);
        pages.0.update();
        let state = pages.0.world().get::<GitState>(page).unwrap();
        assert!(!state.snapshot.loading);
        assert_eq!(state.snapshot.message, "Git repository source changed");
        assert!(state.repository().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_changed_context_source_is_refused_before_probe_admission() {
        let first = test_repo::init();
        let second = test_repo::init();
        let parent = tempfile::tempdir().unwrap();
        let alias = parent.path().join("repository");
        std::os::unix::fs::symlink(first.path(), &alias).unwrap();
        let mut pages = ReadPages::new();
        let page = pages.0.world_mut().spawn(GitState::default()).id();
        pages
            .0
            .world_mut()
            .trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
                page,
                &crate::state::GitPageContext {
                    working_directory: alias.to_string_lossy().into_owned(),
                    page_url: crate::GIT_PAGE_URL.into(),
                },
            ));
        pages.0.world_mut().flush();
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(second.path(), &alias).unwrap();
        pages.0.update();
        let state = pages.0.world().get::<GitState>(page).unwrap();
        assert!(!state.snapshot.loading);
        assert_eq!(state.snapshot.message, "Git repository source changed");
        assert!(state.repository().is_none());
    }

    #[test]
    fn rewriting_a_worktree_git_file_refuses_its_retained_page() {
        let first = test_repo::init();
        let second = test_repo::init();
        test_repo::run(first.path(), &["commit", "--allow-empty", "-qm", "base"]);
        let temporary = tempfile::tempdir().unwrap();
        let linked = temporary.path().join("linked");
        test_repo::run(
            first.path(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "linked",
                linked.to_str().unwrap(),
            ],
        );
        let mut pages = ReadPages::new();
        let page = pages.open(&linked);
        assert_eq!(pages.wait(page).branch, "linked");
        let marker = linked.join(".git");
        let previous_inode = std::fs::metadata(&marker).unwrap().ino();
        std::fs::write(
            &marker,
            format!("gitdir: {}\n", second.path().join(".git").display()),
        )
        .unwrap();
        assert_eq!(std::fs::metadata(&marker).unwrap().ino(), previous_inode);
        pages.0.world_mut().entity_mut(page).remove::<GitState>();
        pages.0.world_mut().flush();
        let mut context = bevy::ecs::system::SystemState::<super::super::state::RetainedPages>::new(
            pages.0.world_mut(),
        );
        assert!(
            context
                .get_mut(pages.0.world_mut())
                .unwrap()
                .restore(&linked.to_string_lossy())
                .is_none()
        );
    }
}
