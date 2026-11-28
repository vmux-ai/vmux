use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;

use bevy_app::{App, PreStartup, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::JobWake;
use vmux_ecs::cli::{CliInvocation, CliResult, CliWake};
use vmux_ecs::{Cwd, Operation, OperationState, Order};
use vmux_job::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet};

use crate::Completed;
use crate::debug_io::{Flight, Task};
use crate::repository::Checkout;
use crate::repository_io::Read;
use crate::signals::Signals;

pub struct RepositorySelectionPlugin;

impl bevy_app::Plugin for RepositorySelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<Task<Read>>::default())
            .add_systems(PreStartup, ensure_job_wake)
            .configure_sets(Update, Stage::Resolve.in_set(JobSet::Apply))
            .add_systems(
                Update,
                (
                    request,
                    seed,
                    locate,
                    inspect,
                    cancel,
                    collect,
                    collect_worktrees,
                    collect_common,
                    collect_root,
                    schedule,
                    normalize,
                    discover,
                    settle,
                    choose,
                    finish,
                )
                    .chain()
                    .in_set(Stage::Resolve),
            );
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stage {
    Resolve,
}

fn ensure_job_wake(
    wake: Option<Res<JobWake>>,
    cli_wake: Option<Res<CliWake>>,
    mut commands: Commands,
) {
    if wake.is_none() {
        let target = cli_wake.map_or_else(CliWake::current, |wake| wake.clone());
        commands.insert_resource(JobWake(std::sync::Arc::new(target).into()));
    }
}

#[derive(Component)]
pub struct Input {
    pub path: PathBuf,
    pub branch: Option<OsString>,
}

#[derive(Component)]
struct Seeded;

#[derive(Component)]
struct Directory {
    client: bool,
    cloud: bool,
}

#[derive(Component)]
struct Location(Entity);

#[derive(Component)]
struct Inspected;

#[derive(Component)]
struct Worktree {
    branch: Option<OsString>,
    cloud: bool,
}

#[derive(Component)]
struct Choice(Entity);

type Incoming<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static CliInvocation),
    (Without<Input>, Without<Checkout>, Without<CliResult>),
>;

fn request(requests: Incoming, mut commands: Commands) {
    for (entity, invocation) in &requests {
        if invocation.command.starts_with("dx.ci.") {
            continue;
        }
        if !invocation.command.starts_with("dx.")
            || invocation.is("dx.help")
            || invocation.is("dx.update")
            || invocation.is("dx.tui")
        {
            continue;
        }
        let path = match invocation.value_os("root") {
            Some(path) => PathBuf::from(path),
            None => match std::env::current_dir() {
                Ok(path) => path,
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(CliResult(Err(error.to_string())));
                    continue;
                }
            },
        };
        commands.entity(entity).insert(Input {
            path,
            branch: invocation.value_os("branch").map(OsString::from),
        });
    }
}

type Seeding<'w, 's> =
    Query<'w, 's, (Entity, &'static Input), (Without<Seeded>, Without<CliResult>)>;

fn seed(requests: Seeding, mut commands: Commands) {
    for (entity, input) in &requests {
        let mut path = match input.path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
                continue;
            }
        };
        let mut order = 0;
        loop {
            commands.spawn((
                Cwd(path.clone()),
                Directory {
                    client: path.join("crates/app/vmux_cli/Cargo.toml").is_file(),
                    cloud: path.join("Cargo.toml").is_file()
                        && (path.join("dx/feature.ron").is_file()
                            || path.join("docker-compose.yml").is_file()
                            || path.join(".gitmodules").is_file()),
                },
                Order(order),
                ChildOf(entity),
                Completed,
            ));
            if !path.pop() {
                break;
            }
            order += 1;
        }
        commands.entity(entity).insert(Seeded);
    }
}

type Unlocated<'w, 's> =
    Query<'w, 's, Entity, (With<Seeded>, Without<Location>, Without<CliResult>)>;

fn locate(
    requests: Unlocated,
    directories: Query<(Entity, &Directory, &Order, &ChildOf)>,
    mut commands: Commands,
) {
    for request in &requests {
        let mut nearest = None;
        for (entity, directory, order, parent) in &directories {
            if parent.parent() != request || !directory.client && !directory.cloud {
                continue;
            }
            if nearest.is_none_or(|(_, previous)| order.0 < previous) {
                nearest = Some((entity, order.0));
            }
        }
        match nearest {
            Some((entity, _)) => {
                commands.entity(request).insert(Location(entity));
            }
            None => {
                commands.entity(request).insert(CliResult(Err(
                    "Not inside a Vmux repository; use --root PATH".into(),
                )));
            }
        }
    }
}

#[derive(Component)]
struct Scanned;

#[derive(Component)]
struct Scan {
    cloud: bool,
    nested: bool,
    owner: Entity,
}

#[derive(Component)]
struct Listed;

#[derive(Component)]
struct Common(PathBuf);

#[derive(Component)]
struct RootSetting(Option<PathBuf>);

#[derive(Component)]
struct Normalized;

#[derive(Component)]
struct Origin(Entity);

#[derive(Component)]
struct WorktreesRead;

#[derive(Component)]
struct CommonRead;

#[derive(Component)]
struct RootRead;

#[derive(Component)]
struct Received(String);

#[derive(Component)]
struct Missing;

type Inspecting<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Input, &'static Location),
    (Without<Scanned>, Without<CliResult>),
>;

fn inspect(requests: Inspecting, directories: Query<(&Cwd, &Directory)>, mut commands: Commands) {
    for (entity, input, location) in &requests {
        if input.branch.is_none() {
            commands.entity(entity).insert((Scanned, Inspected));
            continue;
        }
        let (cwd, directory) = directories.get(location.0).unwrap();
        commands.spawn((
            Operation::bundle("dx.repository.scan").unwrap().running(),
            Cwd(cwd.0.clone()),
            Scan {
                cloud: directory.cloud,
                nested: false,
                owner: entity,
            },
            ChildOf(entity),
        ));
        commands.entity(entity).insert(Scanned);
    }
}

type CancelingReads<'w, 's> =
    Query<'w, 's, (&'static ChildOf, &'static mut OperationState), (With<Read>, Without<Scan>)>;

type Unresolved<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<Input>,
        Without<Checkout>,
        Without<crate::Accepted>,
        Without<CliResult>,
    ),
>;

fn cancel(
    requests: Unresolved,
    mut scans: Query<(Entity, &Scan, &mut OperationState), Without<Read>>,
    mut reads: CancelingReads,
    flights: Query<&Flight>,
    mut commands: Commands,
) {
    let Some(signal) = Signals::pending() else {
        return;
    };
    for entity in &requests {
        if !Signals::active() && !flights.iter().any(Flight::pending) {
            commands
                .entity(entity)
                .insert(CliResult(Ok((128 + signal) as u8)));
        }
        for (scan_entity, scan, mut state) in &mut scans {
            if scan.owner != entity {
                continue;
            }
            *state = OperationState::Canceled;
            for (parent, mut state) in &mut reads {
                if parent.parent() == scan_entity {
                    *state = OperationState::Canceled;
                }
            }
        }
    }
}

type Scheduling<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Scan,
        &'static Cwd,
        &'static ChildOf,
        Option<&'static Listed>,
        Option<&'static Common>,
    ),
    (Without<RootSetting>, Without<Completed>),
>;

fn schedule(
    scans: Scheduling,
    requests: Query<Entity, (With<Input>, Without<CliResult>)>,
    reads: Query<&OperationState, With<Read>>,
    running: Query<Entity, With<crate::Running>>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    if Signals::pending().is_some()
        || !running.is_empty()
        || reads
            .iter()
            .any(|state| matches!(state, OperationState::Pending | OperationState::Running))
    {
        return;
    }
    for (entity, scan, cwd, parent, listed, common) in &scans {
        if parent.parent() != scan.owner || !requests.contains(scan.owner) {
            continue;
        }
        let arguments = if listed.is_none() {
            vec!["worktree", "list", "--porcelain", "-z"]
        } else if common.is_none() {
            vec!["rev-parse", "--path-format=absolute", "--git-common-dir"]
        } else {
            vec!["config", "--local", "--get", "core.worktree"]
        };
        let read = Read {
            root: cwd.0.clone(),
            arguments,
        };
        let (task, flight) = Task::new(read.clone(), None, Vec::new(), wake.0.clone());
        let mut child = commands.spawn((
            Operation::bundle("dx.repository.read").unwrap(),
            JobRequest(task),
            flight,
            read,
            ChildOf(entity),
        ));
        if listed.is_none() {
            child.insert(WorktreesRead);
        } else if common.is_none() {
            child.insert(CommonRead);
        } else {
            child.insert(RootRead);
        }
        wake.0.wake_by_ref();
        break;
    }
}

type Receipts<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Has<RootRead>,
        &'static JobResult<Task<Read>>,
        &'static JobOwner<Task<Read>>,
        &'static ChildOf,
    ),
    Without<Completed>,
>;

fn collect(
    receipts: Receipts,
    scans: Query<(&Scan, &ChildOf)>,
    requests: Query<Entity, (With<Input>, Without<CliResult>)>,
    mut commands: Commands,
) {
    for (entity, root, result, owner, parent) in &receipts {
        if Signals::pending().is_some() {
            commands
                .entity(entity)
                .insert((Completed, OperationState::Canceled));
            continue;
        }
        let Ok((scan, request)) = scans.get(parent.parent()) else {
            continue;
        };
        if !owner.accepts(Some(parent))
            || request.parent() != scan.owner
            || !requests.contains(scan.owner)
        {
            commands
                .entity(entity)
                .insert((Completed, OperationState::Canceled));
            continue;
        }
        let report = match &result.0 {
            Ok(report) => report,
            Err(error) => {
                commands
                    .entity(scan.owner)
                    .insert(CliResult(Err(error.clone())));
                commands
                    .entity(entity)
                    .insert((Completed, OperationState::Failed));
                continue;
            }
        };
        let output = match &report.result {
            Ok(output) => output,
            Err(error) => {
                commands
                    .entity(scan.owner)
                    .insert(CliResult(Err(error.clone())));
                commands
                    .entity(entity)
                    .insert((Completed, OperationState::Failed));
                continue;
            }
        };
        let missing = root && output.status.code() == Some(1);
        if !(output.status.success() || missing) {
            commands.entity(scan.owner).insert(CliResult(Err(format!(
                "Git repository read failed: {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ))));
            commands
                .entity(entity)
                .insert((Completed, OperationState::Failed));
            continue;
        }
        let text = match std::str::from_utf8(&output.stdout) {
            Ok(text) => text,
            Err(error) => {
                commands
                    .entity(scan.owner)
                    .insert(CliResult(Err(error.to_string())));
                commands
                    .entity(entity)
                    .insert((Completed, OperationState::Failed));
                continue;
            }
        };
        let mut receipt = commands.entity(entity);
        receipt
            .insert(Received(text.to_owned()))
            .remove::<JobResult<Task<Read>>>();
        if missing {
            receipt.insert(Missing);
        }
    }
}

type WorktreeReceipts<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Received, &'static ChildOf),
    (With<WorktreesRead>, Without<Completed>),
>;
type CommonReceipts<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Received, &'static ChildOf),
    (With<CommonRead>, Without<Completed>),
>;
type RootReceipts<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Received, &'static ChildOf, Has<Missing>),
    (With<RootRead>, Without<Completed>),
>;

fn collect_worktrees(receipts: WorktreeReceipts, scans: Query<&Scan>, mut commands: Commands) {
    for (entity, received, parent) in &receipts {
        let Ok(scan) = scans.get(parent.parent()) else {
            continue;
        };
        let mut path = None;
        let mut branch = None;
        for value in received.0.split('\0') {
            if let Some(value) = value.strip_prefix("worktree ") {
                path = Some(PathBuf::from(value));
            } else if let Some(value) = value.strip_prefix("branch refs/heads/") {
                branch = Some(OsString::from(value));
            } else if value.is_empty()
                && let Some(path) = path.take()
            {
                commands.spawn((
                    Cwd(path),
                    Worktree {
                        branch: branch.take(),
                        cloud: scan.cloud,
                    },
                    Origin(parent.parent()),
                    ChildOf(scan.owner),
                    Completed,
                ));
            }
        }
        commands.entity(parent.parent()).insert(Listed);
        commands
            .entity(entity)
            .insert((Completed, OperationState::Succeeded))
            .remove::<Received>();
    }
}

fn collect_common(receipts: CommonReceipts, mut commands: Commands) {
    for (entity, received, parent) in &receipts {
        commands
            .entity(parent.parent())
            .insert(Common(PathBuf::from(received.0.trim())));
        commands
            .entity(entity)
            .insert((Completed, OperationState::Succeeded))
            .remove::<Received>();
    }
}

fn collect_root(receipts: RootReceipts, mut commands: Commands) {
    for (entity, received, parent, missing) in &receipts {
        let root = if missing {
            None
        } else {
            Some(PathBuf::from(received.0.trim()))
        };
        commands.entity(parent.parent()).insert(RootSetting(root));
        commands
            .entity(entity)
            .insert((Completed, OperationState::Succeeded))
            .remove::<Received>();
    }
}

fn normalize(
    scans: Query<(Entity, &Scan, &Common, &RootSetting), Without<Normalized>>,
    mut worktrees: Query<(Entity, &mut Cwd, &Origin), With<Worktree>>,
    mut commands: Commands,
) {
    for (entity, scan, common, setting) in &scans {
        let primary = match &setting.0 {
            Some(path) => match common.0.join(path).canonicalize() {
                Ok(path) => Some(path),
                Err(error) => {
                    commands
                        .entity(scan.owner)
                        .insert(CliResult(Err(error.to_string())));
                    continue;
                }
            },
            None => None,
        };
        for (candidate, mut cwd, origin) in &mut worktrees {
            if origin.0 != entity {
                continue;
            }
            if let Some(primary) = &primary
                && cwd.0 == common.0
            {
                cwd.0 = primary.clone();
            }
            if scan.nested {
                if !cwd.0.join(".git").exists() {
                    commands.entity(candidate).despawn();
                    continue;
                }
                match cwd.0.canonicalize() {
                    Ok(path) => cwd.0 = path,
                    Err(error) => {
                        commands
                            .entity(scan.owner)
                            .insert(CliResult(Err(error.to_string())));
                        break;
                    }
                }
            }
        }
        commands.entity(entity).insert(Normalized);
    }
}

type Discovering<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Scan, &'static ChildOf),
    (With<Normalized>, Without<Completed>),
>;

fn discover(
    scans: Discovering,
    known: Query<(&Cwd, &Scan)>,
    worktrees: Query<(&Cwd, &Origin), With<Worktree>>,
    requests: Query<Entity, (With<Input>, Without<CliResult>)>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, scan, parent) in &scans {
        if parent.parent() != scan.owner || !requests.contains(scan.owner) {
            commands
                .entity(entity)
                .insert((Completed, OperationState::Canceled));
            continue;
        }
        let mut paths = BTreeSet::new();
        for (cwd, previous) in &known {
            if previous.owner == scan.owner {
                paths.insert(cwd.0.clone());
            }
        }
        if scan.cloud {
            for (cwd, origin) in &worktrees {
                if origin.0 != entity {
                    continue;
                }
                let mut candidates = vec![cwd.0.join("client")];
                if let Some(parent) = cwd.0.parent() {
                    candidates.push(parent.join("vmux"));
                }
                for client in candidates {
                    if !client.join("crates/app/vmux_cli/Cargo.toml").is_file()
                        || !client.join(".git").exists()
                        || !paths.insert(client.clone())
                    {
                        continue;
                    }
                    commands.spawn((
                        Operation::bundle("dx.repository.scan").unwrap().running(),
                        Cwd(client),
                        Scan {
                            cloud: false,
                            nested: true,
                            owner: scan.owner,
                        },
                        ChildOf(scan.owner),
                    ));
                    wake.0.wake_by_ref();
                }
            }
        }
        commands
            .entity(entity)
            .insert((Completed, OperationState::Succeeded));
    }
}

type Settling<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Children),
    (With<Scanned>, Without<Inspected>, Without<CliResult>),
>;

fn settle(
    requests: Settling,
    completed: Query<Entity, (With<Scan>, With<Completed>)>,
    scans: Query<Entity, With<Scan>>,
    worktrees: Query<(Entity, &Cwd, &Worktree, &ChildOf)>,
    mut commands: Commands,
) {
    for (request, children) in &requests {
        if children
            .iter()
            .any(|child| scans.contains(child) && !completed.contains(child))
        {
            continue;
        }
        let mut paths = BTreeSet::new();
        for (entity, cwd, worktree, parent) in &worktrees {
            if parent.parent() == request && !worktree.cloud && !paths.insert(cwd.0.clone()) {
                commands.entity(entity).despawn();
            }
        }
        commands.entity(request).insert(Inspected);
    }
}

type Choosing<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Input, &'static Location),
    (With<Inspected>, Without<Choice>, Without<CliResult>),
>;

fn choose(
    requests: Choosing,
    worktrees: Query<(Entity, &Cwd, &Worktree, &ChildOf)>,
    mut commands: Commands,
) {
    if Signals::pending().is_some() {
        return;
    }
    for (request, input, location) in &requests {
        let Some(name) = &input.branch else {
            commands.entity(request).insert(Choice(location.0));
            continue;
        };
        let mut best = None;
        let mut ambiguous = false;
        for (entity, folder, worktree, parent) in &worktrees {
            if parent.parent() != request {
                continue;
            }
            let rank = if worktree.branch.as_deref() == Some(name.as_os_str()) {
                if worktree.cloud { 0 } else { 1 }
            } else if folder.0.file_name() == Some(name.as_os_str()) {
                if worktree.cloud { 2 } else { 3 }
            } else {
                continue;
            };
            match best {
                None => {
                    best = Some((entity, rank));
                    ambiguous = false;
                }
                Some((_, previous)) if rank < previous => {
                    best = Some((entity, rank));
                    ambiguous = false;
                }
                Some((_, previous)) if rank == previous => ambiguous = true,
                _ => {}
            }
        }
        if ambiguous {
            let error = if best.is_some_and(|(_, rank)| rank >= 2) {
                "Worktree alias is ambiguous; use the full branch name"
            } else {
                "Branch matches multiple worktrees"
            };
            commands
                .entity(request)
                .insert(CliResult(Err(error.into())));
        } else if let Some((entity, _)) = best {
            commands.entity(request).insert(Choice(entity));
        } else {
            commands.entity(request).insert(CliResult(Err(
                "Branch has no worktree; create its worktree explicitly first".into(),
            )));
        }
    }
}

type Finishing<'w, 's> =
    Query<'w, 's, (Entity, &'static Choice), (Without<Checkout>, Without<CliResult>)>;

fn finish(requests: Finishing, folders: Query<&Cwd>, mut commands: Commands) {
    if Signals::pending().is_some() {
        return;
    }
    for (entity, choice) in &requests {
        let mut root = folders.get(choice.0).unwrap().0.clone();
        if !root.join("crates/app/vmux_cli/Cargo.toml").is_file() {
            let nested = root.join("client");
            if nested.join("crates/app/vmux_cli/Cargo.toml").is_file() {
                root = nested;
            } else if let Some(sibling) = root.parent().map(|parent| parent.join("vmux"))
                && sibling.join("crates/app/vmux_cli/Cargo.toml").is_file()
            {
                root = sibling;
            }
        }
        if !root.join("crates/app/vmux_cli/Cargo.toml").is_file() {
            commands.entity(entity).insert(CliResult(Err(
                "Selected checkout has no initialized Vmux client".into(),
            )));
            continue;
        }
        let root = match root.canonicalize() {
            Ok(root) => root,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
                continue;
            }
        };
        let mut checkout = Checkout {
            target: root.join("target"),
            root,
            build_target: None,
        };
        match checkout.configure(
            std::env::var_os("CARGO_TARGET_DIR").as_deref(),
            std::env::var_os("CARGO_BUILD_TARGET").as_deref(),
        ) {
            Ok(()) => {
                commands.entity(entity).insert(checkout);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
            }
        }
    }
}
