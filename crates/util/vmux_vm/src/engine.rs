use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use bevy_app::{App, AppExit, Plugin, PostUpdate, ScheduleRunnerPlugin, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Settings;
use crate::connection::{ConnectionIssue, ConnectionPlugin};
use crate::process::{Executable, Lease, Process};

pub struct EnginePlugin;

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ConnectionPlugin)
            .configure_sets(
                Update,
                (
                    EngineSet::Launch,
                    EngineSet::Connect,
                    EngineSet::Observe,
                    EngineSet::Retire,
                )
                    .chain(),
            )
            .add_systems(PostUpdate, (publish, release).chain())
            .add_systems(
                Update,
                (initialize, launch).chain().in_set(EngineSet::Launch),
            )
            .add_systems(Update, observe.in_set(EngineSet::Observe))
            .add_systems(Update, (retire, finish).chain().in_set(EngineSet::Retire));
    }
}

#[derive(Component)]
pub struct Engine;

impl Engine {
    pub fn bundle(settings: Settings, helper: PathBuf) -> impl Bundle {
        (
            Name::new("Container engine"),
            Self,
            settings,
            Launch(helper),
            Phase::Created,
        )
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Created,
    NetworkStarting,
    DockerStarting,
    Ready,
    Stopping,
    Stopped,
    Failed,
}

impl Phase {
    pub(crate) fn connecting(self) -> bool {
        matches!(self, Self::DockerStarting | Self::Ready)
    }
}

#[derive(Component, Clone, Debug)]
pub struct Failure(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct State {
    pub(crate) phase: Phase,
    pub(crate) failure: Option<String>,
    #[serde(default)]
    pub(crate) owner: Option<PathBuf>,
}

#[derive(Component)]
struct ApplicationOwner(PathBuf);

#[derive(Component)]
struct Published(State);

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EngineSet {
    Launch,
    Connect,
    Observe,
    Retire,
}

#[derive(Component)]
struct Launch(PathBuf);

#[derive(Component)]
pub(crate) struct Network;

#[derive(Component)]
pub(crate) struct Machine;

#[derive(Component)]
struct Terminating;

#[derive(Component)]
pub(crate) struct ForcedShutdown;

#[derive(Component)]
pub(crate) struct Deadline(pub(crate) Instant);

#[derive(Component)]
struct ResultSink(Sender<io::Result<()>>);

pub(crate) struct Runtime;

impl Runtime {
    pub(crate) fn run(settings: Settings, helper: PathBuf) -> io::Result<()> {
        let (sender, receiver) = channel();
        let mut app = App::new();
        app.add_plugins((
            ScheduleRunnerPlugin::run_loop(Duration::from_millis(50)),
            EnginePlugin,
        ));
        let mut engine = app
            .world_mut()
            .spawn((Engine::bundle(settings, helper), ResultSink(sender)));
        if let Some(owner) = std::env::var_os("VMUX_ENGINE_OWNER") {
            engine.insert(ApplicationOwner(PathBuf::from(owner)));
        }
        app.run();
        receiver.recv().map_err(io::Error::other)?
    }
}

impl Launch {
    fn network(&self, settings: &Settings) -> io::Result<Process> {
        let ssh_port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        Process::supervised(
            Command::new(Executable::named(&self.0, settings, " - Network")?)
                .arg("network")
                .arg(settings.directory.join("settings.json"))
                .arg(ssh_port.to_string()),
        )
    }

    fn machine(&self, settings: &Settings) -> io::Result<Process> {
        let configuration = settings.directory.join("settings.json");
        let helper = Executable::named(&self.0, settings, " - Linux")?;
        #[cfg(target_os = "macos")]
        let mut command = crate::sandbox::Sandbox::command(&helper, settings, &configuration)?;
        #[cfg(not(target_os = "macos"))]
        let mut command = {
            let mut command = Command::new(helper);
            command.arg("native").arg(configuration);
            command
        };
        command.env("VMUX_ENGINE_PARENT", std::process::id().to_string());
        Process::spawn(&mut command)
    }
}

fn initialize(
    mut engines: Query<(Entity, &mut Settings, &mut Phase, &Launch), With<Engine>>,
    mut commands: Commands,
) {
    for (entity, mut settings, mut phase, launcher) in &mut engines {
        if *phase != Phase::Created {
            continue;
        }
        if let Err(error) = settings.validate() {
            *phase = Phase::Failed;
            commands.entity(entity).insert(Failure(error.to_string()));
            continue;
        }
        let lease = match Lease::acquire(&settings.directory) {
            Ok(lease) => lease,
            Err(error) => {
                *phase = Phase::Failed;
                commands.entity(entity).insert(Failure(error.to_string()));
                continue;
            }
        };
        commands.entity(entity).insert(lease);
        if let Err(error) = Lease::retired(&settings.directory) {
            *phase = Phase::Failed;
            commands.entity(entity).insert(Failure(error.to_string()));
            continue;
        }
        if let Err(error) = settings.clean_sockets() {
            *phase = Phase::Failed;
            commands.entity(entity).insert(Failure(error.to_string()));
            continue;
        }
        if let Err(error) = settings.save() {
            *phase = Phase::Failed;
            commands.entity(entity).insert(Failure(error.to_string()));
            continue;
        }
        match launcher.network(&settings) {
            Ok(process) => {
                *phase = Phase::NetworkStarting;
                commands
                    .entity(entity)
                    .insert(Deadline(Instant::now() + Duration::from_secs(15)));
                commands.spawn((Name::new("VM network"), Network, ChildOf(entity), process));
            }
            Err(error) => {
                *phase = Phase::Failed;
                commands.entity(entity).insert(Failure(error.to_string()));
            }
        }
    }
}

fn launch(
    mut engines: Query<(Entity, &Settings, &Launch, &mut Phase, &Deadline), With<Engine>>,
    mut commands: Commands,
) {
    for (entity, settings, launcher, mut phase, deadline) in &mut engines {
        if *phase != Phase::NetworkStarting {
            continue;
        }
        if !settings.network_socket().exists() {
            if Instant::now() >= deadline.0 {
                *phase = Phase::Stopping;
                commands
                    .entity(entity)
                    .insert(Failure("network helper did not start".into()));
            }
            continue;
        }
        match launcher.machine(settings) {
            Ok(process) => {
                *phase = Phase::DockerStarting;
                commands
                    .entity(entity)
                    .insert(Deadline(Instant::now() + Duration::from_secs(240)));
                commands.spawn((Name::new("Linux VM"), Machine, ChildOf(entity), process));
            }
            Err(error) => {
                *phase = Phase::Stopping;
                commands.entity(entity).insert(Failure(error.to_string()));
            }
        }
    }
}

type ObservedProcesses<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static mut Process,
        Option<&'static Machine>,
        Option<&'static Network>,
    ),
>;

fn observe(
    mut processes: ObservedProcesses,
    mut engines: Query<&mut Phase, With<Engine>>,
    mut commands: Commands,
) {
    for (entity, owner, mut process, machine, network) in &mut processes {
        let result = match process.poll() {
            Ok(None) => continue,
            Ok(Some(status)) => {
                if machine.is_some() && status.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "{} exited: {status}",
                        if network.is_some() {
                            "network helper"
                        } else {
                            "VM"
                        }
                    ))
                }
            }
            Err(error) => Err(error.to_string()),
        };
        commands.entity(entity).despawn();
        let Ok(mut phase) = engines.get_mut(owner.parent()) else {
            continue;
        };
        if matches!(*phase, Phase::Stopping | Phase::Stopped | Phase::Failed) {
            continue;
        }
        *phase = Phase::Stopping;
        if let Err(error) = result {
            commands.entity(owner.parent()).insert(Failure(error));
        }
    }
}

fn retire(
    mut processes: Query<(Entity, &ChildOf, &mut Process, Option<&Machine>), Without<Terminating>>,
    engines: Query<(&Phase, Option<&ForcedShutdown>), With<Engine>>,
    machines: Query<&ChildOf, (With<Machine>, With<Process>)>,
    mut commands: Commands,
) {
    for (entity, owner, mut process, machine) in &mut processes {
        let Ok((phase, forced)) = engines.get(owner.parent()) else {
            continue;
        };
        if !matches!(phase, Phase::Stopping | Phase::Failed) {
            continue;
        }
        if forced.is_none()
            && (machine.is_some()
                || machines
                    .iter()
                    .any(|machine| machine.parent() == owner.parent()))
        {
            continue;
        }
        match process.terminate() {
            Ok(()) => {
                commands.entity(entity).insert(Terminating);
            }
            Err(error) => {
                commands
                    .entity(owner.parent())
                    .insert(Failure(error.to_string()));
            }
        }
    }
}

type CompletingEngines<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static mut Phase,
        Option<&'static Lease>,
        Option<&'static Failure>,
        Option<&'static ResultSink>,
    ),
    With<Engine>,
>;

fn finish(
    mut engines: CompletingEngines,
    processes: Query<&ChildOf, With<Process>>,
    mut exits: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    for (entity, settings, mut phase, lease, failure, sink) in &mut engines {
        if !matches!(*phase, Phase::Stopping | Phase::Failed)
            || processes.iter().any(|owner| owner.parent() == entity)
            || (lease.is_none() && sink.is_none())
        {
            continue;
        }
        let cleanup = if lease.is_some() {
            settings.clean_sockets()
        } else {
            Ok(())
        };
        if let Err(error) = &cleanup {
            commands.entity(entity).insert(Failure(error.to_string()));
        }
        let result = match failure {
            Some(failure) => Err(io::Error::other(failure.0.clone())),
            None => cleanup,
        };
        *phase = if result.is_ok() {
            Phase::Stopped
        } else {
            Phase::Failed
        };
        commands.entity(entity).remove::<ConnectionIssue>();
        if let Some(sink) = sink {
            let exit = if result.is_ok() {
                AppExit::Success
            } else {
                AppExit::error()
            };
            let _ = sink.0.send(result);
            commands.entity(entity).remove::<ResultSink>();
            exits.write(exit);
        }
    }
}

type PublishedEngines<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static Phase,
        Option<&'static Failure>,
        Option<&'static ConnectionIssue>,
        Option<&'static Published>,
        Option<&'static ApplicationOwner>,
    ),
    (With<Engine>, With<Lease>),
>;

fn publish(engines: PublishedEngines, mut commands: Commands) {
    for (entity, settings, phase, failure, issue, previous, owner) in &engines {
        let failure = match failure {
            Some(failure) => Some(failure.0.clone()),
            None => issue.map(|issue| issue.0.clone()),
        };
        let state = State {
            phase: *phase,
            failure,
            owner: owner.map(|owner| owner.0.clone()),
        };
        if previous.is_some_and(|previous| previous.0 == state) {
            continue;
        }
        let bytes = match serde_json::to_vec(&state) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let file = match tempfile::NamedTempFile::new_in(&settings.directory) {
            Ok(file) => file,
            Err(_) => continue,
        };
        if std::fs::write(file.path(), bytes).is_err()
            || file
                .persist(settings.directory.join("engine-state.json"))
                .is_err()
        {
            continue;
        }
        commands.entity(entity).insert(Published(state));
    }
}

type LeasedEngines<'w, 's> = Query<'w, 's, (Entity, &'static Phase), (With<Engine>, With<Lease>)>;

fn release(
    engines: LeasedEngines,
    processes: Query<&ChildOf, With<Process>>,
    mut commands: Commands,
) {
    for (entity, phase) in &engines {
        if matches!(phase, Phase::Stopped | Phase::Failed)
            && !processes.iter().any(|owner| owner.parent() == entity)
        {
            commands.entity(entity).remove::<Lease>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EngineClient;

    struct Fixture {
        app: App,
        engine: Entity,
        _directory: tempfile::TempDir,
    }

    impl Fixture {
        fn new(phase: Phase, deadline: Instant) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let mut settings = Settings::new(directory.path().join("engine"), directory.path());
            settings.validate().unwrap();
            let lease = Lease::acquire(&settings.directory).unwrap();
            let mut app = App::new();
            app.add_plugins(EnginePlugin);
            let engine = app
                .world_mut()
                .spawn((
                    Name::new("Test engine"),
                    Engine,
                    settings,
                    Launch(PathBuf::from("unused")),
                    phase,
                    Deadline(deadline),
                    lease,
                ))
                .id();
            Self {
                app,
                engine,
                _directory: directory,
            }
        }

        fn network(&mut self, program: &str) {
            let mut command = Command::new("sh");
            command.args(["-c", program]);
            self.app.world_mut().spawn((
                Network,
                Name::new("Test network"),
                ChildOf(self.engine),
                Process::spawn(&mut command).unwrap(),
            ));
        }

        fn machine(&mut self, program: &str) {
            let mut command = Command::new("sh");
            command.args(["-c", program]);
            self.app.world_mut().spawn((
                Machine,
                Name::new("Test machine"),
                ChildOf(self.engine),
                Process::spawn(&mut command).unwrap(),
            ));
        }

        fn settle(&mut self) -> Phase {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                self.app.update();
                let phase = *self.app.world().get::<Phase>(self.engine).unwrap();
                if matches!(phase, Phase::Stopped | Phase::Failed) {
                    return phase;
                }
                assert!(Instant::now() < deadline, "engine lifecycle did not settle");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn retired(&mut self) -> bool {
            let mut processes = self.app.world_mut().query::<&Process>();
            processes.iter(self.app.world()).next().is_none()
                && self.app.world().get::<Lease>(self.engine).is_none()
        }
    }

    #[test]
    fn network_exit_fails_startup_and_releases_the_engine_lease() {
        let mut fixture = Fixture::new(
            Phase::NetworkStarting,
            Instant::now() + Duration::from_secs(15),
        );
        fixture.network("exit 7");
        assert_eq!(fixture.settle(), Phase::Failed);
        assert!(fixture.retired());
        assert!(fixture.app.world().get::<Failure>(fixture.engine).is_some());
        let settings = fixture
            .app
            .world()
            .get::<Settings>(fixture.engine)
            .unwrap()
            .clone();
        let status = EngineClient::new(settings).status();
        assert!(!status.running);
        assert_eq!(status.phase, Phase::Failed);
        assert!(status.failure.unwrap().contains("network helper exited"));
    }

    #[test]
    fn network_timeout_reaps_the_owned_process_before_completion() {
        let mut fixture = Fixture::new(
            Phase::NetworkStarting,
            Instant::now() - Duration::from_secs(1),
        );
        fixture.network("exec sleep 60");
        assert_eq!(fixture.settle(), Phase::Failed);
        assert!(fixture.retired());
    }

    #[test]
    fn successful_machine_exit_retires_network_before_stopped() {
        let mut fixture = Fixture::new(
            Phase::DockerStarting,
            Instant::now() + Duration::from_secs(240),
        );
        fixture.network("exec sleep 60");
        fixture.machine("exit 0");
        assert_eq!(fixture.settle(), Phase::Stopped);
        assert!(fixture.retired());
        assert!(fixture.app.world().get::<Failure>(fixture.engine).is_none());
    }
}
