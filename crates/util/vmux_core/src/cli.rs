use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;

use bevy::app::{App, Plugin, Startup};
use bevy_ecs::prelude::*;
use serde::Deserialize;

use crate::host::manifest::FeatureManifestSource;

pub struct CliManifestPlugin;

impl Plugin for CliManifestPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_feature_cli_manifests);
    }
}

fn register_feature_cli_manifests(sources: Query<&FeatureManifestSource>, mut commands: Commands) {
    let mut seen = HashSet::new();
    for source in &sources {
        if !seen.insert(source.as_str()) {
            continue;
        }
        let feature = ron::from_str::<FeatureCliManifest>(source.as_str())
            .expect("embedded feature manifest must contain valid CLI metadata");
        if let Some(manifest) = feature.cli {
            commands.spawn(manifest);
        }
    }
}

#[derive(Deserialize)]
struct FeatureCliManifest {
    #[serde(default)]
    cli: Option<CliManifest>,
}

#[derive(Clone, Component, Debug, Deserialize, PartialEq, Eq)]
pub struct CliManifest {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub commands: Vec<CliCommandManifest>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct CliCommandManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub about: Option<String>,
    #[serde(default)]
    pub arguments: Vec<CliArgumentManifest>,
    #[serde(default)]
    pub commands: Vec<CliCommandManifest>,
    #[serde(default)]
    pub subcommand_required: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct CliArgumentManifest {
    pub id: String,
    #[serde(default)]
    pub long: Option<String>,
    #[serde(default)]
    pub short: Option<char>,
    #[serde(default)]
    pub value_name: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub flag: bool,
    #[serde(default)]
    pub index: Option<usize>,
}

#[derive(Clone, Component, Debug)]
pub struct CliInvocation {
    pub command: String,
    pub arguments: BTreeMap<String, Vec<OsString>>,
}

impl CliInvocation {
    pub fn is(&self, command: &str) -> bool {
        self.command == command
    }

    pub fn value(&self, id: &str) -> Option<&str> {
        self.arguments
            .get(id)
            .and_then(|values| values.first())
            .and_then(|value| value.to_str())
    }

    pub fn value_os(&self, id: &str) -> Option<&std::ffi::OsStr> {
        self.arguments
            .get(id)
            .and_then(|values| values.first())
            .map(OsString::as_os_str)
    }

    pub fn flag(&self, id: &str) -> bool {
        self.value(id) == Some("true")
    }
}

#[derive(Component, Debug)]
pub struct CliResult(pub Result<u8, String>);

impl CliResult {
    pub fn success() -> Self {
        Self(Ok(0))
    }

    pub fn from_io(result: std::io::Result<i32>) -> Self {
        Self(
            result
                .map(|code| u8::try_from(code).unwrap_or(1))
                .map_err(|error| error.to_string()),
        )
    }

    pub fn from_unit(result: std::io::Result<()>) -> Self {
        Self(result.map(|()| 0).map_err(|error| error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_command_manifest() {
        let manifest = ron::from_str::<CliManifest>(
            r#"(
                default: Some("app.open"),
                commands: [(
                    id: "tool",
                    name: "tools",
                    commands: [(
                        id: "tool.import.npm",
                        name: "npm",
                        arguments: [(
                            id: "path",
                            index: Some(1),
                        )],
                    )],
                )],
            )"#,
        )
        .unwrap();

        assert_eq!(manifest.default.as_deref(), Some("app.open"));
        assert_eq!(manifest.commands[0].commands[0].id, "tool.import.npm");
    }

    #[test]
    fn parses_cli_from_feature_manifest() {
        let manifest = ron::from_str::<FeatureCliManifest>(
            r#"(
                cli: (
                    commands: [(id: "tool", name: "tools")],
                ),
                ignored: true,
            )"#,
        )
        .unwrap();

        assert_eq!(manifest.cli.unwrap().commands[0].id, "tool");
    }
}
