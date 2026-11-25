use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;
use vmux_ecs::profile::{ActiveProfile, CurrentProfile, ProfileDirectories};
use vmux_ecs::time::{Clock, Deadline, DeadlinePlugin, DeadlineReached, TimeWake};
use vmux_input::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
};
use vmux_setting::AppSettings;

use super::native::{CaptureHandle, CapturePorts, OpenRequest};
use super::{GIF_FPS, GIF_MAX_EDGE, RECORDING_BITRATE_BPS, RECORDING_MAX_EDGE, RecordingStatus};
use crate::capture_output::{CaptureOutput, CaptureSize, CaptureSource, CropRect, ResolvedCapture};

pub(crate) struct RecordingPlugin;

struct CaptureTimeout;

impl Plugin for RecordingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DeadlinePlugin::<CaptureTimeout>::default())
            .add_observer(on_timeout)
            .add_message::<RecordStartRequest>()
            .add_message::<AppExit>()
            .add_message::<RecordStartResponse>()
            .add_message::<RecordStopRequest>()
            .add_message::<RecordStopResponse>()
            .add_message::<PauseRecording>()
            .add_message::<ResumeRecording>()
            .add_message::<FinishRecording>()
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                (
                    (
                        sync_wake,
                        start,
                        ApplyDeferred,
                        open,
                        receive_opened,
                        ApplyDeferred,
                        stop,
                        control,
                        expire,
                        ApplyDeferred,
                        finish,
                        receive_encoded,
                        ApplyDeferred,
                    )
                        .chain(),
                    (
                        select_output,
                        prepare_parent,
                        publish_mp4,
                        publish_gif,
                        measure,
                        cleanup,
                        ApplyDeferred,
                        respond,
                        retire,
                        project,
                    )
                        .chain(),
                )
                    .chain()
                    .after(vmux_command::WriteCommandRequests),
            )
            .add_systems(Last, exit);
    }
}

#[derive(Message)]
pub(crate) struct PauseRecording;

#[derive(Message)]
pub(crate) struct ResumeRecording;

#[derive(Message)]
pub(crate) struct FinishRecording;

#[derive(Component, Default)]
#[require(RecordingStatus)]
struct CurrentAttempt(Option<vmux_ecs::EntityTarget<Attempt>>);

impl CurrentAttempt {
    fn entity(&self) -> Option<Entity> {
        self.0.map(|target| target.entity())
    }
}

#[derive(Component)]
#[require(Phase)]
struct Attempt(RecordStartRequest);

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    Preparing,
    Capturing,
    Paused,
    Finalizing,
    Publishing,
    Complete,
    Failed,
    Canceled,
}

impl Phase {
    fn settled(self) -> bool {
        matches!(self, Self::Complete | Self::Failed)
    }
}

#[derive(Component)]
pub(super) struct Source {
    window: vmux_ecs::EntityTarget<Window>,
    profile: vmux_ecs::EntityTarget<ActiveProfile>,
    identity: vmux_profile::Profile,
    namespace: PathBuf,
    temporary_root: PathBuf,
    output_directory: PathBuf,
    width: u32,
    height: u32,
    crop: Option<CropRect>,
    scale: f64,
}

impl Source {
    fn new(
        capture: ResolvedCapture,
        profile: Entity,
        identity: vmux_profile::Profile,
        paths: &vmux_profile::ProfilePaths,
        output_directory: PathBuf,
    ) -> Self {
        let size = match capture.crop {
            Some(crop) => CaptureSize::new(crop.w, crop.h),
            None => capture.size,
        }
        .downscaled(RECORDING_MAX_EDGE);
        Self {
            window: vmux_ecs::EntityTarget::new(capture.window),
            profile: vmux_ecs::EntityTarget::new(profile),
            identity,
            namespace: paths.shared_data(),
            temporary_root: paths.recording(),
            output_directory,
            width: size.width,
            height: size.height,
            crop: capture.crop,
            scale: capture.scale,
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct Scope<'w, 's> {
    windows: Query<'w, 's, (), With<Window>>,
    profiles: Query<'w, 's, (&'static ActiveProfile, &'static ProfileDirectories)>,
}

impl Scope<'_, '_> {
    fn valid(&self, source: &Source) -> bool {
        self.windows.contains(source.window.entity())
            && self
                .profiles
                .get(source.profile.entity())
                .is_ok_and(|(profile, paths)| {
                    profile.0 == source.identity && paths.0.shared_data() == source.namespace
                })
    }
}

#[derive(Component)]
struct TemporaryOutputs {
    directory: PathBuf,
    mp4: PathBuf,
    gif: Option<PathBuf>,
}

impl TemporaryOutputs {
    fn new(root: &std::path::Path, attempt: Entity, gif: bool) -> Self {
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S-%9f");
        let directory = root.join(format!(".vmux-recording-{stamp}-{}", attempt.to_bits()));
        Self {
            mp4: directory.join("recording.mp4"),
            gif: gif.then(|| directory.join("recording.gif")),
            directory,
        }
    }
}

#[derive(Component)]
struct ReservedTemporaryDirectory;

#[derive(Component)]
struct OpenSubmitted;

#[derive(Component)]
struct FinishSubmitted;

#[derive(Component)]
struct StartResponded;

#[derive(Component)]
struct Failure(String);

#[derive(Component)]
struct CleanupFailure(String);

#[derive(Component)]
struct ExitRequested(AppExit);

#[derive(Component)]
struct TimedOut;

fn on_timeout(reached: On<DeadlineReached<CaptureTimeout>>, mut commands: Commands) {
    commands.entity(reached.entity).try_insert(TimedOut);
}

#[derive(Component)]
struct StopReceipt {
    attempt: Entity,
    request_id: [u8; 16],
}

#[derive(Component)]
struct StopIntent {
    directory: Option<String>,
    name: Option<String>,
    automatic: bool,
}

impl StopIntent {
    fn automatic() -> Self {
        Self {
            directory: None,
            name: None,
            automatic: true,
        }
    }
}

#[derive(Component)]
struct EncodedDuration(u64);

#[derive(Component)]
struct OutputPaths {
    mp4: PathBuf,
    gif: Option<PathBuf>,
}

#[derive(Component)]
struct ParentPrepared;

#[derive(Component)]
struct Mp4Published;

#[derive(Component)]
struct GifPublished;

#[derive(Component)]
struct TerminalResult(Result<RecordingInfo, String>);

fn spawn(mut commands: Commands, wake: Option<Res<TimeWake>>) {
    let wake = wake.map(|wake| {
        let wake = wake.clone();
        Arc::new(move || wake.wake()) as super::WakeFn
    });
    commands.spawn((
        Name::new("Recording capture"),
        CurrentAttempt::default(),
        CapturePorts::new(wake),
    ));
}

fn sync_wake(
    wake: Option<Res<TimeWake>>,
    runtime: Query<Entity, With<CapturePorts>>,
    attempts: Query<(), With<Attempt>>,
    mut commands: Commands,
) {
    let Some(wake) = wake else {
        return;
    };
    if !wake.is_changed() || !attempts.is_empty() {
        return;
    }
    let Ok(entity) = runtime.single() else {
        return;
    };
    let wake = wake.clone();
    commands
        .entity(entity)
        .insert(CapturePorts::new(Some(Arc::new(move || wake.wake()))));
}

fn start(
    mut requests: MessageReader<RecordStartRequest>,
    mut responses: MessageWriter<RecordStartResponse>,
    mut runtime: Query<(&mut CurrentAttempt, Has<ExitRequested>)>,
    attempts: Query<&Phase, With<Attempt>>,
    profile: CurrentProfile,
    source: CaptureSource,
    settings: Res<AppSettings>,
    clock: Res<Clock>,
    mut commands: Commands,
) {
    let Ok((mut current, exiting)) = runtime.single_mut() else {
        return;
    };
    for request in requests.read() {
        let error = if exiting {
            Some("application is exiting".into())
        } else if !vmux_profile::Product::screen() {
            Some("Screen recording is disabled by this application.".into())
        } else if current
            .entity()
            .is_some_and(|entity| !attempts.get(entity).is_ok_and(|phase| phase.settled()))
        {
            Some("a recording is already in progress; stop it first".into())
        } else {
            None
        };
        if let Some(error) = error {
            responses.write(RecordStartResponse {
                request_id: request.request_id,
                result: Err(error),
            });
            continue;
        }
        let capture = match source.resolve(request.pane.as_deref()) {
            Ok(capture) => capture,
            Err(error) => {
                responses.write(RecordStartResponse {
                    request_id: request.request_id,
                    result: Err(error),
                });
                continue;
            }
        };
        let (Some(owner), Some(identity), Some(paths)) =
            (profile.entity(), profile.profile(), profile.paths())
        else {
            responses.write(RecordStartResponse {
                request_id: request.request_id,
                result: Err("no active recording profile".into()),
            });
            continue;
        };
        let entity = commands
            .spawn(vmux_ecs::Operation::bundle("recording.capture").unwrap())
            .id();
        let output_directory = match settings.recording.output_dir.as_deref() {
            Some(path) if !path.trim().is_empty() => PathBuf::from(path.trim()),
            _ => paths.recording(),
        };
        commands.entity(entity).insert((
            Name::new("Recording attempt"),
            vmux_ecs::OperationRetained,
            Attempt(request.clone()),
            Source::new(capture, owner, identity.clone(), paths, output_directory),
            TemporaryOutputs::new(&paths.recording(), entity, request.gif),
            Deadline::<CaptureTimeout>::after(&clock, Duration::from_secs(8)),
        ));
        current.0 = Some(vmux_ecs::EntityTarget::new(entity));
    }
}

fn open(
    _non_send: NonSendMarker,
    ports: Query<&CapturePorts>,
    requests: Query<
        (Entity, &Source, &TemporaryOutputs),
        (
            With<Attempt>,
            Without<OpenSubmitted>,
            Without<StartResponded>,
        ),
    >,
    scope: Scope,
    mut responses: MessageWriter<RecordStartResponse>,
    attempts: Query<&Attempt>,
    mut commands: Commands,
) {
    let Ok(ports) = ports.single() else {
        return;
    };
    for (entity, source, paths) in &requests {
        if !scope.valid(source) {
            let request = attempts.get(entity).unwrap();
            let error = "recording source was retired".to_string();
            responses.write(RecordStartResponse {
                request_id: request.0.request_id,
                result: Err(error.clone()),
            });
            commands
                .entity(entity)
                .insert((
                    StartResponded,
                    Failure(error.clone()),
                    Phase::Failed,
                    TerminalResult(Err(error)),
                ))
                .remove::<(Deadline<CaptureTimeout>, TimedOut)>();
            continue;
        }
        if let Err(error) = Filesystem::ensure(&source.temporary_root)
            .and_then(|()| Filesystem::reserve(&paths.directory))
        {
            let request = attempts.get(entity).unwrap();
            responses.write(RecordStartResponse {
                request_id: request.0.request_id,
                result: Err(error.to_string()),
            });
            commands
                .entity(entity)
                .insert((
                    StartResponded,
                    Failure(error.to_string()),
                    Phase::Failed,
                    TerminalResult(Err(error.to_string())),
                ))
                .remove::<(Deadline<CaptureTimeout>, TimedOut)>();
            continue;
        }
        commands.entity(entity).insert(ReservedTemporaryDirectory);
        let request = OpenRequest {
            attempt: entity,
            window: source.window.entity(),
            width: source.width,
            height: source.height,
            crop: source.crop,
            scale: source.scale,
            bitrate: RECORDING_BITRATE_BPS,
            frame_rate: 60,
            gif_fps: GIF_FPS,
            gif_max_edge: GIF_MAX_EDGE,
            temp_mp4: paths.mp4.clone(),
            temp_gif: paths.gif.clone(),
        };
        match ports.open(request) {
            Ok(()) => {
                commands.entity(entity).insert(OpenSubmitted);
            }
            Err(error) => {
                let request = attempts.get(entity).unwrap();
                responses.write(RecordStartResponse {
                    request_id: request.0.request_id,
                    result: Err(error.clone()),
                });
                commands
                    .entity(entity)
                    .insert((StartResponded, Failure(error), Phase::Canceled))
                    .remove::<(Deadline<CaptureTimeout>, TimedOut)>();
            }
        }
    }
}

fn receive_opened(
    ports: Query<&CapturePorts>,
    mut attempts: Query<(&Attempt, &mut Phase, Option<&StartResponded>), With<Attempt>>,
    mut responses: MessageWriter<RecordStartResponse>,
    mut commands: Commands,
) {
    for ports in &ports {
        while let Some(receipt) = ports.opened() {
            let Ok((attempt, mut phase, responded)) = attempts.get_mut(receipt.attempt) else {
                if let Some(handle) = receipt.handle {
                    handle.finish();
                }
                continue;
            };
            if *phase != Phase::Preparing && *phase != Phase::Canceled {
                continue;
            }
            commands
                .entity(receipt.attempt)
                .remove::<(OpenSubmitted, Deadline<CaptureTimeout>, TimedOut)>();
            if let Some(handle) = receipt.handle {
                commands.entity(receipt.attempt).insert(handle);
            }
            if *phase == Phase::Canceled {
                continue;
            }
            if responded.is_none() {
                responses.write(RecordStartResponse {
                    request_id: attempt.0.request_id,
                    result: receipt
                        .result
                        .as_ref()
                        .map(|_| attempt.0.max_secs)
                        .map_err(Clone::clone),
                });
                commands.entity(receipt.attempt).insert(StartResponded);
            }
            match receipt.result {
                Ok(started) => {
                    *phase = Phase::Capturing;
                    commands
                        .entity(receipt.attempt)
                        .insert(Deadline::<CaptureTimeout>::at(
                            started + Duration::from_secs(u64::from(attempt.0.max_secs)),
                        ));
                }
                Err(error) => {
                    *phase = Phase::Canceled;
                    commands.entity(receipt.attempt).insert(Failure(error));
                }
            }
        }
    }
}

fn stop(
    mut requests: MessageReader<RecordStopRequest>,
    current: Query<&CurrentAttempt>,
    attempts: Query<
        (
            Option<&StopIntent>,
            Option<&TerminalResult>,
            Option<&CleanupFailure>,
        ),
        With<Attempt>,
    >,
    mut responses: MessageWriter<RecordStopResponse>,
    mut commands: Commands,
) {
    let entity = current.single().ok().and_then(|current| current.entity());
    let mut accepted = false;
    for request in requests.read() {
        let Some(entity) = entity else {
            responses.write(RecordStopResponse {
                request_id: request.request_id,
                result: Err("no recording available".into()),
            });
            continue;
        };
        let Ok((intent, result, cleanup)) = attempts.get(entity) else {
            responses.write(RecordStopResponse {
                request_id: request.request_id,
                result: Err("recording attempt is unavailable".into()),
            });
            continue;
        };
        if let Some(result) = result {
            if let Some(cleanup) = cleanup {
                warn!("recording: retrying retained cleanup: {}", cleanup.0);
                commands.entity(entity).remove::<CleanupFailure>();
            }
            responses.write(RecordStopResponse {
                request_id: request.request_id,
                result: result.0.clone(),
            });
            continue;
        }
        commands.spawn((
            vmux_ecs::Operation::bundle("recording.stop").unwrap(),
            vmux_ecs::OperationRetained,
            StopReceipt {
                attempt: entity,
                request_id: request.request_id,
            },
        ));
        if intent.is_none() && !accepted {
            commands.entity(entity).insert(StopIntent {
                directory: request.dir.clone(),
                name: request.name.clone(),
                automatic: false,
            });
            accepted = true;
        }
    }
}

fn control(
    _non_send: NonSendMarker,
    mut pause: MessageReader<PauseRecording>,
    mut resume: MessageReader<ResumeRecording>,
    mut done: MessageReader<FinishRecording>,
    current: Query<&CurrentAttempt>,
    mut attempts: Query<(&mut Phase, Option<&CaptureHandle>, Option<&StopIntent>), With<Attempt>>,
    mut commands: Commands,
) {
    let pause = pause.read().count() > 0;
    let resume = resume.read().count() > 0;
    let done = done.read().count() > 0;
    let Some(entity) = current.single().ok().and_then(|current| current.entity()) else {
        return;
    };
    let Ok((mut phase, handle, intent)) = attempts.get_mut(entity) else {
        return;
    };
    if pause
        && *phase == Phase::Capturing
        && let Some(handle) = handle
    {
        handle.pause();
        *phase = Phase::Paused;
    }
    if resume
        && *phase == Phase::Paused
        && let Some(handle) = handle
    {
        handle.resume();
        *phase = Phase::Capturing;
    }
    if done && intent.is_none() && !phase.settled() {
        commands.entity(entity).insert(StopIntent::automatic());
    }
}

fn expire(
    mut attempts: Query<(
        Entity,
        &Attempt,
        &Source,
        &mut Phase,
        Option<&Deadline<CaptureTimeout>>,
        Has<TimedOut>,
        Option<&StopIntent>,
        Option<&StartResponded>,
        &vmux_ecs::OperationState,
    )>,
    clock: Res<Clock>,
    scope: Scope,
    mut responses: MessageWriter<RecordStartResponse>,
    mut commands: Commands,
) {
    for (entity, attempt, source, mut phase, deadline, timed_out, intent, responded, operation) in
        &mut attempts
    {
        if phase.settled() {
            continue;
        }
        let owner = scope.valid(source);
        let expired = timed_out || deadline.is_some_and(|deadline| deadline.is_due(&clock));
        if !owner
            || *operation == vmux_ecs::OperationState::Canceled
            || expired && *phase == Phase::Preparing
        {
            let error = if !owner {
                "recording source was retired"
            } else if *operation == vmux_ecs::OperationState::Canceled {
                "recording operation was canceled"
            } else {
                "timed out preparing capture"
            };
            if responded.is_none() {
                responses.write(RecordStartResponse {
                    request_id: attempt.0.request_id,
                    result: Err(error.into()),
                });
                commands.entity(entity).insert(StartResponded);
            }
            *phase = Phase::Canceled;
            commands
                .entity(entity)
                .insert(Failure(error.into()))
                .remove::<(Deadline<CaptureTimeout>, TimedOut)>();
        } else if expired && intent.is_none() {
            commands
                .entity(entity)
                .insert(StopIntent::automatic())
                .remove::<(Deadline<CaptureTimeout>, TimedOut)>();
        }
    }
}

fn finish(
    _non_send: NonSendMarker,
    mut attempts: Query<
        (Entity, &mut Phase, &CaptureHandle, Option<&StopIntent>),
        Without<FinishSubmitted>,
    >,
    mut commands: Commands,
) {
    for (entity, mut phase, handle, intent) in &mut attempts {
        if *phase == Phase::Canceled
            || intent.is_some() && matches!(*phase, Phase::Capturing | Phase::Paused)
        {
            handle.finish();
            if *phase != Phase::Canceled {
                *phase = Phase::Finalizing;
            }
            commands
                .entity(entity)
                .insert(FinishSubmitted)
                .remove::<(Deadline<CaptureTimeout>, TimedOut)>();
        }
    }
}

fn receive_encoded(
    ports: Query<&CapturePorts>,
    mut attempts: Query<(&mut Phase, Option<&FinishSubmitted>), With<Attempt>>,
    mut commands: Commands,
) {
    for ports in &ports {
        while let Some(receipt) = ports.encoded() {
            let Ok((mut phase, submitted)) = attempts.get_mut(receipt.attempt) else {
                continue;
            };
            if submitted.is_none() || !matches!(*phase, Phase::Finalizing | Phase::Canceled) {
                continue;
            }
            commands
                .entity(receipt.attempt)
                .remove::<CaptureHandle>()
                .remove::<FinishSubmitted>();
            if *phase == Phase::Canceled {
                continue;
            }
            match receipt.result {
                Ok(duration) => {
                    *phase = Phase::Publishing;
                    commands
                        .entity(receipt.attempt)
                        .insert(EncodedDuration(duration));
                }
                Err(error) => {
                    *phase = Phase::Canceled;
                    commands.entity(receipt.attempt).insert(Failure(error));
                }
            }
        }
    }
}

fn select_output(
    attempts: Query<
        (Entity, &Phase, &Source, &TemporaryOutputs, &StopIntent),
        Without<OutputPaths>,
    >,
    mut commands: Commands,
) {
    for (entity, phase, source, temporary, intent) in &attempts {
        if *phase != Phase::Publishing {
            continue;
        }
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
        let (mp4, gif) = CaptureOutput::paths(
            intent.directory.as_deref(),
            intent.name.as_deref(),
            temporary.gif.is_some(),
            &timestamp,
            &source.output_directory,
        );
        commands.entity(entity).insert(OutputPaths { mp4, gif });
    }
}

fn prepare_parent(
    mut attempts: Query<(Entity, &mut Phase, &Source, &OutputPaths), Without<ParentPrepared>>,
    scope: Scope,
    mut commands: Commands,
) {
    for (entity, mut phase, source, paths) in &mut attempts {
        if *phase != Phase::Publishing {
            continue;
        }
        if !scope.valid(source) {
            *phase = Phase::Canceled;
            commands
                .entity(entity)
                .insert(Failure("recording source was retired".into()));
            continue;
        }
        let result = match paths.mp4.parent() {
            Some(path) => Filesystem::ensure(path),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "recording output parent is unavailable",
            )),
        };
        match result {
            Ok(()) => {
                commands.entity(entity).insert(ParentPrepared);
            }
            Err(error) => {
                *phase = Phase::Canceled;
                commands.entity(entity).insert(Failure(error.to_string()));
            }
        }
    }
}

fn publish_mp4(
    mut attempts: Query<
        (Entity, &mut Phase, &Source, &TemporaryOutputs, &OutputPaths),
        (With<ParentPrepared>, Without<Mp4Published>),
    >,
    scope: Scope,
    mut commands: Commands,
) {
    for (entity, mut phase, source, temporary, paths) in &mut attempts {
        if *phase != Phase::Publishing {
            continue;
        }
        if !scope.valid(source) {
            *phase = Phase::Canceled;
            commands
                .entity(entity)
                .insert(Failure("recording source was retired".into()));
            continue;
        }
        match Filesystem::relocate(&temporary.mp4, &paths.mp4) {
            Ok(()) => {
                commands.entity(entity).insert(Mp4Published);
            }
            Err(error) => {
                *phase = Phase::Canceled;
                commands.entity(entity).insert(Failure(error.to_string()));
            }
        }
    }
}

fn publish_gif(
    mut attempts: Query<
        (Entity, &mut Phase, &Source, &TemporaryOutputs, &OutputPaths),
        (With<Mp4Published>, Without<GifPublished>),
    >,
    scope: Scope,
    mut commands: Commands,
) {
    for (entity, mut phase, source, temporary, paths) in &mut attempts {
        if *phase != Phase::Publishing {
            continue;
        }
        if !scope.valid(source) {
            *phase = Phase::Canceled;
            commands
                .entity(entity)
                .insert(Failure("recording source was retired".into()));
            continue;
        }
        let result = match (&temporary.gif, &paths.gif) {
            (Some(source), Some(target)) => Filesystem::relocate(source, target),
            _ => Ok(()),
        };
        match result {
            Ok(()) => {
                commands.entity(entity).insert(GifPublished);
            }
            Err(error) => {
                *phase = Phase::Canceled;
                commands.entity(entity).insert(Failure(error.to_string()));
            }
        }
    }
}

fn measure(
    mut attempts: Query<
        (
            Entity,
            &mut Phase,
            &Source,
            &OutputPaths,
            &StopIntent,
            &EncodedDuration,
        ),
        (With<Mp4Published>, With<GifPublished>),
    >,
    scope: Scope,
    mut commands: Commands,
) {
    for (entity, mut phase, source, paths, intent, duration) in &mut attempts {
        if *phase != Phase::Publishing {
            continue;
        }
        if !scope.valid(source) {
            *phase = Phase::Canceled;
            commands
                .entity(entity)
                .insert(Failure("recording source was retired".into()));
            continue;
        }
        match Filesystem::size(&paths.mp4) {
            Ok(bytes) => {
                commands
                    .entity(entity)
                    .insert(TerminalResult(Ok(RecordingInfo {
                        mp4_path: paths.mp4.to_string_lossy().into_owned(),
                        gif_path: paths
                            .gif
                            .as_ref()
                            .map(|path| path.to_string_lossy().into_owned()),
                        duration_ms: duration.0,
                        bytes,
                        auto_stopped: intent.automatic,
                    })));
                *phase = Phase::Complete;
            }
            Err(error) => {
                *phase = Phase::Canceled;
                commands.entity(entity).insert(Failure(error.to_string()));
            }
        }
    }
}

fn cleanup(
    mut attempts: Query<
        (
            Entity,
            &mut Phase,
            &TemporaryOutputs,
            Option<&Failure>,
            Has<ReservedTemporaryDirectory>,
        ),
        (
            Without<OpenSubmitted>,
            Without<FinishSubmitted>,
            Without<CaptureHandle>,
            Without<CleanupFailure>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, mut phase, paths, failure, reserved) in &mut attempts {
        if !matches!(*phase, Phase::Canceled | Phase::Complete | Phase::Failed) {
            continue;
        }
        if reserved {
            let mut failure = None;
            for path in [Some(&paths.mp4), paths.gif.as_ref()] {
                let Some(path) = path else {
                    continue;
                };
                if let Err(error) = Filesystem::remove_file(path)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    failure = Some(error);
                    break;
                }
            }
            if failure.is_none()
                && let Err(error) = Filesystem::remove_directory(&paths.directory)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                failure = Some(error);
            }
            match failure {
                None => {
                    commands
                        .entity(entity)
                        .remove::<ReservedTemporaryDirectory>();
                }
                Some(error) => {
                    warn!(
                        "recording: retained temporary directory {}: {}",
                        paths.directory.display(),
                        error
                    );
                    commands
                        .entity(entity)
                        .insert(CleanupFailure(error.to_string()));
                }
            }
        }
        if *phase == Phase::Canceled {
            let error = failure.map_or("recording canceled", |failure| failure.0.as_str());
            commands
                .entity(entity)
                .insert(TerminalResult(Err(error.into())));
            *phase = Phase::Failed;
        }
    }
}

fn respond(
    receipts: Query<(Entity, &StopReceipt)>,
    results: Query<&TerminalResult, With<Attempt>>,
    mut responses: MessageWriter<RecordStopResponse>,
    mut commands: Commands,
) {
    for (entity, receipt) in &receipts {
        let Ok(result) = results.get(receipt.attempt) else {
            continue;
        };
        responses.write(RecordStopResponse {
            request_id: receipt.request_id,
            result: result.0.clone(),
        });
        commands.entity(entity).despawn();
    }
}

fn retire(
    current: Query<&CurrentAttempt>,
    attempts: Query<(Entity, &Phase), (With<Attempt>, Without<ReservedTemporaryDirectory>)>,
    receipts: Query<&StopReceipt>,
    mut commands: Commands,
) {
    let current = current.single().ok().and_then(|current| current.entity());
    for (entity, phase) in &attempts {
        if phase.settled()
            && Some(entity) != current
            && !receipts.iter().any(|receipt| receipt.attempt == entity)
        {
            commands.entity(entity).despawn();
        }
    }
}

fn project(
    mut current: Query<(&CurrentAttempt, &mut RecordingStatus)>,
    attempts: Query<&Phase, With<Attempt>>,
) {
    for (current, mut status) in &mut current {
        *status = match current
            .entity()
            .and_then(|entity| attempts.get(entity).ok())
        {
            Some(Phase::Capturing) => RecordingStatus::Recording,
            Some(Phase::Paused) => RecordingStatus::Paused,
            _ => RecordingStatus::Idle,
        };
    }
}

fn exit(
    mut exits: ResMut<Messages<AppExit>>,
    runtime: Query<(Entity, &CapturePorts, Option<&ExitRequested>)>,
    mut attempts: Query<(Entity, &Attempt, &mut Phase, Has<StartResponded>), With<Attempt>>,
    mut starts: MessageWriter<RecordStartResponse>,
    wake: Option<Res<TimeWake>>,
    mut commands: Commands,
) {
    let Ok((runtime, ports, pending)) = runtime.single() else {
        return;
    };
    let mut requested = pending.map(|pending| pending.0.clone());
    for exit in exits.drain() {
        if requested.as_ref().is_none_or(|current| !current.is_error()) {
            requested = Some(exit);
        }
    }
    let Some(requested) = requested else {
        return;
    };
    let mut waiting = false;
    for (entity, attempt, mut phase, responded) in &mut attempts {
        if phase.settled() {
            continue;
        }
        waiting = true;
        *phase = Phase::Canceled;
        let error = "recording canceled by application exit";
        commands
            .entity(entity)
            .insert(Failure(error.into()))
            .remove::<(Deadline<CaptureTimeout>, TimedOut)>();
        if !responded {
            starts.write(RecordStartResponse {
                request_id: attempt.0.request_id,
                result: Err(error.into()),
            });
            commands.entity(entity).insert(StartResponded);
        }
    }
    if waiting {
        commands.entity(runtime).insert(ExitRequested(requested));
        ports.wake();
        if let Some(wake) = wake {
            wake.wake();
        }
    } else {
        commands.entity(runtime).remove::<ExitRequested>();
        exits.write(requested);
    }
}

struct Filesystem;

impl Filesystem {
    fn ensure(path: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn reserve(path: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir(path)
    }

    fn relocate(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
        std::fs::rename(source, target)
    }

    fn size(path: &std::path::Path) -> std::io::Result<u64> {
        std::fs::metadata(path).map(|metadata| metadata.len())
    }

    fn remove_file(path: &std::path::Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_directory(path: &std::path::Path) -> std::io::Result<()> {
        std::fs::remove_dir(path)
    }
}

#[cfg(test)]
mod tests {
    use super::super::native::{CaptureControl, Completion, Encoded, OpenCapture, Opened};
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Default)]
    struct NativeFixture {
        pending: Mutex<Option<(OpenRequest, Completion)>>,
        opens: AtomicUsize,
    }

    impl OpenCapture for NativeFixture {
        fn open(&self, request: OpenRequest, completion: Completion) -> Result<(), String> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            let mut pending = self.pending.lock().unwrap();
            assert!(
                pending.is_none(),
                "native open was submitted twice without a receipt"
            );
            *pending = Some((request, completion));
            Ok(())
        }
    }

    #[derive(Default)]
    struct HandleFixture {
        finished: AtomicBool,
        finishes: AtomicUsize,
        pauses: AtomicUsize,
        resumes: AtomicUsize,
    }

    impl CaptureControl for HandleFixture {
        fn pause(&self) {
            self.pauses.fetch_add(1, Ordering::SeqCst);
        }

        fn resume(&self) {
            self.resumes.fetch_add(1, Ordering::SeqCst);
        }

        fn finish(self: Arc<Self>) {
            if !self.finished.swap(true, Ordering::SeqCst) {
                self.finishes.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        backend: Arc<NativeFixture>,
        native: Option<(OpenRequest, Completion, Arc<HandleFixture>)>,
        window: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let mut settings = AppSettings::default();
            settings.recording.output_dir = Some(
                directory
                    .path()
                    .join("output")
                    .to_string_lossy()
                    .into_owned(),
            );
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, RecordingPlugin))
                .insert_resource(settings);
            let window = app
                .world_mut()
                .spawn((Window::default(), vmux_ecs::Active))
                .id();
            let identity = vmux_profile::Profile::named("personal");
            app.world_mut().spawn((
                ActiveProfile(identity.clone()),
                ProfileDirectories(vmux_profile::ProfilePaths::at(
                    identity,
                    directory.path().to_path_buf(),
                )),
                Name::new("Fixture profile"),
            ));
            app.update();
            let backend = Arc::new(NativeFixture::default());
            let mut ports = app.world_mut().query::<&mut CapturePorts>();
            ports
                .single_mut(app.world_mut())
                .unwrap()
                .replace_backend(backend.clone());
            Self {
                app,
                directory,
                backend,
                native: None,
                window,
            }
        }

        fn start(&mut self, request_id: [u8; 16], gif: bool) {
            self.app.world_mut().write_message(RecordStartRequest {
                request_id,
                gif,
                max_secs: 30,
                pane: None,
            });
            self.app.update();
        }

        fn opened(&mut self) {
            let (request, completion) = self
                .backend
                .pending
                .lock()
                .unwrap()
                .take()
                .expect("an accepted native open");
            let handle = Arc::new(HandleFixture::default());
            completion.opened(Opened {
                attempt: request.attempt,
                handle: Some(CaptureHandle::new(handle.clone())),
                result: Ok(Instant::now()),
            });
            self.native = Some((request, completion, handle));
            self.app.update();
        }

        fn refused(&mut self, error: &str) {
            let (request, completion) = self
                .backend
                .pending
                .lock()
                .unwrap()
                .take()
                .expect("an accepted native open");
            completion.opened(Opened {
                attempt: request.attempt,
                handle: None,
                result: Err(error.into()),
            });
            self.app.update();
        }

        fn stop(&mut self, request_id: [u8; 16], name: &str) {
            self.app.world_mut().write_message(RecordStopRequest {
                request_id,
                dir: None,
                name: Some(name.into()),
            });
            self.app.update();
        }

        fn encoded(&mut self, result: Result<u64, &str>, gif_present: bool) {
            let (request, completion, _) = self.native.as_ref().unwrap();
            std::fs::write(&request.temp_mp4, b"independent MP4 fixture bytes").unwrap();
            if gif_present && let Some(path) = &request.temp_gif {
                std::fs::write(path, b"independent GIF fixture bytes").unwrap();
            }
            completion.encoded(Encoded {
                attempt: request.attempt,
                result: result.map_err(str::to_string),
            });
            self.app.update();
        }

        fn stop_responses(&mut self) -> Vec<RecordStopResponse> {
            self.app
                .world_mut()
                .resource_mut::<Messages<RecordStopResponse>>()
                .drain()
                .collect()
        }

        fn start_responses(&mut self) -> Vec<RecordStartResponse> {
            self.app
                .world_mut()
                .resource_mut::<Messages<RecordStartResponse>>()
                .drain()
                .collect()
        }

        fn automatic(&mut self) {
            self.app.world_mut().write_message(FinishRecording);
            self.app.update();
            self.encoded(Ok(750), true);
        }
    }

    #[test]
    fn a_previous_auto_result_cannot_replace_a_new_attempt_failure() {
        let mut fixture = Fixture::new();
        fixture.start([1; 16], false);
        fixture.opened();
        fixture.automatic();
        fixture.start([2; 16], false);
        fixture.refused("second capture refused");
        fixture.stop([3; 16], "second");
        let responses = fixture.stop_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, [3; 16]);
        assert_eq!(
            responses[0].result.as_ref().err().unwrap(),
            "second capture refused"
        );
    }

    #[test]
    fn stop_after_auto_completion_returns_the_current_artifact_without_native_work() {
        let mut fixture = Fixture::new();
        fixture.start([4; 16], true);
        fixture.opened();
        fixture.automatic();
        fixture.stop([5; 16], "unused-destination");
        let responses = fixture.stop_responses();
        assert_eq!(responses.len(), 1);
        let result = responses[0].result.as_ref().unwrap();
        assert!(result.auto_stopped);
        assert_eq!(result.duration_ms, 750);
        assert_eq!(
            std::fs::read(&result.mp4_path).unwrap(),
            b"independent MP4 fixture bytes"
        );
        assert_eq!(
            std::fs::read(result.gif_path.as_ref().unwrap()).unwrap(),
            b"independent GIF fixture bytes"
        );
        assert_eq!(fixture.backend.opens.load(Ordering::SeqCst), 1);
        assert_eq!(
            fixture
                .native
                .as_ref()
                .unwrap()
                .2
                .finishes
                .load(Ordering::SeqCst),
            1
        );
        assert!(
            !fixture
                .directory
                .path()
                .join("output/unused-destination.mp4")
                .exists()
        );
    }

    #[test]
    fn stop_during_preparation_keeps_its_request_and_output_selection() {
        let mut fixture = Fixture::new();
        fixture.start([6; 16], false);
        fixture.stop([7; 16], "prepared-stop");
        assert!(fixture.stop_responses().is_empty());
        fixture.opened();
        assert_eq!(
            fixture
                .native
                .as_ref()
                .unwrap()
                .2
                .finishes
                .load(Ordering::SeqCst),
            1
        );
        fixture.encoded(Ok(30), false);
        let responses = fixture.stop_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, [7; 16]);
        let result = responses[0].result.as_ref().unwrap();
        assert!(!result.auto_stopped);
        assert_eq!(
            PathBuf::from(&result.mp4_path),
            fixture.directory.path().join("output/prepared-stop.mp4")
        );
    }

    #[test]
    fn same_frame_stop_receipts_share_the_first_accepted_destination() {
        let mut fixture = Fixture::new();
        fixture.start([8; 16], false);
        fixture.opened();
        for (request_id, name) in [([9; 16], "first"), ([10; 16], "second")] {
            fixture.app.world_mut().write_message(RecordStopRequest {
                request_id,
                dir: None,
                name: Some(name.into()),
            });
        }
        fixture.app.update();
        fixture.encoded(Ok(50), false);
        let responses = fixture.stop_responses();
        assert_eq!(responses.len(), 2);
        for response in responses {
            assert_eq!(
                PathBuf::from(response.result.unwrap().mp4_path),
                fixture.directory.path().join("output/first.mp4")
            );
        }
        assert!(!fixture.directory.path().join("output/second.mp4").exists());
        assert_eq!(
            fixture
                .native
                .as_ref()
                .unwrap()
                .2
                .finishes
                .load(Ordering::SeqCst),
            1
        );
    }

    #[test]
    fn a_late_native_start_after_timeout_drains_without_publication() {
        let mut fixture = Fixture::new();
        fixture.start([11; 16], false);
        fixture.stop([12; 16], "timed-out");
        let entity = fixture
            .backend
            .pending
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .0
            .attempt;
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(Deadline::<CaptureTimeout>::at(
                Instant::now() - Duration::from_secs(1),
            ));
        fixture.app.update();
        assert!(
            fixture
                .start_responses()
                .iter()
                .any(|response| response.request_id == [11; 16] && response.result.is_err())
        );
        assert!(fixture.stop_responses().is_empty());
        fixture.opened();
        assert_eq!(
            fixture
                .native
                .as_ref()
                .unwrap()
                .2
                .finishes
                .load(Ordering::SeqCst),
            1
        );
        fixture.encoded(Ok(100), false);
        let responses = fixture.stop_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(
            responses[0].result.as_ref().err().unwrap(),
            "timed out preparing capture"
        );
        assert!(
            !fixture
                .directory
                .path()
                .join("output/timed-out.mp4")
                .exists()
        );
        assert!(!fixture.native.as_ref().unwrap().0.temp_mp4.exists());
    }

    #[test]
    fn a_retired_window_drains_the_original_capture_and_refuses_its_stop() {
        let mut fixture = Fixture::new();
        fixture.start([13; 16], false);
        fixture.opened();
        fixture.stop([14; 16], "retired");
        fixture.app.world_mut().despawn(fixture.window);
        fixture.app.update();
        fixture.encoded(Ok(100), false);
        let responses = fixture.stop_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(
            responses[0].result.as_ref().err().unwrap(),
            "recording source was retired"
        );
        assert!(!fixture.directory.path().join("output/retired.mp4").exists());
        assert!(!fixture.native.as_ref().unwrap().0.temp_mp4.exists());
    }

    #[test]
    fn a_stale_encoded_receipt_cannot_clear_a_new_recording() {
        let mut fixture = Fixture::new();
        fixture.start([15; 16], false);
        fixture.opened();
        fixture.automatic();
        let (old_attempt, old_completion) = {
            let (request, completion, _) = fixture.native.as_ref().unwrap();
            (request.attempt, completion.clone())
        };
        fixture.start([16; 16], false);
        fixture.opened();
        old_completion.encoded(Encoded {
            attempt: old_attempt,
            result: Err("duplicate old error".into()),
        });
        fixture.app.update();
        let mut statuses = fixture.app.world_mut().query::<&RecordingStatus>();
        assert_eq!(
            *statuses.single(fixture.app.world()).unwrap(),
            RecordingStatus::Recording
        );
        fixture.stop([17; 16], "new");
        fixture.encoded(Ok(150), false);
        assert!(fixture.stop_responses()[0].result.is_ok());
    }

    #[test]
    fn gif_publication_failure_preserves_published_mp4_and_reports_failure() {
        let mut fixture = Fixture::new();
        fixture.start([18; 16], true);
        fixture.opened();
        fixture.stop([19; 16], "partial");
        fixture.encoded(Ok(250), false);
        assert!(fixture.stop_responses()[0].result.is_err());
        assert_eq!(
            std::fs::read(fixture.directory.path().join("output/partial.mp4")).unwrap(),
            b"independent MP4 fixture bytes"
        );
        assert!(!fixture.directory.path().join("output/partial.gif").exists());
        assert!(
            !fixture
                .native
                .as_ref()
                .unwrap()
                .0
                .temp_mp4
                .parent()
                .unwrap()
                .exists()
        );
    }

    #[test]
    fn two_same_frame_starts_submit_only_one_native_capture() {
        let mut fixture = Fixture::new();
        for request_id in [[20; 16], [21; 16]] {
            fixture.app.world_mut().write_message(RecordStartRequest {
                request_id,
                gif: false,
                max_secs: 30,
                pane: None,
            });
        }
        fixture.app.update();
        assert_eq!(fixture.backend.opens.load(Ordering::SeqCst), 1);
        let responses = fixture.start_responses();
        assert!(
            responses
                .iter()
                .any(|response| response.request_id == [21; 16] && response.result.is_err())
        );
        fixture.opened();
        assert!(
            fixture
                .start_responses()
                .iter()
                .any(|response| response.request_id == [20; 16] && response.result.is_ok())
        );
        fixture.stop([22; 16], "one");
        fixture.encoded(Ok(10), false);
        assert!(fixture.stop_responses()[0].result.is_ok());
    }

    #[test]
    fn pause_keeps_the_deadline_and_expiry_finishes_the_current_attempt() {
        let mut fixture = Fixture::new();
        fixture.start([23; 16], false);
        fixture.opened();
        fixture.app.world_mut().write_message(PauseRecording);
        fixture.app.update();
        let entity = fixture.native.as_ref().unwrap().0.attempt;
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(Deadline::<CaptureTimeout>::at(
                Instant::now() - Duration::from_secs(1),
            ));
        fixture.app.update();
        assert_eq!(
            fixture
                .native
                .as_ref()
                .unwrap()
                .2
                .pauses
                .load(Ordering::SeqCst),
            1
        );
        assert_eq!(
            fixture
                .native
                .as_ref()
                .unwrap()
                .2
                .finishes
                .load(Ordering::SeqCst),
            1
        );
        fixture.encoded(Ok(300), false);
        fixture.stop([24; 16], "expired");
        assert!(
            fixture.stop_responses()[0]
                .result
                .as_ref()
                .unwrap()
                .auto_stopped
        );
    }

    #[test]
    fn stop_without_an_attempt_replies_in_the_input_frame() {
        let mut fixture = Fixture::new();
        fixture.stop([25; 16], "missing");
        let responses = fixture.stop_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, [25; 16]);
        assert!(responses[0].result.is_err());
        assert_eq!(fixture.backend.opens.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn default_output_uses_the_accepted_profile_directory() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .resource_mut::<AppSettings>()
            .recording
            .output_dir = None;
        fixture.start([26; 16], false);
        fixture.opened();
        fixture.stop([27; 16], "profile-local");
        fixture.encoded(Ok(450), false);
        let responses = fixture.stop_responses();
        let result = responses[0].result.as_ref().unwrap();
        assert!(PathBuf::from(&result.mp4_path).starts_with(fixture.directory.path()));
        assert_eq!(
            std::fs::read(&result.mp4_path).unwrap(),
            b"independent MP4 fixture bytes"
        );
    }

    #[test]
    fn failed_temporary_cleanup_retains_custody_and_explicit_stop_retries_it() {
        let mut fixture = Fixture::new();
        fixture.start([28; 16], false);
        fixture.opened();
        let (attempt, directory) = {
            let (request, _, _) = fixture.native.as_ref().unwrap();
            (
                request.attempt,
                request.temp_mp4.parent().unwrap().to_path_buf(),
            )
        };
        let other_content = directory.join("preserve.txt");
        std::fs::write(&other_content, b"other content").unwrap();
        fixture.stop([29; 16], "saved");
        fixture.encoded(Ok(500), false);
        assert!(fixture.stop_responses()[0].result.is_ok());
        assert_eq!(std::fs::read(&other_content).unwrap(), b"other content");
        assert!(
            fixture
                .app
                .world()
                .get::<ReservedTemporaryDirectory>(attempt)
                .is_some()
        );
        assert!(fixture.app.world().get::<CleanupFailure>(attempt).is_some());
        std::fs::remove_file(other_content).unwrap();
        fixture.stop([30; 16], "ignored");
        assert!(fixture.stop_responses()[0].result.is_ok());
        assert!(!directory.exists());
        assert!(
            fixture
                .app
                .world()
                .get::<ReservedTemporaryDirectory>(attempt)
                .is_none()
        );
        assert!(fixture.directory.path().join("output/saved.mp4").exists());
    }

    #[test]
    fn canceled_operation_drains_without_publishing_its_output() {
        let mut fixture = Fixture::new();
        fixture.start([31; 16], false);
        fixture.opened();
        fixture.stop([32; 16], "canceled");
        let attempt = fixture.native.as_ref().unwrap().0.attempt;
        fixture
            .app
            .world_mut()
            .entity_mut(attempt)
            .insert(vmux_ecs::OperationState::Canceled);
        fixture.app.update();
        fixture.encoded(Ok(600), false);
        let responses = fixture.stop_responses();
        assert_eq!(
            responses[0].result.as_ref().err().unwrap(),
            "recording operation was canceled"
        );
        assert_eq!(
            fixture
                .native
                .as_ref()
                .unwrap()
                .2
                .finishes
                .load(Ordering::SeqCst),
            1
        );
        assert!(
            !fixture
                .directory
                .path()
                .join("output/canceled.mp4")
                .exists()
        );
        assert!(!fixture.native.as_ref().unwrap().0.temp_mp4.exists());
    }

    #[test]
    fn graceful_exit_waits_for_capture_drain_and_temporary_cleanup() {
        let mut fixture = Fixture::new();
        fixture.start([33; 16], false);
        fixture.opened();
        fixture.app.world_mut().write_message(AppExit::Success);
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<AppExit>>()
                .is_empty()
        );
        fixture.app.update();
        assert_eq!(
            fixture
                .native
                .as_ref()
                .unwrap()
                .2
                .finishes
                .load(Ordering::SeqCst),
            1
        );
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<AppExit>>()
                .is_empty()
        );
        fixture.encoded(Ok(800), false);
        let exits = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<AppExit>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(exits, vec![AppExit::Success]);
        assert!(
            !fixture
                .native
                .as_ref()
                .unwrap()
                .0
                .temp_mp4
                .parent()
                .unwrap()
                .exists()
        );
        assert!(!fixture.directory.path().join("output").exists());
    }

    #[test]
    fn graceful_exit_retains_late_open_and_preserves_the_original_error() {
        let mut fixture = Fixture::new();
        fixture.start([34; 16], false);
        let requested = AppExit::Error(std::num::NonZeroU8::new(7).unwrap());
        fixture.app.world_mut().write_message(requested.clone());
        fixture.app.update();
        fixture.app.world_mut().write_message(AppExit::Success);
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<AppExit>>()
                .is_empty()
        );
        fixture.opened();
        assert_eq!(
            fixture
                .native
                .as_ref()
                .unwrap()
                .2
                .finishes
                .load(Ordering::SeqCst),
            1
        );
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<AppExit>>()
                .is_empty()
        );
        fixture.encoded(Ok(900), false);
        let exits = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<AppExit>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(exits, vec![requested]);
        assert!(!fixture.native.as_ref().unwrap().0.temp_mp4.exists());
        assert!(!fixture.directory.path().join("output").exists());
    }
}
