use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::page::{HostedPage, PageReady};
use vmux_ecs::wake::Wake;
use vmux_ecs::{KeyboardOwner, PageMetadata, Revision, UiStatePlugin, UiStateWrite};
use vmux_vm::{EngineClient, Phase, Settings, Status};

use crate::ContainerPlugin;
use crate::control::{Busy, Command, ControlPlugin, ControlSet, Finished, Reply, Request};
use crate::event::{
    ContainerLogsRequest, ContainerSettingsRequest, ContainerStartRequest, ContainerStopRequest,
    ContainerUiState,
};
use crate::inventory::{Catalog, Inventory, InventoryPlugin, Output};

impl Plugin for ContainerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_plugins((
                Self::MANIFEST
                    .plugin()
                    .hosted(HostedPage::page(Self::URL, "VM")),
                crate::Docker::MANIFEST
                    .plugin()
                    .hosted(HostedPage::page(crate::Docker::URL, "Docker")),
                crate::Containers::MANIFEST
                    .plugin()
                    .hosted(HostedPage::subtree(crate::Containers::URL, "Containers")),
            ))
            .add_plugins(UiStatePlugin::<ContainerUiState>::default())
            .add_plugins(UiEventPlugin::<(
                ContainerStartRequest,
                ContainerStopRequest,
                ContainerLogsRequest,
                ContainerSettingsRequest,
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
        app.add_plugins((
            crate::ui::VmPage::plugin(),
            crate::ui::DockerPage::plugin(),
            crate::ui::ContainerPage::plugin(),
        ));
        app.add_observer(page_ready)
            .add_observer(start)
            .add_observer(configure)
            .add_observer(stop)
            .add_observer(logs)
            .add_observer(finish)
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                (inspect, autostart).chain().in_set(ContainerSet::Inspect),
            )
            .add_systems(Update, project.in_set(ContainerSet::Project));
    }
}

#[derive(Component)]
pub(crate) struct EngineView;

#[derive(Component)]
pub(crate) struct PageView;

#[derive(Component)]
struct Inspected;

#[derive(Component)]
struct Opened;

#[derive(Component)]
pub(crate) struct Observed(pub(crate) Status);

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ContainerSet {
    Inspect,
    Inventory,
    Project,
}

#[derive(Component)]
struct NextInspection(Instant);

#[derive(Component)]
struct InspectTask {
    settings: Settings,
    revision: Revision,
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
        Err(_) => Settings::new(directory, EngineView::default_share()),
    };
    let status = Status {
        running: false,
        docker_ready: false,
        socket: settings.socket(),
        log: settings.log(),
        phase: Phase::Created,
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
        Revision::default(),
    ));
}

impl EngineView {
    fn default_share() -> PathBuf {
        match std::env::var_os("HOME") {
            Some(path) => PathBuf::from(path),
            None => vmux_profile::ProfilePaths::current().projects(),
        }
    }
}

fn page_ready(
    trigger: On<UiInput<PageReady>>,
    pages: Query<(&PageMetadata, Option<&Revision>)>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((page, revision)) = pages.get(entity) else {
        return;
    };
    if !vmux_api::RoutePattern::new(ContainerPlugin::URL, false).matches(&page.url)
        && !vmux_api::RoutePattern::new(crate::Docker::URL, false).matches(&page.url)
        && !vmux_api::RoutePattern::new(crate::Containers::URL, true).matches(&page.url)
    {
        return;
    }
    commands.entity(entity).insert((
        PageView,
        vmux_ecs::address::AddressScope::subtree(ContainerPlugin::URL),
    ));
    if revision.is_none() {
        commands
            .entity(entity)
            .insert((Revision::default(), LastError::default()));
    }
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
    let Ok((entity, settings, _, _)) = engines.single() else {
        return;
    };
    commands.trigger(Request {
        entity,
        reply: entity,
        command: Command::Start,
        settings: settings.clone(),
        requester: event.webview,
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
        requester: event.webview,
    });
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
        requester: page,
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
        requester: page,
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
        &'static mut Revision,
    ),
    With<EngineView>,
>;

type CompletedPages<'w, 's> = Query<
    'w,
    's,
    (&'static mut Revision, &'static mut LastError),
    (With<PageView>, Without<EngineView>),
>;

fn finish(
    trigger: On<Finished>,
    mut engines: CompletedEngines,
    mut pages: CompletedPages,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok((mut observed, mut logs, mut error, mut settings, mut revision, mut engine_revision)) =
        engines.get_mut(event.entity)
    else {
        return;
    };
    if event.mutates {
        *engine_revision = engine_revision.saturating_next();
    }
    if let Ok((mut completed, mut page_error)) = pages.get_mut(event.requester) {
        if event.mutates {
            completed.0 = completed.0.wrapping_add(1);
        }
        match &event.result {
            Ok(_) => page_error.0.clear(),
            Err(error) => page_error.0 = error.clone(),
        }
    }
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
        &'static Revision,
        Option<&'static mut InspectTask>,
    ),
    With<EngineView>,
>;

fn inspect(
    mut engines: InspectedEngines,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, settings, mut observed, mut next, revision, task) in &mut engines {
        if let Some(mut task) = task {
            let Some(status) = future::block_on(future::poll_once(&mut task.task)) else {
                continue;
            };
            if *settings == task.settings && *revision == task.revision {
                observed.0 = status;
                commands.entity(entity).insert(Inspected);
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
            revision: *revision,
            task: IoTaskPool::get().spawn(async move {
                let status = EngineClient::new(settings).status();
                drop(wake);
                status
            }),
        });
    }
}

type UnopenedEngines<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static Observed,
        Option<&'static Busy>,
    ),
    (With<EngineView>, With<Inspected>, Without<Opened>),
>;

fn autostart(
    pages: Query<Entity, (With<PageView>, With<KeyboardOwner>)>,
    engines: UnopenedEngines,
    mut commands: Commands,
) {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return;
    }
    let Some(page) = pages.iter().next() else {
        return;
    };
    for (entity, settings, observed, busy) in &engines {
        commands.entity(entity).insert(Opened);
        if observed.0.running || observed.0.docker_ready || busy.is_some() {
            continue;
        }
        commands.trigger(Request {
            entity,
            reply: entity,
            command: Command::Start,
            settings: settings.clone(),
            requester: page,
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
        &'static ConfigurationRevision,
        &'static Revision,
    ),
    With<EngineView>,
>;

type ProjectedPages<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static Output>,
        &'static Revision,
        &'static LastError,
        Option<&'static vmux_ecs::UiState<ContainerUiState>>,
    ),
    With<PageView>,
>;

fn project(
    pages: ProjectedPages,
    engines: ProjectedEngines,
    catalog: Catalog,
    mut commands: Commands,
) {
    let Ok((settings, observed, logs, error, busy, engine, inventory, revision, engine_revision)) =
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
    let mut containers = catalog.snapshot(engine);
    if !(observed.0.docker_ready || observed.0.running && observed.0.phase == Phase::Ready) {
        for container in &mut containers {
            container.state = "exited".into();
            container.status.clear();
        }
    }
    let snapshot = ContainerUiState {
        supported: cfg!(all(target_os = "macos", target_arch = "aarch64")),
        running: observed.0.running,
        ready: observed.0.docker_ready,
        busy: busy.is_some(),
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
        engine_revision: engine_revision.0,
        inventory_revision: inventory.map_or(0, |inventory| inventory.revision.0),
        containers,
        log_id: String::new(),
        container_logs: String::new(),
        container_log_error: String::new(),
        container_logs_ready: false,
        configuration_revision: revision.0,
        starting: busy.is_some_and(Busy::starting) && !observed.0.docker_ready,
        stopping: busy.is_some_and(Busy::stopping),
        control_revision: 0,
        control_error: String::new(),
    };
    for (page, output, completed, page_error, published) in &pages {
        let mut snapshot = snapshot.clone();
        snapshot.control_revision = completed.0;
        snapshot.control_error = page_error.0.clone();
        snapshot.error = if page_error.0.is_empty() {
            observed.0.failure.clone().unwrap_or_default()
        } else {
            page_error.0.clone()
        };
        if let Some(output) = output {
            snapshot.log_id = output.id.clone();
            snapshot.container_logs = output.text.clone();
            snapshot.container_log_error = output.error.clone();
            snapshot.container_logs_ready = output.ready;
        }
        if published.and_then(vmux_ecs::UiState::current) == Some(&snapshot) {
            continue;
        }
        commands.trigger(UiStateWrite::<ContainerUiState>::from_event(
            page, &snapshot,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::{
        BinHostEmitEvent, BinIpcEpoch, BinIpcNavigationRevision, BinIpcSurfaceGeneration, Browsers,
    };
    use vmux_api::{BinEvent, UiEventPermissions};

    #[derive(Resource, Default)]
    struct Delivery {
        writes: usize,
        frames: Vec<ContainerUiState>,
    }

    fn record_write(_: On<UiStateWrite<ContainerUiState>>, mut delivery: ResMut<Delivery>) {
        delivery.writes += 1;
    }

    fn record_frame(event: On<BinHostEmitEvent>, mut delivery: ResMut<Delivery>) {
        if event.event().id() != ContainerUiState::id() {
            return;
        }
        event.event().mark_delivered();
        let frame = rkyv::from_bytes::<
            vmux_api::UiStateFrame<ContainerUiState, ContainerUiState>,
            rkyv::rancor::Error,
        >(event.event().payload())
        .unwrap();
        let state = match frame {
            vmux_api::UiStateFrame::Snapshot(state) => state,
            vmux_api::UiStateFrame::Updates(mut states) => states.pop().unwrap(),
        };
        delivery.frames.push(state);
    }

    #[test]
    fn idle_projection_sleeps_but_changes_and_page_ready_deliver_current_state() {
        let mut app = App::new();
        app.add_plugins(ContainerPlugin)
            .add_message::<AppExit>()
            .init_resource::<Delivery>()
            .add_observer(record_write)
            .add_observer(record_frame);
        let directory = tempfile::tempdir().unwrap();
        let settings = Settings::new(directory.path(), directory.path());
        let engine = app
            .world_mut()
            .spawn((
                EngineView,
                settings.clone(),
                Observed(Status {
                    running: false,
                    docker_ready: false,
                    socket: settings.socket(),
                    log: settings.log(),
                    phase: Phase::Stopped,
                    failure: None,
                }),
                Logs::default(),
                LastError::default(),
                ConfigurationRevision::default(),
                Revision::default(),
            ))
            .id();
        let page = app
            .world_mut()
            .spawn((
                PageView,
                PageMetadata {
                    url: crate::Containers::URL.into(),
                    ..Default::default()
                },
                Revision::default(),
                LastError::default(),
                BinIpcSurfaceGeneration(1),
                BinIpcNavigationRevision(1),
                BinIpcEpoch(1),
            ))
            .id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(page);
        app.world_mut().insert_non_send(browsers);
        app.world_mut().spawn(UiEventPermissions {
            url: crate::Containers::URL,
            owns_subtree: true,
            permissions: &[ContainerUiState::PERMISSION],
        });
        app.world_mut().run_schedule(Update);
        app.world_mut().run_schedule(Last);
        assert_eq!(app.world().resource::<Delivery>().writes, 1);
        assert_eq!(app.world().resource::<Delivery>().frames.len(), 1);
        for _ in 0..20 {
            app.world_mut().run_schedule(Update);
            app.world_mut().run_schedule(Last);
        }
        assert_eq!(app.world().resource::<Delivery>().writes, 1);
        assert_eq!(app.world().resource::<Delivery>().frames.len(), 1);
        app.world_mut().get_mut::<Observed>(engine).unwrap().0.phase = Phase::DockerStarting;
        app.world_mut().run_schedule(Update);
        app.world_mut().run_schedule(Last);
        assert_eq!(app.world().resource::<Delivery>().writes, 2);
        assert_eq!(app.world().resource::<Delivery>().frames[1].phase, "docker");
        app.world_mut().get_mut::<LastError>(page).unwrap().0 = "start refused".into();
        app.world_mut().run_schedule(Update);
        app.world_mut().run_schedule(Last);
        assert_eq!(app.world().resource::<Delivery>().writes, 3);
        assert_eq!(
            app.world().resource::<Delivery>().frames[2].control_error,
            "start refused"
        );
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: PageReady {},
        });
        app.world_mut().run_schedule(Update);
        app.world_mut().run_schedule(Last);
        assert_eq!(app.world().resource::<Delivery>().writes, 3);
        let delivery = app.world().resource::<Delivery>();
        assert_eq!(delivery.frames.len(), 4);
        assert_eq!(delivery.frames[3], delivery.frames[2]);
    }
}
