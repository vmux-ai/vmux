use std::io;
use std::time::{Duration, Instant};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_tasks::{IoTaskPool, Task, TaskPoolBuilder, futures_lite::future};

use crate::Settings;
use crate::client::EngineClient;
use crate::engine::{Deadline, Engine, EngineSet, Failure, ForcedShutdown, Machine, Phase};
use crate::ports::PortForwarder;
use crate::process::Process;
use crate::ssh::Ssh;

pub(crate) struct ConnectionPlugin;

impl Plugin for ConnectionPlugin {
    fn build(&self, app: &mut App) {
        IoTaskPool::get_or_init(|| {
            TaskPoolBuilder::new()
                .num_threads(2)
                .thread_name("VM I/O".into())
                .build()
        });
        app.add_systems(
            Update,
            (begin, poll, shutdown).chain().in_set(EngineSet::Connect),
        );
    }
}

#[derive(Component, Clone, Debug)]
pub(crate) struct ConnectionIssue(pub(crate) String);

#[derive(Component)]
struct Connection {
    stage: Stage,
    next: Instant,
    ports: Option<PortForwarder>,
}

impl Default for Connection {
    fn default() -> Self {
        Self {
            stage: Stage::Transport,
            next: Instant::now(),
            ports: None,
        }
    }
}

#[derive(Clone, Copy)]
enum Stage {
    Transport,
    Forwarding,
    Docker,
    Ports,
}

#[derive(Component)]
struct ConnectionTask(Task<Report>);

enum Report {
    Transport(bool),
    Forwarding(io::Result<()>),
    Docker(bool),
    Ports(PortForwarder, io::Result<()>),
}

#[derive(Component)]
struct StopWindow {
    deadline: Instant,
    next: Instant,
}

#[derive(Component)]
struct StopTask(Task<io::Result<()>>);

type ConnectingEngines<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static Phase,
        Option<&'static mut Connection>,
        Option<&'static ConnectionTask>,
    ),
    With<Engine>,
>;

fn begin(mut engines: ConnectingEngines, mut commands: Commands) {
    for (entity, settings, phase, connection, task) in &mut engines {
        if !phase.connecting() || task.is_some() {
            continue;
        }
        let Some(mut connection) = connection else {
            commands.entity(entity).insert(Connection::default());
            continue;
        };
        if Instant::now() < connection.next {
            continue;
        }
        let settings = settings.clone();
        let stage = connection.stage;
        let ports = if matches!(stage, Stage::Ports) {
            connection.ports.take()
        } else {
            None
        };
        let task = IoTaskPool::get().spawn(async move {
            match stage {
                Stage::Transport => {
                    Report::Transport(EngineClient::ping(&settings.transport_socket()))
                }
                Stage::Forwarding => Report::Forwarding(Ssh::forward(&settings)),
                Stage::Docker => Report::Docker(EngineClient::ping(&settings.socket())),
                Stage::Ports => {
                    let mut ports = ports.unwrap_or_else(|| {
                        PortForwarder::new(&settings.socket(), &settings.network_api())
                    });
                    let result = ports.refresh();
                    Report::Ports(ports, result)
                }
            }
        });
        commands.entity(entity).insert(ConnectionTask(task));
    }
}

type ActiveConnections<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static mut Phase,
        &'static mut Connection,
        &'static mut ConnectionTask,
        &'static Deadline,
    ),
    With<Engine>,
>;

fn poll(mut engines: ActiveConnections, mut commands: Commands) {
    for (entity, settings, mut phase, mut connection, mut task, deadline) in &mut engines {
        if !phase.connecting() {
            commands.entity(entity).remove::<ConnectionTask>();
            continue;
        }
        if *phase == Phase::DockerStarting && Instant::now() >= deadline.0 {
            *phase = Phase::Stopping;
            commands
                .entity(entity)
                .insert(Failure("Docker did not become ready".into()))
                .remove::<ConnectionTask>();
            continue;
        }
        let Some(report) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<ConnectionTask>();
        connection.next = Instant::now() + Duration::from_millis(250);
        match report {
            Report::Transport(true) => connection.stage = Stage::Forwarding,
            Report::Transport(false) => {}
            Report::Forwarding(Ok(())) => connection.stage = Stage::Docker,
            Report::Forwarding(Err(error)) => {
                commands
                    .entity(entity)
                    .insert(ConnectionIssue(error.to_string()));
            }
            Report::Docker(true) => {
                *phase = Phase::Ready;
                connection.stage = Stage::Ports;
                connection.ports = Some(PortForwarder::new(
                    &settings.socket(),
                    &settings.network_api(),
                ));
                commands.entity(entity).remove::<ConnectionIssue>();
            }
            Report::Docker(false) => {}
            Report::Ports(ports, result) => {
                connection.ports = Some(ports);
                connection.next = Instant::now() + Duration::from_secs(1);
                match result {
                    Ok(()) => {
                        commands.entity(entity).remove::<ConnectionIssue>();
                    }
                    Err(error) => {
                        commands
                            .entity(entity)
                            .insert(ConnectionIssue(error.to_string()));
                    }
                }
            }
        }
    }
}

type StoppingEngines<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static Phase,
        Option<&'static mut StopWindow>,
        Option<&'static mut StopTask>,
    ),
    With<Engine>,
>;

fn shutdown(
    mut engines: StoppingEngines,
    machines: Query<&ChildOf, (With<Machine>, With<Process>)>,
    mut commands: Commands,
) {
    for (entity, settings, phase, window, task) in &mut engines {
        if !matches!(phase, Phase::Stopping | Phase::Failed)
            || !machines.iter().any(|machine| machine.parent() == entity)
        {
            continue;
        }
        commands.entity(entity).remove::<ConnectionTask>();
        let Some(mut window) = window else {
            commands.entity(entity).insert(StopWindow {
                deadline: Instant::now() + Duration::from_secs(60),
                next: Instant::now(),
            });
            continue;
        };
        if Instant::now() >= window.deadline {
            commands
                .entity(entity)
                .insert((ForcedShutdown, Failure("VM did not finish shutdown".into())))
                .remove::<StopTask>();
            continue;
        }
        if let Some(mut task) = task {
            let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
                continue;
            };
            commands.entity(entity).remove::<StopTask>();
            window.next = Instant::now() + Duration::from_secs(1);
            match result {
                Ok(()) => {
                    commands.entity(entity).remove::<ConnectionIssue>();
                }
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(ConnectionIssue(error.to_string()));
                }
            }
            continue;
        }
        if Instant::now() < window.next {
            continue;
        }
        let settings = settings.clone();
        commands.entity(entity).insert(StopTask(
            IoTaskPool::get().spawn(async move { EngineClient::new(settings).request_stop() }),
        ));
    }
}
