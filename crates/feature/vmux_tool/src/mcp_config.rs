use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use bevy_ecs::prelude::Component;
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{ManagedMcpServer, ManagedMcpTransport};

use crate::{McpManifest, McpServerManifest, McpTransport};

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConfigFile {
    #[serde(default = "McpConfigFile::default_version")]
    pub version: u32,
    #[serde(default)]
    pub servers: BTreeMap<String, McpServerOverride>,
}

impl Default for McpConfigFile {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            servers: BTreeMap::new(),
        }
    }
}

impl McpConfigFile {
    const VERSION: u32 = 1;

    fn default_version() -> u32 {
        Self::VERSION
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        let mut config: Self = ron::from_str(source).map_err(|error| error.to_string())?;
        if config.version != Self::VERSION {
            return Err(format!(
                "unsupported MCP config version: {}",
                config.version
            ));
        }
        config.normalize();
        Ok(config)
    }

    pub fn encode(&self) -> Result<String, String> {
        let mut config = self.clone();
        config.version = Self::VERSION;
        config.normalize();
        let pretty = ron::ser::PrettyConfig::new()
            .struct_names(false)
            .separate_tuple_members(true);
        let mut text =
            ron::ser::to_string_pretty(&config, pretty).map_err(|error| error.to_string())?;
        text.push('\n');
        Ok(text)
    }

    pub fn normalize(&mut self) {
        self.servers
            .retain(|id, _| !id.trim().is_empty() && id != "vmux");
    }

    pub fn from_legacy(manifest: &McpManifest) -> Self {
        let mut servers = BTreeMap::new();
        for (id, server) in &manifest.servers {
            if id == "vmux" || id.trim().is_empty() {
                continue;
            }
            let legacy: &McpServerManifest = server;
            if let Some(spec) = McpServerSpec::from_legacy(legacy) {
                servers.insert(id.clone(), McpServerOverride::Define(spec));
            }
        }
        Self {
            version: Self::VERSION,
            servers,
        }
    }

    pub fn validate_server_id(id: &str) -> Result<String, String> {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err("MCP server ID is required".to_string());
        }
        if trimmed == "vmux" {
            return Err("MCP server ID \"vmux\" is reserved".to_string());
        }
        if trimmed.contains('\0') {
            return Err("MCP server ID contains an invalid character".to_string());
        }
        Ok(trimmed.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpServerOverride {
    Enable,
    Disable,
    Define(McpServerSpec),
    Patch(McpServerPatch),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpServerSpec {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default = "McpServerSpec::default_enabled")]
        enabled: bool,
        #[serde(default)]
        agents: Vec<String>,
    },
    Http {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        header_env: BTreeMap<String, String>,
        #[serde(default)]
        bearer_token_env_var: Option<String>,
        #[serde(default)]
        auth: McpAuthSpec,
        #[serde(default = "McpServerSpec::default_enabled")]
        enabled: bool,
        #[serde(default)]
        agents: Vec<String>,
    },
    Sse {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        header_env: BTreeMap<String, String>,
        #[serde(default)]
        bearer_token_env_var: Option<String>,
        #[serde(default)]
        auth: McpAuthSpec,
        #[serde(default = "McpServerSpec::default_enabled")]
        enabled: bool,
        #[serde(default)]
        agents: Vec<String>,
    },
}

impl McpServerSpec {
    fn default_enabled() -> bool {
        true
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Stdio { command, .. } => {
                if command.trim().is_empty() {
                    return Err("stdio MCP server requires a command".to_string());
                }
                Ok(())
            }
            Self::Http { url, .. } | Self::Sse { url, .. } => {
                let parsed = url::Url::parse(url.trim())
                    .map_err(|error| format!("invalid MCP server URL: {error}"))?;
                if !matches!(parsed.scheme(), "http" | "https") {
                    return Err("remote MCP server URL must use http or https".to_string());
                }
                Ok(())
            }
        }
    }

    pub fn from_legacy(manifest: &McpServerManifest) -> Option<Self> {
        match manifest.transport {
            McpTransport::Stdio => Some(Self::Stdio {
                command: manifest.command.clone()?,
                args: manifest.args.clone(),
                env: manifest.env.clone(),
                cwd: manifest.cwd.clone(),
                enabled: true,
                agents: Vec::new(),
            }),
            McpTransport::Http => Some(Self::Http {
                url: manifest.url.clone()?,
                headers: manifest.headers.clone(),
                header_env: manifest.header_env.clone(),
                bearer_token_env_var: manifest.bearer_token_env_var.clone(),
                auth: McpAuthSpec::None,
                enabled: true,
                agents: Vec::new(),
            }),
            McpTransport::Sse => Some(Self::Sse {
                url: manifest.url.clone()?,
                headers: manifest.headers.clone(),
                header_env: manifest.header_env.clone(),
                bearer_token_env_var: manifest.bearer_token_env_var.clone(),
                auth: McpAuthSpec::None,
                enabled: true,
                agents: Vec::new(),
            }),
        }
    }

    pub fn is_enabled(&self) -> bool {
        match self {
            Self::Stdio { enabled, .. }
            | Self::Http { enabled, .. }
            | Self::Sse { enabled, .. } => *enabled,
        }
    }

    pub fn set_enabled(&mut self, value: bool) {
        match self {
            Self::Stdio { enabled, .. }
            | Self::Http { enabled, .. }
            | Self::Sse { enabled, .. } => *enabled = value,
        }
    }

    pub fn matches_agent(&self, agent_id: &str) -> bool {
        let agents = match self {
            Self::Stdio { agents, .. } | Self::Http { agents, .. } | Self::Sse { agents, .. } => {
                agents
            }
        };
        agents.is_empty() || agents.iter().any(|candidate| candidate == agent_id)
    }

    pub fn apply_patch(&mut self, patch: &McpServerPatch) {
        if let Some(value) = patch.enabled {
            self.set_enabled(value);
        }
        match self {
            Self::Stdio {
                args, env, agents, ..
            } => {
                if let Some(next_args) = &patch.args {
                    *args = next_args.clone();
                }
                for (key, value) in &patch.env {
                    env.insert(key.clone(), value.clone());
                }
                if let Some(next_agents) = &patch.agents {
                    *agents = next_agents.clone();
                }
            }
            Self::Http {
                headers,
                header_env,
                bearer_token_env_var,
                agents,
                ..
            }
            | Self::Sse {
                headers,
                header_env,
                bearer_token_env_var,
                agents,
                ..
            } => {
                for (key, value) in &patch.headers {
                    headers.insert(key.clone(), value.clone());
                }
                for (key, value) in &patch.header_env {
                    header_env.insert(key.clone(), value.clone());
                }
                if let Some(variable) = &patch.bearer_token_env_var {
                    *bearer_token_env_var = Some(variable.clone());
                }
                if let Some(next_agents) = &patch.agents {
                    *agents = next_agents.clone();
                }
            }
        }
    }

    pub fn transport_label(&self) -> &'static str {
        match self {
            Self::Stdio { .. } => "stdio",
            Self::Http { .. } => "http",
            Self::Sse { .. } => "sse",
        }
    }

    pub fn target_summary(&self) -> String {
        match self {
            Self::Stdio { command, args, .. } => {
                if args.is_empty() {
                    command.clone()
                } else {
                    format!("{command} {}", args.join(" "))
                }
            }
            Self::Http { url, .. } | Self::Sse { url, .. } => url.clone(),
        }
    }

    pub fn to_managed(
        &self,
        name: &str,
        env_lookup: impl Fn(&str) -> Option<String>,
    ) -> ManagedMcpServer {
        match self {
            Self::Stdio {
                command,
                args,
                env,
                cwd,
                ..
            } => ManagedMcpServer {
                name: name.to_string(),
                transport: ManagedMcpTransport::Stdio,
                command: Some(command.clone()),
                args: args.clone(),
                env: env
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
                cwd: cwd.clone(),
                url: None,
                headers: Vec::new(),
            },
            Self::Http {
                url,
                headers,
                header_env,
                bearer_token_env_var,
                ..
            } => ManagedMcpServer {
                name: name.to_string(),
                transport: ManagedMcpTransport::Http,
                command: None,
                args: Vec::new(),
                env: Vec::new(),
                cwd: None,
                url: Some(url.clone()),
                headers: Self::resolve_headers(
                    headers,
                    header_env,
                    bearer_token_env_var.as_deref(),
                    env_lookup,
                ),
            },
            Self::Sse {
                url,
                headers,
                header_env,
                bearer_token_env_var,
                ..
            } => ManagedMcpServer {
                name: name.to_string(),
                transport: ManagedMcpTransport::Sse,
                command: None,
                args: Vec::new(),
                env: Vec::new(),
                cwd: None,
                url: Some(url.clone()),
                headers: Self::resolve_headers(
                    headers,
                    header_env,
                    bearer_token_env_var.as_deref(),
                    env_lookup,
                ),
            },
        }
    }

    fn resolve_headers(
        headers: &BTreeMap<String, String>,
        header_env: &BTreeMap<String, String>,
        bearer_token_env_var: Option<&str>,
        env_lookup: impl Fn(&str) -> Option<String>,
    ) -> Vec<(String, String)> {
        let mut resolved = headers.clone();
        for (header, variable) in header_env {
            if let Some(value) = env_lookup(variable) {
                resolved.insert(header.clone(), value);
            }
        }
        if let Some(variable) = bearer_token_env_var
            && let Some(value) = env_lookup(variable)
        {
            resolved.insert("Authorization".to_string(), format!("Bearer {value}"));
        }
        resolved.into_iter().collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerPatch {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub header_env: BTreeMap<String, String>,
    #[serde(default)]
    pub bearer_token_env_var: Option<String>,
    #[serde(default)]
    pub agents: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpAuthSpec {
    #[default]
    None,
    OAuth {
        #[serde(default)]
        scopes: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum McpConfigScope {
    LegacyTools,
    #[default]
    Global,
    Profile,
    Project,
    ProjectLocal,
}

impl McpConfigScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::LegacyTools => "legacy tools.toml",
            Self::Global => "global · mcp.ron",
            Self::Profile => "profile · mcp.ron",
            Self::Project => "project · .vmux/mcp.ron",
            Self::ProjectLocal => "local · .vmux/mcp.local.ron",
        }
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpTrustStore {
    #[serde(default = "McpConfigFile::default_version")]
    pub version: u32,
    #[serde(default)]
    pub repositories: BTreeMap<String, BTreeMap<String, String>>,
}

impl McpTrustStore {
    pub fn parse(source: &str) -> Result<Self, String> {
        let store: Self = ron::from_str(source).map_err(|error| error.to_string())?;
        if store.version != McpConfigFile::VERSION {
            return Err(format!("unsupported MCP trust version: {}", store.version));
        }
        Ok(store)
    }

    pub fn encode(&self) -> Result<String, String> {
        let mut store = self.clone();
        store.version = McpConfigFile::VERSION;
        let pretty = ron::ser::PrettyConfig::new()
            .struct_names(false)
            .separate_tuple_members(true);
        let mut text =
            ron::ser::to_string_pretty(&store, pretty).map_err(|error| error.to_string())?;
        text.push('\n');
        Ok(text)
    }

    pub fn is_trusted(&self, repo_root: &str, server_id: &str, expected: &str) -> bool {
        self.repositories
            .get(repo_root)
            .and_then(|servers| servers.get(server_id))
            .is_some_and(|saved| saved == expected)
    }

    pub fn trust(&mut self, repo_root: String, server_id: String, entry_digest: String) {
        self.repositories
            .entry(repo_root)
            .or_default()
            .insert(server_id, entry_digest);
    }

    pub fn digest_entry(entry: &McpServerOverride) -> Result<String, String> {
        let canonical = ron::to_string(entry).map_err(|error| error.to_string())?;
        let hash = digest(&SHA256, canonical.as_bytes());
        let mut hex = String::with_capacity(hash.as_ref().len() * 2);
        for byte in hash.as_ref() {
            use std::fmt::Write;
            let _ = write!(&mut hex, "{byte:02x}");
        }
        Ok(hex)
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct McpWorkspaceRoot {
    pub checkout_root: PathBuf,
    pub primary_root: PathBuf,
}

impl McpWorkspaceRoot {
    pub fn resolve(cwd: &Path) -> Option<Self> {
        let mut current = if cwd.is_file() { cwd.parent()? } else { cwd };
        loop {
            let dot_git = current.join(".git");
            if fs::symlink_metadata(&dot_git).is_ok() {
                let checkout_root = current
                    .canonicalize()
                    .unwrap_or_else(|_| current.to_path_buf());
                let primary_root = Self::primary_for_checkout(&checkout_root);
                return Some(Self {
                    checkout_root,
                    primary_root,
                });
            }
            if current.join(".vmux/mcp.ron").is_file()
                || current.join(".vmux/mcp.local.ron").is_file()
            {
                let checkout_root = current
                    .canonicalize()
                    .unwrap_or_else(|_| current.to_path_buf());
                return Some(Self {
                    primary_root: checkout_root.clone(),
                    checkout_root,
                });
            }
            current = current.parent()?;
        }
    }

    fn primary_for_checkout(checkout: &Path) -> PathBuf {
        let dot_git = checkout.join(".git");
        let Ok(metadata) = fs::symlink_metadata(&dot_git) else {
            return checkout.to_path_buf();
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return checkout.to_path_buf();
        }
        let Ok(text) = fs::read_to_string(&dot_git) else {
            return checkout.to_path_buf();
        };
        let Some(gitdir) = text.trim().strip_prefix("gitdir:").map(str::trim) else {
            return checkout.to_path_buf();
        };
        let gitdir = Self::resolve_relative(checkout, gitdir);
        let Ok(gitdir) = gitdir.canonicalize() else {
            return checkout.to_path_buf();
        };
        let commondir_file = gitdir.join("commondir");
        let Ok(meta) = fs::symlink_metadata(&commondir_file) else {
            return checkout.to_path_buf();
        };
        if !meta.is_file() || meta.file_type().is_symlink() {
            return checkout.to_path_buf();
        }
        let Ok(commondir) = fs::read_to_string(&commondir_file) else {
            return checkout.to_path_buf();
        };
        let common = Self::resolve_relative(&gitdir, commondir.trim());
        let Ok(common) = common.canonicalize() else {
            return checkout.to_path_buf();
        };
        Self::primary_from_common(&common).unwrap_or_else(|| checkout.to_path_buf())
    }

    fn resolve_relative(base: &Path, target: &str) -> PathBuf {
        let path = Path::new(target);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            base.join(path)
        }
    }

    fn primary_from_common(common: &Path) -> Option<PathBuf> {
        if common.file_name() == Some(OsStr::new(".git")) {
            let parent = common.parent()?;
            let metadata = fs::symlink_metadata(parent).ok()?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                return parent.canonicalize().ok();
            }
            return None;
        }
        for name in ["config.worktree", "config"] {
            let Ok(text) = fs::read_to_string(common.join(name)) else {
                continue;
            };
            let mut in_core = false;
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    let section = trimmed[1..trimmed.len() - 1].trim();
                    in_core = section.eq_ignore_ascii_case("core");
                    continue;
                }
                if !in_core {
                    continue;
                }
                let Some((key, value)) = trimmed.split_once('=') else {
                    continue;
                };
                if !key.trim().eq_ignore_ascii_case("worktree") {
                    continue;
                }
                let value = value.trim().trim_matches('"');
                if value.is_empty() {
                    continue;
                }
                let candidate = Self::resolve_relative(common, value);
                let Ok(metadata) = fs::symlink_metadata(&candidate) else {
                    continue;
                };
                if metadata.is_dir()
                    && !metadata.file_type().is_symlink()
                    && let Ok(canonical) = candidate.canonicalize()
                {
                    return Some(canonical);
                }
            }
        }
        None
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct McpResolvedServer {
    pub id: String,
    pub spec: McpServerSpec,
    pub source: McpConfigScope,
    pub enabled: bool,
    pub trusted: bool,
    pub repo_root: Option<String>,
    pub digest: Option<String>,
}

impl McpResolvedServer {
    pub fn to_managed_for_agent(
        &self,
        agent_id: &str,
        env_lookup: impl Fn(&str) -> Option<String>,
    ) -> Option<ManagedMcpServer> {
        if !self.enabled || !self.trusted || !self.spec.matches_agent(agent_id) {
            return None;
        }
        Some(self.spec.to_managed(&self.id, env_lookup))
    }

    pub fn detail(&self, external_sources: &str, conflict: bool) -> String {
        let transport = self.spec.transport_label();
        let target = self.spec.target_summary();
        let scope = self.source.label();
        let mut parts = vec![transport.to_string(), scope.to_string()];
        if !target.is_empty() {
            parts.push(target);
        }
        if !self.trusted {
            parts.push("untrusted".to_string());
        } else if !self.enabled {
            parts.push("disabled".to_string());
        }
        if conflict {
            parts.push(format!("conflicts in {external_sources}"));
        } else if !external_sources.is_empty() {
            parts.push(format!("also in {external_sources}"));
        }
        parts.join(" · ")
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct McpLayerInput {
    pub legacy: McpConfigFile,
    pub global: McpConfigFile,
    pub profile: McpConfigFile,
    pub project: McpConfigFile,
    pub primary_local: McpConfigFile,
    pub worktree_local: McpConfigFile,
    pub trust: McpTrustStore,
    pub repo_root: Option<String>,
    pub diagnostics: Vec<String>,
}

impl McpLayerInput {
    pub fn resolve_managed(
        &self,
        agent_id: &str,
        env_lookup: impl Fn(&str) -> Option<String>,
    ) -> (Vec<McpResolvedServer>, Vec<ManagedMcpServer>) {
        let servers = self.resolve();
        let mut managed = Vec::new();
        for server in &servers {
            if let Some(item) = server.to_managed_for_agent(agent_id, &env_lookup) {
                managed.push(item);
            }
        }
        (servers, managed)
    }

    pub fn resolve(&self) -> Vec<McpResolvedServer> {
        let mut resolved = BTreeMap::<String, McpResolvedServer>::new();
        self.apply_layer(
            &mut resolved,
            &self.legacy,
            McpConfigScope::LegacyTools,
            false,
        );
        self.apply_layer(&mut resolved, &self.global, McpConfigScope::Global, false);
        self.apply_layer(&mut resolved, &self.profile, McpConfigScope::Profile, false);
        self.apply_layer(&mut resolved, &self.project, McpConfigScope::Project, true);
        self.apply_layer(
            &mut resolved,
            &self.primary_local,
            McpConfigScope::ProjectLocal,
            false,
        );
        self.apply_layer(
            &mut resolved,
            &self.worktree_local,
            McpConfigScope::ProjectLocal,
            false,
        );
        resolved.into_values().collect()
    }

    fn apply_layer(
        &self,
        resolved: &mut BTreeMap<String, McpResolvedServer>,
        layer: &McpConfigFile,
        scope: McpConfigScope,
        trust_gated: bool,
    ) {
        for (id, entry) in &layer.servers {
            if id == "vmux" || id.trim().is_empty() {
                continue;
            }
            match entry {
                McpServerOverride::Define(spec) => {
                    if spec.validate().is_err() {
                        continue;
                    }
                    let (trusted, digest, repo_root) = if trust_gated {
                        let entry_digest = McpTrustStore::digest_entry(entry).ok();
                        let approved = match (&self.repo_root, &entry_digest) {
                            (Some(repo), Some(digest)) => self.trust.is_trusted(repo, id, digest),
                            _ => false,
                        };
                        (approved, entry_digest, self.repo_root.clone())
                    } else {
                        (true, None, None)
                    };
                    resolved.insert(
                        id.clone(),
                        McpResolvedServer {
                            id: id.clone(),
                            enabled: spec.is_enabled(),
                            spec: spec.clone(),
                            source: scope,
                            trusted,
                            repo_root,
                            digest,
                        },
                    );
                }
                McpServerOverride::Enable => {
                    if let Some(existing) = resolved.get_mut(id) {
                        existing.enabled = true;
                        existing.spec.set_enabled(true);
                        existing.source = scope;
                    }
                }
                McpServerOverride::Disable => {
                    if let Some(existing) = resolved.get_mut(id) {
                        existing.enabled = false;
                        existing.spec.set_enabled(false);
                        existing.source = scope;
                    }
                }
                McpServerOverride::Patch(patch) => {
                    let Some(existing) = resolved.get_mut(id) else {
                        continue;
                    };
                    existing.spec.apply_patch(patch);
                    existing.enabled = existing.spec.is_enabled();
                    existing.source = scope;
                    if trust_gated {
                        let entry_digest = McpTrustStore::digest_entry(entry).ok();
                        let approved = match (&self.repo_root, &entry_digest) {
                            (Some(repo), Some(digest)) => self.trust.is_trusted(repo, id, digest),
                            _ => false,
                        };
                        existing.trusted = approved;
                        existing.digest = entry_digest;
                        existing.repo_root = self.repo_root.clone();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl McpServerSpec {
        fn test_http(url: &str) -> Self {
            Self::Http {
                url: url.to_string(),
                headers: BTreeMap::new(),
                header_env: BTreeMap::new(),
                bearer_token_env_var: None,
                auth: McpAuthSpec::None,
                enabled: true,
                agents: Vec::new(),
            }
        }

        fn test_stdio(command: &str) -> Self {
            Self::Stdio {
                command: command.to_string(),
                args: vec!["--flag".to_string()],
                env: BTreeMap::new(),
                cwd: None,
                enabled: true,
                agents: Vec::new(),
            }
        }
    }

    #[test]
    fn ron_round_trip_strips_reserved_vmux_and_preserves_overrides() {
        let mut config = McpConfigFile::default();
        config.servers.insert(
            "vmux".to_string(),
            McpServerOverride::Define(McpServerSpec::test_stdio("evil")),
        );
        config.servers.insert(
            "linear-vmx".to_string(),
            McpServerOverride::Define(McpServerSpec::test_http("https://mcp.linear.app/mcp")),
        );
        config
            .servers
            .insert("linear-mistral".to_string(), McpServerOverride::Disable);

        let encoded = config.encode().unwrap();
        let decoded = McpConfigFile::parse(&encoded).unwrap();
        assert!(!decoded.servers.contains_key("vmux"));
        assert_eq!(
            decoded.servers.get("linear-vmx"),
            Some(&McpServerOverride::Define(McpServerSpec::test_http(
                "https://mcp.linear.app/mcp"
            )))
        );
        assert_eq!(
            decoded.servers.get("linear-mistral"),
            Some(&McpServerOverride::Disable)
        );
    }

    #[test]
    fn layered_resolution_applies_precedence_and_trust_gating() {
        let mut input = McpLayerInput {
            repo_root: Some("/repos/app".to_string()),
            ..Default::default()
        };
        input.global.servers.insert(
            "linear-mistral".to_string(),
            McpServerOverride::Define(McpServerSpec::test_http("https://mcp.linear.app/mcp")),
        );
        input.global.servers.insert(
            "shared".to_string(),
            McpServerOverride::Define(McpServerSpec::test_stdio("npx")),
        );
        let project_entry =
            McpServerOverride::Define(McpServerSpec::test_stdio("./scripts/repo-mcp"));
        input
            .project
            .servers
            .insert("repo-docs".to_string(), project_entry.clone());
        input
            .project
            .servers
            .insert("linear-mistral".to_string(), McpServerOverride::Disable);
        input.primary_local.servers.insert(
            "shared".to_string(),
            McpServerOverride::Patch(McpServerPatch {
                env: BTreeMap::from([("MODE".to_string(), "local".to_string())]),
                ..Default::default()
            }),
        );

        let before_trust = input.resolve();
        let linear = before_trust
            .iter()
            .find(|server| server.id == "linear-mistral")
            .unwrap();
        assert!(!linear.enabled);
        assert!(linear.trusted);
        assert_eq!(linear.source, McpConfigScope::Project);

        let repo_docs = before_trust
            .iter()
            .find(|server| server.id == "repo-docs")
            .unwrap();
        assert!(!repo_docs.trusted);
        assert!(
            repo_docs
                .to_managed_for_agent("claude-acp", |_| None)
                .is_none()
        );

        let digest = McpTrustStore::digest_entry(&project_entry).unwrap();
        input
            .trust
            .trust("/repos/app".to_string(), "repo-docs".to_string(), digest);
        let after_trust = input.resolve();
        let repo_docs = after_trust
            .iter()
            .find(|server| server.id == "repo-docs")
            .unwrap();
        assert!(repo_docs.trusted);
        assert!(
            repo_docs
                .to_managed_for_agent("claude-acp", |_| None)
                .is_some()
        );

        input.project.servers.insert(
            "repo-docs".to_string(),
            McpServerOverride::Define(McpServerSpec::test_stdio("./scripts/changed-mcp")),
        );
        let mutated = input.resolve();
        let repo_docs = mutated
            .iter()
            .find(|server| server.id == "repo-docs")
            .unwrap();
        assert!(!repo_docs.trusted);
    }

    #[test]
    fn project_local_enable_cannot_bypass_untrusted_project_definition() {
        let mut input = McpLayerInput {
            repo_root: Some("/repos/app".to_string()),
            ..Default::default()
        };
        input.project.servers.insert(
            "repo-tool".to_string(),
            McpServerOverride::Define(McpServerSpec::test_stdio("./untrusted")),
        );
        input
            .primary_local
            .servers
            .insert("repo-tool".to_string(), McpServerOverride::Enable);
        let resolved = input.resolve();
        assert!(!resolved[0].trusted);
    }

    #[test]
    fn workspace_root_resolves_linked_worktree_to_primary_repo() {
        let dir = tempfile::tempdir().unwrap();
        let primary = dir.path().join("primary");
        let worktree = primary.join(".worktrees/feat");
        let git_common = primary.join(".git");
        let git_worktree = git_common.join("worktrees/feat");
        fs::create_dir_all(&git_worktree).unwrap();
        fs::create_dir_all(worktree.join("nested/sub")).unwrap();
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", git_worktree.display()),
        )
        .unwrap();
        fs::write(git_worktree.join("commondir"), "../..\n").unwrap();

        let root = McpWorkspaceRoot::resolve(&worktree.join("nested/sub")).unwrap();
        assert_eq!(root.checkout_root, worktree.canonicalize().unwrap());
        assert_eq!(root.primary_root, primary.canonicalize().unwrap());
    }

    #[test]
    fn resolved_headers_inject_env_and_bearer_token() {
        let spec = McpServerSpec::Http {
            url: "https://mcp.linear.app/mcp".to_string(),
            headers: BTreeMap::from([("X-Static".to_string(), "1".to_string())]),
            header_env: BTreeMap::from([("X-Env".to_string(), "ENV_VAL".to_string())]),
            bearer_token_env_var: Some("LINEAR_TOKEN".to_string()),
            auth: McpAuthSpec::None,
            enabled: true,
            agents: vec!["claude-acp".to_string()],
        };
        let resolved = McpResolvedServer {
            id: "linear-vmx".to_string(),
            spec,
            source: McpConfigScope::Global,
            enabled: true,
            trusted: true,
            repo_root: None,
            digest: None,
        };
        assert!(
            resolved
                .to_managed_for_agent("codex-acp", |_| None)
                .is_none()
        );
        let managed = resolved
            .to_managed_for_agent("claude-acp", |key| match key {
                "ENV_VAL" => Some("from-env".to_string()),
                "LINEAR_TOKEN" => Some("secret".to_string()),
                _ => None,
            })
            .unwrap();
        assert_eq!(managed.name, "linear-vmx");
        assert_eq!(managed.transport, ManagedMcpTransport::Http);
        assert_eq!(
            managed.headers,
            vec![
                ("Authorization".to_string(), "Bearer secret".to_string()),
                ("X-Env".to_string(), "from-env".to_string()),
                ("X-Static".to_string(), "1".to_string()),
            ]
        );
    }
}
