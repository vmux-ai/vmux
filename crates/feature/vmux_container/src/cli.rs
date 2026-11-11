use std::io;
use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::cli::{CliInvocation, CliResult};
use vmux_ecs::manifest::FeaturePlugin;
use vmux_vm::Settings;

use crate::control::{Command, ControlPlugin, Finished, Reply, Request};

pub struct ContainerCliPlugin;

impl Plugin for ContainerCliPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<ControlPlugin>() {
            app.add_plugins(ControlPlugin);
        }
        app.add_observer(finish).add_systems(Update, request);
    }
}

fn request(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        let command = match invocation.command.as_str() {
            "container.start" => Command::Start,
            "container.stop" => Command::Stop,
            "container.status" => Command::Status,
            "container.logs" => Command::Logs,
            _ => continue,
        };
        match Configuration::of(invocation) {
            Ok(settings) => commands.trigger(Request {
                entity,
                reply: entity,
                command,
                settings,
            }),
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult::from(Err::<i32, _>(error)));
            }
        }
    }
}

struct Configuration;

impl Configuration {
    fn of(invocation: &CliInvocation) -> io::Result<Settings> {
        let directory = match invocation.value("data") {
            Some(path) => PathBuf::from(path),
            None => vmux_profile::ProfilePaths::current()
                .shared_data()
                .join("containers"),
        };
        let saved = directory.join("settings.json");
        let mut settings = if saved.exists() {
            Settings::load(&saved)?
        } else {
            Settings::new(directory, std::env::current_dir()?)
        };
        if let Some(shared) = invocation.value("shared") {
            settings.shared_directory = PathBuf::from(shared);
        }
        if let Some(cpus) = invocation.value("cpus") {
            settings.cpus = cpus.parse().map_err(io::Error::other)?;
        }
        if let Some(memory) = invocation.value("memory") {
            settings.memory_mib = memory.parse().map_err(io::Error::other)?;
        }
        Ok(settings)
    }
}

fn finish(
    trigger: On<Finished>,
    invocations: Query<(), With<CliInvocation>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    if !invocations.contains(event.entity) {
        return;
    }
    let result = match &event.result {
        Ok(Reply::Started(status)) => {
            println!("Docker ready: unix://{}", status.socket.display());
            println!(
                "docker --host unix://{} compose up",
                status.socket.display()
            );
            Ok(0)
        }
        Ok(Reply::Stopped(_)) => Ok(0),
        Ok(Reply::Configured(_) | Reply::Changed) => Ok(0),
        Ok(Reply::Status(status)) => {
            println!(
                "running={} docker_ready={} socket={} log={}",
                status.running,
                status.docker_ready,
                status.socket.display(),
                status.log.display()
            );
            Ok(i32::from(!status.docker_ready))
        }
        Ok(Reply::Logs(logs)) => {
            print!("{logs}");
            Ok(0)
        }
        Err(error) => Err(io::Error::other(error.clone())),
    };
    commands
        .entity(event.entity)
        .insert(CliResult::from(result));
}
