use std::path::PathBuf;

use super::registry::RegistryAgent;
#[cfg(test)]
use crate::AgentKind;
use crate::host::launch::AgentLaunchPolicy;
use crate::manifest::CliProviderManifest;

pub(super) struct AcpEnvironment(Vec<(String, String)>);

impl AcpEnvironment {
    pub(super) fn build(
        mut base: Vec<(String, String)>,
        login_env: &[(String, String)],
        path_prepend: Option<String>,
    ) -> Self {
        base.extend(login_env.iter().cloned());
        let mut environment = Self(base);
        environment.deduplicate();
        environment.prepend_path(path_prepend);
        environment
    }

    pub(super) fn for_agent(
        mut self,
        agent_id: &str,
        policy: &AgentLaunchPolicy,
        manifest: &CliProviderManifest,
    ) -> Self {
        match RegistryAgent::canonical_id(agent_id) {
            "mistral-vibe" => self.apply_vibe(),
            "codex-acp" => self.apply_codex(policy, manifest),
            "claude-acp" => self.apply_claude(),
            _ => {}
        }
        self
    }

    pub(super) fn with_managed_servers(
        mut self,
        agent_id: &str,
        direct_only_namespace: &str,
        server_names: impl IntoIterator<Item = String>,
    ) -> Self {
        if RegistryAgent::canonical_id(agent_id) != "codex-acp" {
            return self;
        }
        let existing = self
            .0
            .iter()
            .rev()
            .find(|(key, _)| key == "CODEX_CONFIG")
            .map(|(_, value)| value.as_str());
        let (mut config, warning) = Self::parse_codex_config(existing);
        if let Some(warning) = warning {
            bevy::log::warn!("{warning}");
        }
        let features = config
            .entry("features")
            .or_insert_with(|| serde_json::json!({}));
        if !features.is_object() {
            *features = serde_json::json!({});
        }
        let code_mode = features
            .as_object_mut()
            .unwrap()
            .entry("code_mode")
            .or_insert_with(|| serde_json::json!({}));
        if !code_mode.is_object() {
            *code_mode = serde_json::json!({});
        }
        let namespaces = code_mode
            .as_object_mut()
            .unwrap()
            .entry("direct_only_tool_namespaces")
            .or_insert_with(|| serde_json::json!([]));
        if !namespaces.is_array() {
            *namespaces = serde_json::json!([]);
        }
        let namespaces = namespaces.as_array_mut().unwrap();
        let vmux = serde_json::Value::String(direct_only_namespace.to_string());
        if !namespaces.contains(&vmux) {
            namespaces.push(vmux);
        }
        for server_name in server_names {
            let namespace = serde_json::Value::String(format!("mcp__{server_name}"));
            if !namespaces.contains(&namespace) {
                namespaces.push(namespace);
            }
        }
        self.0.retain(|(key, _)| key != "CODEX_CONFIG");
        self.0.push((
            "CODEX_CONFIG".to_string(),
            serde_json::Value::Object(config).to_string(),
        ));
        self
    }

    pub(super) fn into_inner(self) -> Vec<(String, String)> {
        self.0
    }

    fn prepend_path(&mut self, prepend: Option<String>) {
        let Some(directory) = prepend else {
            return;
        };
        let existing = self
            .0
            .iter()
            .find(|(key, _)| key == "PATH")
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var("PATH").ok())
            .filter(|value| !value.is_empty());
        let path = match existing {
            Some(existing) => format!("{directory}:{existing}"),
            None => directory,
        };
        self.0.retain(|(key, _)| key != "PATH");
        self.0.push(("PATH".to_string(), path));
    }

    fn deduplicate(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut environment = Vec::with_capacity(self.0.len());
        for (key, value) in std::mem::take(&mut self.0).into_iter().rev() {
            if seen.insert(key.clone()) {
                environment.push((key, value));
            }
        }
        environment.reverse();
        self.0 = environment;
    }

    fn apply_claude(&mut self) {
        self.0.retain(|(key, _)| key != "MCP_TOOL_TIMEOUT");
        self.0.push((
            "MCP_TOOL_TIMEOUT".to_string(),
            (crate::mcp::LONG_MCP_TOOL_TIMEOUT_SECS * 1_000).to_string(),
        ));
    }

    fn apply_vibe(&mut self) {
        let mut disabled = Vec::new();
        if let Some(value) = self
            .0
            .iter()
            .rev()
            .find(|(key, _)| key == "VIBE_DISABLED_TOOLS")
            .map(|(_, value)| value)
        {
            match serde_json::from_str::<Vec<String>>(value) {
                Ok(existing) => Self::extend_unique(&mut disabled, existing),
                Err(error) => bevy::log::warn!(
                    "acp: existing VIBE_DISABLED_TOOLS is invalid JSON ({error}); discarding it"
                ),
            }
        }
        Self::extend_unique(&mut disabled, ["bash".to_string()]);
        self.0.retain(|(key, _)| key != "VIBE_DISABLED_TOOLS");
        self.0.push((
            "VIBE_DISABLED_TOOLS".to_string(),
            serde_json::to_string(&disabled).unwrap(),
        ));
        let mut mcp_servers: Vec<serde_json::Value> = Vec::new();
        if let Some(value) = self
            .0
            .iter()
            .rev()
            .find(|(key, _)| key == "VIBE_MCP_SERVERS")
            .map(|(_, value)| value)
        {
            match serde_json::from_str::<Vec<serde_json::Value>>(value) {
                Ok(existing) => {
                    for server in existing {
                        if let Some(name) = server.get("name").and_then(serde_json::Value::as_str) {
                            mcp_servers.retain(|candidate| {
                                candidate.get("name").and_then(serde_json::Value::as_str)
                                    != Some(name)
                            });
                        }
                        mcp_servers.push(server);
                    }
                }
                Err(error) => bevy::log::warn!(
                    "acp: existing VIBE_MCP_SERVERS is invalid JSON ({error}); discarding it"
                ),
            }
        }
        self.0.retain(|(key, _)| key != "VIBE_MCP_SERVERS");
        if !mcp_servers.is_empty() {
            self.0.push((
                "VIBE_MCP_SERVERS".to_string(),
                serde_json::to_string(&mcp_servers).unwrap(),
            ));
        }
    }

    fn apply_codex(&mut self, policy: &AgentLaunchPolicy, manifest: &CliProviderManifest) {
        self.0
            .retain(|(key, _)| key != "DISABLE_MCP_CONFIG_FILTERING");
        let existing = self
            .0
            .iter()
            .rev()
            .find(|(key, _)| key == "CODEX_CONFIG")
            .map(|(_, value)| value.as_str());
        let (mut config, warning) = Self::parse_codex_config(existing);
        if let Some(warning) = warning {
            bevy::log::warn!("{warning}");
        }
        config.insert(
            "approvals_reviewer".to_string(),
            serde_json::Value::String("user".to_string()),
        );
        let features = config
            .entry("features")
            .or_insert_with(|| serde_json::json!({}));
        if !features.is_object() {
            *features = serde_json::json!({});
        }
        let features = features.as_object_mut().unwrap();
        for feature in &manifest.disabled_features {
            features.insert(feature.clone(), serde_json::Value::Bool(false));
        }
        let code_mode = features
            .entry("code_mode")
            .or_insert_with(|| serde_json::json!({}));
        if !code_mode.is_object() {
            *code_mode = serde_json::json!({});
        }
        code_mode.as_object_mut().unwrap().insert(
            "direct_only_tool_namespaces".to_string(),
            serde_json::json!([manifest.direct_only_namespace]),
        );
        let tools = config
            .entry("tools")
            .or_insert_with(|| serde_json::json!({}));
        if !tools.is_object() {
            *tools = serde_json::json!({});
        }
        tools
            .as_object_mut()
            .unwrap()
            .insert("web_search".to_string(), serde_json::Value::Bool(false));
        Self::disable_codex_skills(
            &mut config,
            &crate::host::cli::codex::codex_disabled_skill_files(policy.disabled_skill_roots()),
        );
        let mcp_servers = config
            .entry("mcp_servers")
            .or_insert_with(|| serde_json::json!({}));
        if !mcp_servers.is_object() {
            *mcp_servers = serde_json::json!({});
        }
        let vmux = mcp_servers
            .as_object_mut()
            .unwrap()
            .entry("vmux")
            .or_insert_with(|| serde_json::json!({}));
        if !vmux.is_object() {
            *vmux = serde_json::json!({});
        }
        vmux.as_object_mut().unwrap().insert(
            "tool_timeout_sec".to_string(),
            serde_json::json!(crate::mcp::LONG_MCP_TOOL_TIMEOUT_SECS),
        );
        let instructions = config
            .get("developer_instructions")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let instructions = if instructions.contains("mcp__vmux__run") {
            instructions.to_string()
        } else if instructions.is_empty() {
            manifest.run_prompt.clone()
        } else {
            format!("{instructions}\n\n{}", manifest.run_prompt)
        };
        let instructions = policy.prompt(&instructions);
        let instructions = if manifest.conversation_title_prompt.is_empty()
            || instructions.contains("mcp__vmux__set_conversation_title")
        {
            instructions
        } else {
            format!("{instructions}\n\n{}", manifest.conversation_title_prompt)
        };
        config.insert(
            "developer_instructions".to_string(),
            serde_json::Value::String(instructions),
        );
        self.0.retain(|(key, _)| key != "CODEX_CONFIG");
        self.0.push((
            "CODEX_CONFIG".to_string(),
            serde_json::Value::Object(config).to_string(),
        ));
    }

    fn disable_codex_skills(
        config: &mut serde_json::Map<String, serde_json::Value>,
        skill_files: &[PathBuf],
    ) {
        if skill_files.is_empty() {
            return;
        }
        let skills = config
            .entry("skills")
            .or_insert_with(|| serde_json::json!({}));
        if !skills.is_object() {
            *skills = serde_json::json!({});
        }
        let configured = skills
            .as_object_mut()
            .unwrap()
            .entry("config")
            .or_insert_with(|| serde_json::json!([]));
        if !configured.is_array() {
            *configured = serde_json::json!([]);
        }
        let configured = configured.as_array_mut().unwrap();
        for skill_file in skill_files {
            let path = skill_file.to_string_lossy();
            if let Some(existing) = configured.iter_mut().find(|entry| {
                entry
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|candidate| candidate == path)
            }) {
                existing
                    .as_object_mut()
                    .unwrap()
                    .insert("enabled".to_string(), serde_json::Value::Bool(false));
            } else {
                configured.push(serde_json::json!({
                    "path": path,
                    "enabled": false,
                }));
            }
        }
    }

    pub(super) fn parse_codex_config(
        value: Option<&str>,
    ) -> (serde_json::Map<String, serde_json::Value>, Option<String>) {
        let Some(value) = value else {
            return (serde_json::Map::new(), None);
        };
        match serde_json::from_str::<serde_json::Value>(value) {
            Ok(serde_json::Value::Object(config)) => (config, None),
            Ok(value) => {
                let kind = match value {
                    serde_json::Value::Null => "null",
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Object(_) => unreachable!(),
                };
                (
                    serde_json::Map::new(),
                    Some(format!(
                        "acp: existing CODEX_CONFIG is not a JSON object ({kind}); discarding it"
                    )),
                )
            }
            Err(error) => (
                serde_json::Map::new(),
                Some(format!(
                    "acp: existing CODEX_CONFIG is invalid JSON ({error}); discarding it"
                )),
            ),
        }
    }

    fn extend_unique(values: &mut Vec<String>, additions: impl IntoIterator<Item = String>) {
        for value in additions {
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }
}

impl From<Vec<(String, String)>> for AcpEnvironment {
    fn from(environment: Vec<(String, String)>) -> Self {
        Self(environment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(key: &str, value: &str) -> (String, String) {
        (key.to_string(), value.to_string())
    }

    fn manifest(kind: AgentKind) -> CliProviderManifest {
        CliProviderManifest::bundled(kind)
    }

    #[test]
    fn login_environment_overrides_registry_environment() {
        let base = vec![env("MISTRAL_API_KEY", ""), env("KEEP", "1")];
        let login = vec![
            env("MISTRAL_API_KEY", "real-key"),
            env("PATH", "/login/bin"),
        ];
        let environment = AcpEnvironment::build(base, &login, None).into_inner();

        assert!(environment.contains(&env("MISTRAL_API_KEY", "real-key")));
        assert!(environment.contains(&env("KEEP", "1")));
        assert!(environment.contains(&env("PATH", "/login/bin")));
    }

    #[test]
    fn managed_binary_precedes_login_path() {
        let login = vec![env("PATH", "/login/bin")];
        let environment =
            AcpEnvironment::build(Vec::new(), &login, Some("/managed/node/bin".to_string()))
                .into_inner();
        let path = environment
            .iter()
            .find(|(key, _)| key == "PATH")
            .map(|(_, value)| value.as_str());

        assert_eq!(path, Some("/managed/node/bin:/login/bin"));
    }

    #[test]
    fn managed_binary_uses_environment_path() {
        let environment = AcpEnvironment::build(
            vec![env("PATH", "/from/login")],
            &[],
            Some("/managed".to_string()),
        )
        .into_inner();

        assert_eq!(
            environment
                .iter()
                .find(|(key, _)| key == "PATH")
                .map(|(_, value)| value.as_str()),
            Some("/managed:/from/login")
        );
    }

    #[test]
    fn codex_environment_applies_feature_policy_and_routes_shell_commands_through_vmux() {
        let policy = AgentLaunchPolicy::new(
            vec!["Feature-owned agent instruction.".to_string()],
            Vec::new(),
        );
        for agent_id in ["codex", "codex-acp"] {
            let manifest = manifest(AgentKind::Codex);
            let environment = AcpEnvironment::from(Vec::new())
                .for_agent(agent_id, &policy, &manifest)
                .into_inner();
            let config = environment
                .iter()
                .find(|(key, _)| key == "CODEX_CONFIG")
                .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
                .expect("codex ACP compatibility config");

            assert_eq!(config["features"]["shell_tool"], false);
            assert_eq!(config["features"]["unified_exec"], false);
            assert_eq!(config["tools"]["web_search"], false);
            assert_eq!(config["approvals_reviewer"], "user");
            assert_eq!(config["mcp_servers"]["vmux"]["tool_timeout_sec"], 660);
            assert!(
                environment
                    .iter()
                    .all(|(key, _)| key != "DISABLE_MCP_CONFIG_FILTERING")
            );
            assert_eq!(
                config["features"]["code_mode"]["direct_only_tool_namespaces"],
                serde_json::json!(["mcp__vmux"])
            );
            let instructions = config["developer_instructions"].as_str().unwrap();
            assert!(instructions.contains("mcp__vmux__run"));
            assert!(instructions.contains("mcp__vmux__set_conversation_title"));
            assert!(instructions.contains("first tool of the turn"));
            assert!(instructions.contains("raw first prompt as a provisional title"));
            assert!(instructions.contains("topic materially changes"));
            assert!(instructions.contains("same-topic follow-ups"));
            assert!(instructions.contains("never needs user permission"));
            assert!(instructions.contains("Feature-owned agent instruction."));
        }
    }

    #[test]
    fn codex_environment_exposes_managed_namespaces() {
        let manifest = manifest(AgentKind::Codex);
        let environment = AcpEnvironment::from(Vec::new())
            .for_agent("codex-acp", &AgentLaunchPolicy::default(), &manifest)
            .with_managed_servers(
                "codex-acp",
                &manifest.direct_only_namespace,
                ["vmux_linear".to_string(), "vmux_notion".to_string()],
            )
            .into_inner();
        let config = environment
            .iter()
            .find(|(key, _)| key == "CODEX_CONFIG")
            .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
            .expect("codex ACP compatibility config");

        assert_eq!(
            config["features"]["code_mode"]["direct_only_tool_namespaces"],
            serde_json::json!(["mcp__vmux", "mcp__vmux_linear", "mcp__vmux_notion"])
        );
    }

    #[test]
    fn codex_environment_disables_session_skills() {
        let mut config = serde_json::json!({
            "skills": {
                "config": [
                    {"path": "/tmp/knowledge/alpha/SKILL.md", "enabled": true},
                    {"path": "/tmp/other", "enabled": true}
                ]
            }
        })
        .as_object()
        .unwrap()
        .clone();
        AcpEnvironment::disable_codex_skills(
            &mut config,
            &[
                PathBuf::from("/tmp/knowledge/alpha/SKILL.md"),
                PathBuf::from("/tmp/knowledge/beta/SKILL.md"),
            ],
        );

        assert_eq!(config["skills"]["config"][0]["enabled"], false);
        assert_eq!(config["skills"]["config"][1]["enabled"], true);
        assert_eq!(
            config["skills"]["config"][2],
            serde_json::json!({"path": "/tmp/knowledge/beta/SKILL.md", "enabled": false})
        );
    }

    #[test]
    fn claude_environment_extends_mcp_timeout() {
        for agent_id in ["claude", "claude-acp"] {
            let manifest = manifest(AgentKind::Claude);
            let environment = AcpEnvironment::from(vec![env("MCP_TOOL_TIMEOUT", "60000")])
                .for_agent(agent_id, &AgentLaunchPolicy::default(), &manifest)
                .into_inner();
            assert_eq!(
                environment
                    .iter()
                    .find(|(key, _)| key == "MCP_TOOL_TIMEOUT")
                    .map(|(_, value)| value.as_str()),
                Some("660000")
            );
        }
    }

    #[test]
    fn vibe_environment_disables_shell_tool() {
        let manifest = manifest(AgentKind::Vibe);
        let environment = AcpEnvironment::from(vec![
            env("VIBE_DISABLED_TOOLS", r#"["from-env"]"#),
            env(
                "VIBE_MCP_SERVERS",
                r#"[{"name":"from-env","transport":"stdio","command":"env-command"}]"#,
            ),
        ])
        .for_agent("mistral-vibe", &AgentLaunchPolicy::default(), &manifest)
        .into_inner();
        let disabled = environment
            .iter()
            .find(|(key, _)| key == "VIBE_DISABLED_TOOLS")
            .map(|(_, value)| serde_json::from_str::<Vec<String>>(value).unwrap())
            .expect("Vibe ACP disabled tools");

        assert_eq!(disabled, vec!["from-env", "bash"]);
        let mcp_servers = environment
            .iter()
            .find(|(key, _)| key == "VIBE_MCP_SERVERS")
            .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
            .expect("Vibe ACP MCP servers");
        assert_eq!(mcp_servers[0]["name"], "from-env");
    }

    #[test]
    fn vibe_environment_discards_invalid_mcp_configuration() {
        let manifest = manifest(AgentKind::Vibe);
        let environment = AcpEnvironment::from(vec![env("VIBE_MCP_SERVERS", "not-json")])
            .for_agent("mistral-vibe", &AgentLaunchPolicy::default(), &manifest)
            .into_inner();

        assert!(environment.iter().all(|(key, _)| key != "VIBE_MCP_SERVERS"));
    }

    #[test]
    fn codex_environment_preserves_existing_configuration() {
        let manifest = manifest(AgentKind::Codex);
        let environment = AcpEnvironment::from(vec![env(
            "CODEX_CONFIG",
            r#"{"model":"gpt-test","features":{"custom_feature":true,"code_mode":{"custom_setting":"keep"}}}"#,
        )])
        .for_agent("codex", &AgentLaunchPolicy::default(), &manifest)
        .into_inner();
        let config = environment
            .iter()
            .find(|(key, _)| key == "CODEX_CONFIG")
            .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
            .unwrap();

        assert_eq!(config["model"], "gpt-test");
        assert_eq!(config["features"]["custom_feature"], true);
        assert_eq!(config["features"]["code_mode"]["custom_setting"], "keep");
        assert_eq!(config["features"]["shell_tool"], false);
    }

    #[test]
    fn codex_environment_reports_discarded_configuration() {
        let (_, invalid_json) = AcpEnvironment::parse_codex_config(Some("{not-json"));
        assert!(invalid_json.unwrap().contains("invalid JSON"));

        let (_, non_object) = AcpEnvironment::parse_codex_config(Some("[]"));
        assert!(non_object.unwrap().contains("not a JSON object"));
    }
}
