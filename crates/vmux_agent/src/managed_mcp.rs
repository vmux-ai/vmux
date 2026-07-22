//! Tools-managed MCP server projection for CLI and ACP agents.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use vmux_core::profile::tools::{McpServerManifest, McpTransport};
use vmux_service::protocol::{ManagedMcpServer, ManagedMcpTransport};

/// Loads Tools-owned MCP servers without blocking agent startup on a malformed manifest.
#[cfg(not(test))]
pub fn load() -> BTreeMap<String, McpServerManifest> {
    match vmux_core::profile::tools::load_manifest() {
        Ok(manifest) => manifest.mcp.servers,
        Err(error) => {
            bevy::log::warn!("managed MCP servers unavailable: {error}");
            BTreeMap::new()
        }
    }
}

#[cfg(test)]
pub fn load() -> BTreeMap<String, McpServerManifest> {
    BTreeMap::new()
}

/// Builds the wire representation passed to ACP agents at session creation.
pub fn acp_servers() -> Vec<ManagedMcpServer> {
    load()
        .into_iter()
        .map(|(name, server)| acp_server(name, server))
        .collect()
}

fn acp_server(name: String, server: McpServerManifest) -> ManagedMcpServer {
    let headers = server.resolved_headers().into_iter().collect();
    ManagedMcpServer {
        name,
        transport: match server.transport {
            McpTransport::Stdio => ManagedMcpTransport::Stdio,
            McpTransport::Http => ManagedMcpTransport::Http,
            McpTransport::Sse => ManagedMcpTransport::Sse,
        },
        command: server.command,
        args: server.args,
        env: server.env.into_iter().collect(),
        url: server.url,
        headers,
    }
}

/// Converts one Tools server to Claude's `mcpServers` JSON shape.
pub fn claude_value(server: &McpServerManifest) -> Value {
    let mut value = Map::new();
    match server.transport {
        McpTransport::Stdio => {
            if let Some(command) = &server.command {
                value.insert("command".to_string(), Value::String(command.clone()));
            }
            insert_array(&mut value, "args", &server.args);
            insert_object(&mut value, "env", &server.env);
            if let Some(cwd) = &server.cwd {
                value.insert("cwd".to_string(), Value::String(cwd.clone()));
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            value.insert(
                "type".to_string(),
                Value::String(
                    match server.transport {
                        McpTransport::Sse => "sse",
                        _ => "http",
                    }
                    .to_string(),
                ),
            );
            if let Some(url) = &server.url {
                value.insert("url".to_string(), Value::String(url.clone()));
            }
            insert_object(&mut value, "headers", &server.resolved_headers());
        }
    }
    Value::Object(value)
}

/// Converts one Tools server to Vibe's `VIBE_MCP_SERVERS` JSON shape.
pub fn vibe_value(name: &str, server: &McpServerManifest) -> Value {
    let mut value = match claude_value(server) {
        Value::Object(value) => value,
        _ => Map::new(),
    };
    value.insert("name".to_string(), Value::String(name.to_string()));
    value.insert(
        "transport".to_string(),
        Value::String(
            match server.transport {
                McpTransport::Stdio => "stdio",
                McpTransport::Http => "http",
                McpTransport::Sse => "sse",
            }
            .to_string(),
        ),
    );
    value.remove("type");
    Value::Object(value)
}

fn insert_array(value: &mut Map<String, Value>, name: &str, entries: &[String]) {
    if !entries.is_empty() {
        value.insert(
            name.to_string(),
            Value::Array(entries.iter().cloned().map(Value::String).collect()),
        );
    }
}

fn insert_object(value: &mut Map<String, Value>, name: &str, entries: &BTreeMap<String, String>) {
    if !entries.is_empty() {
        value.insert(
            name.to_string(),
            Value::Object(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                    .collect(),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_projection_resolves_remote_headers() {
        let server = McpServerManifest {
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            url: Some("https://example.com/mcp".to_string()),
            headers: BTreeMap::from([("X-Key".to_string(), "value".to_string())]),
            header_env: BTreeMap::new(),
            bearer_token_env_var: None,
        };

        assert_eq!(
            claude_value(&server),
            serde_json::json!({
                "type": "http",
                "url": "https://example.com/mcp",
                "headers": {"X-Key": "value"}
            })
        );
    }

    #[test]
    fn acp_projection_preserves_stdio_launch_configuration() {
        let server = McpServerManifest {
            transport: McpTransport::Stdio,
            command: Some("npx".to_string()),
            args: vec!["-y".to_string(), "server".to_string()],
            env: BTreeMap::from([("MODE".to_string(), "local".to_string())]),
            cwd: None,
            url: None,
            headers: BTreeMap::new(),
            header_env: BTreeMap::new(),
            bearer_token_env_var: None,
        };

        assert_eq!(
            acp_server("local".to_string(), server),
            ManagedMcpServer {
                name: "local".to_string(),
                transport: ManagedMcpTransport::Stdio,
                command: Some("npx".to_string()),
                args: vec!["-y".to_string(), "server".to_string()],
                env: vec![("MODE".to_string(), "local".to_string())],
                url: None,
                headers: Vec::new(),
            }
        );
    }
}
