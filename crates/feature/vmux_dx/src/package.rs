use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use base64::{Engine, engine::general_purpose::STANDARD};

use bevy_app::{App, PreUpdate, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::jobs::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake};
use vmux_ecs::{Cwd, Operation, OperationState, Order};

use crate::cli_package_io::{CreateTemporary, Directory as TemporaryDirectory};
use crate::debug_io::{Flight, Task};
use crate::execution::Failure;
use crate::package_io::{CleanupCapture, Copy, Fingerprint, Hash, Scoped, Write};
use crate::publication;
use crate::repository::Checkout;
use crate::signals::Signals;
use crate::storage_io::{
    Capture, DirectoryEntries, FileBytes, Inspect, PathMetadata, ReadDirectory, ReadFile,
    StorageJob, ToolOutput,
};
use crate::tooling_io::CreateDirectory;
use crate::work::Outcome;
use crate::{Attempt, Completion, Lease, Settings};

pub(crate) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            IoPlugin::<Capture>::default(),
            IoPlugin::<Inspect>::default(),
            IoPlugin::<ReadFile>::default(),
            IoPlugin::<ReadDirectory>::default(),
            IoPlugin::<Hash>::default(),
            IoPlugin::<Copy>::default(),
            IoPlugin::<Write>::default(),
            IoPlugin::<CleanupCapture>::default(),
            IoPlugin::<CreateDirectory>::default(),
            IoPlugin::<CreateTemporary>::default(),
        ))
        .configure_sets(
            Update,
            (Stage::Collect, Stage::Decide, Stage::Submit, Stage::Finish)
                .chain()
                .in_set(JobSet::Apply)
                .after(crate::Stage::Execute),
        )
        .add_observer(accept::<Binaries>)
        .add_observer(accept::<Web>)
        .add_observer(accept::<Before>)
        .add_observer(accept::<Desktop>)
        .add_observer(accept::<Verify>)
        .add_systems(PreUpdate, cancel)
        .add_systems(
            Update,
            (
                cancel,
                check_outputs,
                (prepare_cargo, probe_cargo, configure_cargo).chain(),
                compile,
                prepare_web,
                (scan_web, scan_directories).chain(),
                hash_web,
                verify_web,
                copy_binaries,
                (
                    prepare_desktop,
                    configure_desktop,
                    (create_staging, provision_keychain, prepare_certificate).chain(),
                    (
                        create_keychain,
                        configure_keychain,
                        activate_keychain,
                        confirm_keychain,
                    )
                        .chain(),
                    (execute_packager, run_packager).chain(),
                    title_desktop,
                    postprocess_desktop,
                    inspect_desktop,
                    (verify_desktop, extract_certificate).chain(),
                    cleanup_keychain,
                    (observe_publication, publish_desktop).chain(),
                )
                    .chain(),
                (
                    prepare_cef,
                    validate_cef,
                    inject_web,
                    (bundle_cef, collect_cef).chain(),
                    trim_cef,
                    resources,
                    embed_service,
                    remove_service,
                    configure_service,
                )
                    .chain(),
                (
                    scan_signatures,
                    inspect_signatures,
                    sign,
                    verify_signature,
                    notarize,
                    staple,
                    assess,
                    complete_callbacks,
                )
                    .chain(),
                (
                    collect_guards,
                    cleanup_guards,
                    submit_guards,
                    retire_guards,
                    forget_witnesses,
                )
                    .chain(),
                wake,
            )
                .chain()
                .in_set(Stage::Decide),
        )
        .add_systems(Update, finish.in_set(Stage::Finish));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Stage {
    Collect,
    Decide,
    Submit,
    Finish,
}

#[derive(EntityEvent)]
pub(crate) struct Requested<T: Component> {
    pub entity: Entity,
    pub context: Entity,
    pub attempt: u64,
    pub input: T,
}

#[derive(Component, Clone)]
pub(crate) struct Binaries;

#[derive(Component, Clone)]
pub(crate) struct Before {
    pub app: PathBuf,
    pub format: OsString,
    pub force: bool,
}

#[derive(Component, Clone)]
pub(crate) struct Desktop {
    pub profile: String,
}

#[derive(Component)]
struct Fast(bool);

#[derive(Component)]
struct Manifest(toml::Table);

impl Manifest {
    fn configuration(
        &self,
        product: &crate::product::ProductMetadata,
    ) -> crate::Result<serde_json::Value> {
        let metadata = self
            .0
            .get("package")
            .and_then(|value| value.get("metadata"))
            .and_then(|value| value.get("packager"))
            .ok_or("Missing packaging configuration")?;
        let mut config = serde_json::to_value(metadata)?;
        config["icons"] = serde_json::json!([product.value("VMUX_HOST_ICON")]);
        config["dmg"]["background"] = product.value("VMUX_HOST_DMG_BACKGROUND").into();
        config["macos"]["entitlements"] = product.value("VMUX_HOST_ENTITLEMENTS").into();
        config["macos"]["info-plist-path"] = product.value("VMUX_HOST_USAGE_DESCRIPTIONS").into();
        for binary in config["binaries"]
            .as_array_mut()
            .ok_or("Missing packaging binaries")?
        {
            if binary["path"] == "Vmux Service" {
                binary["path"] = product.value("VMUX_HOST_SERVICE_NAME").into();
            }
        }
        config["before-packaging-command"] = "\"$VMUX_DX_BIN\" internal package-binaries".into();
        config["before-each-package-command"] = "\"$VMUX_DX_BIN\" internal before-package".into();
        Ok(config)
    }
}

#[derive(Component)]
struct Version(String);

#[derive(Component)]
struct Title(String);

#[derive(Component)]
struct Staging(std::sync::Arc<crate::temporary::Temporary>);

#[derive(Component)]
struct SigningDirectory(std::sync::Arc<crate::temporary::Temporary>);

#[derive(Component)]
struct Keychain(PathBuf);

#[derive(Component)]
struct OriginalKeychain(OsString);

#[derive(Component)]
struct GuardRef(Entity);

#[derive(Component)]
#[require(CleanupPhase)]
struct KeychainScope {
    owner: Entity,
    context: Entity,
    attempt: u64,
    directory: PathBuf,
    environment: crate::environment::Environment,
    lease: Option<std::sync::Arc<crate::target_lock::TargetLock>>,
}

#[derive(Component)]
struct ReleaseKeychain;

#[derive(Component)]
struct GuardIo(Entity);

#[derive(Component)]
struct Witness {
    owner: Entity,
    context: Entity,
}

#[derive(bevy_ecs::system::SystemParam)]
struct Keys<'w, 's> {
    references: Query<'w, 's, &'static GuardRef>,
    directories: Query<'w, 's, &'static SigningDirectory>,
    keychains: Query<'w, 's, &'static Keychain>,
}

impl Keys<'_, '_> {
    fn entity(&self, owner: Entity) -> Option<Entity> {
        self.references.get(owner).ok().map(|reference| reference.0)
    }

    fn directory(&self, owner: Entity) -> Option<&SigningDirectory> {
        self.directories.get(self.entity(owner)?).ok()
    }

    fn keychain(&self, owner: Entity) -> Option<&Keychain> {
        self.keychains.get(self.entity(owner)?).ok()
    }
}

#[derive(bevy_ecs::system::SystemParam)]
pub(crate) struct Drains<'w, 's> {
    witnesses: Query<'w, 's, &'static Witness>,
    guards: Query<'w, 's, &'static KeychainScope>,
}

impl Drains<'_, '_> {
    pub(crate) fn pending(&self, context: Entity) -> bool {
        self.witnesses
            .iter()
            .any(|witness| witness.context == context)
            || self.guards.iter().any(|guard| guard.context == context)
    }
}

#[derive(Component)]
struct Password(String);

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum CleanupPhase {
    #[default]
    Start,
    Restore,
    Delete,
    Done,
}

#[derive(Component)]
struct Published(Entity);

#[derive(Component)]
struct Observation(Entity);

#[derive(Component)]
struct CertificateDirectory(std::sync::Arc<crate::temporary::Temporary>);

#[derive(Component)]
struct Bundle(PathBuf);

impl Bundle {
    fn service_contents(&self, product: &crate::product::ProductMetadata) -> PathBuf {
        self.0
            .join("Contents/Library/LoginItems")
            .join(product.value("VMUX_HOST_SERVICE_BUNDLE"))
            .join("Contents")
    }
}

#[derive(Component)]
struct Framework(PathBuf);

#[derive(Component)]
struct CefVersion(String);

impl CefVersion {
    fn from_manifest(bytes: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(bytes).ok()?;
        let table = text.parse::<toml::Table>().ok()?;
        let mut parts = table.get("cef_version")?.as_str()?.split('.');
        let major = parts.next()?.parse::<u64>().ok()?;
        let minor = parts.next()?.parse::<u64>().ok()?;
        let patch = parts.next()?.parse::<u64>().ok()?;
        if parts.next().is_some() {
            return None;
        }
        let compatibility = major.checked_mul(10)?.checked_add(minor)?;
        Some(Self(format!("{compatibility}.0.{patch}")))
    }
}

#[derive(Component)]
struct SignTarget;

#[derive(Component)]
struct Signed;

#[derive(Component)]
struct Mach;

#[derive(Component)]
struct Signing;

#[derive(Component)]
struct Identifier(Option<String>);

#[derive(Component, Clone)]
pub(crate) struct Verify {
    pub app: PathBuf,
}

#[derive(Component, Clone)]
pub(crate) struct Web {
    pub directory: PathBuf,
}

#[derive(Component)]
#[require(Phase)]
struct Scope {
    context: Entity,
    attempt: u64,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Start,
    Cli,
    Desktop,
    ServiceBuild,
    Helper,
    Web,
    Stamp,
    Scan,
    Hash,
    Verify,
    Copy,
    Metadata,
    Staging,
    KeychainList,
    KeychainTemporary,
    KeychainPassword,
    KeychainCreate,
    KeychainConfigure,
    KeychainUnlock,
    KeychainImport,
    KeychainPartition,
    KeychainActivate,
    KeychainIdentity,
    KeychainConfirm,
    Packager,
    Packaged,
    Title,
    Postprocess,
    InspectBundle,
    VerifyBundle,
    Certificate,
    Extracted,
    Publication,
    Cef,
    ValidateSource,
    InjectWeb,
    BundleCef,
    WebCopy,
    InspectEmbedded,
    Embedded,
    ValidateEmbedded,
    Locales,
    Resources,
    Service,
    ServiceCopy,
    ServiceConfig,
    SignScan,
    SignInspect,
    Sign,
    Signature,
    NotaryArchive,
    NotarySubmit,
    NotaryReceipt,
    Staple,
    Assess,
    NotaryCleanup,
    Done,
}

#[derive(Component)]
struct Failed(Failure);

#[derive(Component)]
struct RecoveryFailure(Failure);

#[derive(Component)]
struct Complete;

#[derive(Component)]
struct Io;

#[derive(Component)]
struct Owner(Entity);

#[derive(Component)]
struct AcceptedIo {
    context: Entity,
    attempt: u64,
}

#[derive(Component)]
struct Submitted;

#[derive(Component)]
struct Received;

#[derive(Component)]
struct Optional;

#[derive(Component)]
struct Source(PathBuf);

#[derive(Component)]
struct Directory;

#[derive(Component)]
struct Scanned;

#[derive(Component)]
struct File;

#[derive(Component)]
struct Expected(String);

#[derive(Component, Clone, Copy)]
struct Purpose {
    kind: PurposeKind,
    entity: Option<Entity>,
}

impl Purpose {
    fn new(kind: PurposeKind) -> Self {
        Self { kind, entity: None }
    }

    fn targeted(kind: PurposeKind, entity: Entity) -> Self {
        Self {
            kind,
            entity: Some(entity),
        }
    }

    fn target(&self, kind: PurposeKind) -> Option<&Entity> {
        if self.kind != kind {
            return None;
        }
        self.entity.as_ref()
    }

    fn is(&self, kind: PurposeKind) -> bool {
        self.kind == kind && self.entity.is_none()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PurposeKind {
    Compile,
    Required,
    Stamp,
    Directory,
    Hash,
    Copy,
    CargoCache,
    CargoProbe,
    CefManifest,
    CefBinary,
    CefArch,
    CefVersion,
    CefExport,
    Embedded,
    Locales,
    Helpers,
    Service,
    Launch,
    Plist,
    Inspect,
    Signature,
    Notary,
    Metadata,
    AppManifest,
    Revision,
    Branch,
    Staging,
    Keychains,
    SigningDirectory,
    Password,
    Provision,
    Identity,
    Packager,
    AppPlist,
    Postprocess,
    BundleRequired,
    BundleForbidden,
    BundleLaunch,
    CliHash,
    DesktopHash,
    VerifyBundle,
    CertificateDirectory,
    Certificate,
    Restore,
    Delete,
}

type Closing<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Scope, Option<&'static Failed>),
    Or<(With<Failed>, With<Complete>)>,
>;

type WakeRequests<'w, 's> =
    Query<'w, 's, Entity, (With<Scope>, Without<Failed>, Without<Complete>)>;

type RetiredGuards<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static KeychainScope,
        &'static CleanupPhase,
        Option<&'static RecoveryFailure>,
        Option<&'static SigningDirectory>,
    ),
>;

type SigningTargets<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Cwd,
        &'static Identifier,
        &'static Order,
        &'static ChildOf,
    ),
    (With<SignTarget>, Without<Signed>),
>;

type SigningDirectories<'w, 's> =
    Query<'w, 's, (Entity, &'static Cwd, &'static ChildOf), (With<Signing>, Without<Scanned>)>;

type WebDirectories<'w, 's> =
    Query<'w, 's, (Entity, &'static Cwd, &'static ChildOf), (With<Directory>, Without<Scanned>)>;

type CargoRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Scope, &'static mut CargoPhase),
    (With<CargoPreparation>, Without<Failed>, Without<Complete>),
>;

type CargoIncoming<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Scope),
    (Or<(With<Desktop>, With<Binaries>)>, Without<CargoPhase>),
>;

type Requests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Scope, &'static mut Phase),
    (Without<Failed>, Without<Complete>),
>;
type Replies<'w, 's, T> =
    Query<'w, 's, (Entity, &'static Owner, &'static Purpose, &'static T), With<Received>>;
type Returned<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static Owner,
        &'static ChildOf,
        &'static JobOwner<Task<Scoped<J>>>,
        &'static JobResult<Task<Scoped<J>>>,
    ),
    Without<Received>,
>;
type Queued<'w, 's, J> =
    Query<'w, 's, (Entity, &'static Owner, &'static J), (With<Io>, Without<Submitted>)>;
type Pending<'w, 's> = Query<'w, 's, &'static Owner, (With<Io>, Without<Received>)>;

#[derive(bevy_ecs::system::SystemParam)]
struct Receipts<'w, 's> {
    files: Replies<'w, 's, FileBytes>,
    outputs: Replies<'w, 's, ToolOutput>,
    paths: Replies<'w, 's, PathMetadata>,
    hashes: Replies<'w, 's, Fingerprint>,
    entries: Replies<'w, 's, DirectoryEntries>,
    temporary: Replies<'w, 's, TemporaryDirectory>,
    pending: Pending<'w, 's>,
}

#[derive(bevy_ecs::system::SystemParam)]
struct Loans<'w, 's> {
    keys: Keys<'w, 's>,
    staging: Query<'w, 's, &'static Staging>,
    certificates: Query<'w, 's, &'static CertificateDirectory>,
    targets: Query<'w, 's, &'static Lease>,
}

#[derive(bevy_ecs::system::SystemParam)]
struct Inputs<'w, 's> {
    desktops: Query<'w, 's, &'static Desktop>,
    fast: Query<'w, 's, &'static Fast>,
    manifests: Query<'w, 's, &'static Manifest>,
    versions: Query<'w, 's, &'static Version>,
    names: Query<'w, 's, &'static Name>,
    staging: Query<'w, 's, &'static Staging>,
    bundles: Query<'w, 's, &'static Bundle>,
}

#[derive(bevy_ecs::system::SystemParam)]
struct CleanupInputs<'w, 's> {
    keys: Keys<'w, 's>,
    phases: Query<'w, 's, &'static CleanupPhase, With<KeychainScope>>,
    failures: Query<'w, 's, &'static RecoveryFailure>,
    certificates: Query<'w, 's, &'static CertificateDirectory>,
}

#[derive(bevy_ecs::system::SystemParam)]
struct GuardLifetimes<'w, 's> {
    scopes: Query<'w, 's, &'static Scope, Without<Failed>>,
    states: Query<'w, 's, &'static OperationState>,
    flights: Query<'w, 's, (&'static Witness, &'static Flight)>,
}

#[derive(bevy_ecs::system::SystemParam)]
struct PublicationInputs<'w, 's> {
    observations: Query<'w, 's, &'static Observation>,
    processes: crate::process_observation::Facts<'w, 's>,
    contexts: Query<'w, 's, &'static Checkout>,
    staging: Query<'w, 's, &'static Staging>,
    publications: Query<'w, 's, &'static Published>,
    results: Query<
        'w,
        's,
        (Has<vmux_ecs::Ready>, Option<&'static publication::Failed>),
        With<publication::Drained>,
    >,
}

#[derive(bevy_ecs::system::SystemParam)]
struct Lifetimes<'w, 's> {
    observations: Query<'w, 's, &'static Observation>,
    processes: crate::process_observation::Facts<'w, 's>,
    cleanup: Query<'w, 's, &'static CleanupPhase>,
    flights: Query<'w, 's, (&'static Owner, &'static Flight)>,
    rows: Query<'w, 's, (Entity, &'static ChildOf)>,
}

impl Lifetimes<'_, '_> {
    fn drained(&self, entity: Entity, desktop: bool) -> bool {
        if self
            .flights
            .iter()
            .any(|(owner, flight)| owner.0 == entity && flight.pending())
        {
            return false;
        };
        if self
            .observations
            .get(entity)
            .is_ok_and(|observation| !self.processes.drained(observation.0))
        {
            return false;
        };
        if desktop
            && !self
                .cleanup
                .get(entity)
                .is_ok_and(|phase| *phase == CleanupPhase::Done)
        {
            return false;
        };
        true
    }
}

#[derive(bevy_ecs::system::SystemParam)]
struct ArtifactResults<'w, 's> {
    published: Query<'w, 's, &'static Published>,
    publications: Query<'w, 's, (), With<publication::Drained>>,
    destinations:
        Query<'w, 's, (&'static ChildOf, &'static publication::Destination), With<vmux_ecs::Ready>>,
}

#[derive(bevy_ecs::system::SystemParam)]
struct Outcomes<'w, 's> {
    desktops: Query<'w, 's, (), With<Desktop>>,
    recovery: Query<'w, 's, &'static RecoveryFailure>,
    keys: Keys<'w, 's>,
}

impl Outcomes<'_, '_> {
    fn result(
        &self,
        entity: Entity,
        error: Option<&Failed>,
        artifacts: Vec<PathBuf>,
    ) -> Result<Outcome, Failure> {
        if let Some(error) = error {
            let Ok(recovery) = self.recovery.get(entity) else {
                return Err(error.0.clone());
            };
            let path = match self.keys.directory(entity) {
                Some(directory) => {
                    directory
                        .0
                        .retention()
                        .store(true, std::sync::atomic::Ordering::Release);
                    directory.0.path().display().to_string()
                }
                None => String::new(),
            };
            return Err(error.0.clone().context(&format!(
                "Keychain cleanup failed: {}; retained {}",
                recovery.0, path
            )));
        }
        if self.desktops.contains(entity) {
            return Ok(Outcome::Artifacts(artifacts));
        }
        Ok(Outcome::Done)
    }
}

#[derive(bevy_ecs::system::SystemParam)]
struct Writer<'w, 's> {
    commands: Commands<'w, 's>,
}

impl Writer<'_, '_> {
    fn write(&mut self, entity: Entity, path: PathBuf, bytes: Vec<u8>) {
        self.request(
            entity,
            Purpose::new(PurposeKind::Copy),
            Write {
                path,
                bytes,
                mode: 0o644,
            },
        );
    }

    fn copy(&mut self, entity: Entity, source: PathBuf, destination: PathBuf) {
        self.request(
            entity,
            Purpose::new(PurposeKind::Copy),
            Copy {
                source,
                destination,
            },
        );
    }

    fn remove(&mut self, entity: Entity, checkout: &Checkout, settings: &Settings, path: PathBuf) {
        self.capture(
            entity,
            Purpose::new(PurposeKind::Copy),
            checkout,
            settings,
            "rm",
            vec![path.into_os_string()],
        );
    }

    fn labels(
        &mut self,
        entity: Entity,
        context: (&Checkout, &mut Settings),
        product: &crate::product::ProductMetadata,
        desktop: &Desktop,
        revision: &str,
        branch: Option<String>,
    ) {
        let (checkout, settings) = context;
        let name = if desktop.profile == "local" {
            format!("{} ({revision})", product.value("VMUX_HOST_NAME"))
        } else {
            product.value("VMUX_HOST_NAME").to_owned()
        };
        let identifier = if desktop.profile == "local" {
            format!("{}.{revision}", product.value("VMUX_HOST_BUNDLE_ID"))
        } else {
            product.value("VMUX_HOST_BUNDLE_ID").to_owned()
        };
        let title = if desktop.profile == "local" {
            format!(
                "{} ({})",
                product.value("VMUX_HOST_NAME"),
                branch
                    .filter(|branch| !branch.is_empty())
                    .unwrap_or_else(|| revision.to_owned())
            )
        } else {
            product.value("VMUX_HOST_NAME").to_owned()
        };
        settings.0.set("VMUX_GIT_HASH", revision);
        settings.0.set("VMUX_BUNDLE_ID", &identifier);
        settings.0.set("VMUX_BUILD_PROFILE", &desktop.profile);
        settings.0.set(
            "VMUX_CARGO_RELEASE_DIR",
            checkout.directory("release").as_os_str(),
        );
        settings
            .0
            .set("VMUX_TARGET_LOCK_TARGET", checkout.target.as_os_str());
        settings
            .0
            .set("VMUX_TARGET_LOCK_OWNER_PID", std::process::id().to_string());
        settings
            .0
            .set("VMUX_BUILD_STORAGE_TARGET", checkout.target.as_os_str());
        settings
            .0
            .set("VMUX_BUILD_STORAGE_ROOT", checkout.root.as_os_str());
        if let Some(binary) = settings.0.get("TAILWINDCSS_BIN").map(OsStr::to_owned) {
            settings.0.set("TAILWINDCSS", binary);
        }
        self.commands.entity(entity).insert((
            Name::new(name),
            Identifier(Some(identifier)),
            Title(title),
        ));
    }

    fn fail(&mut self, entity: Entity, failure: Failure) {
        self.commands.entity(entity).insert(Failed(failure));
    }

    fn request<J: StorageJob>(&mut self, root: Entity, purpose: Purpose, job: J) -> Entity {
        self.commands
            .spawn((
                Io,
                Owner(root),
                purpose,
                job,
                Operation::bundle("dx.package.io").unwrap(),
                ChildOf(root),
            ))
            .id()
    }

    fn capture(
        &mut self,
        root: Entity,
        purpose: Purpose,
        checkout: &Checkout,
        settings: &Settings,
        program: impl Into<OsString>,
        arguments: Vec<OsString>,
    ) -> Entity {
        self.request(
            root,
            purpose,
            Capture {
                environment: settings.0.clone(),
                directory: checkout.root.clone(),
                program: program.into(),
                arguments,
            },
        )
    }
}

struct IoPlugin<J: StorageJob>(std::marker::PhantomData<fn() -> J>);

impl<J: StorageJob> Default for IoPlugin<J> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<J: StorageJob> bevy_app::Plugin for IoPlugin<J> {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<Task<Scoped<J>>>::default())
            .add_systems(Update, collect::<J>.in_set(Stage::Collect))
            .add_systems(Update, submit::<J>.in_set(Stage::Submit));
    }
}

fn accept<T: Component + Clone>(
    event: On<Requested<T>>,
    existing: Query<(), With<Scope>>,
    mut commands: Commands,
) {
    if existing.contains(event.entity) {
        return;
    }
    commands.entity(event.entity).remove::<Arguments>();
    commands.entity(event.entity).insert((
        Scope {
            context: event.context,
            attempt: event.attempt,
        },
        Phase::Start,
        event.input.clone(),
    ));
}

fn check_outputs(
    outputs: Replies<ToolOutput>,
    failures: Query<(), With<Failed>>,
    optional: Query<(), With<Optional>>,
    mut commands: Commands,
) {
    for (entity, owner, _, output) in &outputs {
        if !output.0.status.success() && !optional.contains(entity) && !failures.contains(owner.0) {
            commands.entity(owner.0).insert(Failed(
                Failure::status(output.0.status)
                    .context(&String::from_utf8_lossy(&output.0.stderr)),
            ));
        }
    }
}

fn collect<J: StorageJob>(
    results: Returned<J>,
    inputs: Query<&AcceptedIo>,
    scopes: Query<&Scope>,
    failures: Query<(), With<Failed>>,
    optional: Query<(), With<Optional>>,
    mut commands: Commands,
) {
    for (entity, owner, parent, job_owner, result) in &results {
        let current = inputs.get(entity).is_ok_and(|input| {
            scopes
                .get(owner.0)
                .is_ok_and(|scope| scope.context == input.context && scope.attempt == input.attempt)
        });
        if !current || parent.parent() != owner.0 || !job_owner.accepts(Some(parent)) {
            commands
                .entity(entity)
                .insert((Received, OperationState::Canceled))
                .remove::<JobResult<Task<Scoped<J>>>>();
            continue;
        }
        match &result.0 {
            Ok(report) => match &report.result {
                Ok(value) => {
                    commands.entity(entity).insert(value.clone());
                }
                Err(error) => {
                    if !optional.contains(entity) && !failures.contains(owner.0) {
                        commands.entity(owner.0).insert(Failed(error.clone()));
                    }
                }
            },
            Err(error) => {
                if !failures.contains(owner.0) {
                    commands
                        .entity(owner.0)
                        .insert(Failed(Failure::message(error.clone())));
                }
            }
        }
        commands
            .entity(entity)
            .insert((Received, OperationState::Succeeded))
            .remove::<JobResult<Task<Scoped<J>>>>();
    }
}

fn submit<J: StorageJob>(
    pending: Queued<J>,
    loans: Loans,
    scopes: Query<&Scope>,
    failures: Query<(), With<Failed>>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    let Loans {
        staging,
        keys,
        certificates,
        targets: loans,
        ..
    } = loans;
    for (entity, owner, job) in &pending {
        let Ok(scope) = scopes.get(owner.0) else {
            continue;
        };
        if failures.contains(owner.0) {
            continue;
        };
        let loan = loans.get(scope.context).ok().map(|lease| lease.0.clone());
        let mut temporary = Vec::new();
        if let Ok(value) = staging.get(owner.0) {
            temporary.push(value.0.clone());
        }
        if let Some(value) = keys.directory(owner.0) {
            temporary.push(value.0.clone());
        }
        if let Ok(value) = certificates.get(owner.0) {
            temporary.push(value.0.clone());
        }
        let (task, flight) = Task::new(Scoped(job.clone()), loan, temporary, wake.0.clone());
        commands.spawn((
            Witness {
                owner: owner.0,
                context: scope.context,
            },
            flight.clone(),
            Name::new("Native package IO"),
            vmux_ecs::CreatedAt::now(),
        ));
        commands.entity(entity).insert((
            Submitted,
            AcceptedIo {
                context: scope.context,
                attempt: scope.attempt,
            },
            JobRequest(task),
            flight,
            OperationState::Running,
        ));
        wake.0.wake_by_ref();
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
#[require(CargoPhase)]
struct CargoPreparation;

#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
enum CargoPhase {
    #[default]
    Cache,
    Version,
    Compiler,
    Ready,
}

fn prepare_cargo(
    requests: CargoIncoming,
    mut contexts: Query<(&Checkout, &mut Settings)>,
    mut io: Writer,
) {
    for (entity, scope) in &requests {
        let Ok((checkout, mut settings)) = contexts.get_mut(scope.context) else {
            continue;
        };
        let result = (|| -> crate::Result<Option<PathBuf>> {
            settings
                .0
                .set("VMUX_TARGET_LOCK_TARGET", checkout.target.as_os_str());
            settings
                .0
                .set("VMUX_TARGET_LOCK_OWNER_PID", std::process::id().to_string());
            settings
                .0
                .set("VMUX_BUILD_STORAGE_TARGET", checkout.target.as_os_str());
            settings
                .0
                .set("VMUX_BUILD_STORAGE_ROOT", checkout.root.as_os_str());
            settings
                .0
                .set("VMUX_DX_BIN", std::env::current_exe()?.as_os_str());
            if let Some(binary) = settings.0.get("TAILWINDCSS_BIN").map(OsStr::to_owned) {
                settings.0.set("TAILWINDCSS", binary);
            }
            let cache = if let Some(path) = settings
                .0
                .get("VMUX_CEF_SDK_CACHE")
                .filter(|path| !path.is_empty())
            {
                Some(PathBuf::from(path))
            } else if settings.0.get("CI") != Some(OsStr::new("true")) {
                let home = PathBuf::from(settings.0.required("HOME")?);
                Some(if cfg!(target_os = "macos") {
                    home.join("Library/Caches/Vmux/cef-sdk")
                } else {
                    settings
                        .0
                        .get("XDG_CACHE_HOME")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| home.join(".cache"))
                        .join("vmux/cef-sdk")
                })
            } else {
                None
            };
            settings.0.remove("CEF_PATH");
            if let Some(cache) = &cache {
                settings.0.set("CEF_PATH", cache.as_os_str());
            }
            Ok(cache)
        })();
        match result {
            Ok(Some(cache)) => {
                io.request(
                    entity,
                    Purpose::new(PurposeKind::CargoCache),
                    CreateDirectory(cache),
                );
            }
            Ok(None) => {}
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
                continue;
            }
        }
        io.commands.entity(entity).insert(CargoPreparation);
    }
}

fn probe_cargo(
    mut requests: CargoRequests,
    contexts: Query<(&Checkout, &Settings)>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != CargoPhase::Cache || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };
        if settings.0.get("VMUX_DISABLE_SCCACHE") == Some(OsStr::new("1")) {
            *phase = CargoPhase::Ready;
            continue;
        };
        let receipt = io.capture(
            entity,
            Purpose::new(PurposeKind::CargoProbe),
            checkout,
            settings,
            "sccache",
            vec!["--version".into()],
        );
        io.commands.entity(receipt).insert(Optional);
        *phase = CargoPhase::Version;
    }
}

fn configure_cargo(
    mut requests: CargoRequests,
    mut contexts: Query<(&Checkout, &mut Settings)>,
    outputs: Replies<ToolOutput>,
    replies: Query<(Entity, &Owner, &Purpose), With<Received>>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(*phase, CargoPhase::Version | CargoPhase::Compiler)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let Ok((checkout, mut settings)) = contexts.get_mut(scope.context) else {
            continue;
        };
        let mut success = false;
        for (_, owner, purpose, output) in &outputs {
            if owner.0 == entity && purpose.is(PurposeKind::CargoProbe) {
                success = output.0.status.success();
            }
        }
        for (receipt, owner, purpose) in &replies {
            if owner.0 == entity && purpose.is(PurposeKind::CargoProbe) {
                io.commands.entity(receipt).despawn();
            }
        }
        if *phase == CargoPhase::Version && success {
            let compiler = settings
                .0
                .get("RUSTC")
                .unwrap_or(OsStr::new("rustc"))
                .to_owned();
            let receipt = io.capture(
                entity,
                Purpose::new(PurposeKind::CargoProbe),
                checkout,
                &settings,
                "sccache",
                vec![compiler, "-vV".into()],
            );
            io.commands.entity(receipt).insert(Optional);
            *phase = CargoPhase::Compiler;
        } else {
            if success {
                for name in ["CMAKE_C_COMPILER_LAUNCHER", "CMAKE_CXX_COMPILER_LAUNCHER"] {
                    if !settings.0.present(name) {
                        settings.0.set(name, "sccache");
                    }
                }
                if settings.0.get("CI") != Some(OsStr::new("true"))
                    && !settings.0.present("RUSTC_WRAPPER")
                {
                    settings.0.set("RUSTC_WRAPPER", "sccache");
                }
            }
            *phase = CargoPhase::Ready;
        }
    }
}

fn compile(
    mut requests: Requests,
    cargo: Query<&CargoPhase>,
    binaries: Query<(), With<Binaries>>,
    contexts: Query<(&Checkout, &Settings)>,
    outputs: Replies<ToolOutput>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if !cargo
            .get(entity)
            .is_ok_and(|phase| *phase == CargoPhase::Ready)
            || !binaries.contains(entity)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        }
        for (receipt, owner, purpose, output) in &outputs {
            if owner.0 == entity && purpose.is(PurposeKind::Compile) {
                if !output.0.status.success() {
                    io.fail(entity, Failure::status(output.0.status));
                    io.commands.entity(receipt).despawn();
                    *phase = Phase::Done;
                    continue;
                }
                *phase = match *phase {
                    Phase::Cli => Phase::Desktop,
                    Phase::Desktop => Phase::ServiceBuild,
                    Phase::ServiceBuild => Phase::Helper,
                    Phase::Helper => Phase::Web,
                    value => value,
                };
                io.commands.entity(receipt).despawn();
            }
        }
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };
        let mut environment = settings.0.clone();
        let mut arguments = match *phase {
            Phase::Start => {
                *phase = Phase::Cli;
                vec!["build", "-p", "vmux_cli", "-p", "vmux_vm", "--release"]
            }
            Phase::Desktop => vec!["build", "-p", "vmux_desktop", "--release"],
            Phase::ServiceBuild => vec![
                "build",
                "-p",
                "vmux_app",
                "--bin",
                "vmux_service",
                "--release",
                "--features",
                "service-daemon",
            ],
            Phase::Helper => vec![
                "build",
                "-p",
                "bevy_cef_debug_render_process",
                "--profile",
                "cef-helper",
            ],
            _ => continue,
        }
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
        if *phase == Phase::Desktop {
            if let Some(features) = environment.get("VMUX_DESKTOP_FEATURE_SET") {
                arguments.extend([
                    "--no-default-features".into(),
                    "--features".into(),
                    format!("package,{}", features.to_string_lossy()).into(),
                ]);
            } else {
                arguments.extend(["--features".into(), "vmux_desktop/package".into()]);
            }
        }
        environment.set(
            "CARGO_INCREMENTAL",
            settings.0.incremental(Some(OsStr::new("build"))),
        );
        let program = environment
            .get("CARGO_BIN")
            .unwrap_or(OsStr::new("cargo"))
            .to_owned();
        io.request(
            entity,
            Purpose::new(PurposeKind::Compile),
            Capture {
                environment,
                directory: checkout.root.clone(),
                program,
                arguments,
            },
        );
    }
}

fn prepare_web(
    mut requests: Requests,
    verifications: Query<&Verify>,
    webs: Query<&Web>,
    before: Query<&Before>,
    contexts: Query<&Checkout>,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase == Phase::Start
            && let Ok(verification) = verifications.get(entity)
        {
            if verification.app.as_os_str().is_empty() {
                io.fail(entity, Failure::message("Missing app bundle path"));
                continue;
            };
            io.commands
                .entity(entity)
                .insert(Bundle(verification.app.clone()));
            *phase = Phase::InspectBundle;
            continue;
        }
        let source = if *phase == Phase::Start {
            if let Ok(web) = webs.get(entity) {
                web.directory.clone()
            } else if let Ok(request) = before.get(entity) {
                if !request.force && request.format != "dmg" {
                    io.commands.entity(entity).insert(Complete);
                    continue;
                }
                let Ok(checkout) = contexts.get(scope.context) else {
                    continue;
                };
                if request.app.as_os_str().is_empty() {
                    io.fail(entity, Failure::message("Missing VMUX_APP_BUNDLE"));
                    continue;
                };
                if !request.app.is_absolute() || request.app.extension() != Some(OsStr::new("app"))
                {
                    io.fail(
                        entity,
                        Failure::message("Packaging requires an absolute app bundle path"),
                    );
                    continue;
                }
                io.commands
                    .entity(entity)
                    .insert(Bundle(request.app.clone()));
                checkout.root.join("crates/util/vmux_ui/dist")
            } else {
                continue;
            }
        } else if *phase == Phase::Web {
            let Ok(checkout) = contexts.get(scope.context) else {
                continue;
            };
            checkout.root.join("crates/util/vmux_ui/dist")
        } else {
            continue;
        };
        for name in [
            "assets/index.css",
            "assets/theme.css",
            "assets/favicons/tools.svg",
            "assets/tool-logos/acp.png",
            "assets/tool-logos/homebrew.svg",
            "assets/tool-logos/mcp.png",
            "assets/tool-logos/npm.svg",
        ] {
            io.request(
                entity,
                Purpose::new(PurposeKind::Required),
                Inspect(source.join(name)),
            );
        }
        io.request(
            entity,
            Purpose::new(PurposeKind::Stamp),
            ReadFile(source.join(".bundle-stamp")),
        );
        io.commands.entity(entity).insert(Source(source));
        *phase = Phase::Stamp;
    }
}

fn scan_web(mut requests: Requests, receipts: Receipts, sources: Query<&Source>, mut io: Writer) {
    let Receipts {
        paths: required,
        files: stamps,
        entries: _entries,
        pending,
        ..
    } = receipts;
    for (entity, _, mut phase) in &mut requests {
        if pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let Ok(source) = sources.get(entity) else {
            continue;
        };
        if *phase == Phase::Stamp {
            let mut valid = true;
            for (receipt, owner, purpose, metadata) in &required {
                if owner.0 != entity || !purpose.is(PurposeKind::Required) {
                    continue;
                };
                if !metadata.0.as_ref().is_some_and(|value| value.is_file()) {
                    io.fail(entity, Failure::message("Incomplete stylesheet bundle"));
                    valid = false;
                }
                io.commands.entity(receipt).despawn();
            }
            for (receipt, owner, purpose, bytes) in &stamps {
                if owner.0 != entity || !purpose.is(PurposeKind::Stamp) {
                    continue;
                };
                let mut expected = BTreeSet::new();
                if let Ok(text) = std::str::from_utf8(&bytes.0) {
                    for line in text.lines() {
                        let Some((hash, name)) = line.split_once("  ") else {
                            valid = false;
                            break;
                        };
                        if hash.len() != 64
                            || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                            || name.is_empty()
                            || PathBuf::from(name).is_absolute()
                            || name.split('/').any(|part| matches!(part, "" | "." | ".."))
                            || !expected.insert(name.to_owned())
                        {
                            valid = false;
                            break;
                        }
                        io.commands.spawn((
                            Expected(hash.into()),
                            Cwd(source.0.join(name)),
                            File,
                            Owner(entity),
                            ChildOf(entity),
                        ));
                    }
                } else {
                    valid = false
                };
                io.commands.entity(receipt).despawn();
            }
            if !valid {
                io.fail(
                    entity,
                    Failure::message("Invalid or unsafe stylesheet stamp"),
                );
                continue;
            }
            io.commands
                .spawn((Directory, Cwd(source.0.clone()), ChildOf(entity)));
            *phase = Phase::Scan;
            continue;
        }
    }
}

fn scan_directories(
    mut requests: Requests,
    receipts: Receipts,
    sources: Query<&Source>,
    directories: WebDirectories,
    mut io: Writer,
) {
    let Receipts {
        paths: _required,
        files: _stamps,
        entries,
        pending,
        ..
    } = receipts;
    for (entity, _, mut phase) in &mut requests {
        if pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let Ok(_source) = sources.get(entity) else {
            continue;
        };
        if *phase != Phase::Scan {
            continue;
        };
        let mut remaining = false;
        for (directory, path, parent) in &directories {
            if parent.parent() != entity {
                continue;
            };
            remaining = true;
            let mut data = None;
            for (receipt, owner, purpose, value) in &entries {
                if owner.0 == entity
                    && matches!(purpose, Purpose { kind: PurposeKind::Directory, entity: Some(id) } if *id == directory)
                {
                    data = Some(&value.0);
                    io.commands.entity(receipt).despawn();
                    break;
                }
            }
            let Some(data) = data else {
                io.request(
                    entity,
                    Purpose::targeted(PurposeKind::Directory, directory),
                    ReadDirectory(path.0.clone()),
                );
                continue;
            };
            for (path, metadata) in data {
                if metadata.is_dir() {
                    io.commands
                        .spawn((Directory, Cwd(path.clone()), ChildOf(entity)));
                } else if metadata.is_file()
                    && path.file_name() != Some(OsStr::new(".bundle-stamp"))
                {
                    io.commands
                        .spawn((Cwd(path.clone()), Owner(entity), Actual, ChildOf(entity)));
                } else if !metadata.is_file() {
                    io.fail(
                        entity,
                        Failure::message("Stylesheet bundle contains a non-regular entry"),
                    );
                }
            }
            io.commands.entity(directory).insert(Scanned);
        }
        if !remaining {
            *phase = Phase::Hash
        };
    }
}

#[derive(Component)]
struct Actual;

fn hash_web(
    mut requests: Requests,
    files: Query<(Entity, &Cwd, &Owner), With<File>>,
    mut io: Writer,
) {
    for (entity, _, mut phase) in &mut requests {
        if *phase != Phase::Hash {
            continue;
        };
        for (file, path, owner) in &files {
            if owner.0 == entity {
                io.request(
                    entity,
                    Purpose::targeted(PurposeKind::Hash, file),
                    Hash(path.0.clone()),
                );
            }
        }
        *phase = Phase::Verify;
    }
}

fn verify_web(
    mut requests: Requests,
    receipts: Receipts,
    files: Query<(Entity, &Cwd, &Owner, &Expected), With<File>>,
    actual: Query<(&Cwd, &Owner), With<Actual>>,
    binaries: Query<(), With<Binaries>>,
    before: Query<(), With<Before>>,
    mut commands: Commands,
) {
    let Receipts {
        hashes, pending, ..
    } = receipts;
    for (entity, _, mut phase) in &mut requests {
        if *phase != Phase::Verify || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let mut expected_paths = BTreeSet::new();
        let mut valid = true;
        for (file, path, owner, expected) in &files {
            if owner.0 != entity {
                continue;
            };
            expected_paths.insert(path.0.clone());
            let mut matched = false;
            for (receipt, owner, purpose, hash) in &hashes {
                if owner.0 == entity
                    && matches!(purpose, Purpose { kind: PurposeKind::Hash, entity: Some(id) } if *id == file)
                {
                    matched = hash.0 == expected.0;
                    commands.entity(receipt).despawn();
                    break;
                }
            }
            valid &= matched;
        }
        let mut actual_paths = BTreeSet::new();
        for (path, owner) in &actual {
            if owner.0 == entity {
                actual_paths.insert(path.0.clone());
            }
        }
        if !valid || actual_paths != expected_paths {
            commands.entity(entity).insert(Failed(Failure::message(
                "Stylesheet checksum or file coverage mismatch",
            )));
        } else if binaries.contains(entity) {
            *phase = Phase::Copy;
        } else if before.contains(entity) {
            *phase = Phase::Cef;
        } else {
            *phase = Phase::Done;
            commands.entity(entity).insert(Complete);
        }
    }
}

fn copy_binaries(
    mut requests: Requests,
    binaries: Query<(), With<Binaries>>,
    contexts: Query<(&Checkout, &Settings)>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if !binaries.contains(entity) {
            continue;
        };
        if *phase == Phase::Copy {
            let Ok((checkout, settings)) = contexts.get(scope.context) else {
                continue;
            };
            let product = match settings.0.product(&checkout.root) {
                Ok(product) => product,
                Err(error) => {
                    io.fail(entity, Failure::from_error(error));
                    continue;
                }
            };
            let release = checkout.directory("release");
            for (source, destination) in [
                (
                    checkout
                        .directory("cef-helper")
                        .join("bevy_cef_debug_render_process"),
                    release.join("bevy_cef_debug_render_process"),
                ),
                (
                    release.join("vmux_service"),
                    release.join(product.value("VMUX_HOST_SERVICE_NAME")),
                ),
            ] {
                io.copy(entity, source, destination);
            }
            *phase = Phase::Done;
        } else if *phase == Phase::Done && !pending.iter().any(|owner| owner.0 == entity) {
            io.commands.entity(entity).insert(Complete);
        }
    }
}

fn prepare_cef(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    frameworks: Query<&Framework>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(*phase, Phase::Cef | Phase::Embedded)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };
        let framework = if *phase == Phase::Embedded {
            let Ok(bundle) = bundles.get(entity) else {
                continue;
            };
            bundle
                .0
                .join("Contents/Frameworks/Chromium Embedded Framework.framework")
        } else if let Ok(framework) = frameworks.get(entity) {
            framework.0.clone()
        } else if let Some(path) = settings.0.get("CEF_FRAMEWORK_DIR") {
            PathBuf::from(path)
        } else if let Some(home) = settings.0.get("HOME") {
            PathBuf::from(home).join(".local/share/Chromium Embedded Framework.framework")
        } else {
            io.fail(entity, Failure::message("Missing HOME"));
            continue;
        };
        if *phase == Phase::Cef {
            io.commands
                .entity(entity)
                .insert(Framework(framework.clone()));
            io.request(
                entity,
                Purpose::new(PurposeKind::CefManifest),
                ReadFile(checkout.root.join("cef/manifest.toml")),
            );
        }
        let binary = framework.join("Chromium Embedded Framework");
        io.request(
            entity,
            Purpose::new(PurposeKind::CefBinary),
            Inspect(binary.clone()),
        );
        for (purpose, program, argument) in [
            (Purpose::new(PurposeKind::CefArch), "lipo", "-archs"),
            (Purpose::new(PurposeKind::CefVersion), "otool", "-L"),
            (Purpose::new(PurposeKind::CefExport), "nm", "-gU"),
        ] {
            io.capture(
                entity,
                purpose,
                checkout,
                settings,
                program,
                vec![argument.into(), binary.clone().into_os_string()],
            );
        }
        *phase = if *phase == Phase::Cef {
            Phase::ValidateSource
        } else {
            Phase::ValidateEmbedded
        };
    }
}

fn validate_cef(
    mut requests: Requests,
    receipts: Receipts,
    versions: Query<&CefVersion>,
    bundles: Query<&Bundle>,
    mut io: Writer,
) {
    let Receipts {
        files: manifests,
        paths: metadata,
        outputs,
        pending,
        ..
    } = receipts;
    for (entity, _, mut phase) in &mut requests {
        if !matches!(*phase, Phase::ValidateSource | Phase::ValidateEmbedded)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let mut expected = versions.get(entity).ok().map(|version| version.0.clone());
        if *phase == Phase::ValidateSource {
            for (receipt, owner, purpose, bytes) in &manifests {
                if owner.0 != entity || !purpose.is(PurposeKind::CefManifest) {
                    continue;
                };
                if let Some(version) = CefVersion::from_manifest(&bytes.0) {
                    expected = Some(version.0);
                }
                io.commands.entity(receipt).despawn();
            }
        }
        let mut executable = false;
        for (receipt, owner, purpose, value) in &metadata {
            if owner.0 != entity || !purpose.is(PurposeKind::CefBinary) {
                continue;
            };
            executable = value.0.as_ref().is_some_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            });
            io.commands.entity(receipt).despawn();
        }
        let mut arm64 = false;
        let mut version_matches = false;
        let mut exports = false;
        for (receipt, owner, purpose, output) in &outputs {
            if owner.0 != entity {
                continue;
            };
            let Ok(text) = std::str::from_utf8(&output.0.stdout) else {
                continue;
            };
            match purpose.kind {
                PurposeKind::CefArch => arm64 = text.split_whitespace().any(|arch| arch == "arm64"),
                PurposeKind::CefVersion => {
                    version_matches = expected.as_deref().is_some_and(|expected| {
                        text.split("compatibility version ")
                            .nth(1)
                            .and_then(|value| value.split(',').next())
                            == Some(expected)
                    })
                }
                PurposeKind::CefExport => {
                    exports = text.lines().any(|line| {
                        line.split_whitespace().last() == Some("_cef_set_os_crypt_keys")
                    })
                }
                _ => continue,
            }
            io.commands.entity(receipt).despawn();
        }
        if !executable || !arm64 || !version_matches || !exports {
            io.fail(
                entity,
                Failure::message("CEF binary, architecture, version or exports failed validation"),
            );
            continue;
        }
        if let Some(expected) = expected {
            io.commands.entity(entity).insert(CefVersion(expected));
        }
        if *phase == Phase::ValidateSource {
            *phase = Phase::InjectWeb;
        } else {
            let Ok(bundle) = bundles.get(entity) else {
                continue;
            };
            io.request(
                entity,
                Purpose::new(PurposeKind::Locales),
                ReadDirectory(
                    bundle.0.join(
                        "Contents/Frameworks/Chromium Embedded Framework.framework/Resources",
                    ),
                ),
            );
            *phase = Phase::Locales;
        }
    }
}

fn inject_web(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::InjectWeb || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        let web = bundle.0.join("Contents/Resources/webview-apps");
        io.capture(
            entity,
            Purpose::new(PurposeKind::Copy),
            checkout,
            settings,
            "rm",
            vec!["-rf".into(), web.clone().into_os_string()],
        );
        *phase = Phase::BundleCef;
    }
}

fn bundle_cef(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    frameworks: Query<&Framework>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(
            *phase,
            Phase::BundleCef | Phase::WebCopy | Phase::InspectEmbedded
        ) || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle), Ok(_framework)) = (
            contexts.get(scope.context),
            bundles.get(entity),
            frameworks.get(entity),
        ) else {
            continue;
        };
        if *phase == Phase::BundleCef {
            let web = bundle.0.join("Contents/Resources/webview-apps/_shared");
            io.capture(
                entity,
                Purpose::new(PurposeKind::Copy),
                checkout,
                settings,
                "mkdir",
                vec!["-p".into(), web.clone().into_os_string()],
            );
            *phase = Phase::WebCopy;
            continue;
        }
        if *phase == Phase::WebCopy {
            io.capture(
                entity,
                Purpose::new(PurposeKind::Copy),
                checkout,
                settings,
                "cp",
                vec![
                    "-R".into(),
                    checkout
                        .root
                        .join("crates/util/vmux_ui/dist/.")
                        .into_os_string(),
                    bundle
                        .0
                        .join("Contents/Resources/webview-apps/_shared")
                        .into_os_string(),
                ],
            );
            io.request(
                entity,
                Purpose::new(PurposeKind::Embedded),
                Inspect(
                    bundle
                        .0
                        .join("Contents/Frameworks/Chromium Embedded Framework.framework"),
                ),
            );
            *phase = Phase::InspectEmbedded;
            continue;
        }
    }
}

fn collect_cef(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    frameworks: Query<&Framework>,
    metadata: Replies<PathMetadata>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(
            *phase,
            Phase::BundleCef | Phase::WebCopy | Phase::InspectEmbedded
        ) || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle), Ok(framework)) = (
            contexts.get(scope.context),
            bundles.get(entity),
            frameworks.get(entity),
        ) else {
            continue;
        };
        let mut inspected = false;
        let mut embedded = false;
        for (receipt, owner, purpose, value) in &metadata {
            if owner.0 == entity && purpose.is(PurposeKind::Embedded) {
                inspected = true;
                embedded = value.0.as_ref().is_some_and(|value| value.is_dir());
                io.commands.entity(receipt).despawn();
            }
        }
        if !inspected {
            continue;
        };
        let verifier = match settings.0.required("VMUX_DX_BIN") {
            Ok(binary) => binary.to_owned(),
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
                continue;
            }
        };
        io.capture(
            entity,
            Purpose::new(PurposeKind::Copy),
            checkout,
            settings,
            verifier,
            vec![
                "internal".into(),
                "verify-web-bundle".into(),
                "--".into(),
                bundle
                    .0
                    .join("Contents/Resources/webview-apps/_shared")
                    .into_os_string(),
            ],
        );
        if !embedded {
            io.capture(
                entity,
                Purpose::new(PurposeKind::Copy),
                checkout,
                settings,
                settings
                    .0
                    .get("BEVY_CEF_BUNDLE_APP_BIN")
                    .unwrap_or(OsStr::new("bevy_cef_bundle_app"))
                    .to_owned(),
                vec![
                    "--app".into(),
                    bundle.0.clone().into_os_string(),
                    "--bundle-id-base".into(),
                    settings
                        .0
                        .get("VMUX_BUNDLE_ID")
                        .unwrap_or(OsStr::new("ai.vmux.desktop"))
                        .to_owned(),
                    "--bin-name".into(),
                    "vmux_desktop".into(),
                    "--cef-framework".into(),
                    framework.0.clone().into_os_string(),
                    "--helper-bin".into(),
                    checkout
                        .directory("release")
                        .join("bevy_cef_debug_render_process")
                        .into_os_string(),
                    "--no-sign".into(),
                ],
            );
        }
        *phase = Phase::Embedded;
    }
}

fn trim_cef(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    entries: Replies<DirectoryEntries>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::Locales || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        for (receipt, owner, purpose, value) in &entries {
            if owner.0 != entity || !purpose.is(PurposeKind::Locales) {
                continue;
            };
            let mut english = false;
            for (path, metadata) in &value.0 {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");
                english |= name == "en.lproj" && metadata.is_dir();
                if name.ends_with(".lproj")
                    && !matches!(name, "en.lproj" | "en_GB.lproj" | "Base.lproj")
                {
                    io.capture(
                        entity,
                        Purpose::new(PurposeKind::Copy),
                        checkout,
                        settings,
                        "rm",
                        vec!["-rf".into(), path.clone().into_os_string()],
                    );
                }
            }
            if !english {
                io.fail(entity, Failure::message("CEF English locale is missing"));
            }
            io.commands.entity(receipt).despawn();
        }
        io.request(
            entity,
            Purpose::new(PurposeKind::Helpers),
            ReadDirectory(bundle.0.join("Contents/Frameworks")),
        );
        *phase = Phase::Resources;
    }
}

fn resources(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    helpers: Replies<DirectoryEntries>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::Resources || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        let product = match settings.0.product(&checkout.root) {
            Ok(product) => product,
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
                continue;
            }
        };
        let destination = bundle.0.join("Contents/Resources");
        for (source, destination) in [
            (
                PathBuf::from(product.value("VMUX_HOST_ICON")),
                destination.join("Vmux.icns"),
            ),
            (
                PathBuf::from(product.value("VMUX_HOST_TRAY_ICON")),
                destination.join("tray.png"),
            ),
            (
                checkout.root.join("packaging/macos/crash_reporter.cfg"),
                destination.join("crash_reporter.cfg"),
            ),
        ] {
            io.copy(entity, source, destination);
        }
        for (receipt, owner, purpose, value) in &helpers {
            if owner.0 != entity || !purpose.is(PurposeKind::Helpers) {
                continue;
            };
            for (path, _) in &value.0 {
                if path.extension() != Some(OsStr::new("app")) {
                    continue;
                };
                io.copy(
                    entity,
                    product.value("VMUX_HOST_ICON").into(),
                    path.join("Contents/Resources/Vmux.icns"),
                );
                let helper = io
                    .commands
                    .spawn((
                        HelperPlist,
                        Cwd(path.join("Contents/Info.plist")),
                        ChildOf(entity),
                    ))
                    .id();
                io.request(
                    entity,
                    Purpose::targeted(PurposeKind::Plist, helper),
                    ReadFile(path.join("Contents/Info.plist")),
                );
            }
            io.commands.entity(receipt).despawn();
        }
        *phase = Phase::Service;
    }
}

#[derive(Component)]
struct HelperPlist;

fn embed_service(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    helper_paths: Query<&Cwd, With<HelperPlist>>,
    files: Replies<FileBytes>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::Service || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        for (receipt, owner, purpose, bytes) in &files {
            if owner.0 != entity {
                continue;
            };
            let Some(helper) = purpose.target(PurposeKind::Plist) else {
                continue;
            };
            let Ok(path) = helper_paths.get(*helper) else {
                continue;
            };
            let mut value = match plist::Value::from_reader(std::io::Cursor::new(&bytes.0)) {
                Ok(value) => value,
                Err(error) => {
                    io.fail(entity, Failure::message(error.to_string()));
                    continue;
                }
            };
            let Some(dictionary) = value.as_dictionary_mut() else {
                io.fail(entity, Failure::message("Invalid helper plist"));
                continue;
            };
            dictionary.insert(
                "CFBundleIconFile".into(),
                plist::Value::String("Vmux".into()),
            );
            let mut bytes = Vec::new();
            if let Err(error) = value.to_writer_xml(&mut bytes) {
                io.fail(entity, Failure::message(error.to_string()));
                continue;
            };
            io.write(entity, path.0.clone(), bytes);
            io.commands.entity(receipt).despawn();
        }
        let product = match settings.0.product(&checkout.root) {
            Ok(product) => product,
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
                continue;
            }
        };
        let source = bundle
            .0
            .join("Contents/MacOS")
            .join(product.value("VMUX_HOST_SERVICE_NAME"));
        io.request(entity, Purpose::new(PurposeKind::Service), Inspect(source));
        io.request(
            entity,
            Purpose::new(PurposeKind::Launch),
            ReadFile(checkout.root.join("packaging/macos/ai.vmux.service.plist")),
        );
        *phase = Phase::ServiceCopy;
    }
}

fn remove_service(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    metadata: Replies<PathMetadata>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::ServiceCopy || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        let product = match settings.0.product(&checkout.root) {
            Ok(product) => product,
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
                continue;
            }
        };
        let source = bundle
            .0
            .join("Contents/MacOS")
            .join(product.value("VMUX_HOST_SERVICE_NAME"));
        let destination = bundle
            .0
            .join("Contents/Library/LoginItems")
            .join(product.value("VMUX_HOST_SERVICE_BUNDLE"))
            .join("Contents/MacOS")
            .join(product.value("VMUX_HOST_SERVICE_NAME"));
        let mut inspected = false;
        for (receipt, owner, purpose, metadata) in &metadata {
            if owner.0 != entity || !purpose.is(PurposeKind::Service) {
                continue;
            };
            inspected = true;
            if metadata
                .0
                .as_ref()
                .is_some_and(|metadata| metadata.is_file())
            {
                io.copy(entity, source.clone(), destination.clone());
                io.commands.entity(entity).insert(RemoveService);
            } else {
                io.request(
                    entity,
                    Purpose::new(PurposeKind::Required),
                    Inspect(destination.clone()),
                );
            }
            io.commands.entity(receipt).despawn();
        }
        if inspected {
            *phase = Phase::ServiceConfig
        };
    }
}

#[derive(Component)]
struct RemoveService;

fn configure_service(
    mut requests: Requests,
    receipts: Receipts,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    removal: Query<(), With<RemoveService>>,
    mut io: Writer,
) {
    let Receipts {
        files,
        paths: metadata,
        pending,
        ..
    } = receipts;
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::ServiceConfig || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        let result = (|| -> crate::Result<()> {
            let product = settings.0.product(&checkout.root)?;
            let profile = settings
                .0
                .required("VMUX_BUILD_PROFILE")?
                .to_str()
                .ok_or("Invalid build profile")?;
            let hash = settings
                .0
                .get("VMUX_GIT_HASH")
                .unwrap_or(OsStr::new(""))
                .to_string_lossy();
            let label = if profile == "local" {
                format!("{}.{hash}", product.value("VMUX_HOST_SERVICE_ID"))
            } else if profile != "release" {
                format!("{}.{profile}", product.value("VMUX_HOST_SERVICE_ID"))
            } else {
                product.value("VMUX_HOST_SERVICE_ID").to_owned()
            };
            let service = bundle.service_contents(&product);
            if removal.contains(entity) {
                io.remove(
                    entity,
                    checkout,
                    settings,
                    bundle
                        .0
                        .join("Contents/MacOS")
                        .join(product.value("VMUX_HOST_SERVICE_NAME")),
                );
            }
            for (receipt, owner, purpose, metadata) in &metadata {
                if owner.0 == entity && purpose.is(PurposeKind::Required) {
                    if !metadata
                        .0
                        .as_ref()
                        .is_some_and(|metadata| metadata.is_file())
                    {
                        return Err("Missing packaged service executable".into());
                    };
                    io.commands.entity(receipt).despawn();
                }
            }
            let mut prepared = false;
            for (receipt, owner, purpose, bytes) in &files {
                if owner.0 != entity || !purpose.is(PurposeKind::Launch) {
                    continue;
                };
                let bundle_id = settings
                    .0
                    .get("VMUX_BUNDLE_ID")
                    .unwrap_or(OsStr::new("ai.vmux.desktop"))
                    .to_string_lossy();
                let (info_bytes, bytes) =
                    bytes.service_plists(&product, &label, profile, &bundle_id)?;
                io.write(entity, service.join("Info.plist"), info_bytes);
                io.copy(
                    entity,
                    product.value("VMUX_HOST_ICON").into(),
                    service.join("Resources/Vmux.icns"),
                );
                io.write(
                    entity,
                    bundle
                        .0
                        .join("Contents/Library/LaunchAgents")
                        .join(product.value("VMUX_HOST_AGENT_PLIST")),
                    bytes,
                );
                io.commands.entity(receipt).despawn();
                prepared = true;
            }
            if !prepared {
                return Err("Missing service launch configuration".into());
            }
            Ok(())
        })();
        if let Err(error) = result {
            io.fail(entity, Failure::from_error(error));
            continue;
        };
        *phase = Phase::SignScan;
    }
}

impl FileBytes {
    fn service_plists(
        &self,
        product: &crate::product::ProductMetadata,
        label: &str,
        profile: &str,
        bundle_id: &str,
    ) -> crate::Result<(Vec<u8>, Vec<u8>)> {
        let name = product.value("VMUX_HOST_SERVICE_NAME");
        let mut info = plist::Dictionary::new();
        for (key, value) in [
            ("CFBundleDevelopmentRegion", "en"),
            ("CFBundleExecutable", name),
            ("CFBundleName", name),
            ("CFBundleDisplayName", name),
            ("CFBundleIdentifier", label),
            ("CFBundleInfoDictionaryVersion", "6.0"),
            ("CFBundlePackageType", "APPL"),
            ("CFBundleIconFile", "Vmux"),
        ] {
            info.insert(key.into(), plist::Value::String(value.into()));
        }
        info.insert("LSUIElement".into(), plist::Value::Boolean(true));
        let mut info_bytes = Vec::new();
        plist::Value::Dictionary(info).to_writer_xml(&mut info_bytes)?;
        let mut launch = plist::Value::from_reader(std::io::Cursor::new(&self.0))?;
        let dictionary = launch
            .as_dictionary_mut()
            .ok_or("Invalid service launch plist")?;
        dictionary.insert("Label".into(), plist::Value::String(label.to_owned()));
        dictionary.insert(
            "BundleProgram".into(),
            plist::Value::String(format!(
                "Contents/Library/LoginItems/{}/Contents/MacOS/{}",
                product.value("VMUX_HOST_SERVICE_BUNDLE"),
                product.value("VMUX_HOST_SERVICE_NAME")
            )),
        );
        dictionary.insert(
            "AssociatedBundleIdentifiers".into(),
            plist::Value::Array(vec![plist::Value::String(bundle_id.to_owned())]),
        );
        dictionary
            .get_mut("EnvironmentVariables")
            .and_then(plist::Value::as_dictionary_mut)
            .ok_or("Missing launch environment")?
            .insert(
                "VMUX_BUILD_PROFILE".into(),
                plist::Value::String(profile.into()),
            );
        let log = plist::Value::String(format!(
            "/tmp/{}-service.log",
            product.value("VMUX_HOST_RUNTIME_STEM")
        ));
        dictionary.insert("StandardOutPath".into(), log.clone());
        dictionary.insert("StandardErrorPath".into(), log);
        let mut bytes = Vec::new();
        launch.to_writer_xml(&mut bytes)?;
        Ok((info_bytes, bytes))
    }
}

fn prepare_desktop(
    mut requests: Requests,
    cargo: Query<&CargoPhase>,
    desktops: Query<&Desktop>,
    contexts: Query<(&Checkout, &Settings)>,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::Start {
            continue;
        };
        let Ok(desktop) = desktops.get(entity) else {
            continue;
        };
        if !cargo
            .get(entity)
            .is_ok_and(|phase| *phase == CargoPhase::Ready)
        {
            continue;
        };
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };
        if !cfg!(target_os = "macos") {
            io.fail(entity, Failure::message("Desktop packaging requires macOS"));
            continue;
        };
        let fast = settings.0.get("VMUX_CI_FAST_PACKAGE") == Some(OsStr::new("1"));
        if fast {
            let event = settings.0.get("GITHUB_EVENT_NAME");
            let feature = event == Some(OsStr::new("push"))
                && settings.0.get("VMUX_CI_PREFLIGHT") == Some(OsStr::new("true"))
                && settings
                    .0
                    .get("GITHUB_REF")
                    .and_then(OsStr::to_str)
                    .and_then(|reference| reference.strip_prefix("refs/heads/"))
                    .is_some_and(|branch| {
                        !branch.is_empty()
                            && !matches!(branch, "main" | "master")
                            && !branch.starts_with("release/")
                    });
            if desktop.profile != "release"
                || settings.0.get("IS_RELEASE_PR") == Some(OsStr::new("1"))
                || !(event == Some(OsStr::new("pull_request")) || feature)
            {
                io.fail(entity, Failure::message("Fast package mode requires a non-release pull request or verified feature preflight"));
                continue;
            };
        }
        io.commands.entity(entity).insert(Fast(fast));
        io.request(
            entity,
            Purpose::new(PurposeKind::Metadata),
            ReadFile(checkout.root.join("Cargo.toml")),
        );
        io.request(
            entity,
            Purpose::new(PurposeKind::AppManifest),
            ReadFile(checkout.root.join("crates/app/vmux_desktop/Cargo.toml")),
        );
        io.capture(
            entity,
            Purpose::new(PurposeKind::Revision),
            checkout,
            settings,
            "git",
            vec!["rev-parse".into(), "--short=7".into(), "HEAD".into()],
        );
        let branch = io.capture(
            entity,
            Purpose::new(PurposeKind::Branch),
            checkout,
            settings,
            "git",
            vec![
                "symbolic-ref".into(),
                "--quiet".into(),
                "--short".into(),
                "HEAD".into(),
            ],
        );
        io.commands.entity(branch).insert(Optional);
        *phase = Phase::Metadata;
    }
}

fn configure_desktop(
    mut requests: Requests,
    receipts: Receipts,
    desktops: Query<&Desktop>,
    fast: Query<&Fast>,
    mut contexts: Query<(&Checkout, &mut Settings)>,
    mut io: Writer,
) {
    let Receipts {
        files,
        outputs,
        pending,
        ..
    } = receipts;
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::Metadata || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let Ok(desktop) = desktops.get(entity) else {
            continue;
        };
        let Ok((checkout, mut settings)) = contexts.get_mut(scope.context) else {
            continue;
        };
        let result = (|| -> crate::Result<()> {
            let mut revision = None;
            let mut branch = None;
            for (receipt, owner, purpose, output) in &outputs {
                if owner.0 != entity {
                    continue;
                };
                match purpose.kind {
                    PurposeKind::Revision => {
                        revision = Some(std::str::from_utf8(&output.0.stdout)?.trim().to_owned())
                    }
                    PurposeKind::Branch if output.0.status.success() => {
                        branch = Some(std::str::from_utf8(&output.0.stdout)?.trim().to_owned())
                    }
                    PurposeKind::Branch => {}
                    _ => continue,
                }
                io.commands.entity(receipt).despawn();
            }
            let revision = revision.ok_or("Missing Git revision")?;
            if revision.len() != 7 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("Invalid Git revision".into());
            };
            for (receipt, owner, purpose, bytes) in &files {
                if owner.0 != entity {
                    continue;
                };
                let table: toml::Table = std::str::from_utf8(&bytes.0)?.parse()?;
                match purpose.kind {
                    PurposeKind::Metadata => {
                        io.commands.entity(entity).insert(Version(
                            table
                                .get("workspace")
                                .and_then(|value| value.get("package"))
                                .and_then(|value| value.get("version"))
                                .and_then(toml::Value::as_str)
                                .ok_or("Missing workspace version")?
                                .into(),
                        ));
                    }
                    PurposeKind::AppManifest => {
                        io.commands.entity(entity).insert(Manifest(table));
                    }
                    _ => continue,
                }
                io.commands.entity(receipt).despawn();
            }
            let product = settings.0.product(&checkout.root)?;
            for (&key, value) in product.values() {
                settings.0.set(key, value.as_str());
            }
            if (desktop.profile == "local" || fast.get(entity).is_ok_and(|fast| fast.0))
                && !settings.0.present("SKIP_NOTARIZE")
            {
                settings.0.set("SKIP_NOTARIZE", "1");
            }
            io.labels(
                entity,
                (checkout, &mut settings),
                &product,
                desktop,
                &revision,
                branch,
            );
            settings
                .0
                .set("VMUX_DX_BIN", std::env::current_exe()?.as_os_str());
            io.commands.entity(entity).insert(CleanupPhase::Start);
            io.request(
                entity,
                Purpose::new(PurposeKind::Staging),
                CreateDirectory(checkout.directory("release")),
            );
            Ok(())
        })();
        if let Err(error) = result {
            io.fail(entity, Failure::from_error(error));
            continue;
        };
        *phase = Phase::Staging;
    }
}

fn create_staging(
    mut requests: Requests,
    receipts: Receipts,
    contexts: Query<(&Checkout, &Settings)>,
    names: Query<&Name>,
    mut io: Writer,
) {
    let Receipts {
        temporary: directories,
        outputs: _outputs,
        pending,
        ..
    } = receipts;
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(*phase, Phase::Staging) || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };

        let mut created = false;
        for (receipt, owner, purpose, directory) in &directories {
            if owner.0 != entity || !purpose.is(PurposeKind::Staging) {
                continue;
            };
            let Ok(name) = names.get(entity) else {
                continue;
            };
            io.commands.entity(entity).insert((
                Staging(directory.0.clone()),
                Bundle(directory.0.path().join(format!("{}.app", name.as_str()))),
            ));
            io.commands.entity(receipt).despawn();
            created = true;
        }
        if created {
            io.capture(
                entity,
                Purpose::new(PurposeKind::Keychains),
                checkout,
                settings,
                "security",
                vec!["list-keychains".into(), "-d".into(), "user".into()],
            );
            *phase = Phase::KeychainList;
        } else {
            io.request(
                entity,
                Purpose::new(PurposeKind::Staging),
                CreateTemporary {
                    parent: checkout.directory("release"),
                    prefix: ".vmux-package".into(),
                },
            );
        }
    }
}

fn provision_keychain(
    mut requests: Requests,
    receipts: Receipts,
    loans: Query<&Lease>,
    contexts: Query<(&Checkout, &Settings)>,
    mut io: Writer,
) {
    let Receipts {
        temporary: _directories,
        outputs,
        pending,
        ..
    } = receipts;
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(*phase, Phase::KeychainList) || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };

        for (receipt, owner, purpose, output) in &outputs {
            if owner.0 != entity || !purpose.is(PurposeKind::Keychains) {
                continue;
            };
            let guard = io
                .commands
                .spawn((
                    KeychainScope {
                        owner: entity,
                        context: scope.context,
                        attempt: scope.attempt,
                        directory: checkout.root.clone(),
                        environment: settings.0.clone(),
                        lease: loans.get(scope.context).ok().map(|lease| lease.0.clone()),
                    },
                    CleanupPhase::Start,
                    Name::new("Signing keychain cleanup"),
                    vmux_ecs::CreatedAt::now(),
                ))
                .id();
            io.commands.entity(entity).insert(GuardRef(guard));
            for (order, line) in String::from_utf8_lossy(&output.0.stdout)
                .lines()
                .enumerate()
            {
                let path = line.trim().trim_matches('"');
                if !path.is_empty() {
                    io.commands.spawn((
                        OriginalKeychain(path.into()),
                        Order(order as u32),
                        ChildOf(guard),
                    ));
                }
            }
            io.commands.entity(receipt).despawn();
        }
        if settings.0.present("APPLE_CERTIFICATE")
            || settings.0.present("APPLE_CERTIFICATE_PASSWORD")
        {
            io.request(
                entity,
                Purpose::new(PurposeKind::SigningDirectory),
                CreateTemporary {
                    parent: std::env::temp_dir(),
                    prefix: "vmux-build".into(),
                },
            );
            *phase = Phase::KeychainTemporary;
        } else {
            *phase = Phase::Packager
        };
    }
}

fn prepare_certificate(
    mut requests: Requests,
    receipts: Receipts,
    keys: Keys,
    contexts: Query<(&Checkout, &Settings)>,
    mut io: Writer,
) {
    let Receipts {
        temporary: directories,
        outputs,
        pending,
        ..
    } = receipts;
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(*phase, Phase::KeychainTemporary | Phase::KeychainPassword)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };
        if *phase == Phase::KeychainTemporary {
            for (receipt, owner, purpose, directory) in &directories {
                if owner.0 != entity || !purpose.is(PurposeKind::SigningDirectory) {
                    continue;
                };
                let Some(guard) = keys.entity(entity) else {
                    continue;
                };
                io.commands
                    .entity(guard)
                    .insert(SigningDirectory(directory.0.clone()));
                io.capture(
                    entity,
                    Purpose::new(PurposeKind::Password),
                    checkout,
                    settings,
                    "uuidgen",
                    Vec::new(),
                );
                io.commands.entity(receipt).despawn();
            }
            *phase = Phase::KeychainPassword;
        } else {
            for (receipt, owner, purpose, output) in &outputs {
                if owner.0 != entity || !purpose.is(PurposeKind::Password) {
                    continue;
                };
                io.commands.entity(entity).insert(Password(
                    String::from_utf8_lossy(&output.0.stdout).trim().into(),
                ));
                io.commands.entity(receipt).despawn();
                *phase = Phase::KeychainCreate;
            }
        }
    }
}

fn create_keychain(
    mut requests: Requests,
    receipts: Receipts,
    contexts: Query<(&Checkout, &Settings)>,
    keys: Keys,
    passwords: Query<&Password>,
    mut io: Writer,
) {
    let Receipts {
        outputs: _outputs,
        pending,
        ..
    } = receipts;
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(*phase, Phase::KeychainCreate | Phase::KeychainConfigure)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let (Ok((checkout, settings)), Ok(directory), Ok(password)) = (
            contexts.get(scope.context),
            keys.directory(entity).ok_or(()),
            passwords.get(entity),
        ) else {
            continue;
        };
        let path = directory.0.path().join("signing.keychain-db");
        let result = (|| -> crate::Result<Vec<OsString>> {
            match *phase {
                Phase::KeychainCreate => {
                    let certificate = STANDARD
                        .decode(settings.0.required("APPLE_CERTIFICATE")?.as_encoded_bytes())?;
                    settings.0.required("APPLE_CERTIFICATE_PASSWORD")?;
                    settings.0.required("APPLE_SIGNING_IDENTITY")?;
                    io.request(
                        entity,
                        Purpose::new(PurposeKind::Provision),
                        Write {
                            path: directory.0.path().join("cert.p12"),
                            bytes: certificate,
                            mode: 0o600,
                        },
                    );
                    io.commands
                        .entity(keys.entity(entity).unwrap())
                        .insert(Keychain(path.clone()));
                    *phase = Phase::KeychainConfigure;
                    Ok(vec![
                        "create-keychain".into(),
                        "-p".into(),
                        password.0.clone().into(),
                        path.into_os_string(),
                    ])
                }
                Phase::KeychainConfigure => {
                    *phase = Phase::KeychainUnlock;
                    Ok(vec![
                        "set-keychain-settings".into(),
                        "-lut".into(),
                        "21600".into(),
                        path.into_os_string(),
                    ])
                }
                _ => unreachable!(),
            }
        })();
        match result {
            Ok(arguments) => {
                io.capture(
                    entity,
                    if *phase == Phase::KeychainConfirm {
                        Purpose::new(PurposeKind::Identity)
                    } else {
                        Purpose::new(PurposeKind::Provision)
                    },
                    checkout,
                    settings,
                    "security",
                    arguments,
                );
            }
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
            }
        }
    }
}

fn configure_keychain(
    mut requests: Requests,
    receipts: Receipts,
    contexts: Query<(&Checkout, &Settings)>,
    keys: Keys,
    passwords: Query<&Password>,
    mut io: Writer,
) {
    let Receipts {
        outputs: _outputs,
        pending,
        ..
    } = receipts;
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(
            *phase,
            Phase::KeychainUnlock | Phase::KeychainImport | Phase::KeychainPartition
        ) || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let (Ok((checkout, settings)), Ok(directory), Ok(password)) = (
            contexts.get(scope.context),
            keys.directory(entity).ok_or(()),
            passwords.get(entity),
        ) else {
            continue;
        };
        let path = directory.0.path().join("signing.keychain-db");
        let result = (|| -> crate::Result<Vec<OsString>> {
            match *phase {
                Phase::KeychainUnlock => {
                    *phase = Phase::KeychainImport;
                    Ok(vec![
                        "unlock-keychain".into(),
                        "-p".into(),
                        password.0.clone().into(),
                        path.into_os_string(),
                    ])
                }
                Phase::KeychainImport => {
                    *phase = Phase::KeychainPartition;
                    Ok(vec![
                        "import".into(),
                        directory.0.path().join("cert.p12").into_os_string(),
                        "-P".into(),
                        settings
                            .0
                            .required("APPLE_CERTIFICATE_PASSWORD")?
                            .to_owned(),
                        "-A".into(),
                        "-f".into(),
                        "pkcs12".into(),
                        "-k".into(),
                        path.into_os_string(),
                    ])
                }
                Phase::KeychainPartition => {
                    *phase = Phase::KeychainActivate;
                    Ok(vec![
                        "set-key-partition-list".into(),
                        "-S".into(),
                        "apple-tool:,apple:".into(),
                        "-k".into(),
                        password.0.clone().into(),
                        path.into_os_string(),
                    ])
                }
                _ => unreachable!(),
            }
        })();
        match result {
            Ok(arguments) => {
                io.capture(
                    entity,
                    if *phase == Phase::KeychainConfirm {
                        Purpose::new(PurposeKind::Identity)
                    } else {
                        Purpose::new(PurposeKind::Provision)
                    },
                    checkout,
                    settings,
                    "security",
                    arguments,
                );
            }
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
            }
        }
    }
}

fn activate_keychain(
    mut requests: Requests,
    receipts: Receipts,
    contexts: Query<(&Checkout, &Settings)>,
    keys: Keys,
    passwords: Query<&Password>,
    originals: Query<(&OriginalKeychain, &Order, &ChildOf)>,
    mut io: Writer,
) {
    let Receipts {
        outputs: _outputs,
        pending,
        ..
    } = receipts;
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(*phase, Phase::KeychainActivate)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let (Ok((checkout, settings)), Ok(directory), Ok(_password)) = (
            contexts.get(scope.context),
            keys.directory(entity).ok_or(()),
            passwords.get(entity),
        ) else {
            continue;
        };
        let path = directory.0.path().join("signing.keychain-db");
        let result: crate::Result<Vec<OsString>> = {
            match *phase {
                Phase::KeychainActivate => {
                    *phase = Phase::KeychainIdentity;
                    let mut arguments = vec![
                        "list-keychains".into(),
                        "-d".into(),
                        "user".into(),
                        "-s".into(),
                        path.into_os_string(),
                    ];
                    let mut original = Vec::new();
                    for (keychain, order, parent) in &originals {
                        if Some(parent.parent()) == keys.entity(entity) {
                            original.push((order.0, keychain.0.clone()));
                        }
                    }
                    original.sort_by_key(|(order, _)| *order);
                    for (_, path) in original {
                        arguments.push(path);
                    }
                    Ok(arguments)
                }
                _ => unreachable!(),
            }
        };
        match result {
            Ok(arguments) => {
                io.capture(
                    entity,
                    if *phase == Phase::KeychainConfirm {
                        Purpose::new(PurposeKind::Identity)
                    } else {
                        Purpose::new(PurposeKind::Provision)
                    },
                    checkout,
                    settings,
                    "security",
                    arguments,
                );
            }
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
            }
        }
    }
}

fn confirm_keychain(
    mut requests: Requests,
    receipts: Receipts,
    contexts: Query<(&Checkout, &Settings)>,
    keys: Keys,
    passwords: Query<&Password>,
    mut io: Writer,
) {
    let Receipts {
        outputs, pending, ..
    } = receipts;
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(*phase, Phase::KeychainIdentity | Phase::KeychainConfirm)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let (Ok((checkout, settings)), Ok(directory), Ok(_password)) = (
            contexts.get(scope.context),
            keys.directory(entity).ok_or(()),
            passwords.get(entity),
        ) else {
            continue;
        };
        let path = directory.0.path().join("signing.keychain-db");
        let result: crate::Result<Vec<OsString>> = {
            if *phase == Phase::KeychainIdentity {
                *phase = Phase::KeychainConfirm;
            }
            Ok(vec![
                "find-identity".into(),
                "-v".into(),
                "-p".into(),
                "codesigning".into(),
                path.into_os_string(),
            ])
        };
        if *phase == Phase::KeychainConfirm {
            for (receipt, owner, purpose, output) in &outputs {
                if owner.0 != entity || !purpose.is(PurposeKind::Identity) {
                    continue;
                };
                let identity = settings
                    .0
                    .get("APPLE_SIGNING_IDENTITY")
                    .unwrap_or(OsStr::new(""))
                    .to_string_lossy();
                if !String::from_utf8_lossy(&output.0.stdout).contains(&format!("\"{identity}\"")) {
                    io.fail(
                        entity,
                        Failure::message(
                            "APPLE_SIGNING_IDENTITY does not match the imported identity",
                        ),
                    );
                } else {
                    *phase = Phase::Packager
                };
                io.commands.entity(receipt).despawn();
            }
            if *phase == Phase::Packager {
                continue;
            };
            if outputs.iter().any(|(_, owner, purpose, _)| {
                owner.0 == entity && purpose.is(PurposeKind::Identity)
            }) {
                continue;
            };
        }
        match result {
            Ok(arguments) => {
                io.capture(
                    entity,
                    if *phase == Phase::KeychainConfirm {
                        Purpose::new(PurposeKind::Identity)
                    } else {
                        Purpose::new(PurposeKind::Provision)
                    },
                    checkout,
                    settings,
                    "security",
                    arguments,
                );
            }
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
            }
        }
    }
}

fn execute_packager(
    mut requests: Requests,
    inputs: Inputs,
    prepared: Query<(), With<Arguments>>,
    keys: Keys,
    mut contexts: Query<(&Checkout, &mut Settings)>,
    mut io: Writer,
) {
    let Inputs {
        desktops,
        fast,
        manifests,
        versions,
        names,
        staging,
        bundles,
        ..
    } = inputs;
    for (entity, scope, phase) in &mut requests {
        if *phase != Phase::Packager || prepared.contains(entity) {
            continue;
        };
        let (
            Ok(desktop),
            Ok(manifest),
            Ok(version),
            Ok(name),
            Ok(staging),
            Ok(bundle),
            Ok((checkout, mut settings)),
        ) = (
            desktops.get(entity),
            manifests.get(entity),
            versions.get(entity),
            names.get(entity),
            staging.get(entity),
            bundles.get(entity),
            contexts.get_mut(scope.context),
        )
        else {
            continue;
        };
        let result = (|| -> crate::Result<Vec<OsString>> {
            let product = settings.0.product(&checkout.root)?;
            let mut config = manifest.configuration(&product)?;
            config["name"] = "vmux_desktop".into();
            config["product-name"] = name.as_str().into();
            config["identifier"] = settings
                .0
                .required("VMUX_BUNDLE_ID")?
                .to_string_lossy()
                .into_owned()
                .into();
            config["version"] = version.0.clone().into();
            config["out-dir"] = staging.0.path().to_string_lossy().into_owned().into();
            config["binaries-dir"] = checkout
                .directory("release")
                .to_string_lossy()
                .into_owned()
                .into();
            settings.0.set("VMUX_APP_BUNDLE", bundle.0.as_os_str());
            settings.0.set("APP_BUNDLE", bundle.0.as_os_str());
            if let Some(keychain) = keys.keychain(entity) {
                settings.0.set("CODESIGN_KEYCHAIN", keychain.0.as_os_str());
            }
            let mut arguments = vec![
                "packager".into(),
                "--config".into(),
                serde_json::to_string(&config)?.into(),
            ];
            if let Some(target) = settings.0.get("CARGO_BUILD_TARGET") {
                arguments.extend(["--target".into(), target.to_owned()]);
            }
            if desktop.profile == "local" || fast.get(entity).is_ok_and(|fast| fast.0) {
                arguments.extend(["--formats".into(), "app".into()]);
            }
            Ok(arguments)
        })();
        match result {
            Ok(arguments) => {
                io.commands.entity(entity).insert(Arguments(arguments));
            }
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
            }
        }
    }
}

#[derive(Component)]
struct Arguments(Vec<OsString>);

fn run_packager(
    mut requests: Requests,
    prepared: Query<&Arguments>,
    contexts: Query<(&Checkout, &Settings)>,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::Packager {
            continue;
        }
        let Ok(arguments) = prepared.get(entity) else {
            continue;
        };
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };
        io.capture(
            entity,
            Purpose::new(PurposeKind::Packager),
            checkout,
            settings,
            settings
                .0
                .get("CARGO_BIN")
                .unwrap_or(OsStr::new("cargo"))
                .to_owned(),
            arguments.0.clone(),
        );
        *phase = Phase::Packaged;
    }
}

fn title_desktop(
    mut requests: Requests,
    bundles: Query<&Bundle>,
    titles: Query<&Title>,
    files: Replies<FileBytes>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, _, mut phase) in &mut requests {
        if !matches!(*phase, Phase::Packaged | Phase::Title)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let Ok(bundle) = bundles.get(entity) else {
            continue;
        };
        if *phase == Phase::Packaged {
            io.request(
                entity,
                Purpose::new(PurposeKind::AppPlist),
                ReadFile(bundle.0.join("Contents/Info.plist")),
            );
            *phase = Phase::Title;
            continue;
        }
        let Ok(title) = titles.get(entity) else {
            continue;
        };
        for (receipt, owner, purpose, bytes) in &files {
            if owner.0 != entity || !purpose.is(PurposeKind::AppPlist) {
                continue;
            };
            let result = (|| -> crate::Result<Vec<u8>> {
                let mut value = plist::Value::from_reader(std::io::Cursor::new(&bytes.0))?;
                let dictionary = value
                    .as_dictionary_mut()
                    .ok_or("Invalid application plist")?;
                for key in ["CFBundleName", "CFBundleDisplayName"] {
                    dictionary.insert(key.into(), plist::Value::String(title.0.clone()));
                }
                let mut bytes = Vec::new();
                value.to_writer_xml(&mut bytes)?;
                Ok(bytes)
            })();
            match result {
                Ok(bytes) => {
                    io.write(entity, bundle.0.join("Contents/Info.plist"), bytes);
                    *phase = Phase::Postprocess;
                }
                Err(error) => {
                    io.fail(entity, Failure::from_error(error));
                }
            }
            io.commands.entity(receipt).despawn();
        }
    }
}

fn postprocess_desktop(
    mut requests: Requests,
    desktops: Query<&Desktop>,
    fast: Query<&Fast>,
    contexts: Query<(&Checkout, &Settings)>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::Postprocess || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok(desktop), Ok((checkout, settings))) =
            (desktops.get(entity), contexts.get(scope.context))
        else {
            continue;
        };
        if desktop.profile == "local" || fast.get(entity).is_ok_and(|fast| fast.0) {
            let program = match settings.0.required("VMUX_DX_BIN") {
                Ok(program) => program.to_owned(),
                Err(error) => {
                    io.fail(entity, Failure::from_error(error));
                    continue;
                }
            };
            io.capture(
                entity,
                Purpose::new(PurposeKind::Postprocess),
                checkout,
                settings,
                program,
                vec!["internal".into(), "finalize-package".into()],
            );
        }
        *phase = Phase::InspectBundle;
    }
}

fn inspect_desktop(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::InspectBundle || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        let product = match settings.0.product(&checkout.root) {
            Ok(product) => product,
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
                continue;
            }
        };
        let mut required = vec!["Contents/MacOS/vmux_vm".into(), "Contents/MacOS/vmux_desktop".into(), "Contents/MacOS/vmux".into(), "Contents/Frameworks/vmux_desktop Helper.app/Contents/MacOS/vmux_desktop Helper".into(), "Contents/Frameworks/vmux_desktop Helper.app/Contents/Resources/Vmux.icns".into(), "Contents/Frameworks/vmux_desktop Helper (Renderer).app/Contents/MacOS/vmux_desktop Helper (Renderer)".into(), "Contents/Info.plist".into(), "Contents/Resources/Vmux.icns".into(), "Contents/Resources/crash_reporter.cfg".into()];
        required.extend([
            format!(
                "Contents/Library/LoginItems/{}/Contents/MacOS/{}",
                product.value("VMUX_HOST_SERVICE_BUNDLE"),
                product.value("VMUX_HOST_SERVICE_NAME")
            ),
            format!(
                "Contents/Library/LoginItems/{}/Contents/Resources/Vmux.icns",
                product.value("VMUX_HOST_SERVICE_BUNDLE")
            ),
            format!(
                "Contents/Library/LaunchAgents/{}",
                product.value("VMUX_HOST_AGENT_PLIST")
            ),
        ]);
        for path in required {
            io.request(
                entity,
                Purpose::new(PurposeKind::BundleRequired),
                Inspect(bundle.0.join(path)),
            );
        }
        for path in [
            "Contents/MacOS/vmux_service",
            "Contents/Frameworks/Vmux Helper.app",
        ] {
            io.request(
                entity,
                Purpose::new(PurposeKind::BundleForbidden),
                Inspect(bundle.0.join(path)),
            );
        }
        io.request(
            entity,
            Purpose::new(PurposeKind::BundleLaunch),
            ReadFile(
                bundle
                    .0
                    .join("Contents/Library/LaunchAgents")
                    .join(product.value("VMUX_HOST_AGENT_PLIST")),
            ),
        );
        io.request(
            entity,
            Purpose::new(PurposeKind::CliHash),
            Hash(bundle.0.join("Contents/MacOS/vmux")),
        );
        io.request(
            entity,
            Purpose::new(PurposeKind::DesktopHash),
            Hash(bundle.0.join("Contents/MacOS/vmux_desktop")),
        );
        *phase = Phase::VerifyBundle;
    }
}

fn verify_desktop(
    mut requests: Requests,
    receipts: Receipts,
    verifications: Query<(), With<Verify>>,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    mut io: Writer,
) {
    let Receipts {
        paths,
        files,
        hashes,
        pending,
        ..
    } = receipts;
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::VerifyBundle || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        let mut valid = true;
        for (receipt, owner, purpose, path) in &paths {
            if owner.0 != entity {
                continue;
            };
            match purpose.kind {
                PurposeKind::BundleRequired => valid &= path.0.is_some(),
                PurposeKind::BundleForbidden => valid &= path.0.is_none(),
                _ => continue,
            };
            io.commands.entity(receipt).despawn();
        }
        for (receipt, owner, purpose, bytes) in &files {
            if owner.0 != entity || !purpose.is(PurposeKind::BundleLaunch) {
                continue;
            };
            valid &= !bytes.0.windows(11).any(|window| window == b"{{PROFILE}}");
            io.commands.entity(receipt).despawn();
        }
        let mut cli = None;
        let mut desktop = None;
        for (receipt, owner, purpose, hash) in &hashes {
            if owner.0 != entity {
                continue;
            };
            match purpose.kind {
                PurposeKind::CliHash => cli = Some(&hash.0),
                PurposeKind::DesktopHash => desktop = Some(&hash.0),
                _ => continue,
            };
            io.commands.entity(receipt).despawn();
        }
        if !valid || cli.is_none() || desktop.is_none() || cli == desktop {
            io.fail(
                entity,
                Failure::message(
                    "Package resources, LaunchAgent or executable identities failed validation",
                ),
            );
            continue;
        };
        if verifications.contains(entity) {
            io.commands.entity(entity).insert(Complete);
            continue;
        };
        io.capture(
            entity,
            Purpose::new(PurposeKind::VerifyBundle),
            checkout,
            settings,
            "codesign",
            vec![
                "--verify".into(),
                "--deep".into(),
                "--strict".into(),
                bundle.0.clone().into_os_string(),
            ],
        );
        io.request(
            entity,
            Purpose::new(PurposeKind::CertificateDirectory),
            CreateTemporary {
                parent: std::env::temp_dir(),
                prefix: "vmux-signature-check".into(),
            },
        );
        *phase = Phase::Certificate;
    }
}

fn extract_certificate(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    directories: Replies<TemporaryDirectory>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::Certificate || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        }
        let (Ok((_checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        for (receipt, owner, purpose, directory) in &directories {
            if owner.0 != entity || !purpose.is(PurposeKind::CertificateDirectory) {
                continue;
            };
            io.commands
                .entity(entity)
                .insert(CertificateDirectory(directory.0.clone()));
            io.request(
                entity,
                Purpose::new(PurposeKind::Certificate),
                Capture {
                    environment: settings.0.clone(),
                    directory: directory.0.path().into(),
                    program: "codesign".into(),
                    arguments: vec![
                        "--display".into(),
                        "--extract-certificates".into(),
                        bundle.0.join("Contents/MacOS/vmux").into_os_string(),
                    ],
                },
            );
            io.commands.entity(receipt).despawn();
        }
        *phase = Phase::Extracted;
    }
}

fn cleanup_keychain(
    requests: Query<(Entity, &Scope, Option<&Failed>, &Phase), With<Desktop>>,
    cleanup: CleanupInputs,
    receipts: Receipts,
    flights: Query<(&Witness, &Flight)>,
    mut io: Writer,
) {
    let CleanupInputs {
        keys,
        phases: guard_phases,
        failures,
        certificates,
        ..
    } = cleanup;
    let Receipts { paths, pending, .. } = receipts;
    for (entity, _, failure, phase) in &requests {
        if failure.is_none() && *phase != Phase::Extracted {
            continue;
        };
        if flights
            .iter()
            .any(|(witness, flight)| witness.owner == entity && flight.pending())
        {
            continue;
        };
        if pending.iter().any(|owner| owner.0 == entity) && failure.is_none() {
            continue;
        };
        if *phase == Phase::Extracted && failure.is_none() {
            let Ok(directory) = certificates.get(entity) else {
                continue;
            };
            let mut checked = false;
            for (receipt, owner, purpose, metadata) in &paths {
                if owner.0 != entity || !purpose.is(PurposeKind::Certificate) {
                    continue;
                };
                checked = true;
                if !metadata
                    .0
                    .as_ref()
                    .is_some_and(|metadata| metadata.is_file() && metadata.len() > 0)
                {
                    io.fail(
                        entity,
                        Failure::message("Packaged CLI has no signing certificate"),
                    );
                }
                io.commands.entity(receipt).despawn();
            }
            if !checked
                && !keys.entity(entity).is_some_and(|guard| {
                    guard_phases
                        .get(guard)
                        .is_ok_and(|phase| *phase != CleanupPhase::Start)
                })
            {
                io.request(
                    entity,
                    Purpose::new(PurposeKind::Certificate),
                    Inspect(directory.0.path().join("codesign0")),
                );
                continue;
            }
        }
        let Some(guard) = keys.entity(entity) else {
            io.commands.entity(entity).insert(CleanupPhase::Done);
            continue;
        };
        if guard_phases
            .get(guard)
            .is_ok_and(|phase| *phase == CleanupPhase::Done)
        {
            if let Ok(error) = failures.get(guard) {
                io.commands
                    .entity(entity)
                    .insert(RecoveryFailure(error.0.clone()));
                if failure.is_none() {
                    io.fail(entity, error.0.clone());
                }
            } else if failure.is_none() {
                io.commands.entity(entity).insert(Phase::Publication);
            }
            io.commands.entity(entity).insert(CleanupPhase::Done);
        } else {
            io.commands.entity(guard).insert(ReleaseKeychain);
        }
    }
}

type GuardReplies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GuardIo,
        &'static ChildOf,
        &'static JobOwner<Task<Scoped<CleanupCapture>>>,
        &'static JobResult<Task<Scoped<CleanupCapture>>>,
    ),
    Without<Received>,
>;

fn collect_guards(results: GuardReplies, mut commands: Commands) {
    for (entity, guard, parent, owner, result) in &results {
        if parent.parent() != guard.0 || !owner.accepts(Some(parent)) {
            continue;
        };
        let result = match &result.0 {
            Ok(report) => report.result.clone(),
            Err(error) => Err(Failure::message(error.clone())),
        };
        let failure = match result {
            Ok(output) if output.0.status.success() => None,
            Ok(output) => Some(Failure::status(output.0.status)),
            Err(error) => Some(error),
        };
        if let Some(error) = failure {
            commands.entity(guard.0).insert(RecoveryFailure(error));
        }
        commands
            .entity(entity)
            .insert(Received)
            .remove::<JobResult<Task<Scoped<CleanupCapture>>>>();
    }
}

fn cleanup_guards(
    mut guards: Query<(
        Entity,
        &KeychainScope,
        &mut CleanupPhase,
        Has<ReleaseKeychain>,
    )>,
    lifetimes: GuardLifetimes,
    originals: Query<(&OriginalKeychain, &Order, &ChildOf)>,
    keys: Query<&Keychain>,
    replies: Query<(&GuardIo, &Purpose), With<Received>>,
    mut commands: Commands,
) {
    let GuardLifetimes {
        scopes,
        states,
        flights,
        ..
    } = lifetimes;
    for (entity, scope, mut phase, released) in &mut guards {
        let live = scopes
            .get(scope.owner)
            .is_ok_and(|owner| owner.context == scope.context && owner.attempt == scope.attempt)
            && states
                .get(scope.context)
                .is_ok_and(|state| *state == OperationState::Running)
            && Signals::pending().is_none();
        if *phase == CleanupPhase::Start && live && !released {
            continue;
        };
        if flights
            .iter()
            .any(|(witness, flight)| witness.owner == scope.owner && flight.pending())
        {
            continue;
        };
        let arguments =
            match *phase {
                CleanupPhase::Start => {
                    let mut values = Vec::new();
                    for (path, order, parent) in &originals {
                        if parent.parent() == entity {
                            values.push((order.0, path.0.clone()));
                        }
                    }
                    values.sort_by_key(|(order, _)| *order);
                    let mut arguments = vec![
                        "list-keychains".into(),
                        "-d".into(),
                        "user".into(),
                        "-s".into(),
                    ];
                    for (_, path) in values {
                        arguments.push(path);
                    }
                    *phase = CleanupPhase::Restore;
                    Some((Purpose::new(PurposeKind::Restore), arguments))
                }
                CleanupPhase::Restore => {
                    if !replies.iter().any(|(guard, purpose)| {
                        guard.0 == entity && purpose.is(PurposeKind::Restore)
                    }) {
                        continue;
                    };
                    if let Ok(path) = keys.get(entity) {
                        *phase = CleanupPhase::Delete;
                        Some((
                            Purpose::new(PurposeKind::Delete),
                            vec!["delete-keychain".into(), path.0.clone().into_os_string()],
                        ))
                    } else {
                        *phase = CleanupPhase::Done;
                        None
                    }
                }
                CleanupPhase::Delete => {
                    if replies.iter().any(|(guard, purpose)| {
                        guard.0 == entity && purpose.is(PurposeKind::Delete)
                    }) {
                        *phase = CleanupPhase::Done
                    };
                    None
                }
                CleanupPhase::Done => None,
            };
        if let Some((purpose, arguments)) = arguments {
            commands.spawn((
                Operation::bundle("dx.keychain.cleanup").unwrap(),
                GuardIo(entity),
                purpose,
                CleanupCapture {
                    environment: scope.environment.clone(),
                    directory: scope.directory.clone(),
                    arguments,
                },
                ChildOf(entity),
            ));
        }
    }
}

fn submit_guards(
    queued: Query<(Entity, &GuardIo, &CleanupCapture), Without<Submitted>>,
    guards: Query<&KeychainScope>,
    temporary: Query<&SigningDirectory>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, guard, job) in &queued {
        let Ok(scope) = guards.get(guard.0) else {
            continue;
        };
        let temporary = temporary
            .get(guard.0)
            .ok()
            .map(|directory| directory.0.clone())
            .into_iter()
            .collect();
        let (task, flight) = Task::new(
            Scoped(job.clone()),
            scope.lease.clone(),
            temporary,
            wake.0.clone(),
        );
        commands.spawn((
            Witness {
                owner: scope.owner,
                context: scope.context,
            },
            flight.clone(),
            Name::new("Signing cleanup IO"),
            vmux_ecs::CreatedAt::now(),
        ));
        commands.entity(entity).insert((
            Submitted,
            JobRequest(task),
            flight,
            OperationState::Running,
        ));
        wake.0.wake_by_ref();
    }
}

fn retire_guards(
    guards: RetiredGuards,
    scopes: Query<(), With<Scope>>,
    completed: Query<(), With<crate::Completed>>,
    states: Query<&OperationState>,
    flights: Query<(&Witness, &Flight)>,
    mut commands: Commands,
) {
    for (entity, scope, phase, failure, directory) in &guards {
        if *phase != CleanupPhase::Done
            || scopes.contains(scope.owner)
            || flights
                .iter()
                .any(|(witness, flight)| witness.owner == scope.owner && flight.pending())
        {
            continue;
        };
        if !completed.contains(scope.owner) {
            let mut error = "Development step owner disappeared".to_owned();
            if let Some(failure) = failure {
                let path = directory
                    .map(|directory| {
                        directory
                            .0
                            .retention()
                            .store(true, std::sync::atomic::Ordering::Release);
                        directory.0.path().display().to_string()
                    })
                    .unwrap_or_default();
                error = format!(
                    "{error}; keychain cleanup failed: {}; retained {path}",
                    failure.0
                );
            }
            if let Ok(mut owner) = commands.get_entity(scope.owner) {
                owner
                    .remove::<crate::Running>()
                    .insert((crate::Completed, OperationState::Failed));
            }
            if let Ok(state) = states.get(scope.context) {
                if *state != OperationState::Canceled {
                    commands
                        .entity(scope.context)
                        .insert(OperationState::Failed);
                }
                commands
                    .entity(scope.context)
                    .insert(crate::Retirement(Failure::message(error)));
            }
        }
        commands.entity(entity).despawn();
    }
}

fn forget_witnesses(
    witnesses: Query<(Entity, &Witness, &Flight)>,
    scopes: Query<(), With<Scope>>,
    completed: Query<(), With<crate::Completed>>,
    guards: Query<&KeychainScope>,
    states: Query<&OperationState>,
    mut commands: Commands,
) {
    for (entity, witness, flight) in &witnesses {
        if flight.pending() {
            continue;
        };
        if !scopes.contains(witness.owner)
            && !completed.contains(witness.owner)
            && !guards.iter().any(|guard| guard.owner == witness.owner)
        {
            if let Ok(state) = states.get(witness.context) {
                if *state != OperationState::Canceled {
                    commands
                        .entity(witness.context)
                        .insert(OperationState::Failed);
                }
                commands
                    .entity(witness.context)
                    .insert(crate::Retirement(Failure::message(
                        "Development step owner disappeared",
                    )));
            }
            if let Ok(mut owner) = commands.get_entity(witness.owner) {
                owner
                    .remove::<crate::Running>()
                    .insert((crate::Completed, OperationState::Failed));
            }
        }
        commands.entity(entity).despawn();
    }
}

fn observe_publication(
    mut requests: Requests,
    artifacts: PublicationInputs,
    pending: Pending,
    desktops: Query<(), With<Desktop>>,
    mut io: Writer,
) {
    let PublicationInputs {
        observations,
        processes,
        publications,
        results,
        ..
    } = artifacts;
    for (entity, scope, phase) in &mut requests {
        if *phase != Phase::Publication
            || !desktops.contains(entity)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        if let Ok(observation) = observations.get(entity) {
            if let Some(error) = processes.error(observation.0) {
                io.fail(entity, error.clone());
                continue;
            };
            if !processes.ready(observation.0) {
                continue;
            };
        } else {
            let observation = io
                .commands
                .spawn((
                    crate::process_observation::Request {
                        context: scope.context,
                        attempt: scope.attempt,
                    },
                    ChildOf(entity),
                ))
                .id();
            io.commands.entity(entity).insert(Observation(observation));
            continue;
        }
        if let Ok(publication) = publications.get(entity)
            && let Ok((ready, failure)) = results.get(publication.0)
        {
            if let Some(failure) = failure {
                io.fail(entity, failure.0.clone());
            } else if ready {
                io.commands.entity(entity).insert(Complete);
            }
        }
    }
}

fn publish_desktop(
    mut requests: Requests,
    artifacts: PublicationInputs,
    receipts: Receipts,
    loans: Query<&Lease>,
    desktops: Query<(), With<Desktop>>,
    mut io: Writer,
) {
    let PublicationInputs {
        observations,
        processes,
        contexts,
        staging,
        publications,
        ..
    } = artifacts;
    let Receipts {
        entries, pending, ..
    } = receipts;
    for (entity, scope, phase) in &mut requests {
        if *phase != Phase::Publication
            || !desktops.contains(entity)
            || publications.contains(entity)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        }
        let Ok(observation) = observations.get(entity) else {
            continue;
        };
        if !processes.ready(observation.0) {
            continue;
        }
        let (Ok(checkout), Ok(staging)) = (contexts.get(scope.context), staging.get(entity)) else {
            continue;
        };
        let mut listed = false;
        for (receipt, owner, purpose, entries) in &entries {
            if owner.0 != entity || !purpose.is(PurposeKind::Staging) {
                continue;
            };
            listed = true;
            if entries.0.iter().any(|(path, _)| {
                path.file_name().is_some_and(|name| {
                    processes.uses(observation.0, &checkout.directory("release").join(name))
                })
            }) {
                io.fail(
                    entity,
                    Failure::message(
                        "An artifact being replaced is running; quit that build first",
                    ),
                );
                continue;
            }
            let publication = io
                .commands
                .spawn((publication::Request {
                    owner: entity,
                    lease: loans.get(scope.context).ok().map(|lease| lease.0.clone()),
                    context: scope.context,
                    attempt: scope.attempt,
                    staging: staging.0.clone(),
                },))
                .id();
            for (order, (path, _)) in entries.0.iter().enumerate() {
                let Some(name) = path.file_name() else {
                    continue;
                };
                if name == ".cargo-packager" {
                    continue;
                };
                io.commands.spawn((
                    publication::Move,
                    Cwd(path.clone()),
                    publication::Destination(checkout.directory("release").join(name)),
                    Order(order as u32),
                    ChildOf(publication),
                ));
            }
            io.commands.entity(entity).insert(Published(publication));
            io.commands.entity(receipt).despawn();
        }
        if !listed {
            io.request(
                entity,
                Purpose::new(PurposeKind::Staging),
                ReadDirectory(staging.0.path().into()),
            );
        }
    }
}

fn scan_signatures(
    mut requests: Requests,
    bundles: Query<&Bundle>,
    directories: SigningDirectories,
    started: Query<(), With<SignaturesStarted>>,
    entries: Replies<DirectoryEntries>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, _, mut phase) in &mut requests {
        if *phase != Phase::SignScan || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let Ok(bundle) = bundles.get(entity) else {
            continue;
        };
        let mut remaining = false;
        let mut found = false;
        for (directory, path, parent) in &directories {
            if parent.parent() != entity {
                continue;
            };
            found = true;
            remaining = true;
            let mut values = None;
            for (receipt, owner, purpose, entries) in &entries {
                if owner.0 == entity
                    && matches!(purpose, Purpose { kind: PurposeKind::Directory, entity: Some(id) } if *id == directory)
                {
                    values = Some(&entries.0);
                    io.commands.entity(receipt).despawn();
                    break;
                }
            }
            let Some(values) = values else {
                io.request(
                    entity,
                    Purpose::targeted(PurposeKind::Directory, directory),
                    ReadDirectory(path.0.clone()),
                );
                continue;
            };
            for (path, metadata) in values {
                if metadata.is_dir() {
                    io.commands
                        .spawn((Signing, Cwd(path.clone()), ChildOf(entity)));
                    if path.extension() == Some(OsStr::new("app")) {
                        io.commands.spawn((
                            SignTarget,
                            Cwd(path.clone()),
                            Identifier(None),
                            Order(2000u32.saturating_sub(path.components().count() as u32)),
                            ChildOf(entity),
                        ));
                    }
                } else if metadata.is_file()
                    && (path.starts_with(bundle.0.join("Contents/Frameworks"))
                        || path.parent() == Some(bundle.0.join("Contents/MacOS").as_path())
                            && path.file_name() != Some(OsStr::new("vmux_desktop")))
                {
                    io.commands
                        .spawn((Mach, Cwd(path.clone()), ChildOf(entity)));
                }
            }
            io.commands.entity(directory).insert(Scanned);
        }
        if !started.contains(entity) && !found {
            for path in [
                bundle.0.join("Contents/Frameworks"),
                bundle.0.join("Contents/Library"),
                bundle.0.join("Contents/MacOS"),
            ] {
                io.commands.spawn((Signing, Cwd(path), ChildOf(entity)));
            }
            io.commands.entity(entity).insert(SignaturesStarted);
        } else if !remaining {
            *phase = Phase::SignInspect;
        }
    }
}

#[derive(Component)]
struct SignaturesStarted;

fn inspect_signatures(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    files: Query<(Entity, &Cwd, &ChildOf), With<Mach>>,
    outputs: Replies<ToolOutput>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::SignInspect || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        let product = match settings.0.product(&checkout.root) {
            Ok(product) => product,
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
                continue;
            }
        };
        let mut inspected = false;
        for (receipt, owner, purpose, output) in &outputs {
            if owner.0 != entity {
                continue;
            };
            let Some(file) = purpose.target(PurposeKind::Inspect) else {
                continue;
            };
            let Ok((_, path, parent)) = files.get(*file) else {
                continue;
            };
            if parent.parent() != entity {
                continue;
            };
            inspected = true;
            if String::from_utf8_lossy(&output.0.stdout).contains("Mach-O") {
                let executable = path.0.parent() == Some(bundle.0.join("Contents/MacOS").as_path());
                let name = path
                    .0
                    .file_name()
                    .unwrap_or(OsStr::new(""))
                    .to_string_lossy();
                let identifier = if executable {
                    Some(if name == "vmux" {
                        product.value("VMUX_HOST_CLI_ID").to_owned()
                    } else {
                        format!("{}.{}", product.value("VMUX_HOST_NAMESPACE"), name)
                    })
                } else {
                    None
                };
                io.commands.entity(*file).insert((
                    SignTarget,
                    Identifier(identifier),
                    Order(if executable { 3000 } else { 0 }),
                ));
            }
            io.commands.entity(receipt).despawn();
        }
        if inspected || files.iter().all(|(_, _, parent)| parent.parent() != entity) {
            io.commands.spawn((
                SignTarget,
                Cwd(bundle
                    .0
                    .join("Contents/Frameworks/Chromium Embedded Framework.framework")),
                Identifier(None),
                Order(1000),
                ChildOf(entity),
            ));
            io.commands.spawn((
                SignTarget,
                Cwd(bundle.0.clone()),
                Identifier(None),
                Order(4000),
                ChildOf(entity),
            ));
            *phase = Phase::Sign;
        } else {
            for (file, path, parent) in &files {
                if parent.parent() == entity {
                    io.capture(
                        entity,
                        Purpose::targeted(PurposeKind::Inspect, file),
                        checkout,
                        settings,
                        "file",
                        vec![path.0.clone().into_os_string()],
                    );
                }
            }
        }
    }
}

fn sign(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    targets: SigningTargets,
    outputs: Replies<ToolOutput>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::Sign || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let mut just_signed = None;
        for (receipt, owner, purpose, _) in &outputs {
            if owner.0 != entity {
                continue;
            };
            let Some(target) = purpose.target(PurposeKind::Signature) else {
                continue;
            };
            io.commands.entity(*target).insert(Signed);
            just_signed = Some(*target);
            io.commands.entity(receipt).despawn();
        }
        let mut next = None;
        for (target, path, identifier, order, parent) in &targets {
            if parent.parent() != entity || just_signed == Some(target) {
                continue;
            };
            if next.as_ref().is_none_or(
                |(_, _, _, previous): &(Entity, &Cwd, &Identifier, &Order)| order.0 < previous.0,
            ) {
                next = Some((target, path, identifier, order));
            }
        }
        let Some((target, path, identifier, _)) = next else {
            *phase = Phase::Signature;
            continue;
        };
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };
        let result = (|| -> crate::Result<Vec<OsString>> {
            let product = settings.0.product(&checkout.root)?;
            let entitlements = if path.0.file_name() == Some(OsStr::new("vmux_vm")) {
                checkout
                    .root
                    .join("crates/util/vmux_vm/hypervisor.entitlements")
            } else {
                product.value("VMUX_HOST_ENTITLEMENTS").into()
            };
            let mut arguments = vec![
                "--force".into(),
                "--verify".into(),
                "--sign".into(),
                settings.0.required("APPLE_SIGNING_IDENTITY")?.to_owned(),
                "--options".into(),
                "runtime".into(),
                "--entitlements".into(),
                entitlements.into_os_string(),
            ];
            if let Some(keychain) = settings.0.get("CODESIGN_KEYCHAIN") {
                arguments.extend(["--keychain".into(), keychain.to_owned()]);
            }
            if let Some(identifier) = &identifier.0 {
                arguments.extend(["--identifier".into(), identifier.into()]);
            }
            arguments.push(path.0.clone().into_os_string());
            Ok(arguments)
        })();
        match result {
            Ok(arguments) => {
                io.capture(
                    entity,
                    Purpose::targeted(PurposeKind::Signature, target),
                    checkout,
                    settings,
                    "codesign",
                    arguments,
                );
            }
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
            }
        }
    }
}

fn verify_signature(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::Signature || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        io.capture(
            entity,
            Purpose::new(PurposeKind::Copy),
            checkout,
            settings,
            "codesign",
            vec![
                "--verify".into(),
                "--deep".into(),
                "--strict".into(),
                bundle.0.clone().into_os_string(),
            ],
        );
        *phase = Phase::NotaryArchive;
    }
}

fn notarize(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    outputs: Replies<ToolOutput>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(
            *phase,
            Phase::NotaryArchive | Phase::NotarySubmit | Phase::NotaryReceipt
        ) || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        if *phase == Phase::NotaryArchive {
            if settings.0.get("SKIP_NOTARIZE") == Some(OsStr::new("1")) {
                io.commands.entity(entity).insert(Complete);
                *phase = Phase::Done;
            } else {
                io.capture(
                    entity,
                    Purpose::new(PurposeKind::Copy),
                    checkout,
                    settings,
                    "ditto",
                    vec![
                        "-c".into(),
                        "-k".into(),
                        "--keepParent".into(),
                        bundle.0.clone().into_os_string(),
                        bundle.0.with_extension("notarize.zip").into_os_string(),
                    ],
                );
                *phase = Phase::NotarySubmit;
            }
        } else if *phase == Phase::NotarySubmit {
            let result = (|| -> crate::Result<Vec<OsString>> {
                Ok(vec![
                    "notarytool".into(),
                    "submit".into(),
                    bundle.0.with_extension("notarize.zip").into_os_string(),
                    "--apple-id".into(),
                    settings.0.required("APPLE_ID")?.to_owned(),
                    "--password".into(),
                    settings.0.required("APPLE_APP_PASSWORD")?.to_owned(),
                    "--team-id".into(),
                    settings.0.required("APPLE_TEAM_ID")?.to_owned(),
                    "--wait".into(),
                    "--output-format".into(),
                    "json".into(),
                ])
            })();
            match result {
                Ok(arguments) => {
                    io.capture(
                        entity,
                        Purpose::new(PurposeKind::Notary),
                        checkout,
                        settings,
                        "xcrun",
                        arguments,
                    );
                    *phase = Phase::NotaryReceipt;
                }
                Err(error) => {
                    io.fail(entity, Failure::from_error(error));
                }
            }
        } else {
            for (receipt, owner, purpose, output) in &outputs {
                if owner.0 != entity || !purpose.is(PurposeKind::Notary) {
                    continue;
                };
                let receipt_value: Result<serde_json::Value, _> =
                    serde_json::from_slice(&output.0.stdout);
                if receipt_value.ok().is_some_and(|value| {
                    value.get("status").and_then(serde_json::Value::as_str) == Some("Accepted")
                }) {
                    *phase = Phase::Staple;
                } else {
                    io.fail(
                        entity,
                        Failure::message("Notarization did not accept the app"),
                    );
                }
                io.commands.entity(receipt).despawn();
            }
        }
    }
}

fn staple(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if *phase != Phase::Staple || pending.iter().any(|owner| owner.0 == entity) {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        io.capture(
            entity,
            Purpose::new(PurposeKind::Copy),
            checkout,
            settings,
            "xcrun",
            vec![
                "stapler".into(),
                "staple".into(),
                bundle.0.clone().into_os_string(),
            ],
        );
        *phase = Phase::Assess;
    }
}

fn assess(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    bundles: Query<&Bundle>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if !matches!(*phase, Phase::Assess | Phase::NotaryCleanup)
            || pending.iter().any(|owner| owner.0 == entity)
        {
            continue;
        };
        let (Ok((checkout, settings)), Ok(bundle)) =
            (contexts.get(scope.context), bundles.get(entity))
        else {
            continue;
        };
        if *phase == Phase::Assess {
            io.capture(
                entity,
                Purpose::new(PurposeKind::Copy),
                checkout,
                settings,
                "spctl",
                vec![
                    "--assess".into(),
                    "--type".into(),
                    "execute".into(),
                    bundle.0.clone().into_os_string(),
                ],
            );
            *phase = Phase::NotaryCleanup;
        } else {
            io.capture(
                entity,
                Purpose::new(PurposeKind::Copy),
                checkout,
                settings,
                "rm",
                vec![bundle.0.with_extension("notarize.zip").into_os_string()],
            );
            *phase = Phase::Done;
        }
    }
}

fn complete_callbacks(
    requests: Query<(Entity, &Phase), With<Before>>,
    pending: Pending,
    mut commands: Commands,
) {
    for (entity, phase) in &requests {
        if *phase == Phase::Done && !pending.iter().any(|owner| owner.0 == entity) {
            commands.entity(entity).insert(Complete);
        }
    }
}

fn cancel(
    requests: Query<(Entity, &Scope, Option<&ChildOf>)>,
    failures: Query<(), With<Failed>>,
    owners: Query<&Attempt>,
    states: Query<&OperationState>,
    io: Query<(Entity, &Owner), With<Io>>,
    mut commands: Commands,
) {
    for (entity, scope, parent) in &requests {
        if Signals::pending().is_none()
            && parent.is_some_and(|parent| parent.parent() == scope.context)
            && owners
                .get(entity)
                .is_ok_and(|attempt| attempt.0 == scope.attempt)
            && states
                .get(scope.context)
                .is_ok_and(|state| *state == OperationState::Running)
        {
            continue;
        };
        if !failures.contains(entity) {
            commands.entity(entity).insert(Failed(Failure::interrupted(
                Signals::pending().unwrap_or(15),
            )));
        }
        for (child, owner) in &io {
            if owner.0 == entity {
                commands.entity(child).insert(OperationState::Canceled);
            }
        }
    }
    for (child, owner) in &io {
        if !requests.contains(owner.0) {
            commands.entity(child).insert(OperationState::Canceled);
        }
    }
}

#[derive(bevy_ecs::system::SystemParam)]
struct Waiting<'w, 's> {
    observations: Query<'w, 's, &'static Observation>,
    processes: crate::process_observation::Facts<'w, 's>,
    publications: Query<'w, 's, &'static Published>,
    published: Query<'w, 's, (), With<publication::Drained>>,
    guards: Query<
        'w,
        's,
        (
            &'static KeychainScope,
            &'static CleanupPhase,
            Has<ReleaseKeychain>,
        ),
    >,
    flights: Query<'w, 's, (&'static Witness, &'static Flight)>,
}

impl Waiting<'_, '_> {
    fn pending(&self, entity: Entity) -> bool {
        if self
            .observations
            .get(entity)
            .is_ok_and(|observation| !self.processes.drained(observation.0))
        {
            return true;
        }
        if self
            .publications
            .get(entity)
            .is_ok_and(|publication| !self.published.contains(publication.0))
        {
            return true;
        }
        if self.guards.iter().any(|(scope, phase, released)| {
            scope.owner == entity && released && *phase != CleanupPhase::Done
        }) {
            return true;
        }
        self.flights
            .iter()
            .any(|(witness, flight)| witness.owner == entity && flight.pending())
    }
}

fn wake(
    requests: WakeRequests,
    waiting: Waiting,
    io: Query<(&Owner, &OperationState), With<Io>>,
    wake: Res<JobWake>,
) {
    for entity in &requests {
        if waiting.pending(entity) {
            continue;
        }
        if !io
            .iter()
            .any(|(owner, state)| owner.0 == entity && *state == OperationState::Running)
        {
            wake.0.wake_by_ref();
        }
    }
}

fn finish(
    requests: Closing,
    outcomes: Outcomes,
    artifacts: ArtifactResults,
    lifetimes: Lifetimes,
    wake: Res<JobWake>,
    mut completions: MessageWriter<Completion>,
    mut commands: Commands,
) {
    let ArtifactResults {
        published,
        publications,
        destinations,
        ..
    } = artifacts;
    for (entity, scope, error) in &requests {
        if !lifetimes.drained(entity, outcomes.desktops.contains(entity)) {
            continue;
        }
        if published
            .get(entity)
            .is_ok_and(|published| !publications.contains(published.0))
        {
            continue;
        };
        let mut artifacts = Vec::new();
        if let Ok(publication) = published.get(entity) {
            for (parent, destination) in &destinations {
                if parent.parent() == publication.0 {
                    artifacts.push(destination.0.clone());
                }
            }
        }
        let mut remaining = false;
        for (child, parent) in &lifetimes.rows {
            if parent.parent() == entity
                && !published
                    .get(entity)
                    .is_ok_and(|publication| publication.0 == child)
            {
                commands.entity(child).despawn();
                remaining = true;
            }
        }
        if remaining {
            commands.entity(entity).remove::<Observation>();
            wake.0.wake_by_ref();
            continue;
        };
        if let Ok(publication) = published.get(entity) {
            commands.entity(publication.0).despawn();
        }
        completions.write(Completion {
            entity,
            context: scope.context,
            attempt: scope.attempt,
            result: outcomes.result(entity, error, artifacts),
        });
        if let Some(guard) = outcomes.keys.entity(entity) {
            commands.entity(guard).despawn();
        }
        commands.entity(entity).remove::<(
            Scope,
            Arguments,
            Staging,
            SigningDirectory,
            CertificateDirectory,
            Password,
            Keychain,
        )>();
        wake.0.wake_by_ref();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, Instant};
    use vmux_ecs::cli::CliResult;

    #[test]
    fn removing_a_step_drains_its_worker_and_restores_the_independent_keychain() {
        let _permit = crate::testing::Permit::acquire();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        fs::create_dir(root.join("bin")).unwrap();
        for (name, body) in [
            (
                "worker",
                "printf '%s' \"$$\" > \"$TEST_ROOT/pid\"\nprintf started > \"$TEST_ROOT/started\"\nexec sleep 60\n",
            ),
            (
                "security",
                "pid=$(cat \"$TEST_ROOT/pid\")\nif kill -0 \"$pid\" 2>/dev/null; then exit 9; fi\nprintf '%s\\n' \"$*\" >> \"$TEST_ROOT/cleanup\"\n",
            ),
        ] {
            let path = root.join("bin").join(name);
            fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut environment = crate::environment::Environment::current();
        environment.set(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        );
        environment.set("TEST_ROOT", root.as_os_str());
        let mut app = App::new();
        app.add_plugins(crate::Plugin);
        let context = app
            .world_mut()
            .spawn((
                crate::Accepted,
                Operation::bundle("dx.package.fixture").unwrap().running(),
                Settings(environment.clone()),
            ))
            .id();
        let step = app
            .world_mut()
            .spawn((
                Scope {
                    context,
                    attempt: 1,
                },
                Phase::Done,
                crate::Attempt(1),
                crate::Running,
                crate::Started {
                    context,
                    attempt: 1,
                },
                ChildOf(context),
            ))
            .id();
        let temporary =
            std::sync::Arc::new(crate::temporary::Temporary::create(&root, "signing").unwrap());
        let signing = temporary.path().to_owned();
        let guard = app
            .world_mut()
            .spawn((
                KeychainScope {
                    owner: step,
                    context,
                    attempt: 1,
                    directory: root.clone(),
                    environment: environment.clone(),
                    lease: None,
                },
                CleanupPhase::Start,
                SigningDirectory(temporary),
                Keychain(signing.join("signing.keychain-db")),
            ))
            .id();
        app.world_mut().entity_mut(step).insert(GuardRef(guard));
        app.world_mut().spawn((
            OriginalKeychain("/fixture/login.keychain-db".into()),
            Order(0),
            ChildOf(guard),
        ));
        app.world_mut().spawn((
            Io,
            Owner(step),
            Purpose::new(PurposeKind::Copy),
            Capture {
                environment,
                directory: root.clone(),
                program: "worker".into(),
                arguments: Vec::new(),
            },
            Operation::bundle("dx.package.io").unwrap(),
            ChildOf(step),
        ));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.join("started").exists() {
            app.update();
            assert!(Instant::now() < deadline, "native worker did not start");
            std::thread::sleep(Duration::from_millis(5));
        }
        app.world_mut().entity_mut(step).despawn();
        assert!(app.world().get::<CliResult>(context).is_none());
        loop {
            app.update();
            if app.world().get::<CliResult>(context).is_some() {
                break;
            };
            assert!(Instant::now() < deadline, "keychain cleanup did not drain");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            app.world().get::<CliResult>(context),
            Some(CliResult(Err(_)))
        ));
        let calls = fs::read_to_string(root.join("cleanup")).unwrap();
        assert!(calls.starts_with("list-keychains -d user -s /fixture/login.keychain-db\n"));
        assert!(
            calls
                .lines()
                .last()
                .unwrap()
                .starts_with("delete-keychain ")
        );
        assert!(!signing.exists());
        assert!(app.world().get_entity(guard).is_err());
    }
}
