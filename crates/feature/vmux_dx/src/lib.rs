pub use ci::CiPlugin;

#[cfg(feature = "development")]
use std::ffi::OsString;
#[cfg(feature = "development")]
use std::sync::{Arc, Mutex, mpsc};
#[cfg(feature = "development")]
use std::thread;

#[cfg(feature = "development")]
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
#[cfg(feature = "development")]
use bevy_ecs::system::SystemParam;
#[cfg(feature = "development")]
use vmux_ecs::cli::{CliInvocation, CliResult};
#[cfg(feature = "development")]
use vmux_ecs::{CreatedAt, Cwd, Operation, OperationState};
#[cfg(feature = "development")]
use vmux_feature::manifest::{FeatureManifestSource, FeaturePlugin};

use environment::Environment;
#[cfg(feature = "development")]
use execution::Failure;
#[cfg(feature = "development")]
use repository::Checkout;
#[cfg(feature = "development")]
use steps::{
    BinaryenStep, CliCompileStep, CliPackageStep, ConfiguredToolPath, ConsumerStep, CssStep,
    DesktopPackageStep, DioxusStep, DoctorStep, FlattenWebStep, HelperStep, InternalStep,
    NativeInputsStep, NativeToolsStep, PackageToolsStep, ProducedArtifacts, ReleaseIosStep,
    ResetLayoutStep, ResolvedExecutable, ResolvedIdentity, ResourcesIosStep, ScopeStep, SeedStep,
    SimulatorStep, StorageCheckStep, StorageCleanStep, StorageFinishStep, StoragePreflightStep,
    StorageStatusStep, TailwindStep, VerifyIosStep,
};
#[cfg(feature = "development")]
use storage::Storage;
#[cfg(feature = "development")]
use target_lock::TargetLock;

#[cfg(feature = "development")]
mod cargo;
mod ci;
mod ci_io;
#[cfg(feature = "ci-syntax")]
#[cfg(feature = "development")]
mod ci_native;
#[cfg(feature = "development")]
mod ci_scope;
#[cfg(feature = "development")]
mod cli_package;
#[cfg(feature = "development")]
mod cli_package_io;
#[cfg(feature = "development")]
mod compiler_lint;
#[cfg(feature = "development")]
mod compiler_lint_io;
#[cfg(feature = "development")]
mod consumer;
mod consumer_graph;
#[cfg(feature = "development")]
mod debug;
#[cfg(feature = "development")]
mod debug_io;
mod environment;
mod execution;
#[cfg(feature = "development")]
mod local;
#[cfg(feature = "development")]
mod native;
#[cfg(feature = "development")]
mod package;
#[cfg(feature = "development")]
mod package_io;
#[cfg(feature = "development")]
mod policy;
#[cfg(feature = "development")]
mod process_observation;
#[path = "../../../build_product.rs"]
#[allow(dead_code)]
#[cfg(feature = "development")]
mod product;
#[cfg(feature = "development")]
mod publication;
#[cfg(feature = "development")]
mod publication_io;
#[cfg(feature = "development")]
pub mod repository;
#[cfg(feature = "development")]
mod repository_io;
#[cfg(feature = "development")]
mod repository_selection;
#[cfg(feature = "development")]
mod reset;
#[cfg(feature = "development")]
mod reset_io;
mod signals;
#[cfg(feature = "development")]
mod source;
#[cfg(feature = "development")]
mod steps;
#[cfg(feature = "development")]
mod storage;
#[cfg(feature = "development")]
mod storage_io;
#[cfg(feature = "development")]
mod storage_runtime;
#[cfg(feature = "development")]
mod target_lock;
mod temporary;
#[cfg(test)]
mod testing;
#[cfg(feature = "development")]
mod tooling;
#[cfg(feature = "development")]
mod tooling_io;
#[cfg(feature = "development")]
mod update_io;
#[cfg(feature = "development")]
pub mod updates;
#[cfg(feature = "development")]
mod workspace;
#[cfg(feature = "development")]
mod workspace_io;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(feature = "development")]
pub struct DxPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
#[cfg(feature = "development")]
enum Stage {
    Initialize,
    Execute,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
#[cfg(feature = "development")]
enum ExecutionSet {
    Dispatch,
    RunNative,
    Finish,
}

#[cfg(feature = "development")]
struct NativeStepsPlugin;

#[cfg(feature = "development")]
impl bevy_app::Plugin for NativeStepsPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                ExecutionSet::Dispatch,
                ExecutionSet::RunNative,
                ExecutionSet::Finish,
            )
                .chain()
                .in_set(Stage::Execute),
        )
        .add_systems(
            Update,
            (
                (
                    dispatch_storage_check,
                    dispatch_storage_preflight,
                    dispatch_storage_finish,
                    dispatch_storage_status,
                    dispatch_storage_clean,
                    dispatch_desktop_package,
                    dispatch_cli_compile,
                    dispatch_cli_package,
                    dispatch_reset_layout,
                    dispatch_internal_package,
                )
                    .chain()
                    .in_set(ExecutionSet::Dispatch),
                (
                    (
                        run_seed,
                        run_native_tools,
                        run_helper,
                        run_package_tools,
                        run_dioxus,
                        run_binaryen,
                        run_tailwind,
                        run_resources_ios,
                        run_release_ios,
                    ),
                    (
                        run_verify_ios,
                        run_css,
                        run_flatten_web,
                        run_simulator,
                        run_doctor,
                        run_ci_scope,
                        run_ci_native,
                        run_sdk_consumer,
                        run_internal,
                    ),
                )
                    .in_set(ExecutionSet::RunNative),
            ),
        );
    }
}

#[cfg(feature = "development")]
impl bevy_app::Plugin for DxPlugin {
    fn build(&self, app: &mut App) {
        Self::runtime(app);
        app.add_plugins((
            FeaturePlugin::<Feature>::default(),
            CiPlugin,
            updates::UpdatesPlugin,
            workspace::WorkspacePlugin,
            repository_selection::RepositorySelectionPlugin,
            debug::DebugPlugin,
            storage_runtime::StorageRuntimePlugin,
            process_observation::ProcessObservationPlugin,
            publication::PublicationPlugin,
            cli_package::CliPackagePlugin,
            package::PackagePlugin,
            reset::ResetPlugin,
            tooling::ToolingPlugin,
            NativeStepsPlugin,
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
        .add_systems(
            Update,
            (normalize, initialize).chain().in_set(Stage::Initialize),
        )
        .insert_resource(Mailbox::new())
        .add_message::<Completion>()
        .add_systems(
            Update,
            (
                admit,
                (
                    resolve,
                    load_settings,
                    configure_target,
                    ensure_tooling,
                    configure_settings,
                    accept,
                )
                    .chain(),
                (
                    (
                        plan_provider,
                        prepare_build,
                        plan_desktop_dev,
                        plan_desktop_package,
                        plan_mobile_release,
                        plan_mobile,
                        plan_web,
                        refuse_build,
                        finish_build_plan,
                    )
                        .chain(),
                    (
                        plan_run,
                        plan_stop,
                        plan_reset_layout,
                        plan_test,
                        plan_lint,
                        plan_doctor,
                        plan_setup,
                        plan_cache_status,
                        plan_cache_clean,
                        plan_ci_scope,
                        plan_ci_native,
                        plan_checks,
                        plan_sdk,
                        plan_cli_release,
                        plan_internal,
                    )
                        .chain(),
                )
                    .chain(),
                refuse_plan,
                receive,
                cancel,
                (collect, apply).chain(),
                retire,
                start,
            )
                .chain()
                .before(ExecutionSet::Dispatch)
                .in_set(Stage::Execute),
        )
        .add_systems(Update, finish.in_set(ExecutionSet::Finish));
    }
}

#[cfg(feature = "development")]
impl DxPlugin {
    fn runtime(app: &mut App) {
        if !app
            .world()
            .contains_resource::<vmux_ecs::blocking::BlockingRuntime>()
        {
            app.insert_resource(vmux_ecs::blocking::BlockingRuntime::new(
                vmux_ecs::blocking::BlockingPool::new("vmux-dx-short", 1, 16),
                vmux_ecs::blocking::BlockingPool::new("vmux-dx-long", 0, 0),
                vmux_ecs::blocking::BlockingPool::new("vmux-dx-process", 1, 16),
            ));
        }
    }

    pub fn selected(arguments: impl Iterator<Item = OsString>) -> bool {
        workspace::Host::selected(arguments.collect())
    }
}

#[cfg(feature = "development")]
struct Feature;

#[cfg(feature = "development")]
impl FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[derive(Component)]
#[require(Tail)]
#[cfg(feature = "development")]
struct Accepted;

#[derive(Component)]
#[cfg(feature = "development")]
struct Admitting;
#[derive(Component)]
#[cfg(feature = "development")]
struct Admission;
#[derive(Component)]
#[cfg(feature = "development")]
struct Validated(std::path::PathBuf);
#[derive(Component)]
#[component(immutable)]
#[cfg(feature = "development")]
struct Started {
    context: Entity,
    attempt: u64,
}
#[derive(Component)]
#[cfg(feature = "development")]
struct Retirement(Failure);
#[derive(Component)]
#[cfg(feature = "development")]
struct Assigned;

#[derive(Component, Default)]
#[cfg(feature = "development")]
struct Tail(Option<Entity>);

#[cfg(feature = "development")]
#[derive(Component, Default)]
pub(crate) struct Step;

#[derive(Component)]
#[cfg(feature = "development")]
struct NativeSubmitted;

#[derive(Component, Clone)]
struct Settings(Environment);

#[derive(Component)]
#[cfg(feature = "development")]
struct Lease(Arc<TargetLock>);

#[derive(Component)]
#[cfg(feature = "development")]
struct Predecessor(Entity);

#[derive(Component)]
#[cfg(feature = "development")]
struct Running;

#[derive(Component)]
#[cfg(feature = "development")]
struct Completed;

#[derive(Component)]
#[cfg(feature = "development")]
pub struct Artifact(pub std::path::PathBuf);

#[derive(Component)]
#[cfg(feature = "development")]
struct Attempt(u64);

#[derive(Resource)]
#[cfg(feature = "development")]
struct Mailbox {
    sender: mpsc::SyncSender<Completion>,
    receiver: Mutex<mpsc::Receiver<Completion>>,
    tool_path_tx: mpsc::SyncSender<(Entity, ConfiguredToolPath)>,
    tool_path_rx: Mutex<mpsc::Receiver<(Entity, ConfiguredToolPath)>>,
    artifacts_tx: mpsc::SyncSender<(Entity, ProducedArtifacts)>,
    artifacts_rx: Mutex<mpsc::Receiver<(Entity, ProducedArtifacts)>>,
    wake: thread::Thread,
}

#[cfg(feature = "development")]
impl Mailbox {
    fn new() -> Self {
        let (sender, receiver) = mpsc::sync_channel(1);
        let (tool_path_tx, tool_path_rx) = mpsc::sync_channel(1);
        let (artifacts_tx, artifacts_rx) = mpsc::sync_channel(1);
        Self {
            sender,
            receiver: Mutex::new(receiver),
            tool_path_tx,
            tool_path_rx: Mutex::new(tool_path_rx),
            artifacts_tx,
            artifacts_rx: Mutex::new(artifacts_rx),
            wake: thread::current(),
        }
    }
}

#[derive(Message)]
#[cfg(feature = "development")]
struct Completion {
    entity: Entity,
    context: Entity,
    attempt: u64,
    result: std::result::Result<(), Failure>,
}

#[cfg(feature = "development")]
type Completions<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static ChildOf>,
        Option<&'static Attempt>,
        &'static Started,
    ),
    (With<Running>, With<Step>, Without<Admission>),
>;

#[cfg(feature = "development")]
type Incoming<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        Option<&'static workspace::Dispatch>,
        Option<&'static Checkout>,
    ),
    (
        Without<Accepted>,
        Without<CliResult>,
        Without<tooling::Waiting>,
        Without<Admitting>,
        Without<Preparing>,
    ),
>;
#[cfg(feature = "development")]
type Pending<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Has<debug::Request>,
        &'static ChildOf,
        Option<&'static Predecessor>,
        &'static Attempt,
    ),
    (With<Step>, Without<Running>, Without<Completed>),
>;
#[cfg(feature = "development")]
type Inputs<'w, 's> = Query<
    'w,
    's,
    (
        &'static Checkout,
        &'static Settings,
        &'static Storage,
        Option<&'static Lease>,
        Option<&'static workspace::Executable>,
        &'static OperationState,
    ),
    Without<CliResult>,
>;
#[cfg(feature = "development")]
type Cancellations<'w, 's> =
    Query<'w, 's, (Entity, &'static mut OperationState), (With<Accepted>, Without<CliResult>)>;
#[cfg(feature = "development")]
type Finishing<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Children, &'static OperationState),
    (With<Accepted>, Without<CliResult>),
>;

#[cfg(feature = "development")]
fn normalize(mut requests: Query<&mut CliInvocation, Without<CliResult>>) {
    for mut invocation in &mut requests {
        if invocation.is("dx.desktop") {
            invocation.command = "dx.run".into();
        }
    }
}

#[cfg(feature = "development")]
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

#[cfg(feature = "development")]
type Admissions<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static ChildOf>,
        &'static Cwd,
        Option<&'static Attempt>,
        Option<&'static Started>,
    ),
    With<Admission>,
>;

#[cfg(feature = "development")]
fn admit(
    mut completions: MessageReader<Completion>,
    steps: Admissions,
    pending: Query<(), With<Admitting>>,
    states: Query<&OperationState>,
    mut commands: Commands,
) {
    for completion in completions.read() {
        let Ok((parent, path, attempt, started)) = steps.get(completion.entity) else {
            continue;
        };
        if !pending.contains(completion.context) {
            continue;
        };
        let valid = parent.is_some_and(|parent| parent.parent() == completion.context)
            && attempt.is_some_and(|attempt| attempt.0 == completion.attempt)
            && started.is_some_and(|started| {
                started.context == completion.context && started.attempt == completion.attempt
            });
        if !valid {
            commands.entity(completion.context).insert(CliResult(Err(
                "Development admission ownership changed".into(),
            )));
        } else if states
            .get(completion.context)
            .is_ok_and(|state| *state == OperationState::Canceled)
        {
            let code = (128 + signals::Signals::pending().unwrap_or(15)) as u8;
            commands
                .entity(completion.context)
                .insert(CliResult(Ok(code)));
        } else {
            match &completion.result {
                Ok(_) => {
                    commands
                        .entity(completion.context)
                        .insert(Validated(path.0.clone()));
                }
                Err(error) => {
                    commands
                        .entity(completion.context)
                        .insert(CliResult(Err(error.to_string())));
                }
            }
        }
        commands.entity(completion.context).remove::<Admitting>();
        commands.entity(completion.entity).despawn();
    }
}

#[derive(Component)]
#[cfg(feature = "development")]
struct Preparing;

#[derive(Component)]
#[cfg(feature = "development")]
struct SettingsLoaded;

#[derive(Component)]
#[cfg(feature = "development")]
struct TargetConfigured;

#[derive(Component)]
#[cfg(feature = "development")]
struct ToolsPrepared;

#[derive(Component)]
#[cfg(feature = "development")]
struct SettingsConfigured;

#[cfg(feature = "development")]
type Preparations<'w, 's, F> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        Option<&'static workspace::Dispatch>,
        &'static mut Checkout,
        &'static mut Settings,
    ),
    (With<Preparing>, Without<CliResult>, F),
>;

#[cfg(feature = "development")]
fn resolve(requests: Incoming, providers: Query<&workspace::Provider>, mut commands: Commands) {
    for (entity, invocation, dispatch, resolved_context) in &requests {
        if invocation.command.starts_with("dx.ci.") {
            continue;
        }
        if invocation.is("dx.update") {
            continue;
        }
        let provider = dispatch.and_then(|dispatch| providers.get(dispatch.provider).ok());
        if !invocation.command.starts_with("dx.") && provider.is_none() {
            continue;
        }
        if invocation.is("dx.help") {
            println!(
                "Vmux development commands: build, run, serve, stop, test, lint, doctor, cache\nUse vmux <command> --help."
            );
            commands.entity(entity).insert(CliResult::default());
            continue;
        }
        let resolved: Result<Checkout> = match provider {
            Some(provider) => {
                let target = provider
                    .root
                    .join(".vmux/providers")
                    .join(&provider.package);
                let mut path = provider.root.clone();
                let mut refused = false;
                for component in target.strip_prefix(&provider.root).unwrap().components() {
                    path.push(component);
                    if std::fs::symlink_metadata(&path)
                        .is_ok_and(|metadata| metadata.file_type().is_symlink())
                    {
                        refused = true;
                        break;
                    }
                }
                if refused {
                    Err("Workspace provider cache cannot use symlinks".into())
                } else {
                    match std::fs::create_dir_all(target.parent().unwrap()) {
                        Ok(()) => Ok(Checkout {
                            root: provider.root.clone(),
                            target,
                            build_target: Some(env!("VMUX_DX_HOST_TARGET").into()),
                        }),
                        Err(error) => Err(error.into()),
                    }
                }
            }
            None => match resolved_context {
                Some(checkout) => Ok(checkout.clone()),
                None => continue,
            },
        };
        let checkout = match resolved {
            Ok(checkout) => checkout,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
                continue;
            }
        };
        commands
            .entity(entity)
            .insert((Preparing, checkout, Settings(Environment::current())));
    }
}

#[cfg(feature = "development")]
fn load_settings(mut requests: Preparations<Without<SettingsLoaded>>, mut commands: Commands) {
    for (entity, invocation, dispatch, checkout, mut settings) in &mut requests {
        let environment = &mut settings.0;
        if dispatch.is_none()
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
        commands.entity(entity).insert(SettingsLoaded);
    }
}

#[cfg(feature = "development")]
fn configure_target(
    mut requests: Preparations<(With<SettingsLoaded>, Without<TargetConfigured>)>,
    mut commands: Commands,
) {
    for (entity, invocation, dispatch, mut checkout, mut settings) in &mut requests {
        let environment = &mut settings.0;
        let platform = if invocation.is("dx.serve") {
            Some("web")
        } else {
            invocation.value("platform")
        };
        if invocation.is("dx.internal") && invocation.value("task") == Some("cli-release") {
            environment.remove("CARGO_BUILD_TARGET");
            environment.set(
                "CARGO_TARGET_DIR",
                checkout
                    .tools_root(environment)
                    .join(".vmux/standalone")
                    .as_os_str(),
            );
        }
        if platform == Some("web") && !environment.present("CARGO_TARGET_DIR") {
            environment.set(
                "CARGO_TARGET_DIR",
                checkout.root.join("website/target").as_os_str(),
            );
        }
        if dispatch.is_none()
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
        commands.entity(entity).insert(TargetConfigured);
    }
}

#[cfg(feature = "development")]
fn ensure_tooling(
    mut requests: Preparations<(
        With<TargetConfigured>,
        Without<ToolsPrepared>,
        Without<tooling::Waiting>,
    )>,
    prepared: Query<(), With<tooling::Checked>>,
    mut commands: Commands,
) {
    for (entity, _, dispatch, checkout, mut settings) in &mut requests {
        if dispatch.is_none() && !prepared.contains(entity) {
            commands.trigger(tooling::Requested {
                entity,
                checkout: checkout.clone(),
                environment: settings.0.clone(),
            });
            continue;
        }
        if dispatch.is_none() {
            settings
                .0
                .set("CARGO_TARGET_DIR", checkout.target.as_os_str());
        }
        commands.entity(entity).insert(ToolsPrepared);
    }
}

#[cfg(feature = "development")]
fn configure_settings(
    mut requests: Preparations<(With<ToolsPrepared>, Without<SettingsConfigured>)>,
    mut commands: Commands,
) {
    for (entity, invocation, _, _, mut settings) in &mut requests {
        let environment = &mut settings.0;
        let profile = if invocation.is("dx.serve") {
            Some("dev")
        } else {
            invocation.value("profile")
        };
        let platform = if invocation.is("dx.serve") {
            Some("web")
        } else {
            invocation.value("platform")
        };
        if let Some(profile) = profile {
            environment.set("VMUX_BUILD_PROFILE", profile);
        } else if !environment.present("VMUX_BUILD_PROFILE") {
            environment.set("VMUX_BUILD_PROFILE", "dev");
        }
        if let Some(platform) = platform {
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
        commands.entity(entity).insert(SettingsConfigured);
    }
}

#[cfg(feature = "development")]
fn accept(
    mut requests: Preparations<(
        With<SettingsConfigured>,
        Without<Accepted>,
        Without<Admitting>,
    )>,
    validated: Query<&Validated>,
    mut commands: Commands,
) {
    for (entity, invocation, _, checkout, settings) in &mut requests {
        let environment = settings.0.clone();
        let storage = match policy::Policy::configured(&environment) {
            Ok(policy) => Storage::new(policy),
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
                continue;
            }
        };
        if !validated
            .get(entity)
            .is_ok_and(|validated| validated.0 == checkout.target)
        {
            commands.entity(entity).insert((
                Admitting,
                Operation::bundle(&invocation.command).unwrap().running(),
                checkout.clone(),
                storage,
                Settings(environment),
            ));
            commands.spawn((
                Step,
                Admission,
                Started {
                    context: entity,
                    attempt: 1,
                },
                ChildOf(entity),
                Attempt(1),
                Running,
                OperationState::Running,
                storage_runtime::Request::new(storage_runtime::RequestKind::Validate),
                storage_runtime::Captured {
                    root: entity,
                    attempt: 1,
                },
                storage_runtime::Loan(None),
            ));
            continue;
        }
        commands.entity(entity).insert((
            Accepted,
            Operation::bundle(&invocation.command).unwrap().running(),
            Name::new(invocation.command.clone()),
            CreatedAt::now(),
            Cwd(checkout.root.clone()),
            checkout.clone(),
            storage,
            Settings(environment),
        ));
    }
}

#[derive(SystemParam)]
#[cfg(feature = "development")]
struct Plans<'w, 's> {
    commands: Commands<'w, 's>,
}

#[cfg(feature = "development")]
impl Plans<'_, '_> {
    fn push<B: Bundle>(&mut self, owner: Entity, tail: &mut Tail, bundle: B) -> Entity {
        let mut step = self.commands.spawn((
            Step,
            OperationState::Pending,
            CreatedAt::now(),
            ChildOf(owner),
            Attempt(1),
            bundle,
        ));
        if let Some(previous) = tail.0 {
            step.insert(Predecessor(previous));
        }
        let entity = step.id();
        tail.0 = Some(entity);
        entity
    }

    fn step<W: Component>(&mut self, owner: Entity, tail: &mut Tail, name: &'static str, work: W) {
        self.push(owner, tail, (Name::new(name), work));
    }

    fn request<T: Component + Clone>(
        &mut self,
        owner: Entity,
        tail: &mut Tail,
        name: &'static str,
        request: T,
    ) {
        let entity = self.push(owner, tail, Name::new(name));
        self.commands.trigger(debug::Requested { entity, request });
    }
}

#[derive(Component)]
#[cfg(feature = "development")]
struct Planned;

#[cfg(feature = "development")]
type Planning<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        Option<&'static workspace::Dispatch>,
        &'static Storage,
        &'static mut Tail,
    ),
    (With<Accepted>, Without<Planned>, Without<CliResult>),
>;

#[cfg(feature = "development")]
fn plan_provider(
    mut operations: Planning,
    providers: Query<&workspace::Provider>,
    mut plans: Plans,
) {
    for (entity, _invocation, dispatch, storage, mut tail) in &mut operations {
        if dispatch.is_none() {
            continue;
        }
        let dispatch = dispatch.unwrap();
        let Ok(provider) = providers.get(dispatch.provider) else {
            continue;
        };
        plans.request(
            entity,
            &mut tail,
            "Check available build space",
            debug::Capacity {
                minimum: storage.policy().minimum,
            },
        );
        plans.request(
            entity,
            &mut tail,
            "Acquire target lock",
            debug::Acquire { inherit: false },
        );
        plans.request(
            entity,
            &mut tail,
            "Check available build space",
            debug::Capacity {
                minimum: storage.policy().minimum,
            },
        );
        plans.request(
            entity,
            &mut tail,
            "Build workspace CLI provider",
            debug::Compile {
                package: provider.package.clone(),
                binary: provider.binary.clone(),
                manifest: provider.cargo.clone(),
            },
        );
        plans.request(
            entity,
            &mut tail,
            "Publish workspace CLI executable snapshot",
            debug::Snapshot {
                binary: provider.binary.clone(),
            },
        );
        plans.request(
            entity,
            &mut tail,
            "Run workspace CLI provider",
            debug::Invoke {
                arguments: dispatch.arguments.clone(),
            },
        );

        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn prepare_build(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.build") && tail.0.is_none()) {
            continue;
        }
        plans.step(entity, &mut tail, "Check build storage", StorageCheckStep);
        plans.request(
            entity,
            &mut tail,
            "Acquire target lock",
            debug::Acquire { inherit: true },
        );
        plans.step(entity, &mut tail, "Seed target cache", SeedStep);
        plans.step(
            entity,
            &mut tail,
            "Check build storage",
            StoragePreflightStep,
        );
    }
}

#[cfg(feature = "development")]
fn plan_desktop_dev(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let profile = invocation.value("profile").unwrap_or("dev");
        let platform = invocation.value("platform").unwrap_or("desktop");
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if !(dispatch.is_none()
            && invocation.is("dx.build")
            && (platform == "desktop" && profile == "dev"))
        {
            continue;
        }

        plans.step(entity, &mut tail, "Prepare native tools", NativeToolsStep);
        plans.step(
            entity,
            &mut tail,
            "Prepare stylesheet compiler",
            TailwindStep,
        );
        plans.request(
            entity,
            &mut tail,
            "Prepare signing identity",
            debug::IdentityRequest { print: false },
        );
        plans.step(entity, &mut tail, "Prepare CEF helper", HelperStep);
        plans.request(entity, &mut tail, "Invalidate old artifact", debug::Clear);
        plans.request(
            entity,
            &mut tail,
            "Run Cargo",
            cargo::Run::new(&["build", "-p", "vmux_cli", "-p", "vmux_vm"]),
        );
        plans.request(
            entity,
            &mut tail,
            "Run Cargo",
            cargo::Run::new(&[
                "build",
                "-p",
                "vmux_app",
                "--bin",
                "vmux_service",
                "--features",
                "service-daemon",
            ]),
        );
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
            args.extend(["--", "-C", "link-arg=-Wl,-headerpad,1000"].map(OsString::from));
        }
        plans.request(
            entity,
            &mut tail,
            "Run Cargo",
            cargo::Run { arguments: args },
        );
        if cfg!(target_os = "macos") {
            plans.request(entity, &mut tail, "Sign debug binaries", debug::Sign);
            plans.request(entity, &mut tail, "Bundle debug app", debug::Bundle);
        }
        plans.request(
            entity,
            &mut tail,
            "Verify and publish debug artifact",
            debug::Publish,
        );
    }
}

#[cfg(feature = "development")]
fn plan_desktop_package(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let profile = invocation.value("profile").unwrap_or("dev");
        let platform = invocation.value("platform").unwrap_or("desktop");
        if !(dispatch.is_none()
            && invocation.is("dx.build")
            && (platform == "desktop" && profile != "dev"))
        {
            continue;
        }

        plans.step(entity, &mut tail, "Prepare native tools", NativeToolsStep);
        plans.step(
            entity,
            &mut tail,
            "Prepare stylesheet compiler",
            TailwindStep,
        );
        plans.step(
            entity,
            &mut tail,
            "Prepare packaging tools",
            PackageToolsStep,
        );
        plans.request(
            entity,
            &mut tail,
            "Prepare signing identity",
            debug::IdentityRequest { print: false },
        );
        plans.step(
            entity,
            &mut tail,
            "Package app",
            DesktopPackageStep(profile.into()),
        );
    }
}

#[cfg(feature = "development")]
fn plan_mobile_release(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let profile = invocation.value("profile").unwrap_or("dev");
        let platform = invocation.value("platform").unwrap_or("desktop");
        if !(dispatch.is_none()
            && invocation.is("dx.build")
            && (platform == "ios" && profile == "release"))
        {
            continue;
        }

        plans.step(
            entity,
            &mut tail,
            "Prepare Dioxus tools",
            DioxusStep("ios".into()),
        );
        plans.step(entity, &mut tail, "Sign and package iOS", ReleaseIosStep);
    }
}

#[cfg(feature = "development")]
fn plan_mobile(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let profile = invocation.value("profile").unwrap_or("dev");
        let platform = invocation.value("platform").unwrap_or("desktop");
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if !(dispatch.is_none()
            && invocation.is("dx.build")
            && ((platform == "ios" && profile != "release") || platform == "android"))
        {
            continue;
        }

        plans.step(
            entity,
            &mut tail,
            "Prepare Dioxus tools",
            DioxusStep(platform.into()),
        );
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
        plans.request(
            entity,
            &mut tail,
            "Run tool",
            debug::Tool {
                program: "dx".into(),
                arguments: args,
            },
        );
        if platform == "ios" {
            plans.step(entity, &mut tail, "Inject iOS resources", ResourcesIosStep);
            plans.step(entity, &mut tail, "Verify iOS bundle", VerifyIosStep(false));
        }
    }
}

#[cfg(feature = "development")]
fn plan_web(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let profile = invocation.value("profile").unwrap_or("dev");
        let platform = invocation.value("platform").unwrap_or("desktop");
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if !(dispatch.is_none() && invocation.is("dx.build") && (platform == "web")) {
            continue;
        }

        plans.step(
            entity,
            &mut tail,
            "Prepare Dioxus tools",
            DioxusStep(platform.into()),
        );
        plans.step(entity, &mut tail, "Prepare wasm optimizer", BinaryenStep);
        plans.step(
            entity,
            &mut tail,
            "Prepare stylesheet compiler",
            TailwindStep,
        );
        plans.step(entity, &mut tail, "Build stylesheet", CssStep);
        let mut args = vec!["build".into(), "--platform".into(), "web".into()];
        if profile != "dev" {
            args.extend(["--ssg".into(), "--release".into()]);
        }
        args.append(&mut forwarded);
        plans.request(
            entity,
            &mut tail,
            "Run tool",
            debug::Tool {
                program: "dx".into(),
                arguments: args,
            },
        );
        if profile != "dev" {
            plans.step(entity, &mut tail, "Publish web artifact", FlattenWebStep);
        }
    }
}

#[cfg(feature = "development")]
fn refuse_build(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, _tail) in &mut operations {
        let platform = invocation.value("platform").unwrap_or("desktop");
        if !(dispatch.is_none()
            && invocation.is("dx.build")
            && (!matches!(platform, "desktop" | "ios" | "android" | "web")))
        {
            continue;
        }

        plans.commands.entity(entity).insert((
            OperationState::Failed,
            CliResult(Err("Unsupported build platform".into())),
        ));
        continue;
    }
}

#[cfg(feature = "development")]
fn finish_build_plan(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let profile = invocation.value("profile").unwrap_or("dev");
        if !(dispatch.is_none() && invocation.is("dx.build")) {
            continue;
        }
        plans.step(
            entity,
            &mut tail,
            "Finish build storage",
            StorageFinishStep(profile.into()),
        );
        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_run(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let profile = if invocation.is("dx.serve") {
            "dev"
        } else {
            invocation.value("profile").unwrap_or("dev")
        };
        let platform = if invocation.is("dx.serve") {
            "web"
        } else {
            invocation.value("platform").unwrap_or("desktop")
        };
        if !(dispatch.is_none() && (invocation.is("dx.run") || invocation.is("dx.serve"))) {
            continue;
        }

        plans.request(
            entity,
            &mut tail,
            "Acquire target lock",
            debug::Acquire { inherit: true },
        );
        if platform != "desktop" {
            plans.step(
                entity,
                &mut tail,
                "Prepare Dioxus tools",
                DioxusStep(platform.into()),
            );
            if platform == "web" {
                plans.step(entity, &mut tail, "Prepare wasm optimizer", BinaryenStep);
                plans.step(
                    entity,
                    &mut tail,
                    "Prepare stylesheet compiler",
                    TailwindStep,
                );
                plans.step(entity, &mut tail, "Build stylesheet", CssStep);
            }
            if platform == "ios" {
                plans.step(entity, &mut tail, "Prepare simulator", SimulatorStep);
            }
        }
        plans.request(
            entity,
            &mut tail,
            "Run artifact",
            debug::Run {
                profile: profile.into(),
                platform: platform.into(),
            },
        );

        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_stop(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.stop")) {
            continue;
        }
        plans.request(entity, &mut tail, "Stop development service", debug::Stop);
        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_reset_layout(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.reset-layout")) {
            continue;
        }
        plans.step(entity, &mut tail, "Reset layout files", ResetLayoutStep);
        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_test(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let profile = invocation.value("profile").unwrap_or("dev");
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if !(dispatch.is_none() && invocation.is("dx.test")) {
            continue;
        }

        plans.request(
            entity,
            &mut tail,
            "Acquire target lock",
            debug::Acquire { inherit: true },
        );
        plans.step(entity, &mut tail, "Seed target cache", SeedStep);
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
        plans.request(
            entity,
            &mut tail,
            "Run Cargo",
            cargo::Run { arguments: args },
        );

        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_lint(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let profile = invocation.value("profile").unwrap_or("dev");
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if !(dispatch.is_none() && invocation.is("dx.lint")) {
            continue;
        }

        plans.request(
            entity,
            &mut tail,
            "Acquire target lock",
            debug::Acquire { inherit: true },
        );
        plans.step(entity, &mut tail, "Seed target cache", SeedStep);
        plans.request(
            entity,
            &mut tail,
            "Run Cargo",
            cargo::Run::new(&["fmt", "--all", "--", "--check"]),
        );
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
        let compiler_arguments = args[1..].to_vec();
        args.extend(["--".into(), "-D".into(), "warnings".into()]);
        plans.request(
            entity,
            &mut tail,
            "Run Cargo",
            cargo::Run { arguments: args },
        );

        plans.request(
            entity,
            &mut tail,
            "Run compiler ownership lints",
            compiler_lint::Run {
                fixtures: false,
                arguments: Some(compiler_arguments),
            },
        );

        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_doctor(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.doctor")) {
            continue;
        }
        plans.step(entity, &mut tail, "Inspect tools", DoctorStep);
        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_setup(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let platform = invocation.value("platform").unwrap_or("desktop");
        if !(dispatch.is_none() && invocation.is("dx.setup")) {
            continue;
        }

        if platform == "desktop" {
            plans.step(entity, &mut tail, "Check build storage", StorageCheckStep);
            plans.request(
                entity,
                &mut tail,
                "Acquire target lock",
                debug::Acquire { inherit: true },
            );
            plans.step(entity, &mut tail, "Prepare native tools", NativeToolsStep);
            plans.step(
                entity,
                &mut tail,
                "Prepare stylesheet compiler",
                TailwindStep,
            );
            plans.step(
                entity,
                &mut tail,
                "Prepare packaging tools",
                PackageToolsStep,
            );
            plans.request(
                entity,
                &mut tail,
                "Prepare signing identity",
                debug::IdentityRequest { print: false },
            );
            plans.step(entity, &mut tail, "Prepare CEF helper", HelperStep);
        } else {
            plans.step(
                entity,
                &mut tail,
                "Prepare Dioxus tools",
                DioxusStep(platform.into()),
            );
            if platform == "web" {
                plans.step(entity, &mut tail, "Prepare wasm optimizer", BinaryenStep);
                plans.step(
                    entity,
                    &mut tail,
                    "Prepare stylesheet compiler",
                    TailwindStep,
                );
            }
        }

        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_cache_status(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.cache.status")) {
            continue;
        }
        plans.step(entity, &mut tail, "Report build storage", StorageStatusStep);
        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_cache_clean(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.cache.clean")) {
            continue;
        }

        plans.request(
            entity,
            &mut tail,
            "Acquire target lock",
            debug::Acquire { inherit: true },
        );
        plans.step(
            entity,
            &mut tail,
            "Clean build cache",
            StorageCleanStep(invocation.flag("apply")),
        );

        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_ci_scope(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if !(dispatch.is_none() && invocation.is("dx.ci-scope")) {
            continue;
        }
        plans.step(entity, &mut tail, "Select CI scope", ScopeStep(forwarded));
        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_ci_native(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if !(dispatch.is_none() && invocation.is("dx.ci-native")) {
            continue;
        }
        plans.step(
            entity,
            &mut tail,
            "Classify native CI inputs",
            NativeInputsStep(forwarded),
        );
        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_checks(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.checks.test")) {
            continue;
        }
        plans.step(
            entity,
            &mut tail,
            "Run external integration",
            InternalStep {
                task: "pre-push".into(),
                arguments: Vec::new(),
            },
        );
        plans.request(
            entity,
            &mut tail,
            "Check compiler lint fixtures and source",
            compiler_lint::Run {
                fixtures: true,
                arguments: Some(Vec::new()),
            },
        );
        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_sdk(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.checks.sdk")) {
            continue;
        }
        let mut arguments = Vec::new();
        if invocation.flag("host") {
            arguments.push("--host".into());
        }
        if invocation.flag("git") {
            arguments.push("--git".into());
        }
        plans.request(
            entity,
            &mut tail,
            "Acquire target lock",
            debug::Acquire { inherit: true },
        );
        plans.step(
            entity,
            &mut tail,
            "Validate public SDK",
            ConsumerStep(arguments),
        );
        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_cli_release(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none()
            && invocation.is("dx.internal")
            && invocation.value("task") == Some("cli-release"))
        {
            continue;
        }
        let forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        let preview = match forwarded.as_slice() {
            [] => false,
            [value] if value == "preview" => true,
            _ => {
                plans.commands.entity(entity).insert((
                    OperationState::Failed,
                    CliResult(Err(
                        "cli-release accepts only an optional preview channel".into()
                    )),
                ));
                continue;
            }
        };
        plans.step(entity, &mut tail, "Check build storage", StorageCheckStep);
        plans.request(
            entity,
            &mut tail,
            "Acquire target lock",
            debug::Acquire { inherit: true },
        );
        plans.step(
            entity,
            &mut tail,
            "Check build storage",
            StoragePreflightStep,
        );
        plans.step(
            entity,
            &mut tail,
            "Build standalone release CLI",
            CliCompileStep,
        );
        plans.step(
            entity,
            &mut tail,
            "Verify and package standalone release CLI",
            CliPackageStep(preview),
        );
        plans.step(
            entity,
            &mut tail,
            "Finish build storage",
            StorageFinishStep("release".into()),
        );
        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn plan_internal(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if !(dispatch.is_none() && invocation.is("dx.internal")) {
            continue;
        }
        let Some(task) = invocation.value("task") else {
            plans.commands.entity(entity).insert((
                OperationState::Failed,
                CliResult(Err("Missing integration task".into())),
            ));
            continue;
        };
        if task == "cli-release" {
            continue;
        }
        if task == "cargo" {
            plans.request(
                entity,
                &mut tail,
                "Acquire target lock",
                debug::Acquire { inherit: true },
            );
            plans.step(entity, &mut tail, "Seed target cache", SeedStep);
        }
        match task {
            "ecs-lint" => {
                let test = forwarded.as_slice() == [OsString::from("--test")];
                plans.request(
                    entity,
                    &mut tail,
                    "Run compiler ownership lints",
                    compiler_lint::Run {
                        fixtures: test,
                        arguments: if test { None } else { Some(forwarded) },
                    },
                );
            }
            "cargo" => {
                plans.request(
                    entity,
                    &mut tail,
                    "Run Cargo",
                    cargo::Run {
                        arguments: forwarded,
                    },
                );
            }
            "ensure-signing" => {
                plans.request(
                    entity,
                    &mut tail,
                    "Prepare signing identity",
                    debug::IdentityRequest { print: true },
                );
            }
            "debug-sign" => {
                plans.request(entity, &mut tail, "Sign debug binaries", debug::Sign);
            }
            "debug-bundle" => {
                plans.request(entity, &mut tail, "Bundle debug app", debug::Bundle);
            }
            _ => {
                plans.step(
                    entity,
                    &mut tail,
                    "Run external integration",
                    InternalStep {
                        task: task.into(),
                        arguments: forwarded,
                    },
                );
            }
        }
        plans.commands.entity(entity).insert(Planned);
    }
}

#[cfg(feature = "development")]
fn refuse_plan(operations: Planning, mut commands: Commands) {
    for (entity, _, dispatch, _, _) in &operations {
        let error = if dispatch.is_some() {
            "Workspace provider is unavailable"
        } else {
            "Unknown development command"
        };
        commands
            .entity(entity)
            .insert((OperationState::Failed, CliResult(Err(error.into()))));
    }
}

#[cfg(feature = "development")]
fn start(
    steps: Pending,
    running: Query<Entity, With<Running>>,
    completed: Query<Entity, With<Completed>>,
    operations: Inputs,
    mailbox: Res<Mailbox>,
    reads: Query<&OperationState, With<repository_io::Read>>,
    mut commands: Commands,
) {
    if !running.is_empty()
        || reads
            .iter()
            .any(|state| matches!(state, OperationState::Pending | OperationState::Running))
    {
        return;
    }
    for (entity, requested, parent, predecessor, attempt) in &steps {
        if predecessor.is_some_and(|previous| !completed.contains(previous.0)) {
            continue;
        }
        let Ok((checkout, settings, _storage, lease, executable, state)) =
            operations.get(parent.parent())
        else {
            continue;
        };
        if *state != OperationState::Running {
            continue;
        };
        let mut captured_settings = settings.clone();
        if let Some(executable) = executable {
            captured_settings
                .0
                .set("VMUX_DX_PROVIDER_EXECUTABLE", executable.path().as_os_str());
        }
        let mut step = commands.entity(entity);
        step.insert((
            Started {
                context: parent.parent(),
                attempt: attempt.0,
            },
            debug::Captured {
                root: parent.parent(),
                attempt: attempt.0,
            },
            checkout.clone(),
            captured_settings,
            Running,
            OperationState::Running,
        ));
        if requested {
            step.insert(debug::Lease(lease.map(|lease| lease.0.clone())));
        }
        mailbox.wake.unpark();
        break;
    }
}

#[derive(SystemParam)]
#[cfg(feature = "development")]
struct StorageDispatcher<'w, 's> {
    loans: Query<'w, 's, &'static Lease>,
    commands: Commands<'w, 's>,
}

#[cfg(feature = "development")]
impl StorageDispatcher<'_, '_> {
    fn assign(&mut self, entity: Entity, started: &Started, request: storage_runtime::Request) {
        self.commands.entity(entity).insert((
            Assigned,
            request,
            storage_runtime::Captured {
                root: started.context,
                attempt: started.attempt,
            },
            storage_runtime::Loan(
                self.loans
                    .get(started.context)
                    .ok()
                    .map(|lease| lease.0.clone()),
            ),
        ));
    }
}

#[cfg(feature = "development")]
type StorageStepQuery<'w, 's, W> = Query<
    'w,
    's,
    (Entity, &'static W, &'static Started, &'static Settings),
    (With<Running>, Without<Assigned>, Without<NativeSubmitted>),
>;

#[cfg(feature = "development")]
fn dispatch_storage_check(
    requests: StorageStepQuery<StorageCheckStep>,
    mut dispatcher: StorageDispatcher,
) {
    for (entity, _, started, _) in &requests {
        dispatcher.assign(
            entity,
            started,
            storage_runtime::Request::new(storage_runtime::RequestKind::Check),
        );
    }
}

#[cfg(feature = "development")]
fn dispatch_storage_preflight(
    requests: StorageStepQuery<StoragePreflightStep>,
    mut dispatcher: StorageDispatcher,
) {
    for (entity, _, started, _) in &requests {
        dispatcher.assign(
            entity,
            started,
            storage_runtime::Request::new(storage_runtime::RequestKind::Preflight),
        );
    }
}

#[cfg(feature = "development")]
fn dispatch_storage_finish(
    requests: StorageStepQuery<StorageFinishStep>,
    mut dispatcher: StorageDispatcher,
) {
    for (entity, step, started, _) in &requests {
        dispatcher.assign(
            entity,
            started,
            storage_runtime::Request::finish(step.0.clone()),
        );
    }
}

#[cfg(feature = "development")]
fn dispatch_storage_status(
    requests: StorageStepQuery<StorageStatusStep>,
    mut dispatcher: StorageDispatcher,
) {
    for (entity, _, started, _) in &requests {
        dispatcher.assign(
            entity,
            started,
            storage_runtime::Request::new(storage_runtime::RequestKind::Report),
        );
    }
}

#[cfg(feature = "development")]
fn dispatch_storage_clean(
    requests: StorageStepQuery<StorageCleanStep>,
    mut dispatcher: StorageDispatcher,
) {
    for (entity, step, started, settings) in &requests {
        dispatcher.assign(
            entity,
            started,
            storage_runtime::Request::clean(
                step.0
                    || settings.0.get("VMUX_BUILD_STORAGE_APPLY")
                        == Some(std::ffi::OsStr::new("1")),
            ),
        );
    }
}

#[cfg(feature = "development")]
type WorkflowStepQuery<'w, 's, W> = Query<
    'w,
    's,
    (
        Entity,
        &'static W,
        &'static Started,
        &'static Checkout,
        &'static Settings,
    ),
    (With<Running>, Without<Assigned>, Without<NativeSubmitted>),
>;

#[cfg(feature = "development")]
fn dispatch_desktop_package(
    requests: WorkflowStepQuery<DesktopPackageStep>,
    mut commands: Commands,
) {
    for (entity, step, started, _, _) in &requests {
        commands.trigger(package::Requested {
            entity,
            context: started.context,
            attempt: started.attempt,
            input: package::Desktop {
                profile: step.0.clone(),
            },
        });
        commands.entity(entity).insert(Assigned);
    }
}

#[cfg(feature = "development")]
fn dispatch_cli_compile(requests: WorkflowStepQuery<CliCompileStep>, mut commands: Commands) {
    for (entity, _, started, _, _) in &requests {
        commands.trigger(cli_package::Requested {
            entity,
            context: started.context,
            attempt: started.attempt,
            input: cli_package::Compile,
        });
        commands.entity(entity).insert(Assigned);
    }
}

#[cfg(feature = "development")]
fn dispatch_cli_package(requests: WorkflowStepQuery<CliPackageStep>, mut commands: Commands) {
    for (entity, step, started, _, _) in &requests {
        commands.trigger(cli_package::Requested {
            entity,
            context: started.context,
            attempt: started.attempt,
            input: cli_package::Package { preview: step.0 },
        });
        commands.entity(entity).insert(Assigned);
    }
}

#[cfg(feature = "development")]
fn dispatch_reset_layout(requests: WorkflowStepQuery<ResetLayoutStep>, mut commands: Commands) {
    for (entity, _, started, _, _) in &requests {
        commands.trigger(reset::Requested {
            entity,
            context: started.context,
            attempt: started.attempt,
        });
        commands.entity(entity).insert(Assigned);
    }
}

#[cfg(feature = "development")]
fn dispatch_internal_package(requests: WorkflowStepQuery<InternalStep>, mut commands: Commands) {
    for (entity, step, started, checkout, settings) in &requests {
        match step.task.as_str() {
            "package-binaries" => commands.trigger(package::Requested {
                entity,
                context: started.context,
                attempt: started.attempt,
                input: package::Binaries,
            }),
            "before-package" | "finalize-package" => commands.trigger(package::Requested {
                entity,
                context: started.context,
                attempt: started.attempt,
                input: package::Before {
                    app: std::path::PathBuf::from(
                        settings
                            .0
                            .get("VMUX_APP_BUNDLE")
                            .unwrap_or(std::ffi::OsStr::new("")),
                    ),
                    format: settings
                        .0
                        .get("CARGO_PACKAGER_FORMAT")
                        .unwrap_or(std::ffi::OsStr::new(""))
                        .to_owned(),
                    force: step.task == "finalize-package",
                },
            }),
            "verify-bundle" => commands.trigger(package::Requested {
                entity,
                context: started.context,
                attempt: started.attempt,
                input: package::Verify {
                    app: step
                        .arguments
                        .first()
                        .map(std::path::PathBuf::from)
                        .unwrap_or_default(),
                },
            }),
            "verify-web-bundle" => commands.trigger(package::Requested {
                entity,
                context: started.context,
                attempt: started.attempt,
                input: package::Web {
                    directory: step
                        .arguments
                        .first()
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| checkout.root.join("crates/util/vmux_ui/dist")),
                },
            }),
            _ => continue,
        }
        commands.entity(entity).insert(Assigned);
    }
}

#[cfg(feature = "development")]
type NativeReady<'w, 's, W> = Query<
    'w,
    's,
    (
        Entity,
        &'static W,
        Option<&'static ChildOf>,
        &'static Attempt,
        &'static debug::Captured,
        &'static Checkout,
        &'static Settings,
    ),
    (With<Running>, Without<NativeSubmitted>, Without<Assigned>),
>;

#[derive(SystemParam)]
#[cfg(feature = "development")]
struct NativeRunner<'w, 's> {
    operations: Inputs<'w, 's>,
    mailbox: Res<'w, Mailbox>,
    commands: Commands<'w, 's>,
}

#[cfg(feature = "development")]
impl NativeRunner<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    fn spawn<F>(
        &mut self,
        entity: Entity,
        parent: Option<&ChildOf>,
        attempt: &Attempt,
        captured: &debug::Captured,
        checkout: &Checkout,
        settings: &Settings,
        name: &'static str,
        execute: F,
    ) -> bool
    where
        F: FnOnce(
                &Checkout,
                Environment,
                &mpsc::SyncSender<(Entity, ConfiguredToolPath)>,
                &mpsc::SyncSender<(Entity, ProducedArtifacts)>,
                Entity,
            ) -> Result<()>
            + Send
            + 'static,
    {
        if parent.is_none_or(|parent| parent.parent() != captured.root)
            || attempt.0 != captured.attempt
        {
            self.commands.entity(entity).despawn();
            if self
                .operations
                .get(captured.root)
                .is_ok_and(|(_, _, _, _, _, state)| *state == OperationState::Running)
            {
                self.commands
                    .entity(captured.root)
                    .remove::<Lease>()
                    .insert((
                        OperationState::Failed,
                        CliResult(Err("Development step ownership changed".into())),
                    ));
            }
            return false;
        }
        let Ok((_, _, _storage, lease, executable, _)) = self.operations.get(captured.root) else {
            return false;
        };
        let checkout = checkout.clone();
        let mut environment = settings.0.clone();
        if let Some(executable) = executable {
            environment.set("VMUX_DX_PROVIDER_EXECUTABLE", executable.path().as_os_str());
        }
        let lease = lease.map(|lease| lease.0.clone());
        let sender = self.mailbox.sender.clone();
        let tool_path_tx = self.mailbox.tool_path_tx.clone();
        let artifacts_tx = self.mailbox.artifacts_tx.clone();
        let wake = self.mailbox.wake.clone();
        let root = captured.root;
        let attempt = attempt.0;
        self.commands.entity(entity).insert(NativeSubmitted);
        thread::spawn(move || {
            let _lease = lease;
            let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                signals::Signals::check()?;
                execute(&checkout, environment, &tool_path_tx, &artifacts_tx, entity)
            })) {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(Failure::from_error(error).context(name)),
                Err(_) => Err(Failure::message("Development step panicked")),
            };
            let _ = sender.send(Completion {
                entity,
                context: root,
                attempt,
                result,
            });
            wake.unpark();
        });
        true
    }
}

#[cfg(feature = "development")]
fn run_seed(steps: NativeReady<SeedStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Seed target cache",
            |checkout, env, _, _, _| native::Native::new(checkout, env).seed(),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_native_tools(steps: NativeReady<NativeToolsStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Prepare native tools",
            |checkout, env, _, _, _| native::Native::new(checkout, env).ensure_native(),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_helper(steps: NativeReady<HelperStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Prepare CEF helper",
            |checkout, env, _, _, _| native::Native::new(checkout, env).helper(),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_package_tools(steps: NativeReady<PackageToolsStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Prepare packaging tools",
            |checkout, env, _, _, _| native::Native::new(checkout, env).package_tools(),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_dioxus(steps: NativeReady<DioxusStep>, mut runner: NativeRunner) {
    for (entity, step, parent, attempt, captured, checkout, settings) in &steps {
        let platform = step.0.clone();
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Prepare Dioxus tools",
            move |checkout, env, tool_paths, _, entity| {
                let mut native = native::Native::new(checkout, env);
                native.dioxus(&platform)?;
                let _ = tool_paths.send((
                    entity,
                    ConfiguredToolPath {
                        key: "DX_BIN",
                        path: native.tool_path("DX_BIN", "dx"),
                    },
                ));
                Ok(())
            },
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_binaryen(steps: NativeReady<BinaryenStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Prepare wasm optimizer",
            |checkout, env, _, _, _| native::Native::new(checkout, env).binaryen(),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_tailwind(steps: NativeReady<TailwindStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Prepare stylesheet compiler",
            |checkout, env, tool_paths, _, entity| {
                let mut native = native::Native::new(checkout, env);
                native.tailwind()?;
                let _ = tool_paths.send((
                    entity,
                    ConfiguredToolPath {
                        key: "TAILWINDCSS_BIN",
                        path: native.tool_path("TAILWINDCSS_BIN", "tailwindcss"),
                    },
                ));
                Ok(())
            },
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_resources_ios(steps: NativeReady<ResourcesIosStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Inject iOS resources",
            |checkout, env, _, _, _| native::Native::new(checkout, env).resources_ios(),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_release_ios(steps: NativeReady<ReleaseIosStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Sign and package iOS",
            |checkout, env, _, artifacts, entity| {
                native::Native::new(checkout, env).ios_release()?;
                let _ = artifacts.send((
                    entity,
                    ProducedArtifacts(vec![
                        checkout
                            .target
                            .join("dx/vmux_mobile/release/ios/VmuxMobile.app"),
                    ]),
                ));
                Ok(())
            },
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_verify_ios(steps: NativeReady<VerifyIosStep>, mut runner: NativeRunner) {
    for (entity, step, parent, attempt, captured, checkout, settings) in &steps {
        let signed = step.0;
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Verify iOS bundle",
            move |checkout, env, _, _, _| native::Native::new(checkout, env).verify_ios(signed),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_css(steps: NativeReady<CssStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Build stylesheet",
            |checkout, env, _, _, _| native::Native::new(checkout, env).css(),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_flatten_web(steps: NativeReady<FlattenWebStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Publish web artifact",
            |checkout, env, _, _, _| native::Native::new(checkout, env).flatten_web(),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_simulator(steps: NativeReady<SimulatorStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Prepare simulator",
            |checkout, env, _, _, _| native::Native::new(checkout, env).simulator(),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_doctor(steps: NativeReady<DoctorStep>, mut runner: NativeRunner) {
    for (entity, _, parent, attempt, captured, checkout, settings) in &steps {
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Inspect tools",
            |checkout, env, _, _, _| native::Native::new(checkout, env).doctor(),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_ci_scope(steps: NativeReady<ScopeStep>, mut runner: NativeRunner) {
    for (entity, step, parent, attempt, captured, checkout, settings) in &steps {
        let step = step.clone();
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Select CI scope",
            move |checkout, _, _, _, _| step.run(checkout),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_ci_native(steps: NativeReady<NativeInputsStep>, mut runner: NativeRunner) {
    for (entity, step, parent, attempt, captured, checkout, settings) in &steps {
        let step = step.clone();
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Classify native CI inputs",
            move |checkout, _, _, _, _| step.run(checkout),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_sdk_consumer(steps: NativeReady<ConsumerStep>, mut runner: NativeRunner) {
    for (entity, step, parent, attempt, captured, checkout, settings) in &steps {
        let step = step.clone();
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Validate public SDK",
            move |checkout, env, _, _, _| step.run(checkout, env),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn run_internal(steps: NativeReady<InternalStep>, mut runner: NativeRunner) {
    for (entity, step, parent, attempt, captured, checkout, settings) in &steps {
        let task = step.task.clone();
        let args = step.arguments.clone();
        if runner.spawn(
            entity,
            parent,
            attempt,
            captured,
            checkout,
            settings,
            "Run external integration",
            move |checkout, env, _, _, _| native::Native::new(checkout, env).internal(&task, &args),
        ) {
            break;
        }
    }
}

#[cfg(feature = "development")]
fn receive(mailbox: Res<Mailbox>, mut messages: MessageWriter<Completion>, mut commands: Commands) {
    let tool_paths = mailbox.tool_path_rx.lock().unwrap();
    while let Ok((entity, tool_path)) = tool_paths.try_recv() {
        commands.entity(entity).try_insert(tool_path);
    }
    drop(tool_paths);
    let artifacts = mailbox.artifacts_rx.lock().unwrap();
    while let Ok((entity, produced)) = artifacts.try_recv() {
        commands.entity(entity).try_insert(produced);
    }
    drop(artifacts);
    let receiver = mailbox.receiver.lock().unwrap();
    while let Ok(completion) = receiver.try_recv() {
        messages.write(completion);
    }
}

#[derive(SystemParam)]
#[cfg(feature = "development")]
struct Drains<'w, 's> {
    running: Query<'w, 's, &'static Started, With<Running>>,
    packages: package::Drains<'w, 's>,
    standalone: cli_package::Drains<'w, 's>,
    publications: publication::Drains<'w, 's>,
}

#[cfg(feature = "development")]
impl Drains<'_, '_> {
    fn pending(&self, context: Entity) -> bool {
        self.running
            .iter()
            .any(|started| started.context == context)
            || self.packages.pending(context)
            || self.standalone.pending(context)
            || self.publications.pending(context)
    }
}

#[cfg(feature = "development")]
fn cancel(
    signals: Option<Res<signals::Signals>>,
    drains: Drains,
    mut operations: Cancellations,
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
        if !drains.pending(entity) {
            commands
                .entity(entity)
                .insert(CliResult(Ok((128 + signal) as u8)));
        }
    }
}

#[derive(SystemParam)]
#[cfg(feature = "development")]
struct Replies<'w, 's> {
    commands: Commands<'w, 's>,
}

#[cfg(feature = "development")]
impl Replies<'_, '_> {
    fn send(&mut self, entity: Entity, result: CliResult) {
        self.commands.entity(entity).insert(result);
    }
}

#[cfg(feature = "development")]
fn collect(
    mut completions: MessageReader<Completion>,
    steps: Completions,
    mut operations: Query<(&mut Settings, &mut OperationState), With<Accepted>>,
    mut replies: Replies,
) {
    for completion in completions.read() {
        let Ok((parent, attempt, started)) = steps.get(completion.entity) else {
            continue;
        };
        if started.context != completion.context || started.attempt != completion.attempt {
            continue;
        };
        if attempt.is_none_or(|attempt| attempt.0 != completion.attempt)
            || parent.is_none_or(|parent| parent.parent() != completion.context)
        {
            replies.commands.entity(completion.entity).despawn();
            if let Ok((_, mut state)) = operations.get_mut(completion.context) {
                if *state == OperationState::Canceled {
                    replies.send(
                        completion.context,
                        CliResult(Ok((128 + signals::Signals::pending().unwrap_or(15)) as u8)),
                    );
                } else {
                    *state = OperationState::Failed;
                    replies.send(
                        completion.context,
                        CliResult(Err("Development step ownership changed".into())),
                    );
                }
            }
            continue;
        }
        let Ok((_, state)) = operations.get_mut(completion.context) else {
            continue;
        };
        if *state == OperationState::Canceled {
            replies
                .commands
                .entity(completion.entity)
                .remove::<(Running, Lease)>()
                .insert((Completed, OperationState::Canceled));
            continue;
        }
        if *state != OperationState::Running {
            replies
                .commands
                .entity(completion.entity)
                .remove::<(Running, Lease)>()
                .insert((Completed, OperationState::Failed));
            replies.send(
                completion.context,
                CliResult(Err("Development operation is no longer running".into())),
            );
            continue;
        }
        replies
            .commands
            .entity(completion.entity)
            .insert(Receipt(completion.result.clone()));
    }
}

#[derive(Component)]
#[component(immutable)]
#[cfg(feature = "development")]
struct Receipt(std::result::Result<(), Failure>);

#[cfg(feature = "development")]
type Applying<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Started,
        Option<&'static ChildOf>,
        Option<&'static Attempt>,
        &'static Receipt,
    ),
>;

#[cfg(feature = "development")]
type StepOutputs<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static ResolvedExecutable>,
        Option<&'static ResolvedIdentity>,
        Option<&'static ProducedArtifacts>,
        Option<&'static ConfiguredToolPath>,
    ),
>;

#[cfg(feature = "development")]
fn apply(
    receipts: Applying,
    outputs: StepOutputs,
    mut operations: Query<(&mut Settings, &mut OperationState), With<Accepted>>,
    leases: Query<&Lease>,
    mut replies: Replies,
) {
    for (entity, started, parent, attempt, receipt) in &receipts {
        let Ok((mut settings, mut state)) = operations.get_mut(started.context) else {
            continue;
        };
        if parent.is_none_or(|parent| parent.parent() != started.context)
            || attempt.is_none_or(|attempt| attempt.0 != started.attempt)
        {
            replies.commands.entity(entity).despawn();
            *state = OperationState::Failed;
            replies.send(
                started.context,
                CliResult(Err("Development step ownership changed".into())),
            );
            continue;
        }
        if *state != OperationState::Running {
            replies
                .commands
                .entity(entity)
                .remove::<(Running, Lease, Receipt)>()
                .insert((Completed, state.clone()));
            continue;
        }
        let parent = parent.unwrap();
        match &receipt.0 {
            Ok(()) => {
                if let Ok(lease) = leases.get(entity) {
                    replies
                        .commands
                        .entity(started.context)
                        .insert(Lease(lease.0.clone()));
                    replies.commands.entity(entity).remove::<Lease>();
                }
                if let Ok((executable, identity, artifacts, tool_path)) = outputs.get(entity) {
                    if let Some(executable) = executable {
                        replies
                            .commands
                            .entity(parent.parent())
                            .insert(executable.0.clone())
                            .remove::<Lease>();
                    }
                    if let Some(identity) = identity {
                        settings.0.set("APPLE_SIGNING_IDENTITY", &identity.0);
                    }
                    if let Some(artifacts) = artifacts {
                        for path in &artifacts.0 {
                            replies.commands.spawn((
                                Name::new(path.display().to_string()),
                                Artifact(path.clone()),
                                CreatedAt::now(),
                                vmux_ecs::Ready,
                                Completed,
                                ChildOf(parent.parent()),
                            ));
                        }
                    }
                    if let Some(tool_path) = tool_path {
                        settings.0.set(tool_path.key, &tool_path.path);
                    }
                }
                replies
                    .commands
                    .entity(entity)
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
                replies
                    .commands
                    .entity(entity)
                    .remove::<Running>()
                    .insert((Completed, OperationState::Failed));
                replies.send(parent.parent(), CliResult(Ok(error.code() as u8)));
            }
        }
        replies.commands.entity(entity).remove::<Receipt>();
    }
}

#[cfg(feature = "development")]
fn retire(
    requests: Query<(Entity, &Retirement, &OperationState), Without<CliResult>>,
    drains: Drains,
    mut commands: Commands,
) {
    for (entity, failure, state) in &requests {
        if drains.pending(entity) {
            continue;
        };
        let result = if *state == OperationState::Canceled {
            CliResult(Ok((128 + signals::Signals::pending().unwrap_or(15)) as u8))
        } else {
            CliResult(Err(failure.0.to_string()))
        };
        commands.entity(entity).insert(result);
    }
}

#[cfg(feature = "development")]
fn finish(
    operations: Finishing,
    drains: Drains,
    origins: Query<&Started>,
    completed: Query<Entity, With<Completed>>,
    artifacts: Query<&Artifact, With<vmux_ecs::Ready>>,
    mut commands: Commands,
) {
    for (entity, children, state) in &operations {
        if *state == OperationState::Canceled || drains.pending(entity) {
            continue;
        }
        let mut ready = true;
        for child in children.iter() {
            if !completed.contains(child)
                || origins
                    .get(child)
                    .is_ok_and(|started| started.context != entity)
            {
                ready = false;
                break;
            }
        }
        if ready {
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
#[cfg(feature = "development")]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        operation: Entity,
        previous: Option<Entity>,
        _permit: crate::testing::Permit,
    }

    impl Fixture {
        fn new() -> Self {
            let permit = crate::testing::Permit::acquire();
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().canonicalize().unwrap();
            let target = root.join("target");
            let checkout = Checkout {
                root: root.clone(),
                target: target.clone(),
                build_target: None,
            };
            let storage = Storage::new(policy::Policy::environment().unwrap());
            let mut app = App::new();
            app.add_plugins(DxPlugin);
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
                _permit: permit,
                app,
                directory,
                operation,
                previous: None,
            }
        }

        fn step(&mut self, command: &str) -> Entity {
            let work = debug::Tool {
                program: "/bin/sh".into(),
                arguments: vec!["-c".into(), command.into()],
            };
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

        fn program(&self, name: &str, script: &str) -> std::path::PathBuf {
            let directory = self.directory.path().join("tools");
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        fn cargo(&mut self, arguments: &[&str]) -> Entity {
            let program = self.program("cargo", r#"
if [ "$1" = '-V' ]; then
    printf 'cargo fixture 1\n'
    exit 0
fi
printf '%s\n' "$@" > "$VMUX_CARGO_FIXTURE/arguments"
printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s' "$CARGO_INCREMENTAL" "$CEF_PATH" "$CMAKE_C_COMPILER_LAUNCHER" "$CMAKE_CXX_COMPILER_LAUNCHER" "$RUSTC_WRAPPER" "$TAILWINDCSS" "$VMUX_TARGET_LOCK_TARGET" "$VMUX_BUILD_STORAGE_ROOT" "$VMUX_TARGET_LOCK_OWNER_PID" "$VMUX_DX_BIN" > "$VMUX_CARGO_FIXTURE/environment"
exit "${VMUX_CARGO_FIXTURE_EXIT:-0}"
"#);
            let rustc = self.program("rustc", "printf 'rustc fixture 1\\nhost: fixture-host\\n'");
            let root = self.directory.path().canonicalize().unwrap();
            let mut settings = self
                .app
                .world_mut()
                .get_mut::<Settings>(self.operation)
                .unwrap();
            for name in [
                "RUSTC_WRAPPER",
                "RUSTC_WORKSPACE_WRAPPER",
                "CMAKE_C_COMPILER_LAUNCHER",
                "CMAKE_CXX_COMPILER_LAUNCHER",
                "CARGO_INCREMENTAL",
                "RUSTFLAGS",
                "CARGO_ENCODED_RUSTFLAGS",
                "CARGO_BUILD_TARGET",
                "MACOSX_DEPLOYMENT_TARGET",
                "VMUX_DX_PLATFORM",
                "VMUX_CARGO_FIXTURE_EXIT",
            ] {
                settings.0.remove(name);
            }
            settings.0.set("CARGO_BIN", program.as_os_str());
            settings.0.set("RUSTC", rustc.as_os_str());
            settings.0.set(
                "PATH",
                format!("{}:/usr/bin:/bin", root.join("tools").display()),
            );
            settings.0.set("VMUX_CARGO_FIXTURE", root.as_os_str());
            settings
                .0
                .set("VMUX_CEF_SDK_CACHE", root.join("cef-sdk").as_os_str());
            settings.0.set("VMUX_DISABLE_SCCACHE", "1");
            settings.0.set("VMUX_BUILD_PROFILE", "dev");
            settings.0.set("CI", "true");
            self.app
                .world_mut()
                .spawn((
                    cargo::Run::new(arguments),
                    OperationState::Pending,
                    ChildOf(self.operation),
                    Attempt(1),
                ))
                .id()
        }
    }

    #[test]
    fn typed_cargo_preserves_arguments_environment_overrides_and_probe_fallbacks() {
        for probe in [
            None,
            Some("exit 7"),
            Some("[ \"$1\" = '--version' ]"),
            Some("printf '\\377'; exit 0"),
        ] {
            let mut fixture = Fixture::new();
            let step = fixture.cargo(&["fmt", "--", "space in argument"]);
            if let Some(probe) = probe {
                fixture.program("sccache", probe);
            }
            let mut settings = fixture
                .app
                .world_mut()
                .get_mut::<Settings>(fixture.operation)
                .unwrap();
            settings.0.set("VMUX_DISABLE_SCCACHE", "0");
            settings.0.set("CI", "false");
            settings.0.set("CARGO_INCREMENTAL", "custom");
            settings.0.set("CMAKE_C_COMPILER_LAUNCHER", "custom-c");
            settings.0.set("TAILWINDCSS_BIN", "selected-tailwind");
            assert_eq!(fixture.result(), 0);
            assert_eq!(
                std::fs::read_to_string(fixture.directory.path().join("arguments")).unwrap(),
                "fmt\n--\nspace in argument\n"
            );
            let environment =
                std::fs::read_to_string(fixture.directory.path().join("environment")).unwrap();
            let values: Vec<_> = environment.split('|').collect();
            assert_eq!(values[0], "custom");
            assert!(values[1].ends_with("/cef-sdk"));
            assert_eq!(values[2], "custom-c");
            assert_eq!(
                values[3],
                if probe == Some("printf '\\377'; exit 0") {
                    "sccache"
                } else {
                    ""
                }
            );
            assert_eq!(
                values[4],
                if probe == Some("printf '\\377'; exit 0") {
                    "sccache"
                } else {
                    ""
                }
            );
            assert_eq!(values[5], "selected-tailwind");
            assert_eq!(
                std::path::Path::new(values[6]),
                fixture
                    .directory
                    .path()
                    .canonicalize()
                    .unwrap()
                    .join("target")
            );
            assert_eq!(
                std::path::Path::new(values[7]),
                fixture.directory.path().canonicalize().unwrap()
            );
            assert_eq!(values[8], std::process::id().to_string());
            assert_eq!(
                std::path::Path::new(values[9]),
                std::env::current_exe().unwrap()
            );
            assert_eq!(
                fixture
                    .app
                    .world()
                    .get::<Settings>(step)
                    .unwrap()
                    .0
                    .get("CARGO_INCREMENTAL"),
                Some(std::ffi::OsStr::new("custom"))
            );
        }
    }

    #[test]
    fn typed_cargo_records_the_existing_git_seed_contract_and_replaces_old_markers() {
        for original in [None, Some(""), Some("0")] {
            let mut fixture = Fixture::new();
            let step = fixture.cargo(&["build", "--package", "fixture"]);
            let root = fixture.directory.path().canonicalize().unwrap();
            std::fs::create_dir_all(root.join(".cargo")).unwrap();
            std::fs::create_dir_all(root.join("nested")).unwrap();
            std::fs::create_dir_all(root.join("target/.vmux-seed")).unwrap();
            std::fs::write(root.join("target/.vmux-seed/obsolete"), "").unwrap();
            let files = [
                ".cargo/config.toml",
                "Cargo.lock",
                "Cargo.toml",
                "nested/Cargo.toml",
            ];
            for file in files {
                std::fs::write(root.join(file), format!("fixture: {file}\n")).unwrap();
            }
            let mut settings = fixture
                .app
                .world_mut()
                .get_mut::<Settings>(fixture.operation)
                .unwrap();
            settings.0.set("CI", "false");
            settings.0.set("RUSTFLAGS", "-C debuginfo=0");
            if let Some(original) = original {
                settings.0.set("CARGO_INCREMENTAL", original);
            }
            let incremental = if original == Some("0") { "0" } else { "1" };
            let mut input = format!(
                "cargo=cargo fixture 1\nrustc=rustc fixture 1\nhost: fixture-host\nrustc-wrapper=\nrustc-workspace-wrapper=\ncargo-incremental={incremental}\nrustflags=-C debuginfo=0\ncargo-encoded-rustflags=\ncargo-build-target=\nmacosx-deployment-target=\n"
            );
            for file in files {
                input.push_str(&format!("file={file}\n"));
            }
            let output = std::process::Command::new("/usr/bin/git")
                .current_dir(&root)
                .arg("hash-object")
                .args(files)
                .output()
                .unwrap();
            assert!(output.status.success());
            input.push_str(std::str::from_utf8(&output.stdout).unwrap());
            std::fs::write(root.join("seed-input"), input).unwrap();
            let output = std::process::Command::new("/usr/bin/git")
                .current_dir(&root)
                .args(["hash-object", "seed-input"])
                .output()
                .unwrap();
            assert!(output.status.success());
            let expected = String::from_utf8(output.stdout).unwrap();
            assert_eq!(fixture.result(), 0);
            let markers: Vec<_> = std::fs::read_dir(root.join("target/.vmux-seed"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            assert_eq!(markers, vec![OsString::from(expected.trim())]);
            assert_eq!(
                fixture
                    .app
                    .world()
                    .get::<Settings>(step)
                    .unwrap()
                    .0
                    .get("CARGO_INCREMENTAL"),
                original.map(std::ffi::OsStr::new)
            );
            assert_eq!(
                fixture
                    .app
                    .world()
                    .get::<Settings>(fixture.operation)
                    .unwrap()
                    .0
                    .get("CARGO_INCREMENTAL"),
                original.map(std::ffi::OsStr::new)
            );
        }
    }

    #[test]
    fn typed_cargo_failure_preserves_exit_code_and_skips_seed_publication() {
        let mut fixture = Fixture::new();
        let step = fixture.cargo(&["build"]);
        let mut settings = fixture
            .app
            .world_mut()
            .get_mut::<Settings>(fixture.operation)
            .unwrap();
        settings.0.set("CI", "false");
        settings.0.set("VMUX_CARGO_FIXTURE_EXIT", "37");
        assert_eq!(fixture.result(), 37);
        assert!(!fixture.directory.path().join("target/.vmux-seed").exists());
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Settings>(step)
                .unwrap()
                .0
                .get("CARGO_INCREMENTAL"),
            None
        );
    }

    #[test]
    fn typed_cargo_seed_failure_preserves_the_marker_and_reports_failure_after_the_tool_succeeds() {
        let mut fixture = Fixture::new();
        fixture.cargo(&["build"]);
        fixture
            .app
            .world_mut()
            .get_mut::<Settings>(fixture.operation)
            .unwrap()
            .0
            .set("CI", "false");
        let target = fixture.directory.path().join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join(".vmux-seed"), "protected marker").unwrap();
        assert_eq!(fixture.result(), 1);
        assert_eq!(
            std::fs::read_to_string(fixture.directory.path().join("arguments")).unwrap(),
            "build\n"
        );
        assert_eq!(
            std::fs::read_to_string(target.join(".vmux-seed")).unwrap(),
            "protected marker"
        );
    }

    #[test]
    fn typed_cargo_in_ci_clears_inherited_cef_and_preserves_the_selected_wrapper() {
        let mut fixture = Fixture::new();
        fixture.cargo(&["build"]);
        fixture.program("sccache", "exit 0");
        let mut settings = fixture
            .app
            .world_mut()
            .get_mut::<Settings>(fixture.operation)
            .unwrap();
        settings.0.set("VMUX_DISABLE_SCCACHE", "0");
        settings.0.remove("VMUX_CEF_SDK_CACHE");
        settings.0.set("CEF_PATH", "inherited-sdk");
        settings.0.set("RUSTC_WRAPPER", "selected-wrapper");
        assert_eq!(fixture.result(), 0);
        let environment =
            std::fs::read_to_string(fixture.directory.path().join("environment")).unwrap();
        let values: Vec<_> = environment.split('|').collect();
        assert_eq!(values[0], "0");
        assert_eq!(values[1], "");
        assert_eq!(values[2], "sccache");
        assert_eq!(values[3], "sccache");
        assert_eq!(values[4], "selected-wrapper");
        assert!(!fixture.directory.path().join("target/.vmux-seed").exists());
    }

    #[test]
    fn canceled_cargo_probe_drains_its_process_and_retains_the_target_until_completion() {
        let mut fixture = Fixture::new();
        let step = fixture.cargo(&["build"]);
        fixture.program(
            "sccache",
            "printf '%s' \"$$\" > \"$VMUX_CARGO_FIXTURE/probe-pid\"; exec /bin/sleep 30",
        );
        fixture
            .app
            .world_mut()
            .get_mut::<Settings>(fixture.operation)
            .unwrap()
            .0
            .set("VMUX_DISABLE_SCCACHE", "0");
        let checkout = fixture
            .app
            .world()
            .get::<Checkout>(fixture.operation)
            .unwrap()
            .clone();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.operation)
            .insert(Lease(Arc::new(
                TargetLock::acquire(&checkout.root, &checkout.target).unwrap(),
            )));
        let deadline = Instant::now() + Duration::from_secs(5);
        let pid = loop {
            fixture.app.update();
            if let Ok(text) = std::fs::read_to_string(fixture.directory.path().join("probe-pid"))
                && let Ok(pid) = text.parse::<u32>()
            {
                break pid;
            }
            assert!(Instant::now() < deadline, "Cargo probe did not start");
            std::thread::park_timeout(Duration::from_millis(5));
        };
        assert!(signals::Signals::alive(pid));
        assert!(
            TargetLock::try_acquire(&checkout.root, &checkout.target)
                .unwrap()
                .is_none()
        );
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.operation)
            .insert(OperationState::Canceled);
        loop {
            fixture.app.update();
            if fixture.app.world().get::<Completed>(step).is_some() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Canceled Cargo probe did not drain"
            );
            std::thread::park_timeout(Duration::from_millis(5));
        }
        assert!(!signals::Signals::alive(pid));
        assert!(!fixture.directory.path().join("arguments").exists());
        assert!(!fixture.directory.path().join("target/.vmux-seed").exists());
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Settings>(step)
                .unwrap()
                .0
                .get("CARGO_INCREMENTAL"),
            None
        );
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.operation)
            .remove::<Lease>();
        assert!(
            TargetLock::try_acquire(&checkout.root, &checkout.target)
                .unwrap()
                .is_some()
        );
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
        fixture
            .app
            .world_mut()
            .entity_mut(step)
            .remove::<debug::Tool>();
        fixture.app.world_mut().entity_mut(step).insert((
            Running,
            Started {
                context: fixture.operation,
                attempt: 1,
            },
            NativeSubmitted,
            debug::Captured {
                root: fixture.operation,
                attempt: 1,
            },
        ));
        fixture.app.world_mut().write_message(Completion {
            entity: step,
            context: fixture.operation,
            attempt: 0,
            result: Ok(()),
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
            context: fixture.operation,
            attempt: 1,
            result: Ok(()),
        });
        assert_eq!(fixture.result(), 0);
    }

    #[test]
    fn native_receipts_cannot_cross_their_captured_operation_when_a_step_is_reparented() {
        for detached in [false, true] {
            let mut fixture = Fixture::new();
            fixture.app.world_mut().spawn((
                Completed,
                OperationState::Succeeded,
                ChildOf(fixture.operation),
            ));
            let step = fixture.step("exit 99");
            fixture
                .app
                .world_mut()
                .entity_mut(step)
                .remove::<debug::Tool>();
            fixture.app.world_mut().entity_mut(step).insert((
                Running,
                Started {
                    context: fixture.operation,
                    attempt: 1,
                },
                NativeSubmitted,
                debug::Captured {
                    root: fixture.operation,
                    attempt: 1,
                },
            ));
            let mut environment = Environment::current();
            environment.remove("VMUX_DX_OWNER_ORACLE");
            let other = fixture
                .app
                .world_mut()
                .spawn((
                    Accepted,
                    Operation::bundle("dx.fixture").unwrap().running(),
                    Settings(environment),
                ))
                .id();
            if detached {
                fixture.app.world_mut().entity_mut(step).remove::<ChildOf>();
            } else {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(step)
                    .insert(ChildOf(other));
            }
            fixture.app.update();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<CliResult>(fixture.operation)
                    .is_none()
            );
            fixture
                .app
                .world_mut()
                .entity_mut(step)
                .insert(ConfiguredToolPath {
                    key: "VMUX_DX_OWNER_ORACLE",
                    path: "wrong-owner".into(),
                });
            fixture.app.world_mut().write_message(Completion {
                entity: step,
                context: fixture.operation,
                attempt: 1,
                result: Ok(()),
            });
            fixture.app.update();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Settings>(other)
                    .unwrap()
                    .0
                    .get("VMUX_DX_OWNER_ORACLE")
                    .is_none()
            );
            assert!(fixture.app.world().get_entity(step).is_err());
            assert_eq!(
                fixture.app.world().get::<OperationState>(fixture.operation),
                Some(&OperationState::Failed)
            );
            let checkout = fixture
                .app
                .world()
                .get::<Checkout>(fixture.operation)
                .unwrap()
                .clone();
            let settings = fixture
                .app
                .world()
                .get::<Settings>(fixture.operation)
                .unwrap()
                .clone();
            let storage = fixture
                .app
                .world()
                .get::<Storage>(fixture.operation)
                .unwrap()
                .clone();
            fixture.operation = fixture
                .app
                .world_mut()
                .spawn((
                    Accepted,
                    Operation::bundle("dx.fixture").unwrap().running(),
                    checkout,
                    settings,
                    storage,
                ))
                .id();
            fixture.previous = None;
            fixture.step("printf recovered > recovered");
            assert_eq!(fixture.result(), 0);
            assert_eq!(
                std::fs::read_to_string(fixture.directory.path().join("recovered")).unwrap(),
                "recovered"
            );
        }
    }

    #[test]
    fn acquired_lease_transfers_to_the_root_without_losing_exclusion() {
        let mut fixture = Fixture::new();
        fixture.app.world_mut().spawn((
            debug::Acquire { inherit: false },
            OperationState::Pending,
            ChildOf(fixture.operation),
            Attempt(1),
        ));
        assert_eq!(fixture.result(), 0);
        let checkout = fixture
            .app
            .world()
            .get::<Checkout>(fixture.operation)
            .unwrap()
            .clone();
        assert!(
            TargetLock::try_acquire(&checkout.root, &checkout.target)
                .unwrap()
                .is_none()
        );
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.operation)
            .remove::<Lease>();
        assert!(
            TargetLock::try_acquire(&checkout.root, &checkout.target)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn discarded_acquired_lease_releases_exclusion_after_the_request_drains() {
        for disposition in [
            OperationState::Canceled,
            OperationState::Failed,
            OperationState::Running,
        ] {
            let mut fixture = Fixture::new();
            let step = fixture
                .app
                .world_mut()
                .spawn((
                    debug::Acquire { inherit: false },
                    OperationState::Pending,
                    ChildOf(fixture.operation),
                    Attempt(1),
                ))
                .id();
            let checkout = fixture
                .app
                .world()
                .get::<Checkout>(fixture.operation)
                .unwrap()
                .clone();
            let deadline = Instant::now() + Duration::from_secs(5);
            while fixture.app.world().get::<Lease>(step).is_none() {
                fixture.app.update();
                assert!(Instant::now() < deadline, "lease candidate did not arrive");
                std::thread::park_timeout(Duration::from_millis(5));
            }
            if disposition == OperationState::Running {
                let other = fixture.app.world_mut().spawn_empty().id();
                fixture
                    .app
                    .world_mut()
                    .entity_mut(step)
                    .insert(ChildOf(other));
            } else {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(fixture.operation)
                    .insert(disposition);
            }
            loop {
                fixture.app.update();
                if fixture.app.world().get::<Completed>(step).is_some()
                    || fixture.app.world().get_entity(step).is_err()
                {
                    break;
                }
                assert!(Instant::now() < deadline, "discarded lease did not drain");
                std::thread::park_timeout(Duration::from_millis(5));
            }
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Lease>(fixture.operation)
                    .is_none()
            );
            assert!(
                TargetLock::try_acquire(&checkout.root, &checkout.target)
                    .unwrap()
                    .is_some()
            );
        }
    }

    #[test]
    fn discarded_unconsumed_lock_receipts_release_exclusion_after_completion() {
        for disposition in [OperationState::Canceled, OperationState::Failed] {
            let mut fixture = Fixture::new();
            fixture.app.add_observer(
                move |receipt: On<Add, debug_io::AcquiredLock>,
                      parents: Query<&ChildOf>,
                      mut roots: Query<&mut OperationState, With<Accepted>>| {
                    let request = parents.get(receipt.entity).unwrap().parent();
                    let root = parents.get(request).unwrap().parent();
                    *roots.get_mut(root).unwrap() = disposition.clone();
                },
            );
            let step = fixture
                .app
                .world_mut()
                .spawn((
                    debug::Acquire { inherit: false },
                    OperationState::Pending,
                    ChildOf(fixture.operation),
                    Attempt(1),
                ))
                .id();
            let checkout = fixture
                .app
                .world()
                .get::<Checkout>(fixture.operation)
                .unwrap()
                .clone();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                fixture.app.update();
                if fixture.app.world().get::<Completed>(step).is_some() {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "unconsumed lock did not complete"
                );
                std::thread::park_timeout(Duration::from_millis(5));
            }
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Lease>(fixture.operation)
                    .is_none()
            );
            assert!(
                TargetLock::try_acquire(&checkout.root, &checkout.target)
                    .unwrap()
                    .is_some()
            );
        }
    }

    #[test]
    fn lock_inheritance_uses_the_process_contract_and_rejects_configuration_overrides() {
        let mut fixture = Fixture::new();
        if std::env::var_os("VMUX_LOCK_BORROWER_TEST").is_some() {
            let root =
                std::path::PathBuf::from(std::env::var_os("VMUX_BUILD_STORAGE_ROOT").unwrap());
            let target =
                std::path::PathBuf::from(std::env::var_os("VMUX_TARGET_LOCK_TARGET").unwrap());
            let storage = Storage::new(policy::Policy::environment().unwrap());
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.operation)
                .insert((
                    Checkout {
                        root,
                        target,
                        build_target: None,
                    },
                    storage,
                ));
            fixture.app.world_mut().spawn((
                debug::Acquire { inherit: true },
                OperationState::Pending,
                ChildOf(fixture.operation),
                Attempt(1),
            ));
            assert_eq!(fixture.result(), 0);
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Lease>(fixture.operation)
                    .is_none()
            );
            return;
        }
        let checkout = fixture
            .app
            .world()
            .get::<Checkout>(fixture.operation)
            .unwrap()
            .clone();
        let held = TargetLock::acquire(&checkout.root, &checkout.target).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::lock_inheritance_uses_the_process_contract_and_rejects_configuration_overrides",
                "--nocapture",
            ])
            .env("VMUX_LOCK_BORROWER_TEST", "1")
            .env("VMUX_TARGET_LOCK_OWNER_PID", std::process::id().to_string())
            .env("VMUX_BUILD_STORAGE_ROOT", &checkout.root)
            .env("VMUX_TARGET_LOCK_TARGET", &checkout.target)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        assert!(
            TargetLock::try_acquire(&checkout.root, &checkout.target)
                .unwrap()
                .is_none()
        );
        let mut settings = fixture
            .app
            .world_mut()
            .get_mut::<Settings>(fixture.operation)
            .unwrap();
        settings.0.set(
            "VMUX_TARGET_LOCK_OWNER_PID",
            unsafe { libc::getppid() }.to_string(),
        );
        settings
            .0
            .set("VMUX_BUILD_STORAGE_ROOT", checkout.root.as_os_str());
        settings
            .0
            .set("VMUX_TARGET_LOCK_TARGET", checkout.target.as_os_str());
        fixture.app.world_mut().spawn((
            debug::Acquire { inherit: true },
            OperationState::Pending,
            ChildOf(fixture.operation),
            Attempt(1),
        ));
        for _ in 0..4 {
            fixture.app.update();
        }
        assert!(
            fixture
                .app
                .world()
                .get::<CliResult>(fixture.operation)
                .is_none()
        );
        drop(held);
        assert_eq!(fixture.result(), 0);
    }

    #[test]
    fn typed_tool_uses_the_selected_program_and_website_working_directory() {
        let mut fixture = Fixture::new();
        let root = fixture.directory.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("website")).unwrap();
        let mut settings = fixture
            .app
            .world_mut()
            .get_mut::<Settings>(fixture.operation)
            .unwrap();
        settings.0.set("DX_BIN", "/bin/sh");
        settings.0.set("VMUX_TOOL_PROBE_ROOT", root.as_os_str());
        fixture.app.world_mut().spawn((
            debug::Tool {
                program: "dx".into(),
                arguments: vec![
                    "-c".into(),
                    "pwd > \"$VMUX_TOOL_PROBE_ROOT/selected-cwd\"".into(),
                    "--platform".into(),
                    "web".into(),
                ],
            },
            OperationState::Pending,
            ChildOf(fixture.operation),
            Attempt(1),
        ));
        assert_eq!(fixture.result(), 0);
        assert_eq!(
            std::fs::read_to_string(root.join("selected-cwd"))
                .unwrap()
                .trim(),
            root.join("website").display().to_string()
        );
    }
    #[test]
    fn reparented_completion_cannot_mutate_an_unrelated_command() {
        let mut fixture = Fixture::new();
        let step = fixture.step("exit 99");
        let mut environment = Environment::current();
        environment.remove("APPLE_SIGNING_IDENTITY");
        let unrelated = fixture
            .app
            .world_mut()
            .spawn((
                Accepted,
                Operation::bundle("dx.unrelated").unwrap().running(),
                Settings(environment),
            ))
            .id();
        fixture.app.world_mut().entity_mut(step).insert((
            Running,
            Started {
                context: fixture.operation,
                attempt: 1,
            },
            ChildOf(unrelated),
        ));
        fixture
            .app
            .world_mut()
            .entity_mut(step)
            .insert(ResolvedIdentity("foreign identity".into()));
        fixture.app.world_mut().write_message(Completion {
            entity: step,
            context: fixture.operation,
            attempt: 1,
            result: Ok(()),
        });
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<Settings>(unrelated)
                .unwrap()
                .0
                .get("APPLE_SIGNING_IDENTITY")
                .is_none()
        );
        assert!(fixture.app.world().get::<CliResult>(unrelated).is_none());
        assert!(matches!(
            fixture.app.world().get::<CliResult>(fixture.operation),
            Some(CliResult(Err(_)))
        ));
    }

    #[test]
    fn canceled_admission_reports_cancellation_when_its_storage_receipt_arrives() {
        let mut fixture = Fixture::new();
        let context = fixture
            .app
            .world_mut()
            .spawn((Admitting, OperationState::Canceled))
            .id();
        let guard = fixture
            .app
            .world_mut()
            .spawn((
                Step,
                Admission,
                Started {
                    context,
                    attempt: 1,
                },
                Attempt(1),
                Cwd(fixture.directory.path().to_owned()),
                Running,
                OperationState::Running,
                ChildOf(context),
            ))
            .id();
        fixture.app.world_mut().write_message(Completion {
            entity: guard,
            context,
            attempt: 1,
            result: Ok(()),
        });
        fixture.app.update();
        assert!(matches!(
            fixture.app.world().get::<CliResult>(context),
            Some(CliResult(Ok(143)))
        ));
    }

    #[test]
    fn detached_or_changed_admission_reports_to_its_original_unaccepted_command() {
        for (detach, state) in [
            (true, OperationState::Running),
            (false, OperationState::Running),
            (true, OperationState::Canceled),
            (false, OperationState::Canceled),
        ] {
            let mut fixture = Fixture::new();
            let context = fixture.app.world_mut().spawn((Admitting, state)).id();
            let guard = fixture
                .app
                .world_mut()
                .spawn((
                    Step,
                    Admission,
                    Started {
                        context,
                        attempt: 1,
                    },
                    Attempt(1),
                    Cwd(fixture.directory.path().to_owned()),
                    Running,
                    OperationState::Running,
                    ChildOf(context),
                ))
                .id();
            if detach {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(guard)
                    .remove::<ChildOf>();
            } else {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(guard)
                    .remove::<Attempt>();
            }
            fixture.app.world_mut().write_message(Completion {
                entity: guard,
                context,
                attempt: 1,
                result: Err(Failure::message("native admission failed")),
            });
            fixture.app.update();
            assert!(matches!(
                fixture.app.world().get::<CliResult>(context),
                Some(CliResult(Err(_)))
            ));
            assert!(fixture.app.world().get::<Admitting>(context).is_none());
            assert!(fixture.app.world().get_entity(guard).is_err());
        }
    }
}
