use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::wake::Wake;
use vmux_vm::{Container as DockerContainer, DockerClient, DockerPort, Settings};

use crate::control::{Busy, Command, Request};
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
                (begin, poll, begin_logs, poll_logs)
                    .chain()
                    .in_set(ContainerSet::Inventory),
            );
    }
}

#[derive(Component, Default)]
pub(crate) struct Inventory {
    pub(crate) ready: bool,
    pub(crate) error: String,
}

#[derive(Component)]
struct NextPoll(Instant);

#[derive(Component)]
struct SnapshotTask(Task<io::Result<Vec<DockerContainer>>>);

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
        Option<&'static mut Inventory>,
        Option<&'static mut NextPoll>,
        Option<&'static SnapshotTask>,
    ),
    With<EngineView>,
>;

fn begin(
    mut engines: InspectedEngines,
    entries: Query<(Entity, &ChildOf), With<Entry>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, settings, observed, inventory, next, task) in &mut engines {
        let (Some(mut inventory), Some(next)) = (inventory, next) else {
            commands
                .entity(entity)
                .insert((Inventory::default(), NextPoll(Instant::now())));
            continue;
        };
        if !observed.0.docker_ready {
            inventory.ready = false;
            inventory.error.clear();
            commands.entity(entity).remove::<SnapshotTask>();
            for (entry, parent) in &entries {
                if parent.parent() == entity {
                    commands.entity(entry).despawn();
                }
            }
            continue;
        }
        if task.is_some() || Instant::now() < next.0 {
            continue;
        }
        let socket = settings.socket();
        let wake = Wake::beside(proxy.as_deref());
        commands
            .entity(entity)
            .insert(SnapshotTask(IoTaskPool::get().spawn(async move {
                let result = DockerClient::new(&socket).list();
                drop(wake);
                result
            })));
    }
}

fn poll(
    mut engines: Query<
        (Entity, &mut Inventory, &mut NextPoll, &mut SnapshotTask),
        With<EngineView>,
    >,
    entries: Query<(Entity, &Id, &ChildOf), With<Entry>>,
    mut commands: Commands,
) {
    for (entity, mut inventory, mut next, mut task) in &mut engines {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<SnapshotTask>();
        next.0 = Instant::now() + Duration::from_secs(2);
        let containers = match result {
            Ok(containers) => containers,
            Err(error) => {
                inventory.error = error.to_string();
                continue;
            }
        };
        inventory.ready = true;
        inventory.error.clear();
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
    let Ok((entity, settings, observed, busy)) = engines.single() else {
        return;
    };
    if busy.is_some() || !observed.0.docker_ready {
        return;
    }
    for id in &event.payload.ids {
        if !catalog.contains(entity, id) {
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
