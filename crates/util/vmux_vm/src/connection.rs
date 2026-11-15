use std::io;
use std::time::{Duration, Instant};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_tasks::{IoTaskPool, Task, TaskPoolBuilder, futures_lite::future};

use crate::Settings;
use crate::client::EngineClient;
use crate::control::GuestControl;
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
            (
                initialize, expire, transport, forward, docker, ports, shutdown,
            )
                .chain()
                .in_set(EngineSet::Connect),
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
struct TransportTask(Task<bool>);

#[derive(Component)]
struct ForwardingTask(Task<io::Result<()>>);

#[derive(Component)]
struct DockerTask(Task<bool>);

#[derive(Component)]
struct PortsTask(Task<(PortForwarder, io::Result<()>)>);

#[derive(Component)]
struct StopWindow {
    deadline: Instant,
    next: Instant,
}

#[derive(Component)]
struct StopTask(Task<io::Result<()>>);

type NewConnections<'w, 's> =
    Query<'w, 's, (Entity, &'static Phase), (With<Engine>, Without<Connection>)>;

fn initialize(engines: NewConnections, mut commands: Commands) {
    for (entity, phase) in &engines {
        if phase.connecting() {
            commands.entity(entity).insert(Connection::default());
        }
    }
}

fn expire(
    mut engines: Query<(Entity, &mut Phase, &Deadline), With<Engine>>,
    mut commands: Commands,
) {
    for (entity, mut phase, deadline) in &mut engines {
        if *phase == Phase::DockerStarting && Instant::now() >= deadline.0 {
            *phase = Phase::Stopping;
            commands
                .entity(entity)
                .insert(Failure("Docker did not become ready".into()));
        }
        if !phase.connecting() {
            commands
                .entity(entity)
                .remove::<(TransportTask, ForwardingTask, DockerTask, PortsTask)>();
        }
    }
}

type TransportConnections<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static Phase,
        &'static mut Connection,
        Option<&'static mut TransportTask>,
    ),
    With<Engine>,
>;

fn transport(mut engines: TransportConnections, mut commands: Commands) {
    for (entity, settings, phase, mut connection, task) in &mut engines {
        if !phase.connecting() || !matches!(connection.stage, Stage::Transport) {
            continue;
        }
        if let Some(mut task) = task {
            let Some(ready) = future::block_on(future::poll_once(&mut task.0)) else {
                continue;
            };
            commands.entity(entity).remove::<TransportTask>();
            connection.next = Instant::now() + Duration::from_millis(250);
            if ready {
                connection.stage = Stage::Forwarding;
            }
            continue;
        }
        if Instant::now() < connection.next {
            continue;
        }
        let settings = settings.clone();
        commands
            .entity(entity)
            .insert(TransportTask(IoTaskPool::get().spawn(async move {
                EngineClient::ping(&settings.transport_socket())
            })));
    }
}

type ForwardingConnections<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static Phase,
        &'static mut Connection,
        Option<&'static mut ForwardingTask>,
    ),
    With<Engine>,
>;

fn forward(mut engines: ForwardingConnections, mut commands: Commands) {
    for (entity, settings, phase, mut connection, task) in &mut engines {
        if !phase.connecting() || !matches!(connection.stage, Stage::Forwarding) {
            continue;
        }
        if let Some(mut task) = task {
            let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
                continue;
            };
            commands.entity(entity).remove::<ForwardingTask>();
            connection.next = Instant::now() + Duration::from_millis(250);
            match result {
                Ok(()) => connection.stage = Stage::Docker,
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(ConnectionIssue(error.to_string()));
                }
            }
            continue;
        }
        if Instant::now() < connection.next {
            continue;
        }
        let settings = settings.clone();
        commands.entity(entity).insert(ForwardingTask(
            IoTaskPool::get().spawn(async move { Ssh::forward(&settings) }),
        ));
    }
}

type DockerConnections<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static mut Phase,
        &'static mut Connection,
        Option<&'static mut DockerTask>,
    ),
    With<Engine>,
>;

fn docker(mut engines: DockerConnections, mut commands: Commands) {
    for (entity, settings, mut phase, mut connection, task) in &mut engines {
        if !phase.connecting() || !matches!(connection.stage, Stage::Docker) {
            continue;
        }
        if let Some(mut task) = task {
            let Some(ready) = future::block_on(future::poll_once(&mut task.0)) else {
                continue;
            };
            commands.entity(entity).remove::<DockerTask>();
            connection.next = Instant::now() + Duration::from_millis(250);
            if ready {
                *phase = Phase::Ready;
                connection.stage = Stage::Ports;
                connection.ports = Some(PortForwarder::new(
                    &settings.socket(),
                    &settings.network_api(),
                ));
                commands.entity(entity).remove::<ConnectionIssue>();
            }
            continue;
        }
        if Instant::now() < connection.next {
            continue;
        }
        let settings = settings.clone();
        commands.entity(entity).insert(DockerTask(
            IoTaskPool::get().spawn(async move { EngineClient::ping(&settings.socket()) }),
        ));
    }
}

type PortConnections<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static Phase,
        &'static mut Connection,
        Option<&'static mut PortsTask>,
    ),
    With<Engine>,
>;

fn ports(mut engines: PortConnections, mut commands: Commands) {
    for (entity, settings, phase, mut connection, task) in &mut engines {
        if !phase.connecting() || !matches!(connection.stage, Stage::Ports) {
            continue;
        }
        if let Some(mut task) = task {
            let Some((ports, result)) = future::block_on(future::poll_once(&mut task.0)) else {
                continue;
            };
            commands.entity(entity).remove::<PortsTask>();
            connection.ports = Some(ports);
            connection.next = Instant::now() + Duration::from_secs(1);
            match result {
                Ok(()) => {
                    commands.entity(entity).remove::<ConnectionIssue>();
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                            | io::ErrorKind::ConnectionReset
                            | io::ErrorKind::ConnectionAborted
                            | io::ErrorKind::ConnectionRefused
                            | io::ErrorKind::UnexpectedEof
                    ) => {}
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(ConnectionIssue(error.to_string()));
                }
            }
            continue;
        }
        if Instant::now() < connection.next {
            continue;
        }
        let settings = settings.clone();
        let mut ports = match connection.ports.take() {
            Some(ports) => ports,
            None => PortForwarder::new(&settings.socket(), &settings.network_api()),
        };
        commands
            .entity(entity)
            .insert(PortsTask(IoTaskPool::get().spawn(async move {
                let result = ports.refresh();
                (ports, result)
            })));
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
        commands
            .entity(entity)
            .remove::<(TransportTask, ForwardingTask, DockerTask, PortsTask)>();
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
            IoTaskPool::get().spawn(async move { GuestControl::stop(&settings) }),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EnginePlugin;

    #[test]
    fn accepted_stop_retires_an_unresponsive_guest_after_the_shutdown_deadline() {
        let directory = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(directory.path().join("engine"), directory.path());
        settings.validate().unwrap();
        let lease = crate::process::Lease::acquire(&settings.directory).unwrap();
        let mut app = App::new();
        app.add_plugins(EnginePlugin);
        let engine = app
            .world_mut()
            .spawn((Engine, settings.clone(), Phase::Ready, lease))
            .id();
        let mut process = std::process::Command::new("sleep");
        process.arg("60");
        app.world_mut().spawn((
            Name::new("Unresponsive test VM"),
            Machine,
            ChildOf(engine),
            Process::spawn(&mut process).unwrap(),
        ));
        app.update();
        let client = std::thread::spawn(move || crate::EngineClient::new(settings).request_stop());
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.world().get::<Phase>(engine) != Some(&Phase::Stopping) {
            assert!(
                Instant::now() < deadline,
                "stop request was not adopted by the supervisor"
            );
            app.update();
            std::thread::sleep(Duration::from_millis(1));
        }
        client.join().unwrap().unwrap();
        app.world_mut()
            .get_mut::<StopWindow>(engine)
            .unwrap()
            .deadline = Instant::now() - Duration::from_secs(1);
        loop {
            app.update();
            if app.world().get::<crate::process::Lease>(engine).is_none() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "unresponsive guest retained its VM lease"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.world().get::<Phase>(engine), Some(&Phase::Failed));
        assert!(
            app.world_mut()
                .query::<&Process>()
                .iter(app.world())
                .next()
                .is_none()
        );
    }
    #[test]
    fn transient_port_reads_preserve_the_last_forwarding_result() {
        for (kind, previous, expected) in [
            (io::ErrorKind::WouldBlock, None, None),
            (
                io::ErrorKind::TimedOut,
                Some("address already in use"),
                Some("address already in use"),
            ),
            (io::ErrorKind::PermissionDenied, None, Some("read failed")),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let settings = Settings::new(directory.path(), directory.path());
            let ports = PortForwarder::new(&settings.socket(), &settings.network_api());
            let mut app = App::new();
            app.add_plugins(EnginePlugin);
            let entity =
                app.world_mut()
                    .spawn((
                        Engine,
                        settings,
                        Phase::Ready,
                        Connection {
                            stage: Stage::Ports,
                            next: Instant::now(),
                            ports: None,
                        },
                        PortsTask(IoTaskPool::get().spawn(async move {
                            (ports, Err(io::Error::new(kind, "read failed")))
                        })),
                        Deadline(Instant::now() + Duration::from_secs(3)),
                    ))
                    .id();
            if let Some(previous) = previous {
                app.world_mut()
                    .entity_mut(entity)
                    .insert(ConnectionIssue(previous.into()));
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            while app.world().get::<PortsTask>(entity).is_some() {
                assert!(Instant::now() < deadline);
                app.update();
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(
                app.world()
                    .get::<ConnectionIssue>(entity)
                    .map(|issue| issue.0.as_str()),
                expected
            );
        }
    }
}
