use std::collections::BTreeSet;
use std::ffi::OsString;
use std::num::NonZero;
use std::time::Duration;

use bevy_app::{App, AppExit, First, Last, Plugin, PreUpdate};
use bevy_ecs::prelude::*;
use clap::error::ErrorKind;
use vmux_ecs::cli::{CliInvocation, CliResult, CliSet, CliWake};
use vmux_ecs::manifest::FeatureManifest;

use catalog::CliCatalog;
use parser::CliParser;

mod catalog;
mod parser;

pub struct CliRuntimePlugin;

impl Plugin for CliRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(First, collect_catalog)
            .add_systems(PreUpdate, parse_arguments.in_set(CliSet::Parse))
            .add_systems(Last, exit_with_result)
            .set_runner(CliRunner::run);
    }
}

struct CliRunner;

impl CliRunner {
    fn run(mut app: App) -> AppExit {
        app.finish();
        app.cleanup();
        app.world_mut().insert_resource(CliWake::current());
        app.world_mut()
            .spawn(CliArguments(std::env::args_os().collect()));
        loop {
            app.update();
            if let Some(exit) = app.should_exit() {
                return exit;
            }
            std::thread::park_timeout(Duration::from_secs(1));
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
    let default = default.unwrap_or_else(|| "dx.help".into());
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
                let code = error.exit_code() as u8;
                let _ = error.print();
                exits.write(if success {
                    AppExit::Success
                } else {
                    AppExit::Error(NonZero::new(code).unwrap_or(NonZero::<u8>::MIN))
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Parsed {
        app: App,
        entity: Entity,
    }

    impl Parsed {
        fn new(arguments: &[&str]) -> Self {
            let mut app = App::new();
            app.add_plugins(CliRuntimePlugin);
            app.world_mut().spawn(FeatureManifest::parse(include_str!(
                "../../../feature/vmux_dx/src/feature.ron"
            )));
            let entity = app
                .world_mut()
                .spawn(CliArguments(arguments.iter().map(OsString::from).collect()))
                .id();
            app.update();
            Self { app, entity }
        }

        fn invocation(&self) -> &CliInvocation {
            self.app
                .world()
                .get::<CliInvocation>(self.entity)
                .expect("valid CLI input must become canonical intent")
        }
    }

    #[test]
    fn desktop_spelling_preserves_the_positional_branch_and_launch_options() {
        let parsed = Parsed::new(&[
            "vmux",
            "desktop",
            "onboarding",
            "--user-profile",
            "fresh",
            "-p",
            "local",
        ]);
        let invocation = parsed.invocation();
        assert_eq!(invocation.command, "dx.desktop");
        assert_eq!(invocation.value("branch"), Some("onboarding"));
        assert_eq!(invocation.value("profile"), Some("local"));
        assert_eq!(invocation.value("user-profile"), Some("fresh"));
        assert_eq!(invocation.value("platform"), None);
    }

    #[test]
    fn desktop_omits_a_branch_instead_of_implicitly_selecting_main() {
        let current = Parsed::new(&["vmux", "desktop"]);
        let explicit = Parsed::new(&["vmux", "desktop", "main"]);
        assert_eq!(current.invocation().value("branch"), None);
        assert_eq!(explicit.invocation().value("branch"), Some("main"));
        assert_eq!(current.invocation().command, explicit.invocation().command);
    }

    #[test]
    fn desktop_does_not_accept_a_different_platform() {
        let parsed = Parsed::new(&["vmux", "desktop", "--platform", "ios"]);
        assert!(
            parsed
                .app
                .world()
                .get::<CliInvocation>(parsed.entity)
                .is_none()
        );
        let existing = Parsed::new(&["vmux", "run", "--platform", "ios"]);
        assert_eq!(existing.invocation().value("platform"), Some("ios"));
    }
}
