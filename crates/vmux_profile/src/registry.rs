//! Declarative local-tool manifest and Stow-style dotfile links.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

const MANIFEST_VERSION: u32 = 1;

/// Desired package and dotfile state stored in `registry.toml`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryManifest {
    #[serde(default = "manifest_version")]
    pub version: u32,
    #[serde(default)]
    pub packages: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub dotfiles: DotfilesManifest,
}

impl Default for RegistryManifest {
    fn default() -> Self {
        Self {
            version: MANIFEST_VERSION,
            packages: BTreeMap::new(),
            dotfiles: DotfilesManifest::default(),
        }
    }
}

impl RegistryManifest {
    /// Returns whether a provider package is managed by the manifest.
    pub fn contains(&self, provider: &str, name: &str) -> bool {
        self.packages
            .get(provider)
            .is_some_and(|packages| packages.iter().any(|package| package == name))
    }

    /// Adds or removes a provider package and normalizes ordering.
    pub fn set_package(&mut self, provider: &str, name: &str, enabled: bool) {
        if enabled {
            let packages = self.packages.entry(provider.to_string()).or_default();
            if !packages.iter().any(|package| package == name) {
                packages.push(name.to_string());
            }
        } else if let Some(packages) = self.packages.get_mut(provider) {
            packages.retain(|package| package != name);
            if packages.is_empty() {
                self.packages.remove(provider);
            }
        }
        self.normalize();
    }

    /// Enables or disables a Stow-style dotfile package.
    pub fn set_dotfile_package(&mut self, name: &str, enabled: bool) {
        if enabled {
            if !self.dotfiles.packages.iter().any(|package| package == name) {
                self.dotfiles.packages.push(name.to_string());
            }
        } else {
            self.dotfiles.packages.retain(|package| package != name);
        }
        self.normalize();
    }

    fn normalize(&mut self) {
        self.packages.retain(|_, packages| {
            packages.sort_by_key(|package| package.to_ascii_lowercase());
            packages.dedup();
            !packages.is_empty()
        });
        self.dotfiles
            .packages
            .sort_by_key(|package| package.to_ascii_lowercase());
        self.dotfiles.packages.dedup();
    }
}

/// Enabled package directories under the Registry dotfile root.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DotfilesManifest {
    #[serde(default)]
    pub packages: Vec<String>,
}

/// Current relationship between one Registry source and its home target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DotfileLinkState {
    Linked,
    Missing,
    Conflict,
}

/// Planned source-to-home link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DotfileLink {
    pub source: PathBuf,
    pub target: PathBuf,
    pub state: DotfileLinkState,
}

/// Complete non-mutating plan for one dotfile package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DotfilePlan {
    pub package: String,
    pub links: Vec<DotfileLink>,
}

impl DotfilePlan {
    /// Number of targets already linked to the expected source.
    pub fn linked(&self) -> usize {
        self.links
            .iter()
            .filter(|link| link.state == DotfileLinkState::Linked)
            .count()
    }

    /// Number of absent targets that Apply would create.
    pub fn missing(&self) -> usize {
        self.links
            .iter()
            .filter(|link| link.state == DotfileLinkState::Missing)
            .count()
    }

    /// Number of targets blocking Apply.
    pub fn conflicts(&self) -> usize {
        self.links
            .iter()
            .filter(|link| link.state == DotfileLinkState::Conflict)
            .count()
    }
}

/// Profile-agnostic Registry directory under `~/.vmux`.
pub fn root_dir() -> PathBuf {
    super::config_dir().join("registry")
}

/// Desired-state manifest path.
pub fn manifest_path() -> PathBuf {
    root_dir().join("registry.toml")
}

/// Root of Stow-style package directories.
pub fn dotfiles_dir() -> PathBuf {
    root_dir().join("dotfiles")
}

/// Loads the user Registry manifest, returning an empty in-memory manifest when absent.
pub fn load_manifest() -> Result<RegistryManifest, String> {
    load_manifest_from(&manifest_path())
}

/// Loads and validates a Registry manifest from an explicit path.
pub fn load_manifest_from(path: &Path) -> Result<RegistryManifest, String> {
    if !path.is_file() {
        return Ok(RegistryManifest::default());
    }
    let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut manifest: RegistryManifest =
        toml::from_str(&source).map_err(|error| error.to_string())?;
    if manifest.version != MANIFEST_VERSION {
        return Err(format!(
            "unsupported registry manifest version: {}",
            manifest.version
        ));
    }
    manifest.normalize();
    Ok(manifest)
}

/// Atomically writes the normalized user Registry manifest.
pub fn write_manifest(manifest: &RegistryManifest) -> Result<(), String> {
    write_manifest_to(&manifest_path(), manifest)
}

/// Atomically writes a normalized Registry manifest to an explicit path.
pub fn write_manifest_to(path: &Path, manifest: &RegistryManifest) -> Result<(), String> {
    let mut manifest = manifest.clone();
    manifest.version = MANIFEST_VERSION;
    manifest.normalize();
    let source = toml::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    let parent = path.parent().ok_or("registry manifest has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("toml.tmp");
    std::fs::write(&temporary, source).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())
}

/// Lists valid dotfile package directories.
pub fn dotfile_packages() -> Vec<String> {
    dotfile_packages_in(&dotfiles_dir())
}

/// Lists valid dotfile package directories below an explicit root.
pub fn dotfile_packages_in(root: &Path) -> Vec<String> {
    let mut packages = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_package_name(name))
        .collect::<Vec<_>>();
    packages.sort_by_key(|package| package.to_ascii_lowercase());
    packages
}

/// Builds a non-mutating link plan for one user dotfile package.
pub fn plan_dotfile_package(package: &str) -> Result<DotfilePlan, String> {
    plan_dotfile_package_in(&dotfiles_dir(), &home_dir(), package)
}

/// Builds a non-mutating link plan with explicit roots.
pub fn plan_dotfile_package_in(
    dotfiles_root: &Path,
    home: &Path,
    package: &str,
) -> Result<DotfilePlan, String> {
    validate_package_name(package)?;
    let package_root = dotfiles_root.join(package);
    if !package_root.is_dir() {
        return Err(format!("dotfile package does not exist: {package}"));
    }
    let mut sources = Vec::new();
    collect_files(&package_root, &mut sources).map_err(|error| error.to_string())?;
    sources.sort();
    let links = sources
        .into_iter()
        .filter_map(|source| {
            let relative = source.strip_prefix(&package_root).ok()?;
            let target = home.join(relative);
            let state = link_state(&source, &target);
            Some(DotfileLink {
                source,
                target,
                state,
            })
        })
        .collect();
    Ok(DotfilePlan {
        package: package.to_string(),
        links,
    })
}

/// Applies one user dotfile package transactionally.
pub fn apply_dotfile_package(package: &str) -> Result<usize, String> {
    apply_dotfile_package_in(&dotfiles_dir(), &home_dir(), package)
}

/// Applies one dotfile package with explicit roots.
pub fn apply_dotfile_package_in(
    dotfiles_root: &Path,
    home: &Path,
    package: &str,
) -> Result<usize, String> {
    let plan = plan_dotfile_package_in(dotfiles_root, home, package)?;
    apply_dotfile_plan(&plan).map(|created| created.len())
}

fn apply_dotfile_plan(plan: &DotfilePlan) -> Result<Vec<PathBuf>, String> {
    if plan.conflicts() > 0 {
        return Err(format!(
            "dotfile package {} has {} conflict(s)",
            plan.package,
            plan.conflicts()
        ));
    }
    let mut created = Vec::new();
    for link in plan
        .links
        .iter()
        .filter(|link| link.state == DotfileLinkState::Missing)
    {
        let result = (|| -> io::Result<()> {
            if let Some(parent) = link.target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            create_relative_symlink(&link.source, &link.target)
        })();
        if let Err(error) = result {
            for target in created.iter().rev() {
                let _ = std::fs::remove_file(target);
            }
            return Err(error.to_string());
        }
        created.push(link.target.clone());
    }
    Ok(created)
}

/// Removes links owned by one user dotfile package.
pub fn unlink_dotfile_package(package: &str) -> Result<usize, String> {
    unlink_dotfile_package_in(&dotfiles_dir(), &home_dir(), package)
}

/// Removes links owned by one dotfile package with explicit roots.
pub fn unlink_dotfile_package_in(
    dotfiles_root: &Path,
    home: &Path,
    package: &str,
) -> Result<usize, String> {
    let plan = plan_dotfile_package_in(dotfiles_root, home, package)?;
    let mut removed = 0;
    for link in plan
        .links
        .iter()
        .filter(|link| link.state == DotfileLinkState::Linked)
    {
        std::fs::remove_file(&link.target).map_err(|error| error.to_string())?;
        removed += 1;
    }
    Ok(removed)
}

/// Applies every enabled dotfile package after a complete conflict preflight.
pub fn apply_enabled_dotfiles(manifest: &RegistryManifest) -> Result<usize, String> {
    apply_enabled_dotfiles_in(manifest, &dotfiles_dir(), &home_dir())
}

fn apply_enabled_dotfiles_in(
    manifest: &RegistryManifest,
    dotfiles_root: &Path,
    home: &Path,
) -> Result<usize, String> {
    let mut plans = Vec::new();
    for package in &manifest.dotfiles.packages {
        plans.push(plan_dotfile_package_in(dotfiles_root, home, package)?);
    }
    if let Some(plan) = plans.iter().find(|plan| plan.conflicts() > 0) {
        return Err(format!(
            "dotfile package {} has {} conflict(s)",
            plan.package,
            plan.conflicts()
        ));
    }
    let mut created = Vec::new();
    for plan in &plans {
        match apply_dotfile_plan(plan) {
            Ok(links) => created.extend(links),
            Err(error) => {
                for target in created.iter().rev() {
                    let _ = std::fs::remove_file(target);
                }
                return Err(error);
            }
        }
    }
    Ok(created.len())
}

/// Moves a home file into a package, links it back, and enables the package.
pub fn adopt_dotfile(path: &Path, package: &str) -> Result<PathBuf, String> {
    adopt_dotfile_in(
        &dotfiles_dir(),
        &home_dir(),
        &manifest_path(),
        path,
        package,
    )
}

/// Adopts a home file using explicit Registry and manifest paths.
pub fn adopt_dotfile_in(
    dotfiles_root: &Path,
    home: &Path,
    manifest_path: &Path,
    path: &Path,
    package: &str,
) -> Result<PathBuf, String> {
    validate_package_name(package)?;
    let path = if let Ok(relative) = path.strip_prefix("~") {
        home.join(relative)
    } else if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    if !path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_file())
    {
        return Err(format!("dotfile is not a file: {}", path.display()));
    }
    if path.starts_with(dotfiles_root) {
        return Err("dotfile is already inside the registry".to_string());
    }
    let relative = path
        .strip_prefix(home)
        .map_err(|_| format!("dotfile must be inside {}", home.display()))?;
    if relative.as_os_str().is_empty() || contains_parent_component(relative) {
        return Err("invalid dotfile path".to_string());
    }
    let destination = dotfiles_root.join(package).join(relative);
    if destination.exists() || destination.symlink_metadata().is_ok() {
        return Err(format!(
            "registry dotfile already exists: {}",
            destination.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&path, &destination).map_err(|error| error.to_string())?;
    if let Err(error) = create_relative_symlink(&destination, &path) {
        let _ = std::fs::rename(&destination, &path);
        return Err(error.to_string());
    }
    let mut manifest = load_manifest_from(manifest_path)?;
    manifest.set_dotfile_package(package, true);
    if let Err(error) = write_manifest_to(manifest_path, &manifest) {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::rename(&destination, &path);
        return Err(error);
    }
    Ok(destination)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn manifest_version() -> u32 {
    MANIFEST_VERSION
}

fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
}

fn validate_package_name(name: &str) -> Result<(), String> {
    valid_package_name(name)
        .then_some(())
        .ok_or_else(|| format!("invalid dotfile package name: {name}"))
}

fn collect_files(directory: &Path, output: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_files(&path, output)?;
        } else if file_type.is_file() {
            output.push(path);
        }
    }
    Ok(())
}

fn link_state(source: &Path, target: &Path) -> DotfileLinkState {
    let Ok(metadata) = target.symlink_metadata() else {
        return DotfileLinkState::Missing;
    };
    if !metadata.file_type().is_symlink() {
        return DotfileLinkState::Conflict;
    }
    let Ok(link) = std::fs::read_link(target) else {
        return DotfileLinkState::Conflict;
    };
    let resolved = if link.is_absolute() {
        link
    } else {
        target.parent().unwrap_or(Path::new("/")).join(link)
    };
    if canonical_or_normalized(&resolved) == canonical_or_normalized(source) {
        DotfileLinkState::Linked
    } else {
        DotfileLinkState::Conflict
    }
}

fn canonical_or_normalized(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| normalize(path))
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn contains_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| component == Component::ParentDir)
}

fn create_relative_symlink(source: &Path, target: &Path) -> io::Result<()> {
    let parent = target.parent().unwrap_or(Path::new("/"));
    let relative = relative_path(parent, source);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(relative, target)
    }
    #[cfg(not(unix))]
    {
        std::os::windows::fs::symlink_file(relative, target)
    }
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from = normalize(from);
    let to = normalize(to);
    let from_components = from.components().collect::<Vec<_>>();
    let to_components = to.components().collect::<Vec<_>>();
    let common = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();
    for _ in common..from_components.len() {
        relative.push("..");
    }
    for component in &to_components[common..] {
        relative.push(component.as_os_str());
    }
    relative
}

/// Returns the manifest's desired package set for one provider.
pub fn managed_package_set(manifest: &RegistryManifest, provider: &str) -> BTreeSet<String> {
    manifest
        .packages
        .get(provider)
        .into_iter()
        .flatten()
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_roundtrip_normalizes_packages() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("registry.toml");
        let mut manifest = RegistryManifest::default();
        manifest.set_package("npm", "typescript", true);
        manifest.set_package("npm", "eslint", true);
        manifest.set_package("npm", "typescript", true);
        manifest.set_dotfile_package("shell", true);
        write_manifest_to(&path, &manifest).unwrap();

        let loaded = load_manifest_from(&path).unwrap();
        assert_eq!(loaded.packages["npm"], ["eslint", "typescript"]);
        assert_eq!(loaded.dotfiles.packages, ["shell"]);
    }

    #[test]
    fn unsupported_manifest_versions_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("registry.toml");
        std::fs::write(&path, "version = 2\n").unwrap();
        assert!(
            load_manifest_from(&path)
                .unwrap_err()
                .contains("unsupported registry manifest version: 2")
        );
    }

    #[cfg(unix)]
    #[test]
    fn plan_apply_and_unlink_dotfile_package() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let dotfiles = temp.path().join("registry/dotfiles");
        std::fs::create_dir_all(dotfiles.join("shell/.config/nushell")).unwrap();
        std::fs::write(dotfiles.join("shell/.config/nushell/config.nu"), "echo hi").unwrap();

        let plan = plan_dotfile_package_in(&dotfiles, &home, "shell").unwrap();
        assert_eq!(plan.missing(), 1);
        assert_eq!(
            apply_dotfile_package_in(&dotfiles, &home, "shell").unwrap(),
            1
        );
        let target = home.join(".config/nushell/config.nu");
        assert!(target.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "echo hi");
        assert_eq!(
            unlink_dotfile_package_in(&dotfiles, &home, "shell").unwrap(),
            1
        );
        assert!(!target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn conflicts_block_the_entire_apply() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let dotfiles = temp.path().join("registry/dotfiles");
        std::fs::create_dir_all(dotfiles.join("git")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(dotfiles.join("git/.gitconfig"), "managed").unwrap();
        std::fs::write(home.join(".gitconfig"), "existing").unwrap();

        let error = apply_dotfile_package_in(&dotfiles, &home, "git").unwrap_err();
        assert!(error.contains("1 conflict"));
        assert_eq!(
            std::fs::read_to_string(home.join(".gitconfig")).unwrap(),
            "existing"
        );
    }

    #[cfg(unix)]
    #[test]
    fn enabled_packages_are_preflighted_before_any_links_are_created() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let dotfiles = temp.path().join("registry/dotfiles");
        std::fs::create_dir_all(dotfiles.join("git")).unwrap();
        std::fs::create_dir_all(dotfiles.join("shell/.config/nushell")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(dotfiles.join("git/.gitconfig"), "managed").unwrap();
        std::fs::write(dotfiles.join("shell/.config/nushell/config.nu"), "echo hi").unwrap();
        std::fs::write(home.join(".gitconfig"), "existing").unwrap();
        let mut manifest = RegistryManifest::default();
        manifest.set_dotfile_package("shell", true);
        manifest.set_dotfile_package("git", true);

        let result = apply_enabled_dotfiles_in(&manifest, &dotfiles, &home);

        assert!(result.unwrap_err().contains("git"));
        assert!(!home.join(".config/nushell/config.nu").exists());
    }

    #[cfg(unix)]
    #[test]
    fn adopt_moves_file_links_it_and_updates_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let dotfiles = temp.path().join("registry/dotfiles");
        let manifest = temp.path().join("registry/registry.toml");
        std::fs::create_dir_all(home.join(".config/nushell")).unwrap();
        let source = home.join(".config/nushell/config.nu");
        std::fs::write(&source, "echo hi").unwrap();

        let destination = adopt_dotfile_in(&dotfiles, &home, &manifest, &source, "shell").unwrap();
        assert_eq!(
            destination,
            dotfiles.join("shell/.config/nushell/config.nu")
        );
        assert!(source.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(source).unwrap(), "echo hi");
        assert_eq!(
            load_manifest_from(&manifest).unwrap().dotfiles.packages,
            ["shell"]
        );
    }
}
