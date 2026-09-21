use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::manifest::{
    expand_user_path, home_dir, load_manifest, load_manifest_from, manifest_path,
    migrate_legacy_storage, write_manifest, write_manifest_to,
};

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    pub fn resolved_headers(&self) -> BTreeMap<String, String> {
        let mut headers = self.headers.clone();
        for (name, variable) in &self.header_env {
            if let Ok(value) = std::env::var(variable) {
                headers.insert(name.clone(), value);
            }
        }
        if let Some(variable) = &self.bearer_token_env_var
            && let Ok(value) = std::env::var(variable)
        {
            headers.insert("Authorization".to_string(), format!("Bearer {value}"));
        }
        headers
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredMcpServer {
    pub definition: McpServerManifest,
    pub sources: Vec<PathBuf>,
    pub conflict: bool,
}

pub fn default_mcp_config_paths() -> Vec<PathBuf> {
    let home = home_dir();
    [
        home.join(".codex/config.toml"),
        home.join(".claude.json"),
        home.join(".vibe/config.toml"),
        home.join(".mcp.json"),
    ]
    .into_iter()
    .filter(|path| path.is_file())
    .collect()
}

pub fn discover_mcp_servers() -> (BTreeMap<String, DiscoveredMcpServer>, Vec<String>) {
    let mut discovered = BTreeMap::<String, DiscoveredMcpServer>::new();
    let mut errors = Vec::new();
    for path in default_mcp_config_paths() {
        match parse_mcp_config_file(&path) {
            Ok(servers) => {
                for (name, definition) in servers {
                    match discovered.get_mut(&name) {
                        Some(existing) => {
                            existing.conflict |= existing.definition != definition;
                            existing.sources.push(path.clone());
                        }
                        None => {
                            discovered.insert(
                                name,
                                DiscoveredMcpServer {
                                    definition,
                                    sources: vec![path.clone()],
                                    conflict: false,
                                },
                            );
                        }
                    }
                }
            }
            Err(error) => errors.push(format!("{}: {error}", path.display())),
        }
    }
    (discovered, errors)
}

pub fn import_mcp_config(path: &Path) -> Result<usize, String> {
    migrate_legacy_storage()?;
    import_mcp_config_to(path, &manifest_path())
}

pub fn import_mcp_config_to(path: &Path, manifest_path: &Path) -> Result<usize, String> {
    let path = expand_user_path(path)?;
    let servers = parse_mcp_config_file(&path)?;
    if servers.is_empty() {
        return Err(format!("no MCP servers found in {}", path.display()));
    }
    let mut manifest = load_manifest_from(manifest_path)?;
    let mut imported = 0;
    for (name, definition) in servers {
        if name == "vmux" {
            continue;
        }
        imported += usize::from(manifest.mcp.servers.get(&name) != Some(&definition));
        manifest.mcp.servers.insert(name, definition);
    }
    write_manifest_to(manifest_path, &manifest)?;
    Ok(imported)
}

pub fn import_default_mcp_configs() -> Result<usize, String> {
    let (discovered, errors) = discover_mcp_servers();
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    let conflicts = discovered
        .iter()
        .filter(|(_, server)| server.conflict)
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    if !conflicts.is_empty() {
        return Err(format!(
            "conflicting MCP definitions: {}",
            conflicts.join(", ")
        ));
    }
    let mut manifest = load_manifest()?;
    let mut imported = 0;
    for (name, server) in discovered {
        if name == "vmux" {
            continue;
        }
        imported += usize::from(manifest.mcp.servers.get(&name) != Some(&server.definition));
        manifest.mcp.servers.insert(name, server.definition);
    }
    write_manifest(&manifest)?;
    Ok(imported)
}

pub fn import_discovered_mcp_server(name: &str) -> Result<(), String> {
    let (discovered, errors) = discover_mcp_servers();
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    let server = discovered
        .get(name)
        .ok_or_else(|| format!("MCP server not found: {name}"))?;
    if server.conflict {
        return Err(format!(
            "MCP server {name} has conflicting definitions; import an explicit config path"
        ));
    }
    let mut manifest = load_manifest()?;
    manifest
        .mcp
        .servers
        .insert(name.to_string(), server.definition.clone());
    write_manifest(&manifest)
}

pub fn parse_mcp_config_file(path: &Path) -> Result<BTreeMap<String, McpServerManifest>, String> {
    let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    parse_mcp_config(&source)
}

pub fn parse_mcp_config(source: &str) -> Result<BTreeMap<String, McpServerManifest>, String> {
    if let Ok(document) = serde_json::from_str::<serde_json::Value>(source) {
        return parse_json_mcp_document(&document);
    }
    let document: toml::Value = toml::from_str(source).map_err(|error| error.to_string())?;
    parse_toml_mcp_document(&document)
}

fn parse_json_mcp_document(
    document: &serde_json::Value,
) -> Result<BTreeMap<String, McpServerManifest>, String> {
    let Some(servers) = document
        .get("mcpServers")
        .or_else(|| document.get("mcp_servers"))
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(BTreeMap::new());
    };
    let mut parsed = BTreeMap::new();
    for (name, value) in servers {
        if name == "vmux"
            || value.get("enabled").and_then(serde_json::Value::as_bool) == Some(false)
        {
            continue;
        }
        parsed.insert(name.clone(), parse_json_mcp_server(value)?);
    }
    Ok(parsed)
}

fn parse_toml_mcp_document(
    document: &toml::Value,
) -> Result<BTreeMap<String, McpServerManifest>, String> {
    let Some(servers) = document.get("mcp_servers") else {
        return Ok(BTreeMap::new());
    };
    let mut parsed = BTreeMap::new();
    match servers {
        toml::Value::Table(table) => {
            for (name, value) in table {
                if name == "vmux"
                    || value.get("enabled").and_then(toml::Value::as_bool) == Some(false)
                {
                    continue;
                }
                let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
                parsed.insert(name.clone(), parse_json_mcp_server(&value)?);
            }
        }
        toml::Value::Array(entries) => {
            for entry in entries {
                let name = entry
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .ok_or("MCP server is missing name")?;
                if name == "vmux"
                    || entry.get("enabled").and_then(toml::Value::as_bool) == Some(false)
                {
                    continue;
                }
                let value = serde_json::to_value(entry).map_err(|error| error.to_string())?;
                parsed.insert(name.to_string(), parse_json_mcp_server(&value)?);
            }
        }
        _ => return Err("mcp_servers must be a table or array".to_string()),
    }
    Ok(parsed)
}

fn parse_json_mcp_server(value: &serde_json::Value) -> Result<McpServerManifest, String> {
    let object = value.as_object().ok_or("MCP server must be an object")?;
    let command = string_field(object, "command");
    let url = string_field(object, "url");
    let transport = string_field(object, "transport")
        .or_else(|| string_field(object, "type"))
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
    let headers = string_map_field(object, "headers")
        .or_else(|| string_map_field(object, "http_headers"))
        .unwrap_or_default();
    Ok(McpServerManifest {
        transport,
        command,
        args,
        env: string_map_field(object, "env").unwrap_or_default(),
        cwd: string_field(object, "cwd"),
        url,
        headers,
        header_env: string_map_field(object, "env_http_headers").unwrap_or_default(),
        bearer_token_env_var: string_field(object, "bearer_token_env_var"),
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
