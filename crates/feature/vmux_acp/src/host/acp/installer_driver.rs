use std::path::{Path, PathBuf};

use vmux_api::protocol::ClientMessage;
use vmux_ecs::event::InstallPhase;
use vmux_ecs::{Cwd, ProcessAnchor};
use vmux_editor::lsp::archive::ArchiveKind;
use vmux_editor::lsp::package_path::PackagePath;
use vmux_session::{AgentId, SessionId};
use vmux_setting::AppSettings;

use super::registry::{BinaryTarget, PackageDist};
use super::{AcpInstallKey, AcpInstallRequest, AcpInstallWaiter, AcpLaunch, InstallState};
use crate::AcpSessionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedAgent {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub path_prepend: Option<String>,
    pub package_added: bool,
}

impl AcpInstallRequest {
    pub(super) fn key(&self) -> AcpInstallKey {
        let fallback = self.fallback.as_ref();
        AcpInstallKey {
            profile: self.profile,
            agents: self.agents.clone(),
            catalog: self.catalog,
            catalog_revision: self.catalog_revision,
            agent_id: self.agent_id.clone(),
            version: fallback.and_then(|config| config.version.clone()),
            fallback_command: fallback
                .map(|config| config.command.clone())
                .unwrap_or_default(),
            fallback_args: fallback
                .map(|config| config.args.clone())
                .unwrap_or_default(),
            fallback_env: fallback
                .map(|config| config.env.clone())
                .unwrap_or_default(),
            shell: self.shell.clone(),
        }
    }
}

impl AcpInstallWaiter {
    pub(super) fn matches(&self, session_id: &SessionId, agent_id: &AgentId) -> bool {
        self.sid == session_id.0 && self.agent_id == agent_id.0
    }
}

impl AcpLaunch {
    pub(super) fn message_for(
        &self,
        session_id: &SessionId,
        agent_id: &AgentId,
        cwd: &Cwd,
        anchor: &ProcessAnchor,
        resume: Option<&AcpSessionId>,
        settings: Option<&AppSettings>,
    ) -> ClientMessage {
        let shell = settings
            .map(AppSettings::shell)
            .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_default());
        let mcp = crate::mcp_driver::McpLaunchSpec::acp(&cwd.0, anchor.0, &shell)
            .resolve()
            .inspect_err(|error| {
                bevy::log::warn!(
                    "acp: vmux_mcp sidecar unresolved; agent runs without vmux tools: {error}"
                );
            })
            .ok();
        ClientMessage::SpawnAcpAgent {
            sid: session_id.0.clone(),
            agent_id: agent_id.0.clone(),
            command: self.command.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
            cwd: cwd.0.to_string_lossy().into_owned(),
            anchor: anchor.0,
            mcp_command: mcp.as_ref().map(|mcp| mcp.command.clone()),
            mcp_args: mcp.map(|mcp| mcp.args).unwrap_or_default(),
            resume_acp_session_id: resume.map(|resume| resume.0.clone()),
        }
    }
}

impl InstallState {
    pub(super) fn from_phase(phase: InstallPhase, pct: Option<u8>, message: &str) -> Self {
        if matches!(phase, InstallPhase::Done) {
            Self {
                pct: None,
                message: "Starting agent…".to_string(),
            }
        } else {
            Self {
                pct,
                message: message.to_string(),
            }
        }
    }

    pub(super) fn preparing() -> Self {
        Self {
            pct: None,
            message: "Preparing agent…".to_string(),
        }
    }
}

impl PackageDist {
    pub(super) fn spec(&self, version: Option<&str>) -> String {
        match version.map(str::trim) {
            Some(version) if !version.is_empty() => {
                let package = match self.package.rfind('@') {
                    Some(at) if at > 0 => &self.package[..at],
                    _ => &self.package,
                };
                format!("{package}@{version}")
            }
            _ => self.package.clone(),
        }
    }
}

impl BinaryTarget {
    pub(super) fn archive_filename(&self) -> &str {
        let path = self
            .archive
            .split(['?', '#'])
            .next()
            .unwrap_or(&self.archive);
        path.rsplit('/')
            .find(|segment| !segment.is_empty())
            .unwrap_or("archive")
    }

    pub(super) fn command_basename(&self) -> &str {
        let relative = self.cmd.trim_start_matches("./").trim_start_matches(".\\");
        Path::new(relative)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(relative)
    }

    pub(super) fn command_path(
        &self,
        package_dir: &Path,
        archive: &str,
    ) -> Result<PathBuf, String> {
        let relative = self
            .cmd
            .trim_start_matches("./")
            .trim_start_matches(".\\")
            .replace('\\', "/");
        let relative = PackagePath::parse(&relative)?;
        match ArchiveKind::for_file(archive) {
            ArchiveKind::TarGz | ArchiveKind::Zip => Ok(package_dir.join(relative.as_path())),
            ArchiveKind::Gz | ArchiveKind::Raw => Ok(package_dir.join(self.command_basename())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_spec_pins_version_when_present() {
        let scoped = PackageDist {
            package: "@scope/pkg".to_string(),
            args: Vec::new(),
            env: Default::default(),
        };
        let plain = PackageDist {
            package: "pkg".to_string(),
            args: Vec::new(),
            env: Default::default(),
        };
        assert_eq!(scoped.spec(None), "@scope/pkg");
        assert_eq!(scoped.spec(Some("1.2.3")), "@scope/pkg@1.2.3");
        assert_eq!(plain.spec(Some("  ")), "pkg");
        assert_eq!(plain.spec(Some("1.0.0")), "pkg@1.0.0");
    }

    #[test]
    fn package_spec_replaces_a_baked_registry_version() {
        let scoped = PackageDist {
            package: "@scope/pkg@1.1.9".to_string(),
            args: Vec::new(),
            env: Default::default(),
        };
        let plain = PackageDist {
            package: "pkg@1.1.9".to_string(),
            args: Vec::new(),
            env: Default::default(),
        };
        assert_eq!(scoped.spec(Some("1.1.8")), "@scope/pkg@1.1.8");
        assert_eq!(plain.spec(Some("1.1.8")), "pkg@1.1.8");
        assert_eq!(scoped.spec(None), "@scope/pkg@1.1.9");
    }

    #[test]
    fn cmd_basename_strips_prefix_and_dirs() {
        for (command, expected) in [
            ("./vibe", "vibe"),
            ("vibe", "vibe"),
            ("./bin/agent", "agent"),
        ] {
            let target = BinaryTarget {
                archive: String::new(),
                cmd: command.to_string(),
                sha256: None,
                args: Vec::new(),
                env: Default::default(),
            };
            assert_eq!(target.command_basename(), expected);
        }
    }

    #[test]
    fn archive_filename_takes_last_segment() {
        for (archive, expected) in [
            (
                "https://x/y/vibe-darwin-arm64.tar.gz",
                "vibe-darwin-arm64.tar.gz",
            ),
            ("https://x/y/bin.zip?token=1", "bin.zip"),
        ] {
            let target = BinaryTarget {
                archive: archive.to_string(),
                cmd: String::new(),
                sha256: None,
                args: Vec::new(),
                env: Default::default(),
            };
            assert_eq!(target.archive_filename(), expected);
        }
    }

    #[test]
    fn resolved_cmd_path_by_archive_kind() {
        let pkg = Path::new("/pkg");
        let tar = BinaryTarget {
            archive: "https://x/a.tar.gz".into(),
            cmd: "./bin/agent".into(),
            sha256: None,
            args: vec![],
            env: Default::default(),
        };
        assert_eq!(
            tar.command_path(pkg, "a.tar.gz").unwrap(),
            Path::new("/pkg/bin/agent")
        );
        let gz = BinaryTarget {
            archive: "https://x/a.gz".into(),
            cmd: "./agent".into(),
            sha256: None,
            args: vec![],
            env: Default::default(),
        };
        assert_eq!(
            gz.command_path(pkg, "a.gz").unwrap(),
            Path::new("/pkg/agent")
        );
        let escaping = BinaryTarget {
            archive: "https://x/a.tar.gz".into(),
            cmd: "../agent".into(),
            sha256: None,
            args: vec![],
            env: Default::default(),
        };
        assert!(escaping.command_path(pkg, "a.tar.gz").is_err());
    }
}
