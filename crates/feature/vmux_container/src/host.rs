use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::page::{HostedPage, PageReady};
use vmux_ecs::wake::Wake;
use vmux_ecs::{PageMetadata, UiStatePlugin, UiStateWrite};
use vmux_vm::{EngineClient, Phase, Settings, Status};

use crate::ContainerPlugin;
use crate::compose::ComposeProject;
use crate::control::{Busy, Command, ControlPlugin, ControlSet, Finished, Reply, Request};
use crate::event::{
    ContainerLogsRequest, ContainerProjectOpenRequest, ContainerSettingsRequest,
    ContainerStartRequest, ContainerStopRequest, ContainerUiState,
};
use crate::inventory::{Catalog, Inventory, InventoryPlugin, Output};

impl Plugin for ContainerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_plugins(
                Self::MANIFEST
                    .plugin()
                    .hosted(HostedPage::page(Self::URL, "Containers")),
            )
            .add_plugins(UiStatePlugin::<ContainerUiState>::default())
            .add_plugins(UiEventPlugin::<(
                ContainerStartRequest,
                ContainerStopRequest,
                ContainerLogsRequest,
                ContainerSettingsRequest,
                ContainerProjectOpenRequest,
            )>::default());
        if !app.is_plugin_added::<ControlPlugin>() {
            app.add_plugins(ControlPlugin);
        }
        app.add_plugins(InventoryPlugin).configure_sets(
            Update,
            (
                ControlSet::Complete,
                ContainerSet::Inspect,
                ContainerSet::Inventory,
                ContainerSet::Project,
            )
                .chain(),
        );
        #[cfg(ui)]
        app.add_plugins(crate::ui::ContainerPage::plugin());
        app.add_observer(page_ready)
            .add_observer(start)
            .add_observer(configure)
            .add_observer(open_project)
            .add_observer(stop)
            .add_observer(logs)
            .add_observer(finish)
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                (finish_picks, inspect)
                    .chain()
                    .in_set(ContainerSet::Inspect),
            )
            .add_systems(Update, project.in_set(ContainerSet::Project));
    }
}

#[derive(Component)]
pub(crate) struct EngineView;

#[derive(Component)]
pub(crate) struct PageView;

#[derive(Component)]
pub(crate) struct Observed(pub(crate) Status);

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ContainerSet {
    Inspect,
    Inventory,
    Project,
}

#[derive(Component)]
struct PickTask(Task<std::io::Result<Option<ComposeProject>>>);

#[derive(Component)]
struct NextInspection(Instant);

#[derive(Component)]
struct InspectTask {
    settings: Settings,
    task: Task<Status>,
}

#[derive(Component, Default)]
struct Logs(String);

#[derive(Component, Default)]
struct LastError(String);

#[derive(Component, Default)]
struct ConfigurationRevision(u64);

fn spawn(mut commands: Commands) {
    let directory = vmux_profile::ProfilePaths::current()
        .shared_data()
        .join("containers");
    let settings = match Settings::load(&directory.join("settings.json")) {
        Ok(settings) => settings,
        Err(_) => Settings::new(directory, DefaultShare::path()),
    };
    let status = Status {
        running: false,
        docker_ready: false,
        socket: settings.socket(),
        log: settings.log(),
        phase: Phase::Stopped,
        failure: None,
    };
    commands.spawn((
        Name::new("Container engine view"),
        EngineView,
        settings,
        Observed(status),
        NextInspection(Instant::now()),
        Logs::default(),
        LastError::default(),
        ConfigurationRevision::default(),
    ));
}

struct DefaultShare;

impl DefaultShare {
    fn path() -> PathBuf {
        match std::env::var_os("HOME") {
            Some(path) => PathBuf::from(path),
            None => vmux_profile::ProfilePaths::current().projects(),
        }
    }
}

fn page_ready(
    trigger: On<UiInput<PageReady>>,
    pages: Query<&PageMetadata>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(page) = pages.get(entity) else {
        return;
    };
    if !page.url.starts_with(ContainerPlugin::URL) {
        return;
    }
    commands.entity(entity).insert(PageView);
}

type ConfiguredEngines<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static Observed,
        Option<&'static Busy>,
    ),
    With<EngineView>,
>;

fn start(
    trigger: On<UiInput<ContainerStartRequest>>,
    pages: Query<(), With<PageView>>,
    engines: ConfiguredEngines,
    mut commands: Commands,
) {
    let event = trigger.event();
    if !pages.contains(event.webview) {
        return;
    }
    let Ok((entity, settings, _, busy)) = engines.single() else {
        return;
    };
    if busy.is_some() {
        return;
    }
    commands.trigger(Request {
        entity,
        reply: entity,
        command: Command::Start,
        settings: settings.clone(),
    });
}

fn configure(
    trigger: On<UiInput<ContainerSettingsRequest>>,
    pages: Query<(), With<PageView>>,
    engines: ConfiguredEngines,
    mut commands: Commands,
) {
    let event = trigger.event();
    if !pages.contains(event.webview) {
        return;
    }
    let Ok((entity, settings, observed, busy)) = engines.single() else {
        return;
    };
    if busy.is_some() || observed.0.running {
        return;
    }
    let mut requested = settings.clone();
    requested.shared_directory = PathBuf::from(&event.payload.shared_directory);
    requested.cpus = event.payload.cpus;
    requested.memory_mib = event.payload.memory_mib;
    commands.trigger(Request {
        entity,
        reply: entity,
        command: Command::Configure,
        settings: requested,
    });
}

type PickingEngines<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        Option<&'static Busy>,
        Option<&'static PickTask>,
    ),
    With<EngineView>,
>;

fn open_project(
    trigger: On<UiInput<ContainerProjectOpenRequest>>,
    pages: Query<(), With<PageView>>,
    engines: PickingEngines,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    if !pages.contains(trigger.event().webview) {
        return;
    }
    let Ok((entity, settings, busy, pick)) = engines.single() else {
        return;
    };
    if busy.is_some() || pick.is_some() {
        return;
    }
    let directory = settings.shared_directory.clone();
    let wake = Wake::beside(proxy.as_deref());
    commands
        .entity(entity)
        .insert(PickTask(IoTaskPool::get().spawn(async move {
            let result = ComposeProject::select(directory).await;
            drop(wake);
            result
        })));
}

fn finish_picks(
    mut engines: Query<(Entity, &Settings, &mut LastError, &mut PickTask), With<EngineView>>,
    mut commands: Commands,
) {
    for (entity, settings, mut error, mut task) in &mut engines {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<PickTask>();
        match result {
            Ok(Some(project)) => {
                commands.trigger(Request {
                    entity,
                    reply: entity,
                    command: Command::Project(project.directory()),
                    settings: settings.clone(),
                });
            }
            Ok(None) => {}
            Err(value) => {
                error.0 = value.to_string();
            }
        }
    }
}

fn stop(
    trigger: On<UiInput<ContainerStopRequest>>,
    pages: Query<(), With<PageView>>,
    engines: Query<(Entity, &Settings), With<EngineView>>,
    mut commands: Commands,
) {
    let page = trigger.event().webview;
    if !pages.contains(page) {
        return;
    }
    let Ok((entity, settings)) = engines.single() else {
        return;
    };
    commands.trigger(Request {
        entity,
        reply: entity,
        command: Command::Stop,
        settings: settings.clone(),
    });
}

fn logs(
    trigger: On<UiInput<ContainerLogsRequest>>,
    pages: Query<(), With<PageView>>,
    engines: Query<(Entity, &Settings), With<EngineView>>,
    mut commands: Commands,
) {
    let page = trigger.event().webview;
    if !pages.contains(page) {
        return;
    }
    let Ok((entity, settings)) = engines.single() else {
        return;
    };
    commands.trigger(Request {
        entity,
        reply: entity,
        command: Command::Logs,
        settings: settings.clone(),
    });
}

type CompletedEngines<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Observed,
        &'static mut Logs,
        &'static mut LastError,
        &'static mut Settings,
        &'static mut ConfigurationRevision,
    ),
    With<EngineView>,
>;

fn finish(trigger: On<Finished>, mut engines: CompletedEngines, mut commands: Commands) {
    let event = trigger.event();
    let Ok((mut observed, mut logs, mut error, mut settings, mut revision)) =
        engines.get_mut(event.entity)
    else {
        return;
    };
    match &event.result {
        Ok(Reply::Started(status) | Reply::Stopped(status) | Reply::Status(status)) => {
            observed.0 = status.clone();
            error.0.clear();
        }
        Ok(Reply::Logs(value)) => logs.0 = value.clone(),
        Ok(Reply::Configured(value)) => {
            *settings = value.clone();
            revision.0 += 1;
            observed.0.socket = settings.socket();
            observed.0.log = settings.log();
            error.0.clear();
        }
        Ok(Reply::Changed) => error.0.clear(),
        Err(value) => error.0 = value.clone(),
    }
    commands
        .entity(event.entity)
        .remove::<InspectTask>()
        .insert(NextInspection(Instant::now()));
}

type InspectedEngines<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static mut Observed,
        &'static mut NextInspection,
        Option<&'static mut InspectTask>,
    ),
    With<EngineView>,
>;

fn inspect(
    mut engines: InspectedEngines,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, settings, mut observed, mut next, task) in &mut engines {
        if let Some(mut task) = task {
            let Some(status) = future::block_on(future::poll_once(&mut task.task)) else {
                continue;
            };
            if *settings == task.settings {
                observed.0 = status;
            }
            commands.entity(entity).remove::<InspectTask>();
            next.0 = Instant::now() + Duration::from_secs(1);
            continue;
        }
        if Instant::now() < next.0 {
            continue;
        }
        let settings = settings.clone();
        let requested = settings.clone();
        let wake = Wake::beside(proxy.as_deref());
        commands.entity(entity).insert(InspectTask {
            settings: requested,
            task: IoTaskPool::get().spawn(async move {
                let status = EngineClient::new(settings).status();
                drop(wake);
                status
            }),
        });
    }
}

type ProjectedEngines<'w, 's> = Query<
    'w,
    's,
    (
        &'static Settings,
        &'static Observed,
        &'static Logs,
        &'static LastError,
        Option<&'static Busy>,
        Entity,
        Option<&'static Inventory>,
        Option<&'static PickTask>,
        &'static ConfigurationRevision,
    ),
    With<EngineView>,
>;

fn project(
    pages: Query<(Entity, Option<&Output>), With<PageView>>,
    engines: ProjectedEngines,
    catalog: Catalog,
    mut commands: Commands,
) {
    let Ok((settings, observed, logs, error, busy, engine, inventory, picker, revision)) =
        engines.single()
    else {
        return;
    };
    let phase = match observed.0.phase {
        Phase::Created => "created",
        Phase::NetworkStarting => "network",
        Phase::DockerStarting => "docker",
        Phase::Ready => "ready",
        Phase::Stopping => "stopping",
        Phase::Stopped => "stopped",
        Phase::Failed => "failed",
    };
    let snapshot = ContainerUiState {
        supported: cfg!(all(target_os = "macos", target_arch = "aarch64")),
        running: observed.0.running,
        ready: observed.0.docker_ready,
        busy: busy.is_some() || picker.is_some(),
        phase: phase.into(),
        data_directory: settings.directory.to_string_lossy().into_owned(),
        shared_directory: settings.shared_directory.to_string_lossy().into_owned(),
        cpus: settings.cpus,
        memory_mib: settings.memory_mib,
        socket: observed.0.socket.to_string_lossy().into_owned(),
        error: if error.0.is_empty() {
            observed.0.failure.clone().unwrap_or_default()
        } else {
            error.0.clone()
        },
        logs: logs.0.clone(),
        inventory_ready: inventory.is_some_and(|inventory| inventory.ready),
        inventory_error: inventory
            .map(|inventory| inventory.error.clone())
            .unwrap_or_default(),
        containers: catalog.snapshot(engine),
        log_id: String::new(),
        container_logs: String::new(),
        container_log_error: String::new(),
        container_logs_ready: false,
        configuration_revision: revision.0,
        starting: busy.is_some_and(Busy::starting),
        stopping: busy.is_some_and(Busy::stopping),
        project_starting: busy.is_some_and(Busy::project),
    };
    for (page, output) in &pages {
        let mut snapshot = snapshot.clone();
        if let Some(output) = output {
            snapshot.log_id = output.id.clone();
            snapshot.container_logs = output.text.clone();
            snapshot.container_log_error = output.error.clone();
            snapshot.container_logs_ready = output.ready;
        }
        commands.trigger(UiStateWrite::<ContainerUiState>::from_event(
            page, &snapshot,
        ));
    }
}
