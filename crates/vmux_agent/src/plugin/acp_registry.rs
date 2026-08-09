//! The ACP agent registry: fetch, cache, and parse the standardized agent catalog published at
//! <https://agentclientprotocol.com>. The catalog is the single source of truth for agent
//! discovery, install specs (`distribution`), versions, and icons; vmux consumes it like any
//! other ACP client rather than hardcoding agents.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Canonical registry endpoint. Clients fetch this one JSON document and filter locally.
pub const REGISTRY_URL: &str =
    "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

/// The registry document: `{ version, agents: [...] }`.
#[derive(Debug, Clone, Deserialize)]
pub struct Registry {
    pub version: String,
    #[serde(default)]
    pub agents: Vec<RegistryAgent>,
}

/// One agent entry (an aggregated `<id>/agent.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryAgent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    pub distribution: Distribution,
}

/// How an agent is delivered. A manifest may list several variants (e.g. `binary` + `npx`);
/// vmux prefers `binary` (no runtime), then `npx` (managed Node), then `uvx` (managed uv).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Distribution {
    #[serde(default)]
    pub binary: Option<BTreeMap<String, BinaryTarget>>,
    #[serde(default)]
    pub npx: Option<PackageDist>,
    #[serde(default)]
    pub uvx: Option<PackageDist>,
}

/// A per-platform native binary archive, keyed by ACP platform target (`darwin-aarch64`, …).
#[derive(Debug, Clone, Deserialize)]
pub struct BinaryTarget {
    pub archive: String,
    pub cmd: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// An `npx` (Node) or `uvx` (Python-via-uv) package distribution.
#[derive(Debug, Clone, Deserialize)]
pub struct PackageDist {
    pub package: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// The runtime an agent's chosen distribution needs before it can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// Native binary — no runtime.
    None,
    /// `npx` — needs a (managed) Node.
    Node,
    /// `uvx` — needs a (managed) uv/Python.
    Uv,
}

impl RegistryAgent {
    /// The ACP platform target for the current host, matching the registry's `binary` keys.
    /// `None` on unsupported host tuples.
    pub fn host_target() -> Option<&'static str> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => Some("darwin-aarch64"),
            ("macos", "x86_64") => Some("darwin-x86_64"),
            ("linux", "aarch64") => Some("linux-aarch64"),
            ("linux", "x86_64") => Some("linux-x86_64"),
            ("windows", "aarch64") => Some("windows-aarch64"),
            ("windows", "x86_64") => Some("windows-x86_64"),
            _ => None,
        }
    }

    /// The native binary target for the current host, if this agent ships one.
    pub fn binary_for_host(&self) -> Option<&BinaryTarget> {
        self.distribution.binary.as_ref()?.get(Self::host_target()?)
    }

    /// The runtime vmux would use to run this agent, preferring a host-native binary.
    pub fn preferred_runtime(&self) -> Runtime {
        if self.binary_for_host().is_some() {
            Runtime::None
        } else if self.distribution.npx.is_some() {
            Runtime::Node
        } else if self.distribution.uvx.is_some() {
            Runtime::Uv
        } else {
            Runtime::None
        }
    }
}

/// Runtime store for the cached registry and installed agents.
pub fn agents_dir() -> PathBuf {
    vmux_core::profile::agents_dir()
}

/// Path of the cached registry document.
pub fn cache_path() -> PathBuf {
    agents_dir().join("registry.json")
}

/// Parse a registry document from JSON.
pub fn parse(json: &str) -> Result<Registry, String> {
    serde_json::from_str(json).map_err(|e| format!("acp registry: parse failed: {e}"))
}

/// Load the cached registry, if present and parseable.
pub fn load_cached() -> Option<Registry> {
    parse(&std::fs::read_to_string(cache_path()).ok()?).ok()
}

/// Fetch the registry over the network (blocking) and write it to the cache. Run this on a
/// background thread, not the Bevy schedule.
pub fn fetch_blocking() -> Result<Registry, String> {
    let text = reqwest::blocking::get(REGISTRY_URL)
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| format!("acp registry: fetch failed: {e}"))?;
    let registry = parse(&text)?;
    let dir = agents_dir();
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(cache_path(), &text);
    }
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "version": "1.0.0",
      "agents": [
        {
          "id": "claude-acp",
          "name": "Claude Agent",
          "version": "0.5.0",
          "icon": "https://cdn.example/claude-acp.svg",
          "distribution": {
            "npx": { "package": "@agentclientprotocol/claude-agent-acp", "args": ["--acp"] }
          }
        },
        {
          "id": "mistral-vibe",
          "name": "Mistral Vibe",
          "distribution": {
            "binary": {
              "darwin-aarch64": { "archive": "https://cdn.example/vibe-darwin-arm64.tar.gz", "cmd": "./vibe", "args": ["acp"] },
              "linux-x86_64":  { "archive": "https://cdn.example/vibe-linux-x64.tar.gz",  "cmd": "./vibe", "args": ["acp"] }
            }
          }
        },
        {
          "id": "fast-agent",
          "name": "fast-agent",
          "distribution": { "uvx": { "package": "fast-agent-acp", "args": ["serve"] } }
        }
      ]
    }"#;

    #[test]
    fn parses_all_distribution_types() {
        let reg = parse(SAMPLE).unwrap();
        assert_eq!(reg.version, "1.0.0");
        assert_eq!(reg.agents.len(), 3);

        let claude = &reg.agents[0];
        assert_eq!(claude.id, "claude-acp");
        assert_eq!(
            claude.icon.as_deref(),
            Some("https://cdn.example/claude-acp.svg")
        );
        assert_eq!(
            claude.distribution.npx.as_ref().unwrap().package,
            "@agentclientprotocol/claude-agent-acp"
        );
        assert!(claude.distribution.binary.is_none());

        let vibe = &reg.agents[1];
        assert!(
            vibe.distribution
                .binary
                .as_ref()
                .unwrap()
                .contains_key("linux-x86_64")
        );

        let fast = &reg.agents[2];
        assert_eq!(
            fast.distribution.uvx.as_ref().unwrap().package,
            "fast-agent-acp"
        );
    }

    #[test]
    fn host_target_matches_arch() {
        let t = RegistryAgent::host_target();
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert_eq!(t, Some("darwin-aarch64"));
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        assert_eq!(t, Some("linux-x86_64"));
        let _ = t;
    }

    #[test]
    fn preferred_runtime_prefers_binary_then_node_then_uv() {
        let reg = parse(SAMPLE).unwrap();
        // claude: npx only -> Node.
        assert_eq!(reg.agents[0].preferred_runtime(), Runtime::Node);
        // fast-agent: uvx only -> Uv.
        assert_eq!(reg.agents[2].preferred_runtime(), Runtime::Uv);
        // vibe: binary — on a host the sample covers, no runtime.
        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        ))]
        assert_eq!(reg.agents[1].preferred_runtime(), Runtime::None);
    }

    #[test]
    fn binary_for_host_resolves_matching_target() {
        let reg = parse(SAMPLE).unwrap();
        let vibe = &reg.agents[1];
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            let bin = vibe.binary_for_host().unwrap();
            assert_eq!(bin.cmd, "./vibe");
            assert_eq!(bin.args, vec!["acp".to_string()]);
        }
        let _ = vibe;
    }
}
