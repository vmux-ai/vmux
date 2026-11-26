use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Instant;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::{CommandBarUiState, StartBranchesRequest, StartProjectBranches};
use vmux_api::space::ProjectBranch;
use vmux_ecs::{
    IoTask, IoTaskPlugin, IoTaskSet, Operation, OperationRetained, OperationState, OperationTarget,
    PageMetadata, TaskError,
};
use vmux_git::worktree::{CheckoutInfo, WorktreeRegistration};
use vmux_git::{GitRuntime, GitTask, RepositorySource};

pub(crate) struct BranchesPlugin;

impl Plugin for BranchesPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.add_plugins(vmux_runner::WinitWakePlugin)
            .init_resource::<GitRuntime>()
            .add_plugins((
                UiEventPlugin::<(StartBranchesRequest,)>::default(),
                IoTaskPlugin::<(DiscoveryOutput, RefsOutput, RegistrationsOutput)>::default(),
            ))
            .add_observer(request)
            .add_systems(
                Update,
                (
                    retire,
                    discover,
                    materialize_refs,
                    materialize_registrations,
                    publish,
                    cleanup,
                    read_refs,
                    read_registrations,
                    start,
                )
                    .chain()
                    .after(IoTaskSet::all()),
            );
    }
}

#[derive(Component)]
struct CurrentRequest {
    request_id: u64,
    project: String,
    page_url: Option<String>,
}

#[derive(Component)]
struct BranchRequest {
    owner: Entity,
    request_id: u64,
    project: String,
    source: RepositorySource,
    accepted: Instant,
}

impl BranchRequest {
    fn current(&self, current: &CurrentRequest, page: Option<&PageMetadata>) -> bool {
        self.request_id == current.request_id
            && self.project == current.project
            && current.page_url.as_deref() == page.map(|page| page.url.as_str())
    }
}

#[derive(Component)]
struct BranchRead {
    source: RepositorySource,
    started: Instant,
}

#[derive(Component)]
struct BranchSubscriber(Entity);

#[derive(Component)]
struct RequestRetired;

#[derive(Component)]
struct RequestRefused;

#[derive(Component)]
struct ReadRetired;

#[derive(Component)]
struct LookupFailure(String);

#[derive(Component, Default)]
struct CancelWork {
    discovery: Option<Arc<Mutex<GitTask<()>>>>,
    refs: Option<Arc<Mutex<GitTask<()>>>>,
    registrations: Option<Arc<Mutex<GitTask<()>>>>,
}

impl CancelWork {
    fn cancel(&self) {
        for task in [&self.discovery, &self.refs, &self.registrations] {
            if let Some(task) = task
                && let Ok(mut task) = task.lock()
            {
                task.cancel();
            }
        }
    }
}

struct BridgeState<T> {
    result: Option<Result<T, String>>,
    finished: bool,
    waker: Option<Waker>,
}

struct BridgeSignal<T>(Arc<Mutex<BridgeState<T>>>);

impl<T> BridgeSignal<T> {
    fn complete(self, outcome: Result<T, String>) {
        let waker = if let Ok(mut state) = self.0.lock() {
            state.result = Some(outcome);
            state.finished = true;
            state.waker.take()
        } else {
            None
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl<T> Drop for BridgeSignal<T> {
    fn drop(&mut self) {
        let waker = if let Ok(mut state) = self.0.lock()
            && !state.finished
        {
            state.finished = true;
            state.waker.take()
        } else {
            None
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

struct GitBridge<T> {
    state: Arc<Mutex<BridgeState<T>>>,
    _task: Arc<Mutex<GitTask<()>>>,
}

impl<T: Component> GitBridge<T> {
    fn spawn(
        runtime: &GitRuntime,
        run: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Option<(IoTask<T>, Arc<Mutex<GitTask<()>>>)> {
        let state = Arc::new(Mutex::new(BridgeState {
            result: None,
            finished: false,
            waker: None,
        }));
        let signal = BridgeSignal(Arc::clone(&state));
        let task = runtime.spawn_background(None, move || {
            let signal = signal;
            let outcome = run();
            signal.complete(outcome);
        })?;
        let handle = Arc::new(Mutex::new(task));
        let bridge = Self {
            state,
            _task: Arc::clone(&handle),
        };
        Some((IoTask::spawn(bridge), handle))
    }
}

impl<T> Unpin for GitBridge<T> {}

impl<T> Future for GitBridge<T> {
    type Output = Result<T, String>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let Ok(mut state) = self.state.lock() else {
            return Poll::Ready(Err("Start branch lookup worker failed".into()));
        };
        if state.finished {
            return Poll::Ready(
                state
                    .result
                    .take()
                    .unwrap_or_else(|| Err("Start branch lookup worker failed".into())),
            );
        }
        state.waker = Some(context.waker().clone());
        Poll::Pending
    }
}

#[derive(Component)]
struct DiscoveryOutput {
    checkout: CheckoutInfo,
    source: RepositorySource,
}

#[derive(Component)]
struct Discovered {
    checkout: CheckoutInfo,
    source: RepositorySource,
}

#[derive(Component)]
struct RefsOutput(Vec<String>);

#[derive(Component)]
struct RegistrationsOutput(Vec<WorktreeRegistration>);

#[derive(Component)]
struct RefsComplete;

#[derive(Component)]
struct RegistrationsComplete;

#[derive(Component)]
struct Observation(Entity);

#[derive(Component)]
struct BranchRef;

#[derive(Component)]
struct Registration {
    path: PathBuf,
    branch: Option<String>,
    prunable: bool,
    directory: bool,
}

fn request(
    trigger: On<UiInput<StartBranchesRequest>>,
    pages: Query<(Option<&CurrentRequest>, Option<&PageMetadata>)>,
    mut commands: Commands,
) {
    let owner = trigger.event().webview;
    let Ok((current, page)) = pages.get(owner) else {
        return;
    };
    let input = &trigger.event().payload;
    let project = input.project.clone();
    if project.trim().is_empty() || input.request_id == 0 {
        return;
    }
    if current.is_some_and(|current| input.request_id <= current.request_id) {
        return;
    }
    let source = RepositorySource::capture(Path::new(&project));
    commands.entity(owner).insert(CurrentRequest {
        request_id: input.request_id,
        project: project.clone(),
        page_url: page.map(|page| page.url.clone()),
    });
    commands.spawn((
        Name::new("start.branch.request"),
        vmux_ecs::Active,
        Operation::bundle("start.branches").unwrap(),
        OperationTarget::new(owner),
        OperationRetained,
        BranchRequest {
            owner,
            request_id: input.request_id,
            project,
            source,
            accepted: Instant::now(),
        },
    ));
}

fn retire(
    requests: Query<(Entity, &BranchRequest), Without<RequestRetired>>,
    current: Query<(&CurrentRequest, Option<&PageMetadata>)>,
    reads: Query<
        (
            Entity,
            &BranchRead,
            Option<&Discovered>,
            Option<&CancelWork>,
        ),
        Without<ReadRetired>,
    >,
    subscribers: Query<(&BranchSubscriber, Has<RequestRetired>, Has<RequestRefused>)>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        if !current
            .get(request.owner)
            .is_ok_and(|(current, page)| request.current(current, page))
        {
            commands
                .entity(entity)
                .insert((RequestRetired, OperationState::Canceled));
        } else if !request.source.matches(Path::new(&request.project)) {
            commands.entity(entity).insert(RequestRefused);
        }
    }
    for (entity, read, discovered, work) in &reads {
        if !read.source.matches(read.source.path())
            || discovered
                .is_some_and(|discovered| !discovered.source.matches(discovered.source.path()))
        {
            if let Some(work) = work {
                work.cancel();
            }
            commands.entity(entity).insert((
                ReadRetired,
                LookupFailure("Git repository source changed".into()),
            ));
        } else if !subscribers
            .iter()
            .any(|(subscriber, retired, refused)| !retired && !refused && subscriber.0 == entity)
        {
            if let Some(work) = work {
                work.cancel();
            }
            commands.entity(entity).insert(ReadRetired);
        }
    }
}

type DiscoveryResults<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static BranchRead,
        Option<&'static DiscoveryOutput>,
        Option<&'static TaskError<DiscoveryOutput>>,
        Has<ReadRetired>,
        Option<&'static CancelWork>,
    ),
    Or<(With<DiscoveryOutput>, With<TaskError<DiscoveryOutput>>)>,
>;

fn discover(outputs: DiscoveryResults, mut commands: Commands) {
    for (entity, read, output, error, retired, work) in &outputs {
        commands
            .entity(entity)
            .remove::<(DiscoveryOutput, TaskError<DiscoveryOutput>)>();
        if retired {
            continue;
        }
        if let Some(error) = error {
            if let Some(work) = work {
                work.cancel();
            }
            commands
                .entity(entity)
                .insert((ReadRetired, LookupFailure(error.to_string())));
        } else if let Some(output) = output {
            if read.source.matches(read.source.path())
                && output.source.matches(output.source.path())
            {
                commands.entity(entity).insert(Discovered {
                    checkout: output.checkout.clone(),
                    source: output.source.clone(),
                });
            } else {
                if let Some(work) = work {
                    work.cancel();
                }
                commands.entity(entity).insert((
                    ReadRetired,
                    LookupFailure("Git repository source changed".into()),
                ));
            }
        }
    }
}

type RefsResults<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static RefsOutput>,
        Option<&'static TaskError<RefsOutput>>,
        Has<ReadRetired>,
        Option<&'static CancelWork>,
    ),
    Or<(With<RefsOutput>, With<TaskError<RefsOutput>>)>,
>;

fn materialize_refs(outputs: RefsResults, mut commands: Commands) {
    for (read, output, error, retired, work) in &outputs {
        commands
            .entity(read)
            .remove::<(RefsOutput, TaskError<RefsOutput>)>();
        if retired {
            continue;
        }
        if let Some(error) = error {
            if let Some(work) = work {
                work.cancel();
            }
            commands
                .entity(read)
                .insert((ReadRetired, LookupFailure(error.to_string())));
        } else if let Some(output) = output {
            for (index, branch) in output.0.iter().enumerate() {
                commands.spawn((
                    Name::new(branch.clone()),
                    vmux_ecs::Order(index as u32),
                    Observation(read),
                    BranchRef,
                ));
            }
            commands.entity(read).insert(RefsComplete);
        }
    }
}

type RegistrationsResults<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static RegistrationsOutput>,
        Option<&'static TaskError<RegistrationsOutput>>,
        Has<ReadRetired>,
        Option<&'static CancelWork>,
    ),
    Or<(
        With<RegistrationsOutput>,
        With<TaskError<RegistrationsOutput>>,
    )>,
>;

fn materialize_registrations(outputs: RegistrationsResults, mut commands: Commands) {
    for (read, output, error, retired, work) in &outputs {
        commands
            .entity(read)
            .remove::<(RegistrationsOutput, TaskError<RegistrationsOutput>)>();
        if retired {
            continue;
        }
        if let Some(error) = error {
            if let Some(work) = work {
                work.cancel();
            }
            commands
                .entity(read)
                .insert((ReadRetired, LookupFailure(error.to_string())));
        } else if let Some(output) = output {
            for (index, registration) in output.0.iter().enumerate() {
                commands.spawn((
                    Name::new("start.branch.registration"),
                    vmux_ecs::Order(index as u32),
                    Observation(read),
                    Registration {
                        path: registration.path.clone(),
                        branch: registration.branch.clone(),
                        prunable: registration.prunable,
                        directory: registration.path.is_dir(),
                    },
                ));
            }
            commands.entity(read).insert(RegistrationsComplete);
        }
    }
}

type FinishedReads<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static BranchRead,
        Option<&'static Discovered>,
        Option<&'static LookupFailure>,
        Has<ReadRetired>,
    ),
    (
        With<BranchRead>,
        Without<IoTask<DiscoveryOutput>>,
        Without<IoTask<RefsOutput>>,
        Without<IoTask<RegistrationsOutput>>,
        Or<(
            With<ReadRetired>,
            (With<RefsComplete>, With<RegistrationsComplete>),
        )>,
    ),
>;

fn publish(
    reads: FinishedReads,
    requests: Query<
        (
            Entity,
            &BranchRequest,
            &BranchSubscriber,
            Has<RequestRefused>,
        ),
        Without<RequestRetired>,
    >,
    current: Query<(&CurrentRequest, Option<&PageMetadata>)>,
    refs: Query<(&Observation, &Name, &vmux_ecs::Order), With<BranchRef>>,
    registrations: Query<(&Observation, &Registration, &vmux_ecs::Order)>,
    mut commands: Commands,
) {
    for (read, state, discovered, failure, retired) in &reads {
        let mut ordered = Vec::new();
        for (owner, name, order) in &refs {
            if owner.0 == read {
                ordered.push((order.0, name.as_str()));
            }
        }
        ordered.sort_by_key(|(order, _)| *order);
        let mut holders = HashMap::new();
        for (owner, registration, order) in &registrations {
            if owner.0 != read || registration.prunable || !registration.directory {
                continue;
            }
            let Some(branch) = registration.branch.as_deref() else {
                continue;
            };
            if holders
                .get(branch)
                .is_none_or(|(_, earlier)| order.0 < *earlier)
            {
                holders.insert(branch, (registration, order.0));
            }
        }
        let mut branches = Vec::new();
        if failure.is_none() && !retired {
            for (_, branch) in ordered {
                let checkout = holders
                    .get(branch)
                    .map(|(registration, _)| &registration.path);
                branches.push(ProjectBranch {
                    branch: branch.to_owned(),
                    checkout: checkout
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    label: checkout
                        .and_then(|path| path.file_name())
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    insertions: 0,
                    deletions: 0,
                });
            }
        }
        for (entity, request, subscriber, refused) in &requests {
            if subscriber.0 != read {
                continue;
            }
            let current = current
                .get(request.owner)
                .is_ok_and(|(current, page)| request.current(current, page));
            let accepted = !refused
                && !retired
                && failure.is_none()
                && request.source.matches(Path::new(&request.project))
                && state.source.matches(state.source.path())
                && discovered
                    .is_some_and(|discovered| discovered.source.matches(discovered.source.path()));
            if current {
                let result = if accepted {
                    branches.clone()
                } else {
                    Vec::new()
                };
                if let Some(failure) = failure {
                    warn!(
                        "Start branch lookup failed for {}: {}",
                        request.project, failure.0
                    );
                }
                commands.trigger(vmux_ecs::UiStateWrite::<CommandBarUiState>::from_event(
                    request.owner,
                    &StartProjectBranches {
                        project: request.project.clone(),
                        request_id: request.request_id,
                        branches: result,
                    },
                ));
            }
            let outcome = if !current {
                OperationState::Canceled
            } else if accepted {
                OperationState::Succeeded
            } else {
                OperationState::Failed
            };
            commands
                .entity(entity)
                .remove::<(
                    BranchRequest,
                    BranchSubscriber,
                    RequestRefused,
                    OperationRetained,
                )>()
                .insert(outcome);
        }
    }
}

fn cleanup(
    requests: Query<(Entity, Option<&BranchSubscriber>), With<RequestRetired>>,
    reads: Query<
        (
            Entity,
            Has<IoTask<DiscoveryOutput>>,
            Has<IoTask<RefsOutput>>,
            Has<IoTask<RegistrationsOutput>>,
        ),
        With<BranchRead>,
    >,
    subscribers: Query<&BranchSubscriber>,
    observations: Query<(Entity, &Observation)>,
    mut commands: Commands,
) {
    for (entity, subscriber) in &requests {
        if subscriber.is_none_or(|subscriber| {
            reads.get(subscriber.0).is_err()
                || reads
                    .get(subscriber.0)
                    .is_ok_and(|(_, discovery, refs, registrations)| {
                        !discovery && !refs && !registrations
                    })
        }) {
            commands.entity(entity).remove::<(
                BranchRequest,
                BranchSubscriber,
                RequestRetired,
                OperationRetained,
            )>();
        }
    }
    for (read, discovery, refs, registrations) in &reads {
        if discovery
            || refs
            || registrations
            || subscribers.iter().any(|subscriber| subscriber.0 == read)
        {
            continue;
        }
        for (entity, owner) in &observations {
            if owner.0 == read {
                commands.entity(entity).despawn();
            }
        }
        commands.entity(read).despawn();
    }
}

fn read_refs(
    mut reads: Query<
        (Entity, &Discovered, &mut CancelWork),
        (
            Without<ReadRetired>,
            Without<RefsComplete>,
            Without<RefsOutput>,
            Without<TaskError<RefsOutput>>,
            Without<IoTask<RefsOutput>>,
        ),
    >,
    runtime: Res<GitRuntime>,
    mut commands: Commands,
) {
    for (entity, discovered, mut work) in &mut reads {
        let checkout = discovered.checkout.clone();
        let Some((task, handle)) = GitBridge::spawn(&runtime, move || {
            checkout
                .local_branches()
                .map(RefsOutput)
                .map_err(|error| error.0)
        }) else {
            continue;
        };
        work.refs = Some(handle);
        commands.entity(entity).insert(task);
    }
}

fn read_registrations(
    mut reads: Query<
        (Entity, &Discovered, &mut CancelWork),
        (
            Without<ReadRetired>,
            Without<RegistrationsComplete>,
            Without<RegistrationsOutput>,
            Without<TaskError<RegistrationsOutput>>,
            Without<IoTask<RegistrationsOutput>>,
        ),
    >,
    runtime: Res<GitRuntime>,
    mut commands: Commands,
) {
    for (entity, discovered, mut work) in &mut reads {
        let checkout = discovered.checkout.clone();
        let Some((task, handle)) = GitBridge::spawn(&runtime, move || {
            checkout
                .worktree_registrations()
                .map(RegistrationsOutput)
                .map_err(|error| error.0)
        }) else {
            continue;
        };
        work.registrations = Some(handle);
        commands.entity(entity).insert(task);
    }
}

fn start(
    requests: Query<(Entity, &BranchRequest), (Without<RequestRetired>, Without<BranchSubscriber>)>,
    reads: Query<(
        Entity,
        &BranchRead,
        Has<ReadRetired>,
        Has<RefsComplete>,
        Has<RegistrationsComplete>,
    )>,
    runtime: Res<GitRuntime>,
    mut commands: Commands,
) {
    let mut admitted = Vec::new();
    for (entity, request) in &requests {
        let mut selected = None;
        let mut waiting = false;
        for (read, state, retired, refs, registrations) in &reads {
            if state.source == request.source {
                if !retired && state.started >= request.accepted {
                    selected = Some(read);
                    break;
                }
                if !refs || !registrations {
                    waiting = true;
                }
            }
        }
        for (read, source, started) in &admitted {
            if *source == request.source && *started >= request.accepted {
                selected = Some(*read);
                break;
            }
        }
        if let Some(read) = selected {
            commands
                .entity(entity)
                .insert((BranchSubscriber(read), OperationState::Running));
            continue;
        }
        if waiting {
            continue;
        }
        if !request.source.matches(Path::new(&request.project)) {
            let read = commands
                .spawn((
                    Name::new("start.branch.refused"),
                    BranchRead {
                        source: request.source.clone(),
                        started: Instant::now(),
                    },
                    ReadRetired,
                    LookupFailure("Git repository source changed".into()),
                ))
                .id();
            commands.entity(entity).insert(BranchSubscriber(read));
            continue;
        }
        let source = request.source.clone();
        let path = source.path().to_path_buf();
        let started = Instant::now();
        let Some((task, handle)) = GitBridge::spawn(&runtime, move || {
            CheckoutInfo::try_from(path.as_path())
                .map(|checkout| {
                    let observed = RepositorySource::capture(&checkout.root);
                    DiscoveryOutput {
                        checkout,
                        source: observed,
                    }
                })
                .map_err(|error| error.0)
        }) else {
            continue;
        };
        let read = commands
            .spawn((
                Name::new("start.branch.read"),
                BranchRead {
                    source: source.clone(),
                    started,
                },
                CancelWork {
                    discovery: Some(handle),
                    refs: None,
                    registrations: None,
                },
                task,
            ))
            .id();
        admitted.push((read, source, started));
        commands
            .entity(entity)
            .insert((BranchSubscriber(read), OperationState::Running));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::Duration;

    struct Repository(tempfile::TempDir);

    impl Repository {
        fn new() -> Self {
            Self::with_suffix("")
        }

        fn with_suffix(suffix: &str) -> Self {
            let repo = Self(tempfile::Builder::new().suffix(suffix).tempdir().unwrap());
            repo.run(&["init", "-qb", "main"]);
            repo.run(&["config", "user.name", "Fixture"]);
            repo.run(&["config", "user.email", "fixture@example.test"]);
            repo.run(&[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--allow-empty",
                "-qm",
                "base",
            ]);
            repo.run(&["branch", "topic"]);
            repo
        }

        fn path(&self) -> &Path {
            self.0.path()
        }

        fn run(&self, args: &[&str]) {
            let output = Command::new("git")
                .arg("-C")
                .arg(self.path())
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[derive(Resource, Default)]
    struct Received(Vec<(Entity, StartProjectBranches)>);

    impl Received {
        fn record(
            event: On<vmux_ecs::UiStateWrite<CommandBarUiState>>,
            mut received: ResMut<Self>,
        ) {
            if let Some(branches) = &event.event().update().project_branches {
                received
                    .0
                    .push((event.event().webview(), (**branches).clone()));
            }
        }
    }

    struct BlockedWorkers {
        releases: Vec<mpsc::Sender<()>>,
        tasks: Vec<GitTask<()>>,
    }

    impl BlockedWorkers {
        fn new(runtime: &GitRuntime) -> Self {
            let mut releases = Vec::new();
            let mut tasks = Vec::new();
            for _ in 0..2 {
                let (release, resume) = mpsc::channel();
                let (started, ready) = mpsc::channel();
                let task = runtime
                    .spawn_background(None, move || {
                        started.send(()).unwrap();
                        resume.recv_timeout(Duration::from_secs(10)).unwrap();
                    })
                    .unwrap();
                ready.recv_timeout(Duration::from_secs(2)).unwrap();
                tasks.push(task);
                releases.push(release);
            }
            Self { releases, tasks }
        }
    }

    impl Drop for BlockedWorkers {
        fn drop(&mut self) {
            for release in &self.releases {
                let _ = release.send(());
            }
            for task in &mut self.tasks {
                task.cancel();
            }
        }
    }

    struct BranchPages(App, Repository);

    impl BranchPages {
        fn new(repository: Repository) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, BranchesPlugin))
                .init_resource::<Received>()
                .add_observer(Received::record);
            Self(app, repository)
        }

        fn page(&mut self) -> Entity {
            self.0.world_mut().spawn_empty().id()
        }

        fn request(&mut self, owner: Entity, path: &Path, request_id: u64) {
            self.0.world_mut().trigger(UiInput {
                webview: owner,
                payload: StartBranchesRequest {
                    project: path.to_string_lossy().into_owned(),
                    request_id,
                },
            });
            self.0.world_mut().flush();
        }

        fn wait(&mut self, owner: Entity, request_id: u64) -> StartProjectBranches {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.0.update();
                if let Some((_, output)) = self
                    .0
                    .world()
                    .resource::<Received>()
                    .0
                    .iter()
                    .find(|(target, output)| *target == owner && output.request_id == request_id)
                {
                    return output.clone();
                }
                assert!(Instant::now() < deadline, "branch request did not complete");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn blocked(&self) -> BlockedWorkers {
            BlockedWorkers::new(self.0.world().resource::<GitRuntime>())
        }

        fn operation(&mut self, owner: Entity) -> Entity {
            self.0
                .world_mut()
                .query::<(Entity, &BranchRequest)>()
                .iter(self.0.world())
                .find(|(_, request)| request.owner == owner)
                .unwrap()
                .0
        }

        fn swap_metadata_before_publish(&mut self, replacement: &Path) {
            self.0.add_systems(
                Update,
                MetadataSwap::apply
                    .after(materialize_registrations)
                    .before(publish),
            );
            let directory = self.1.path().canonicalize().unwrap();
            self.0.world_mut().spawn(MetadataSwap {
                target: directory.join(".git"),
                replacement: replacement.join(".git"),
                backup: directory.join("parked-git"),
            });
        }
    }

    #[derive(Component)]
    struct MetadataSwap {
        target: PathBuf,
        replacement: PathBuf,
        backup: PathBuf,
    }

    impl MetadataSwap {
        fn apply(
            swaps: Query<(Entity, &Self)>,
            completed: Query<
                Entity,
                (
                    With<BranchRead>,
                    With<RefsComplete>,
                    With<RegistrationsComplete>,
                ),
            >,
            requests: Query<(&BranchRequest, &BranchSubscriber)>,
            mut commands: Commands,
        ) {
            if completed.is_empty() {
                return;
            }
            for (entity, swap) in &swaps {
                let directory = swap.target.parent().unwrap();
                if !requests.iter().any(|(request, subscriber)| {
                    request.source.path().starts_with(directory) && completed.contains(subscriber.0)
                }) {
                    continue;
                }
                std::fs::rename(&swap.target, &swap.backup).unwrap();
                std::fs::rename(&swap.replacement, &swap.target).unwrap();
                commands.entity(entity).despawn();
            }
        }
    }

    #[test]
    fn significant_project_path_spaces_preserve_request_and_native_refs() {
        let repo = Repository::with_suffix(" ");
        let path = repo.path().to_path_buf();
        let mut pages = BranchPages::new(repo);
        let page = pages.page();
        pages.request(page, &path, 1);
        let response = pages.wait(page, 1);
        assert_eq!(response.project, path.to_string_lossy());
        assert!(
            response
                .branches
                .iter()
                .any(|branch| branch.branch == "topic")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_temporarily_retargeted_alias_stays_refused_after_native_drain() {
        let repo = Repository::new();
        let other = Repository::new();
        let path = repo.path().to_path_buf();
        let temporary = tempfile::tempdir().unwrap();
        let alias = temporary.path().join("project");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        let mut pages = BranchPages::new(repo);
        let blocked = pages.blocked();
        let page = pages.page();
        pages.request(page, &alias, 1);
        let operation = pages.operation(page);
        pages.0.update();
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(other.path(), &alias).unwrap();
        pages.0.update();
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        pages.0.update();
        drop(blocked);
        assert!(pages.wait(page, 1).branches.is_empty());
        assert_eq!(
            pages.0.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
    }

    #[cfg(unix)]
    #[test]
    fn refusing_one_alias_does_not_refuse_its_shared_repository_subscriber() {
        let repo = Repository::new();
        let other = Repository::new();
        let path = repo.path().to_path_buf();
        let temporary = tempfile::tempdir().unwrap();
        let alias = temporary.path().join("project");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        let mut pages = BranchPages::new(repo);
        let blocked = pages.blocked();
        let refused = pages.page();
        let accepted = pages.page();
        pages.request(refused, &alias, 1);
        pages.request(accepted, &path, 1);
        let operation = pages.operation(refused);
        pages.0.update();
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(other.path(), &alias).unwrap();
        pages.0.update();
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        pages.0.update();
        drop(blocked);
        assert!(pages.wait(refused, 1).branches.is_empty());
        assert!(
            pages
                .wait(accepted, 1)
                .branches
                .iter()
                .any(|branch| branch.branch == "topic")
        );
        assert_eq!(
            pages.0.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
    }

    #[test]
    fn repository_metadata_replacement_before_publication_refuses_subdirectory_refs() {
        let repo = Repository::new();
        let other = Repository::new();
        let path = repo.path().join("nested");
        std::fs::create_dir(&path).unwrap();
        let mut pages = BranchPages::new(repo);
        pages.swap_metadata_before_publish(other.path());
        let page = pages.page();
        pages.request(page, &path, 1);
        let operation = pages.operation(page);
        assert!(pages.wait(page, 1).branches.is_empty());
        assert_eq!(
            pages.0.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
    }

    #[test]
    fn both_repository_subscribers_receive_actual_refs_and_checkout_holders() {
        let repo = Repository::new();
        let linked = tempfile::tempdir().unwrap();
        let path = linked.path().join("linked");
        repo.run(&[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            path.to_str().unwrap(),
        ]);
        let repo_path = repo.path().to_path_buf();
        let mut pages = BranchPages::new(repo);
        let first = pages.page();
        let second = pages.page();
        pages.request(first, &repo_path, 1);
        pages.request(second, &repo_path, 1);
        let first = pages.wait(first, 1);
        let second = pages.wait(second, 1);
        for output in [first, second] {
            assert_eq!(output.branches.len(), 3);
            let linked = output
                .branches
                .iter()
                .find(|branch| branch.branch == "linked")
                .unwrap();
            assert_eq!(
                PathBuf::from(&linked.checkout),
                path.canonicalize().unwrap()
            );
            let topic = output
                .branches
                .iter()
                .find(|branch| branch.branch == "topic")
                .unwrap();
            assert!(topic.checkout.is_empty());
        }
    }

    #[test]
    fn duplicate_intent_publishes_once_and_a_new_request_observes_new_refs() {
        let repo = Repository::new();
        let repo_path = repo.path().to_path_buf();
        let mut pages = BranchPages::new(repo);
        let page = pages.page();
        pages.request(page, &repo_path, 1);
        pages.request(page, &repo_path, 1);
        pages.wait(page, 1);
        pages.1.run(&["branch", "created-later"]);
        pages.request(page, &repo_path, 2);
        let output = pages.wait(page, 2);
        assert!(
            output
                .branches
                .iter()
                .any(|branch| branch.branch == "created-later")
        );
        assert_eq!(
            pages
                .0
                .world()
                .resource::<Received>()
                .0
                .iter()
                .filter(|(_, result)| result.request_id == 1)
                .count(),
            1
        );
    }

    #[test]
    fn closing_one_shared_reader_preserves_the_other_destination() {
        let repo = Repository::new();
        let repo_path = repo.path().to_path_buf();
        let mut pages = BranchPages::new(repo);
        let blocked = pages.blocked();
        let first = pages.page();
        let second = pages.page();
        pages.request(first, &repo_path, 1);
        pages.request(second, &repo_path, 1);
        pages.0.update();
        pages.0.world_mut().despawn(first);
        pages.0.update();
        drop(blocked);
        let output = pages.wait(second, 1);
        assert!(
            output
                .branches
                .iter()
                .any(|branch| branch.branch == "topic")
        );
        assert!(
            !pages
                .0
                .world()
                .resource::<Received>()
                .0
                .iter()
                .any(|(owner, _)| *owner == first)
        );
    }

    #[test]
    fn the_last_closed_owner_retains_native_work_until_drain() {
        let repo = Repository::new();
        let repo_path = repo.path().to_path_buf();
        let mut pages = BranchPages::new(repo);
        let blocked = pages.blocked();
        let page = pages.page();
        pages.request(page, &repo_path, 1);
        pages.0.update();
        let read = pages
            .0
            .world_mut()
            .query_filtered::<Entity, With<BranchRead>>()
            .single(pages.0.world())
            .unwrap();
        let operation = pages
            .0
            .world_mut()
            .query_filtered::<Entity, With<BranchRequest>>()
            .single(pages.0.world())
            .unwrap();
        pages.0.world_mut().despawn(page);
        pages.0.update();
        pages.0.update();
        assert!(pages.0.world().get_entity(read).is_ok());
        assert!(pages.0.world().get_entity(operation).is_ok());
        drop(blocked);
        let deadline = Instant::now() + Duration::from_secs(5);
        while pages.0.world().get_entity(read).is_ok() {
            pages.0.update();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(pages.0.world().resource::<Received>().0.is_empty());
        assert_eq!(
            pages.0.world().get::<OperationState>(operation),
            Some(&OperationState::Canceled)
        );
    }

    #[cfg(unix)]
    #[test]
    fn retargeting_an_accepted_alias_refuses_foreign_ref_publication() {
        let first = Repository::new();
        let second = Repository::new();
        second.run(&["branch", "foreign"]);
        let temporary = tempfile::tempdir().unwrap();
        let alias = temporary.path().join("project");
        std::os::unix::fs::symlink(first.path(), &alias).unwrap();
        let mut pages = BranchPages::new(first);
        let page = pages.page();
        pages.request(page, &alias, 1);
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(second.path(), &alias).unwrap();
        assert!(pages.wait(page, 1).branches.is_empty());
    }

    #[test]
    fn a_replaced_page_does_not_receive_an_old_branch_response() {
        let repo = Repository::new();
        let repo_path = repo.path().to_path_buf();
        let mut pages = BranchPages::new(repo);
        let blocked = pages.blocked();
        let page = pages
            .0
            .world_mut()
            .spawn(PageMetadata {
                url: "vmux://start/".into(),
                ..Default::default()
            })
            .id();
        pages.request(page, &repo_path, 1);
        pages.0.update();
        pages
            .0
            .world_mut()
            .get_mut::<PageMetadata>(page)
            .unwrap()
            .url = "vmux://knowledge/".into();
        pages.0.update();
        drop(blocked);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            pages.0.update();
            if pages
                .0
                .world_mut()
                .query_filtered::<Entity, With<BranchRead>>()
                .iter(pages.0.world())
                .next()
                .is_none()
            {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(pages.0.world().resource::<Received>().0.is_empty());
    }
}
