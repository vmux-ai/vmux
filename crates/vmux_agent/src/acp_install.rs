//! Installing ACP agents from the registry into the vmux-managed runtime store,
//! reusing the Mason download/extract/receipt primitives from `vmux_editor`. This module covers
//! the `binary` distribution (native executable, no runtime); the `npx` (managed Node) and `uvx`
//! (managed uv) paths land in later steps.

use std::path::{Path, PathBuf};

use vmux_core::event::InstallPhase;
use vmux_editor::lsp::{archive, download, store};

use crate::acp_registry::{self, BinaryTarget, RegistryAgent};

/// How to launch an installed agent: an absolute command plus its args/env, and an optional
/// directory to prepend to the child's `PATH` (the managed runtime's `bin/`, so e.g. `npx` can
/// find its `node`). `None` for self-contained native binaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgent {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub path_prepend: Option<String>,
}

/// Pinned managed runtime versions (manual bumps).
const NODE_VERSION: &str = "22.11.0";
const UV_VERSION: &str = "0.5.11";

/// The vmux-managed agent store root.
fn store_root() -> PathBuf {
    acp_registry::agents_dir()
}

fn write_agent_receipt(root: &Path, agent: &RegistryAgent) -> Result<(), String> {
    store::write_receipt(
        root,
        &store::Receipt {
            name: agent.id.clone(),
            version: agent.version.clone(),
            source_id: format!("acp:{}", agent.id),
            bin: std::collections::BTreeMap::new(),
        },
    )
    .map_err(|e| e.to_string())
}

/// Final path component of a manifest `cmd` (`"./bin/agent"` → `"agent"`), used as the output
/// name when extracting single-file archives (`.gz`/raw).
fn cmd_basename(cmd: &str) -> &str {
    let rel = cmd.trim_start_matches("./").trim_start_matches(".\\");
    Path::new(rel)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(rel)
}

/// Last path segment of a URL (query/fragment stripped), used as the downloaded archive's filename.
fn archive_filename(url: &str) -> &str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("archive")
}

/// Absolute path of the agent's executable inside its extracted package dir. tar/zip archives
/// unpack their whole tree (so `cmd` is relative to the root); `.gz`/raw yield a single file
/// named by the `cmd` basename.
fn resolved_cmd_path(pkgdir: &Path, target: &BinaryTarget, file: &str) -> PathBuf {
    let rel = target
        .cmd
        .trim_start_matches("./")
        .trim_start_matches(".\\");
    match archive::kind_for(file) {
        archive::ArchiveKind::TarGz | archive::ArchiveKind::Zip => pkgdir.join(rel),
        archive::ArchiveKind::Gz | archive::ArchiveKind::Raw => {
            pkgdir.join(cmd_basename(&target.cmd))
        }
    }
}

/// Ensure a `binary`-distribution agent is installed (download + extract + chmod + receipt),
/// then return how to launch it. Re-installs when the receipt is missing, the executable is
/// gone, or the registry version has moved.
pub fn ensure_binary_installed(
    agent: &RegistryAgent,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let target = agent
        .binary_for_host()
        .ok_or_else(|| format!("no binary distribution for this platform: {}", agent.id))?;
    let root = store_root();
    let pkgdir = store::packages_dir(&root).join(&agent.id);
    let file = archive_filename(&target.archive).to_string();
    let cmd_path = resolved_cmd_path(&pkgdir, target, &file);

    let up_to_date = store::read_receipt(&root, &agent.id)
        .map(|r| r.version == agent.version)
        .unwrap_or(false);
    if !up_to_date || !cmd_path.exists() {
        install_binary(agent, target, &root, &pkgdir, &file, &cmd_path, &mut emit)?;
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

/// Node's platform naming (differs from ACP targets). `None` on platforms we don't manage Node
/// for yet (Windows uses `.zip` archives — deferred).
fn node_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("darwin-arm64"),
        ("macos", "x86_64") => Some("darwin-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("linux", "x86_64") => Some("linux-x64"),
        _ => None,
    }
}

/// The managed Node `bin/` (with `node`/`npm`/`npx`), downloading the pinned Node if absent.
fn ensure_node(
    root: &Path,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<PathBuf, String> {
    let target = node_target().ok_or("managed Node not supported on this platform")?;
    let dirname = format!("node-v{NODE_VERSION}-{target}");
    let node_parent = store::packages_dir(root).join("node");
    let bindir = node_parent.join(&dirname).join("bin");
    if bindir.join("node").exists() {
        return Ok(bindir);
    }

    let file = format!("{dirname}.tar.gz");
    let url = format!("https://nodejs.org/dist/v{NODE_VERSION}/{file}");
    let staging = store::staging_dir(root).join("node");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let dl = staging.join(&file);

    emit(
        InstallPhase::Downloading,
        Some(0),
        "downloading Node runtime",
    );
    download::download_to(&url, &dl, |d, total| {
        let pct = total.and_then(|t| (t > 0).then(|| ((d * 100) / t) as u8));
        emit(InstallPhase::Downloading, pct, "downloading Node runtime");
    })?;

    let _ = std::fs::create_dir_all(&node_parent);
    emit(InstallPhase::Extracting, None, "extracting Node runtime");
    archive::extract(&dl, archive::ArchiveKind::TarGz, &node_parent, &dirname)?;
    let _ = std::fs::remove_dir_all(&staging);
    if !bindir.join("node").exists() {
        return Err("managed Node missing after extract".to_string());
    }
    Ok(bindir)
}

/// Ensure an `npx`-distribution agent can run: install the managed Node, then return an
/// `npx -y <package> <args>` launch spec that resolves `node` via the managed `bin/`.
pub fn ensure_npx_installed(
    agent: &RegistryAgent,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let dist = agent
        .distribution
        .npx
        .as_ref()
        .ok_or_else(|| format!("no npx distribution: {}", agent.id))?;
    let root = store_root();
    let bindir = ensure_node(&root, &mut emit)?;
    write_agent_receipt(&root, agent)?;
    emit(InstallPhase::Done, Some(100), "ready");

    let mut args = vec!["-y".to_string(), dist.package.clone()];
    args.extend(dist.args.iter().cloned());
    Ok(ResolvedAgent {
        command: bindir.join("npx").to_string_lossy().into_owned(),
        args,
        env: dist
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        path_prepend: Some(bindir.to_string_lossy().into_owned()),
    })
}

/// uv's release target triple (Astral naming). `None` on platforms we don't manage uv for yet.
fn uv_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        _ => None,
    }
}

/// The managed uv dir (containing `uv`/`uvx`), downloading the pinned uv if absent.
fn ensure_uv(
    root: &Path,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<PathBuf, String> {
    let target = uv_target().ok_or("managed uv not supported on this platform")?;
    let dirname = format!("uv-{target}");
    let uv_parent = store::packages_dir(root).join("uv");
    let bindir = uv_parent.join(&dirname);
    if bindir.join("uvx").exists() {
        return Ok(bindir);
    }

    let file = format!("{dirname}.tar.gz");
    let url = format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{file}");
    let staging = store::staging_dir(root).join("uv");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let dl = staging.join(&file);

    emit(InstallPhase::Downloading, Some(0), "downloading uv runtime");
    download::download_to(&url, &dl, |d, total| {
        let pct = total.and_then(|t| (t > 0).then(|| ((d * 100) / t) as u8));
        emit(InstallPhase::Downloading, pct, "downloading uv runtime");
    })?;

    let _ = std::fs::create_dir_all(&uv_parent);
    emit(InstallPhase::Extracting, None, "extracting uv runtime");
    archive::extract(&dl, archive::ArchiveKind::TarGz, &uv_parent, &dirname)?;
    let _ = std::fs::remove_dir_all(&staging);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for exe in ["uv", "uvx"] {
            let p = bindir.join(exe);
            if let Ok(meta) = std::fs::metadata(&p) {
                let mut perm = meta.permissions();
                perm.set_mode(0o755);
                let _ = std::fs::set_permissions(&p, perm);
            }
        }
    }
    if !bindir.join("uvx").exists() {
        return Err("managed uv missing after extract".to_string());
    }
    Ok(bindir)
}

/// Ensure a `uvx`-distribution agent can run: install the managed uv, then return a
/// `uvx <package> <args>` launch spec that resolves `uv` via the managed dir.
pub fn ensure_uvx_installed(
    agent: &RegistryAgent,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    let dist = agent
        .distribution
        .uvx
        .as_ref()
        .ok_or_else(|| format!("no uvx distribution: {}", agent.id))?;
    let root = store_root();
    let bindir = ensure_uv(&root, &mut emit)?;
    write_agent_receipt(&root, agent)?;
    emit(InstallPhase::Done, Some(100), "ready");

    let mut args = vec![dist.package.clone()];
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

/// Managed Node `bin/` path (whether or not it is installed).
fn node_bindir(root: &Path) -> Option<PathBuf> {
    let target = node_target()?;
    Some(
        store::packages_dir(root)
            .join("node")
            .join(format!("node-v{NODE_VERSION}-{target}"))
            .join("bin"),
    )
}

/// Managed uv dir path (whether or not it is installed).
fn uv_bindir(root: &Path) -> Option<PathBuf> {
    let target = uv_target()?;
    Some(
        store::packages_dir(root)
            .join("uv")
            .join(format!("uv-{target}")),
    )
}

/// Whether the agent has its own receipt and any required managed runtime is present.
pub fn is_agent_installed(agent: &RegistryAgent) -> bool {
    is_agent_installed_at(&store_root(), agent)
}

fn is_agent_installed_at(root: &Path, agent: &RegistryAgent) -> bool {
    if !store::is_installed(root, &agent.id) {
        return false;
    }
    match agent.preferred_runtime() {
        acp_registry::Runtime::None => true,
        acp_registry::Runtime::Node => node_bindir(root)
            .map(|b| b.join("node").exists())
            .unwrap_or(false),
        acp_registry::Runtime::Uv => uv_bindir(root)
            .map(|b| b.join("uvx").exists())
            .unwrap_or(false),
    }
}

/// Whether a newer version is available for an installed native-binary agent.
pub fn is_update_available(agent: &RegistryAgent) -> bool {
    matches!(agent.preferred_runtime(), acp_registry::Runtime::None)
        && store::read_receipt(&store_root(), &agent.id)
            .map(|r| r.version != agent.version)
            .unwrap_or(false)
}

/// Remove an agent's receipt and native package. Shared npx/uvx runtimes remain installed.
pub fn uninstall(id: &str) -> Result<(), String> {
    uninstall_at(&store_root(), id)
}

fn uninstall_at(root: &Path, id: &str) -> Result<(), String> {
    store::remove(root, id).map_err(|e| e.to_string())
}

/// Map a vmux launcher id to its ACP-registry id where they differ (the built-in CLI ids vs.
/// the registry slugs). Unknown ids pass through unchanged.
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

/// Resolve an agent by launcher id against the registry (cached, else fetched) and ensure it is
/// installed, returning how to launch it. Runs on a background thread (blocking I/O).
pub fn resolve_from_registry(
    agent_id: &str,
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
    ensure_installed(&agent, emit)
}

/// Ensure any registry agent is installed and return how to launch it, preferring a native
/// binary, then npx (managed Node), then uvx (managed uv).
pub fn ensure_installed(
    agent: &RegistryAgent,
    emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<ResolvedAgent, String> {
    use acp_registry::Runtime;
    match agent.preferred_runtime() {
        Runtime::None => ensure_binary_installed(agent, emit),
        Runtime::Node => ensure_npx_installed(agent, emit),
        Runtime::Uv => ensure_uvx_installed(agent, emit),
    }
}

#[allow(clippy::too_many_arguments)]
fn install_binary(
    agent: &RegistryAgent,
    target: &BinaryTarget,
    root: &Path,
    pkgdir: &Path,
    file: &str,
    cmd_path: &Path,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<(), String> {
    let staging = store::staging_dir(root).join(&agent.id);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let dl = staging.join(file);

    emit(InstallPhase::Downloading, Some(0), &target.archive);
    download::download_to(&target.archive, &dl, |d, total| {
        let pct = total.and_then(|t| (t > 0).then(|| ((d * 100) / t) as u8));
        emit(InstallPhase::Downloading, pct, "downloading");
    })?;

    let _ = std::fs::remove_dir_all(pkgdir);
    std::fs::create_dir_all(pkgdir).map_err(|e| e.to_string())?;
    emit(InstallPhase::Extracting, None, "extracting");
    archive::extract(
        &dl,
        archive::kind_for(file),
        pkgdir,
        cmd_basename(&target.cmd),
    )?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(cmd_path) {
            let mut perm = meta.permissions();
            perm.set_mode(0o755);
            let _ = std::fs::set_permissions(cmd_path, perm);
        }
    }
    if !cmd_path.exists() {
        return Err(format!(
            "acp install: executable {} missing after extract (cmd={})",
            cmd_path.display(),
            target.cmd
        ));
    }

    write_agent_receipt(root, agent)?;
    let _ = std::fs::remove_dir_all(&staging);
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
            args: vec![],
            env: Default::default(),
        };
        assert_eq!(
            resolved_cmd_path(pkg, &tar, "a.tar.gz"),
            Path::new("/pkg/bin/agent")
        );
        let gz = BinaryTarget {
            archive: "https://x/a.gz".into(),
            cmd: "./agent".into(),
            args: vec![],
            env: Default::default(),
        };
        assert_eq!(resolved_cmd_path(pkg, &gz, "a.gz"), Path::new("/pkg/agent"));
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
        let installed = npx_agent("installed-agent");
        let available = npx_agent("available-agent");

        assert!(!is_agent_installed_at(&root, &installed));
        assert!(!is_agent_installed_at(&root, &available));

        write_agent_receipt(&root, &installed).unwrap();

        assert!(is_agent_installed_at(&root, &installed));
        assert!(!is_agent_installed_at(&root, &available));

        uninstall_at(&root, &installed.id).unwrap();

        assert!(!is_agent_installed_at(&root, &installed));
        assert!(node.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
