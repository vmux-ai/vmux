use std::io;
use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use crossbeam_channel::{Receiver, TryRecvError};
use vmux_ecs::cli::{CliInvocation, CliResult, CliWake};
use vmux_ecs::manifest::{FeatureManifestSource, FeaturePlugin};
use vmux_vm::{Engine, Settings};

pub struct ContainerCliPlugin;

impl Plugin for ContainerCliPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<Feature>::default())
            .add_systems(Update, (request, poll).chain());
    }
}

struct Feature;

impl FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[derive(Component)]
struct PendingOperation(Receiver<io::Result<i32>>);

struct Request {
    command: String,
    directory: PathBuf,
    shared: Option<PathBuf>,
    cpus: Option<u8>,
    memory: Option<u32>,
}

impl Request {
    fn of(invocation: &CliInvocation) -> io::Result<Option<Self>> {
        if !invocation.command.starts_with("container.") {
            return Ok(None);
        }
        let directory = match invocation.value("data") {
            Some(path) => PathBuf::from(path),
            None => vmux_profile::ProfilePaths::current()
                .shared_data()
                .join("containers"),
        };
        let cpus = match invocation.value("cpus") {
            Some(value) => Some(value.parse().map_err(io::Error::other)?),
            None => None,
        };
        let memory = match invocation.value("memory") {
            Some(value) => Some(value.parse().map_err(io::Error::other)?),
            None => None,
        };
        Ok(Some(Self {
            command: invocation.command.clone(),
            directory,
            shared: invocation.value("shared").map(PathBuf::from),
            cpus,
            memory,
        }))
    }

    fn run(self) -> io::Result<i32> {
        let saved = self.directory.join("settings.json");
        let mut settings = if saved.exists() {
            Settings::load(&saved)?
        } else {
            Settings::new(&self.directory, std::env::current_dir()?)
        };
        if let Some(shared) = self.shared {
            settings.shared_directory = shared;
        }
        if let Some(cpus) = self.cpus {
            settings.cpus = cpus;
        }
        if let Some(memory) = self.memory {
            settings.memory_mib = memory;
        }
        let mut engine = Engine::new(settings);
        match self.command.as_str() {
            "container.start" => {
                let helper = std::env::current_exe()?.with_file_name("vmux_vm");
                let status = engine.start(&helper)?;
                println!("Docker ready: unix://{}", status.socket.display());
                println!(
                    "docker --host unix://{} compose up",
                    status.socket.display()
                );
                Ok(0)
            }
            "container.stop" => {
                engine.stop()?;
                Ok(0)
            }
            "container.status" => {
                let status = engine.status();
                println!(
                    "running={} docker_ready={} socket={} log={}",
                    status.running,
                    status.docker_ready,
                    status.socket.display(),
                    status.log.display()
                );
                Ok(i32::from(!status.docker_ready))
            }
            "container.logs" => {
                let status = engine.status();
                for path in [status.log, self.directory.join("guest.log")] {
                    if path.exists() {
                        print!("{}", std::fs::read_to_string(path)?);
                    }
                }
                Ok(0)
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown container command",
            )),
        }
    }
}

fn request(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    wake: Res<CliWake>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        let request = match Request::of(invocation) {
            Ok(Some(request)) => request,
            Ok(None) => continue,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult::from(Err::<i32, _>(error)));
                continue;
            }
        };
        let wake = wake.clone();
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let spawned = std::thread::Builder::new()
            .name("vmux-container-command".into())
            .spawn(move || {
                let _ = sender.send(request.run());
                wake.wake();
            });
        match spawned {
            Ok(_) => {
                commands.entity(entity).insert(PendingOperation(receiver));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult::from(Err::<i32, _>(error)));
            }
        }
    }
}

fn poll(operations: Query<(Entity, &PendingOperation)>, mut commands: Commands) {
    for (entity, operation) in &operations {
        let result = match operation.0.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => continue,
            Err(TryRecvError::Disconnected) => {
                Err(io::Error::other("container command worker stopped"))
            }
        };
        commands
            .entity(entity)
            .remove::<PendingOperation>()
            .insert(CliResult::from(result));
    }
}
