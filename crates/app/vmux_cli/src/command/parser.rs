use std::collections::BTreeMap;
use std::ffi::OsString;

use clap::builder::{OsStringValueParser, PossibleValuesParser};
use clap::{Arg, ArgAction, ArgMatches, Command};
use vmux_core::cli::{CliArgumentManifest, CliCommandManifest, CliInvocation};

use super::catalog::CliCatalog;

pub(super) struct CliParser<'a> {
    catalog: &'a CliCatalog,
}

impl<'a> CliParser<'a> {
    pub(super) fn new(catalog: &'a CliCatalog) -> Self {
        Self { catalog }
    }

    pub(super) fn parse(
        &self,
        arguments: impl IntoIterator<Item = OsString>,
    ) -> Result<CliInvocation, clap::Error> {
        let mut command = Command::new("vmux")
            .version(env!("CARGO_PKG_VERSION"))
            .about("Vmux command-line interface");
        for child in &self.catalog.commands {
            command = command.subcommand(Self::command(child));
        }
        let matches = command.try_get_matches_from(arguments)?;
        let Some((name, child_matches)) = matches.subcommand() else {
            return Ok(CliInvocation {
                command: self.catalog.default.clone(),
                arguments: BTreeMap::new(),
            });
        };
        let child = self
            .catalog
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

    fn resolve<'b>(
        command: &'b CliCommandManifest,
        matches: &ArgMatches,
        arguments: &mut BTreeMap<String, Vec<OsString>>,
    ) -> &'b CliCommandManifest {
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
