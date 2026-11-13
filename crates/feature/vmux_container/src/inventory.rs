use std::collections::HashMap;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::Revision;
use vmux_ecs::wake::Wake;
use vmux_vm::{Container as DockerContainer, DockerClient, DockerPort, Settings};

use crate::control::{Busy, Command, Finished, Request};
use crate::event::{
    ContainerCommandRequest, ContainerEntry, ContainerInspectRequest, ContainerPort,
};
use crate::host::{ContainerSet, EngineView, Observed, PageView};

pub(crate) struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            ContainerCommandRequest,
            ContainerInspectRequest,
        )>::default())
            .add_observer(control)
            .add_observer(select)
            .add_systems(
                Update,
                (begin, poll, begin_save, poll_save, begin_logs, poll_logs)
                    .chain()
                    .in_set(ContainerSet::Inventory),
            );
    }
}

#[derive(Component, Default)]
pub(crate) struct Inventory {
    pub(crate) ready: bool,
    pub(crate) error: String,
    pub(crate) revision: Revision,
}

#[derive(Component)]
struct NextPoll(Instant);

#[derive(Component)]
struct SnapshotTask {
    revision: Revision,
    origin: Origin,
    task: Task<io::Result<Vec<DockerContainer>>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Origin {
    Cache,
    Daemon,
}

#[derive(Component)]
struct PendingSave(Vec<DockerContainer>);

#[derive(Component)]
struct SaveTask(Task<io::Result<()>>);

struct Cache(PathBuf);

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedInventory {
    version: u32,
    containers: Vec<DockerContainer>,
}

impl Cache {
    fn new(settings: &Settings) -> Self {
        Self(settings.directory.join("inventory.json"))
    }

    fn load(&self) -> io::Result<Vec<DockerContainer>> {
        let bytes = std::fs::read(&self.0)?;
        let saved: SavedInventory = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if saved.version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported inventory cache version",
            ));
        }
        Ok(saved.containers)
    }

    fn save(&self, mut containers: Vec<DockerContainer>) -> io::Result<()> {
        for container in &mut containers {
            container.status.clear();
        }
        let saved = SavedInventory {
            version: 1,
            containers,
        };
        let bytes = serde_json::to_vec(&saved).map_err(io::Error::other)?;
        if std::fs::read(&self.0).is_ok_and(|current| current == bytes) {
            return Ok(());
        }
        let directory = self
            .0
            .parent()
            .ok_or_else(|| io::Error::other("missing inventory cache directory"))?;
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        file.write_all(&bytes)?;
        file.as_file().sync_data()?;
        file.persist(&self.0).map_err(|error| error.error)?;
        Ok(())
    }
}

#[derive(Component)]
struct Entry;

#[derive(Component)]
#[component(immutable)]
struct Id(String);

#[derive(Component)]
struct Image(String);

#[derive(Component)]
struct State {
    state: String,
    status: String,
}

#[derive(Component)]
struct Compose {
    project: String,
    service: String,
}

#[derive(Component)]
struct Ports(Vec<DockerPort>);

type Entries<'w, 's> = Query<
    'w,
    's,
    (
        &'static Id,
        &'static Name,
        &'static Image,
        &'static State,
        &'static Compose,
        &'static Ports,
        &'static ChildOf,
    ),
    With<Entry>,
>;

#[derive(SystemParam)]
pub(crate) struct Catalog<'w, 's> {
    entries: Entries<'w, 's>,
}

impl Catalog<'_, '_> {
    pub(crate) fn snapshot(&self, engine: Entity) -> Vec<ContainerEntry> {
        let mut rows = Vec::new();
        for (id, name, image, state, compose, ports, parent) in &self.entries {
            if parent.parent() != engine {
                continue;
            }
            let mut bindings = Vec::new();
            for port in &ports.0 {
                bindings.push(ContainerPort {
                    host: port.host,
                    guest: port.guest,
                    protocol: port.protocol.clone(),
                });
            }
            rows.push(ContainerEntry {
                id: id.0.clone(),
                name: name.as_str().to_string(),
                image: image.0.clone(),
                state: state.state.clone(),
                status: state.status.clone(),
                project: compose.project.clone(),
                service: compose.service.clone(),
                ports: bindings,
            });
        }
        rows.sort_by(|left, right| {
            left.project
                .cmp(&right.project)
                .then_with(|| left.name.cmp(&right.name))
        });
        rows
    }

    fn contains(&self, engine: Entity, id: &str) -> bool {
        self.entries
            .iter()
            .any(|(entry, _, _, _, _, _, parent)| parent.parent() == engine && entry.0 == id)
    }
}

#[derive(Component)]
struct Selection {
    id: String,
    logs: bool,
    next: Instant,
}

#[derive(Component, Default)]
pub(crate) struct Output {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) error: String,
    pub(crate) ready: bool,
}

#[derive(Component)]
struct LogTask {
    id: String,
    task: Task<io::Result<String>>,
}

type InspectedEngines<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static Observed,
        &'static Revision,
        Option<&'static mut Inventory>,
        Option<&'static mut NextPoll>,
        Option<&'static SnapshotTask>,
    ),
    With<EngineView>,
>;

fn begin(
    mut engines: InspectedEngines,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, settings, observed, revision, inventory, next, task) in &mut engines {
        let (Some(mut inventory), Some(next)) = (inventory, next) else {
            let cache = Cache::new(settings);
            let wake = Wake::beside(proxy.as_deref());
            commands.entity(entity).insert((
                Inventory::default(),
                NextPoll(Instant::now()),
                SnapshotTask {
                    revision: *revision,
                    origin: Origin::Cache,
                    task: IoTaskPool::get().spawn(async move {
                        let result = cache.load();
                        drop(wake);
                        result
                    }),
                },
            ));
            continue;
        };
        if task.is_some_and(|task| task.origin == Origin::Cache) {
            continue;
        }
        if !observed.0.docker_ready {
            if observed.0.running && observed.0.phase == vmux_vm::Phase::Ready {
                continue;
            }
            inventory.error.clear();
            commands.entity(entity).remove::<SnapshotTask>();
            continue;
        }
        if task.is_some() || Instant::now() < next.0 {
            continue;
        }
        let socket = settings.socket();
        let wake = Wake::beside(proxy.as_deref());
        commands.entity(entity).insert(SnapshotTask {
            revision: *revision,
            origin: Origin::Daemon,
            task: IoTaskPool::get().spawn(async move {
                let result = DockerClient::new(&socket).list();
                drop(wake);
                result
            }),
        });
    }
}

fn poll(
    mut engines: Query<
        (
            Entity,
            &mut Inventory,
            &mut NextPoll,
            &mut SnapshotTask,
            &Revision,
        ),
        With<EngineView>,
    >,
    entries: Query<(Entity, &Id, &ChildOf), With<Entry>>,
    mut commands: Commands,
) {
    for (entity, mut inventory, mut next, mut task, revision) in &mut engines {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).remove::<SnapshotTask>();
        if task.origin == Origin::Daemon && task.revision != *revision {
            next.0 = Instant::now();
            continue;
        }
        next.0 = if task.origin == Origin::Cache {
            Instant::now()
        } else {
            Instant::now() + Duration::from_secs(2)
        };
        let containers = match result {
            Ok(containers) => containers,
            Err(error) => {
                if task.origin == Origin::Cache {
                    if error.kind() != io::ErrorKind::NotFound {
                        warn!(%error, "could not restore container inventory");
                    }
                } else {
                    inventory.error = error.to_string();
                }
                continue;
            }
        };
        inventory.ready = true;
        inventory.error.clear();
        inventory.revision = task.revision;
        if task.origin == Origin::Daemon {
            commands
                .entity(entity)
                .insert(PendingSave(containers.clone()));
        }
        let mut existing = HashMap::new();
        for (entry, id, parent) in &entries {
            if parent.parent() == entity {
                existing.insert(id.0.clone(), entry);
            }
        }
        for container in containers {
            let data = (
                Name::new(container.name),
                Image(container.image),
                State {
                    state: container.state,
                    status: container.status,
                },
                Compose {
                    project: container.project,
                    service: container.service,
                },
                Ports(container.ports),
            );
            match existing.remove(&container.id) {
                Some(entry) => {
                    commands.entity(entry).insert(data);
                }
                None => {
                    commands.spawn((Entry, Id(container.id), ChildOf(entity), data));
                }
            }
        }
        for entry in existing.into_values() {
            commands.entity(entry).despawn();
        }
    }
}

type PendingSaves<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Settings, &'static PendingSave),
    (With<EngineView>, Without<SaveTask>),
>;

fn begin_save(
    engines: PendingSaves,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, settings, pending) in &engines {
        let cache = Cache::new(settings);
        let containers = pending.0.clone();
        let wake = Wake::beside(proxy.as_deref());
        commands
            .entity(entity)
            .remove::<PendingSave>()
            .insert(SaveTask(IoTaskPool::get().spawn(async move {
                let result = cache.save(containers);
                drop(wake);
                result
            })));
    }
}

fn poll_save(
    mut engines: Query<(Entity, &mut SaveTask), With<EngineView>>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut engines {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<SaveTask>();
        if let Err(error) = result {
            warn!(%error, "could not cache container inventory");
        }
    }
}

fn control(
    trigger: On<UiInput<ContainerCommandRequest>>,
    pages: Query<(), With<PageView>>,
    engines: Query<(Entity, &Settings, &Observed, Option<&Busy>), With<EngineView>>,
    catalog: Catalog,
    mut commands: Commands,
) {
    let event = trigger.event();
    if !pages.contains(event.webview) || event.payload.ids.is_empty() {
        return;
    }
    let Ok((entity, settings, observed, _)) = engines.single() else {
        return;
    };
    if event.payload.operation != crate::event::ContainerOperation::Start
        && !(observed.0.docker_ready
            || (observed.0.running && observed.0.phase == vmux_vm::Phase::Ready))
    {
        commands.trigger(Finished {
            entity,
            result: Err("Docker is not ready".into()),
            mutates: true,
            requester: event.webview,
        });
        return;
    }
    for id in &event.payload.ids {
        if !catalog.contains(entity, id) {
            commands.trigger(Finished {
                entity,
                result: Err(format!("container {id} is no longer present")),
                mutates: true,
                requester: event.webview,
            });
            return;
        }
    }
    commands.trigger(Request {
        entity,
        reply: entity,
        command: Command::Containers {
            ids: event.payload.ids.clone(),
            operation: event.payload.operation,
        },
        settings: settings.clone(),
        requester: event.webview,
    });
}

fn select(
    trigger: On<UiInput<ContainerInspectRequest>>,
    pages: Query<Option<&Output>, With<PageView>>,
    engines: Query<Entity, With<EngineView>>,
    catalog: Catalog,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok(previous) = pages.get(event.webview) else {
        return;
    };
    let Ok(engine) = engines.single() else {
        return;
    };
    if !event.payload.id.is_empty() && !catalog.contains(engine, &event.payload.id) {
        return;
    }
    commands.entity(event.webview).insert(Selection {
        id: event.payload.id.clone(),
        logs: event.payload.logs,
        next: Instant::now(),
    });
    if previous.is_none_or(|output| output.id != event.payload.id) {
        commands.entity(event.webview).insert(Output {
            id: event.payload.id.clone(),
            ..Default::default()
        });
    }
}

type LogPages<'w, 's> =
    Query<'w, 's, (Entity, &'static Selection, Option<&'static LogTask>), With<PageView>>;

fn begin_logs(
    pages: LogPages,
    engines: Query<(&Settings, &Observed), With<EngineView>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok((settings, observed)) = engines.single() else {
        return;
    };
    if !observed.0.docker_ready {
        return;
    }
    for (page, selection, task) in &pages {
        if !selection.logs
            || selection.id.is_empty()
            || task.is_some()
            || Instant::now() < selection.next
        {
            continue;
        }
        let socket = settings.socket();
        let id = selection.id.clone();
        let request_id = id.clone();
        let wake = Wake::beside(proxy.as_deref());
        commands.entity(page).insert(LogTask {
            id,
            task: IoTaskPool::get().spawn(async move {
                let result = DockerClient::new(&socket).logs(&request_id);
                drop(wake);
                result
            }),
        });
    }
}

fn poll_logs(
    mut pages: Query<(Entity, &mut Selection, &mut Output, &mut LogTask), With<PageView>>,
    mut commands: Commands,
) {
    for (page, mut selection, mut output, mut task) in &mut pages {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(page).remove::<LogTask>();
        if selection.id != task.id {
            continue;
        }
        selection.next = Instant::now() + Duration::from_secs(2);
        output.ready = true;
        match result {
            Ok(text) => {
                output.text = text;
                output.error.clear();
            }
            Err(error) => {
                output.error = error.to_string();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_vm::{Phase, Status};

    #[test]
    fn offline_host_restores_saved_containers_and_fresh_inventory_replaces_the_cache() {
        IoTaskPool::get_or_init(|| bevy::tasks::TaskPoolBuilder::new().num_threads(2).build());
        let directory = tempfile::tempdir().unwrap();
        let settings = Settings::new(directory.path(), "/fixtures/shared");
        let cache = Cache::new(&settings);
        std::fs::write(&cache.0, r#"{"version":1,"containers":[{"id":"aaaaaaaaaaaa","name":"account","image":"account:dev","state":"exited","status":"","project":"vmux-cloud","service":"account","dependencies":[],"ports":[]}]}"#).unwrap();
        let mut app = App::new();
        app.add_plugins(InventoryPlugin);
        let engine = app
            .world_mut()
            .spawn((
                EngineView,
                settings.clone(),
                Revision(0),
                Observed(Status {
                    running: false,
                    docker_ready: false,
                    socket: settings.socket(),
                    log: settings.log(),
                    phase: Phase::Stopped,
                    failure: None,
                }),
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !app
            .world()
            .get::<Inventory>(engine)
            .is_some_and(|inventory| inventory.ready)
        {
            assert!(Instant::now() < deadline);
            app.update();
            std::thread::sleep(Duration::from_millis(1));
        }
        let rows = app
            .world_mut()
            .run_system_once(move |catalog: Catalog| catalog.snapshot(engine))
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "account");
        assert_eq!(rows[0].project, "vmux-cloud");
        assert!(app.world().get::<SaveTask>(engine).is_none());
        app.world_mut().entity_mut(engine).insert((
            Revision(1),
            Observed(Status {
                running: true,
                docker_ready: false,
                socket: settings.socket(),
                log: settings.log(),
                phase: Phase::Ready,
                failure: None,
            }),
            SnapshotTask {
                revision: Revision(1),
                origin: Origin::Daemon,
                task: IoTaskPool::get().spawn(async { Ok(Vec::new()) }),
            },
        ));
        loop {
            assert!(Instant::now() < deadline);
            app.update();
            let world = app.world();
            if world.get::<SnapshotTask>(engine).is_none()
                && world.get::<SaveTask>(engine).is_none()
                && world.get::<PendingSave>(engine).is_none()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(cache.load().unwrap().is_empty());
        assert_eq!(
            app.world().get::<Inventory>(engine).unwrap().revision,
            Revision(1)
        );
    }

    #[test]
    fn cache_save_ignores_only_relative_uptime_and_refuses_unknown_versions() {
        let directory = tempfile::tempdir().unwrap();
        let cache = Cache::new(&Settings::new(directory.path(), "/fixtures/shared"));
        let mut container = DockerContainer {
            id: "aaaaaaaaaaaa".into(),
            name: "account".into(),
            image: "account:dev".into(),
            state: "running".into(),
            status: "Up 1 second".into(),
            project: "vmux-cloud".into(),
            service: "account".into(),
            dependencies: Vec::new(),
            ports: Vec::new(),
        };
        cache.save(vec![container.clone()]).unwrap();
        let before = std::fs::metadata(&cache.0).unwrap().modified().unwrap();
        container.status = "Up 2 hours".into();
        cache.save(vec![container.clone()]).unwrap();
        assert_eq!(
            std::fs::metadata(&cache.0).unwrap().modified().unwrap(),
            before
        );
        container.image = "account:new".into();
        cache.save(vec![container]).unwrap();
        assert_eq!(cache.load().unwrap()[0].image, "account:new");
        std::fs::write(&cache.0, r#"{"version":99,"containers":[]}"#).unwrap();
        assert_eq!(cache.load().unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn stopping_an_engine_retains_known_container_configuration() {
        let mut app = App::new();
        app.add_plugins(InventoryPlugin);
        let settings = Settings::new("/fixtures/data", "/fixtures/shared");
        let engine = app
            .world_mut()
            .spawn((
                EngineView,
                settings.clone(),
                Revision(1),
                Observed(Status {
                    running: false,
                    docker_ready: false,
                    socket: settings.socket(),
                    log: settings.log(),
                    phase: Phase::Stopped,
                    failure: None,
                }),
                Inventory {
                    ready: true,
                    ..Default::default()
                },
                NextPoll(Instant::now()),
            ))
            .id();
        let entry = app
            .world_mut()
            .spawn((
                Entry,
                Id("aaaaaaaaaaaa".into()),
                Name::new("account"),
                Image("account:dev".into()),
                State {
                    state: "exited".into(),
                    status: String::new(),
                },
                Compose {
                    project: "cloud".into(),
                    service: "account".into(),
                },
                Ports(Vec::new()),
                ChildOf(engine),
            ))
            .id();
        app.update();
        assert!(app.world().get::<Entry>(entry).is_some());
        assert!(app.world().get::<Inventory>(engine).unwrap().ready);
        let rows = app
            .world_mut()
            .run_system_once(move |catalog: Catalog| catalog.snapshot(engine))
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].image, "account:dev");
        assert_eq!(rows[0].project, "cloud");
    }

    #[test]
    fn a_snapshot_started_before_completion_cannot_replace_current_entries() {
        IoTaskPool::get_or_init(|| bevy::tasks::TaskPoolBuilder::new().num_threads(1).build());
        let mut app = App::new();
        app.add_plugins(InventoryPlugin);
        let settings = Settings::new("/fixtures/data", "/fixtures/shared");
        let engine = app
            .world_mut()
            .spawn((
                EngineView,
                settings.clone(),
                Revision(1),
                Observed(Status {
                    running: true,
                    docker_ready: false,
                    socket: settings.socket(),
                    log: settings.log(),
                    phase: Phase::Ready,
                    failure: None,
                }),
                Inventory {
                    ready: true,
                    ..Default::default()
                },
                NextPoll(Instant::now()),
                SnapshotTask {
                    revision: Revision(0),
                    origin: Origin::Daemon,
                    task: IoTaskPool::get().spawn(async { Ok(Vec::new()) }),
                },
            ))
            .id();
        let entry = app
            .world_mut()
            .spawn((
                Entry,
                Id("aaaaaaaaaaaa".into()),
                Name::new("kept"),
                Image("image".into()),
                State {
                    state: "running".into(),
                    status: "Up".into(),
                },
                Compose {
                    project: "cloud".into(),
                    service: "web".into(),
                },
                Ports(Vec::new()),
                ChildOf(engine),
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.world().get::<SnapshotTask>(engine).is_some() {
            assert!(Instant::now() < deadline);
            app.update();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.world().get::<Entry>(entry).is_some());
        assert_eq!(
            app.world().get::<Inventory>(engine).unwrap().revision,
            Revision(0)
        );
        app.world_mut().entity_mut(engine).insert(SnapshotTask {
            revision: Revision(1),
            origin: Origin::Daemon,
            task: IoTaskPool::get().spawn(async { Ok(Vec::new()) }),
        });
        while app.world().get::<SnapshotTask>(engine).is_some() {
            assert!(Instant::now() < deadline);
            app.update();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.world().get::<Entry>(entry).is_none());
        assert_eq!(
            app.world().get::<Inventory>(engine).unwrap().revision,
            Revision(1)
        );
    }
}
