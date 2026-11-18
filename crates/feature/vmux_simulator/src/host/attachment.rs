use super::device::{Axe, SimulatorDevice};
use super::hid::HidBroker;
use super::input::{DeviceTouchSession, SimulatorClipboard, SimulatorKeyboard};
use super::stream::StreamServer;
use super::{DevicePixels, DevicePoints};
use crate::event::SimulatorUiState;
use crate::url::SimulatorRoute;
use bevy::app::{App, Plugin};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::process::{Command, Output};
use vmux_ecs::page::PageReady;
use vmux_ecs::{
    EntityTarget, Operation, OperationCompleted, OperationPlugin, OperationRetained,
    OperationState, OperationTarget, Order, PageMetadata, UiState,
};

pub(super) struct AttachmentPlugin;

impl Plugin for AttachmentPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.add_systems(Startup, initialize)
            .add_systems(
                Update,
                (
                    poll::<Probe>,
                    poll::<Booted>,
                    poll::<Available>,
                    poll::<Boot>,
                    poll::<BootReady>,
                    poll::<Points>,
                    poll::<Pixels>,
                    poll::<HidBroker>,
                    poll::<SimulatorClipboard>,
                    poll::<SimulatorKeyboard>,
                    poll::<StreamServer>,
                )
                    .before(AttachmentSet),
            )
            .add_systems(
                Update,
                (
                    admit,
                    cancel,
                    probe,
                    materialize,
                    select,
                    boot,
                    wait_boot,
                    read_points,
                    read_pixels,
                    connect_hid,
                    connect_clipboard,
                    connect_keyboard,
                    start_stream,
                    finish,
                    retire,
                )
                    .chain()
                    .in_set(AttachmentSet),
            );
    }
}

fn read_points(
    requests: Query<
        (
            Entity,
            &Phase,
            &OperationState,
            &Axe,
            &SimulatorDevice,
            Option<&'static Points>,
        ),
        (With<Attempt>, Without<Busy>, Without<Failure>),
    >,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, phase, state, axe, device, result) in &requests {
        if *phase != Phase::Points
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        if result.is_some() {
            commands.entity(entity).insert(Phase::Pixels);
            continue;
        }
        let axe = Axe::at(axe.path().to_path_buf());
        let device = device.clone();
        let wake = wake.as_deref().map(|proxy| (**proxy).clone());
        let task = NativeTask::spawn(async move { Ok(Points(device.point_size(&axe))) }, wake);
        commands.entity(entity).insert((Busy, task));
    }
}

fn read_pixels(
    requests: Query<
        (
            Entity,
            &Phase,
            &OperationState,
            &Axe,
            &SimulatorDevice,
            Option<&'static Pixels>,
        ),
        (With<Attempt>, Without<Busy>, Without<Failure>),
    >,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, phase, state, axe, device, result) in &requests {
        if *phase != Phase::Pixels
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        if result.is_some() {
            commands.entity(entity).insert(Phase::Hid);
            continue;
        }
        let axe = Axe::at(axe.path().to_path_buf());
        let device = device.clone();
        let wake = wake.as_deref().map(|proxy| (**proxy).clone());
        let task = NativeTask::spawn(async move { Ok(Pixels(device.pixel_size(&axe))) }, wake);
        commands.entity(entity).insert((Busy, task));
    }
}

fn connect_hid(
    requests: Query<
        (
            Entity,
            &Phase,
            &OperationState,
            &Axe,
            &SimulatorDevice,
            Option<&'static HidBroker>,
        ),
        (With<Attempt>, Without<Busy>, Without<Failure>),
    >,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, phase, state, axe, device, result) in &requests {
        if *phase != Phase::Hid
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        if result.is_some() {
            commands.entity(entity).insert(Phase::Clipboard);
            continue;
        }
        let axe = Axe::at(axe.path().to_path_buf());
        let device = device.clone();
        let wake = wake.as_deref().map(|proxy| (**proxy).clone());
        let task = NativeTask::spawn(
            async move {
                HidBroker::start(&axe, &device)
                    .map_err(|error| format!("could not start simulator input: {error}"))
            },
            wake,
        );
        commands.entity(entity).insert((Busy, task));
    }
}

fn connect_clipboard(
    requests: Query<
        (
            Entity,
            &Phase,
            &OperationState,
            &Axe,
            &SimulatorDevice,
            Option<&'static SimulatorClipboard>,
        ),
        (With<Attempt>, Without<Busy>, Without<Failure>),
    >,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, phase, state, axe, device, result) in &requests {
        if *phase != Phase::Clipboard
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        if result.is_some() {
            commands.entity(entity).insert(Phase::Keyboard);
            continue;
        }
        let axe = Axe::at(axe.path().to_path_buf());
        let device = device.clone();
        let wake = wake.as_deref().map(|proxy| (**proxy).clone());
        let task = NativeTask::spawn(
            async move {
                SimulatorClipboard::start(&axe, &device)
                    .map_err(|error| format!("could not start simulator clipboard: {error}"))
            },
            wake,
        );
        commands.entity(entity).insert((Busy, task));
    }
}

fn connect_keyboard(
    requests: Query<
        (
            Entity,
            &Phase,
            &OperationState,
            &Axe,
            &SimulatorDevice,
            Option<&'static SimulatorKeyboard>,
        ),
        (With<Attempt>, Without<Busy>, Without<Failure>),
    >,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, phase, state, axe, device, result) in &requests {
        if *phase != Phase::Keyboard
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        if result.is_some() {
            commands.entity(entity).insert(Phase::Stream);
            continue;
        }
        let axe = Axe::at(axe.path().to_path_buf());
        let device = device.clone();
        let wake = wake.as_deref().map(|proxy| (**proxy).clone());
        let task = NativeTask::spawn(
            async move {
                SimulatorKeyboard::start(&axe, &device)
                    .map_err(|error| format!("could not start simulator keyboard: {error}"))
            },
            wake,
        );
        commands.entity(entity).insert((Busy, task));
    }
}

fn start_stream(
    requests: Query<
        (
            Entity,
            &Phase,
            &OperationState,
            &Axe,
            &SimulatorDevice,
            &Pixels,
            Option<&StreamServer>,
        ),
        (With<Attempt>, Without<Busy>, Without<Failure>),
    >,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, phase, state, axe, device, pixels, result) in &requests {
        if *phase != Phase::Stream
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        if result.is_some() {
            commands.entity(entity).insert(Phase::Complete);
            continue;
        }
        let axe = Axe::at(axe.path().to_path_buf());
        let device = device.clone();
        let pixels = pixels.0;
        let wake = wake.as_deref().map(|proxy| (**proxy).clone());
        let task = NativeTask::spawn(
            async move {
                StreamServer::start(&axe, device, pixels)
                    .map_err(|error| format!("could not serve the simulator stream: {error}"))
            },
            wake,
        );
        commands.entity(entity).insert((Busy, task));
    }
}

type Finished<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Phase,
        &'static AttachedRoute,
        &'static OperationState,
        &'static OperationTarget,
        &'static Axe,
        &'static SimulatorDevice,
        &'static Points,
        &'static Pixels,
        &'static HidBroker,
        &'static SimulatorClipboard,
        &'static SimulatorKeyboard,
        &'static StreamServer,
    ),
    (With<Attempt>, Without<Busy>, Without<Failure>),
>;

fn finish(
    requests: Finished,
    views: Query<(&PageMetadata, &EntityTarget<Attempt>), With<PageReady>>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (
        entity,
        phase,
        route,
        state,
        target,
        axe,
        device,
        points,
        pixels,
        _hid,
        _clipboard,
        _keyboard,
        stream,
    ) in &requests
    {
        if *phase != Phase::Complete
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        let view = target.entity();
        let Ok((metadata, target)) = views.get(view) else {
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        };
        if target.entity() != entity
            || SimulatorRoute::try_from(metadata.url.as_str())
                .ok()
                .as_ref()
                != Some(&route.0)
        {
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        }
        commands.entity(entity).move_components::<(
            HidBroker,
            SimulatorClipboard,
            SimulatorKeyboard,
            StreamServer,
        )>(view);
        info!(
            "mirroring {} on loopback port {}",
            device.name,
            stream.port()
        );
        let mut view = commands.entity(view);
        view.remove::<EntityTarget<Attempt>>().insert((
            Axe::at(axe.path().to_path_buf()),
            device.clone(),
            DeviceTouchSession::default(),
        ));
        if let Some((width, height)) = points.0 {
            view.insert(DevicePoints(width, height));
        }
        if let Some((width, height)) = pixels.0 {
            view.insert(DevicePixels(width, height));
        }
        commands.trigger(OperationCompleted {
            entity,
            result: Ok(()),
        });
        if let Some(wake) = wake.as_deref() {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
    }
}

fn retire(
    operations: Query<
        (
            Entity,
            &OperationState,
            Option<&OperationTarget>,
            Option<&Failure>,
        ),
        (With<Attempt>, Without<Busy>),
    >,
    views: Query<&EntityTarget<Attempt>>,
    children: Query<(Entity, &ChildOf), With<Reported>>,
    mut commands: Commands,
) {
    for (entity, state, target, failure) in &operations {
        if failure.is_none() && matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if let Some(target) = target
            && views
                .get(target.entity())
                .is_ok_and(|target| target.entity() == entity)
        {
            let mut view = commands.entity(target.entity());
            view.remove::<EntityTarget<Attempt>>();
            if let Some(failure) = failure
                && *state != OperationState::Canceled
            {
                view.insert(AttachmentFailed(failure.0.clone()));
            }
        }
        if let Some(failure) = failure {
            error!("could not attach an iOS Simulator: {}", failure.0);
            commands.trigger(OperationCompleted {
                entity,
                result: Err(failure.0.clone()),
            });
        }
        commands.entity(entity).remove::<(
            Attempt,
            Axe,
            SimulatorDevice,
            Candidates,
            Phase,
            AttachedRoute,
            Failure,
        )>();
        commands.entity(entity).remove::<(
            Probe,
            Booted,
            Available,
            Boot,
            BootReady,
            Points,
            Pixels,
            HidBroker,
            SimulatorClipboard,
            SimulatorKeyboard,
            StreamServer,
        )>();
        commands.entity(entity).remove::<Selecting>();
        for (child, parent) in &children {
            if parent.parent() == entity {
                commands.entity(child).despawn();
            }
        }
        commands.entity(entity).remove::<OperationRetained>();
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct AttachmentSet;

#[derive(Component)]
#[require(Phase)]
pub(super) struct Attempt;

#[derive(Component, Clone, PartialEq, Eq)]
pub(super) struct AttachedRoute(pub(super) SimulatorRoute);

#[derive(Component)]
pub(super) struct AttachmentFailed(pub(super) String);

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    #[default]
    Locating,
    Booted,
    Available,
    Booting,
    BootReady,
    Points,
    Pixels,
    Hid,
    Clipboard,
    Keyboard,
    Stream,
    Complete,
}

impl Phase {
    pub(super) fn status(&self) -> &'static str {
        match self {
            Self::Locating => "locating",
            Self::Booted | Self::Available | Self::Booting | Self::BootReady => "booting",
            Self::Points | Self::Pixels | Self::Hid | Self::Clipboard | Self::Keyboard => {
                "connecting"
            }
            Self::Stream | Self::Complete => "streaming",
        }
    }
}

#[derive(Resource)]
struct NativePaths {
    executable: Option<PathBuf>,
    xcrun: PathBuf,
    axe: Option<PathBuf>,
}

fn initialize(paths: Option<Res<NativePaths>>, mut commands: Commands) {
    if paths.is_some() {
        return;
    }
    commands.insert_resource(NativePaths {
        executable: std::env::current_exe().ok(),
        xcrun: PathBuf::from("xcrun"),
        axe: None,
    });
}

#[derive(Component)]
struct Candidates(VecDeque<PathBuf>);

#[derive(Component)]
struct Busy;

#[derive(Component)]
struct Failure(String);

#[derive(Component)]
struct NativeTask<T: Component>(Task<Result<T, String>>);

impl<T: Component> NativeTask<T> {
    fn spawn(
        work: impl Future<Output = Result<T, String>> + Send + 'static,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
    ) -> Self {
        Self(IoTaskPool::get().spawn(async move {
            let output = work.await;
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            output
        }))
    }
}

#[derive(Component)]
struct Probe {
    path: PathBuf,
    output: Result<Output, String>,
}

#[derive(Component)]
struct Booted(Result<Output, String>);

#[derive(Component)]
struct Available(Result<Output, String>);

#[derive(Component)]
struct Boot(Result<Output, String>);

#[derive(Component)]
struct BootReady(Result<Output, String>);

#[derive(Component)]
struct Points(Option<(f32, f32)>);

#[derive(Component)]
struct Pixels(Option<(u32, u32)>);

type Views<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PageMetadata,
        Option<&'static AttachedRoute>,
        Option<&'static SimulatorDevice>,
        Option<&'static EntityTarget<Attempt>>,
        Has<AttachmentFailed>,
    ),
    With<PageReady>,
>;

fn admit(views: Views, paths: Res<NativePaths>, mut commands: Commands) {
    for (view, metadata, attached, device, pending, failed) in &views {
        let Ok(route) = SimulatorRoute::try_from(metadata.url.as_str()) else {
            continue;
        };
        let matches = attached.is_some_and(|current| current.0 == route)
            || device.is_some_and(|device| device.matches_route(&route));
        if matches && (pending.is_some() || failed || device.is_some()) {
            continue;
        }
        if let Some(pending) = pending {
            commands
                .entity(pending.entity())
                .insert(OperationState::Canceled);
            continue;
        }
        commands.entity(view).remove::<(
            EntityTarget<Attempt>,
            AttachmentFailed,
            Axe,
            HidBroker,
            SimulatorClipboard,
            SimulatorKeyboard,
            SimulatorDevice,
            DevicePoints,
            DevicePixels,
            StreamServer,
            UiState<SimulatorUiState>,
            DeviceTouchSession,
        )>();
        let mut candidates = VecDeque::new();
        if let Some(path) = &paths.axe {
            candidates.push_back(path.clone());
        } else {
            candidates.push_back(PathBuf::from(Axe::BIN));
            candidates.push_back(PathBuf::from("/opt/homebrew/bin/axe"));
            candidates.push_back(PathBuf::from("/usr/local/bin/axe"));
            if let Some(contents) = paths
                .executable
                .as_ref()
                .and_then(|path| path.parent())
                .and_then(|path| path.parent())
            {
                candidates.push_back(contents.join("Resources/axe/axe"));
            }
        }
        let operation = commands
            .spawn((
                Operation::bundle("simulator.attach").unwrap().running(),
                OperationTarget::new(view),
                OperationRetained,
                Attempt,
                AttachedRoute(route.clone()),
                Candidates(candidates),
            ))
            .id();
        commands.entity(view).insert((
            EntityTarget::<Attempt>::new(operation),
            AttachedRoute(route),
        ));
    }
}

fn cancel(
    operations: Query<
        (
            Entity,
            Option<&OperationTarget>,
            &AttachedRoute,
            &OperationState,
        ),
        With<Attempt>,
    >,
    views: Query<(&PageMetadata, &EntityTarget<Attempt>), With<PageReady>>,
    mut commands: Commands,
) {
    for (entity, target, route, state) in &operations {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let current = target.and_then(|target| views.get(target.entity()).ok());
        if current.is_none_or(|(metadata, target)| {
            target.entity() != entity
                || SimulatorRoute::try_from(metadata.url.as_str())
                    .ok()
                    .as_ref()
                    != Some(&route.0)
        }) {
            commands.entity(entity).insert(OperationState::Canceled);
        }
    }
}

fn poll<T: Component>(
    mut tasks: Query<(Entity, &mut NativeTask<T>, &OperationState), With<Attempt>>,
    mut commands: Commands,
) {
    for (entity, mut task, state) in &mut tasks {
        let Some(output) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<(NativeTask<T>, Busy)>();
        if matches!(state, OperationState::Pending | OperationState::Running) {
            match output {
                Ok(output) => {
                    commands.entity(entity).insert(output);
                }
                Err(error) => {
                    commands.entity(entity).insert(Failure(error));
                }
            }
        }
    }
}

type LocateRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Phase,
        &'static OperationState,
        &'static mut Candidates,
        Option<&'static Probe>,
    ),
    (With<Attempt>, Without<Busy>, Without<Failure>),
>;

fn probe(
    mut requests: LocateRequests,
    wake: Option<Res<EventLoopProxyWrapper>>,
    paths: Res<NativePaths>,
    mut commands: Commands,
) {
    for (entity, phase, state, mut candidates, result) in &mut requests {
        if *phase != Phase::Locating
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        if let Some(result) = result {
            commands.entity(entity).remove::<Probe>();
            if let Ok(output) = &result.output
                && output.status.success()
            {
                info!(
                    "axe {} at {}",
                    String::from_utf8_lossy(&output.stdout).trim(),
                    result.path.display()
                );
                let mut command = Command::new(&paths.xcrun);
                command.args(["simctl", "list", "devices", "booted", "-j"]);
                let wake = wake.as_deref().map(|proxy| (**proxy).clone());
                let task = NativeTask::spawn(
                    async move { Ok(Booted(command.output().map_err(|error| error.to_string()))) },
                    wake,
                );
                commands.entity(entity).insert((
                    Axe::at(result.path.clone()),
                    Phase::Booted,
                    Busy,
                    task,
                ));
                continue;
            }
        }
        let Some(path) = candidates.0.pop_front() else {
            commands.entity(entity).insert(Failure(format!(
                "`{}` not found; install it with `brew install cameroncooke/axe/axe`",
                Axe::BIN
            )));
            continue;
        };
        let mut command = Command::new(&path);
        command.arg("--version");
        let wake = wake.as_deref().map(|proxy| (**proxy).clone());
        let task = NativeTask::spawn(
            async move {
                Ok(Probe {
                    path,
                    output: command.output().map_err(|error| error.to_string()),
                })
            },
            wake,
        );
        commands.entity(entity).insert((Busy, task));
    }
}

type SelectionRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Phase,
        &'static OperationState,
        &'static AttachedRoute,
        Option<&'static Booted>,
        Option<&'static Available>,
    ),
    (With<Attempt>, Without<Busy>, Without<Failure>),
>;

#[derive(Component)]
struct Selecting;

#[derive(Component)]
struct Reported(bool);

fn materialize(requests: SelectionRequests, mut commands: Commands) {
    for (entity, phase, state, _, booted, available) in &requests {
        if !matches!(phase, Phase::Booted | Phase::Available)
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        let result = match phase {
            Phase::Booted => {
                let Some(result) = booted else {
                    continue;
                };
                &result.0
            }
            Phase::Available => {
                let Some(result) = available else {
                    continue;
                };
                &result.0
            }
            _ => unreachable!(),
        };
        commands.entity(entity).remove::<(Booted, Available)>();
        let output = match result {
            Ok(output) if output.status.success() => Some(output),
            Ok(output) if *phase == Phase::Available => {
                commands.entity(entity).insert(Failure(format!(
                    "simctl could not list available devices: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
                continue;
            }
            Err(error) if *phase == Phase::Available => {
                commands
                    .entity(entity)
                    .insert(Failure(format!("could not run simctl: {error}")));
                continue;
            }
            _ => None,
        };
        let parsed = output
            .and_then(|output| serde_json::from_slice::<serde_json::Value>(&output.stdout).ok());
        let runtimes = parsed
            .as_ref()
            .and_then(|parsed| parsed.get("devices"))
            .and_then(|devices| devices.as_object());
        let mut order = 0;
        if let Some(runtimes) = runtimes {
            for (runtime, entries) in runtimes {
                let Some(version) = crate::url::IosVersion::from_runtime_key(runtime) else {
                    continue;
                };
                let Some(entries) = entries.as_array() else {
                    continue;
                };
                for entry in entries {
                    let udid = entry.get("udid").and_then(|value| value.as_str());
                    let name = entry.get("name").and_then(|value| value.as_str());
                    let (Some(udid), Some(name)) = (udid, name) else {
                        continue;
                    };
                    let available =
                        entry.get("isAvailable").and_then(|value| value.as_bool()) != Some(false);
                    commands.spawn((
                        SimulatorDevice {
                            udid: udid.to_string(),
                            name: name.to_string(),
                            version: Some(version.clone()),
                        },
                        Reported(available),
                        Order(order),
                        ChildOf(entity),
                    ));
                    order += 1;
                }
            }
        }
        commands.entity(entity).insert(Selecting);
    }
}

type SelectingRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Phase,
        &'static OperationState,
        &'static AttachedRoute,
    ),
    (
        With<Attempt>,
        With<Selecting>,
        Without<Busy>,
        Without<Failure>,
    ),
>;

fn select(
    requests: SelectingRequests,
    candidates: Query<(Entity, &SimulatorDevice, &Reported, &Order, &ChildOf)>,
    paths: Res<NativePaths>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, phase, state, route) in &requests {
        if !matches!(phase, Phase::Booted | Phase::Available)
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        let mut selected: Option<(&SimulatorDevice, u32)> = None;
        for (candidate, device, reported, order, parent) in &candidates {
            if parent.parent() != entity {
                continue;
            }
            commands.entity(candidate).despawn();
            if !reported.0 || !device.matches_route(&route.0) {
                continue;
            }
            let replace = match selected {
                Some((current, previous)) => {
                    device.version > current.version
                        || (device.version == current.version && order.0 < previous)
                }
                None => true,
            };
            if replace {
                selected = Some((device, order.0));
            }
        }
        commands.entity(entity).remove::<Selecting>();
        let Some((device, _)) = selected else {
            if *phase == Phase::Booted {
                let mut command = Command::new(&paths.xcrun);
                command.args(["simctl", "list", "devices", "available", "-j"]);
                let wake = wake.as_deref().map(|proxy| (**proxy).clone());
                let task = NativeTask::spawn(
                    async move {
                        Ok(Available(
                            command.output().map_err(|error| error.to_string()),
                        ))
                    },
                    wake,
                );
                commands
                    .entity(entity)
                    .insert((Phase::Available, Busy, task));
            } else {
                let runtime = route
                    .0
                    .version()
                    .map(|version| format!(" for iOS {version}"))
                    .unwrap_or_default();
                let model = route
                    .0
                    .device_name()
                    .map(|name| format!(" named {name}"))
                    .unwrap_or_default();
                commands.entity(entity).insert(Failure(format!(
                    "no available iOS Simulator{model}{runtime}"
                )));
            }
            continue;
        };
        if *phase == Phase::Booted {
            commands
                .entity(entity)
                .insert((device.clone(), Phase::Points));
            continue;
        }
        info!("booting {} ({})", device.name, device.udid);
        let mut command = Command::new(&paths.xcrun);
        command.args(["simctl", "boot", &device.udid]);
        let wake = wake.as_deref().map(|proxy| (**proxy).clone());
        let task = NativeTask::spawn(
            async move { Ok(Boot(command.output().map_err(|error| error.to_string()))) },
            wake,
        );
        commands
            .entity(entity)
            .insert((device.clone(), Phase::Booting, Busy, task));
    }
}

fn boot(
    requests: Query<
        (Entity, &Phase, &OperationState, &SimulatorDevice, &Boot),
        (With<Attempt>, Without<Busy>, Without<Failure>),
    >,
    paths: Res<NativePaths>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, phase, state, device, result) in &requests {
        if *phase != Phase::Booting
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        if let Err(error) = &result.0 {
            commands
                .entity(entity)
                .insert(Failure(format!("could not run simctl boot: {error}")));
            continue;
        }
        let mut command = Command::new(&paths.xcrun);
        command.args(["simctl", "bootstatus", &device.udid, "-b"]);
        let wake = wake.as_deref().map(|proxy| (**proxy).clone());
        let task = NativeTask::spawn(
            async move {
                Ok(BootReady(
                    command.output().map_err(|error| error.to_string()),
                ))
            },
            wake,
        );
        commands
            .entity(entity)
            .insert((Phase::BootReady, Busy, task));
    }
}

fn wait_boot(
    requests: Query<
        (
            Entity,
            &Phase,
            &OperationState,
            &SimulatorDevice,
            &Boot,
            &BootReady,
        ),
        (With<Attempt>, Without<Busy>, Without<Failure>),
    >,
    mut commands: Commands,
) {
    for (entity, phase, state, device, boot, result) in &requests {
        if *phase != Phase::BootReady
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        match &result.0 {
            Ok(output) if output.status.success() => {
                info!("booted {} ({})", device.name, device.udid);
                commands
                    .entity(entity)
                    .remove::<(Boot, BootReady)>()
                    .insert(Phase::Points);
            }
            Ok(output) => {
                let boot_error = boot
                    .0
                    .as_ref()
                    .map(|output| String::from_utf8_lossy(&output.stderr).trim().to_string())
                    .unwrap_or_default();
                commands.entity(entity).insert(Failure(format!(
                    "could not boot {}: {} {}",
                    device.name,
                    boot_error,
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Failure(format!("could not run simctl bootstatus: {error}")));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::TaskPoolBuilder;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    struct Fixture {
        app: App,
        directory: PathBuf,
    }

    impl Fixture {
        fn new(booted: &str, available: &str, block_boot: bool, boot_exit: u8) -> Self {
            IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(4).build());
            let directory = std::env::temp_dir().join(format!(
                "vmx253-simulator-attachment-{}",
                uuid::Uuid::new_v4()
            ));
            fs::create_dir(&directory).unwrap();
            fs::write(directory.join("actions"), "").unwrap();
            fs::write(directory.join("booted.json"), booted).unwrap();
            fs::write(directory.join("available.json"), available).unwrap();
            fs::write(directory.join("boot-exit"), boot_exit.to_string()).unwrap();
            if block_boot {
                fs::write(directory.join("block-boot"), "").unwrap();
            }
            let axe = directory.join("axe");
            fs::write(
                &axe,
                r#"#!/bin/sh
fixture_dir=$(dirname "$0")
printf '%s\n' "$*" >> "$fixture_dir/actions"
if [ "$1" = "--version" ]; then
    printf 'fixture axe\n'
    exit 0
fi
while [ -d "$fixture_dir" ] && [ ! -f "$fixture_dir/release-axe" ]; do
    sleep 0.01
done
exit 1
"#,
            )
            .unwrap();
            fs::set_permissions(&axe, fs::Permissions::from_mode(0o700)).unwrap();
            let xcrun = directory.join("xcrun");
            fs::write(&xcrun, r#"#!/bin/sh
fixture_dir=$(dirname "$0")
printf '%s\n' "$*" >> "$fixture_dir/actions"
if [ "$2" = "list" ]; then
    cat "$fixture_dir/$4.json"
    exit 0
fi
if [ "$2" = "boot" ]; then
    while [ -d "$fixture_dir" ] && [ -f "$fixture_dir/block-boot" ] && [ ! -f "$fixture_dir/release-boot" ]; do
        sleep 0.01
    done
    if [ -f "$fixture_dir/boot-exit" ]; then
        exit "$(cat "$fixture_dir/boot-exit")"
    fi
    exit 1
fi
if [ "$2" = "bootstatus" ]; then
    exit 0
fi
exit 1
"#).unwrap();
            fs::set_permissions(&xcrun, fs::Permissions::from_mode(0o700)).unwrap();
            let mut app = App::new();
            app.insert_resource(NativePaths {
                executable: None,
                xcrun,
                axe: Some(axe),
            })
            .add_plugins(AttachmentPlugin);
            Self { app, directory }
        }

        fn page(&mut self, url: &str) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    PageReady,
                    PageMetadata {
                        url: url.to_owned(),
                        ..Default::default()
                    },
                ))
                .id()
        }

        fn wait(&mut self, mut ready: impl FnMut(&World) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if ready(self.app.world()) {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "attachment did not reach the expected observable state"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        fn actions(&self) -> Vec<String> {
            fs::read_to_string(self.directory.join("actions"))
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        fn wait_action(&mut self, action: &str) {
            let path = self.directory.join("actions");
            self.wait(|_| {
                fs::read_to_string(&path)
                    .unwrap()
                    .lines()
                    .any(|line| line == action)
            });
        }

        fn operation(&self, page: Entity) -> Entity {
            self.app
                .world()
                .get::<EntityTarget<Attempt>>(page)
                .unwrap()
                .entity()
        }

        fn selected(&mut self, page: Entity) -> SimulatorDevice {
            self.wait(|world| {
                world
                    .get::<EntityTarget<Attempt>>(page)
                    .is_some_and(|target| world.get::<SimulatorDevice>(target.entity()).is_some())
            });
            self.app
                .world()
                .get::<SimulatorDevice>(self.operation(page))
                .unwrap()
                .clone()
        }

        fn concurrent_pixels(&self) -> [Option<(u32, u32)>; 2] {
            for (name, width, height) in [("first", 7u32, 13u32), ("second", 19u32, 23u32)] {
                let mut png = vec![0; 24];
                png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
                png[12..16].copy_from_slice(b"IHDR");
                png[16..20].copy_from_slice(&width.to_be_bytes());
                png[20..24].copy_from_slice(&height.to_be_bytes());
                fs::write(self.directory.join(format!("{name}.png")), png).unwrap();
            }
            let path = self.directory.join("pixel-axe");
            fs::write(
                &path,
                r#"#!/bin/sh
fixture_dir=$(dirname "$0")
output=$5
printf '%s\n' "$output" >> "$fixture_dir/pixel-outputs"
if mkdir "$fixture_dir/first" 2>/dev/null; then
    printf '%s\n' "$output" > "$fixture_dir/first-output"
    cp "$fixture_dir/first.png" "$output"
    while [ ! -f "$fixture_dir/second-ready" ]; do sleep 0.01; done
else
    while [ ! -f "$fixture_dir/first-output" ]; do sleep 0.01; done
    cp "$fixture_dir/second.png" "$output"
    touch "$fixture_dir/second-ready"
    first_output=$(cat "$fixture_dir/first-output")
    while [ -f "$first_output" ]; do sleep 0.01; done
fi
"#,
            )
            .unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            let axe = Axe::at(path);
            let device = SimulatorDevice {
                udid: "SHARED".into(),
                name: "Phone".into(),
                version: None,
            };
            std::thread::scope(|threads| {
                let first = threads.spawn(|| device.pixel_size(&axe));
                let second = threads.spawn(|| device.pixel_size(&axe));
                [first.join().unwrap(), second.join().unwrap()]
            })
        }

        fn release_boot(&self) {
            fs::write(self.directory.join("release-boot"), "").unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let world = self.app.world_mut();
            let operations: Vec<Entity> = world
                .query_filtered::<Entity, With<Attempt>>()
                .iter(world)
                .collect();
            let pages: Vec<Entity> = world
                .query_filtered::<Entity, With<PageReady>>()
                .iter(world)
                .collect();
            for page in pages {
                world.despawn(page);
            }
            for entity in &operations {
                world.entity_mut(*entity).insert(OperationState::Canceled);
            }
            let _ = fs::write(self.directory.join("release-boot"), "");
            let _ = fs::write(self.directory.join("release-axe"), "");
            self.wait(|world| {
                operations
                    .iter()
                    .all(|entity| world.get::<Attempt>(*entity).is_none())
            });
            fs::remove_dir_all(&self.directory).unwrap();
        }
    }

    #[test]
    fn canonical_device_selection_uses_the_newest_available_runtime_and_stable_order() {
        let booted = r#"{"devices":{
            "com.apple.CoreSimulator.SimRuntime.watchOS-27-0":[{"udid":"WATCH","name":"Watch"}],
            "com.apple.CoreSimulator.SimRuntime.iOS-9-0":[{"udid":"OLD","name":"Phone"}],
            "com.apple.CoreSimulator.SimRuntime.iOS-10-0":[
                {"udid":"UNAVAILABLE","name":"Phone","isAvailable":false},
                {"udid":"FIRST","name":"Phone"},
                {"udid":"SECOND","name":"Phone"}
            ]
        }}"#;
        let mut fixture = Fixture::new(booted, r#"{"devices":{}}"#, false, 0);
        let page = fixture.page("vmux://simulator");
        let device = fixture.selected(page);
        assert_eq!(device.udid, "FIRST");
        assert_eq!(device.version.unwrap().as_str(), "10.0");
        fixture.wait_action("describe-ui --udid FIRST");
        assert_eq!(
            fixture
                .actions()
                .iter()
                .filter(|action| action.starts_with("simctl "))
                .cloned()
                .collect::<Vec<_>>(),
            ["simctl list devices booted -j"]
        );
    }

    #[test]
    fn pinned_routes_select_independent_devices_for_each_page() {
        let booted = r#"{"devices":{
            "com.apple.CoreSimulator.SimRuntime.iOS-9-0":[{"udid":"OLD","name":"Phone"}],
            "com.apple.CoreSimulator.SimRuntime.iOS-10-0":[
                {"udid":"REGULAR","name":"Phone"},
                {"udid":"PRO","name":"Phone Pro"}
            ]
        }}"#;
        let mut fixture = Fixture::new(booted, r#"{"devices":{}}"#, false, 0);
        let older = fixture.page("vmux://simulator/ios/9.0");
        let pro = fixture.page("vmux://simulator/ios/10.0/Phone%20Pro");
        assert_eq!(fixture.selected(older).udid, "OLD");
        assert_eq!(fixture.selected(pro).udid, "PRO");
        assert_ne!(fixture.operation(older), fixture.operation(pro));
    }

    #[test]
    fn readiness_is_checked_after_a_boot_command_reports_failure() {
        let available = r#"{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-10-0":[{"udid":"BOOT","name":"Phone"}]}}"#;
        let mut fixture = Fixture::new(r#"{"devices":{}}"#, available, false, 1);
        let page = fixture.page("vmux://simulator/ios/10.0");
        fixture.wait_action("describe-ui --udid BOOT");
        assert_eq!(fixture.selected(page).udid, "BOOT");
        assert_eq!(
            fixture
                .actions()
                .iter()
                .filter(|action| action.starts_with("simctl "))
                .cloned()
                .collect::<Vec<_>>(),
            [
                "simctl list devices booted -j",
                "simctl list devices available -j",
                "simctl boot BOOT",
                "simctl bootstatus BOOT -b"
            ]
        );
    }

    #[test]
    fn replacing_a_route_retains_accepted_io_until_drain_and_rejects_its_result() {
        let available = r#"{"devices":{
            "com.apple.CoreSimulator.SimRuntime.iOS-9-0":[{"udid":"OLD","name":"Phone"}],
            "com.apple.CoreSimulator.SimRuntime.iOS-10-0":[{"udid":"NEW","name":"Phone"}]
        }}"#;
        let mut fixture = Fixture::new(r#"{"devices":{}}"#, available, true, 0);
        let page = fixture.page("vmux://simulator/ios/9.0");
        fixture.wait_action("simctl boot OLD");
        let old = fixture.operation(page);
        fixture
            .app
            .world_mut()
            .get_mut::<PageMetadata>(page)
            .unwrap()
            .url = "vmux://simulator/ios/10.0".to_owned();
        for _ in 0..3 {
            fixture.app.update();
        }
        assert_eq!(fixture.operation(page), old);
        assert_eq!(
            fixture.app.world().get::<OperationState>(old),
            Some(&OperationState::Canceled)
        );
        assert!(fixture.app.world().get::<OperationRetained>(old).is_some());
        assert!(fixture.app.world().get::<Busy>(old).is_some());
        assert!(
            !fixture
                .actions()
                .iter()
                .any(|action| action == "simctl bootstatus OLD -b")
        );
        fixture.release_boot();
        fixture.wait(|world| {
            world
                .get::<EntityTarget<Attempt>>(page)
                .is_some_and(|target| target.entity() != old)
        });
        assert!(fixture.app.world().get::<Attempt>(old).is_none());
        assert!(fixture.app.world().get::<OperationRetained>(old).is_none());
        assert_eq!(fixture.selected(page).udid, "NEW");
        fixture.wait_action("describe-ui --udid NEW");
        assert!(
            !fixture
                .actions()
                .iter()
                .any(|action| action == "simctl bootstatus OLD -b")
        );
    }
    #[test]
    fn concurrent_pixel_reads_preserve_each_snapshot_and_release_owned_files() {
        let fixture = Fixture::new(r#"{"devices":{}}"#, r#"{"devices":{}}"#, false, 0);
        let mut results = fixture.concurrent_pixels();
        results.sort();
        assert_eq!(results, [Some((7, 13)), Some((19, 23))]);
        let outputs = fs::read_to_string(fixture.directory.join("pixel-outputs")).unwrap();
        for path in outputs.lines() {
            assert!(
                !PathBuf::from(path).exists(),
                "snapshot file was retained after IO completed"
            );
        }
    }
}
