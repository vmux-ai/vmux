use std::io;
use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::cli::{CliInvocation, CliResult};
use vmux_ecs::manifest::FeaturePlugin;
use vmux_vm::Settings;

use crate::control::{
    ControlPlugin, Finished, Inspect, LogText, Ownership, ReadLogs, Start, Started, StatusRead,
    Stop, Stopped, Target,
};

pub struct ContainerCliPlugin;

impl Plugin for ContainerCliPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<ControlPlugin>() {
            app.add_plugins(ControlPlugin);
        }
        app.add_observer(started)
            .add_observer(stopped)
            .add_observer(inspected)
            .add_observer(logged)
            .add_systems(Update, request);
    }
}

fn request(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        if !matches!(
            invocation.command.as_str(),
            "container.start" | "container.stop" | "container.status" | "container.logs"
        ) {
            continue;
        }
        let settings = match Configuration::of(invocation) {
            Ok(settings) => settings,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult::from(Err::<i32, _>(error)));
                continue;
            }
        };
        let target = Target {
            reply: entity,
            settings,
            requester: entity,
        };
        match invocation.command.as_str() {
            "container.start" => commands.trigger(Start {
                entity,
                target,
                ownership: Ownership::Independent,
            }),
            "container.stop" => commands.trigger(Stop { entity, target }),
            "container.status" => commands.trigger(Inspect { entity, target }),
            "container.logs" => commands.trigger(ReadLogs { entity, target }),
            _ => unreachable!(),
        };
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

fn started(
    trigger: On<Finished<Started>>,
    invocations: Query<(), With<CliInvocation>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    if !invocations.contains(event.entity) {
        return;
    }
    let result = match &event.result {
        Ok(value) => {
            println!("Docker ready: unix://{}", value.0.socket.display());
            println!(
                "docker --host unix://{} compose up",
                value.0.socket.display()
            );
            Ok(0)
        }
        Err(error) => Err(io::Error::other(error.clone())),
    };
    commands
        .entity(event.entity)
        .insert(CliResult::from(result));
}

fn stopped(
    trigger: On<Finished<Stopped>>,
    invocations: Query<(), With<CliInvocation>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    if !invocations.contains(event.entity) {
        return;
    }
    let result = match &event.result {
        Ok(_) => Ok(0),
        Err(error) => Err(io::Error::other(error.clone())),
    };
    commands
        .entity(event.entity)
        .insert(CliResult::from(result));
}

fn inspected(
    trigger: On<Finished<StatusRead>>,
    invocations: Query<(), With<CliInvocation>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    if !invocations.contains(event.entity) {
        return;
    }
    let result = match &event.result {
        Ok(value) => {
            println!(
                "running={} docker_ready={} socket={} log={}",
                value.0.running,
                value.0.docker_ready,
                value.0.socket.display(),
                value.0.log.display()
            );
            Ok(i32::from(!value.0.docker_ready))
        }
        Err(error) => Err(io::Error::other(error.clone())),
    };
    commands
        .entity(event.entity)
        .insert(CliResult::from(result));
}

fn logged(
    trigger: On<Finished<LogText>>,
    invocations: Query<(), With<CliInvocation>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    if !invocations.contains(event.entity) {
        return;
    }
    let result = match &event.result {
        Ok(value) => {
            print!("{}", value.0);
            Ok(0)
        }
        Err(error) => Err(io::Error::other(error.clone())),
    };
    commands
        .entity(event.entity)
        .insert(CliResult::from(result));
}
