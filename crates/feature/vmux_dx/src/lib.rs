use std::ffi::OsString;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
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
mod debug;
mod debug_io;
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
        if !app
            .world()
            .contains_resource::<vmux_ecs::blocking::BlockingRuntime>()
        {
            app.insert_resource(vmux_ecs::blocking::BlockingRuntime::new(
                vmux_ecs::blocking::BlockingPool::new("vmux-dx-short", 0, 0),
                vmux_ecs::blocking::BlockingPool::new("vmux-dx-long", 0, 0),
                vmux_ecs::blocking::BlockingPool::new("vmux-dx-process", 1, 16),
            ));
        }
        app.add_plugins((
            FeaturePlugin::<Feature>::default(),
            updates::Plugin,
            workspace::Plugin,
            repository_selection::Plugin,
            debug::Plugin,
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
                (
                    accept,
                    load_settings,
                    configure_target,
                    ensure_tooling,
                    configure_settings,
                    prepare_storage,
                    admit,
                )
                    .chain(),
                cancel,
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
                        plan_internal,
                    )
                        .chain(),
                )
                    .chain(),
                refuse_plan,
                receive,
                collect,
                select_local,
                start,
                native,
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
#[require(Tail)]
struct Accepted;

#[derive(Component, Default)]
struct Tail(Option<Entity>);

#[derive(Component)]
#[require(AdmissionPhase)]
struct Admission;

#[derive(Component, Default, PartialEq, Eq)]
enum AdmissionPhase {
    #[default]
    Load,
    Target,
    Tooling,
    Configure,
    Storage,
    Ready,
}
#[derive(Component, Default)]
pub(crate) struct Step;

#[derive(Component)]
struct NativeSubmitted(debug::Captured);

#[derive(Component, Clone)]
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
    root: Entity,
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
    (Without<Admission>, Without<Accepted>, Without<CliResult>),
>;
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
    (With<Planned>, Without<CliResult>),
>;
type Cancellations<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut OperationState),
    (Or<(With<Accepted>, With<Admission>)>, Without<CliResult>),
>;
type Finishing<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Children, &'static OperationState),
    (With<Accepted>, With<Planned>, Without<CliResult>),
>;
type Preparing<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        Option<&'static workspace::Dispatch>,
        &'static mut Checkout,
        &'static mut Settings,
        &'static mut AdmissionPhase,
        &'static OperationState,
    ),
    (With<Admission>, Without<Accepted>, Without<CliResult>),
>;
type Receiving<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static ChildOf>,
        &'static Attempt,
        &'static debug::Captured,
        Option<&'static NativeSubmitted>,
    ),
    With<Running>,
>;
type Adopting<'w, 's> = Query<
    'w,
    's,
    (&'static mut Settings, &'static mut OperationState),
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
        if dispatch.is_some() && provider.is_none() {
            commands.entity(entity).insert((
                OperationState::Failed,
                CliResult(Err("Workspace provider is unavailable".into())),
            ));
            continue;
        }
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
                    .join(".build-tools/providers")
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
        match resolved {
            Ok(checkout) => {
                commands.entity(entity).insert((
                    Admission,
                    Operation::bundle(&invocation.command).unwrap().running(),
                    checkout,
                    Settings(Environment::current()),
                ));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, CliResult(Err(error.to_string()))));
            }
        }
    }
}

fn load_settings(mut requests: Preparing, mut commands: Commands) {
    for (entity, invocation, dispatch, checkout, mut settings, mut phase, state) in &mut requests {
        if *state != OperationState::Running || *phase != AdmissionPhase::Load {
            continue;
        }
        if dispatch.is_none()
            && let Err(error) = settings.0.load(&checkout.root)
        {
            commands
                .entity(entity)
                .insert((OperationState::Failed, CliResult(Err(error.to_string()))));
            continue;
        }
        if (invocation.is("dx.build") || invocation.is("dx.run"))
            && let Err(error) = settings.0.product(&checkout.root)
        {
            commands
                .entity(entity)
                .insert((OperationState::Failed, CliResult(Err(error.to_string()))));
            continue;
        }
        *phase = AdmissionPhase::Target;
    }
}

fn configure_target(mut requests: Preparing, mut commands: Commands) {
    for (entity, invocation, dispatch, mut checkout, mut settings, mut phase, state) in
        &mut requests
    {
        if *state != OperationState::Running || *phase != AdmissionPhase::Target {
            continue;
        }
        if invocation.is("dx.internal") && invocation.value("task") == Some("cli-release") {
            settings.0.remove("CARGO_BUILD_TARGET");
            settings.0.set(
                "CARGO_TARGET_DIR",
                checkout.root.join(".build-tools/standalone").as_os_str(),
            );
        }
        let web = invocation.is("dx.serve") || invocation.value("platform") == Some("web");
        if web && !settings.0.present("CARGO_TARGET_DIR") {
            settings.0.set(
                "CARGO_TARGET_DIR",
                checkout.root.join("website/target").as_os_str(),
            );
        }
        if dispatch.is_none()
            && let Err(error) = checkout.configure(
                settings.0.get("CARGO_TARGET_DIR"),
                settings.0.get("CARGO_BUILD_TARGET"),
            )
        {
            commands
                .entity(entity)
                .insert((OperationState::Failed, CliResult(Err(error.to_string()))));
            continue;
        }
        *phase = AdmissionPhase::Tooling;
    }
}

fn ensure_tooling(mut requests: Preparing, mut commands: Commands) {
    for (entity, _, dispatch, checkout, settings, mut phase, state) in &mut requests {
        if *state != OperationState::Running || *phase != AdmissionPhase::Tooling {
            continue;
        }
        if dispatch.is_none()
            && let Err(error) = tooling::Tooling::ensure(&checkout, &settings.0)
        {
            commands
                .entity(entity)
                .insert((OperationState::Failed, CliResult(Err(error.to_string()))));
            continue;
        }
        *phase = AdmissionPhase::Configure;
    }
}

fn configure_settings(mut requests: Preparing) {
    for (_, invocation, dispatch, checkout, mut settings, mut phase, state) in &mut requests {
        if *state != OperationState::Running || *phase != AdmissionPhase::Configure {
            continue;
        }
        if dispatch.is_none() {
            settings
                .0
                .set("CARGO_TARGET_DIR", checkout.target.as_os_str());
        }
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
            settings.0.set("VMUX_BUILD_PROFILE", profile);
        } else if !settings.0.present("VMUX_BUILD_PROFILE") {
            settings.0.set("VMUX_BUILD_PROFILE", "dev");
        }
        if let Some(platform) = platform {
            settings.0.set("VMUX_DX_PLATFORM", platform);
        }
        if let Some(features) = invocation.value_os("features") {
            settings.0.set("VMUX_DESKTOP_FEATURE_SET", features);
        }
        if let Some(profile) = invocation.value_os("user-profile") {
            settings.0.set("VMUX_PROFILE", profile);
        } else if !settings.0.present("VMUX_PROFILE") {
            settings.0.set("VMUX_PROFILE", "personal");
        }
        *phase = AdmissionPhase::Storage;
    }
}

fn prepare_storage(mut requests: Preparing, mut commands: Commands) {
    for (entity, invocation, _, checkout, settings, mut phase, state) in &mut requests {
        if *state != OperationState::Running || *phase != AdmissionPhase::Storage {
            continue;
        }
        let configured = policy::Policy::configured(&settings.0);
        let storage = match configured {
            Ok(policy) => Storage::new(checkout.root.clone(), checkout.target.clone(), policy),
            Err(error) => Err(error),
        };
        let storage = match storage {
            Ok(storage) => storage,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, CliResult(Err(error.to_string()))));
                continue;
            }
        };
        if invocation.is("dx.run")
            && invocation.value("platform").unwrap_or("desktop") == "desktop"
            && invocation.value("profile") == Some("local")
        {
            let product = match settings.0.product(&checkout.root) {
                Ok(product) => product.value("VMUX_HOST_NAME").to_owned(),
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert((OperationState::Failed, CliResult(Err(error.to_string()))));
                    continue;
                }
            };
            commands.entity(entity).insert(local::Selection { product });
        }
        commands.entity(entity).insert(storage);
        *phase = AdmissionPhase::Ready;
    }
}

fn admit(mut requests: Preparing, storage: Query<(), With<Storage>>, mut commands: Commands) {
    for (entity, invocation, _, checkout, _, phase, state) in &mut requests {
        if *state != OperationState::Running
            || *phase != AdmissionPhase::Ready
            || !storage.contains(entity)
        {
            continue;
        }
        commands.entity(entity).insert((
            Accepted,
            Name::new(invocation.command.clone()),
            CreatedAt::now(),
            Cwd(checkout.root.clone()),
        ));
    }
}

#[derive(SystemParam)]
struct Plans<'w, 's> {
    commands: Commands<'w, 's>,
}

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

    fn native(&mut self, owner: Entity, tail: &mut Tail, work: Work) {
        let name = work.name();
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
struct Planned;

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
                minimum: storage.minimum(),
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
                minimum: storage.minimum(),
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

fn prepare_build(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.build") && tail.0.is_none()) {
            continue;
        }
        plans.native(entity, &mut tail, Work::Check);
        plans.request(
            entity,
            &mut tail,
            "Acquire target lock",
            debug::Acquire { inherit: true },
        );
        plans.native(entity, &mut tail, Work::Seed);
        plans.native(entity, &mut tail, Work::Preflight);
    }
}

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

        plans.native(entity, &mut tail, Work::Native);
        plans.native(entity, &mut tail, Work::Tailwind);
        plans.request(
            entity,
            &mut tail,
            "Prepare signing identity",
            debug::IdentityRequest { print: false },
        );
        plans.native(entity, &mut tail, Work::Helper);
        plans.request(entity, &mut tail, "Invalidate old artifact", debug::Clear);
        plans.native(entity, &mut tail, Work::cargo(&["build", "-p", "vmux_cli", "-p", "vmux_vm"]));
        plans.native(
            entity,
            &mut tail,
            Work::cargo(&[
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
        plans.native(entity, &mut tail, Work::Cargo(args));
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

        plans.native(entity, &mut tail, Work::Native);
        plans.native(entity, &mut tail, Work::Tailwind);
        plans.native(entity, &mut tail, Work::PackageTools);
        plans.request(
            entity,
            &mut tail,
            "Prepare signing identity",
            debug::IdentityRequest { print: false },
        );
        plans.native(entity, &mut tail, Work::Package(profile.into()));
    }
}

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

        plans.native(entity, &mut tail, Work::Dioxus("ios".into()));
        plans.native(entity, &mut tail, Work::ReleaseIos);
    }
}

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

        plans.native(entity, &mut tail, Work::Dioxus(platform.into()));
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
        plans.native(entity, &mut tail, Work::Tool("dx".into(), args));
        if platform == "ios" {
            plans.native(entity, &mut tail, Work::ResourcesIos);
            plans.native(entity, &mut tail, Work::VerifyIos(false));
        }
    }
}

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

        plans.native(entity, &mut tail, Work::Dioxus(platform.into()));
        plans.native(entity, &mut tail, Work::Binaryen);
        plans.native(entity, &mut tail, Work::Tailwind);
        plans.native(entity, &mut tail, Work::Css);
        let mut args = vec!["build".into(), "--platform".into(), "web".into()];
        if profile != "dev" {
            args.extend(["--ssg".into(), "--release".into()]);
        }
        args.append(&mut forwarded);
        plans.native(entity, &mut tail, Work::Tool("dx".into(), args));
        if profile != "dev" {
            plans.native(entity, &mut tail, Work::FlattenWeb);
        }
    }
}

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

fn finish_build_plan(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let profile = invocation.value("profile").unwrap_or("dev");
        if !(dispatch.is_none() && invocation.is("dx.build")) {
            continue;
        }
        plans.native(entity, &mut tail, Work::Finish(profile.into()));
        plans.commands.entity(entity).insert(Planned);
    }
}

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
            plans.native(entity, &mut tail, Work::Dioxus(platform.into()));
            if platform == "web" {
                plans.native(entity, &mut tail, Work::Binaryen);
                plans.native(entity, &mut tail, Work::Tailwind);
                plans.native(entity, &mut tail, Work::Css);
            }
            if platform == "ios" {
                plans.native(entity, &mut tail, Work::Simulator);
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

fn plan_stop(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.stop")) {
            continue;
        }
        plans.request(entity, &mut tail, "Stop development service", debug::Stop);
        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_reset_layout(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.reset-layout")) {
            continue;
        }
        plans.native(entity, &mut tail, Work::Reset);
        plans.commands.entity(entity).insert(Planned);
    }
}

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
        plans.native(entity, &mut tail, Work::Seed);
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
        plans.native(entity, &mut tail, Work::Cargo(args));

        plans.commands.entity(entity).insert(Planned);
    }
}

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
        plans.native(entity, &mut tail, Work::Seed);
        plans.native(
            entity,
            &mut tail,
            Work::cargo(&["fmt", "--all", "--", "--check"]),
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
        args.extend(["--".into(), "-D".into(), "warnings".into()]);
        plans.native(entity, &mut tail, Work::Cargo(args));

        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_doctor(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.doctor")) {
            continue;
        }
        plans.native(entity, &mut tail, Work::Doctor);
        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_setup(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        let platform = invocation.value("platform").unwrap_or("desktop");
        if !(dispatch.is_none() && invocation.is("dx.setup")) {
            continue;
        }

        if platform == "desktop" {
            plans.native(entity, &mut tail, Work::Check);
            plans.request(
                entity,
                &mut tail,
                "Acquire target lock",
                debug::Acquire { inherit: true },
            );
            plans.native(entity, &mut tail, Work::Native);
            plans.native(entity, &mut tail, Work::Tailwind);
            plans.native(entity, &mut tail, Work::PackageTools);
            plans.request(
                entity,
                &mut tail,
                "Prepare signing identity",
                debug::IdentityRequest { print: false },
            );
            plans.native(entity, &mut tail, Work::Helper);
        } else {
            plans.native(entity, &mut tail, Work::Dioxus(platform.into()));
            if platform == "web" {
                plans.native(entity, &mut tail, Work::Binaryen);
                plans.native(entity, &mut tail, Work::Tailwind);
            }
        }

        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_cache_status(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.cache.status")) {
            continue;
        }
        plans.native(entity, &mut tail, Work::Status);
        plans.commands.entity(entity).insert(Planned);
    }
}

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
        plans.native(entity, &mut tail, Work::Clean(invocation.flag("apply")));

        plans.commands.entity(entity).insert(Planned);
    }
}

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
        plans.native(entity, &mut tail, Work::Scope(forwarded));
        plans.commands.entity(entity).insert(Planned);
    }
}

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
        plans.native(entity, &mut tail, Work::NativeInputs(forwarded));
        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_checks(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _storage, mut tail) in &mut operations {
        if !(dispatch.is_none() && invocation.is("dx.checks.test")) {
            continue;
        }
        plans.native(
            entity,
            &mut tail,
            Work::Internal("pre-push".into(), Vec::new()),
        );
        plans.commands.entity(entity).insert(Planned);
    }
}

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
        plans.native(entity, &mut tail, Work::Consumer(arguments));
        plans.commands.entity(entity).insert(Planned);
    }
}

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
            plans.native(entity, &mut tail, Work::Check);
            plans.request(
                entity,
                &mut tail,
                "Acquire target lock",
                debug::Acquire { inherit: true },
            );
            plans.native(entity, &mut tail, Work::Preflight);
            plans.native(entity, &mut tail, Work::CliCompile);
            plans.native(entity, &mut tail, Work::CliPackage(preview));
            plans.native(entity, &mut tail, Work::Finish("release".into()));
        } else {
            if task == "cargo" {
                plans.request(
                    entity,
                    &mut tail,
                    "Acquire target lock",
                    debug::Acquire { inherit: true },
                );
                plans.native(entity, &mut tail, Work::Seed);
            }
            match task {
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
                    plans.native(entity, &mut tail, Work::Internal(task.into(), forwarded));
                }
            }
        }

        plans.commands.entity(entity).insert(Planned);
    }
}

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
        }
        let mut captured_settings = settings.clone();
        if let Some(executable) = executable {
            captured_settings
                .0
                .set("VMUX_DX_PROVIDER_EXECUTABLE", executable.path().as_os_str());
        }
        let mut step = commands.entity(entity);
        step.insert((
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

type NativeReady<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Work,
        Option<&'static ChildOf>,
        &'static Attempt,
        &'static debug::Captured,
        &'static Checkout,
        &'static Settings,
    ),
    (With<Running>, Without<NativeSubmitted>),
>;

fn native(steps: NativeReady, operations: Inputs, mailbox: Res<Mailbox>, mut commands: Commands) {
    for (entity, work, parent, attempt, captured, checkout, settings) in &steps {
        if parent.is_none_or(|parent| parent.parent() != captured.root)
            || attempt.0 != captured.attempt
        {
            commands.entity(entity).despawn();
            if operations
                .get(captured.root)
                .is_ok_and(|(_, _, _, _, _, state)| *state == OperationState::Running)
            {
                commands.entity(captured.root).remove::<Lease>().insert((
                    OperationState::Failed,
                    CliResult(Err("Development step ownership changed".into())),
                ));
            }
            continue;
        }
        let Ok((_, _, storage, lease, executable, _)) = operations.get(captured.root) else {
            continue;
        };
        let checkout = checkout.clone();
        let mut environment = settings.0.clone();
        if let Some(executable) = executable {
            environment.set("VMUX_DX_PROVIDER_EXECUTABLE", executable.path().as_os_str());
        }
        let storage = storage.clone();
        let lease = lease.map(|lease| lease.0.clone());
        let work = work.clone();
        let sender = mailbox.sender.clone();
        let wake = mailbox.wake.clone();
        let root = captured.root;
        let attempt = attempt.0;
        commands
            .entity(entity)
            .insert(NativeSubmitted(debug::Captured { root, attempt }));
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
                root,
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

fn cancel(
    signals: Option<Res<signals::Signals>>,
    mut operations: Cancellations,
    running: Query<(&debug::Captured, Option<&NativeSubmitted>), With<Running>>,
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
        if !running.iter().any(|(captured, submitted)| {
            submitted
                .map(|submitted| submitted.0.root)
                .unwrap_or(captured.root)
                == entity
        }) {
            commands
                .entity(entity)
                .insert(CliResult(Ok((128 + signal) as u8)));
        }
    }
}

fn collect(
    mut completions: ResMut<Messages<Completion>>,
    steps: Receiving,
    mut operations: Adopting,
    leases: Query<&Lease>,
    mut commands: Commands,
) {
    for completion in completions.drain() {
        let Ok((parent, attempt, captured, submitted)) = steps.get(completion.entity) else {
            continue;
        };
        let accepted = submitted.map(|submitted| &submitted.0).unwrap_or(captured);
        if accepted.root != completion.root || accepted.attempt != completion.attempt {
            continue;
        }
        if parent.is_none_or(|parent| parent.parent() != accepted.root)
            || attempt.0 != accepted.attempt
            || captured.root != accepted.root
            || captured.attempt != accepted.attempt
        {
            commands.entity(completion.entity).despawn();
            if let Ok((_, mut state)) = operations.get_mut(accepted.root) {
                commands.entity(accepted.root).remove::<Lease>();
                if *state == OperationState::Running {
                    *state = OperationState::Failed;
                    commands
                        .entity(accepted.root)
                        .insert(CliResult(Err("Development step ownership changed".into())));
                }
            }
            continue;
        }
        let Ok((mut settings, mut state)) = operations.get_mut(completion.root) else {
            commands.entity(completion.entity).despawn();
            continue;
        };
        if *state == OperationState::Canceled {
            commands
                .entity(completion.entity)
                .remove::<(Running, Lease)>()
                .insert((Completed, OperationState::Canceled));
            continue;
        }
        match completion.result {
            Ok(outcome) => {
                match outcome {
                    Outcome::Executable(executable) => {
                        commands
                            .entity(completion.root)
                            .insert(executable)
                            .remove::<Lease>();
                    }
                    Outcome::Done => {}
                    Outcome::Identity(identity) => {
                        settings.0.set("APPLE_SIGNING_IDENTITY", &identity);
                    }
                    Outcome::Artifacts(paths) => {
                        for path in paths {
                            commands.spawn((
                                Name::new(path.display().to_string()),
                                Artifact(path),
                                CreatedAt::now(),
                                vmux_ecs::Ready,
                                Completed,
                                ChildOf(completion.root),
                            ));
                        }
                    }
                    Outcome::ToolPath(key, path) => {
                        let key = key.to_str().expect("tool configuration key");
                        settings.0.set(key, &path);
                    }
                }
                if let Ok(lease) = leases.get(completion.entity) {
                    commands
                        .entity(completion.root)
                        .insert(Lease(lease.0.clone()));
                }
                commands
                    .entity(completion.entity)
                    .remove::<(Running, Lease)>()
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
                    .remove::<(Running, Lease)>()
                    .insert((Completed, OperationState::Failed));
                commands
                    .entity(completion.root)
                    .insert(CliResult(Ok(error.code() as u8)));
            }
        }
    }
}

fn finish(
    operations: Finishing,
    completed: Query<Entity, With<Completed>>,
    running: Query<(&debug::Captured, Option<&NativeSubmitted>), With<Running>>,
    artifacts: Query<&Artifact, With<vmux_ecs::Ready>>,
    mut commands: Commands,
) {
    for (entity, children, state) in &operations {
        if *state == OperationState::Canceled {
            continue;
        }
        if running.iter().any(|(captured, submitted)| {
            submitted
                .map(|submitted| submitted.0.root)
                .unwrap_or(captured.root)
                == entity
        }) {
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
                    Planned,
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
    fn unplanned_and_failed_operations_cannot_start_their_materialized_children() {
        for planned in [false, true] {
            let mut fixture = Fixture::new();
            let step = fixture.step("printf unexpected > unexpected");
            if planned {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(fixture.operation)
                    .insert(OperationState::Failed);
            } else {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(fixture.operation)
                    .remove::<Planned>();
            }
            fixture.app.update();
            assert!(fixture.app.world().get::<Running>(step).is_none());
            assert!(!fixture.directory.path().join("unexpected").exists());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<CliResult>(fixture.operation)
                    .is_none()
            );
        }
    }

    #[test]
    fn a_removed_provider_is_refused_before_admission() {
        let mut app = App::new();
        app.add_plugins(Plugin);
        let provider = app.world_mut().spawn_empty().id();
        app.world_mut().despawn(provider);
        let operation = app
            .world_mut()
            .spawn((
                CliInvocation {
                    command: "fixture.run".into(),
                    arguments: Default::default(),
                },
                workspace::Dispatch {
                    provider,
                    arguments: Vec::new(),
                },
            ))
            .id();
        app.update();
        let result = app
            .world()
            .get::<CliResult>(operation)
            .expect("removed provider must not wait indefinitely");
        assert_eq!(
            result.0.as_ref().unwrap_err(),
            "Workspace provider is unavailable"
        );
        assert!(app.world().get::<Admission>(operation).is_none());
    }

    #[test]
    fn stale_completion_cannot_advance_the_current_attempt() {
        let mut fixture = Fixture::new();
        let step = fixture.step("exit 99");
        fixture.app.world_mut().entity_mut(step).insert((
            Running,
            NativeSubmitted(debug::Captured {
                root: fixture.operation,
                attempt: 1,
            }),
            debug::Captured {
                root: fixture.operation,
                attempt: 1,
            },
        ));
        fixture.app.world_mut().write_message(Completion {
            entity: step,
            root: fixture.operation,
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
            root: fixture.operation,
            attempt: 1,
            result: Ok(Outcome::Done),
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
            fixture.app.world_mut().entity_mut(step).insert((
                Running,
                NativeSubmitted(debug::Captured {
                    root: fixture.operation,
                    attempt: 1,
                }),
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
            fixture.app.world_mut().write_message(Completion {
                entity: step,
                root: fixture.operation,
                attempt: 1,
                result: Ok(Outcome::ToolPath(
                    "VMUX_DX_OWNER_ORACLE".into(),
                    "wrong-owner".into(),
                )),
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
            let storage = Storage::new(
                root.clone(),
                target.clone(),
                policy::Policy::environment().unwrap(),
            )
            .unwrap();
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
}
