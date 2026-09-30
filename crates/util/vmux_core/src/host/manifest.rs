use std::collections::HashSet;

use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::InputSchema;

use crate::cli::CliManifest;

#[derive(Component, Clone)]
pub struct FeatureManifest {
    pub commands: Vec<Command>,
    pub tools: Vec<Tool>,
    pub cli: Option<CliManifest>,
    pub mcp_servers: Vec<McpServer>,
}

impl FeatureManifest {
    pub fn parse(source: &str) -> Self {
        let manifest: Self =
            ron::from_str(source).expect("embedded feature manifest must be valid RON");
        manifest.validate();
        manifest
    }

    fn validate(&self) {
        for command in &self.commands {
            if let Some(mcp) = &command.mcp
                && let Some(input_schema) = &mcp.input_schema
            {
                input_schema
                    .validate()
                    .expect("embedded command input schemas must be valid");
            }
        }
        for tool in &self.tools {
            tool.input_schema
                .validate()
                .expect("embedded tool input schemas must be valid");
        }
    }
}

impl<'de> Deserialize<'de> for FeatureManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Manifest {
            #[serde(default)]
            commands: Vec<Command>,
            #[serde(default)]
            tools: Vec<Tool>,
            #[serde(default)]
            cli: Option<CliManifest>,
            #[serde(default)]
            mcp_servers: Vec<McpServer>,
        }

        let manifest = Manifest::deserialize(deserializer)?;
        Ok(Self {
            commands: manifest.commands,
            tools: manifest.tools,
            cli: manifest.cli,
            mcp_servers: manifest.mcp_servers,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub enum ShortcutKind {
    Direct(String),
    Chord(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Shortcut {
    pub shortcut: ShortcutKind,
    pub when: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandMcp {
    pub description: String,
    #[serde(default)]
    pub input_schema: Option<InputSchema>,
    #[serde(default)]
    pub allow_agent: bool,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub id: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub label: String,
    pub group: String,
    #[serde(default)]
    pub accelerator: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default = "native_menu")]
    pub native_menu: bool,
    #[serde(default)]
    pub shortcut_label: Option<String>,
    #[serde(default)]
    pub shortcuts: Vec<Shortcut>,
    #[serde(default)]
    pub mcp: Option<CommandMcp>,
}

fn native_menu() -> bool {
    true
}

#[derive(Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolAvailability {
    #[default]
    Always,
    OutsideAcpSession,
    WithoutAcpTerminals,
}

impl ToolAvailability {
    pub fn allows(self, acp_session: bool, acp_terminals: bool) -> bool {
        match self {
            Self::Always => true,
            Self::OutsideAcpSession => !acp_session,
            Self::WithoutAcpTerminals => !acp_terminals,
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    pub input_schema: InputSchema,
    #[serde(default)]
    pub availability: ToolAvailability,
    #[serde(default)]
    pub shell_aware: bool,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub url: String,
    pub scopes: Vec<String>,
}

#[derive(Resource, Default)]
struct RegisteredFeatureManifests(HashSet<(usize, usize)>);

pub struct FeatureManifestPlugin {
    source: &'static str,
}

impl FeatureManifestPlugin {
    pub const fn new(source: &'static str) -> Self {
        Self { source }
    }
}

impl Plugin for FeatureManifestPlugin {
    fn build(&self, app: &mut App) {
        let source = self.source;
        let mut registered = app
            .world_mut()
            .get_resource_or_init::<RegisteredFeatureManifests>();
        if !registered
            .0
            .insert((source.as_ptr() as usize, source.len()))
        {
            return;
        }
        drop(registered);
        let manifest = FeatureManifest::parse(source);
        app.add_systems(PreStartup, move |mut commands: Commands| {
            commands.spawn((Name::new("Feature manifest"), manifest.clone()));
        });
    }

    fn is_unique(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"(
        commands: [(id: "open", label: "Open", group: "File")],
        tools: [(
            name: "open_file",
            description: "Open a file",
            input_schema: (type: Object),
        )],
        cli: (default: Some("app.open")),
    )"#;

    #[test]
    fn one_feature_entity_owns_all_parsed_sections() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            FeatureManifestPlugin::new(SOURCE),
            FeatureManifestPlugin::new(SOURCE),
        ));
        app.update();

        let manifests = app
            .world_mut()
            .query::<&FeatureManifest>()
            .iter(app.world())
            .collect::<Vec<_>>();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].commands[0].id, "open");
        assert_eq!(manifests[0].tools[0].name, "open_file");
        assert_eq!(
            manifests[0].cli.as_ref().unwrap().default.as_deref(),
            Some("app.open")
        );
    }
}
