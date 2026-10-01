use std::path::{Path, PathBuf};

use vmux_ecs::event::InstallPhase;
use vmux_editor::lsp::archive::ArchiveKind;
use vmux_editor::lsp::download::{self, RemoteArtifact};
use vmux_editor::lsp::package_path::{PackageName, PackagePath, Sha256Digest};
use vmux_editor::lsp::store::{PackageStore, Receipt};

use super::registry::{BinaryTarget, PackageDist, Registry, RegistryAgent, Runtime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedAgent {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub path_prepend: Option<String>,
    pub package_added: bool,
}

const NODE_VERSION: &str = "22.11.0";
const UV_VERSION: &str = "0.5.11";

pub(super) struct AgentInstaller {
    store: PackageStore,
}

impl AgentInstaller {
    pub(super) fn current() -> Self {
        Self::at(PackageStore::at(
            vmux_ecs::profile::ProfilePaths::current().agents(),
        ))
    }

    fn at(store: PackageStore) -> Self {
        Self { store }
    }

    pub(super) fn resolve(
        &self,
        agent_id: &str,
        version: Option<&str>,
        emit: impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<ResolvedAgent, String> {
        let agent = match Registry::cached().and_then(|registry| registry.agent(agent_id)) {
            Some(agent) => agent,
            None => Registry::fetch_blocking()?
                .agent(agent_id)
                .ok_or_else(|| format!("agent not in ACP registry: {agent_id}"))?,
        };
        agent.ensure_installed(&self.store, version, emit)
    }

    pub(super) fn uninstall(&self, id: &str) -> Result<(), String> {
        let name = PackageName::parse(id)?;
        self.store.remove(&name).map_err(|error| error.to_string())
    }

    fn is_installed(&self, agent: &RegistryAgent) -> bool {
        agent.is_installed_at(&self.store)
    }
}

impl PackageDist {
    fn spec(&self, version: Option<&str>) -> String {
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
    fn archive_filename(&self) -> &str {
        let path = self
            .archive
            .split(['?', '#'])
            .next()
            .unwrap_or(&self.archive);
        path.rsplit('/')
            .find(|segment| !segment.is_empty())
            .unwrap_or("archive")
    }

    fn command_basename(&self) -> &str {
        let relative = self.cmd.trim_start_matches("./").trim_start_matches(".\\");
        Path::new(relative)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(relative)
    }

    fn command_path(&self, package_dir: &Path, archive: &str) -> Result<PathBuf, String> {
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

impl RegistryAgent {
    fn ensure_binary(
        &self,
        store: &PackageStore,
        mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<ResolvedAgent, String> {
        let target = self
            .binary_for_host()
            .ok_or_else(|| format!("no binary distribution for this platform: {}", self.id))?;
        let name = PackageName::parse(&self.id)?;
        let package_dir = store.package_dir(&name);
        let archive = target.archive_filename().to_string();
        PackageName::parse(&archive)?;
        let command = target.command_path(&package_dir, &archive)?;

        let up_to_date = store
            .read_receipt(&name)
            .map(|receipt| receipt.version == self.version)
            .unwrap_or(false);
        let package_added = !self.is_installed_at(store);
        if !up_to_date || !command.exists() {
            target.install(self, store, &name, &archive, &mut emit)?;
        }

        Ok(ResolvedAgent {
            command: command.to_string_lossy().into_owned(),
            args: target.args.clone(),
            env: target
                .env
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            path_prepend: None,
            package_added,
        })
    }
}

struct NodeRuntime {
    store: PackageStore,
}

impl NodeRuntime {
    fn new(store: &PackageStore) -> Self {
        Self {
            store: store.clone(),
        }
    }

    fn target() -> Option<&'static str> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => Some("darwin-arm64"),
            ("macos", "x86_64") => Some("darwin-x64"),
            ("linux", "aarch64") => Some("linux-arm64"),
            ("linux", "x86_64") => Some("linux-x64"),
            _ => None,
        }
    }

    fn bin_dir(&self) -> Option<PathBuf> {
        let target = Self::target()?;
        Some(
            self.store
                .packages_dir()
                .join("node")
                .join(format!("node-v{NODE_VERSION}-{target}"))
                .join("bin"),
        )
    }

    fn cli(&self, file: &str) -> Option<PathBuf> {
        Some(
            self.bin_dir()?
                .parent()?
                .join("lib/node_modules/npm/bin")
                .join(file),
        )
    }

    fn is_ready(&self) -> bool {
        self.bin_dir()
            .is_some_and(|directory| directory.join("node").is_file())
            && self.cli("npx-cli.js").is_some_and(|path| path.is_file())
    }

    fn ensure(
        &self,
        emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<PathBuf, String> {
        let target = Self::target().ok_or("managed Node not supported on this platform")?;
        let dirname = format!("node-v{NODE_VERSION}-{target}");
        let name = PackageName::parse("node")?;
        let node_parent = self.store.package_dir(&name);
        let bindir = node_parent.join(&dirname).join("bin");
        if bindir.join("node").exists() {
            return Ok(bindir);
        }

        let file = format!("{dirname}.tar.gz");
        let url = format!("https://nodejs.org/dist/v{NODE_VERSION}/{file}");
        let checksum_url = format!("https://nodejs.org/dist/v{NODE_VERSION}/SHASUMS256.txt");
        let digest =
            Sha256Digest::from_manifest(&checksum_url, &file, download::CHECKSUM_MAX_BYTES)?;
        let staging_root = self.store.staging_dir();
        std::fs::create_dir_all(&staging_root).map_err(|error| error.to_string())?;
        let staging = tempfile::Builder::new()
            .prefix("node")
            .tempdir_in(&staging_root)
            .map_err(|error| error.to_string())?;
        let download_path = staging.path().join(&file);

        emit(
            InstallPhase::Downloading,
            Some(0),
            "downloading Node runtime",
        );
        RemoteArtifact::new(url.clone(), digest).download_to(
            &download_path,
            download::PACKAGE_MAX_BYTES,
            |downloaded, total| {
                let percent =
                    total.and_then(|total| (total > 0).then(|| ((downloaded * 100) / total) as u8));
                emit(
                    InstallPhase::Downloading,
                    percent,
                    "downloading Node runtime",
                );
            },
        )?;

        let staged_package = staging.path().join("package");
        emit(InstallPhase::Extracting, None, "extracting Node runtime");
        ArchiveKind::TarGz.extract(&download_path, &staged_package, &dirname)?;
        if !staged_package.join(&dirname).join("bin/node").exists()
            || !staged_package
                .join(&dirname)
                .join("lib/node_modules/npm/bin/npx-cli.js")
                .exists()
        {
            return Err("managed Node missing after extract".to_string());
        }
        Receipt {
            name: name.clone(),
            version: Some(NODE_VERSION.to_string()),
            source_id: url,
            bin: Default::default(),
        }
        .write_to(&staged_package)
        .map_err(|error| error.to_string())?;
        self.store
            .activate_package(&name, &staged_package)
            .map_err(|error| error.to_string())?;
        Ok(bindir)
    }
}

impl RegistryAgent {
    fn ensure_npx(
        &self,
        store: &PackageStore,
        version: Option<&str>,
        mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<ResolvedAgent, String> {
        let distribution = self
            .distribution
            .npx
            .as_ref()
            .ok_or_else(|| format!("no npx distribution: {}", self.id))?;
        let package_added = !self.is_installed_at(store);
        let runtime = NodeRuntime::new(store);
        let bin_dir = runtime.ensure(&mut emit)?;
        let npx = runtime
            .cli("npx-cli.js")
            .filter(|path| path.is_file())
            .ok_or("managed npx missing after extract")?;
        self.write_receipt(store, version)?;
        emit(InstallPhase::Done, Some(100), "ready");

        let mut args = vec![
            npx.to_string_lossy().into_owned(),
            "-y".to_string(),
            distribution.spec(version),
        ];
        args.extend(distribution.args.iter().cloned());
        Ok(ResolvedAgent {
            command: bin_dir.join("node").to_string_lossy().into_owned(),
            args,
            env: distribution
                .env
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            path_prepend: Some(bin_dir.to_string_lossy().into_owned()),
            package_added,
        })
    }
}

struct UvRuntime {
    store: PackageStore,
}

impl UvRuntime {
    fn new(store: &PackageStore) -> Self {
        Self {
            store: store.clone(),
        }
    }

    fn target() -> Option<&'static str> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => Some("aarch64-apple-darwin"),
            ("macos", "x86_64") => Some("x86_64-apple-darwin"),
            ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
            ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
            _ => None,
        }
    }

    fn bin_dir(&self) -> Option<PathBuf> {
        let target = Self::target()?;
        Some(
            self.store
                .packages_dir()
                .join("uv")
                .join(format!("uv-{target}")),
        )
    }

    fn is_ready(&self) -> bool {
        self.bin_dir()
            .is_some_and(|directory| directory.join("uvx").exists())
    }

    fn ensure(
        &self,
        emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<PathBuf, String> {
        let target = Self::target().ok_or("managed uv not supported on this platform")?;
        let dirname = format!("uv-{target}");
        let name = PackageName::parse("uv")?;
        let uv_parent = self.store.package_dir(&name);
        let bindir = uv_parent.join(&dirname);
        if bindir.join("uvx").exists() {
            return Ok(bindir);
        }

        let file = format!("{dirname}.tar.gz");
        let url = format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{file}");
        let checksum_url = format!("{url}.sha256");
        let digest =
            Sha256Digest::from_manifest(&checksum_url, &file, download::CHECKSUM_MAX_BYTES)?;
        let staging_root = self.store.staging_dir();
        std::fs::create_dir_all(&staging_root).map_err(|error| error.to_string())?;
        let staging = tempfile::Builder::new()
            .prefix("uv")
            .tempdir_in(&staging_root)
            .map_err(|error| error.to_string())?;
        let download_path = staging.path().join(&file);

        emit(InstallPhase::Downloading, Some(0), "downloading uv runtime");
        RemoteArtifact::new(url.clone(), digest).download_to(
            &download_path,
            download::PACKAGE_MAX_BYTES,
            |downloaded, total| {
                let percent =
                    total.and_then(|total| (total > 0).then(|| ((downloaded * 100) / total) as u8));
                emit(InstallPhase::Downloading, percent, "downloading uv runtime");
            },
        )?;

        let staged_package = staging.path().join("package");
        emit(InstallPhase::Extracting, None, "extracting uv runtime");
        ArchiveKind::TarGz.extract(&download_path, &staged_package, &dirname)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for executable in ["uv", "uvx"] {
                let path = staged_package.join(&dirname).join(executable);
                if let Ok(metadata) = std::fs::metadata(&path) {
                    let mut permissions = metadata.permissions();
                    permissions.set_mode(0o755);
                    let _ = std::fs::set_permissions(&path, permissions);
                }
            }
        }
        if !staged_package.join(&dirname).join("uvx").exists() {
            return Err("managed uv missing after extract".to_string());
        }
        Receipt {
            name: name.clone(),
            version: Some(UV_VERSION.to_string()),
            source_id: url,
            bin: Default::default(),
        }
        .write_to(&staged_package)
        .map_err(|error| error.to_string())?;
        self.store
            .activate_package(&name, &staged_package)
            .map_err(|error| error.to_string())?;
        Ok(bindir)
    }
}

impl RegistryAgent {
    fn ensure_uvx(
        &self,
        store: &PackageStore,
        version: Option<&str>,
        mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<ResolvedAgent, String> {
        let distribution = self
            .distribution
            .uvx
            .as_ref()
            .ok_or_else(|| format!("no uvx distribution: {}", self.id))?;
        let package_added = !self.is_installed_at(store);
        let bin_dir = UvRuntime::new(store).ensure(&mut emit)?;
        self.write_receipt(store, version)?;
        emit(InstallPhase::Done, Some(100), "ready");

        let mut args = vec![distribution.spec(version)];
        args.extend(distribution.args.iter().cloned());
        Ok(ResolvedAgent {
            command: bin_dir.join("uvx").to_string_lossy().into_owned(),
            args,
            env: distribution
                .env
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            path_prepend: Some(bin_dir.to_string_lossy().into_owned()),
            package_added,
        })
    }
}

impl RegistryAgent {
    pub(crate) fn is_installed(&self) -> bool {
        AgentInstaller::current().is_installed(self)
    }

    fn is_installed_at(&self, store: &PackageStore) -> bool {
        let Ok(name) = PackageName::parse(&self.id) else {
            return false;
        };
        if !store.is_installed(&name) {
            return false;
        }
        match self.preferred_runtime() {
            Runtime::None => true,
            Runtime::Node => NodeRuntime::new(store).is_ready(),
            Runtime::Uv => UvRuntime::new(store).is_ready(),
        }
    }

    fn write_receipt(&self, store: &PackageStore, version: Option<&str>) -> Result<(), String> {
        let name = PackageName::parse(&self.id)?;
        store
            .write_receipt(
                &name,
                &Receipt {
                    name: name.clone(),
                    version: version.map(str::to_string).or_else(|| self.version.clone()),
                    source_id: format!("acp:{}", self.id),
                    bin: std::collections::BTreeMap::new(),
                },
            )
            .map_err(|error| error.to_string())
    }

    fn ensure_installed(
        &self,
        store: &PackageStore,
        version: Option<&str>,
        emit: impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<ResolvedAgent, String> {
        match self.preferred_runtime() {
            Runtime::None => self.ensure_binary(store, emit),
            Runtime::Node => self.ensure_npx(store, version, emit),
            Runtime::Uv => self.ensure_uvx(store, version, emit),
        }
    }
}

impl BinaryTarget {
    fn install(
        &self,
        agent: &RegistryAgent,
        store: &PackageStore,
        name: &PackageName,
        archive: &str,
        emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<(), String> {
        let digest = self
            .sha256
            .as_ref()
            .ok_or_else(|| format!("ACP registry has no SHA-256 digest for {}", agent.id))?;
        let staging_root = store.staging_dir();
        std::fs::create_dir_all(&staging_root).map_err(|error| error.to_string())?;
        let staging = tempfile::Builder::new()
            .prefix(name.as_str())
            .tempdir_in(&staging_root)
            .map_err(|error| error.to_string())?;
        let download_path = staging.path().join(archive);

        emit(InstallPhase::Downloading, Some(0), &self.archive);
        RemoteArtifact::new(self.archive.clone(), digest.clone()).download_to(
            &download_path,
            download::PACKAGE_MAX_BYTES,
            |downloaded, total| {
                let percent =
                    total.and_then(|total| (total > 0).then(|| ((downloaded * 100) / total) as u8));
                emit(InstallPhase::Downloading, percent, "downloading");
            },
        )?;

        let staged_package = staging.path().join("package");
        emit(InstallPhase::Extracting, None, "extracting");
        ArchiveKind::for_file(archive).extract(
            &download_path,
            &staged_package,
            self.command_basename(),
        )?;
        let staged_command = self.command_path(&staged_package, archive)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&staged_command) {
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o755);
                let _ = std::fs::set_permissions(&staged_command, permissions);
            }
        }
        if !staged_command.exists() {
            return Err(format!(
                "acp install: executable {} missing after extract (cmd={})",
                staged_command.display(),
                self.cmd
            ));
        }

        Receipt {
            name: name.clone(),
            version: agent.version.clone(),
            source_id: format!("acp:{}", agent.id),
            bin: Default::default(),
        }
        .write_to(&staged_package)
        .map_err(|error| error.to_string())?;
        store
            .activate_package(name, &staged_package)
            .map_err(|error| error.to_string())?;
        emit(InstallPhase::Done, Some(100), "installed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::registry::{Distribution, PackageDist};
    use super::*;

    fn npx_agent(id: &str) -> RegistryAgent {
        RegistryAgent {
            id: id.to_string(),
            name: id.to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            icon: None,
            repository: None,
            distribution: Distribution {
                binary: None,
                npx: Some(PackageDist {
                    package: format!("@example/{id}"),
                    args: vec![],
                    env: Default::default(),
                }),
                uvx: None,
            },
        }
    }

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

    #[test]
    fn shared_node_does_not_mark_every_npx_agent_installed() {
        let root = std::env::temp_dir().join(format!(
            "vmux-acp-install-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = PackageStore::at(&root);
        let runtime = NodeRuntime::new(&store);
        let node = runtime.bin_dir().unwrap().join("node");
        std::fs::create_dir_all(node.parent().unwrap()).unwrap();
        std::fs::write(&node, b"").unwrap();
        let npx = runtime.cli("npx-cli.js").unwrap();
        std::fs::create_dir_all(npx.parent().unwrap()).unwrap();
        std::fs::write(npx, b"").unwrap();
        let installed = npx_agent("installed-agent");
        let available = npx_agent("available-agent");

        assert!(!installed.is_installed_at(&store));
        assert!(!available.is_installed_at(&store));

        installed.write_receipt(&store, None).unwrap();

        assert!(installed.is_installed_at(&store));
        assert!(!available.is_installed_at(&store));

        AgentInstaller::at(store.clone())
            .uninstall(&installed.id)
            .unwrap();

        assert!(!installed.is_installed_at(&store));
        assert!(node.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
