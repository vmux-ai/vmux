use std::collections::{BTreeMap, BTreeSet};

use bevy_ecs::prelude::Component;
use ron::value::RawValue;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use vmux_api::JsonSchema;

use crate::CliManifest;

#[derive(Component, Clone)]
pub struct FeatureManifest {
    pub pages: Vec<Page>,
    pub commands: Vec<Command>,
    pub command_bar: Vec<CommandBarEntry>,
    pub search_engines: Vec<SearchEngineManifest>,
    pub tool_providers: Vec<ToolProviderManifest>,
    pub tools: Vec<Tool>,
    pub cli: Option<CliManifest>,
    pub mcp_servers: Vec<McpServer>,
    pub auth_providers: Vec<AuthProvider>,
    settings: Option<Box<RawValue>>,
    policy: Option<Box<RawValue>>,
    policies: Option<Box<RawValue>>,
}

impl FeatureManifest {
    pub fn of<M: FeatureManifestSource>() -> Self {
        Self::parse(M::SOURCE)
    }

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

    pub fn settings<T: DeserializeOwned>(&self) -> Result<Option<T>, String> {
        Self::decode(&self.settings)
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
        let mut providers = BTreeSet::new();
        for provider in &self.auth_providers {
            provider
                .validate()
                .expect("embedded auth providers must be valid");
            assert!(
                providers.insert(provider.id.as_str()),
                "embedded auth provider IDs must be unique"
            );
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
            command_bar: Vec<CommandBarEntry>,
            #[serde(default)]
            search_engines: Vec<SearchEngineManifest>,
            #[serde(default)]
            tool_providers: Vec<ToolProviderManifest>,
            #[serde(default)]
            tools: Vec<Tool>,
            #[serde(default)]
            cli: Option<CliManifest>,
            #[serde(default)]
            mcp_servers: Vec<McpServer>,
            #[serde(default)]
            auth_providers: Vec<AuthProvider>,
            #[serde(default)]
            settings: Option<Box<RawValue>>,
            #[serde(default)]
            policy: Option<Box<RawValue>>,
            #[serde(default)]
            policies: Option<Box<RawValue>>,
        }

        let manifest = Manifest::deserialize(deserializer)?;
        Ok(Self {
            pages: manifest.pages,
            commands: manifest.commands,
            command_bar: manifest.command_bar,
            search_engines: manifest.search_engines,
            tool_providers: manifest.tool_providers,
            tools: manifest.tools,
            cli: manifest.cli,
            mcp_servers: manifest.mcp_servers,
            auth_providers: manifest.auth_providers,
            settings: manifest.settings,
            policy: manifest.policy,
            policies: manifest.policies,
        })
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    #[serde(default, rename = "name")]
    _name: String,
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
    pub startup: bool,
    #[serde(default)]
    pub reports_title: bool,
    #[serde(default)]
    pub bookmark: Option<i32>,
    #[serde(default)]
    pub manifest: bool,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub placement: Option<PagePlacement>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PagePlacement {
    pub group: String,
    #[serde(default)]
    pub auxiliary: bool,
    #[serde(default)]
    pub refresh: bool,
    #[serde(default)]
    pub reuse: PageReuse,
    #[serde(default)]
    pub split: Option<PageSplitPreference>,
}

#[derive(Clone, Default, Deserialize)]
pub enum PageReuse {
    #[default]
    Exact,
    Group,
    Route,
    Fragmentless,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSplitPreference {
    pub anchor_group: String,
    pub allowed_groups: Vec<String>,
    pub axis: PageSplitAxis,
}

#[derive(Clone, Deserialize)]
pub enum PageSplitAxis {
    Row,
    Column,
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
    pub input_schema: Option<JsonSchema>,
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
    pub toolbar: Option<CommandToolbar>,
    #[serde(default)]
    pub mcp: Option<CommandMcp>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandToolbar {
    pub icon: vmux_api::BuiltinIcon,
    #[serde(default)]
    pub rank: i32,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandBarEntry {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub title_message_id: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub subtitle_message_id: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub leading: String,
    #[serde(default)]
    pub trailing: String,
    #[serde(default)]
    pub badge: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub query_prefix: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub rank: i32,
    #[serde(default = "enabled")]
    pub close: bool,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchEngineManifest {
    pub id: String,
    pub name: String,
    pub hosts: Vec<String>,
    pub query_url: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolProviderManifest {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub title_message_id: String,
    #[serde(default)]
    pub route_title: String,
    #[serde(default)]
    pub route_title_message_id: String,
    #[serde(default)]
    pub short_label: String,
    #[serde(default)]
    pub route: String,
    #[serde(default)]
    pub rank: i32,
    #[serde(default)]
    pub thumbnails: bool,
    #[serde(default)]
    pub apply: bool,
    #[serde(default)]
    pub brewfile: bool,
}

fn enabled() -> bool {
    true
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
    pub input_schema: JsonSchema,
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

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthProvider {
    pub id: String,
    pub name: String,
    pub flow: AuthFlow,
    #[serde(default)]
    pub revocation_endpoint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub enum AuthFlow {
    DeviceCode(DeviceCodeFlow),
    AuthorizationCodePkce(AuthorizationCodePkceFlow),
    OpenIdConnect(OpenIdConnectFlow),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceCodeFlow {
    pub client_id: String,
    #[serde(default)]
    pub client_id_env: String,
    pub device_authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationCodePkceFlow {
    pub client_id: String,
    #[serde(default)]
    pub client_id_env: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenIdConnectFlow {
    pub client_id: String,
    #[serde(default)]
    pub client_id_env: String,
    pub issuer: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

impl AuthProvider {
    pub fn configure_client_id(&mut self, lookup: impl Fn(&str) -> Option<String>) {
        let (client_id, environment) = match &mut self.flow {
            AuthFlow::DeviceCode(flow) => (&mut flow.client_id, &flow.client_id_env),
            AuthFlow::AuthorizationCodePkce(flow) => (&mut flow.client_id, &flow.client_id_env),
            AuthFlow::OpenIdConnect(flow) => (&mut flow.client_id, &flow.client_id_env),
        };
        if !client_id.trim().is_empty() || environment.trim().is_empty() {
            return;
        }
        if let Some(value) = lookup(environment).filter(|value| !value.trim().is_empty()) {
            *client_id = value;
        }
    }

    pub fn configured(&self) -> bool {
        match &self.flow {
            AuthFlow::DeviceCode(flow) => !flow.client_id.trim().is_empty(),
            AuthFlow::AuthorizationCodePkce(flow) => !flow.client_id.trim().is_empty(),
            AuthFlow::OpenIdConnect(flow) => !flow.client_id.trim().is_empty(),
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err("auth provider ID and name are required".to_string());
        }
        match &self.flow {
            AuthFlow::DeviceCode(flow) => {
                Self::validate_client(&flow.client_id, &flow.client_id_env)?;
                Self::validate_https(
                    &flow.device_authorization_endpoint,
                    "device authorization endpoint",
                )?;
                Self::validate_https(&flow.token_endpoint, "token endpoint")?;
            }
            AuthFlow::AuthorizationCodePkce(flow) => {
                Self::validate_client(&flow.client_id, &flow.client_id_env)?;
                Self::validate_https(&flow.authorization_endpoint, "authorization endpoint")?;
                Self::validate_https(&flow.token_endpoint, "token endpoint")?;
            }
            AuthFlow::OpenIdConnect(flow) => {
                Self::validate_client(&flow.client_id, &flow.client_id_env)?;
                Self::validate_https(&flow.issuer, "OpenID Connect issuer")?;
            }
        }
        if let Some(endpoint) = &self.revocation_endpoint {
            Self::validate_https(endpoint, "revocation endpoint")?;
        }
        Ok(())
    }

    fn validate_client(client_id: &str, environment: &str) -> Result<(), String> {
        if client_id.trim().is_empty() && environment.trim().is_empty() {
            return Err("auth provider client ID or environment variable is required".to_string());
        }
        Ok(())
    }

    fn validate_https(value: &str, label: &str) -> Result<(), String> {
        let url = url::Url::parse(value)
            .map_err(|error| format!("invalid auth provider {label}: {error}"))?;
        if url.scheme() != "https" {
            return Err(format!("auth provider {label} must use https"));
        }
        Ok(())
    }
}

pub trait FeatureManifestSource: Send + Sync + 'static {
    const SOURCE: &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"(
        pages: [(
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
        settings: Some((enabled: true)),
        auth_providers: [(
            id: "example",
            name: "Example",
            flow: DeviceCode((
                client_id: "client-id",
                device_authorization_endpoint: "https://example.com/device",
                token_endpoint: "https://example.com/token",
                scopes: ["read"],
                headers: {"Accept": "application/json"},
            )),
        )],
    )"#;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Policy {
        enabled: bool,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Provider {
        enabled: bool,
    }

    struct Feature;
    impl FeatureManifestSource for Feature {
        const SOURCE: &'static str = SOURCE;
    }

    #[test]
    fn parses_all_typed_sections() {
        let manifest = FeatureManifest::of::<Feature>();

        assert_eq!(manifest.pages[0].url, "vmux://example/");
        assert_eq!(manifest.pages[0].permissions, ["PageReady"]);
        assert_eq!(manifest.commands[0].id, "open");
        assert_eq!(manifest.tools[0].name, "open_file");
        assert_eq!(
            manifest.cli.as_ref().unwrap().default.as_deref(),
            Some("app.open")
        );
        assert_eq!(
            manifest
                .cli
                .as_ref()
                .unwrap()
                .providers::<std::collections::BTreeMap<String, Provider>>()
                .unwrap()
                .unwrap()["example"],
            Provider { enabled: true }
        );
        assert_eq!(
            manifest.policy::<Policy>().unwrap(),
            Some(Policy { enabled: true })
        );
        assert_eq!(
            manifest.settings::<Policy>().unwrap(),
            Some(Policy { enabled: true })
        );
        assert_eq!(manifest.auth_providers[0].id, "example");
        assert!(matches!(
            &manifest.auth_providers[0].flow,
            AuthFlow::DeviceCode(flow) if flow.client_id == "client-id"
        ));
    }

    #[test]
    fn auth_provider_client_id_can_be_supplied_at_runtime() {
        let mut manifest = FeatureManifest::parse(
            r#"(
                auth_providers: [(
                    id: "example",
                    name: "Example",
                    flow: DeviceCode((
                        client_id: "",
                        client_id_env: "EXAMPLE_CLIENT_ID",
                        device_authorization_endpoint: "https://example.com/device",
                        token_endpoint: "https://example.com/token",
                    )),
                )],
            )"#,
        );

        manifest.auth_providers[0]
            .configure_client_id(|name| (name == "EXAMPLE_CLIENT_ID").then(|| "client".into()));

        assert!(manifest.auth_providers[0].configured());
    }
}
