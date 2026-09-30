use std::collections::HashSet;

use bevy::prelude::*;
use ron::value::RawValue;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use vmux_api::InputSchema;

use crate::cli::CliManifest;

#[derive(Component, Clone)]
pub struct FeatureManifest {
    pub pages: Vec<Page>,
    pub commands: Vec<Command>,
    pub tools: Vec<Tool>,
    pub cli: Option<CliManifest>,
    pub mcp_servers: Vec<McpServer>,
    policy: Option<Box<RawValue>>,
    policies: Option<Box<RawValue>>,
}

impl FeatureManifest {
    pub fn parse(source: &str) -> Self {
        let manifest: Self =
            ron::from_str(source).expect("embedded feature manifest must be valid RON");
        manifest.validate();
        manifest
    }

    pub fn policy<T: DeserializeOwned>(&self) -> Result<Option<T>, String> {
        Self::decode(&self.policy)
    }

    pub fn policies<T: DeserializeOwned>(&self) -> Result<Option<T>, String> {
        Self::decode(&self.policies)
    }

    fn decode<T: DeserializeOwned>(value: &Option<Box<RawValue>>) -> Result<Option<T>, String> {
        value
            .as_deref()
            .map(|value| ron::from_str(value.get_ron()))
            .transpose()
            .map_err(|error| error.to_string())
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
            pages: Vec<Page>,
            #[serde(default)]
            commands: Vec<Command>,
            #[serde(default)]
            tools: Vec<Tool>,
            #[serde(default)]
            cli: Option<CliManifest>,
            #[serde(default)]
            mcp_servers: Vec<McpServer>,
            #[serde(default)]
            policy: Option<Box<RawValue>>,
            #[serde(default)]
            policies: Option<Box<RawValue>>,
        }

        let manifest = Manifest::deserialize(deserializer)?;
        Ok(Self {
            pages: manifest.pages,
            commands: manifest.commands,
            tools: manifest.tools,
            cli: manifest.cli,
            mcp_servers: manifest.mcp_servers,
            policy: manifest.policy,
            policies: manifest.policies,
        })
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    #[serde(default)]
    pub name: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub manifest_url: String,
    #[serde(default)]
    pub manifest_title: String,
    #[serde(default)]
    pub asset_host: String,
    #[serde(default)]
    pub owns_subtree: bool,
    #[serde(default)]
    pub title_message_id: String,
    #[serde(default)]
    pub replaces_command: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub command_bar: bool,
    #[serde(default)]
    pub manifest: bool,
    #[serde(default)]
    pub permissions: Vec<String>,
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
        pages: [(
            name: "default",
            url: "vmux://example/",
            title: "Example",
            permissions: ["PageReady"],
        )],
        commands: [(id: "open", label: "Open", group: "File")],
        tools: [(
            name: "open_file",
            description: "Open a file",
            input_schema: (type: Object),
        )],
        cli: Some((
            default: Some("app.open"),
            providers: Some({"example": (enabled: true)}),
        )),
        policy: Some((enabled: true)),
    )"#;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Policy {
        enabled: bool,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Provider {
        enabled: bool,
    }

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
        assert_eq!(manifests[0].pages[0].url, "vmux://example/");
        assert_eq!(manifests[0].pages[0].permissions, ["PageReady"]);
        assert_eq!(manifests[0].commands[0].id, "open");
        assert_eq!(manifests[0].tools[0].name, "open_file");
        assert_eq!(
            manifests[0].cli.as_ref().unwrap().default.as_deref(),
            Some("app.open")
        );
        assert_eq!(
            manifests[0]
                .cli
                .as_ref()
                .unwrap()
                .providers::<std::collections::BTreeMap<String, Provider>>()
                .unwrap()
                .unwrap()["example"],
            Provider { enabled: true }
        );
        assert_eq!(
            manifests[0].policy::<Policy>().unwrap(),
            Some(Policy { enabled: true })
        );
    }
}
