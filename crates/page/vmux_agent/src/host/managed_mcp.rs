use std::collections::BTreeMap;

use serde_json::{Map, Value};
use vmux_api::protocol::{ManagedMcpServer, ManagedMcpTransport};
use vmux_core::profile::mcp_credentials::McpCredentialAccess;
#[cfg(not(test))]
use vmux_core::profile::mcp_credentials::McpCredentialStorage;
use vmux_tool::{McpServerManifest, McpTransport};

#[cfg(not(test))]
pub fn load() -> BTreeMap<String, McpServerManifest> {
    match vmux_tool::ToolStore::current().load() {
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

pub(crate) struct PreparedManagedMcpServers {
    pub(crate) servers: Vec<ManagedMcpServer>,
    pub(crate) revision: u64,
}

pub(crate) fn acp_servers(agent_id: &str) -> Result<PreparedManagedMcpServers, String> {
    for _ in 0..3 {
        let revision = McpCredentialAccess::stable_revision()?;
        let mut servers = Vec::new();
        for (name, server) in load() {
            if crate::acp_tool::registry_id_alias(agent_id) == "codex-acp"
                && server.transport == McpTransport::Sse
            {
                bevy::log::warn!(
                    "managed MCP server {name} skipped for Codex because SSE is unsupported"
                );
                continue;
            }
            servers.push(acp_server(name, server, agent_id));
        }
        if McpCredentialAccess::revision() != revision {
            continue;
        }
        return Ok(PreparedManagedMcpServers { servers, revision });
    }
    Err("MCP configuration changed repeatedly while preparing the agent".to_string())
}

fn acp_server(mut name: String, server: McpServerManifest, agent_id: &str) -> ManagedMcpServer {
    let headers = McpAuthorization::headers(&name, &server)
        .into_iter()
        .collect();
    if crate::acp_tool::registry_id_alias(agent_id) == "codex-acp" {
        name = format!("vmux_{name}");
    }
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
        cwd: server.cwd,
        url: server.url,
        headers,
    }
}

pub fn claude_value(name: &str, server: &McpServerManifest) -> Value {
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
            insert_object(
                &mut value,
                "headers",
                &McpAuthorization::claude_headers(name, server),
            );
        }
    }
    Value::Object(value)
}

pub fn vibe_value(name: &str, server: &McpServerManifest) -> Value {
    let mut value = Map::new();
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
            if let Some(url) = &server.url {
                value.insert("url".to_string(), Value::String(url.clone()));
            }
            insert_object(
                &mut value,
                "headers",
                &McpAuthorization::vibe_headers(server),
            );
            if let Some(variable) = McpAuthorization::bearer_environment_variable(name, server) {
                value.insert("api_key_env".to_string(), Value::String(variable));
                value.insert(
                    "api_key_header".to_string(),
                    Value::String("Authorization".to_string()),
                );
                value.insert(
                    "api_key_format".to_string(),
                    Value::String("Bearer {token}".to_string()),
                );
            }
        }
    }
    Value::Object(value)
}

pub(crate) struct McpAuthorization;

impl McpAuthorization {
    const LINEAR_ID: &str = "linear";
    const LINEAR_URL: &str = "https://mcp.linear.app/mcp";

    fn environment_variable(name: &str, server: &McpServerManifest) -> Option<String> {
        let expected = Self::environment_name(name);
        if name != Self::LINEAR_ID
            || server.transport != McpTransport::Http
            || server.command.is_some()
            || !server.args.is_empty()
            || !server.env.is_empty()
            || server.cwd.is_some()
            || server.url.as_deref() != Some(Self::LINEAR_URL)
            || !server.headers.is_empty()
            || !server.header_env.is_empty()
            || server
                .bearer_token_env_var
                .as_ref()
                .is_some_and(|configured| configured != &expected)
        {
            return None;
        }
        Some(expected)
    }

    fn environment_name(name: &str) -> String {
        use std::fmt::Write;

        let mut encoded = String::from("VMUX_MCP_OAUTH_");
        for byte in name.as_bytes() {
            write!(&mut encoded, "{byte:02X}").unwrap();
        }
        encoded
    }

    fn bearer_environment_variable(name: &str, server: &McpServerManifest) -> Option<String> {
        server
            .bearer_token_env_var
            .clone()
            .or_else(|| Self::environment_variable(name, server))
    }

    fn claude_headers(name: &str, server: &McpServerManifest) -> BTreeMap<String, String> {
        let mut headers = server.headers.clone();
        for (header, variable) in &server.header_env {
            headers.insert(header.clone(), format!("${{{variable}}}"));
        }
        if let Some(variable) = Self::bearer_environment_variable(name, server) {
            headers.insert(
                "Authorization".to_string(),
                format!("Bearer ${{{variable}}}"),
            );
        }
        headers
    }

    fn vibe_headers(server: &McpServerManifest) -> BTreeMap<String, String> {
        let mut headers = server.headers.clone();
        for (header, variable) in &server.header_env {
            if let Ok(value) = std::env::var(variable) {
                headers.insert(header.clone(), value);
            }
        }
        headers
    }

    fn headers(name: &str, server: &McpServerManifest) -> BTreeMap<String, String> {
        let mut headers = Self::vibe_headers(server);
        if let Some(variable) = &server.bearer_token_env_var
            && let Ok(value) = std::env::var(variable)
        {
            headers.insert("Authorization".to_string(), format!("Bearer {value}"));
        }
        if Self::environment_variable(name, server).is_none() {
            return headers;
        }
        match Self::access_token(name, server) {
            Ok(Some(token)) => {
                headers.insert("Authorization".to_string(), format!("Bearer {token}"));
            }
            Ok(None) => {}
            Err(error) => {
                bevy::log::warn!("managed MCP authorization unavailable for {name}: {error}");
            }
        }
        headers
    }

    pub(crate) fn is_environment_variable(name: &str) -> bool {
        name.starts_with("VMUX_MCP_OAUTH_")
    }

    pub(crate) fn environment() -> Vec<(String, String)> {
        let mut env = Vec::new();
        for (name, server) in load() {
            let Some(variable) = Self::environment_variable(&name, &server) else {
                continue;
            };
            match Self::access_token(&name, &server) {
                Ok(Some(token)) => env.push((variable, token)),
                Ok(None) => {}
                Err(error) => {
                    bevy::log::warn!("managed MCP authorization unavailable for {name}: {error}");
                }
            }
        }
        env
    }

    #[cfg(not(test))]
    fn access_token(name: &str, server: &McpServerManifest) -> Result<Option<String>, String> {
        let credentials = McpCredentialAccess::read(|| Self::credentials(name, server))?;
        let Some(credentials) = credentials else {
            return Ok(None);
        };
        if !credentials.expires_soon() {
            return Self::token(credentials).map(Some);
        }
        McpCredentialAccess::refresh(|| {
            let current = McpCredentialAccess::read(|| Self::credentials(name, server))?;
            let Some(current) = current else {
                return Ok(None);
            };
            if !current.expires_soon() {
                return Self::token(current).map(Some);
            }
            let mut refreshed = current.clone();
            Self::refresh(&mut refreshed)?;
            McpCredentialAccess::write(|| {
                let Some(latest) = Self::credentials(name, server)? else {
                    return Ok(None);
                };
                if latest != current {
                    return Self::token(latest).map(Some);
                }
                McpCredentialStorage::store(name, &refreshed)?;
                Self::token(refreshed).map(Some)
            })
        })
    }

    #[cfg(not(test))]
    fn credentials(
        name: &str,
        server: &McpServerManifest,
    ) -> Result<Option<vmux_core::profile::mcp_credentials::McpOauthCredentials>, String> {
        let Some(credentials) = McpCredentialStorage::load(name)? else {
            return Ok(None);
        };
        let resource = server
            .url
            .as_deref()
            .ok_or_else(|| "OAuth MCP server has no resource URL".to_string())?;
        if !credentials.authorizes(resource) {
            return Err("stored OAuth resource does not match the MCP server URL".to_string());
        }
        Ok(Some(credentials))
    }

    #[cfg(not(test))]
    fn token(
        credentials: vmux_core::profile::mcp_credentials::McpOauthCredentials,
    ) -> Result<String, String> {
        if credentials.access_token.is_empty() {
            return Err("stored access token is empty".to_string());
        }
        Ok(credentials.access_token)
    }

    #[cfg(test)]
    fn access_token(_name: &str, _server: &McpServerManifest) -> Result<Option<String>, String> {
        Ok(None)
    }

    #[cfg(not(test))]
    fn refresh(
        credentials: &mut vmux_core::profile::mcp_credentials::McpOauthCredentials,
    ) -> Result<(), String> {
        let refresh_token = credentials
            .refresh_token
            .as_ref()
            .ok_or_else(|| "OAuth session expired and has no refresh token".to_string())?;
        let mut form = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.clone()),
            ("client_id", credentials.client_id.clone()),
        ];
        if let Some(secret) = &credentials.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        if !credentials.resource.is_empty() {
            form.push(("resource", credentials.resource.clone()));
        }
        if !credentials.scope.is_empty() {
            form.push(("scope", credentials.scope.clone()));
        }
        let endpoint = url::Url::parse(&credentials.token_endpoint)
            .map_err(|error| format!("invalid OAuth token endpoint: {error}"))?;
        if endpoint.scheme() != "https" {
            return Err("OAuth token endpoint must use https".to_string());
        }
        let response = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| error.to_string())?
            .post(endpoint)
            .form(&form)
            .send()
            .map_err(|error| error.to_string())?;
        let status = response.status();
        let body = response.text().map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("token refresh failed ({status}): {body}"));
        }
        let token: RefreshTokenResponse = serde_json::from_str(&body)
            .map_err(|error| format!("invalid token refresh response: {error}"))?;
        credentials.access_token = token.access_token;
        if token.refresh_token.is_some() {
            credentials.refresh_token = token.refresh_token;
        }
        credentials.expires_at =
            vmux_core::profile::mcp_credentials::McpOauthCredentials::expires_at(token.expires_in);
        if let Some(scope) = token.scope {
            credentials.scope = scope;
        }
        Ok(())
    }
}

pub(crate) struct CodexMcp;

impl CodexMcp {
    pub(crate) fn servers() -> BTreeMap<String, McpServerManifest> {
        let mut servers = load();
        servers.retain(|name, server| {
            if server.transport == McpTransport::Sse {
                bevy::log::warn!(
                    "managed MCP server {name} skipped for Codex because SSE is unsupported"
                );
                return false;
            }
            if server.bearer_token_env_var.is_none()
                && let Some(variable) = McpAuthorization::environment_variable(name, server)
            {
                server.bearer_token_env_var = Some(variable);
            }
            true
        });
        servers
    }
}

#[cfg(not(test))]
#[derive(serde::Deserialize)]
struct RefreshTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
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
            claude_value("remote", &server),
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
            cwd: Some("/tmp/project".to_string()),
            url: None,
            headers: BTreeMap::new(),
            header_env: BTreeMap::new(),
            bearer_token_env_var: None,
        };

        assert_eq!(
            acp_server("local".to_string(), server, "claude-acp"),
            ManagedMcpServer {
                name: "local".to_string(),
                transport: ManagedMcpTransport::Stdio,
                command: Some("npx".to_string()),
                args: vec!["-y".to_string(), "server".to_string()],
                env: vec![("MODE".to_string(), "local".to_string())],
                cwd: Some("/tmp/project".to_string()),
                url: None,
                headers: Vec::new(),
            }
        );
    }

    #[test]
    fn codex_acp_namespaces_managed_servers() {
        let server = McpServerManifest {
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            url: Some("https://example.com/mcp".to_string()),
            headers: BTreeMap::new(),
            header_env: BTreeMap::new(),
            bearer_token_env_var: None,
        };

        assert_eq!(
            acp_server("linear".to_string(), server, "codex-acp").name,
            "vmux_linear"
        );
    }

    #[test]
    fn oauth_environment_names_are_injective_and_shell_safe() {
        assert_ne!(
            McpAuthorization::environment_name("work.dev"),
            McpAuthorization::environment_name("work-dev")
        );
        assert!(
            McpAuthorization::environment_name("linear")
                .chars()
                .all(|character| character.is_ascii_uppercase()
                    || character.is_ascii_digit()
                    || character == '_')
        );
    }

    #[test]
    fn custom_http_auth_does_not_receive_vmux_oauth_configuration() {
        let mut server = McpServerManifest {
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            url: Some("https://mcp.linear.app/mcp".to_string()),
            headers: BTreeMap::new(),
            header_env: BTreeMap::new(),
            bearer_token_env_var: None,
        };

        assert_eq!(
            McpAuthorization::environment_variable("linear", &server),
            Some(McpAuthorization::environment_name("linear"))
        );
        server
            .headers
            .insert("Authorization".to_string(), "Bearer custom".to_string());
        assert_eq!(
            McpAuthorization::environment_variable("linear", &server),
            None
        );
    }

    #[test]
    fn claude_projection_references_managed_oauth_environment() {
        let server = McpServerManifest {
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            url: Some("https://mcp.linear.app/mcp".to_string()),
            headers: BTreeMap::new(),
            header_env: BTreeMap::new(),
            bearer_token_env_var: None,
        };

        assert_eq!(
            claude_value("linear", &server),
            serde_json::json!({
                "type": "http",
                "url": "https://mcp.linear.app/mcp",
                "headers": {
                    "Authorization": "Bearer ${VMUX_MCP_OAUTH_6C696E656172}"
                }
            })
        );
    }

    #[test]
    fn vibe_projection_references_managed_oauth_environment() {
        let server = McpServerManifest {
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            url: Some("https://mcp.linear.app/mcp".to_string()),
            headers: BTreeMap::new(),
            header_env: BTreeMap::new(),
            bearer_token_env_var: None,
        };

        assert_eq!(
            vibe_value("linear", &server),
            serde_json::json!({
                "name": "linear",
                "transport": "http",
                "url": "https://mcp.linear.app/mcp",
                "api_key_env": "VMUX_MCP_OAUTH_6C696E656172",
                "api_key_header": "Authorization",
                "api_key_format": "Bearer {token}"
            })
        );
    }
}
