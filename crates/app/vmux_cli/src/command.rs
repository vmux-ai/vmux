use std::collections::BTreeSet;
use std::ffi::OsString;
use std::num::NonZero;
use std::time::Duration;

use bevy_app::{App, AppExit, First, Last, Plugin, PreUpdate};
use bevy_ecs::prelude::*;
use clap::error::ErrorKind;
use vmux_ecs::cli::{CliInvocation, CliResult};
use vmux_ecs::host::manifest::FeatureManifest;

mod catalog;
mod parser;

use catalog::CliCatalog;
use parser::CliParser;

pub struct CliRuntimePlugin;

impl Plugin for CliRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.set_runner(CliRunner::run)
            .add_systems(First, collect_catalog)
            .add_systems(PreUpdate, parse_arguments)
            .add_systems(Last, exit_with_result);
    }
}

struct CliRunner;

impl CliRunner {
    fn run(mut app: App) -> AppExit {
        app.finish();
        app.cleanup();
        app.world_mut()
            .spawn(CliArguments(std::env::args_os().collect()));
        loop {
            app.update();
            if let Some(exit) = app.should_exit() {
                return exit;
            }
            std::thread::park_timeout(Duration::from_millis(1));
        }
    }
}

#[derive(Component)]
struct CliArguments(Vec<OsString>);

fn collect_catalog(
    processes: Query<Entity, (With<CliArguments>, Without<CliCatalog>)>,
    manifests: Query<&FeatureManifest>,
    mut commands: Commands,
) {
    let mut default = None;
    let mut registered = Vec::new();
    for feature in &manifests {
        let Some(manifest) = &feature.cli else {
            continue;
        };
        if let Some(candidate) = &manifest.default {
            assert!(
                default.is_none(),
                "only one default CLI command may be registered"
            );
            default = Some(candidate.clone());
        }
        registered.extend(manifest.commands.iter().cloned());
    }
    registered.sort_by(|left, right| left.name.cmp(&right.name));
    let mut names = BTreeSet::new();
    for command in &registered {
        assert!(
            names.insert(command.name.clone()),
            "duplicate CLI command: {}",
            command.name
        );
    }
    let default = default.expect("one default CLI command must be registered");
    for entity in &processes {
        commands.entity(entity).insert(CliCatalog {
            default: default.clone(),
            commands: registered.clone(),
        });
    }
}

fn parse_arguments(
    processes: Query<(Entity, &CliArguments, &CliCatalog), Without<CliInvocation>>,
    mut exits: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    for (entity, arguments, catalog) in &processes {
        match CliParser::new(catalog).parse(arguments.0.iter().cloned()) {
            Ok(invocation) => {
                commands
                    .entity(entity)
                    .insert(invocation)
                    .remove::<CliArguments>();
            }
            Err(error) => {
                let success = matches!(
                    error.kind(),
                    ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
                );
                let _ = error.print();
                exits.write(if success {
                    AppExit::Success
                } else {
                    AppExit::error()
                });
                commands.entity(entity).despawn();
            }
        }
    }
}

fn exit_with_result(
    results: Query<(Entity, &CliResult), Added<CliResult>>,
    mut exits: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    for (entity, result) in &results {
        let exit = match &result.0 {
            Ok(0) => AppExit::Success,
            Ok(code) => AppExit::Error(NonZero::new(*code).unwrap_or(NonZero::<u8>::MIN)),
            Err(error) => {
                eprintln!("vmux: {error}");
                AppExit::error()
            }
        };
        exits.write(exit);
        commands.entity(entity).despawn();
    }
}
