use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_ecs::cli::{CliInvocation, CliResult};
use vmux_ecs::{CreatedAt, Operation, OperationState};

use crate::Result;
use crate::execution::Failure;
use crate::signals::Signals;
use crate::update_io::{Guard, Installation, Published, Verified};

pub struct UpdatesPlugin;

impl bevy_app::Plugin for UpdatesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<StepFailed>()
            .add_message::<Inspected>()
            .add_message::<Saved>()
            .add_message::<CheckedManifest>()
            .add_message::<Downloaded>()
            .add_message::<PublishedArtifact>()
            .add_message::<HandedOffProcess>()
            .add_systems(
                Update,
                (
                    accept,
                    (
                        receive_inspect,
                        receive_save,
                        receive_check,
                        receive_download,
                        receive_publish,
                        receive_handoff,
                    ),
                    (
                        apply_failed,
                        apply_inspected,
                        apply_saved,
                        apply_checked,
                        apply_downloaded,
                        apply_published,
                        apply_handed_off,
                    )
                        .chain(),
                    select,
                    select_release,
                    (
                        start_inspect,
                        ApplyDeferred,
                        start_save,
                        ApplyDeferred,
                        start_check,
                        ApplyDeferred,
                        start_download,
                        ApplyDeferred,
                        start_publish,
                        ApplyDeferred,
                        start_handoff,
                    )
                        .chain(),
                )
                    .chain(),
            );
    }
}

impl UpdatesPlugin {
    pub fn automatic() -> Option<u8> {
        let mut arguments = std::env::args_os().skip(1);
        for argument in &mut arguments {
            if argument == "--" {
                break;
            }
            if matches!(
                argument.to_str(),
                Some("-h" | "--help" | "-V" | "--version")
            ) {
                return None;
            }
        }
        if std::env::var_os("VMUX_CLI_UPDATE_HANDOFF").is_some()
            || std::env::args_os()
                .nth(1)
                .is_some_and(|name| name == "update")
        {
            return None;
        }
        let mut app = App::new();
        app.add_plugins(Self);
        let operation = app.world_mut().spawn(Request::automatic()).id();
        loop {
            app.update();
            if let Some(result) = app.world().get::<CliResult>(operation) {
                if let Some(handoff) = app.world().get::<HandedOff>(operation) {
                    return Some(handoff.0);
                }
                if let Err(error) = &result.0 {
                    eprintln!("CLI update unavailable: {error}");
                }
                return Signals::pending().map(|signal| (128 + signal) as u8);
            }
            thread::park_timeout(Duration::from_millis(100));
        }
    }
}

#[derive(Component, Clone)]
struct Request {
    automatic: bool,
    check: bool,
    channel: Option<Channel>,
    enabled: Option<bool>,
}

impl Request {
    fn automatic() -> Self {
        Self {
            automatic: true,
            check: false,
            channel: None,
            enabled: None,
        }
    }

    fn invocation(invocation: &CliInvocation) -> Self {
        Self {
            automatic: false,
            check: invocation.flag("check"),
            channel: invocation.value("channel").map(|value| match value {
                "preview" => Channel::Preview,
                _ => Channel::Stable,
            }),
            enabled: invocation.value("auto").map(|value| value == "on"),
        }
    }
}

#[derive(Component, Clone, Deserialize, Serialize)]
pub(crate) struct Preferences {
    pub(crate) channel: Channel,
    pub(crate) enabled: bool,
    pub(crate) last_check: Option<u64>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            channel: Channel::Stable,
            enabled: true,
            last_check: None,
        }
    }
}

impl Preferences {
    fn due(&self, now: u64) -> bool {
        self.enabled
            && self
                .last_check
                .is_none_or(|checked| now.saturating_sub(checked) >= 3600)
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Channel {
    #[default]
    Stable,
    Preview,
}

impl Channel {
    pub(crate) fn endpoint(self) -> &'static str {
        match self {
            Self::Stable => "https://vmux.ai/updates.json",
            Self::Preview => {
                "https://github.com/vmux-ai/vmux/releases/download/nightly/updates-preview.json"
            }
        }
    }
}

#[derive(Component, Clone)]
pub(crate) struct Release {
    pub(crate) version: String,
}

#[derive(Deserialize)]
pub(crate) struct Manifest {
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) cli: BTreeMap<String, Download>,
}

impl Release {
    fn available(&self, current: &str, switching: bool) -> Result<bool> {
        let candidate = semver::Version::parse(self.version.trim_start_matches('v'))?;
        let current = semver::Version::parse(current)?;
        Ok(if switching {
            candidate != current
        } else {
            candidate > current
        })
    }
}

#[derive(Component, Clone, Deserialize)]
pub(crate) struct Download {
    pub(crate) url: String,
    pub(crate) signature: String,
}

#[derive(Component)]
struct Accepted;

#[derive(Component)]
struct CheckAfterSave;

#[derive(Component)]
struct Checked;

#[derive(Component)]
struct Lease(Arc<Guard>);

#[derive(Component)]
struct Attempt(u64);

#[derive(Component)]
struct HandedOff(u8);

#[derive(Component)]
struct ActiveStep;

impl ActiveStep {
    fn bundle<S: Component>(parent: Entity, step: S) -> impl Bundle {
        (
            ChildOf(parent),
            Name::new("CLI update step"),
            CreatedAt::now(),
            OperationState::Running,
            Attempt(0),
            ActiveStep,
            step,
        )
    }
}

#[derive(Component)]
struct ActiveRunning;

#[derive(Component, Clone)]
struct InspectStep(bool);

#[derive(Component, Clone)]
struct SaveStep(Preferences);

#[derive(Component, Clone)]
struct CheckStep(Channel);

#[derive(Component, Clone)]
struct DownloadStep {
    version: String,
    download: Download,
}

#[derive(Component, Clone)]
struct PublishStep(Verified);

#[derive(Component, Clone)]
struct HandoffStep(Published);

#[derive(Component)]
struct Running<T: Send + 'static> {
    attempt: u64,
    receiver: Mutex<mpsc::Receiver<std::result::Result<T, Failure>>>,
}

impl<T: Send + 'static> Running<T> {
    fn spawn(
        attempt: u64,
        lease: Option<Arc<Guard>>,
        work: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> (Self, ActiveRunning) {
        let wake = thread::current();
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let _lease = lease;
            let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
                Ok(result) => result.map_err(Failure::from_error),
                Err(_) => Err(Failure::message("CLI update worker panicked")),
            };
            let _ = sender.send(result);
            wake.unpark();
        });
        (
            Self {
                attempt,
                receiver: Mutex::new(receiver),
            },
            ActiveRunning,
        )
    }

    fn poll(&self) -> Option<std::result::Result<T, Failure>> {
        let receiver = self.receiver.lock().unwrap();
        match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err(Failure::message("CLI update worker disconnected")))
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
}

#[derive(Message)]
struct StepFailed {
    entity: Entity,
    attempt: u64,
    error: Failure,
}

#[derive(Message)]
struct Inspected {
    entity: Entity,
    attempt: u64,
    outcome: Option<(Preferences, Arc<Guard>)>,
}

#[derive(Message)]
struct Saved {
    entity: Entity,
    attempt: u64,
}

#[derive(Message)]
struct CheckedManifest {
    entity: Entity,
    attempt: u64,
    manifest: Manifest,
}

#[derive(Message)]
struct Downloaded {
    entity: Entity,
    attempt: u64,
    verified: Verified,
}

#[derive(Message)]
struct PublishedArtifact {
    entity: Entity,
    attempt: u64,
    published: Published,
}

#[derive(Message)]
struct HandedOffProcess {
    entity: Entity,
    attempt: u64,
    code: u8,
}

type Incoming<'w, 's> =
    Query<'w, 's, (Entity, &'static CliInvocation), (Without<Request>, Without<CliResult>)>;
type Requests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Request, Option<&'static Installation>),
    (Without<Accepted>, Without<CliResult>),
>;
type Selection<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        &'static Installation,
        &'static Preferences,
        Option<&'static Children>,
        Option<&'static Checked>,
        Option<&'static Release>,
    ),
    Without<CliResult>,
>;
type ActiveSteps<'w, 's> = Query<'w, 's, (&'static ChildOf, &'static Attempt), With<ActiveRunning>>;
type ActiveInstallations<'w, 's> =
    Query<'w, 's, (&'static Installation, Option<&'static Lease>), Without<CliResult>>;
type InspectRunning<'w, 's> =
    Query<'w, 's, (Entity, &'static Running<Option<(Preferences, Arc<Guard>)>>)>;
type ReleaseSelection<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        &'static Installation,
        &'static Release,
        Option<&'static Children>,
    ),
    Without<CliResult>,
>;

fn accept(
    invocations: Incoming,
    requests: Requests,
    signals: Option<Res<Signals>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        if invocation.is("dx.update") {
            commands
                .entity(entity)
                .insert(Request::invocation(invocation));
            thread::current().unpark();
        }
    }
    for (entity, request, supplied) in &requests {
        let installation = match supplied {
            Some(installation) => installation.clone(),
            None => {
                match Installation::current() {
                    Ok(installation) => {
                        commands.entity(entity).insert(installation);
                        thread::current().unpark();
                    }
                    Err(error) => {
                        commands
                            .entity(entity)
                            .insert(CliResult(Err(error.to_string())));
                    }
                }
                continue;
            }
        };
        if request.automatic && (!installation.standalone() || !installation.trusted()) {
            commands.entity(entity).insert(CliResult::default());
            continue;
        }
        if signals.is_none() {
            match Signals::install() {
                Ok(signals) => {
                    commands.insert_resource(signals);
                }
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(CliResult(Err(error.to_string())));
                    continue;
                }
            }
        }
        commands.entity(entity).insert((
            Accepted,
            Operation::bundle("dx.update").unwrap().running(),
            Name::new("CLI release update"),
            CreatedAt::now(),
            installation,
        ));
        commands.spawn(ActiveStep::bundle(entity, InspectStep(request.automatic)));
    }
}

fn start_inspect(
    pending: Query<(Entity, &InspectStep, &ChildOf, &Attempt), Without<ActiveRunning>>,
    installations: ActiveInstallations,
    running: Query<Entity, With<ActiveRunning>>,
    mut commands: Commands,
) {
    if !running.is_empty() {
        return;
    }
    for (entity, step, parent, attempt) in &pending {
        let Ok((installation, lease)) = installations.get(parent.parent()) else {
            continue;
        };
        let installation = installation.clone();
        let lease = lease.map(|lease| lease.0.clone());
        let nonblocking = step.0;
        commands
            .entity(entity)
            .insert(Running::spawn(attempt.0, lease, move || {
                Signals::check()?;
                let Some(lease) = installation.lock(nonblocking)? else {
                    return Ok(None);
                };
                let lease = Arc::new(lease);
                let preferences = installation.load()?;
                Ok(Some((preferences, lease)))
            }));
        break;
    }
}

fn start_save(
    pending: Query<(Entity, &SaveStep, &ChildOf, &Attempt), Without<ActiveRunning>>,
    installations: ActiveInstallations,
    running: Query<Entity, With<ActiveRunning>>,
    mut commands: Commands,
) {
    if !running.is_empty() {
        return;
    }
    for (entity, step, parent, attempt) in &pending {
        let Ok((installation, lease)) = installations.get(parent.parent()) else {
            continue;
        };
        let installation = installation.clone();
        let lease = lease.map(|lease| lease.0.clone());
        let preferences = step.0.clone();
        commands
            .entity(entity)
            .insert(Running::spawn(attempt.0, lease, move || {
                Signals::check()?;
                installation.save(&preferences)
            }));
        break;
    }
}

fn start_check(
    pending: Query<(Entity, &CheckStep, &ChildOf, &Attempt), Without<ActiveRunning>>,
    installations: ActiveInstallations,
    running: Query<Entity, With<ActiveRunning>>,
    mut commands: Commands,
) {
    if !running.is_empty() {
        return;
    }
    for (entity, step, parent, attempt) in &pending {
        let Ok((installation, lease)) = installations.get(parent.parent()) else {
            continue;
        };
        let installation = installation.clone();
        let lease = lease.map(|lease| lease.0.clone());
        let channel = step.0;
        commands
            .entity(entity)
            .insert(Running::spawn(attempt.0, lease, move || {
                Signals::check()?;
                installation.check(channel)
            }));
        break;
    }
}

fn start_download(
    pending: Query<(Entity, &DownloadStep, &ChildOf, &Attempt), Without<ActiveRunning>>,
    installations: ActiveInstallations,
    running: Query<Entity, With<ActiveRunning>>,
    mut commands: Commands,
) {
    if !running.is_empty() {
        return;
    }
    for (entity, step, parent, attempt) in &pending {
        let Ok((installation, lease)) = installations.get(parent.parent()) else {
            continue;
        };
        let installation = installation.clone();
        let lease = lease.map(|lease| lease.0.clone());
        let version = step.version.clone();
        let download = step.download.clone();
        commands
            .entity(entity)
            .insert(Running::spawn(attempt.0, lease, move || {
                Signals::check()?;
                installation.download(&version, &download)
            }));
        break;
    }
}

fn start_publish(
    pending: Query<(Entity, &PublishStep, &ChildOf, &Attempt), Without<ActiveRunning>>,
    installations: ActiveInstallations,
    running: Query<Entity, With<ActiveRunning>>,
    mut commands: Commands,
) {
    if !running.is_empty() {
        return;
    }
    for (entity, step, parent, attempt) in &pending {
        let Ok((installation, lease)) = installations.get(parent.parent()) else {
            continue;
        };
        let installation = installation.clone();
        let lease = lease.map(|lease| lease.0.clone());
        let verified = step.0.clone();
        commands
            .entity(entity)
            .insert(Running::spawn(attempt.0, lease, move || {
                Signals::check()?;
                installation.publish(&verified)
            }));
        break;
    }
}

fn start_handoff(
    pending: Query<(Entity, &HandoffStep, &ChildOf, &Attempt), Without<ActiveRunning>>,
    installations: ActiveInstallations,
    running: Query<Entity, With<ActiveRunning>>,
    mut commands: Commands,
) {
    if !running.is_empty() {
        return;
    }
    for (entity, step, parent, attempt) in &pending {
        let Ok((installation, lease)) = installations.get(parent.parent()) else {
            continue;
        };
        let installation = installation.clone();
        let lease = lease.map(|lease| lease.0.clone());
        let published = step.0.clone();
        commands
            .entity(entity)
            .insert(Running::spawn(attempt.0, lease, move || {
                Signals::check()?;
                installation.handoff(&published)
            }));
        break;
    }
}

fn receive_inspect(
    running: InspectRunning,
    mut ok: MessageWriter<Inspected>,
    mut failed: MessageWriter<StepFailed>,
) {
    for (entity, running) in &running {
        match running.poll() {
            Some(Ok(outcome)) => {
                ok.write(Inspected {
                    entity,
                    attempt: running.attempt,
                    outcome,
                });
            }
            Some(Err(error)) => {
                failed.write(StepFailed {
                    entity,
                    attempt: running.attempt,
                    error,
                });
            }
            None => {}
        }
    }
}

fn receive_save(
    running: Query<(Entity, &Running<()>)>,
    mut ok: MessageWriter<Saved>,
    mut failed: MessageWriter<StepFailed>,
) {
    for (entity, running) in &running {
        match running.poll() {
            Some(Ok(())) => {
                ok.write(Saved {
                    entity,
                    attempt: running.attempt,
                });
            }
            Some(Err(error)) => {
                failed.write(StepFailed {
                    entity,
                    attempt: running.attempt,
                    error,
                });
            }
            None => {}
        }
    }
}

fn receive_check(
    running: Query<(Entity, &Running<Manifest>)>,
    mut ok: MessageWriter<CheckedManifest>,
    mut failed: MessageWriter<StepFailed>,
) {
    for (entity, running) in &running {
        match running.poll() {
            Some(Ok(manifest)) => {
                ok.write(CheckedManifest {
                    entity,
                    attempt: running.attempt,
                    manifest,
                });
            }
            Some(Err(error)) => {
                failed.write(StepFailed {
                    entity,
                    attempt: running.attempt,
                    error,
                });
            }
            None => {}
        }
    }
}

fn receive_download(
    running: Query<(Entity, &Running<Verified>)>,
    mut ok: MessageWriter<Downloaded>,
    mut failed: MessageWriter<StepFailed>,
) {
    for (entity, running) in &running {
        match running.poll() {
            Some(Ok(verified)) => {
                ok.write(Downloaded {
                    entity,
                    attempt: running.attempt,
                    verified,
                });
            }
            Some(Err(error)) => {
                failed.write(StepFailed {
                    entity,
                    attempt: running.attempt,
                    error,
                });
            }
            None => {}
        }
    }
}

fn receive_publish(
    running: Query<(Entity, &Running<Published>)>,
    mut ok: MessageWriter<PublishedArtifact>,
    mut failed: MessageWriter<StepFailed>,
) {
    for (entity, running) in &running {
        match running.poll() {
            Some(Ok(published)) => {
                ok.write(PublishedArtifact {
                    entity,
                    attempt: running.attempt,
                    published,
                });
            }
            Some(Err(error)) => {
                failed.write(StepFailed {
                    entity,
                    attempt: running.attempt,
                    error,
                });
            }
            None => {}
        }
    }
}

fn receive_handoff(
    running: Query<(Entity, &Running<u8>)>,
    mut ok: MessageWriter<HandedOffProcess>,
    mut failed: MessageWriter<StepFailed>,
) {
    for (entity, running) in &running {
        match running.poll() {
            Some(Ok(code)) => {
                ok.write(HandedOffProcess {
                    entity,
                    attempt: running.attempt,
                    code,
                });
            }
            Some(Err(error)) => {
                failed.write(StepFailed {
                    entity,
                    attempt: running.attempt,
                    error,
                });
            }
            None => {}
        }
    }
}

fn apply_failed(
    mut receipts: MessageReader<StepFailed>,
    steps: ActiveSteps,
    operations: Query<Entity, (With<Request>, Without<CliResult>)>,
    mut commands: Commands,
) {
    for receipt in receipts.read() {
        let Ok((parent, attempt)) = steps.get(receipt.entity) else {
            continue;
        };
        if attempt.0 != receipt.attempt {
            continue;
        }
        let entity = parent.parent();
        if !operations.contains(entity) {
            continue;
        }
        commands.entity(receipt.entity).despawn();
        eprintln!("CLI update failed: {}", receipt.error);
        let state = if Signals::pending().is_some() {
            OperationState::Canceled
        } else {
            OperationState::Failed
        };
        commands
            .entity(entity)
            .insert((state, CliResult(Ok(receipt.error.code() as u8))))
            .remove::<Lease>();
    }
}

fn apply_inspected(
    mut receipts: MessageReader<Inspected>,
    steps: ActiveSteps,
    operations: Query<Entity, (With<Request>, Without<CliResult>)>,
    mut commands: Commands,
) {
    for receipt in receipts.read() {
        let Ok((parent, attempt)) = steps.get(receipt.entity) else {
            continue;
        };
        if attempt.0 != receipt.attempt {
            continue;
        }
        let entity = parent.parent();
        if !operations.contains(entity) {
            continue;
        }
        commands.entity(receipt.entity).despawn();
        match &receipt.outcome {
            None => {
                commands
                    .entity(entity)
                    .insert((OperationState::Succeeded, CliResult::default()));
            }
            Some((preferences, lease)) => {
                commands
                    .entity(entity)
                    .insert((preferences.clone(), Lease(lease.clone())));
            }
        }
    }
}

fn apply_saved(
    mut receipts: MessageReader<Saved>,
    steps: ActiveSteps,
    operations: Query<Option<&CheckAfterSave>, (With<Request>, Without<CliResult>)>,
    mut commands: Commands,
) {
    for receipt in receipts.read() {
        let Ok((parent, attempt)) = steps.get(receipt.entity) else {
            continue;
        };
        if attempt.0 != receipt.attempt {
            continue;
        }
        let entity = parent.parent();
        let Ok(next) = operations.get(entity) else {
            continue;
        };
        commands.entity(receipt.entity).despawn();
        if next.is_some() {
            commands
                .entity(entity)
                .remove::<CheckAfterSave>()
                .insert(Checked);
        } else {
            println!("CLI update preferences saved");
            commands
                .entity(entity)
                .insert((OperationState::Succeeded, CliResult::default()))
                .remove::<Lease>();
        }
    }
}

fn apply_checked(
    mut receipts: MessageReader<CheckedManifest>,
    steps: ActiveSteps,
    operations: Query<Entity, (With<Request>, Without<CliResult>)>,
    mut commands: Commands,
) {
    for receipt in receipts.read() {
        let Ok((parent, attempt)) = steps.get(receipt.entity) else {
            continue;
        };
        if attempt.0 != receipt.attempt {
            continue;
        }
        let entity = parent.parent();
        if !operations.contains(entity) {
            continue;
        }
        commands.entity(receipt.entity).despawn();
        commands.entity(entity).insert(Release {
            version: receipt.manifest.version.clone(),
        });
        for (platform, download) in &receipt.manifest.cli {
            commands.spawn((
                ChildOf(entity),
                Name::new(platform.clone()),
                CreatedAt::now(),
                download.clone(),
            ));
        }
    }
}

fn apply_downloaded(
    mut receipts: MessageReader<Downloaded>,
    steps: ActiveSteps,
    operations: Query<Entity, (With<Request>, Without<CliResult>)>,
    mut commands: Commands,
) {
    for receipt in receipts.read() {
        let Ok((parent, attempt)) = steps.get(receipt.entity) else {
            continue;
        };
        if attempt.0 != receipt.attempt {
            continue;
        }
        let entity = parent.parent();
        if !operations.contains(entity) {
            continue;
        }
        commands.entity(receipt.entity).despawn();
        commands.spawn(ActiveStep::bundle(
            entity,
            PublishStep(receipt.verified.clone()),
        ));
    }
}

fn apply_published(
    mut receipts: MessageReader<PublishedArtifact>,
    steps: ActiveSteps,
    operations: Query<&Request, Without<CliResult>>,
    mut commands: Commands,
) {
    for receipt in receipts.read() {
        let Ok((parent, attempt)) = steps.get(receipt.entity) else {
            continue;
        };
        if attempt.0 != receipt.attempt {
            continue;
        }
        let entity = parent.parent();
        let Ok(request) = operations.get(entity) else {
            continue;
        };
        commands.entity(receipt.entity).despawn();
        if request.automatic {
            eprintln!("CLI {} installed", receipt.published.version());
            commands.entity(entity).remove::<Lease>();
            commands.spawn(ActiveStep::bundle(
                entity,
                HandoffStep(receipt.published.clone()),
            ));
        } else {
            println!("CLI {} installed", receipt.published.version());
            if let Err(error) = receipt.published.complete() {
                eprintln!("CLI update backup retained: {error}");
            }
            commands
                .entity(entity)
                .insert((OperationState::Succeeded, CliResult::default()))
                .remove::<Lease>();
            commands.spawn((
                ChildOf(entity),
                Name::new("Standalone CLI"),
                CreatedAt::now(),
                crate::Artifact(receipt.published.executable().to_path_buf()),
                vmux_ecs::Ready,
            ));
        }
    }
}

fn apply_handed_off(
    mut receipts: MessageReader<HandedOffProcess>,
    steps: ActiveSteps,
    operations: Query<Entity, (With<Request>, Without<CliResult>)>,
    mut commands: Commands,
) {
    for receipt in receipts.read() {
        let Ok((parent, attempt)) = steps.get(receipt.entity) else {
            continue;
        };
        if attempt.0 != receipt.attempt {
            continue;
        }
        let entity = parent.parent();
        if !operations.contains(entity) {
            continue;
        }
        commands.entity(receipt.entity).despawn();
        commands.entity(entity).insert((
            HandedOff(receipt.code),
            if receipt.code == 0 {
                OperationState::Succeeded
            } else {
                OperationState::Failed
            },
            CliResult(Ok(receipt.code)),
        ));
    }
}

fn select_release(
    operations: ReleaseSelection,
    steps: Query<Entity, With<ActiveStep>>,
    downloads: Query<(&Name, &Download, &ChildOf)>,
    mut commands: Commands,
) {
    for (entity, request, installation, release, children) in &operations {
        if children.is_some_and(|children| children.iter().any(|child| steps.contains(child))) {
            continue;
        }
        let available = match release.available(&installation.version, request.channel.is_some()) {
            Ok(available) => available,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, CliResult(Err(error.to_string()))))
                    .remove::<Lease>();
                continue;
            }
        };
        if request.check || !available {
            if request.automatic {
                eprintln!(
                    "CLI release: {} (installed {})",
                    release.version, installation.version
                );
            } else {
                println!(
                    "CLI release: {} (installed {})",
                    release.version, installation.version
                );
            }
            commands
                .entity(entity)
                .insert((OperationState::Succeeded, CliResult::default()))
                .remove::<Lease>();
            continue;
        }
        if !installation.standalone() {
            commands.entity(entity).insert((OperationState::Failed, CliResult(Err("Standalone release updates require the DX-only CLI; runtime, build-cache, and app-bundled executables retain their existing update owner".into())))).remove::<Lease>();
            continue;
        }
        let mut selected = None;
        for (name, download, parent) in &downloads {
            if parent.parent() == entity && name.as_str() == installation.platform {
                selected = Some(download.clone());
                break;
            }
        }
        match selected {
            Some(download) => {
                commands.spawn(ActiveStep::bundle(
                    entity,
                    DownloadStep {
                        version: release.version.clone(),
                        download,
                    },
                ));
                commands.entity(entity).remove::<Release>();
            }
            None => {
                commands
                    .entity(entity)
                    .insert((
                        OperationState::Failed,
                        CliResult(Err(format!(
                            "No standalone CLI release for {}",
                            installation.platform
                        ))),
                    ))
                    .remove::<Lease>();
            }
        }
    }
}

fn select(operations: Selection, steps: Query<Entity, With<ActiveStep>>, mut commands: Commands) {
    for (entity, request, _installation, preferences, children, checked, release) in &operations {
        if release.is_some()
            || children.is_some_and(|children| children.iter().any(|child| steps.contains(child)))
        {
            continue;
        }
        if checked.is_some() {
            commands.entity(entity).remove::<Checked>();
            commands.spawn(ActiveStep::bundle(entity, CheckStep(preferences.channel)));
            continue;
        }
        let mut updated = preferences.clone();
        if let Some(channel) = request.channel
            && channel != updated.channel
        {
            updated.channel = channel;
            updated.last_check = None;
        }
        if let Some(enabled) = request.enabled {
            updated.enabled = enabled;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if request.automatic && !updated.due(now) {
            commands
                .entity(entity)
                .insert((OperationState::Succeeded, CliResult::default()))
                .remove::<Lease>();
            continue;
        }
        if request.enabled.is_none() || request.channel.is_some() || request.check {
            updated.last_check = Some(now);
            commands.entity(entity).insert(CheckAfterSave);
        }
        commands.entity(entity).insert(updated.clone());
        commands.spawn(ActiveStep::bundle(entity, SaveStep(updated)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::os::unix::fs::PermissionsExt;
    use std::time::Instant;

    struct Fixture {
        directory: tempfile::TempDir,
        installation: Installation,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let executable = directory.path().join("bin/vmux");
            std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
            std::fs::write(&executable, "#!/bin/sh\nprintf 'old CLI\\n'\n").unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
            let archive = directory.path().join("cli.tar.gz");
            std::fs::write(&archive, STANDARD.decode("H4sIAAAAAAAA/+3Suw5AQBCFYbWnWFHomIIV76KhEAobcYvHt7YS0boU/9ecyTRniln7ZfMeJlaeZS6ta4qkp9nttdapp+Tpww7LNFejrXyj64fCIKk7k0ytP4ydmRsVrfYjVBFLLKWJ/K/vAwAAAAAAAAAAAAAAAADc2wGosz4BACgAAA==").unwrap()).unwrap();
            let manifest = directory.path().join("manifest.json");
            std::fs::write(&manifest, serde_json::to_vec(&serde_json::json!({
                "version": "v9.0.0",
                "cli": { "macos-aarch64": { "url": "https://fixture/cli.tar.gz", "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IGZpeHR1cmUgc2lnbmF0dXJlClJVUURBd01EQXdNREE4cmpXd2x2VzZZM0ZPcytqVUtCR2x1M3lRT1liYytsTUZHWkdOTitBdStreFBrK0pmNVpiNko2Y2IvemVMT1EzbThnbTZ1Um1OaWYxdHJrS2FydmR3VT0KdHJ1c3RlZCBjb21tZW50OiB0aW1lc3RhbXA6MQlmaWxlOmNsaS50YXIuZ3oKanhaN3hvbG0vOXBLbjFvbkoxZG9ZdFZpUXRaeHBHd3l4Nmdaa043VU9NWEtoTHhuVDdXY3E5NFNXVmRZK2wwbHNya0lZSVA5Tjd3aUlpN0hTSlk3Q1E9PQo=" } }
            })).unwrap()).unwrap();
            let curl = directory.path().join("curl");
            std::fs::write(
                &curl,
                r#"#!/bin/sh
printf '%s\n' "$*" >> "$TEST_CURL_LOG"
case "$*" in
    *--output*)
        while [ "$#" -gt 0 ]; do
            if [ "$1" = --output ]; then cp "$TEST_ARCHIVE" "$2"; exit $?; fi
            shift
        done
        exit 1 ;;
    *) cat "$TEST_MANIFEST" ;;
esac
"#,
            )
            .unwrap();
            std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).unwrap();
            let mut environment = crate::environment::Environment::current();
            environment.set("CURL_BIN", curl.as_os_str());
            environment.set("TEST_ARCHIVE", archive.as_os_str());
            environment.set("TEST_MANIFEST", manifest.as_os_str());
            environment.set(
                "TEST_CURL_LOG",
                directory.path().join("curl.log").as_os_str(),
            );
            let installation = Installation {
                executable,
                root: directory.path().join("updates"),
                version: "8.0.0".into(),
                platform: "macos-aarch64".into(),
                release_compatible: true,
                key: "dW50cnVzdGVkIGNvbW1lbnQ6IGZpeHR1cmUgcHVibGljIGtleQpSV1FEQXdNREF3TURBK3BLYkdQaW5GSUt2dlZRZXhNdXhmbVZSM2F1dnI1N2trSWU2bWtVUnRJcwo=".into(),
                environment,
            };
            Self {
                directory,
                installation,
            }
        }

        fn run(&self, request: Request) -> bool {
            let mut app = App::new();
            app.add_plugins(UpdatesPlugin);
            let operation = app
                .world_mut()
                .spawn((request, self.installation.clone()))
                .id();
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                app.update();
                if let Some(result) = app.world().get::<CliResult>(operation) {
                    assert!(matches!(
                        app.world().get::<OperationState>(operation),
                        Some(
                            OperationState::Succeeded
                                | OperationState::Failed
                                | OperationState::Canceled
                        )
                    ));
                    return result.0 == Ok(0);
                }
                assert!(Instant::now() < deadline, "CLI update operation stalled");
                thread::park_timeout(Duration::from_millis(10));
            }
        }

        fn manual(&self, channel: Channel) -> Request {
            Request {
                automatic: false,
                check: false,
                channel: Some(channel),
                enabled: None,
            }
        }

        fn requests(&self) -> String {
            std::fs::read_to_string(self.directory.path().join("curl.log")).unwrap()
        }
    }

    #[test]
    fn automatic_update_skips_a_busy_installation_without_network_or_binary_mutation() {
        let fixture = Fixture::new();
        let _lease = fixture.installation.lock(false).unwrap().unwrap();
        let original = std::fs::read(&fixture.installation.executable).unwrap();
        assert!(fixture.run(Request::automatic()));
        assert_eq!(
            std::fs::read(&fixture.installation.executable).unwrap(),
            original
        );
        assert!(!fixture.directory.path().join("curl.log").exists());
    }

    #[test]
    fn download_file_limit_stops_an_overflowing_tool_before_its_completion_side_effect() {
        let mut fixture = Fixture::new();
        let curl = fixture.directory.path().join("curl");
        let overflow = fixture.directory.path().join("overflow");
        fixture
            .installation
            .environment
            .set("TEST_OVERFLOW", overflow.as_os_str());
        std::fs::write(
            curl,
            r#"#!/bin/sh
case "$*" in
    *--output*)
        while [ "$#" -gt 0 ]; do
            if [ "$1" = --output ]; then
                dd if=/dev/zero of="$2" bs=1048576 count=65 2>/dev/null || exit $?
                touch "$TEST_OVERFLOW"
                exit 0
            fi
            shift
        done
        exit 1 ;;
    *) cat "$TEST_MANIFEST" ;;
esac
"#,
        )
        .unwrap();
        let original = std::fs::read(&fixture.installation.executable).unwrap();
        assert!(!fixture.run(fixture.manual(Channel::Preview)));
        assert!(!overflow.exists());
        assert_eq!(
            std::fs::read(&fixture.installation.executable).unwrap(),
            original
        );
    }

    #[test]
    fn production_update_lifecycle_verifies_publishes_and_preserves_channel_preferences() {
        if std::env::var_os("VMUX_UPDATE_TEST_ISOLATED").is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg("updates::tests::production_update_lifecycle_verifies_publishes_and_preserves_channel_preferences")
                .arg("--test-threads=1")
                .env("VMUX_UPDATE_TEST_ISOLATED", "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let mut managed = Fixture::new();
        managed.installation.release_compatible = false;
        let original = std::fs::read(&managed.installation.executable).unwrap();
        assert!(!managed.run(managed.manual(Channel::Preview)));
        assert_eq!(
            std::fs::read(&managed.installation.executable).unwrap(),
            original
        );
        assert!(!managed.requests().contains("--output"));
        let mut fixture = Fixture::new();
        let signed_archive = std::fs::read(fixture.directory.path().join("cli.tar.gz")).unwrap();
        assert!(fixture.run(fixture.manual(Channel::Preview)));
        let binary = std::fs::read(&fixture.installation.executable).unwrap();
        let output = std::process::Command::new(&fixture.installation.executable)
            .arg("--version")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"vmux 9.0.0\n");
        fixture.installation.version = "9.0.0".into();
        let requests = fixture.requests();
        assert!(requests.contains("/nightly/updates-preview.json"));
        assert!(fixture.run(Request::automatic()));
        assert_eq!(fixture.requests(), requests);
        let preferences = fixture.installation.load().unwrap();
        assert!(preferences.channel == Channel::Preview);
        assert!(preferences.last_check.is_some());

        fixture.installation.version = "8.0.0".into();
        std::fs::write(
            fixture.directory.path().join("cli.tar.gz"),
            b"corrupt archive",
        )
        .unwrap();
        assert!(!fixture.run(fixture.manual(Channel::Stable)));
        assert_eq!(
            std::fs::read(&fixture.installation.executable).unwrap(),
            binary
        );
        assert!(fixture.installation.load().unwrap().channel == Channel::Stable);
        assert!(fixture.requests().contains("https://vmux.ai/updates.json"));
        fixture.installation.version = "10.0.0".into();
        std::fs::write(
            &fixture.installation.executable,
            "#!/bin/sh\nprintf 'vmux 10.0.0\\n'\n",
        )
        .unwrap();
        std::fs::write(fixture.directory.path().join("cli.tar.gz"), signed_archive).unwrap();
        assert!(fixture.run(fixture.manual(Channel::Stable)));
        let output = std::process::Command::new(&fixture.installation.executable)
            .arg("--version")
            .output()
            .unwrap();
        assert_eq!(output.stdout, b"vmux 9.0.0\n");
        let mut disabled = fixture.installation.load().unwrap();
        disabled.enabled = false;
        disabled.last_check = None;
        fixture.installation.save(&disabled).unwrap();
        let requests = fixture.requests();
        assert!(fixture.run(Request::automatic()));
        assert_eq!(fixture.requests(), requests);
    }
}
