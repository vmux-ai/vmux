use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::mcp_config::{McpAuthSpec, McpResolvedServer, McpServerSpec};
use crate::mcp_oauth::{McpOauthFlow, McpOauthStatus};
use crate::{McpConfigDocument, McpServerManifest};

pub struct McpDoctor;

impl McpDoctor {
    pub fn report(
        home: &Path,
        servers: &[McpResolvedServer],
        diagnostics: &[String],
        oauth: Option<&McpOauthFlow>,
    ) -> Vec<String> {
        let mut lines = Vec::new();
        for warning in diagnostics {
            lines.push(format!("error: {warning}"));
        }
        let external = Self::discover_external(home);
        if servers.is_empty() {
            lines.push("no Vmux-managed MCP servers configured".to_string());
        }
        for server in servers {
            let mut notes = vec![
                server.spec.transport_label().to_string(),
                server.source.label().to_string(),
            ];
            if !server.trusted {
                notes.push(format!(
                    "untrusted (run `vmux tools mcp trust {}`)",
                    server.id
                ));
            } else if !server.enabled {
                notes.push("disabled".to_string());
            } else {
                notes.push("enabled".to_string());
            }
            if let Some((url, _)) = server.spec.oauth_config() {
                let status = oauth
                    .map(|flow| flow.status(&server.id, url))
                    .unwrap_or(McpOauthStatus::Unreadable);
                notes.push(status.label().to_string());
            }
            if let Some(paths) = external.get(&server.id) {
                notes.push(format!("duplicated in {}", paths.join(", ")));
            }
            lines.push(format!("{} · {}", server.id, notes.join(" · ")));
        }
        for (name, paths) in &external {
            if servers.iter().any(|server| &server.id == name) {
                continue;
            }
            if paths.len() > 1 {
                lines.push(format!(
                    "external {name} · configured in multiple agent files: {}",
                    paths.join(", ")
                ));
            }
        }
        lines
    }

    fn discover_external(home: &Path) -> BTreeMap<String, Vec<String>> {
        let mut found = BTreeMap::<String, Vec<String>>::new();
        for relative in [
            ".claude.json",
            ".codex/config.toml",
            ".vibe/config.toml",
            ".gemini/settings.json",
            ".mcp.json",
        ] {
            let path = home.join(relative);
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(document) = McpConfigDocument::parse(&source) else {
                continue;
            };
            let Ok(entries) = document.entries() else {
                continue;
            };
            for (name, value) in entries {
                if name == "vmux" || McpServerManifest::decode(&value).is_err() {
                    continue;
                }
                found
                    .entry(name)
                    .or_default()
                    .push(path.to_string_lossy().into_owned());
            }
        }
        found
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpSyncAgent {
    Claude,
    Codex,
    Gemini,
    Vibe,
}

impl McpSyncAgent {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "claude" | "claude-acp" => Ok(Self::Claude),
            "codex" | "codex-acp" => Ok(Self::Codex),
            "gemini" => Ok(Self::Gemini),
            "vibe" | "mistral-vibe" => Ok(Self::Vibe),
            other => Err(format!(
                "unsupported sync target \"{other}\" (expected claude, codex, gemini, or vibe)"
            )),
        }
    }

    pub fn config_path(self, home: &Path) -> PathBuf {
        match self {
            Self::Claude => home.join(".claude.json"),
            Self::Codex => home.join(".codex").join("config.toml"),
            Self::Gemini => home.join(".gemini").join("settings.json"),
            Self::Vibe => home.join(".vibe").join("config.toml"),
        }
    }

    fn agent_id(self) -> &'static str {
        match self {
            Self::Claude => "claude-acp",
            Self::Codex => "codex-acp",
            Self::Gemini => "gemini",
            Self::Vibe => "mistral-vibe",
        }
    }
}

pub struct McpSync;

impl McpSync {
    pub fn render(
        agent: McpSyncAgent,
        existing: Option<&str>,
        resolved: &crate::McpResolveResult,
        proxy_binary: &str,
        profile_name: &str,
        cwd: Option<&Path>,
    ) -> Result<String, String> {
        let active: Vec<&McpResolvedServer> = resolved
            .servers
            .iter()
            .filter(|server| {
                server.enabled && server.trusted && server.spec.matches_agent(agent.agent_id())
            })
            .collect();
        match agent {
            McpSyncAgent::Claude | McpSyncAgent::Gemini => {
                Self::render_json(agent, existing, &active, proxy_binary, profile_name, cwd)
            }
            McpSyncAgent::Codex => {
                Self::render_codex_toml(existing, &active, proxy_binary, profile_name, cwd)
            }
            McpSyncAgent::Vibe => {
                Self::render_vibe_toml(existing, &active, proxy_binary, profile_name, cwd)
            }
        }
    }

    fn proxy_args(server_id: &str, profile_name: &str, cwd: Option<&Path>) -> Vec<String> {
        let mut args = vec![
            "mcp-proxy".to_string(),
            "--server".to_string(),
            server_id.to_string(),
            "--profile".to_string(),
            profile_name.to_string(),
        ];
        if let Some(cwd) = cwd {
            args.push("--cwd".to_string());
            args.push(cwd.to_string_lossy().into_owned());
        }
        args
    }

    fn needs_proxy(spec: &McpServerSpec, agent: McpSyncAgent) -> bool {
        match spec {
            McpServerSpec::Stdio { cwd, .. } => cwd.is_some(),
            McpServerSpec::Http {
                headers,
                header_env,
                bearer_token_env_var,
                auth,
                ..
            } => {
                matches!(auth, McpAuthSpec::OAuth { .. })
                    || !header_env.is_empty()
                    || (bearer_token_env_var.is_some() && agent != McpSyncAgent::Codex)
                    || (!headers.is_empty() && agent == McpSyncAgent::Vibe)
            }
            McpServerSpec::Sse {
                headers,
                header_env,
                bearer_token_env_var,
                auth,
                ..
            } => {
                agent == McpSyncAgent::Codex
                    || matches!(auth, McpAuthSpec::OAuth { .. })
                    || !header_env.is_empty()
                    || bearer_token_env_var.is_some()
                    || (!headers.is_empty() && agent == McpSyncAgent::Vibe)
            }
        }
    }

    fn render_json(
        agent: McpSyncAgent,
        existing: Option<&str>,
        servers: &[&McpResolvedServer],
        proxy_binary: &str,
        profile_name: &str,
        cwd: Option<&Path>,
    ) -> Result<String, String> {
        let mut root: Value = match existing.filter(|text| !text.trim().is_empty()) {
            Some(text) => serde_json::from_str(text).map_err(|error| error.to_string())?,
            None => json!({}),
        };
        let object = root
            .as_object_mut()
            .ok_or_else(|| "MCP JSON config root must be an object".to_string())?;
        let mcp_servers = object
            .entry("mcpServers")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| "mcpServers must be an object".to_string())?;
        for server in servers {
            let entry = if Self::needs_proxy(&server.spec, agent) {
                json!({
                    "command": proxy_binary,
                    "args": Self::proxy_args(&server.id, profile_name, cwd),
                })
            } else {
                Self::json_entry(&server.spec)
            };
            mcp_servers.insert(server.id.clone(), entry);
        }
        let mut text = serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?;
        text.push('\n');
        Ok(text)
    }

    fn json_entry(spec: &McpServerSpec) -> Value {
        match spec {
            McpServerSpec::Stdio {
                command, args, env, ..
            } => {
                let mut map = Map::new();
                map.insert("command".into(), Value::String(command.clone()));
                if !args.is_empty() {
                    map.insert("args".into(), json!(args));
                }
                if !env.is_empty() {
                    map.insert("env".into(), json!(env));
                }
                Value::Object(map)
            }
            McpServerSpec::Http { url, headers, .. } => {
                let mut map = Map::new();
                map.insert("type".into(), Value::String("http".into()));
                map.insert("url".into(), Value::String(url.clone()));
                if !headers.is_empty() {
                    map.insert("headers".into(), json!(headers));
                }
                Value::Object(map)
            }
            McpServerSpec::Sse { url, headers, .. } => {
                let mut map = Map::new();
                map.insert("type".into(), Value::String("sse".into()));
                map.insert("url".into(), Value::String(url.clone()));
                if !headers.is_empty() {
                    map.insert("headers".into(), json!(headers));
                }
                Value::Object(map)
            }
        }
    }

    fn render_codex_toml(
        existing: Option<&str>,
        servers: &[&McpResolvedServer],
        proxy_binary: &str,
        profile_name: &str,
        cwd: Option<&Path>,
    ) -> Result<String, String> {
        let mut root: toml::Table = match existing.filter(|text| !text.trim().is_empty()) {
            Some(text) => toml::from_str(text).map_err(|error| error.to_string())?,
            None => toml::Table::new(),
        };
        let table = root
            .entry("mcp_servers")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or_else(|| "mcp_servers must be a TOML table".to_string())?;
        for server in servers {
            let mut entry = toml::Table::new();
            if Self::needs_proxy(&server.spec, McpSyncAgent::Codex) {
                entry.insert("command".into(), toml::Value::String(proxy_binary.into()));
                entry.insert(
                    "args".into(),
                    toml::Value::Array(
                        Self::proxy_args(&server.id, profile_name, cwd)
                            .into_iter()
                            .map(toml::Value::String)
                            .collect(),
                    ),
                );
            } else {
                Self::fill_codex_entry(&mut entry, &server.spec);
            }
            table.insert(server.id.clone(), toml::Value::Table(entry));
        }
        toml::to_string_pretty(&root).map_err(|error| error.to_string())
    }

    fn fill_codex_entry(entry: &mut toml::Table, spec: &McpServerSpec) {
        match spec {
            McpServerSpec::Stdio {
                command, args, env, ..
            } => {
                entry.insert("command".into(), toml::Value::String(command.clone()));
                if !args.is_empty() {
                    entry.insert(
                        "args".into(),
                        toml::Value::Array(args.iter().cloned().map(toml::Value::String).collect()),
                    );
                }
                if !env.is_empty() {
                    let mut table = toml::Table::new();
                    for (key, value) in env {
                        table.insert(key.clone(), toml::Value::String(value.clone()));
                    }
                    entry.insert("env".into(), toml::Value::Table(table));
                }
            }
            McpServerSpec::Http {
                url,
                headers,
                bearer_token_env_var,
                ..
            }
            | McpServerSpec::Sse {
                url,
                headers,
                bearer_token_env_var,
                ..
            } => {
                entry.insert("url".into(), toml::Value::String(url.clone()));
                if let Some(var) = bearer_token_env_var {
                    entry.insert(
                        "bearer_token_env_var".into(),
                        toml::Value::String(var.clone()),
                    );
                }
                if !headers.is_empty() {
                    let mut table = toml::Table::new();
                    for (key, value) in headers {
                        table.insert(key.clone(), toml::Value::String(value.clone()));
                    }
                    entry.insert("http_headers".into(), toml::Value::Table(table));
                }
            }
        }
    }

    fn render_vibe_toml(
        existing: Option<&str>,
        servers: &[&McpResolvedServer],
        proxy_binary: &str,
        profile_name: &str,
        cwd: Option<&Path>,
    ) -> Result<String, String> {
        let mut root: toml::Table = match existing.filter(|text| !text.trim().is_empty()) {
            Some(text) => toml::from_str(text).map_err(|error| error.to_string())?,
            None => toml::Table::new(),
        };
        let list = root
            .entry("mcp_servers")
            .or_insert_with(|| toml::Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| "mcp_servers must be a TOML array".to_string())?;
        for server in servers {
            list.retain(|item| item.get("name").and_then(toml::Value::as_str) != Some(&server.id));
            let mut entry = toml::Table::new();
            entry.insert("name".into(), toml::Value::String(server.id.clone()));
            if Self::needs_proxy(&server.spec, McpSyncAgent::Vibe) {
                entry.insert("transport".into(), toml::Value::String("stdio".into()));
                entry.insert("command".into(), toml::Value::String(proxy_binary.into()));
                entry.insert(
                    "args".into(),
                    toml::Value::Array(
                        Self::proxy_args(&server.id, profile_name, cwd)
                            .into_iter()
                            .map(toml::Value::String)
                            .collect(),
                    ),
                );
            } else {
                Self::fill_vibe_entry(&mut entry, &server.spec);
            }
            list.push(toml::Value::Table(entry));
        }
        toml::to_string_pretty(&root).map_err(|error| error.to_string())
    }

    fn fill_vibe_entry(entry: &mut toml::Table, spec: &McpServerSpec) {
        entry.insert(
            "transport".into(),
            toml::Value::String(spec.transport_label().into()),
        );
        match spec {
            McpServerSpec::Stdio { command, args, .. } => {
                entry.insert("command".into(), toml::Value::String(command.clone()));
                if !args.is_empty() {
                    entry.insert(
                        "args".into(),
                        toml::Value::Array(args.iter().cloned().map(toml::Value::String).collect()),
                    );
                }
            }
            McpServerSpec::Http { url, .. } | McpServerSpec::Sse { url, .. } => {
                entry.insert("url".into(), toml::Value::String(url.clone()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_config::McpConfigScope;

    #[test]
    fn sync_routes_oauth_and_cwd_servers_through_vmux_mcp_proxy_while_preserving_other_keys() {
        let servers = vec![
            McpResolvedServer {
                id: "linear-vmx".into(),
                spec: McpServerSpec::Http {
                    url: "https://mcp.linear.app/mcp".into(),
                    headers: BTreeMap::new(),
                    header_env: BTreeMap::new(),
                    bearer_token_env_var: None,
                    auth: McpAuthSpec::OAuth {
                        scopes: vec!["read".into()],
                    },
                    enabled: true,
                    agents: Vec::new(),
                },
                source: McpConfigScope::Global,
                enabled: true,
                trusted: true,
                repo_root: None,
                digest: None,
            },
            McpResolvedServer {
                id: "untrusted".into(),
                spec: McpServerSpec::Stdio {
                    command: "./evil".into(),
                    args: Vec::new(),
                    env: BTreeMap::new(),
                    cwd: None,
                    enabled: true,
                    agents: Vec::new(),
                },
                source: McpConfigScope::Project,
                enabled: true,
                trusted: false,
                repo_root: Some("/repo".into()),
                digest: Some("abc".into()),
            },
        ];

        let resolved = crate::McpResolveResult {
            servers,
            managed: Vec::new(),
            diagnostics: Vec::new(),
            repo_root: None,
        };
        let claude = McpSync::render(
            McpSyncAgent::Claude,
            Some(r#"{"theme":"dark","mcpServers":{"keep":{"command":"echo"}}}"#),
            &resolved,
            "/usr/local/bin/vmux",
            "default",
            None,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&claude).unwrap();
        assert_eq!(parsed["theme"], "dark");
        assert_eq!(parsed["mcpServers"]["keep"]["command"], "echo");
        assert_eq!(
            parsed["mcpServers"]["linear-vmx"]["command"],
            "/usr/local/bin/vmux"
        );
        assert_eq!(
            parsed["mcpServers"]["linear-vmx"]["args"],
            json!([
                "mcp-proxy",
                "--server",
                "linear-vmx",
                "--profile",
                "default"
            ])
        );
        assert!(parsed["mcpServers"].get("untrusted").is_none());

        let codex = McpSync::render(
            McpSyncAgent::Codex,
            Some("model = 'o3'\n"),
            &resolved,
            "/usr/local/bin/vmux",
            "default",
            None,
        )
        .unwrap();
        let parsed_toml: toml::Table = toml::from_str(&codex).unwrap();
        assert_eq!(parsed_toml["model"].as_str(), Some("o3"));
        assert_eq!(
            parsed_toml["mcp_servers"]["linear-vmx"]["command"].as_str(),
            Some("/usr/local/bin/vmux")
        );
    }

    #[test]
    fn doctor_reports_untrusted_oauth_and_external_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".claude.json"),
            r#"{"mcpServers":{"linear-vmx":{"type":"http","url":"https://mcp.linear.app/mcp"}}}"#,
        )
        .unwrap();
        let servers = vec![McpResolvedServer {
            id: "linear-vmx".into(),
            spec: McpServerSpec::Http {
                url: "https://mcp.linear.app/mcp".into(),
                headers: BTreeMap::new(),
                header_env: BTreeMap::new(),
                bearer_token_env_var: None,
                auth: McpAuthSpec::OAuth {
                    scopes: vec!["read".into()],
                },
                enabled: true,
                agents: Vec::new(),
            },
            source: McpConfigScope::Project,
            enabled: true,
            trusted: false,
            repo_root: Some("/repo".into()),
            digest: Some("deadbeef".into()),
        }];
        let profile = vmux_profile::Profile::named("test");
        let paths = vmux_profile::ProfilePaths::at(profile.clone(), dir.path().to_path_buf());
        let oauth = McpOauthFlow::for_profile(profile, paths).unwrap();
        let report = McpDoctor::report(dir.path(), &servers, &[], Some(&oauth));
        assert_eq!(report.len(), 1);
        assert!(report[0].contains("untrusted"));
        assert!(report[0].contains("oauth: not authenticated"));
        assert!(report[0].contains("duplicated in"));
    }
}
