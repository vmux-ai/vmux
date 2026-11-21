use std::path::{Path, PathBuf};
use std::time::Instant;

use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::{CommandBarUiState, StartBranchesRequest, StartProjectBranches};
use vmux_api::space::ProjectBranch;
use vmux_ecs::{Operation, OperationRetained, OperationState, OperationTarget, PageMetadata};
use vmux_git::worktree::{CheckoutInfo, WorktreeRegistration};
use vmux_git::{GitRuntime, GitTask, RepositorySource};

pub(crate) struct BranchesPlugin;

impl Plugin for BranchesPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.init_resource::<GitRuntime>()
            .add_plugins(UiEventPlugin::<(StartBranchesRequest,)>::default())
            .add_observer(request)
            .add_systems(
                Update,
                (
                    retire,
                    poll::<DiscoveryOutput>,
                    poll::<RefsOutput>,
                    poll::<RegistrationsOutput>,
                    discover,
                    materialize_refs,
                    materialize_registrations,
                    publish,
                    cleanup,
                    start_refs,
                    start_registrations,
                    start,
                )
                    .chain(),
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

#[derive(Component)]
struct NativeTask<T: Component>(GitTask<T>);

#[derive(Component)]
struct DiscoveryOutput(Result<(CheckoutInfo, RepositorySource), String>);

#[derive(Component)]
struct Discovered {
    checkout: CheckoutInfo,
    source: RepositorySource,
}

#[derive(Component)]
struct RefsOutput(Result<Vec<String>, String>);

#[derive(Component)]
struct RegistrationsOutput(Result<Vec<WorktreeRegistration>, String>);

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
    let project = input.project.trim().to_owned();
    if project.is_empty() || input.request_id == 0 {
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
    reads: Query<(Entity, &BranchRead, Option<&Discovered>), Without<ReadRetired>>,
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
    for (entity, read, discovered) in &reads {
        if !read.source.matches(read.source.path())
            || discovered
                .is_some_and(|discovered| !discovered.source.matches(discovered.source.path()))
        {
            commands.entity(entity).insert((
                ReadRetired,
                LookupFailure("Git repository source changed".into()),
            ));
        } else if !subscribers
            .iter()
            .any(|(subscriber, retired, refused)| !retired && !refused && subscriber.0 == entity)
        {
            commands.entity(entity).insert(ReadRetired);
        }
    }
}

fn poll<T: Component>(
    mut tasks: Query<(Entity, &mut NativeTask<T>, Has<ReadRetired>)>,
    mut commands: Commands,
) {
    for (entity, mut task, retired) in &mut tasks {
        if retired {
            task.0.cancel();
        }
        if !task.0.is_finished() {
            continue;
        }
        let mut entity = commands.entity(entity);
        entity.remove::<NativeTask<T>>();
        if retired {
            continue;
        }
        match task.0.poll() {
            Some(Ok(output)) => {
                entity.insert(output);
            }
            _ => {
                entity.insert((
                    ReadRetired,
                    LookupFailure("Start branch lookup worker failed".into()),
                ));
            }
        }
    }
}

fn discover(outputs: Query<(Entity, &BranchRead, &DiscoveryOutput)>, mut commands: Commands) {
    for (entity, read, output) in &outputs {
        commands.entity(entity).remove::<DiscoveryOutput>();
        match &output.0 {
            Ok((checkout, source))
                if read.source.matches(read.source.path()) && source.matches(source.path()) =>
            {
                commands.entity(entity).insert(Discovered {
                    checkout: checkout.clone(),
                    source: source.clone(),
                });
            }
            Ok(_) => {
                commands.entity(entity).insert((
                    ReadRetired,
                    LookupFailure("Git repository source changed".into()),
                ));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ReadRetired, LookupFailure(error.clone())));
            }
        }
    }
}

fn materialize_refs(outputs: Query<(Entity, &RefsOutput)>, mut commands: Commands) {
    for (read, output) in &outputs {
        commands.entity(read).remove::<RefsOutput>();
        match &output.0 {
            Ok(branches) => {
                for (index, branch) in branches.iter().enumerate() {
                    commands.spawn((
                        Name::new(branch.clone()),
                        vmux_ecs::Order(index as u32),
                        Observation(read),
                        BranchRef,
                    ));
                }
                commands.entity(read).insert(RefsComplete);
            }
            Err(error) => {
                commands
                    .entity(read)
                    .insert((ReadRetired, LookupFailure(error.clone())));
            }
        }
    }
}

fn materialize_registrations(
    outputs: Query<(Entity, &RegistrationsOutput)>,
    mut commands: Commands,
) {
    for (read, output) in &outputs {
        commands.entity(read).remove::<RegistrationsOutput>();
        match &output.0 {
            Ok(registrations) => {
                for (index, registration) in registrations.iter().enumerate() {
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
            Err(error) => {
                commands
                    .entity(read)
                    .insert((ReadRetired, LookupFailure(error.clone())));
            }
        }
    }
}

type FinishedReads<'w, 's> = Query<
    'w,
    's,
    (Entity, Option<&'static LookupFailure>),
    (
        With<BranchRead>,
        Without<NativeTask<DiscoveryOutput>>,
        Without<NativeTask<RefsOutput>>,
        Without<NativeTask<RegistrationsOutput>>,
        Or<(
            With<ReadRetired>,
            (With<RefsComplete>, With<RegistrationsComplete>),
        )>,
    ),
>;

fn publish(
    reads: FinishedReads,
    requests: Query<(Entity, &BranchRequest, &BranchSubscriber), Without<RequestRetired>>,
    current: Query<(&CurrentRequest, Option<&PageMetadata>)>,
    refs: Query<(&Observation, &Name, &vmux_ecs::Order), With<BranchRef>>,
    registrations: Query<(&Observation, &Registration, &vmux_ecs::Order)>,
    mut commands: Commands,
) {
    for (read, failure) in &reads {
        let mut ordered = Vec::new();
        for (owner, name, order) in &refs {
            if owner.0 == read {
                ordered.push((order.0, name.as_str()));
            }
        }
        ordered.sort_by_key(|(order, _)| *order);
        let mut branches = Vec::new();
        if failure.is_none() {
            for (_, branch) in ordered {
                let mut selected = None;
                for (owner, registration, order) in &registrations {
                    if owner.0 == read
                        && !registration.prunable
                        && registration.directory
                        && registration.branch.as_deref() == Some(branch)
                        && selected.is_none_or(|(earlier, _)| order.0 < earlier)
                    {
                        selected = Some((order.0, registration));
                    }
                }
                let checkout = selected.map(|(_, registration)| &registration.path);
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
        for (entity, request, subscriber) in &requests {
            if subscriber.0 != read {
                continue;
            }
            if current
                .get(request.owner)
                .is_ok_and(|(current, page)| request.current(current, page))
            {
                let result = if request.source.matches(Path::new(&request.project)) {
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
            let state = if failure.is_some() || !request.source.matches(Path::new(&request.project))
            {
                OperationState::Failed
            } else {
                OperationState::Succeeded
            };
            commands
                .entity(entity)
                .remove::<(
                    BranchRequest,
                    BranchSubscriber,
                    RequestRefused,
                    OperationRetained,
                )>()
                .insert(state);
        }
    }
}

fn cleanup(
    requests: Query<(Entity, Option<&BranchSubscriber>), With<RequestRetired>>,
    reads: Query<
        (
            Entity,
            Has<NativeTask<DiscoveryOutput>>,
            Has<NativeTask<RefsOutput>>,
            Has<NativeTask<RegistrationsOutput>>,
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

fn start_refs(
    reads: Query<
        (Entity, &Discovered),
        (
            Without<ReadRetired>,
            Without<RefsComplete>,
            Without<RefsOutput>,
            Without<NativeTask<RefsOutput>>,
        ),
    >,
    runtime: Res<GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, discovered) in &reads {
        let checkout = discovered.checkout.clone();
        let Some(task) = runtime
            .spawn_background(wake.as_deref().map(|wake| (**wake).clone()), move || {
                RefsOutput(checkout.local_branches().map_err(|error| error.0))
            })
        else {
            continue;
        };
        commands.entity(entity).insert(NativeTask(task));
    }
}

fn start_registrations(
    reads: Query<
        (Entity, &Discovered),
        (
            Without<ReadRetired>,
            Without<RegistrationsComplete>,
            Without<RegistrationsOutput>,
            Without<NativeTask<RegistrationsOutput>>,
        ),
    >,
    runtime: Res<GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, discovered) in &reads {
        let checkout = discovered.checkout.clone();
        let Some(task) = runtime
            .spawn_background(wake.as_deref().map(|wake| (**wake).clone()), move || {
                RegistrationsOutput(checkout.worktree_registrations().map_err(|error| error.0))
            })
        else {
            continue;
        };
        commands.entity(entity).insert(NativeTask(task));
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
    wake: Option<Res<EventLoopProxyWrapper>>,
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
        let Some(task) =
            runtime.spawn_background(wake.as_deref().map(|wake| (**wake).clone()), move || {
                DiscoveryOutput(
                    CheckoutInfo::try_from(path.as_path())
                        .map(|checkout| {
                            let observed = RepositorySource::capture(&checkout.root);
                            (checkout, observed)
                        })
                        .map_err(|error| error.0),
                )
            })
        else {
            continue;
        };
        let read = commands
            .spawn((
                Name::new("start.branch.read"),
                BranchRead {
                    source: source.clone(),
                    started,
                },
                NativeTask(task),
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
            let repo = Self(tempfile::tempdir().unwrap());
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
