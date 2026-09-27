use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::num::NonZero;
use std::time::Duration;

use bevy_app::{App, AppExit};
use bevy_ecs::prelude::*;
use clap::builder::{OsStringValueParser, PossibleValuesParser};
use clap::error::ErrorKind;
use clap::{Arg, ArgAction, ArgMatches, Command};
use vmux_core::cli::{
    CliAppHandler, CliArgumentManifest, CliCommandManifest, CliInvocation, CliManifest, CliResult,
};

pub async fn run(mut app: App) -> AppExit {
    let catalog = CliCatalog::from_app(&mut app);
    let invocation = match catalog.parse(std::env::args_os()) {
        Ok(invocation) => invocation,
        Err(error) => {
            let success = matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            );
            let _ = error.print();
            return if success {
                AppExit::Success
            } else {
                AppExit::error()
            };
        }
    };
    let app_handler = {
        let world = app.world_mut();
        let mut handlers = world.query::<&CliAppHandler>();
        handlers
            .iter(world)
            .find(|handler| handler.command == invocation.command)
            .copied()
    };
    if let Some(handler) = app_handler {
        return exit((handler.run)(app, invocation).await);
    }

    app.finish();
    app.cleanup();
    let invocation = app.world_mut().spawn(invocation).id();
    loop {
        app.update();
        let result = app
            .world_mut()
            .get_entity_mut(invocation)
            .ok()
            .and_then(|mut entity| entity.take::<CliResult>());
        if let Some(result) = result {
            return exit(result);
        }
        std::thread::park_timeout(Duration::from_millis(1));
    }
}

struct CliCatalog {
    default: String,
    commands: Vec<CliCommandManifest>,
}

impl CliCatalog {
    fn from_app(app: &mut App) -> Self {
        let mut manifests = app.world_mut().query::<&CliManifest>();
        let mut default = None;
        let mut commands = Vec::new();
        for manifest in manifests.iter(app.world()) {
            if let Some(candidate) = &manifest.default {
                assert!(
                    default.is_none(),
                    "only one default CLI command may be registered"
                );
                default = Some(candidate.clone());
            }
            commands.extend(manifest.commands.iter().cloned());
        }
        commands.sort_by(|left, right| left.name.cmp(&right.name));
        let mut names = BTreeSet::new();
        for command in &commands {
            assert!(
                names.insert(command.name.clone()),
                "duplicate CLI command: {}",
                command.name
            );
        }
        Self {
            default: default.expect("one default CLI command must be registered"),
            commands,
        }
    }

    fn parse(
        &self,
        arguments: impl IntoIterator<Item = OsString>,
    ) -> Result<CliInvocation, clap::Error> {
        let mut command = Command::new("vmux")
            .version(env!("CARGO_PKG_VERSION"))
            .about("Vmux command-line interface");
        for child in &self.commands {
            command = command.subcommand(Self::command(child));
        }
        let matches = command.try_get_matches_from(arguments)?;
        let Some((name, child_matches)) = matches.subcommand() else {
            return Ok(CliInvocation {
                command: self.default.clone(),
                arguments: BTreeMap::new(),
            });
        };
        let child = self
            .commands
            .iter()
            .find(|command| command.name == name)
            .expect("Clap returned an unregistered command");
        let mut arguments = BTreeMap::new();
        let command = Self::resolve(child, child_matches, &mut arguments);
        Ok(CliInvocation {
            command: command.id.clone(),
            arguments,
        })
    }

    fn command(manifest: &CliCommandManifest) -> Command {
        let mut command = Command::new(manifest.name.clone());
        if let Some(about) = &manifest.about {
            command = command.about(about.clone());
        }
        for argument in &manifest.arguments {
            command = command.arg(Self::argument(argument));
        }
        for child in &manifest.commands {
            command = command.subcommand(Self::command(child));
        }
        if manifest.subcommand_required {
            command = command.subcommand_required(true);
        }
        command
    }

    fn argument(manifest: &CliArgumentManifest) -> Arg {
        let mut argument = Arg::new(manifest.id.clone());
        if let Some(long) = &manifest.long {
            argument = argument.long(long.clone());
        }
        if let Some(short) = manifest.short {
            argument = argument.short(short);
        }
        if let Some(value_name) = &manifest.value_name {
            argument = argument.value_name(value_name.clone());
        }
        if let Some(index) = manifest.index {
            argument = argument.index(index);
        }
        if manifest.required {
            argument = argument.required(true);
        }
        if let Some(default) = &manifest.default {
            argument = argument.default_value(default.clone());
        }
        if !manifest.values.is_empty() {
            argument = argument.value_parser(PossibleValuesParser::new(manifest.values.clone()));
        } else if !manifest.flag {
            argument = argument.value_parser(OsStringValueParser::new());
        }
        if manifest.flag {
            argument = argument.action(ArgAction::SetTrue);
        }
        argument
    }

    fn resolve<'a>(
        command: &'a CliCommandManifest,
        matches: &ArgMatches,
        arguments: &mut BTreeMap<String, Vec<OsString>>,
    ) -> &'a CliCommandManifest {
        Self::collect_arguments(&command.arguments, matches, arguments);
        let Some((name, child_matches)) = matches.subcommand() else {
            return command;
        };
        let child = command
            .commands
            .iter()
            .find(|child| child.name == name)
            .expect("Clap returned an unregistered nested command");
        Self::resolve(child, child_matches, arguments)
    }

    fn collect_arguments(
        manifests: &[CliArgumentManifest],
        matches: &ArgMatches,
        arguments: &mut BTreeMap<String, Vec<OsString>>,
    ) {
        for manifest in manifests {
            if manifest.flag {
                if matches.get_flag(&manifest.id) {
                    arguments.insert(manifest.id.clone(), vec![OsString::from("true")]);
                }
                continue;
            }
            if manifest.values.is_empty() {
                let Some(values) = matches.get_many::<OsString>(&manifest.id) else {
                    continue;
                };
                arguments.insert(manifest.id.clone(), values.cloned().collect());
            } else {
                let Some(values) = matches.get_many::<String>(&manifest.id) else {
                    continue;
                };
                arguments.insert(manifest.id.clone(), values.map(OsString::from).collect());
            }
        }
    }
}

fn exit(result: CliResult) -> AppExit {
    match result.0 {
        Ok(0) => AppExit::Success,
        Ok(code) => AppExit::Error(NonZero::new(code).unwrap_or(NonZero::<u8>::MIN)),
        Err(error) => {
            eprintln!("vmux: {error}");
            AppExit::error()
        }
    }
}
