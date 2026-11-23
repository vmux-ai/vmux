use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::Deserialize;
use vmux_editor::lsp::package_path::Sha256Digest;

pub(in crate::host) use super::registry_catalog::RegistryPlugin;
pub(in crate::host::acp) use super::registry_catalog::Source as CatalogSource;
pub(super) use super::registry_catalog::{Catalog, RefreshRequest};

#[derive(Debug, Clone, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub agents: Vec<RegistryAgent>,
}

#[derive(Component, Debug, Clone, Deserialize)]
pub struct RegistryAgent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    pub distribution: Distribution,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Distribution {
    #[serde(default)]
    pub binary: Option<BTreeMap<String, BinaryTarget>>,
    #[serde(default)]
    pub npx: Option<PackageDist>,
    #[serde(default)]
    pub uvx: Option<PackageDist>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinaryTarget {
    pub archive: String,
    pub cmd: String,
    #[serde(default, deserialize_with = "BinaryTarget::deserialize_sha256")]
    pub sha256: Option<Sha256Digest>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackageDist {
    pub package: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    None,
    Node,
    Uv,
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
        let reg: Registry = SAMPLE.parse().unwrap();
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

        let checksummed: Registry = r#"{"version":"1","agents":[{"id":"agent","name":"Agent","distribution":{"binary":{"darwin-aarch64":{"archive":"https://example.com/agent","cmd":"agent","sha256":"ed16a0c68a7df1e55597fcb7c884140ce292def6116cbaab1fc05045433494b9"}}}}]}"#
            .parse()
            .unwrap();
        assert_eq!(
            checksummed.agents[0].distribution.binary.as_ref().unwrap()["darwin-aarch64"]
                .sha256
                .as_ref()
                .unwrap()
                .as_str(),
            "ed16a0c68a7df1e55597fcb7c884140ce292def6116cbaab1fc05045433494b9"
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
        let reg: Registry = SAMPLE.parse().unwrap();
        assert_eq!(reg.agents[0].preferred_runtime(), Runtime::Node);
        assert_eq!(reg.agents[2].preferred_runtime(), Runtime::Uv);
        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        ))]
        assert_eq!(reg.agents[1].preferred_runtime(), Runtime::None);
    }

    #[test]
    fn binary_for_host_resolves_matching_target() {
        let reg: Registry = SAMPLE.parse().unwrap();
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
