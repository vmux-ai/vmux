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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub packages: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "McpManifest::is_empty")]
    pub mcp: McpManifest,
    #[serde(default, skip_serializing_if = "DotfilesManifest::is_empty")]
    pub dotfiles: DotfilesManifest,
}

impl Default for RegistryManifest {
    fn default() -> Self {
        Self {
            version: MANIFEST_VERSION,
            packages: BTreeMap::new(),
            mcp: McpManifest::default(),
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
        self.mcp.servers.remove("vmux");
    }
}

/// MCP servers vmux injects into agents it launches.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpManifest {
    #[serde(default)]
    pub servers: BTreeMap<String, McpServerManifest>,
}

impl McpManifest {
    fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

/// Portable MCP server definition normalized from Claude, Codex, or Vibe config.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerManifest {
    pub transport: McpTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub header_env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer_token_env_var: Option<String>,
}

impl McpServerManifest {
    /// Resolves direct and environment-backed headers for clients that require literal values.
    pub fn resolved_headers(&self) -> BTreeMap<String, String> {
        let mut headers = self.headers.clone();
        for (name, variable) in &self.header_env {
            if let Ok(value) = std::env::var(variable) {
                headers.insert(name.clone(), value);
            }
        }
        if let Some(variable) = &self.bearer_token_env_var
            && let Ok(value) = std::env::var(variable)
        {
            headers.insert("Authorization".to_string(), format!("Bearer {value}"));
        }
        headers
    }
}

/// Transport shared by supported MCP client config formats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
    Sse,
}

/// Package names parsed from one Brewfile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrewfileImport {
    pub formulae: Vec<String>,
    pub casks: Vec<String>,
}

/// Enabled package directories under the Registry dotfile root.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DotfilesManifest {
    #[serde(default)]
    pub packages: Vec<String>,
}

impl DotfilesManifest {
    fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
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

/// Imports formulae and casks from a Brewfile into the Registry manifest.
pub fn import_brewfile(path: &Path) -> Result<(usize, usize), String> {
    import_brewfile_to(path, &manifest_path())
}

/// Imports a Brewfile into an explicit Registry manifest.
pub fn import_brewfile_to(path: &Path, manifest_path: &Path) -> Result<(usize, usize), String> {
    let path = expand_user_path(path)?;
    let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let imported = parse_brewfile(&source);
    if imported.formulae.is_empty() && imported.casks.is_empty() {
        return Err(format!("no formulae or casks found in {}", path.display()));
    }
    let mut manifest = load_manifest_from(manifest_path)?;
    let formulae = add_packages(&mut manifest, "homebrew-formula", &imported.formulae);
    let casks = add_packages(&mut manifest, "homebrew-cask", &imported.casks);
    write_manifest_to(manifest_path, &manifest)?;
    Ok((formulae, casks))
}

/// Parses the Homebrew formula and cask declarations understood by `brew bundle`.
pub fn parse_brewfile(source: &str) -> BrewfileImport {
    let mut import = BrewfileImport::default();
    for line in source.lines() {
        if let Some(name) = parse_quoted_call(line, "brew") {
            import.formulae.push(name);
        } else if let Some(name) = parse_quoted_call(line, "cask") {
            import.casks.push(name);
        }
    }
    normalize_names(&mut import.formulae);
    normalize_names(&mut import.casks);
    import
}

/// Imports dependency names from a package.json as global npm desired state.
pub fn import_npm_manifest(path: &Path) -> Result<usize, String> {
    import_npm_manifest_to(path, &manifest_path())
}

/// Imports a package.json into an explicit Registry manifest.
pub fn import_npm_manifest_to(path: &Path, manifest_path: &Path) -> Result<usize, String> {
    let path = expand_user_path(path)?;
    let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let packages = parse_npm_manifest(&source)?;
    if packages.is_empty() {
        return Err(format!("no dependencies found in {}", path.display()));
    }
    let mut manifest = load_manifest_from(manifest_path)?;
    let imported = add_packages(&mut manifest, "npm", &packages);
    write_manifest_to(manifest_path, &manifest)?;
    Ok(imported)
}

/// Parses installable dependency names from package.json.
pub fn parse_npm_manifest(source: &str) -> Result<Vec<String>, String> {
    let document: serde_json::Value =
        serde_json::from_str(source).map_err(|error| error.to_string())?;
    let mut packages = Vec::new();
    for field in ["dependencies", "devDependencies", "optionalDependencies"] {
        if let Some(entries) = document.get(field).and_then(serde_json::Value::as_object) {
            packages.extend(entries.keys().cloned());
        }
    }
    normalize_names(&mut packages);
    Ok(packages)
}

/// One MCP server discovered in one or more external client configs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredMcpServer {
    pub definition: McpServerManifest,
    pub sources: Vec<PathBuf>,
    pub conflict: bool,
}

/// Default global MCP config files supported by Registry import.
pub fn default_mcp_config_paths() -> Vec<PathBuf> {
    let home = home_dir();
    [
        home.join(".codex/config.toml"),
        home.join(".claude.json"),
        home.join(".vibe/config.toml"),
        home.join(".mcp.json"),
    ]
    .into_iter()
    .filter(|path| path.is_file())
    .collect()
}

/// Discovers MCP servers from the user's global Claude, Codex, and Vibe configs.
pub fn discover_mcp_servers() -> (BTreeMap<String, DiscoveredMcpServer>, Vec<String>) {
    let mut discovered = BTreeMap::<String, DiscoveredMcpServer>::new();
    let mut errors = Vec::new();
    for path in default_mcp_config_paths() {
        match parse_mcp_config_file(&path) {
            Ok(servers) => {
                for (name, definition) in servers {
                    match discovered.get_mut(&name) {
                        Some(existing) => {
                            existing.conflict |= existing.definition != definition;
                            existing.sources.push(path.clone());
                        }
                        None => {
                            discovered.insert(
                                name,
                                DiscoveredMcpServer {
                                    definition,
                                    sources: vec![path.clone()],
                                    conflict: false,
                                },
                            );
                        }
                    }
                }
            }
            Err(error) => errors.push(format!("{}: {error}", path.display())),
        }
    }
    (discovered, errors)
}

/// Imports MCP servers from one Claude, Codex, or Vibe config.
pub fn import_mcp_config(path: &Path) -> Result<usize, String> {
    import_mcp_config_to(path, &manifest_path())
}

/// Imports MCP servers into an explicit Registry manifest.
pub fn import_mcp_config_to(path: &Path, manifest_path: &Path) -> Result<usize, String> {
    let path = expand_user_path(path)?;
    let servers = parse_mcp_config_file(&path)?;
    if servers.is_empty() {
        return Err(format!("no MCP servers found in {}", path.display()));
    }
    let mut manifest = load_manifest_from(manifest_path)?;
    let mut imported = 0;
    for (name, definition) in servers {
        if name == "vmux" {
            continue;
        }
        imported += usize::from(manifest.mcp.servers.get(&name) != Some(&definition));
        manifest.mcp.servers.insert(name, definition);
    }
    write_manifest_to(manifest_path, &manifest)?;
    Ok(imported)
}

/// Imports every unambiguous MCP server found in the default global configs.
pub fn import_default_mcp_configs() -> Result<usize, String> {
    let (discovered, errors) = discover_mcp_servers();
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    let conflicts = discovered
        .iter()
        .filter(|(_, server)| server.conflict)
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    if !conflicts.is_empty() {
        return Err(format!(
            "conflicting MCP definitions: {}",
            conflicts.join(", ")
        ));
    }
    let mut manifest = load_manifest()?;
    let mut imported = 0;
    for (name, server) in discovered {
        if name == "vmux" {
            continue;
        }
        imported += usize::from(manifest.mcp.servers.get(&name) != Some(&server.definition));
        manifest.mcp.servers.insert(name, server.definition);
    }
    write_manifest(&manifest)?;
    Ok(imported)
}

/// Imports one unambiguous MCP server discovered in the default global configs.
pub fn import_discovered_mcp_server(name: &str) -> Result<(), String> {
    let (discovered, errors) = discover_mcp_servers();
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    let server = discovered
        .get(name)
        .ok_or_else(|| format!("MCP server not found: {name}"))?;
    if server.conflict {
        return Err(format!(
            "MCP server {name} has conflicting definitions; import an explicit config path"
        ));
    }
    let mut manifest = load_manifest()?;
    manifest
        .mcp
        .servers
        .insert(name.to_string(), server.definition.clone());
    write_manifest(&manifest)
}

/// Parses MCP server definitions from a Claude JSON or Codex/Vibe TOML config.
pub fn parse_mcp_config_file(path: &Path) -> Result<BTreeMap<String, McpServerManifest>, String> {
    let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    parse_mcp_config(&source)
}

/// Parses MCP server definitions from JSON or TOML config text.
pub fn parse_mcp_config(source: &str) -> Result<BTreeMap<String, McpServerManifest>, String> {
    if let Ok(document) = serde_json::from_str::<serde_json::Value>(source) {
        return parse_json_mcp_document(&document);
    }
    let document: toml::Value = toml::from_str(source).map_err(|error| error.to_string())?;
    parse_toml_mcp_document(&document)
}

/// Copies package directories from an existing Stow root into Registry ownership.
pub fn import_dotfiles(path: &Path) -> Result<usize, String> {
    import_dotfiles_to(path, &dotfiles_dir(), &manifest_path())
}

/// Copies package directories into explicit Registry roots.
pub fn import_dotfiles_to(
    path: &Path,
    dotfiles_root: &Path,
    manifest_path: &Path,
) -> Result<usize, String> {
    let source = expand_user_path(path)?;
    if !source.is_dir() {
        return Err(format!(
            "dotfile root is not a directory: {}",
            source.display()
        ));
    }
    if source.starts_with(dotfiles_root) || dotfiles_root.starts_with(&source) {
        return Err("dotfile import source overlaps the Registry root".to_string());
    }
    let mut packages = std::fs::read_dir(&source)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            entry
                .file_name()
                .into_string()
                .ok()
                .map(|name| (name, entry.path()))
        })
        .filter(|(name, _)| valid_package_name(name))
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| left.0.cmp(&right.0));
    if packages.is_empty() {
        return Err(format!("no Stow packages found in {}", source.display()));
    }
    for (name, _) in &packages {
        if dotfiles_root.join(name).symlink_metadata().is_ok() {
            return Err(format!("Registry dotfile package already exists: {name}"));
        }
    }
    let mut manifest = load_manifest_from(manifest_path)?;
    std::fs::create_dir_all(dotfiles_root).map_err(|error| error.to_string())?;
    let mut staged = Vec::new();
    for (name, package_source) in &packages {
        let temporary = dotfiles_root.join(format!(".{name}.import-{}", std::process::id()));
        if temporary.symlink_metadata().is_ok() {
            std::fs::remove_dir_all(&temporary).map_err(|error| error.to_string())?;
        }
        if let Err(error) = copy_directory(package_source, &temporary) {
            for path in &staged {
                let _ = std::fs::remove_dir_all(path);
            }
            let _ = std::fs::remove_dir_all(&temporary);
            return Err(error);
        }
        staged.push(temporary);
    }
    let mut installed = Vec::new();
    for ((name, _), temporary) in packages.iter().zip(&staged) {
        let destination = dotfiles_root.join(name);
        if let Err(error) = std::fs::rename(temporary, &destination) {
            for path in &staged {
                let _ = std::fs::remove_dir_all(path);
            }
            for path in &installed {
                let _ = std::fs::remove_dir_all(path);
            }
            return Err(error.to_string());
        }
        installed.push(destination);
    }
    for (name, _) in &packages {
        manifest.set_dotfile_package(name, true);
    }
    if let Err(error) = write_manifest_to(manifest_path, &manifest) {
        for path in &installed {
            let _ = std::fs::remove_dir_all(path);
        }
        return Err(error);
    }
    Ok(packages.len())
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

fn add_packages(manifest: &mut RegistryManifest, provider: &str, names: &[String]) -> usize {
    let mut imported = 0;
    for name in names {
        imported += usize::from(!manifest.contains(provider, name));
        manifest.set_package(provider, name, true);
    }
    imported
}

fn normalize_names(names: &mut Vec<String>) {
    names.retain(|name| !name.trim().is_empty());
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup();
}

fn parse_quoted_call(line: &str, call: &str) -> Option<String> {
    let line = line.trim_start();
    let rest = line.strip_prefix(call)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let quote = rest.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let mut escaped = false;
    let mut name = String::new();
    for character in rest[quote.len_utf8()..].chars() {
        if escaped {
            name.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            return (!name.is_empty()).then_some(name);
        } else {
            name.push(character);
        }
    }
    None
}

fn parse_json_mcp_document(
    document: &serde_json::Value,
) -> Result<BTreeMap<String, McpServerManifest>, String> {
    let Some(servers) = document
        .get("mcpServers")
        .or_else(|| document.get("mcp_servers"))
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(BTreeMap::new());
    };
    let mut parsed = BTreeMap::new();
    for (name, value) in servers {
        if name == "vmux"
            || value.get("enabled").and_then(serde_json::Value::as_bool) == Some(false)
        {
            continue;
        }
        parsed.insert(name.clone(), parse_json_mcp_server(value)?);
    }
    Ok(parsed)
}

fn parse_toml_mcp_document(
    document: &toml::Value,
) -> Result<BTreeMap<String, McpServerManifest>, String> {
    let Some(servers) = document.get("mcp_servers") else {
        return Ok(BTreeMap::new());
    };
    let mut parsed = BTreeMap::new();
    match servers {
        toml::Value::Table(table) => {
            for (name, value) in table {
                if name == "vmux"
                    || value.get("enabled").and_then(toml::Value::as_bool) == Some(false)
                {
                    continue;
                }
                let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
                parsed.insert(name.clone(), parse_json_mcp_server(&value)?);
            }
        }
        toml::Value::Array(entries) => {
            for entry in entries {
                let name = entry
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .ok_or("MCP server is missing name")?;
                if name == "vmux"
                    || entry.get("enabled").and_then(toml::Value::as_bool) == Some(false)
                {
                    continue;
                }
                let value = serde_json::to_value(entry).map_err(|error| error.to_string())?;
                parsed.insert(name.to_string(), parse_json_mcp_server(&value)?);
            }
        }
        _ => return Err("mcp_servers must be a table or array".to_string()),
    }
    Ok(parsed)
}

fn parse_json_mcp_server(value: &serde_json::Value) -> Result<McpServerManifest, String> {
    let object = value.as_object().ok_or("MCP server must be an object")?;
    let command = string_field(object, "command");
    let url = string_field(object, "url");
    let transport = string_field(object, "transport")
        .or_else(|| string_field(object, "type"))
        .map(|transport| match transport.as_str() {
            "sse" => McpTransport::Sse,
            "http" | "streamable-http" => McpTransport::Http,
            _ => McpTransport::Stdio,
        })
        .unwrap_or_else(|| {
            if url.is_some() {
                McpTransport::Http
            } else {
                McpTransport::Stdio
            }
        });
    match transport {
        McpTransport::Stdio if command.is_none() => {
            return Err("stdio MCP server is missing command".to_string());
        }
        McpTransport::Http | McpTransport::Sse if url.is_none() => {
            return Err("remote MCP server is missing url".to_string());
        }
        _ => {}
    }
    let args = object
        .get("args")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_string)
        .collect();
    let headers = string_map_field(object, "headers")
        .or_else(|| string_map_field(object, "http_headers"))
        .unwrap_or_default();
    Ok(McpServerManifest {
        transport,
        command,
        args,
        env: string_map_field(object, "env").unwrap_or_default(),
        cwd: string_field(object, "cwd"),
        url,
        headers,
        header_env: string_map_field(object, "env_http_headers").unwrap_or_default(),
        bearer_token_env_var: string_field(object, "bearer_token_env_var"),
    })
}

fn string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Option<String> {
    object
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn string_map_field(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Option<BTreeMap<String, String>> {
    object
        .get(field)
        .and_then(serde_json::Value::as_object)
        .map(|values| {
            values
                .iter()
                .filter_map(|(name, value)| {
                    value
                        .as_str()
                        .map(|value| (name.clone(), value.to_string()))
                })
                .collect()
        })
}

fn expand_user_path(path: &Path) -> Result<PathBuf, String> {
    if let Ok(relative) = path.strip_prefix("~") {
        return Ok(home_dir().join(relative));
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| error.to_string())
    }
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        } else {
            return Err(format!(
                "unsupported entry in dotfile package: {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
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
    fn manifest_omits_empty_sections() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("registry.toml");
        let mut manifest = RegistryManifest::default();
        manifest.set_package("npm", "typescript", true);

        write_manifest_to(&path, &manifest).unwrap();

        let source = std::fs::read_to_string(path).unwrap();
        assert!(source.contains("[packages]"));
        assert!(!source.contains("[mcp"));
        assert!(!source.contains("[dotfiles]"));
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

    #[test]
    fn brewfile_import_separates_formulae_and_casks() {
        let imported = parse_brewfile(
            r#"
tap "homebrew/cask-fonts"
brew "ripgrep"
brew 'openssl@3', link: false
cask "ghostty"
brew "ripgrep"
"#,
        );

        assert_eq!(imported.formulae, ["openssl@3", "ripgrep"]);
        assert_eq!(imported.casks, ["ghostty"]);
    }

    #[test]
    fn npm_import_combines_runtime_development_and_optional_dependencies() {
        let imported = parse_npm_manifest(
            r#"{
                "dependencies": {"typescript": "^5"},
                "devDependencies": {"eslint": "^9"},
                "optionalDependencies": {"prettier": "^3"},
                "peerDependencies": {"react": "^19"}
            }"#,
        )
        .unwrap();

        assert_eq!(imported, ["eslint", "prettier", "typescript"]);
    }

    #[test]
    fn mcp_import_normalizes_codex_and_vibe_formats() {
        let codex = parse_mcp_config(
            r#"
[mcp_servers.docs]
url = "https://example.com/mcp"
bearer_token_env_var = "DOCS_TOKEN"

[mcp_servers.local]
command = "npx"
args = ["-y", "server"]
[mcp_servers.local.env]
MODE = "local"
"#,
        )
        .unwrap();
        assert_eq!(codex["docs"].transport, McpTransport::Http);
        assert_eq!(
            codex["docs"].bearer_token_env_var.as_deref(),
            Some("DOCS_TOKEN")
        );
        assert_eq!(codex["local"].command.as_deref(), Some("npx"));
        assert_eq!(codex["local"].env["MODE"], "local");

        let vibe = parse_mcp_config(
            r#"
[[mcp_servers]]
name = "figma"
transport = "http"
url = "https://example.com/figma"

[[mcp_servers]]
name = "vmux"
transport = "stdio"
command = "vmux"
"#,
        )
        .unwrap();
        assert_eq!(vibe.keys().cloned().collect::<Vec<_>>(), ["figma"]);
    }

    #[test]
    fn mcp_import_normalizes_claude_json() {
        let imported = parse_mcp_config(
            r#"{
                "mcpServers": {
                    "notion": {"type": "http", "url": "https://example.com/notion"},
                    "local": {"command": "uvx", "args": ["server"]}
                }
            }"#,
        )
        .unwrap();

        assert_eq!(imported["notion"].transport, McpTransport::Http);
        assert_eq!(imported["local"].transport, McpTransport::Stdio);
    }

    #[test]
    fn config_without_mcp_section_is_ignored_during_discovery() {
        assert!(parse_mcp_config(r#"{"theme":"dark"}"#).unwrap().is_empty());
        assert!(
            parse_mcp_config("model = \"default\"\n")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn file_imports_merge_without_removing_existing_desired_state() {
        let temp = tempfile::tempdir().unwrap();
        let manifest_path = temp.path().join("registry.toml");
        let brewfile = temp.path().join("Brewfile");
        let package_json = temp.path().join("package.json");
        let mcp = temp.path().join("mcp.json");
        let mut manifest = RegistryManifest::default();
        manifest.set_package("npm", "existing", true);
        write_manifest_to(&manifest_path, &manifest).unwrap();
        std::fs::write(&brewfile, "brew \"ripgrep\"\ncask \"ghostty\"\n").unwrap();
        std::fs::write(&package_json, r#"{"devDependencies":{"eslint":"1"}}"#).unwrap();
        std::fs::write(
            &mcp,
            r#"{"mcpServers":{"docs":{"url":"https://example.com"}}}"#,
        )
        .unwrap();

        assert_eq!(
            import_brewfile_to(&brewfile, &manifest_path).unwrap(),
            (1, 1)
        );
        assert_eq!(
            import_npm_manifest_to(&package_json, &manifest_path).unwrap(),
            1
        );
        assert_eq!(import_mcp_config_to(&mcp, &manifest_path).unwrap(), 1);
        let loaded = load_manifest_from(&manifest_path).unwrap();
        assert_eq!(loaded.packages["npm"], ["eslint", "existing"]);
        assert!(loaded.mcp.servers.contains_key("docs"));
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

    #[test]
    fn dotfile_import_copies_stow_packages_and_enables_them() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("stow");
        let dotfiles = temp.path().join("registry/dotfiles");
        let manifest = temp.path().join("registry/registry.toml");
        std::fs::create_dir_all(source.join("git")).unwrap();
        std::fs::create_dir_all(source.join("shell/.config/nushell")).unwrap();
        std::fs::write(source.join("git/.gitconfig"), "git").unwrap();
        std::fs::write(source.join("shell/.config/nushell/config.nu"), "nu").unwrap();

        assert_eq!(
            import_dotfiles_to(&source, &dotfiles, &manifest).unwrap(),
            2
        );
        assert_eq!(
            std::fs::read_to_string(dotfiles.join("git/.gitconfig")).unwrap(),
            "git"
        );
        assert_eq!(
            load_manifest_from(&manifest).unwrap().dotfiles.packages,
            ["git", "shell"]
        );
        assert!(source.join("git/.gitconfig").is_file());
    }
}
