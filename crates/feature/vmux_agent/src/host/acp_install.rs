use std::path::{Path, PathBuf};

use vmux_core::event::InstallPhase;
use vmux_editor::lsp::archive::ArchiveKind;
use vmux_editor::lsp::download::{self, RemoteArtifact};
use vmux_editor::lsp::package_path::{PackageName, PackagePath, Sha256Digest};
use vmux_editor::lsp::store::{PackageStore, Receipt};

use crate::acp_registry::{self, BinaryTarget, RegistryAgent};

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

fn write_agent_receipt(
    store: &PackageStore,
    agent: &RegistryAgent,
    version: Option<&str>,
) -> Result<(), String> {
    let name = PackageName::parse(&agent.id)?;
    store
        .write_receipt(
            &name,
            &Receipt {
                name: name.clone(),
                version: version
                    .map(str::to_string)
                    .or_else(|| agent.version.clone()),
                source_id: format!("acp:{}", agent.id),
                bin: std::collections::BTreeMap::new(),
            },
        )
        .map_err(|e| e.to_string())
}

fn package_base(package: &str) -> &str {
    match package.rfind('@') {
        Some(at) if at > 0 => &package[..at],
        _ => package,
    }
}

fn package_spec(package: &str, version: Option<&str>) -> String {
    match version.map(str::trim) {
        Some(v) if !v.is_empty() => format!("{}@{v}", package_base(package)),
        _ => package.to_string(),
    }
}

fn cmd_basename(cmd: &str) -> &str {
    let rel = cmd.trim_start_matches("./").trim_start_matches(".\\");
    Path::new(rel)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(rel)
}

fn archive_filename(url: &str) -> &str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("archive")
}

fn resolved_cmd_path(pkgdir: &Path, target: &BinaryTarget, file: &str) -> Result<PathBuf, String> {
    let rel = target
        .cmd
        .trim_start_matches("./")
        .trim_start_matches(".\\")
        .replace('\\', "/");
    let rel = PackagePath::parse(&rel)?;
    match ArchiveKind::for_file(file) {
        ArchiveKind::TarGz | ArchiveKind::Zip => Ok(pkgdir.join(rel.as_path())),
        ArchiveKind::Gz | ArchiveKind::Raw => Ok(pkgdir.join(cmd_basename(rel.as_str()))),
    }
}

fn ensure_binary_installed(
    agent: &RegistryAgent,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let target = agent
        .binary_for_host()
        .ok_or_else(|| format!("no binary distribution for this platform: {}", agent.id))?;
    let store = PackageStore::at(vmux_core::profile::ProfilePaths::current().agents());
    let name = PackageName::parse(&agent.id)?;
    let pkgdir = store.package_dir(&name);
    let file = archive_filename(&target.archive).to_string();
    PackageName::parse(&file)?;
    let cmd_path = resolved_cmd_path(&pkgdir, target, &file)?;

    let up_to_date = store
        .read_receipt(&name)
        .map(|receipt| receipt.version == agent.version)
        .unwrap_or(false);
    let package_added = !is_agent_installed_at(&store, agent);
    if !up_to_date || !cmd_path.exists() {
        install_binary(agent, target, &store, &name, &file, &mut emit)?;
    }

    Ok(ResolvedAgent {
        command: cmd_path.to_string_lossy().into_owned(),
        args: target.args.clone(),
        env: target
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        path_prepend: None,
        package_added,
    })
}

fn node_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("darwin-arm64"),
        ("macos", "x86_64") => Some("darwin-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("linux", "x86_64") => Some("linux-x64"),
        _ => None,
    }
}

fn ensure_node(
    store: &PackageStore,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<PathBuf, String> {
    let target = node_target().ok_or("managed Node not supported on this platform")?;
    let dirname = format!("node-v{NODE_VERSION}-{target}");
    let name = PackageName::parse("node")?;
    let node_parent = store.package_dir(&name);
    let bindir = node_parent.join(&dirname).join("bin");
    if bindir.join("node").exists() {
        return Ok(bindir);
    }

    let file = format!("{dirname}.tar.gz");
    let url = format!("https://nodejs.org/dist/v{NODE_VERSION}/{file}");
    let checksum_url = format!("https://nodejs.org/dist/v{NODE_VERSION}/SHASUMS256.txt");
    let digest = Sha256Digest::from_manifest(&checksum_url, &file, download::CHECKSUM_MAX_BYTES)?;
    let staging_root = store.staging_dir();
    std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix("node")
        .tempdir_in(&staging_root)
        .map_err(|e| e.to_string())?;
    let dl = staging.path().join(&file);

    emit(
        InstallPhase::Downloading,
        Some(0),
        "downloading Node runtime",
    );
    RemoteArtifact::new(url.clone(), digest).download_to(
        &dl,
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
    ArchiveKind::TarGz.extract(&dl, &staged_package, &dirname)?;
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
    store
        .activate_package(&name, &staged_package)
        .map_err(|error| error.to_string())?;
    Ok(bindir)
}

fn ensure_npx_installed(
    agent: &RegistryAgent,
    version: Option<&str>,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let dist = agent
        .distribution
        .npx
        .as_ref()
        .ok_or_else(|| format!("no npx distribution: {}", agent.id))?;
    let store = PackageStore::at(vmux_core::profile::ProfilePaths::current().agents());
    let package_added = !is_agent_installed_at(&store, agent);
    let bindir = ensure_node(&store, &mut emit)?;
    let npx = node_cli(&store, "npx-cli.js")
        .filter(|path| path.is_file())
        .ok_or("managed npx missing after extract")?;
    write_agent_receipt(&store, agent, version)?;
    emit(InstallPhase::Done, Some(100), "ready");

    let mut args = vec![
        npx.to_string_lossy().into_owned(),
        "-y".to_string(),
        package_spec(&dist.package, version),
    ];
    args.extend(dist.args.iter().cloned());
    Ok(ResolvedAgent {
        command: bindir.join("node").to_string_lossy().into_owned(),
        args,
        env: dist
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        path_prepend: Some(bindir.to_string_lossy().into_owned()),
        package_added,
    })
}

fn uv_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        _ => None,
    }
}

fn ensure_uv(
    store: &PackageStore,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<PathBuf, String> {
    let target = uv_target().ok_or("managed uv not supported on this platform")?;
    let dirname = format!("uv-{target}");
    let name = PackageName::parse("uv")?;
    let uv_parent = store.package_dir(&name);
    let bindir = uv_parent.join(&dirname);
    if bindir.join("uvx").exists() {
        return Ok(bindir);
    }

    let file = format!("{dirname}.tar.gz");
    let url = format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{file}");
    let checksum_url = format!("{url}.sha256");
    let digest = Sha256Digest::from_manifest(&checksum_url, &file, download::CHECKSUM_MAX_BYTES)?;
    let staging_root = store.staging_dir();
    std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix("uv")
        .tempdir_in(&staging_root)
        .map_err(|e| e.to_string())?;
    let dl = staging.path().join(&file);

    emit(InstallPhase::Downloading, Some(0), "downloading uv runtime");
    RemoteArtifact::new(url.clone(), digest).download_to(
        &dl,
        download::PACKAGE_MAX_BYTES,
        |downloaded, total| {
            let percent =
                total.and_then(|total| (total > 0).then(|| ((downloaded * 100) / total) as u8));
            emit(InstallPhase::Downloading, percent, "downloading uv runtime");
        },
    )?;

    let staged_package = staging.path().join("package");
    emit(InstallPhase::Extracting, None, "extracting uv runtime");
    ArchiveKind::TarGz.extract(&dl, &staged_package, &dirname)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for exe in ["uv", "uvx"] {
            let p = staged_package.join(&dirname).join(exe);
            if let Ok(meta) = std::fs::metadata(&p) {
                let mut perm = meta.permissions();
                perm.set_mode(0o755);
                let _ = std::fs::set_permissions(&p, perm);
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
    store
        .activate_package(&name, &staged_package)
        .map_err(|error| error.to_string())?;
    Ok(bindir)
}

fn ensure_uvx_installed(
    agent: &RegistryAgent,
    version: Option<&str>,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let dist = agent
        .distribution
        .uvx
        .as_ref()
        .ok_or_else(|| format!("no uvx distribution: {}", agent.id))?;
    let store = PackageStore::at(vmux_core::profile::ProfilePaths::current().agents());
    let package_added = !is_agent_installed_at(&store, agent);
    let bindir = ensure_uv(&store, &mut emit)?;
    write_agent_receipt(&store, agent, version)?;
    emit(InstallPhase::Done, Some(100), "ready");

    let mut args = vec![package_spec(&dist.package, version)];
    args.extend(dist.args.iter().cloned());
    Ok(ResolvedAgent {
        command: bindir.join("uvx").to_string_lossy().into_owned(),
        args,
        env: dist
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        path_prepend: Some(bindir.to_string_lossy().into_owned()),
        package_added,
    })
}

fn node_bindir(store: &PackageStore) -> Option<PathBuf> {
    let target = node_target()?;
    Some(
        store
            .packages_dir()
            .join("node")
            .join(format!("node-v{NODE_VERSION}-{target}"))
            .join("bin"),
    )
}

fn node_cli(store: &PackageStore, file: &str) -> Option<PathBuf> {
    Some(
        node_bindir(store)?
            .parent()?
            .join("lib/node_modules/npm/bin")
            .join(file),
    )
}

fn uv_bindir(store: &PackageStore) -> Option<PathBuf> {
    let target = uv_target()?;
    Some(store.packages_dir().join("uv").join(format!("uv-{target}")))
}

fn is_agent_installed_at(store: &PackageStore, agent: &RegistryAgent) -> bool {
    let Ok(name) = PackageName::parse(&agent.id) else {
        return false;
    };
    if !store.is_installed(&name) {
        return false;
    }
    match agent.preferred_runtime() {
        acp_registry::Runtime::None => true,
        acp_registry::Runtime::Node => {
            node_bindir(store).is_some_and(|bindir| bindir.join("node").is_file())
                && node_cli(store, "npx-cli.js").is_some_and(|path| path.is_file())
        }
        acp_registry::Runtime::Uv => uv_bindir(store)
            .map(|b| b.join("uvx").exists())
            .unwrap_or(false),
    }
}

impl RegistryAgent {
    pub(crate) fn is_installed(&self) -> bool {
        let store = PackageStore::at(vmux_core::profile::ProfilePaths::current().agents());
        is_agent_installed_at(&store, self)
    }
}

pub(super) fn uninstall(id: &str) -> Result<(), String> {
    let store = PackageStore::at(vmux_core::profile::ProfilePaths::current().agents());
    uninstall_at(&store, id)
}

fn uninstall_at(store: &PackageStore, id: &str) -> Result<(), String> {
    let name = PackageName::parse(id)?;
    store.remove(&name).map_err(|error| error.to_string())
}

pub(super) fn resolve_from_registry(
    agent_id: &str,
    version: Option<&str>,
    emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let reg_id = RegistryAgent::canonical_id(agent_id);
    let find = |reg: acp_registry::Registry| {
        reg.agents
            .into_iter()
            .find(|agent| RegistryAgent::ids_match(&agent.id, agent_id))
    };
    let agent = match acp_registry::Registry::cached().and_then(find) {
        Some(a) => a,
        None => acp_registry::Registry::fetch_blocking()?
            .agents
            .into_iter()
            .find(|agent| RegistryAgent::ids_match(&agent.id, agent_id))
            .ok_or_else(|| format!("agent not in ACP registry: {agent_id} ({reg_id})"))?,
    };
    ensure_installed(&agent, version, emit)
}

fn ensure_installed(
    agent: &RegistryAgent,
    version: Option<&str>,
    emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    use acp_registry::Runtime;
    match agent.preferred_runtime() {
        Runtime::None => ensure_binary_installed(agent, emit),
        Runtime::Node => ensure_npx_installed(agent, version, emit),
        Runtime::Uv => ensure_uvx_installed(agent, version, emit),
    }
}

#[allow(clippy::too_many_arguments)]
fn install_binary(
    agent: &RegistryAgent,
    target: &BinaryTarget,
    store: &PackageStore,
    name: &PackageName,
    file: &str,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<(), String> {
    let digest = target
        .sha256
        .as_ref()
        .ok_or_else(|| format!("ACP registry has no SHA-256 digest for {}", agent.id))?;
    let staging_root = store.staging_dir();
    std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix(name.as_str())
        .tempdir_in(&staging_root)
        .map_err(|e| e.to_string())?;
    let dl = staging.path().join(file);

    emit(InstallPhase::Downloading, Some(0), &target.archive);
    RemoteArtifact::new(target.archive.clone(), digest.clone()).download_to(
        &dl,
        download::PACKAGE_MAX_BYTES,
        |downloaded, total| {
            let percent =
                total.and_then(|total| (total > 0).then(|| ((downloaded * 100) / total) as u8));
            emit(InstallPhase::Downloading, percent, "downloading");
        },
    )?;

    let staged_package = staging.path().join("package");
    emit(InstallPhase::Extracting, None, "extracting");
    ArchiveKind::for_file(file).extract(&dl, &staged_package, cmd_basename(&target.cmd))?;
    let staged_cmd = resolved_cmd_path(&staged_package, target, file)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&staged_cmd) {
            let mut perm = meta.permissions();
            perm.set_mode(0o755);
            let _ = std::fs::set_permissions(&staged_cmd, perm);
        }
    }
    if !staged_cmd.exists() {
        return Err(format!(
            "acp install: executable {} missing after extract (cmd={})",
            staged_cmd.display(),
            target.cmd
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

#[cfg(test)]
mod tests {
    use super::*;

    fn npx_agent(id: &str) -> RegistryAgent {
        RegistryAgent {
            id: id.to_string(),
            name: id.to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            icon: None,
            repository: None,
            distribution: acp_registry::Distribution {
                binary: None,
                npx: Some(acp_registry::PackageDist {
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
        assert_eq!(package_spec("@scope/pkg", None), "@scope/pkg");
        assert_eq!(
            package_spec("@scope/pkg", Some("1.2.3")),
            "@scope/pkg@1.2.3"
        );
        assert_eq!(package_spec("pkg", Some("  ")), "pkg");
        assert_eq!(package_spec("pkg", Some("1.0.0")), "pkg@1.0.0");
    }

    #[test]
    fn package_spec_replaces_a_baked_registry_version() {
        assert_eq!(
            package_spec("@scope/pkg@1.1.9", Some("1.1.8")),
            "@scope/pkg@1.1.8"
        );
        assert_eq!(package_spec("pkg@1.1.9", Some("1.1.8")), "pkg@1.1.8");
        assert_eq!(package_spec("@scope/pkg@1.1.9", None), "@scope/pkg@1.1.9");
        assert_eq!(package_base("@scope/pkg"), "@scope/pkg");
    }

    #[test]
    fn cmd_basename_strips_prefix_and_dirs() {
        assert_eq!(cmd_basename("./vibe"), "vibe");
        assert_eq!(cmd_basename("vibe"), "vibe");
        assert_eq!(cmd_basename("./bin/agent"), "agent");
    }

    #[test]
    fn archive_filename_takes_last_segment() {
        assert_eq!(
            archive_filename("https://x/y/vibe-darwin-arm64.tar.gz"),
            "vibe-darwin-arm64.tar.gz"
        );
        assert_eq!(archive_filename("https://x/y/bin.zip?token=1"), "bin.zip");
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
            resolved_cmd_path(pkg, &tar, "a.tar.gz").unwrap(),
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
            resolved_cmd_path(pkg, &gz, "a.gz").unwrap(),
            Path::new("/pkg/agent")
        );
        let escaping = BinaryTarget {
            archive: "https://x/a.tar.gz".into(),
            cmd: "../agent".into(),
            sha256: None,
            args: vec![],
            env: Default::default(),
        };
        assert!(resolved_cmd_path(pkg, &escaping, "a.tar.gz").is_err());
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
        let node = node_bindir(&store).unwrap().join("node");
        std::fs::create_dir_all(node.parent().unwrap()).unwrap();
        std::fs::write(&node, b"").unwrap();
        let npx = node_cli(&store, "npx-cli.js").unwrap();
        std::fs::create_dir_all(npx.parent().unwrap()).unwrap();
        std::fs::write(npx, b"").unwrap();
        let installed = npx_agent("installed-agent");
        let available = npx_agent("available-agent");

        assert!(!is_agent_installed_at(&store, &installed));
        assert!(!is_agent_installed_at(&store, &available));

        write_agent_receipt(&store, &installed, None).unwrap();

        assert!(is_agent_installed_at(&store, &installed));
        assert!(!is_agent_installed_at(&store, &available));

        uninstall_at(&store, &installed.id).unwrap();

        assert!(!is_agent_installed_at(&store, &installed));
        assert!(node.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
