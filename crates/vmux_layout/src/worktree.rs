//! Per-tab managed worktree lifecycle and directory rebinding.

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[cfg(unix)]
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};

use bevy::prelude::*;
use sha2::{Digest, Sha256};

use crate::tab::{Tab, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_git::worktree::{self, CheckoutInfo};

impl Plugin for WorktreePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ManagedWorktreeRoot>()
            .init_resource::<WorktreeReconcileQueue>()
            .add_message::<TabDirectoryObserved>()
            .add_systems(
                Update,
                (
                    ensure_tab_workspaces,
                    queue_added_tab_worktrees,
                    reconcile_next_tab_worktree,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                rebind_tab_directories
                    .in_set(TabDirectoryRebindSet)
                    .after(reconcile_next_tab_worktree),
            );
    }
}

pub struct WorktreePlugin;

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ManagedWorktreeRoot(pub PathBuf);

impl Default for ManagedWorktreeRoot {
    fn default() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"));
        Self(home.join(".vmux/worktrees"))
    }
}

#[derive(Clone, Debug)]
pub struct TabWorktreeActivation {
    pub execution_dir: PathBuf,
    pub metadata: TabWorktree,
    pub ready: TabWorktreeReady,
}

#[derive(Component, Clone, Debug)]
pub struct TabWorktreeReady {
    startup_dir: String,
    project_dir: String,
    metadata: TabWorktree,
    checkout: CheckoutInfo,
    checkout_fingerprint: CheckoutFingerprint,
    execution_fingerprint: PathFingerprint,
}

#[derive(Resource, Default)]
struct WorktreeReconcileQueue(VecDeque<Entity>);

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TabDirectoryRebindSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabDirectoryObservationKind {
    Read,
    Edit,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct TabDirectoryObserved {
    pub tab: Entity,
    pub path: PathBuf,
    pub kind: TabDirectoryObservationKind,
}

/// Sanitize a tab name into a filesystem/branch-safe slug (lowercase alnum, `-` separators).
pub fn sanitize_slug(name: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "task".to_string()
    } else {
        slug
    }
}

/// Whether a tab name is an automatically assigned placeholder.
pub fn is_generated_tab_name(name: &str) -> bool {
    name.is_empty()
        || name.strip_prefix("Tab ").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
        })
}

/// Prefer the selected project name over a generated tab label when naming a worktree.
pub fn tab_worktree_slug_hint(tab_name: &str, project_dir: &Path) -> String {
    if is_generated_tab_name(tab_name) {
        project_dir
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("task")
            .to_string()
    } else {
        tab_name.to_string()
    }
}

fn repository_storage_dir(managed_root: &Path, checkout: &CheckoutInfo) -> PathBuf {
    let repository_name = checkout
        .common_dir
        .parent()
        .and_then(Path::file_name)
        .or_else(|| checkout.root.file_name())
        .and_then(|name| name.to_str())
        .map(sanitize_slug)
        .unwrap_or_else(|| "repository".to_string());
    #[cfg(unix)]
    let digest = Sha256::digest(checkout.common_dir.as_os_str().as_bytes());
    #[cfg(not(unix))]
    let digest = Sha256::digest(checkout.common_dir.to_string_lossy().as_bytes());
    let hash = format!("{digest:x}");
    managed_root.join(format!("{repository_name}-{}", &hash[..12]))
}

fn normalize_missing_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "worktree path has no parent".to_string())?
        .canonicalize()
        .map_err(|error| format!("invalid worktree parent: {error}"))?;
    let name = path
        .file_name()
        .ok_or_else(|| "worktree path has no file name".to_string())?;
    Ok(parent.join(name))
}

fn prepare_managed_destination(
    managed_root: &Path,
    checkout: &CheckoutInfo,
    destination: &Path,
) -> Result<PathBuf, String> {
    if !destination.is_absolute() {
        return Err("managed worktree path must be absolute".to_string());
    }
    let repository_dir = repository_storage_dir(managed_root, checkout);
    std::fs::create_dir_all(&repository_dir)
        .map_err(|error| format!("failed to create worktree directory: {error}"))?;
    let repository_dir = repository_dir
        .canonicalize()
        .map_err(|error| format!("invalid repository storage directory: {error}"))?;
    let destination = normalize_missing_path(destination)?;
    if destination.parent() != Some(repository_dir.as_path()) {
        return Err("managed worktree path escapes its repository storage directory".to_string());
    }
    Ok(destination)
}

fn prepare_recovery_destination(
    managed_root: &Path,
    checkout: &CheckoutInfo,
    destination: &Path,
    branch: &str,
) -> Result<PathBuf, String> {
    let registrations =
        worktree::worktree_registrations(&checkout.root).map_err(|error| error.0)?;
    if let Ok(destination) = normalize_missing_path(destination)
        && registrations.iter().any(|registration| {
            registration.path == destination && registration.branch.as_deref() == Some(branch)
        })
    {
        return Ok(destination);
    }
    prepare_managed_destination(managed_root, checkout, destination)
}

fn canonical_execution_dir(checkout_root: &Path, relative_dir: &Path) -> Result<PathBuf, String> {
    let execution_dir = checkout_root.join(relative_dir);
    let execution_dir = execution_dir
        .canonicalize()
        .map_err(|error| format!("project directory is missing from worktree: {error}"))?;
    if !execution_dir.is_dir() || !execution_dir.starts_with(checkout_root) {
        return Err(format!(
            "project directory escapes worktree: {}",
            execution_dir.display()
        ));
    }
    Ok(execution_dir)
}

pub struct ValidatedLinkedWorkspace {
    pub cwd: PathBuf,
    pub workspace_cwd: PathBuf,
    pub checkout: CheckoutInfo,
}

pub fn validate_linked_workspace(
    cwd: &Path,
    workspace_cwd: &Path,
    branch: &str,
) -> Result<ValidatedLinkedWorkspace, String> {
    let cwd = cwd
        .canonicalize()
        .map_err(|error| format!("invalid worktree directory: {error}"))?;
    let workspace_cwd = workspace_cwd
        .canonicalize()
        .map_err(|error| format!("invalid project directory: {error}"))?;
    let checkout = worktree::checkout_info(&cwd).map_err(|error| error.0)?;
    let workspace = worktree::checkout_info(&workspace_cwd).map_err(|error| error.0)?;
    if checkout.common_dir != workspace.common_dir {
        return Err("worktree belongs to a different repository".to_string());
    }
    if !worktree::is_linked_worktree(&cwd) {
        return Err("worktree directory is not a linked worktree".to_string());
    }
    let actual_branch = worktree::head_ref(&checkout.root).map_err(|error| error.0)?;
    if actual_branch != branch {
        return Err(format!(
            "worktree is on branch {actual_branch}, expected {branch}"
        ));
    }
    Ok(ValidatedLinkedWorkspace {
        cwd,
        workspace_cwd,
        checkout,
    })
}

fn plan_worktree(
    checkout: &CheckoutInfo,
    managed_root: &Path,
    slug_hint: &str,
) -> (PathBuf, String) {
    let base = sanitize_slug(slug_hint);
    let repository_dir = repository_storage_dir(managed_root, checkout);
    let existing = worktree::worktree_list(&checkout.root).unwrap_or_default();
    let branches = worktree::local_branches(&checkout.root).unwrap_or_default();
    let taken = |slug: &str| -> bool {
        let path = repository_dir.join(slug);
        let branch = format!("vmux/{slug}");
        existing.iter().any(|p| p == &path)
            || path.exists()
            || branches.iter().any(|b| b == &branch)
    };
    let mut slug = base.clone();
    let mut n = 2;
    while taken(&slug) {
        slug = format!("{base}-{n}");
        n += 1;
    }
    let path = repository_dir.join(&slug);
    let branch = format!("vmux/{slug}");
    (path, branch)
}

fn activate_added_worktree(
    base_dir: &Path,
    checkout: &CheckoutInfo,
    relative_dir: &Path,
    info: &worktree::WorktreeInfo,
) -> Result<TabWorktreeActivation, String> {
    let managed_checkout = worktree::checkout_info(&info.path).map_err(|error| error.0)?;
    if managed_checkout.common_dir != checkout.common_dir {
        return Err("managed worktree belongs to a different repository".to_string());
    }
    let execution_dir = canonical_execution_dir(&managed_checkout.root, relative_dir)?;
    let metadata = TabWorktree {
        repo_root: checkout.root.to_string_lossy().into_owned(),
        checkout_dir: managed_checkout.root.to_string_lossy().into_owned(),
        branch: info.branch.clone(),
        base_ref: info.base_ref.clone(),
    };
    let ready = TabWorktreeReady::new(
        &execution_dir,
        &base_dir.to_string_lossy(),
        &metadata,
        &managed_checkout,
    )?;
    Ok(TabWorktreeActivation {
        execution_dir,
        metadata,
        ready,
    })
}

fn add_managed_worktree(
    base_dir: &Path,
    checkout: &CheckoutInfo,
    relative_dir: &Path,
    checkout_dir: &Path,
    branch: &str,
    base_ref: &str,
) -> Result<TabWorktreeActivation, String> {
    let info = worktree::worktree_add(&checkout.root, checkout_dir, branch, base_ref)
        .map_err(|error| error.0)?;
    let activation = activate_added_worktree(base_dir, checkout, relative_dir, &info);
    if activation.is_err() {
        let _ = worktree::worktree_remove(&checkout.root, &info.path, &info.branch, false);
    }
    activation
}

/// Create a globally stored managed worktree while preserving `base_dir`'s repository-relative
/// directory.
pub fn create_worktree_blocking(
    base_dir: &Path,
    slug_hint: &str,
    managed_root: &Path,
) -> Result<TabWorktreeActivation, String> {
    let base_dir = base_dir
        .canonicalize()
        .map_err(|error| format!("invalid project directory: {error}"))?;
    let checkout = worktree::checkout_info(&base_dir).map_err(|error| error.0)?;
    worktree::ensure_initial_commit(&checkout.root).map_err(|error| error.0)?;
    let relative_dir = base_dir
        .strip_prefix(&checkout.root)
        .map_err(|_| "project directory is outside its checkout".to_string())?;
    let base_ref = worktree::head_ref(&checkout.root).map_err(|error| error.0)?;
    let (checkout_dir, branch) = plan_worktree(&checkout, managed_root, slug_hint);
    let checkout_dir = prepare_managed_destination(managed_root, &checkout, &checkout_dir)?;
    add_managed_worktree(
        &base_dir,
        &checkout,
        relative_dir,
        &checkout_dir,
        &branch,
        &base_ref,
    )
}

/// Create a globally stored managed worktree on an exact user-selected branch name.
pub fn create_worktree_for_branch_blocking(
    base_dir: &Path,
    branch: &str,
    managed_root: &Path,
) -> Result<TabWorktreeActivation, String> {
    let base_dir = base_dir
        .canonicalize()
        .map_err(|error| format!("invalid project directory: {error}"))?;
    let checkout = worktree::checkout_info(&base_dir).map_err(|error| error.0)?;
    worktree::validate_branch_name(&checkout.root, branch).map_err(|error| error.0)?;
    worktree::ensure_initial_commit(&checkout.root).map_err(|error| error.0)?;
    if let Some(registration) = worktree::worktree_registrations(&checkout.root)
        .map_err(|error| error.0)?
        .into_iter()
        .find(|registration| registration.branch.as_deref() == Some(branch))
    {
        return Err(format!(
            "Branch {branch} is already checked out at {}",
            registration.path.display()
        ));
    }
    if worktree::local_branches(&checkout.root)
        .map_err(|error| error.0)?
        .iter()
        .any(|existing| existing == branch)
    {
        return Err(format!("Branch {branch} already exists"));
    }
    let relative_dir = base_dir
        .strip_prefix(&checkout.root)
        .map_err(|_| "project directory is outside its checkout".to_string())?;
    let base_ref = worktree::head_ref(&checkout.root).map_err(|error| error.0)?;
    let slug = sanitize_slug(branch.strip_prefix("vmux/").unwrap_or(branch));
    let checkout_dir = repository_storage_dir(managed_root, &checkout).join(slug);
    let checkout_dir = prepare_managed_destination(managed_root, &checkout, &checkout_dir)?;
    add_managed_worktree(
        &base_dir,
        &checkout,
        relative_dir,
        &checkout_dir,
        branch,
        &base_ref,
    )
}

pub fn ensure_tab_worktree_available(
    tab: &Tab,
    workspace: &TabWorkspace,
    metadata: &TabWorktree,
    managed_root: &Path,
) -> Result<TabWorktreeActivation, String> {
    let project_dir = Path::new(&workspace.project_dir)
        .canonicalize()
        .map_err(|error| format!("project directory unavailable: {error}"))?;
    let source = worktree::checkout_info(&project_dir).map_err(|error| error.0)?;
    let relative_dir = project_dir
        .strip_prefix(&source.root)
        .map_err(|_| "project directory is outside its checkout".to_string())?;
    let mut checkout_dir = if metadata.checkout_dir.is_empty() {
        let startup_dir = tab
            .startup_dir
            .as_deref()
            .ok_or_else(|| "managed worktree checkout path is missing".to_string())?;
        worktree::checkout_info(Path::new(startup_dir))
            .map(|checkout| checkout.root)
            .unwrap_or_else(|_| PathBuf::from(startup_dir))
    } else {
        PathBuf::from(&metadata.checkout_dir)
    };
    if !checkout_dir.is_dir() {
        if checkout_dir.symlink_metadata().is_ok() {
            return Err(format!(
                "managed worktree path is not a directory: {}",
                checkout_dir.display()
            ));
        }
        checkout_dir =
            prepare_recovery_destination(managed_root, &source, &checkout_dir, &metadata.branch)?;
        worktree::worktree_add_existing(
            &source.root,
            &checkout_dir,
            &metadata.branch,
            &metadata.base_ref,
        )
        .map_err(|error| format!("failed to recover managed worktree: {}", error.0))?;
    }
    let checkout = worktree::checkout_info(&checkout_dir).map_err(|error| error.0)?;
    if checkout.common_dir != source.common_dir {
        return Err("managed worktree belongs to a different repository".to_string());
    }
    if !worktree::is_linked_worktree(&checkout.root) {
        return Err("managed worktree directory is not a linked worktree".to_string());
    }
    let branch = worktree::head_ref(&checkout.root).map_err(|error| error.0)?;
    if branch != metadata.branch {
        return Err(format!(
            "managed worktree is on branch {branch}, expected {}",
            metadata.branch
        ));
    }
    let execution_dir = canonical_execution_dir(&checkout.root, relative_dir)?;
    let mut normalized = metadata.clone();
    normalized.repo_root = source.root.to_string_lossy().into_owned();
    normalized.checkout_dir = checkout.root.to_string_lossy().into_owned();
    let ready = TabWorktreeReady::new(
        &execution_dir,
        &workspace.project_dir,
        &normalized,
        &checkout,
    )?;
    Ok(TabWorktreeActivation {
        execution_dir,
        metadata: normalized,
        ready,
    })
}

fn ensure_tab_workspaces(
    tabs: Query<(Entity, &Tab, Option<&TabWorktree>), Without<TabWorkspace>>,
    mut commands: Commands,
) {
    for (entity, tab, worktree) in &tabs {
        let Some(project_dir) = worktree
            .map(|worktree| worktree.repo_root.as_str())
            .filter(|path| !path.is_empty())
            .or(tab.startup_dir.as_deref())
        else {
            continue;
        };
        let project_dir = Path::new(project_dir)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(project_dir));
        commands.entity(entity).insert(TabWorkspace {
            project_dir: project_dir.to_string_lossy().into_owned(),
        });
    }
}

fn queue_added_tab_worktrees(
    worktrees: Query<(Entity, Option<&TabWorktreeReady>), Added<TabWorktree>>,
    mut queue: ResMut<WorktreeReconcileQueue>,
) {
    for (entity, ready) in &worktrees {
        if ready.is_none() && !queue.0.contains(&entity) {
            queue.0.push_back(entity);
        }
    }
}

fn reconcile_next_tab_worktree(
    mut queue: ResMut<WorktreeReconcileQueue>,
    mut q: Query<(&mut Tab, &TabWorkspace, &TabWorktree), Without<TabWorktreeReady>>,
    managed_root: Res<ManagedWorktreeRoot>,
    mut commands: Commands,
) {
    while let Some(entity) = queue.0.pop_front() {
        let Ok((mut tab, workspace, metadata)) = q.get_mut(entity) else {
            continue;
        };
        match ensure_tab_worktree_available(&tab, workspace, metadata, &managed_root.0) {
            Ok(activation) => {
                let startup_dir = activation.execution_dir.to_string_lossy().into_owned();
                if tab.startup_dir.as_deref() != Some(&startup_dir) {
                    tab.startup_dir = Some(startup_dir);
                }
                let mut entity_commands = commands.entity(entity);
                if metadata != &activation.metadata {
                    entity_commands.insert(activation.metadata);
                }
                entity_commands
                    .insert(activation.ready)
                    .remove::<TabWorktreeUnavailable>();
            }
            Err(message) => {
                commands
                    .entity(entity)
                    .insert(TabWorktreeUnavailable { message });
            }
        }
        break;
    }
}

#[derive(Clone)]
struct CachedCheckoutInfo {
    startup_dir: String,
    info: CheckoutInfo,
    fingerprint: CheckoutFingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PathFingerprint {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CheckoutFingerprint {
    dot_git: PathFingerprint,
    admin_dir: PathBuf,
    common_dir: PathBuf,
    commondir: Option<Vec<u8>>,
    gitdir: Option<Vec<u8>>,
    head: Option<Vec<u8>>,
}

fn path_fingerprint(path: &Path) -> Option<PathFingerprint> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    Some(PathFingerprint {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
    })
}

fn git_admin_dir(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return dot_git.canonicalize().ok();
    }
    let contents = std::fs::read_to_string(&dot_git).ok()?;
    let path = PathBuf::from(contents.strip_prefix("gitdir:")?.trim());
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    path.canonicalize().ok()
}

fn checkout_fingerprint(info: &CheckoutInfo) -> Option<CheckoutFingerprint> {
    let dot_git_path = info.root.join(".git");
    let dot_git = path_fingerprint(&dot_git_path)?;
    let admin_dir = git_admin_dir(&info.root)?;
    let commondir = std::fs::read(admin_dir.join("commondir")).ok();
    let gitdir = std::fs::read(admin_dir.join("gitdir")).ok();
    let head = std::fs::read(admin_dir.join("HEAD")).ok();
    let common_dir = match commondir.as_deref() {
        Some(bytes) => {
            let value = std::str::from_utf8(bytes).ok()?.trim();
            let path = PathBuf::from(value);
            let path = if path.is_absolute() {
                path
            } else {
                admin_dir.join(path)
            };
            path.canonicalize().ok()?
        }
        None => admin_dir.clone(),
    };
    if common_dir != info.common_dir {
        return None;
    }
    Some(CheckoutFingerprint {
        dot_git,
        admin_dir,
        common_dir,
        commondir,
        gitdir,
        head,
    })
}

impl TabWorktreeReady {
    pub fn new(
        execution_dir: &Path,
        project_dir: &str,
        metadata: &TabWorktree,
        checkout: &CheckoutInfo,
    ) -> Result<Self, String> {
        let checkout_fingerprint = checkout_fingerprint(checkout)
            .ok_or_else(|| "failed to fingerprint managed worktree".to_string())?;
        let execution_fingerprint = path_fingerprint(execution_dir)
            .ok_or_else(|| "failed to fingerprint project directory".to_string())?;
        Ok(Self {
            startup_dir: execution_dir.to_string_lossy().into_owned(),
            project_dir: project_dir.to_string(),
            metadata: metadata.clone(),
            checkout: checkout.clone(),
            checkout_fingerprint,
            execution_fingerprint,
        })
    }

    pub fn is_current(&self, tab: &Tab, workspace: &TabWorkspace, metadata: &TabWorktree) -> bool {
        tab.startup_dir.as_deref() == Some(self.startup_dir.as_str())
            && workspace.project_dir == self.project_dir
            && metadata == &self.metadata
            && checkout_fingerprint(&self.checkout).as_ref() == Some(&self.checkout_fingerprint)
            && path_fingerprint(Path::new(&self.startup_dir)).as_ref()
                == Some(&self.execution_fingerprint)
    }
}

fn store_cached_checkout_info(
    cache: &mut HashMap<Entity, CachedCheckoutInfo>,
    tab: Entity,
    startup_dir: String,
    info: &CheckoutInfo,
) {
    let Some(fingerprint) = checkout_fingerprint(info) else {
        cache.remove(&tab);
        return;
    };
    cache.insert(
        tab,
        CachedCheckoutInfo {
            startup_dir,
            info: info.clone(),
            fingerprint,
        },
    );
}

fn cached_checkout_info(
    cache: &mut HashMap<Entity, CachedCheckoutInfo>,
    tab: Entity,
    startup_dir: &str,
    resolve: impl FnOnce(&Path) -> Option<CheckoutInfo>,
) -> Option<CheckoutInfo> {
    if let Some(cached) = cache.get(&tab)
        && cached.startup_dir == startup_dir
        && checkout_fingerprint(&cached.info).as_ref() == Some(&cached.fingerprint)
    {
        return Some(cached.info.clone());
    }
    cache.remove(&tab);
    let info = resolve(Path::new(startup_dir))?;
    store_cached_checkout_info(cache, tab, startup_dir.to_string(), &info);
    Some(info)
}

fn observed_start_dir(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() || !path.exists() {
        return None;
    }
    let start = if path.is_dir() { path } else { path.parent()? };
    start.canonicalize().ok()
}

fn is_within_checkout_without_nested_git_boundary(root: &Path, observed_dir: &Path) -> bool {
    observed_dir.starts_with(root)
        && !observed_dir
            .ancestors()
            .take_while(|ancestor| *ancestor != root)
            .any(|ancestor| ancestor.join(".git").exists())
}

fn rebind_tab_directories(
    mut reader: MessageReader<TabDirectoryObserved>,
    mut tabs: Query<&mut Tab>,
    mut workspaces: Query<&mut TabWorkspace>,
    managed: Query<(), With<TabWorktree>>,
    mut removed_tabs: RemovedComponents<Tab>,
    mut checkout_cache: Local<HashMap<Entity, CachedCheckoutInfo>>,
    mut commands: Commands,
) {
    for tab in removed_tabs.read() {
        checkout_cache.remove(&tab);
    }
    for observed in reader.read() {
        let Some(observed_dir) = observed_start_dir(&observed.path) else {
            continue;
        };
        let Ok(mut tab) = tabs.get_mut(observed.tab) else {
            continue;
        };
        let Some(current) = tab.startup_dir.clone() else {
            continue;
        };
        let Ok(current_dir) = Path::new(&current).canonicalize() else {
            continue;
        };
        if is_within_checkout_without_nested_git_boundary(&current_dir, &observed_dir) {
            continue;
        }
        let Ok(observed_info) = worktree::checkout_info(&observed_dir) else {
            continue;
        };
        let current_info =
            cached_checkout_info(&mut checkout_cache, observed.tab, &current, |path| {
                worktree::checkout_info(path).ok()
            });
        if current_info.is_none()
            && current_dir
                .ancestors()
                .any(|ancestor| ancestor.join(".git").exists())
        {
            continue;
        }
        if current_info.as_ref().is_some_and(|current_info| {
            is_within_checkout_without_nested_git_boundary(&current_info.root, &observed_dir)
        }) {
            continue;
        }
        let should_rebind = match current_info.as_ref() {
            Some(current_info) if current_info.root == observed_info.root => false,
            Some(current_info) if current_info.common_dir == observed_info.common_dir => true,
            Some(_) | None => observed.kind == TabDirectoryObservationKind::Edit,
        };
        if !should_rebind {
            continue;
        }
        let same_repository = current_info
            .as_ref()
            .is_some_and(|current| current.common_dir == observed_info.common_dir);
        let Some(startup_dir) = observed_info.root.to_str().map(str::to_owned) else {
            continue;
        };
        if !same_repository {
            if let Ok(mut workspace) = workspaces.get_mut(observed.tab) {
                workspace.project_dir.clone_from(&startup_dir);
            } else {
                commands.entity(observed.tab).insert(TabWorkspace {
                    project_dir: startup_dir.clone(),
                });
            }
        }
        tab.startup_dir = Some(startup_dir.clone());
        store_cached_checkout_info(
            &mut checkout_cache,
            observed.tab,
            startup_dir,
            &observed_info,
        );
        if managed.contains(observed.tab) {
            commands
                .entity(observed.tab)
                .remove::<TabWorktree>()
                .remove::<TabWorktreeReady>()
                .remove::<TabWorktreeUnavailable>();
        }
    }
}

#[cfg(test)]
#[path = "worktree.test.rs"]
mod tests;
