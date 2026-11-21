use std::collections::BTreeMap;

use bevy_app::{App, Plugin, Startup};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ToolProviderBinding;

pub(crate) struct McpToolPlugin;

impl Plugin for McpToolPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(host)]
        app.add_plugins(crate::mcp_inventory::McpInventoryPlugin);
        app.add_systems(Startup, spawn);
    }
}

fn spawn(mut commands: Commands) {
    let provider = commands
        .spawn(ToolProviderBinding::new::<crate::Feature>(3))
        .id();
    #[cfg(host)]
    commands.entity(provider).insert((
        crate::observation::Handler,
        crate::observation::ManifestProvider,
    ));
    #[cfg(not(host))]
    let _ = provider;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpManifest {
    #[serde(default)]
    pub servers: BTreeMap<String, McpServerManifest>,
}

impl McpManifest {
    pub(crate) fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerManifest {
    pub transport: McpTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub header_env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer_token_env_var: Option<String>,
}

impl McpServerManifest {
    pub(crate) fn description(&self, sources: &str, managed: bool, conflict: bool) -> String {
        let transport = format!("{:?}", self.transport).to_ascii_lowercase();
        if conflict {
            format!("Conflicting definitions in {sources}")
        } else if managed && sources.is_empty() {
            format!("{transport} · legacy tools.toml")
        } else if managed {
            format!("{transport} · configured in {sources} · duplicated in tools.toml")
        } else {
            format!("{transport} · configured in {sources}")
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
    Sse,
}

#[derive(Component)]
pub struct McpConfigDocument {
    document: serde_json::Value,
    toml: bool,
}

impl McpConfigDocument {
    pub fn parse(source: &str) -> Result<Self, String> {
        if let Ok(document) = serde_json::from_str::<serde_json::Value>(source) {
            return Ok(Self {
                document,
                toml: false,
            });
        }
        let document: toml::Value = toml::from_str(source).map_err(|error| error.to_string())?;
        let document = serde_json::to_value(document).map_err(|error| error.to_string())?;
        Ok(Self {
            document,
            toml: true,
        })
    }

    pub(crate) fn entries(&self) -> Result<Vec<(String, serde_json::Value)>, String> {
        let servers = if self.toml {
            self.document.get("mcp_servers")
        } else {
            self.document
                .get("mcpServers")
                .or_else(|| self.document.get("mcp_servers"))
        };
        let Some(servers) = servers else {
            return Ok(Vec::new());
        };
        if !self.toml && !servers.is_object() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        match servers {
            serde_json::Value::Object(servers) => {
                for (name, value) in servers {
                    entries.push((name.clone(), value.clone()));
                }
            }
            serde_json::Value::Array(servers) => {
                for entry in servers {
                    let name = entry
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .ok_or("MCP server is missing name")?;
                    entries.push((name.to_string(), entry.clone()));
                }
            }
            _ => return Err("mcp_servers must be a table or array".to_string()),
        }
        Ok(entries)
    }
}

impl McpServerManifest {
    pub(crate) fn decode(value: &serde_json::Value) -> Result<Self, String> {
        let object = value.as_object().ok_or("MCP server must be an object")?;
        let command = Self::string_field(object, "command");
        let url = Self::string_field(object, "url");
        let transport = Self::string_field(object, "transport")
            .or_else(|| Self::string_field(object, "type"))
            .map(|transport| match transport.as_str() {
                "sse" => McpTransport::Sse,
                "http" | "streamable-http" => McpTransport::Http,
                _ => McpTransport::Stdio,
            })
            .unwrap_or_else(|| {
                if url.is_some() {
                    McpTransport::Http
                } else {
                    McpTransport::Stdio
                }
            });
        match transport {
            McpTransport::Stdio if command.is_none() => {
                return Err("stdio MCP server is missing command".to_string());
            }
            McpTransport::Http | McpTransport::Sse if url.is_none() => {
                return Err("remote MCP server is missing url".to_string());
            }
            _ => {}
        }
        let args = object
            .get("args")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect();
        let headers = Self::string_map_field(object, "headers")
            .or_else(|| Self::string_map_field(object, "http_headers"))
            .unwrap_or_default();
        Ok(McpServerManifest {
            transport,
            command,
            args,
            env: Self::string_map_field(object, "env").unwrap_or_default(),
            cwd: Self::string_field(object, "cwd"),
            url,
            headers,
            header_env: Self::string_map_field(object, "env_http_headers").unwrap_or_default(),
            bearer_token_env_var: Self::string_field(object, "bearer_token_env_var"),
        })
    }

    fn string_field(
        object: &serde_json::Map<String, serde_json::Value>,
        field: &str,
    ) -> Option<String> {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    }

    fn string_map_field(
        object: &serde_json::Map<String, serde_json::Value>,
        field: &str,
    ) -> Option<BTreeMap<String, String>> {
        object
            .get(field)
            .and_then(serde_json::Value::as_object)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|(name, value)| {
                        value
                            .as_str()
                            .map(|value| (name.clone(), value.to_string()))
                    })
                    .collect()
            })
    }
}
