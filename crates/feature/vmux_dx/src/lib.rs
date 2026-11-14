use std::ffi::OsString;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::cli::{CliInvocation, CliResult};
use vmux_ecs::manifest::{FeatureManifestSource, FeaturePlugin};
use vmux_ecs::{CreatedAt, Cwd, Operation, OperationState};

use environment::Environment;
use execution::Failure;
use repository::Checkout;
use storage::Storage;
use target_lock::TargetLock;
use work::{Outcome, Work};

#[cfg(feature = "ci-syntax")]
mod ci_native;
mod ci_scope;
mod consumer;
mod environment;
mod execution;
mod local;
mod native;
mod policy;
mod processes;
#[path = "../../../build_product.rs"]
#[allow(dead_code)]
mod product;
mod profile;
pub mod repository;
mod repository_io;
mod repository_selection;
mod signals;
mod source;
mod storage;
mod target_lock;
mod temporary;
mod tooling;
mod update_io;
pub mod updates;
mod work;
mod workspace;
mod workspace_io;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct Plugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum Stage {
    Initialize,
    Execute,
}

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FeaturePlugin::<Feature>::default(),
            updates::Plugin,
            workspace::Plugin,
            repository_selection::Plugin,
        ))
        .configure_sets(
            Update,
            (
                Stage::Initialize,
                repository_selection::Stage::Resolve,
                Stage::Execute,
            )
                .chain(),
        )
        .add_systems(Update, initialize.in_set(Stage::Initialize))
        .insert_resource(Mailbox::new())
        .add_message::<Completion>()
        .add_systems(
            Update,
            (
                accept,
                plan,
                receive,
                cancel,
                collect,
                select_local,
                start,
                finish,
            )
                .chain()
                .in_set(Stage::Execute),
        );
    }
}

impl Plugin {
    pub fn selected(arguments: impl Iterator<Item = OsString>) -> bool {
        workspace::Host::selected(arguments.collect())
    }
}

struct Feature;

impl FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[derive(Component)]
struct Accepted;

#[derive(Component)]
struct Settings(Environment);

#[derive(Component)]
struct Lease(Arc<TargetLock>);

#[derive(Component)]
struct Predecessor(Entity);

#[derive(Component)]
struct Running;

#[derive(Component)]
struct Completed;

#[derive(Component)]
pub struct Artifact(pub std::path::PathBuf);

#[derive(Component)]
struct Attempt(u64);

#[derive(Resource)]
struct Mailbox {
    sender: mpsc::SyncSender<Completion>,
    receiver: Mutex<mpsc::Receiver<Completion>>,
    wake: thread::Thread,
}

impl Mailbox {
    fn new() -> Self {
        let (sender, receiver) = mpsc::sync_channel(1);
        Self {
            sender,
            receiver: Mutex::new(receiver),
            wake: thread::current(),
        }
    }
}

#[derive(Message)]
struct Completion {
    entity: Entity,
    attempt: u64,
    result: std::result::Result<Outcome, Failure>,
}

type Incoming<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        Option<&'static workspace::Dispatch>,
        Option<&'static Checkout>,
    ),
    (Without<Accepted>, Without<CliResult>),
>;
type Pending<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Work,
        &'static ChildOf,
        Option<&'static Predecessor>,
        &'static Attempt,
    ),
    (Without<Running>, Without<Completed>),
>;
type Inputs<'w, 's> = Query<
    'w,
    's,
    (
        &'static Checkout,
        &'static Settings,
        &'static Storage,
        Option<&'static Lease>,
        Option<&'static workspace::Executable>,
        Option<&'static local::Target>,
        &'static OperationState,
    ),
    Without<CliResult>,
>;
type Cancellations<'w, 's> =
    Query<'w, 's, (Entity, &'static mut OperationState), (With<Accepted>, Without<CliResult>)>;
type Finishing<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Children, &'static OperationState),
    (With<Accepted>, Without<CliResult>),
>;

fn initialize(requests: Incoming, signals: Option<Res<signals::Signals>>, mut commands: Commands) {
    if signals.is_some() {
        return;
    }
    for (entity, invocation, dispatch, _) in &requests {
        if invocation.is("dx.help")
            || invocation.is("dx.update")
            || !invocation.command.starts_with("dx.") && dispatch.is_none()
        {
            continue;
        }
        match signals::Signals::install() {
            Ok(scope) => {
                commands.insert_resource(scope);
                return;
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
            }
        }
    }
}

fn accept(requests: Incoming, providers: Query<&workspace::Provider>, mut commands: Commands) {
    for (entity, invocation, dispatch, resolved_context) in &requests {
        if invocation.is("dx.update") {
            continue;
        }
        let provider = dispatch.and_then(|dispatch| providers.get(dispatch.provider).ok());
        if !invocation.command.starts_with("dx.") && provider.is_none() {
            continue;
        }
        if invocation.is("dx.help") {
            println!(
                "Vmux development commands: build, run, stop, test, lint, doctor, cache\nUse vmux <command> --help."
            );
            commands.entity(entity).insert(CliResult::default());
            continue;
        }
        let resolved = match provider {
            Some(provider) => {
                let target = provider
                    .root
                    .join(".build-tools/providers")
                    .join(&provider.package);
                workspace_io::Cache(target)
                    .prepare(provider)
                    .map(|target| Checkout {
                        root: provider.root.clone(),
                        target,
                        build_target: Some(env!("VMUX_DX_HOST_TARGET").into()),
                    })
            }
            None => match resolved_context {
                Some(checkout) => Ok(checkout.clone()),
                None => continue,
            },
        };
        let mut checkout = match resolved {
            Ok(checkout) => checkout,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
                continue;
            }
        };
        let mut environment = Environment::current();
        if provider.is_none()
            && let Err(error) = environment.load(&checkout.root)
        {
            commands
                .entity(entity)
                .insert(CliResult(Err(error.to_string())));
            continue;
        }
        if (invocation.is("dx.build") || invocation.is("dx.run"))
            && let Err(error) = environment.product(&checkout.root)
        {
            commands
                .entity(entity)
                .insert(CliResult(Err(error.to_string())));
            continue;
        }
        if invocation.is("dx.internal") && invocation.value("task") == Some("cli-release") {
            environment.remove("CARGO_BUILD_TARGET");
            environment.set(
                "CARGO_TARGET_DIR",
                checkout.root.join(".build-tools/standalone").as_os_str(),
            );
        }
        if invocation.value("platform") == Some("web") && !environment.present("CARGO_TARGET_DIR") {
            environment.set(
                "CARGO_TARGET_DIR",
                checkout.root.join("website/target").as_os_str(),
            );
        }
        if provider.is_none()
            && let Err(error) = checkout.configure(
                environment.get("CARGO_TARGET_DIR"),
                environment.get("CARGO_BUILD_TARGET"),
            )
        {
            commands
                .entity(entity)
                .insert(CliResult(Err(error.to_string())));
            continue;
        }
        if provider.is_none()
            && let Err(error) = tooling::Tooling::ensure(&checkout, &environment)
        {
            commands
                .entity(entity)
                .insert(CliResult(Err(error.to_string())));
            continue;
        }
        if provider.is_none() {
            environment.set("CARGO_TARGET_DIR", checkout.target.as_os_str());
        }
        if let Some(profile) = invocation.value("profile") {
            environment.set("VMUX_BUILD_PROFILE", profile);
        } else if !environment.present("VMUX_BUILD_PROFILE") {
            environment.set("VMUX_BUILD_PROFILE", "dev");
        }
        if let Some(platform) = invocation.value("platform") {
            environment.set("VMUX_DX_PLATFORM", platform);
        }
        if let Some(features) = invocation.value_os("features") {
            environment.set("VMUX_DESKTOP_FEATURE_SET", features.to_owned());
        }
        if let Some(profile) = invocation.value_os("user-profile") {
            environment.set("VMUX_PROFILE", profile);
        } else if !environment.present("VMUX_PROFILE") {
            environment.set("VMUX_PROFILE", "personal");
        }
        let storage = match policy::Policy::configured(&environment)
            .and_then(|policy| Storage::new(checkout.root.clone(), checkout.target.clone(), policy))
        {
            Ok(storage) => storage,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
                continue;
            }
        };
        if invocation.is("dx.run")
            && invocation.value("platform").unwrap_or("desktop") == "desktop"
            && invocation.value("profile") == Some("local")
        {
            let product = match environment.product(&checkout.root) {
                Ok(product) => product.value("VMUX_HOST_NAME").to_owned(),
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(CliResult(Err(error.to_string())));
                    continue;
                }
            };
            commands.entity(entity).insert(local::Selection { product });
        }
        commands.entity(entity).insert((
            Accepted,
            Operation::bundle(&invocation.command).unwrap().running(),
            Name::new(invocation.command.clone()),
            CreatedAt::now(),
            Cwd(checkout.root.clone()),
            checkout,
            storage,
            Settings(environment),
        ));
    }
}

#[derive(Component)]
struct Planned;

type Planning<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        Option<&'static workspace::Dispatch>,
    ),
    (With<Accepted>, Without<Planned>, Without<CliResult>),
>;

fn plan(operations: Planning, providers: Query<&workspace::Provider>, mut commands: Commands) {
    for (entity, invocation, dispatch) in &operations {
        let profile = invocation.value("profile").unwrap_or("dev");
        let platform = invocation.value("platform").unwrap_or("desktop");
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        let mut steps = Vec::new();
        if let Some(dispatch) = dispatch {
            let Ok(provider) = providers.get(dispatch.provider) else {
                continue;
            };
            steps.extend([
                Work::WorkspaceAcquire,
                Work::WorkspaceCompile(
                    provider.package.clone(),
                    provider.binary.clone(),
                    provider.cargo.clone(),
                ),
                Work::WorkspaceInvoke(dispatch.arguments.clone()),
            ]);
        } else {
            match invocation.command.as_str() {
                "dx.build" => {
                    steps.extend([Work::Check, Work::Acquire, Work::Seed, Work::Preflight]);
                    match platform {
                        "desktop" if profile == "dev" => {
                            steps.extend([
                                Work::Native,
                                Work::Tailwind,
                                Work::Codesign,
                                Work::Helper,
                                Work::ClearReady,
                            ]);
                            steps.push(Work::cargo(&["build", "-p", "vmux_cli"]));
                            steps.push(Work::cargo(&[
                                "build",
                                "-p",
                                "vmux_app",
                                "--bin",
                                "vmux_service",
                                "--features",
                                "service-daemon",
                            ]));
                            let mut args = if cfg!(target_os = "macos") {
                                vec!["rustc", "-p", "vmux_desktop", "--bin", "vmux_desktop"]
                            } else {
                                vec!["build", "-p", "vmux_desktop"]
                            }
                            .into_iter()
                            .map(OsString::from)
                            .collect::<Vec<_>>();
                            args.extend(["--no-default-features".into(), "--features".into()]);
                            args.push(
                                invocation
                                    .value_os("features")
                                    .unwrap_or(std::ffi::OsStr::new("dev,full"))
                                    .to_owned(),
                            );
                            args.append(&mut forwarded);
                            if cfg!(target_os = "macos") {
                                args.extend(
                                    ["--", "-C", "link-arg=-Wl,-headerpad,1000"]
                                        .map(OsString::from),
                                );
                            }
                            steps.push(Work::Cargo(args));
                            if cfg!(target_os = "macos") {
                                steps.extend([Work::SignDebug, Work::BundleDebug]);
                            }
                            steps.push(Work::PublishDebug);
                        }
                        "desktop" => {
                            steps.extend([
                                Work::Native,
                                Work::Tailwind,
                                Work::PackageTools,
                                Work::Codesign,
                                Work::Package(profile.into()),
                            ]);
                        }
                        "ios" if profile == "release" => {
                            steps.extend([Work::Dioxus("ios".into()), Work::ReleaseIos]);
                        }
                        "ios" | "android" => {
                            steps.push(Work::Dioxus(platform.into()));
                            let mut args = vec![
                                "build".into(),
                                format!("--{platform}").into(),
                                "-p".into(),
                                "vmux_mobile".into(),
                            ];
                            if profile != "dev" {
                                args.push("--release".into());
                            }
                            args.append(&mut forwarded);
                            steps.push(Work::Tool("dx".into(), args));
                            if platform == "ios" {
                                steps.extend([Work::ResourcesIos, Work::VerifyIos(false)]);
                            }
                        }
                        "web" => {
                            steps.extend([
                                Work::Dioxus(platform.into()),
                                Work::Binaryen,
                                Work::Tailwind,
                                Work::Css,
                            ]);
                            let mut args = vec!["build".into(), "--platform".into(), "web".into()];
                            if profile != "dev" {
                                args.extend(["--ssg".into(), "--release".into()]);
                            }
                            args.append(&mut forwarded);
                            steps.push(Work::Tool("dx".into(), args));
                            if profile != "dev" {
                                steps.push(Work::FlattenWeb);
                            }
                        }
                        _ => {
                            commands.entity(entity).insert((
                                OperationState::Failed,
                                CliResult(Err("Unsupported build platform".into())),
                            ));
                            continue;
                        }
                    }
                    steps.push(Work::Finish(profile.into()));
                }
                "dx.run" => {
                    steps.push(Work::Acquire);
                    if platform != "desktop" {
                        steps.push(Work::Dioxus(platform.into()));
                        if platform == "web" {
                            steps.extend([Work::Binaryen, Work::Tailwind, Work::Css]);
                        }
                        if platform == "ios" {
                            steps.push(Work::Simulator);
                        }
                    }
                    if platform == "desktop" && profile == "local" {
                        steps.extend([Work::LocalEntries, Work::LocalLaunch]);
                    } else {
                        steps.push(Work::Run(profile.into(), platform.into()));
                    }
                }
                "dx.stop" => steps.push(Work::Stop),
                "dx.reset-layout" => steps.push(Work::Reset),
                "dx.test" => {
                    steps.extend([Work::Acquire, Work::Seed]);
                    let mut args = vec![
                        "test".into(),
                        "--workspace".into(),
                        "--exclude".into(),
                        "bevy_cef_core".into(),
                    ];
                    if profile != "dev" {
                        args.push("--release".into());
                    }
                    if let Some(features) = invocation.value_os("features") {
                        args.extend(["--features".into(), features.to_owned()]);
                    }
                    args.append(&mut forwarded);
                    steps.push(Work::Cargo(args));
                }
                "dx.lint" => {
                    steps.extend([Work::Acquire, Work::Seed]);
                    steps.push(Work::cargo(&["fmt", "--all", "--", "--check"]));
                    let mut args = vec![
                        "clippy".into(),
                        "--workspace".into(),
                        "--all-targets".into(),
                    ];
                    if profile != "dev" {
                        args.push("--release".into());
                    }
                    if let Some(features) = invocation.value_os("features") {
                        args.extend(["--features".into(), features.to_owned()]);
                    }
                    args.append(&mut forwarded);
                    args.extend(["--".into(), "-D".into(), "warnings".into()]);
                    steps.push(Work::Cargo(args));
                }
                "dx.doctor" => steps.push(Work::Doctor),
                "dx.setup" => {
                    if platform == "desktop" {
                        steps.extend([
                            Work::Check,
                            Work::Acquire,
                            Work::Native,
                            Work::Tailwind,
                            Work::PackageTools,
                            Work::Codesign,
                            Work::Helper,
                        ]);
                    } else {
                        steps.push(Work::Dioxus(platform.into()));
                        if platform == "web" {
                            steps.extend([Work::Binaryen, Work::Tailwind]);
                        }
                    }
                }
                "dx.cache.status" => steps.push(Work::Status),
                "dx.cache.clean" => {
                    steps.extend([Work::Acquire, Work::Clean(invocation.flag("apply"))])
                }
                "dx.ci-scope" => steps.push(Work::Scope(forwarded)),
                "dx.ci-native" => steps.push(Work::NativeInputs(forwarded)),
                "dx.framework-consumer" => steps.extend([Work::Acquire, Work::Consumer(forwarded)]),
                "dx.internal" => {
                    let Some(task) = invocation.value("task") else {
                        commands.entity(entity).insert((
                            OperationState::Failed,
                            CliResult(Err("Missing integration task".into())),
                        ));
                        continue;
                    };
                    if task == "cli-release" {
                        let preview = match forwarded.as_slice() {
                            [] => false,
                            [value] if value == "preview" => true,
                            _ => {
                                commands.entity(entity).insert((
                                    OperationState::Failed,
                                    CliResult(Err(
                                        "cli-release accepts only an optional preview channel"
                                            .into(),
                                    )),
                                ));
                                continue;
                            }
                        };
                        steps.extend([
                            Work::Check,
                            Work::Acquire,
                            Work::Preflight,
                            Work::CliCompile,
                            Work::CliPackage(preview),
                            Work::Finish("release".into()),
                        ]);
                    } else {
                        if task == "cargo" {
                            steps.extend([Work::Acquire, Work::Seed]);
                        }
                        steps.push(Work::Internal(task.into(), forwarded));
                    }
                }
                _ => {
                    commands.entity(entity).insert((
                        OperationState::Failed,
                        CliResult(Err("Unknown development command".into())),
                    ));
                    continue;
                }
            }
        }
        commands.entity(entity).insert(Planned);
        let mut previous = None;
        for work in steps {
            let mut step = commands.spawn((
                Name::new(work.name()),
                OperationState::Pending,
                CreatedAt::now(),
                ChildOf(entity),
                Attempt(1),
                work,
            ));
            if let Some(previous) = previous {
                step.insert(Predecessor(previous));
            }
            previous = Some(step.id());
        }
    }
}

fn start(
    steps: Pending,
    running: Query<Entity, With<Running>>,
    completed: Query<Entity, With<Completed>>,
    operations: Inputs,
    mailbox: Res<Mailbox>,
    mut commands: Commands,
) {
    if !running.is_empty() {
        return;
    }
    for (entity, work, parent, predecessor, attempt) in &steps {
        if predecessor.is_some_and(|previous| !completed.contains(previous.0)) {
            continue;
        }
        let Ok((checkout, settings, storage, lease, executable, local, state)) =
            operations.get(parent.parent())
        else {
            continue;
        };
        if *state == OperationState::Canceled {
            continue;
        }
        if matches!(work, Work::LocalLaunch) && local.is_none() {
            continue;
        }
        let checkout = checkout.clone();
        let mut environment = settings.0.clone();
        if let Some(executable) = executable {
            environment.set("VMUX_DX_PROVIDER_EXECUTABLE", executable.path().as_os_str());
        }
        if let Some(local) = local {
            environment.set("VMUX_DX_LOCAL_APP", local.0.as_os_str());
        }
        let storage = storage.clone();
        let lease = lease.map(|lease| lease.0.clone());
        let work = work.clone();
        let sender = mailbox.sender.clone();
        let wake = mailbox.wake.clone();
        let attempt = attempt.0;
        commands
            .entity(entity)
            .insert((Running, OperationState::Running));
        thread::spawn(move || {
            let name = work.name();
            let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                work.execute(&checkout, environment, &storage, lease)
            })) {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(error)) => Err(Failure::from_error(error).context(name)),
                Err(_) => Err(Failure::message("Development step panicked")),
            };
            let _ = sender.send(Completion {
                entity,
                attempt,
                result,
            });
            wake.unpark();
        });
        break;
    }
}

fn receive(mailbox: Res<Mailbox>, mut messages: MessageWriter<Completion>) {
    let receiver = mailbox.receiver.lock().unwrap();
    while let Ok(completion) = receiver.try_recv() {
        messages.write(completion);
    }
}

type Selections<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static local::Selection, &'static Children),
    (
        With<local::Inspected>,
        Without<local::Target>,
        Without<CliResult>,
    ),
>;

fn select_local(operations: Selections, entries: Query<&local::Entry>, mut commands: Commands) {
    for (entity, selection, children) in &operations {
        let prefix = format!("{} (", selection.product);
        let mut latest = None;
        for child in children.iter() {
            let Ok(entry) = entries.get(child) else {
                continue;
            };
            if !entry.directory || entry.symlink {
                continue;
            }
            let Some(name) = entry.path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with(&prefix) || !name.ends_with(").app") {
                continue;
            }
            if latest.is_none_or(|previous: &local::Entry| entry.modified > previous.modified) {
                latest = Some(entry);
            }
        }
        match latest {
            Some(entry) => {
                commands
                    .entity(entity)
                    .insert(local::Target(entry.path.clone()));
            }
            None => {
                commands.entity(entity).insert((
                    OperationState::Failed,
                    CliResult(Err(
                        "No completed local app; run vmux build -p local first".into()
                    )),
                ));
            }
        }
    }
}

fn cancel(
    signals: Option<Res<signals::Signals>>,
    mut operations: Cancellations,
    running: Query<&ChildOf, With<Running>>,
    mut commands: Commands,
) {
    if signals.is_none() {
        return;
    }
    let Some(signal) = signals::Signals::pending() else {
        return;
    };
    for (entity, mut state) in &mut operations {
        *state = OperationState::Canceled;
        if !running.iter().any(|parent| parent.parent() == entity) {
            commands
                .entity(entity)
                .insert(CliResult(Ok((128 + signal) as u8)));
        }
    }
}

fn collect(
    mut completions: MessageReader<Completion>,
    steps: Query<(&ChildOf, &Attempt), With<Running>>,
    mut operations: Query<(&mut Settings, &mut OperationState), With<Accepted>>,
    mut commands: Commands,
) {
    for completion in completions.read() {
        let Ok((parent, attempt)) = steps.get(completion.entity) else {
            continue;
        };
        if attempt.0 != completion.attempt {
            continue;
        }
        let Ok((mut settings, mut state)) = operations.get_mut(parent.parent()) else {
            continue;
        };
        if *state == OperationState::Canceled {
            commands
                .entity(completion.entity)
                .remove::<Running>()
                .insert((Completed, OperationState::Canceled));
            continue;
        }
        match &completion.result {
            Ok(outcome) => {
                match outcome {
                    Outcome::LocalEntries(entries) => {
                        for entry in entries {
                            commands.spawn((entry.clone(), Completed, ChildOf(parent.parent())));
                        }
                        commands.entity(parent.parent()).insert(local::Inspected);
                    }
                    Outcome::Executable(executable) => {
                        commands
                            .entity(parent.parent())
                            .insert(executable.clone())
                            .remove::<Lease>();
                    }
                    Outcome::Done => {}
                    Outcome::Lease(lease) => {
                        commands
                            .entity(parent.parent())
                            .insert(Lease(lease.clone()));
                    }
                    Outcome::Identity(identity) => {
                        settings.0.set("APPLE_SIGNING_IDENTITY", identity);
                    }
                    Outcome::Artifacts(paths) => {
                        for path in paths {
                            commands.spawn((
                                Name::new(path.display().to_string()),
                                Artifact(path.clone()),
                                CreatedAt::now(),
                                vmux_ecs::Ready,
                                Completed,
                                ChildOf(parent.parent()),
                            ));
                        }
                    }
                    Outcome::ToolPath(key, path) => {
                        let key = key.to_str().expect("tool configuration key");
                        settings.0.set(key, path);
                    }
                }
                commands
                    .entity(completion.entity)
                    .remove::<Running>()
                    .insert((Completed, OperationState::Succeeded));
            }
            Err(error) => {
                *state = if signals::Signals::pending().is_some() {
                    OperationState::Canceled
                } else {
                    OperationState::Failed
                };
                eprintln!("vmux: {error}");
                commands
                    .entity(completion.entity)
                    .remove::<Running>()
                    .insert((Completed, OperationState::Failed));
                commands
                    .entity(parent.parent())
                    .insert(CliResult(Ok(error.code() as u8)));
            }
        }
    }
}

fn finish(
    operations: Finishing,
    completed: Query<Entity, With<Completed>>,
    artifacts: Query<&Artifact, With<vmux_ecs::Ready>>,
    mut commands: Commands,
) {
    for (entity, children, state) in &operations {
        if *state == OperationState::Canceled {
            continue;
        }
        if children.iter().all(|child| completed.contains(child)) {
            for child in children.iter() {
                if let Ok(artifact) = artifacts.get(child) {
                    println!("Ready artifact: {}", artifact.0.display());
                }
            }
            commands
                .entity(entity)
                .insert((OperationState::Succeeded, CliResult::default()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        operation: Entity,
        previous: Option<Entity>,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().canonicalize().unwrap();
            let target = root.join("target");
            let checkout = Checkout {
                root: root.clone(),
                target: target.clone(),
                build_target: None,
            };
            let storage =
                Storage::new(root.clone(), target, policy::Policy::environment().unwrap()).unwrap();
            let mut app = App::new();
            app.add_plugins(Plugin);
            let operation = app
                .world_mut()
                .spawn((
                    Accepted,
                    Operation::bundle("dx.fixture").unwrap().running(),
                    Name::new("DX fixture"),
                    checkout,
                    Settings(Environment::current()),
                    storage,
                ))
                .id();
            Self {
                app,
                directory,
                operation,
                previous: None,
            }
        }

        fn step(&mut self, command: &str) -> Entity {
            let work = Work::Tool("/bin/sh".into(), vec!["-c".into(), command.into()]);
            let mut step = self.app.world_mut().spawn((
                OperationState::Pending,
                ChildOf(self.operation),
                Attempt(1),
                work,
            ));
            if let Some(previous) = self.previous {
                step.insert(Predecessor(previous));
            }
            self.previous = Some(step.id());
            step.id()
        }

        fn result(&mut self) -> u8 {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if let Some(CliResult(Ok(code))) = self.app.world().get::<CliResult>(self.operation)
                {
                    return *code;
                }
                assert!(Instant::now() < deadline, "operation did not finish");
                std::thread::park_timeout(Duration::from_millis(10));
            }
        }
    }

    #[test]
    fn failed_predecessor_preserves_its_exit_code_and_refuses_later_side_effects() {
        let mut fixture = Fixture::new();
        fixture.step("exit 37");
        fixture.step("printf unexpected > later");
        assert_eq!(fixture.result(), 37);
        assert!(!fixture.directory.path().join("later").exists());
    }

    #[test]
    fn successful_steps_follow_their_dependencies_before_reporting_completion() {
        let mut fixture = Fixture::new();
        fixture.step("printf first > order");
        fixture.step("test \"$(cat order)\" = first && printf second >> order");
        assert_eq!(fixture.result(), 0);
        assert_eq!(
            std::fs::read_to_string(fixture.directory.path().join("order")).unwrap(),
            "firstsecond"
        );
    }

    #[test]
    fn stale_completion_cannot_advance_the_current_attempt() {
        let mut fixture = Fixture::new();
        let step = fixture.step("exit 99");
        fixture.app.world_mut().entity_mut(step).insert(Running);
        fixture.app.world_mut().write_message(Completion {
            entity: step,
            attempt: 0,
            result: Ok(Outcome::Done),
        });
        fixture.app.update();
        assert!(fixture.app.world().get::<Completed>(step).is_none());
        assert!(
            fixture
                .app
                .world()
                .get::<CliResult>(fixture.operation)
                .is_none()
        );
        fixture.app.world_mut().write_message(Completion {
            entity: step,
            attempt: 1,
            result: Ok(Outcome::Done),
        });
        assert_eq!(fixture.result(), 0);
    }
}
