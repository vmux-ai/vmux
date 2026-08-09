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
#[path = "acp_registry.test.rs"]
mod tests;
