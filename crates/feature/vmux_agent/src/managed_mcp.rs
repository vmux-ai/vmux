use std::collections::BTreeMap;

use vmux_api::protocol::{ManagedMcpServer, ManagedMcpTransport};
use vmux_ecs::profile::mcp_credentials::McpCredentialAccess;
#[cfg(not(test))]
use vmux_ecs::profile::mcp_credentials::McpCredentialStorage;
use vmux_tool::{McpServerManifest, McpTransport};

use std::fmt::Write;

pub(crate) struct ManagedMcpServers(BTreeMap<String, McpServerManifest>);

impl ManagedMcpServers {
    pub(crate) fn current() -> Self {
        #[cfg(test)]
        {
            Self(BTreeMap::new())
        }

        #[cfg(not(test))]
        {
            match vmux_tool::ToolStore::current().load() {
                Ok(manifest) => Self(manifest.mcp.servers),
                Err(error) => {
                    bevy::log::warn!("managed MCP servers unavailable: {error}");
                    Self(BTreeMap::new())
                }
            }
        }
    }
}

impl IntoIterator for ManagedMcpServers {
    type Item = (String, McpServerManifest);
    type IntoIter = std::collections::btree_map::IntoIter<String, McpServerManifest>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

pub(super) struct PreparedManagedMcpServers {
    pub(super) servers: Vec<ManagedMcpServer>,
    pub(super) revision: u64,
}

impl PreparedManagedMcpServers {
    pub(super) fn prepare() -> Result<Self, String> {
        for _ in 0..3 {
            let revision = McpCredentialAccess::stable_revision()?;
            let mut servers = Vec::new();
            for (name, server) in ManagedMcpServers::current() {
                servers.push(acp_server(name, server));
            }
            if McpCredentialAccess::revision() != revision {
                continue;
            }
            return Ok(Self { servers, revision });
        }
        Err("MCP configuration changed repeatedly while preparing the agent".to_string())
    }
}

fn acp_server(name: String, server: McpServerManifest) -> ManagedMcpServer {
    let headers = McpAuthorization::headers(&name, &server)
        .into_iter()
        .collect();
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
        let mut encoded = String::from("VMUX_MCP_OAUTH_");
        for byte in name.as_bytes() {
            write!(&mut encoded, "{byte:02X}").unwrap();
        }
        encoded
    }

    fn resolved_headers(server: &McpServerManifest) -> BTreeMap<String, String> {
        let mut headers = server.headers.clone();
        for (header, variable) in &server.header_env {
            if let Ok(value) = std::env::var(variable) {
                headers.insert(header.clone(), value);
            }
        }
        headers
    }

    fn headers(name: &str, server: &McpServerManifest) -> BTreeMap<String, String> {
        let mut headers = Self::resolved_headers(server);
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
    ) -> Result<Option<vmux_ecs::profile::mcp_credentials::McpOauthCredentials>, String> {
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
        credentials: vmux_ecs::profile::mcp_credentials::McpOauthCredentials,
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
        credentials: &mut vmux_ecs::profile::mcp_credentials::McpOauthCredentials,
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
            vmux_ecs::profile::mcp_credentials::McpOauthCredentials::expires_at(token.expires_in);
        if let Some(scope) = token.scope {
            credentials.scope = scope;
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

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
            acp_server("local".to_string(), server),
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
}
