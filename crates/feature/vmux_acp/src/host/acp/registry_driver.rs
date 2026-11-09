use std::path::PathBuf;

use serde::Deserialize;
use vmux_editor::lsp::package_path::Sha256Digest;

use super::registry::{BinaryTarget, Registry, RegistryAgent, Runtime};

impl std::str::FromStr for Registry {
    type Err = String;

    fn from_str(json: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(json).map_err(|error| format!("acp registry: parse failed: {error}"))
    }
}

impl Registry {
    pub fn agent(self, id: &str) -> Option<RegistryAgent> {
        self.agents.into_iter().find(|agent| agent.id == id)
    }

    pub fn cached() -> Option<Self> {
        std::fs::read_to_string(Self::cache_path())
            .ok()?
            .parse()
            .ok()
    }

    pub fn fetch_blocking(url: &str) -> Result<Self, String> {
        let text = reqwest::blocking::get(url)
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.text())
            .map_err(|error| format!("acp registry: fetch failed: {error}"))?;
        let registry = text.parse()?;
        let dir = vmux_ecs::profile::ProfilePaths::current().agents();
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = vmux_path::AtomicFile::write(Self::cache_path(), text.as_bytes());
        }
        Ok(registry)
    }

    fn cache_path() -> PathBuf {
        vmux_ecs::profile::ProfilePaths::current()
            .agents()
            .join("registry.json")
    }
}

impl RegistryAgent {
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

    pub fn binary_for_host(&self) -> Option<&BinaryTarget> {
        self.distribution.binary.as_ref()?.get(Self::host_target()?)
    }

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

    #[cfg(test)]
    pub(crate) fn test(id: &str, name: &str, icon: Option<&str>) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            version: None,
            description: None,
            icon: icon.map(str::to_string),
            distribution: Default::default(),
        }
    }
}

pub(super) fn deserialize_sha256<'de, D>(deserializer: D) -> Result<Option<Sha256Digest>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| Sha256Digest::parse(&value).map_err(serde::de::Error::custom))
        .transpose()
}
