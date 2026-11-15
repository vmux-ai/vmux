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
    (Without<Admission>, Without<Accepted>, Without<CliResult>),
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
    fn step(&mut self, owner: Entity, tail: &mut Tail, work: Work) {
        let mut step = self.commands.spawn((
            Name::new(work.name()),
            OperationState::Pending,
            CreatedAt::now(),
            ChildOf(owner),
            Attempt(1),
            work,
        ));
        if let Some(previous) = tail.0 {
            step.insert(Predecessor(previous));
        }
        tail.0 = Some(step.id());
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
        &'static mut Tail,
    ),
    (With<Accepted>, Without<Planned>, Without<CliResult>),
>;

fn plan_provider(
    mut operations: Planning,
    providers: Query<&workspace::Provider>,
    mut plans: Plans,
) {
    for (entity, _, dispatch, mut tail) in &mut operations {
        let Some(dispatch) = dispatch else {
            continue;
        };
        let Ok(provider) = providers.get(dispatch.provider) else {
            plans.commands.entity(entity).insert((
                OperationState::Failed,
                CliResult(Err("Workspace provider is unavailable".into())),
            ));
            continue;
        };
        for work in [
            Work::WorkspaceAcquire,
            Work::WorkspaceCompile(
                provider.package.clone(),
                provider.binary.clone(),
                provider.cargo.clone(),
            ),
            Work::WorkspaceInvoke(dispatch.arguments.clone()),
        ] {
            plans.step(entity, &mut tail, work);
        }
        plans.commands.entity(entity).insert(Planned);
    }
}
fn prepare_build(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        if dispatch.is_some() || !(invocation.is("dx.build")) {
            continue;
        }

        for work in [Work::Check, Work::Acquire, Work::Seed, Work::Preflight] {
            plans.step(entity, &mut tail, work);
        }
    }
}

fn plan_desktop_dev(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
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
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if dispatch.is_some()
            || !(invocation.is("dx.build") && (platform == "desktop" && profile == "dev"))
        {
            continue;
        }

        for work in [
            Work::Native,
            Work::Tailwind,
            Work::Codesign,
            Work::Helper,
            Work::ClearReady,
        ] {
            plans.step(entity, &mut tail, work);
        }
        plans.step(
            entity,
            &mut tail,
            Work::cargo(&["build", "-p", "vmux_cli", "-p", "vmux_vm"]),
        );
        plans.step(
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
        plans.step(entity, &mut tail, Work::Cargo(args));
        if cfg!(target_os = "macos") {
            for work in [Work::SignDebug, Work::BundleDebug] {
                plans.step(entity, &mut tail, work);
            }
        }
        plans.step(entity, &mut tail, Work::PublishDebug);
    }
}

fn plan_desktop_package(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
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
        if dispatch.is_some()
            || !(invocation.is("dx.build") && (platform == "desktop" && profile != "dev"))
        {
            continue;
        }

        for work in [
            Work::Native,
            Work::Tailwind,
            Work::PackageTools,
            Work::Codesign,
            Work::Package(profile.into()),
        ] {
            plans.step(entity, &mut tail, work);
        }
    }
}

fn plan_mobile_release(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
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
        if dispatch.is_some()
            || !(invocation.is("dx.build") && (platform == "ios" && profile == "release"))
        {
            continue;
        }

        for work in [Work::Dioxus("ios".into()), Work::ReleaseIos] {
            plans.step(entity, &mut tail, work);
        }
    }
}

fn plan_mobile(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
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
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if dispatch.is_some()
            || !(invocation.is("dx.build")
                && ((platform == "ios" && profile != "release") || platform == "android"))
        {
            continue;
        }

        plans.step(entity, &mut tail, Work::Dioxus(platform.into()));
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
        plans.step(entity, &mut tail, Work::Tool("dx".into(), args));
        if platform == "ios" {
            for work in [Work::ResourcesIos, Work::VerifyIos(false)] {
                plans.step(entity, &mut tail, work);
            }
        }
    }
}

fn plan_web(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
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
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if dispatch.is_some() || !(invocation.is("dx.build") && (platform == "web")) {
            continue;
        }

        for work in [
            Work::Dioxus(platform.into()),
            Work::Binaryen,
            Work::Tailwind,
            Work::Css,
        ] {
            plans.step(entity, &mut tail, work);
        }
        let mut args = vec!["build".into(), "--platform".into(), "web".into()];
        if profile != "dev" {
            args.extend(["--ssg".into(), "--release".into()]);
        }
        args.append(&mut forwarded);
        plans.step(entity, &mut tail, Work::Tool("dx".into(), args));
        if profile != "dev" {
            plans.step(entity, &mut tail, Work::FlattenWeb);
        }
    }
}

fn refuse_build(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, _tail) in &mut operations {
        let platform = if invocation.is("dx.serve") {
            "web"
        } else {
            invocation.value("platform").unwrap_or("desktop")
        };
        if dispatch.is_some()
            || !invocation.is("dx.build")
            || matches!(platform, "desktop" | "ios" | "android" | "web")
        {
            continue;
        }
        plans.commands.entity(entity).insert((
            OperationState::Failed,
            CliResult(Err("Unsupported build platform".into())),
        ));
    }
}

fn finish_build_plan(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        let profile = if invocation.is("dx.serve") {
            "dev"
        } else {
            invocation.value("profile").unwrap_or("dev")
        };
        if dispatch.is_some() || !(invocation.is("dx.build")) {
            continue;
        }
        plans.step(entity, &mut tail, Work::Finish(profile.into()));
        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_run(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
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
        if dispatch.is_some() || !(invocation.is("dx.run") || invocation.is("dx.serve")) {
            continue;
        }

        plans.step(entity, &mut tail, Work::Acquire);
        if platform != "desktop" {
            plans.step(entity, &mut tail, Work::Dioxus(platform.into()));
            if platform == "web" {
                for work in [Work::Binaryen, Work::Tailwind, Work::Css] {
                    plans.step(entity, &mut tail, work);
                }
            }
            if platform == "ios" {
                plans.step(entity, &mut tail, Work::Simulator);
            }
        }
        if platform == "desktop" && profile == "local" {
            for work in [Work::LocalEntries, Work::LocalLaunch] {
                plans.step(entity, &mut tail, work);
            }
        } else {
            plans.step(
                entity,
                &mut tail,
                Work::Run(profile.into(), platform.into()),
            );
        }

        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_stop(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        if dispatch.is_some() || !(invocation.is("dx.stop")) {
            continue;
        }
        plans.step(entity, &mut tail, Work::Stop);
        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_reset_layout(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        if dispatch.is_some() || !(invocation.is("dx.reset-layout")) {
            continue;
        }
        plans.step(entity, &mut tail, Work::Reset);
        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_test(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        let profile = if invocation.is("dx.serve") {
            "dev"
        } else {
            invocation.value("profile").unwrap_or("dev")
        };
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if dispatch.is_some() || !(invocation.is("dx.test")) {
            continue;
        }

        for work in [Work::Acquire, Work::Seed] {
            plans.step(entity, &mut tail, work);
        }
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
        plans.step(entity, &mut tail, Work::Cargo(args));

        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_lint(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        let profile = if invocation.is("dx.serve") {
            "dev"
        } else {
            invocation.value("profile").unwrap_or("dev")
        };
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if dispatch.is_some() || !(invocation.is("dx.lint")) {
            continue;
        }

        for work in [Work::Acquire, Work::Seed] {
            plans.step(entity, &mut tail, work);
        }
        plans.step(
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
        plans.step(entity, &mut tail, Work::Cargo(args));

        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_doctor(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        if dispatch.is_some() || !(invocation.is("dx.doctor")) {
            continue;
        }
        plans.step(entity, &mut tail, Work::Doctor);
        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_setup(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        let platform = if invocation.is("dx.serve") {
            "web"
        } else {
            invocation.value("platform").unwrap_or("desktop")
        };
        if dispatch.is_some() || !(invocation.is("dx.setup")) {
            continue;
        }

        if platform == "desktop" {
            for work in [
                Work::Check,
                Work::Acquire,
                Work::Native,
                Work::Tailwind,
                Work::PackageTools,
                Work::Codesign,
                Work::Helper,
            ] {
                plans.step(entity, &mut tail, work);
            }
        } else {
            plans.step(entity, &mut tail, Work::Dioxus(platform.into()));
            if platform == "web" {
                for work in [Work::Binaryen, Work::Tailwind] {
                    plans.step(entity, &mut tail, work);
                }
            }
        }

        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_cache_status(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        if dispatch.is_some() || !(invocation.is("dx.cache.status")) {
            continue;
        }
        plans.step(entity, &mut tail, Work::Status);
        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_cache_clean(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        if dispatch.is_some() || !(invocation.is("dx.cache.clean")) {
            continue;
        }

        for work in [Work::Acquire, Work::Clean(invocation.flag("apply"))] {
            plans.step(entity, &mut tail, work);
        }

        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_ci_scope(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        let forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if dispatch.is_some() || !(invocation.is("dx.ci-scope")) {
            continue;
        }
        plans.step(entity, &mut tail, Work::Scope(forwarded));
        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_ci_native(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        let forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if dispatch.is_some() || !(invocation.is("dx.ci-native")) {
            continue;
        }
        plans.step(entity, &mut tail, Work::NativeInputs(forwarded));
        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_checks(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        if dispatch.is_some() || !(invocation.is("dx.checks.test")) {
            continue;
        }
        plans.step(
            entity,
            &mut tail,
            Work::Internal("pre-push".into(), Vec::new()),
        );
        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_sdk(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        if dispatch.is_some() || !(invocation.is("dx.checks.sdk")) {
            continue;
        }

        let mut arguments = Vec::new();
        if invocation.flag("host") {
            arguments.push("--host".into());
        }
        if invocation.flag("git") {
            arguments.push("--git".into());
        }
        for work in [Work::Acquire, Work::Consumer(arguments)] {
            plans.step(entity, &mut tail, work);
        }

        plans.commands.entity(entity).insert(Planned);
    }
}

fn plan_internal(mut operations: Planning, mut plans: Plans) {
    for (entity, invocation, dispatch, mut tail) in &mut operations {
        let forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        if dispatch.is_some() || !(invocation.is("dx.internal")) {
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
            for work in [
                Work::Check,
                Work::Acquire,
                Work::Preflight,
                Work::CliCompile,
                Work::CliPackage(preview),
                Work::Finish("release".into()),
            ] {
                plans.step(entity, &mut tail, work);
            }
        } else {
            if task == "cargo" {
                for work in [Work::Acquire, Work::Seed] {
                    plans.step(entity, &mut tail, work);
                }
            }
            plans.step(entity, &mut tail, Work::Internal(task.into(), forwarded));
        }

        plans.commands.entity(entity).insert(Planned);
    }
}

fn refuse_plan(operations: Planning, mut plans: Plans) {
    for (entity, _, _, _) in &operations {
        plans.commands.entity(entity).insert((
            OperationState::Failed,
            CliResult(Err("Unknown development command".into())),
        ));
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
        if *state != OperationState::Running {
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
