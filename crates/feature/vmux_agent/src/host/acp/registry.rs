use std::collections::BTreeMap;
use std::path::PathBuf;

use bevy::prelude::*;
use crossbeam_channel::Receiver;
use serde::Deserialize;
use vmux_editor::lsp::package_path::Sha256Digest;

pub const REGISTRY_URL: &str =
    "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

pub(crate) struct RegistryPlugin;

impl Plugin for RegistryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, fetch).add_systems(Update, receive);
    }
}

#[derive(Component)]
struct RegistryFetch {
    rx: Receiver<Vec<RegistryAgent>>,
}

fn fetch(mut commands: Commands) {
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::spawn(move || {
        let agents = Registry::fetch_blocking()
            .ok()
            .or_else(Registry::cached)
            .map(|registry| registry.agents)
            .unwrap_or_default();
        let _ = tx.send(agents);
    });
    commands.spawn(RegistryFetch { rx });
}

fn receive(
    fetches: Query<(Entity, &RegistryFetch)>,
    current: Query<Entity, With<RegistryAgent>>,
    mut commands: Commands,
) {
    let mut received = None;
    for (entity, fetch) in &fetches {
        let Ok(agents) = fetch.rx.try_recv() else {
            continue;
        };
        received = Some(agents);
        commands.entity(entity).despawn();
    }
    let Some(agents) = received else {
        return;
    };
    for entity in &current {
        commands.entity(entity).despawn();
    }
    for agent in agents {
        commands.spawn((Name::new(format!("ACP agent {}", agent.id)), agent));
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub agents: Vec<RegistryAgent>,
}

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

    pub fn fetch_blocking() -> Result<Self, String> {
        let text = reqwest::blocking::get(REGISTRY_URL)
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
    #[serde(default, deserialize_with = "deserialize_sha256")]
    pub sha256: Option<Sha256Digest>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

fn deserialize_sha256<'de, D>(deserializer: D) -> Result<Option<Sha256Digest>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| Sha256Digest::parse(&value).map_err(serde::de::Error::custom))
        .transpose()
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
    fn fetch_entity_is_consumed_after_delivery() {
        let mut app = App::new();
        app.add_systems(Update, receive);
        let (tx, rx) = crossbeam_channel::unbounded();
        let fetch = app.world_mut().spawn(RegistryFetch { rx }).id();
        tx.send(vec![RegistryAgent {
            id: "agent".into(),
            name: "Agent".into(),
            version: None,
            description: None,
            icon: None,
            distribution: Distribution::default(),
        }])
        .unwrap();

        app.update();

        assert!(app.world().get_entity(fetch).is_err());
        let mut agents = app.world_mut().query::<&RegistryAgent>();
        assert_eq!(agents.single(app.world()).unwrap().id, "agent");
    }

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
