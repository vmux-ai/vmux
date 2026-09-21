use std::path::{Path, PathBuf};

use vmux_core::event::InstallPhase;
use vmux_editor::lsp::package_path::{PackageName, PackagePath};
use vmux_editor::lsp::{archive, download, store};

use crate::acp_registry::{self, BinaryTarget, RegistryAgent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgent {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub path_prepend: Option<String>,
}

const NODE_VERSION: &str = "22.11.0";
const UV_VERSION: &str = "0.5.11";

fn store_root() -> PathBuf {
    acp_registry::agents_dir()
}

fn write_agent_receipt(
    root: &Path,
    agent: &RegistryAgent,
    version: Option<&str>,
) -> Result<(), String> {
    let name = PackageName::parse(&agent.id)?;
    store::write_receipt(
        root,
        &name,
        &store::Receipt {
            name: agent.id.clone(),
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
    match archive::kind_for(file) {
        archive::ArchiveKind::TarGz | archive::ArchiveKind::Zip => Ok(pkgdir.join(rel.as_path())),
        archive::ArchiveKind::Gz | archive::ArchiveKind::Raw => {
            Ok(pkgdir.join(cmd_basename(rel.as_str())))
        }
    }
}

fn ensure_binary_installed(
    agent: &RegistryAgent,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let target = agent
        .binary_for_host()
        .ok_or_else(|| format!("no binary distribution for this platform: {}", agent.id))?;
    let root = store_root();
    let name = PackageName::parse(&agent.id)?;
    let pkgdir = store::package_dir(&root, &name);
    let file = archive_filename(&target.archive).to_string();
    PackageName::parse(&file)?;
    let cmd_path = resolved_cmd_path(&pkgdir, target, &file)?;

    let up_to_date = store::read_receipt(&root, &name)
        .map(|r| r.version == agent.version)
        .unwrap_or(false);
    if !up_to_date || !cmd_path.exists() {
        install_binary(agent, target, &root, &name, &file, &mut emit)?;
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
    root: &Path,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<PathBuf, String> {
    let target = node_target().ok_or("managed Node not supported on this platform")?;
    let dirname = format!("node-v{NODE_VERSION}-{target}");
    let name = PackageName::parse("node")?;
    let node_parent = store::package_dir(root, &name);
    let bindir = node_parent.join(&dirname).join("bin");
    if bindir.join("node").exists() {
        return Ok(bindir);
    }

    let file = format!("{dirname}.tar.gz");
    let url = format!("https://nodejs.org/dist/v{NODE_VERSION}/{file}");
    let checksum_url = format!("https://nodejs.org/dist/v{NODE_VERSION}/SHASUMS256.txt");
    let digest =
        download::sha256_from_manifest(&checksum_url, &file, download::CHECKSUM_MAX_BYTES)?;
    let staging_root = store::staging_dir(root);
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
    download::download_to(
        &url,
        &dl,
        download::PACKAGE_MAX_BYTES,
        &digest,
        |d, total| {
            let pct = total.and_then(|t| (t > 0).then(|| ((d * 100) / t) as u8));
            emit(InstallPhase::Downloading, pct, "downloading Node runtime");
        },
    )?;

    let staged_package = staging.path().join("package");
    emit(InstallPhase::Extracting, None, "extracting Node runtime");
    archive::extract(&dl, archive::ArchiveKind::TarGz, &staged_package, &dirname)?;
    if !staged_package.join(&dirname).join("bin/node").exists()
        || !staged_package
            .join(&dirname)
            .join("lib/node_modules/npm/bin/npx-cli.js")
            .exists()
    {
        return Err("managed Node missing after extract".to_string());
    }
    store::write_receipt_in(
        &staged_package,
        &store::Receipt {
            name: name.as_str().to_string(),
            version: Some(NODE_VERSION.to_string()),
            source_id: url,
            bin: Default::default(),
        },
    )
    .map_err(|error| error.to_string())?;
    store::activate_package(root, &name, &staged_package).map_err(|error| error.to_string())?;
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
    let root = store_root();
    let bindir = ensure_node(&root, &mut emit)?;
    let npx = node_cli(&root, "npx-cli.js")
        .filter(|path| path.is_file())
        .ok_or("managed npx missing after extract")?;
    write_agent_receipt(&root, agent, version)?;
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
    root: &Path,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<PathBuf, String> {
    let target = uv_target().ok_or("managed uv not supported on this platform")?;
    let dirname = format!("uv-{target}");
    let name = PackageName::parse("uv")?;
    let uv_parent = store::package_dir(root, &name);
    let bindir = uv_parent.join(&dirname);
    if bindir.join("uvx").exists() {
        return Ok(bindir);
    }

    let file = format!("{dirname}.tar.gz");
    let url = format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{file}");
    let checksum_url = format!("{url}.sha256");
    let digest =
        download::sha256_from_manifest(&checksum_url, &file, download::CHECKSUM_MAX_BYTES)?;
    let staging_root = store::staging_dir(root);
    std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix("uv")
        .tempdir_in(&staging_root)
        .map_err(|e| e.to_string())?;
    let dl = staging.path().join(&file);

    emit(InstallPhase::Downloading, Some(0), "downloading uv runtime");
    download::download_to(
        &url,
        &dl,
        download::PACKAGE_MAX_BYTES,
        &digest,
        |d, total| {
            let pct = total.and_then(|t| (t > 0).then(|| ((d * 100) / t) as u8));
            emit(InstallPhase::Downloading, pct, "downloading uv runtime");
        },
    )?;

    let staged_package = staging.path().join("package");
    emit(InstallPhase::Extracting, None, "extracting uv runtime");
    archive::extract(&dl, archive::ArchiveKind::TarGz, &staged_package, &dirname)?;

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
    store::write_receipt_in(
        &staged_package,
        &store::Receipt {
            name: name.as_str().to_string(),
            version: Some(UV_VERSION.to_string()),
            source_id: url,
            bin: Default::default(),
        },
    )
    .map_err(|error| error.to_string())?;
    store::activate_package(root, &name, &staged_package).map_err(|error| error.to_string())?;
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
    let root = store_root();
    let bindir = ensure_uv(&root, &mut emit)?;
    write_agent_receipt(&root, agent, version)?;
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
    })
}

fn node_bindir(root: &Path) -> Option<PathBuf> {
    let target = node_target()?;
    Some(
        store::packages_dir(root)
            .join("node")
            .join(format!("node-v{NODE_VERSION}-{target}"))
            .join("bin"),
    )
}

fn node_cli(root: &Path, file: &str) -> Option<PathBuf> {
    Some(
        node_bindir(root)?
            .parent()?
            .join("lib/node_modules/npm/bin")
            .join(file),
    )
}

fn uv_bindir(root: &Path) -> Option<PathBuf> {
    let target = uv_target()?;
    Some(
        store::packages_dir(root)
            .join("uv")
            .join(format!("uv-{target}")),
    )
}

pub fn is_agent_installed(agent: &RegistryAgent) -> bool {
    is_agent_installed_at(&store_root(), agent)
}

fn is_agent_installed_at(root: &Path, agent: &RegistryAgent) -> bool {
    let Ok(name) = PackageName::parse(&agent.id) else {
        return false;
    };
    if !store::is_installed(root, &name) {
        return false;
    }
    match agent.preferred_runtime() {
        acp_registry::Runtime::None => true,
        acp_registry::Runtime::Node => {
            node_bindir(root).is_some_and(|bindir| bindir.join("node").is_file())
                && node_cli(root, "npx-cli.js").is_some_and(|path| path.is_file())
        }
        acp_registry::Runtime::Uv => uv_bindir(root)
            .map(|b| b.join("uvx").exists())
            .unwrap_or(false),
    }
}

pub fn is_update_available(agent: &RegistryAgent) -> bool {
    let Ok(name) = PackageName::parse(&agent.id) else {
        return false;
    };
    matches!(agent.preferred_runtime(), acp_registry::Runtime::None)
        && store::read_receipt(&store_root(), &name)
            .map(|r| r.version != agent.version)
            .unwrap_or(false)
}

pub fn uninstall(id: &str) -> Result<(), String> {
    uninstall_at(&store_root(), id)
}

fn uninstall_at(root: &Path, id: &str) -> Result<(), String> {
    let name = PackageName::parse(id)?;
    store::remove(root, &name).map_err(|e| e.to_string())
}

pub fn registry_id_alias(id: &str) -> &str {
    match id {
        "claude" => "claude-acp",
        "codex" => "codex-acp",
        "vibe" => "mistral-vibe",
        other => other,
    }
}

pub(crate) fn agent_url_id(id: &str) -> &str {
    id.strip_suffix("-acp").unwrap_or(id)
}

pub(crate) fn agent_ids_match(left: &str, right: &str) -> bool {
    let left = registry_id_alias(left);
    let right = registry_id_alias(right);
    left == right || agent_url_id(left) == agent_url_id(right)
}

pub fn resolve_from_registry(
    agent_id: &str,
    version: Option<&str>,
    emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let reg_id = registry_id_alias(agent_id);
    let find = |reg: acp_registry::Registry| {
        reg.agents
            .into_iter()
            .find(|agent| agent_ids_match(&agent.id, agent_id))
    };
    let agent = match acp_registry::load_cached().and_then(find) {
        Some(a) => a,
        None => acp_registry::fetch_blocking()?
            .agents
            .into_iter()
            .find(|agent| agent_ids_match(&agent.id, agent_id))
            .ok_or_else(|| format!("agent not in ACP registry: {agent_id} ({reg_id})"))?,
    };
    ensure_installed(&agent, version, emit)
}

pub fn ensure_installed(
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

pub fn fetch_package_versions(agent: &RegistryAgent) -> Vec<String> {
    match agent.preferred_runtime() {
        acp_registry::Runtime::Node => agent
            .distribution
            .npx
            .as_ref()
            .map(|dist| npm_versions(package_base(&dist.package)))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn npm_versions(package: &str) -> Vec<String> {
    let root = store_root();
    let managed = node_bindir(&root).and_then(|bindir| {
        let node = bindir.join("node");
        let npm = node_cli(&root, "npm-cli.js")?;
        (node.is_file() && npm.is_file()).then_some((node, npm))
    });
    let mut command = match managed {
        Some((node, npm)) => {
            let mut command = std::process::Command::new(node);
            command.arg(npm);
            command
        }
        None => std::process::Command::new("npm"),
    };
    let output = match command
        .args(["view", package, "versions", "--json"])
        .output()
    {
        Ok(output) if output.status.success() => output.stdout,
        _ => return Vec::new(),
    };
    let mut versions: Vec<String> = match serde_json::from_slice(&output) {
        Ok(serde_json::Value::Array(items)) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        Ok(serde_json::Value::String(one)) => vec![one],
        _ => return Vec::new(),
    };
    versions.reverse();
    versions.truncate(100);
    versions
}

#[allow(clippy::too_many_arguments)]
fn install_binary(
    agent: &RegistryAgent,
    target: &BinaryTarget,
    root: &Path,
    name: &PackageName,
    file: &str,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<(), String> {
    let digest = target
        .sha256
        .as_ref()
        .ok_or_else(|| format!("ACP registry has no SHA-256 digest for {}", agent.id))?;
    let staging_root = store::staging_dir(root);
    std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix(name.as_str())
        .tempdir_in(&staging_root)
        .map_err(|e| e.to_string())?;
    let dl = staging.path().join(file);

    emit(InstallPhase::Downloading, Some(0), &target.archive);
    download::download_to(
        &target.archive,
        &dl,
        download::PACKAGE_MAX_BYTES,
        digest,
        |d, total| {
            let pct = total.and_then(|t| (t > 0).then(|| ((d * 100) / t) as u8));
            emit(InstallPhase::Downloading, pct, "downloading");
        },
    )?;

    let staged_package = staging.path().join("package");
    emit(InstallPhase::Extracting, None, "extracting");
    archive::extract(
        &dl,
        archive::kind_for(file),
        &staged_package,
        cmd_basename(&target.cmd),
    )?;
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

    store::write_receipt_in(
        &staged_package,
        &store::Receipt {
            name: agent.id.clone(),
            version: agent.version.clone(),
            source_id: format!("acp:{}", agent.id),
            bin: Default::default(),
        },
    )
    .map_err(|error| error.to_string())?;
    store::activate_package(root, name, &staged_package).map_err(|error| error.to_string())?;
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
    fn acp_registry_suffix_is_omitted_from_agent_urls() {
        assert_eq!(agent_url_id("codex-acp"), "codex");
        assert_eq!(agent_url_id("custom-acp"), "custom");
        assert_eq!(agent_url_id("mistral-vibe"), "mistral-vibe");
    }

    #[test]
    fn agent_ids_match_url_and_registry_forms() {
        assert!(agent_ids_match("codex", "codex-acp"));
        assert!(agent_ids_match("custom", "custom-acp"));
        assert!(agent_ids_match("vibe", "mistral-vibe"));
        assert!(!agent_ids_match("codex", "custom-acp"));
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
        let node = node_bindir(&root).unwrap().join("node");
        std::fs::create_dir_all(node.parent().unwrap()).unwrap();
        std::fs::write(&node, b"").unwrap();
        let npx = node_cli(&root, "npx-cli.js").unwrap();
        std::fs::create_dir_all(npx.parent().unwrap()).unwrap();
        std::fs::write(npx, b"").unwrap();
        let installed = npx_agent("installed-agent");
        let available = npx_agent("available-agent");

        assert!(!is_agent_installed_at(&root, &installed));
        assert!(!is_agent_installed_at(&root, &available));

        write_agent_receipt(&root, &installed, None).unwrap();

        assert!(is_agent_installed_at(&root, &installed));
        assert!(!is_agent_installed_at(&root, &available));

        uninstall_at(&root, &installed.id).unwrap();

        assert!(!is_agent_installed_at(&root, &installed));
        assert!(node.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
