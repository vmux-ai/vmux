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
        .map_err(|error| format!("invalid workspace directory: {error}"))?;
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
    let relative_dir = base_dir
        .strip_prefix(&checkout.root)
        .map_err(|_| "project directory is outside its checkout".to_string())?;
    let base_ref = worktree::head_ref(&checkout.root).map_err(|error| error.0)?;
    let (checkout_dir, branch) = plan_worktree(&checkout, managed_root, slug_hint);
    let checkout_dir = prepare_managed_destination(managed_root, &checkout, &checkout_dir)?;
    let info = worktree::worktree_add(&checkout.root, &checkout_dir, &branch, &base_ref)
        .map_err(|error| error.0)?;
    let activation = (|| {
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
    })();
    if activation.is_err() {
        let _ = worktree::worktree_remove(&checkout.root, &info.path, &info.branch, false);
    }
    activation
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
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::process::Command;

    #[derive(Resource)]
    struct ObservationInput {
        tab: Entity,
        path: PathBuf,
    }

    #[derive(Resource, Default)]
    struct CapturedStartupDir(Option<String>);

    fn emit_observation(
        input: Res<ObservationInput>,
        mut observations: MessageWriter<TabDirectoryObserved>,
    ) {
        observations.write(TabDirectoryObserved {
            tab: input.tab,
            path: input.path.clone(),
            kind: TabDirectoryObservationKind::Read,
        });
    }

    fn capture_startup_dir(
        input: Res<ObservationInput>,
        tabs: Query<&Tab>,
        mut captured: ResMut<CapturedStartupDir>,
    ) {
        captured.0 = tabs.get(input.tab).unwrap().startup_dir.clone();
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q", "-b", "main"]);
        git(p, &["config", "user.email", "t@example.com"]);
        git(p, &["config", "user.name", "Test"]);
        git(p, &["config", "commit.gpgsign", "false"]);
        std::fs::write(p.join("seed.txt"), "seed\n").unwrap();
        git(p, &["add", "seed.txt"]);
        git(p, &["commit", "-qm", "init"]);
        dir
    }

    fn observe(app: &mut App, tab: Entity, path: &Path) {
        observe_with_kind(app, tab, path, TabDirectoryObservationKind::Read);
    }

    fn observe_edit(app: &mut App, tab: Entity, path: &Path) {
        observe_with_kind(app, tab, path, TabDirectoryObservationKind::Edit);
    }

    fn observe_with_kind(
        app: &mut App,
        tab: Entity,
        path: &Path,
        kind: TabDirectoryObservationKind,
    ) {
        app.world_mut()
            .resource_mut::<Messages<TabDirectoryObserved>>()
            .write(TabDirectoryObserved {
                tab,
                path: path.to_path_buf(),
                kind,
            });
        app.update();
    }

    #[test]
    fn sanitize_slug_normalizes() {
        assert_eq!(sanitize_slug("Auth Refactor!"), "auth-refactor");
        assert_eq!(sanitize_slug("  a//b  "), "a-b");
        assert_eq!(sanitize_slug("***"), "task");
        assert_eq!(sanitize_slug(""), "task");
    }

    #[test]
    fn create_worktree_blocking_uses_repository_hashed_global_root() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let activation =
            create_worktree_blocking(repo.path(), "Auth Refactor", managed_root.path()).unwrap();
        let checkout_dir = PathBuf::from(&activation.metadata.checkout_dir);
        let managed_root = managed_root.path().canonicalize().unwrap();
        assert_eq!(activation.metadata.branch, "vmux/auth-refactor");
        assert!(checkout_dir.is_dir());
        assert!(
            checkout_dir.starts_with(&managed_root)
                && checkout_dir.ends_with("auth-refactor")
                && checkout_dir
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.rsplit_once('-')
                            .is_some_and(|(_, hash)| hash.len() == 12)
                    }),
            "path is <managed-root>/<repo-hash>/auth-refactor: {checkout_dir:?}"
        );
        assert_eq!(activation.execution_dir, checkout_dir);
    }

    #[test]
    fn create_worktree_preserves_nested_project_directory() {
        let repo = init_repo();
        let nested = repo.path().join("crates/app");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("main.rs"), "fn main() {}\n").unwrap();
        git(repo.path(), &["add", "crates/app/main.rs"]);
        git(repo.path(), &["commit", "-qm", "nested project"]);
        let managed_root = tempfile::tempdir().unwrap();

        let activation = create_worktree_blocking(&nested, "nested", managed_root.path()).unwrap();

        assert!(activation.execution_dir.ends_with("nested/crates/app"));
        assert!(activation.execution_dir.join("main.rs").is_file());
    }

    #[test]
    fn plan_worktree_skips_existing_branch_name() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        git(repo.path(), &["branch", "vmux/feat"]);
        let checkout = worktree::checkout_info(repo.path()).unwrap();
        let (path, branch) = plan_worktree(&checkout, managed_root.path(), "feat");
        assert_eq!(branch, "vmux/feat-2");
        assert!(path.starts_with(managed_root.path()));
        assert!(path.ends_with("feat-2"), "{path:?}");
    }

    #[test]
    fn reconcile_recovers_missing_worktree_without_dropping_metadata() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let activation =
            create_worktree_blocking(repo.path(), "recover", managed_root.path()).unwrap();
        let checkout_dir = PathBuf::from(&activation.metadata.checkout_dir);
        std::fs::remove_dir_all(&checkout_dir).unwrap();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "recover".into(),
                    startup_dir: Some(activation.execution_dir.to_string_lossy().into_owned()),
                },
                TabWorkspace {
                    project_dir: repo.path().to_string_lossy().into_owned(),
                },
                activation.metadata,
            ))
            .id();

        app.update();

        assert!(checkout_dir.is_dir());
        assert!(app.world().get::<TabWorktree>(tab).is_some());
        assert!(app.world().get::<TabWorktreeUnavailable>(tab).is_none());
    }

    #[test]
    fn recovery_recreates_pruned_managed_registration() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let activation =
            create_worktree_blocking(repo.path(), "recover", managed_root.path()).unwrap();
        std::fs::remove_dir_all(&activation.metadata.checkout_dir).unwrap();
        git(repo.path(), &["worktree", "prune", "--expire", "now"]);
        let tab = Tab {
            name: "recover".into(),
            startup_dir: Some(activation.execution_dir.to_string_lossy().into_owned()),
        };
        let workspace = TabWorkspace {
            project_dir: repo.path().to_string_lossy().into_owned(),
        };

        let recovered = ensure_tab_worktree_available(
            &tab,
            &workspace,
            &activation.metadata,
            managed_root.path(),
        )
        .unwrap();

        assert!(recovered.execution_dir.is_dir());
        assert_eq!(
            worktree::head_ref(&recovered.execution_dir).unwrap(),
            activation.metadata.branch
        );
    }

    #[test]
    fn reconcile_keeps_metadata_when_recovery_fails() {
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "missing".into(),
                    startup_dir: Some("/no/such/vmux-worktree".into()),
                },
                TabWorkspace {
                    project_dir: "/no/such/vmux-project".into(),
                },
                TabWorktree {
                    repo_root: "/no/such/vmux-project".into(),
                    checkout_dir: "/no/such/vmux-worktree".into(),
                    branch: "vmux/missing".into(),
                    base_ref: "main".into(),
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<TabWorktree>(tab).is_some());
        assert!(app.world().get::<TabWorktreeUnavailable>(tab).is_some());
    }

    #[test]
    fn recovery_rejects_unregistered_path_outside_managed_root() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let activation =
            create_worktree_blocking(repo.path(), "managed", managed_root.path()).unwrap();
        let outside_parent = tempfile::tempdir().unwrap();
        let outside = outside_parent.path().join("escape");
        let mut metadata = activation.metadata;
        metadata.checkout_dir = outside.to_string_lossy().into_owned();
        let tab = Tab {
            name: "managed".into(),
            startup_dir: Some(outside.to_string_lossy().into_owned()),
        };
        let workspace = TabWorkspace {
            project_dir: repo.path().to_string_lossy().into_owned(),
        };

        let error = ensure_tab_worktree_available(&tab, &workspace, &metadata, managed_root.path())
            .unwrap_err();

        assert!(error.contains("repository storage directory"));
        assert!(!outside.exists());
    }

    #[cfg(unix)]
    #[test]
    fn managed_project_directory_cannot_escape_through_symlink() {
        use std::os::unix::fs::symlink;

        let repo = init_repo();
        let nested = repo.path().join("crates/app");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("main.rs"), "fn main() {}\n").unwrap();
        git(repo.path(), &["add", "crates/app/main.rs"]);
        git(repo.path(), &["commit", "-qm", "nested project"]);
        let managed_root = tempfile::tempdir().unwrap();
        let activation = create_worktree_blocking(&nested, "managed", managed_root.path()).unwrap();
        std::fs::remove_dir_all(&activation.execution_dir).unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), &activation.execution_dir).unwrap();
        let tab = Tab {
            name: "managed".into(),
            startup_dir: Some(activation.execution_dir.to_string_lossy().into_owned()),
        };
        let workspace = TabWorkspace {
            project_dir: nested.to_string_lossy().into_owned(),
        };

        let error = ensure_tab_worktree_available(
            &tab,
            &workspace,
            &activation.metadata,
            managed_root.path(),
        )
        .unwrap_err();

        assert!(error.contains("escapes worktree"));
    }

    #[test]
    fn restore_reconciles_at_most_one_worktree_per_frame() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let first = create_worktree_blocking(repo.path(), "first", managed_root.path()).unwrap();
        let second = create_worktree_blocking(repo.path(), "second", managed_root.path()).unwrap();
        std::fs::remove_dir_all(&first.metadata.checkout_dir).unwrap();
        std::fs::remove_dir_all(&second.metadata.checkout_dir).unwrap();
        let mut app = App::new();
        app.insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()))
            .add_plugins(WorktreePlugin);
        for activation in [first, second] {
            app.world_mut().spawn((
                Tab {
                    name: "restore".into(),
                    startup_dir: Some(activation.execution_dir.to_string_lossy().into_owned()),
                },
                TabWorkspace {
                    project_dir: repo.path().to_string_lossy().into_owned(),
                },
                activation.metadata,
            ));
        }

        app.update();
        assert_eq!(
            app.world()
                .iter_entities()
                .filter(|entity| entity.contains::<TabWorktreeReady>())
                .count(),
            1
        );

        app.update();
        assert_eq!(
            app.world()
                .iter_entities()
                .filter(|entity| entity.contains::<TabWorktreeReady>())
                .count(),
            2
        );
    }

    #[test]
    fn observation_rebinds_managed_tab_to_same_repo_checkout() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let managed =
            create_worktree_blocking(repo.path(), "managed", managed_root.path()).unwrap();
        let touched = repo.path().join("seed.txt");
        let expected = repo
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "tab".into(),
                    startup_dir: Some(managed.execution_dir.to_string_lossy().into_owned()),
                },
                TabWorkspace {
                    project_dir: repo.path().to_string_lossy().into_owned(),
                },
                managed.metadata.clone(),
            ))
            .id();

        observe_edit(&mut app, tab, &touched);

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(expected.as_str())
        );
        assert!(app.world().get::<TabWorktree>(tab).is_none());
        assert!(
            Path::new(&managed.metadata.checkout_dir).is_dir(),
            "old checkout is preserved"
        );
    }

    #[test]
    fn observation_rebinds_before_same_frame_consumers() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let managed =
            create_worktree_blocking(repo.path(), "managed", managed_root.path()).unwrap();
        let expected = repo
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin)
            .init_resource::<CapturedStartupDir>();
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(managed.execution_dir.to_string_lossy().into_owned()),
            })
            .id();
        app.insert_resource(ObservationInput {
            tab,
            path: repo.path().join("seed.txt"),
        })
        .add_systems(Update, emit_observation.before(TabDirectoryRebindSet))
        .add_systems(Update, capture_startup_dir.after(TabDirectoryRebindSet));

        app.update();

        assert_eq!(
            app.world().resource::<CapturedStartupDir>().0.as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn observation_rebinds_repeatedly_within_same_repo() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let first = create_worktree_blocking(repo.path(), "first", managed_root.path()).unwrap();
        let second_path = repo.path().join(".worktrees/second");
        worktree::worktree_add(repo.path(), &second_path, "vmux/second", "main").unwrap();
        let second_file = second_path.join("seed.txt");
        let main_file = repo.path().join("seed.txt");
        let second_expected = second_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let main_expected = repo
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(first.execution_dir.to_string_lossy().into_owned()),
            })
            .id();

        observe(&mut app, tab, &second_file);
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(second_expected.as_str())
        );

        observe(&mut app, tab, &main_file);
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(main_expected.as_str())
        );
    }

    #[test]
    fn observation_keeps_same_checkout_directory() {
        let repo = init_repo();
        let original = repo
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(original.clone()),
            })
            .id();

        observe(&mut app, tab, &repo.path().join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(original.as_str())
        );
    }

    #[test]
    fn observation_rebinds_from_main_checkout_to_nested_linked_worktree() {
        let repo = init_repo();
        let linked_path = repo.path().join(".worktrees/linked");
        worktree::worktree_add(repo.path(), &linked_path, "vmux/linked", "main").unwrap();
        let expected = linked_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(repo.path().to_string_lossy().into_owned()),
            })
            .id();

        observe(&mut app, tab, &linked_path.join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn observation_ignores_unrelated_repo_nested_inside_checkout() {
        let repo = init_repo();
        let nested = repo.path().join("vendor/nested");
        std::fs::create_dir_all(&nested).unwrap();
        git(&nested, &["init", "-q", "-b", "main"]);
        git(&nested, &["config", "user.email", "t@example.com"]);
        git(&nested, &["config", "user.name", "Test"]);
        git(&nested, &["config", "commit.gpgsign", "false"]);
        std::fs::write(nested.join("nested.txt"), "nested\n").unwrap();
        git(&nested, &["add", "nested.txt"]);
        git(&nested, &["commit", "-qm", "init"]);
        let original = repo.path().to_string_lossy().into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(original.clone()),
            })
            .id();

        observe(&mut app, tab, &nested.join("nested.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(original.as_str())
        );
    }

    #[test]
    fn cached_checkout_info_resolves_again_after_startup_or_git_identity_changes() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir(repo.path().join(".git")).unwrap();
        let next_root = repo.path().join(".worktrees/next");
        std::fs::create_dir_all(next_root.join(".git")).unwrap();
        let startup_dir = repo.path().to_string_lossy().into_owned();
        let next_startup_dir = next_root.to_string_lossy().into_owned();
        let tab = Entity::from_bits(1);
        let calls = Cell::new(0);
        let first = vmux_git::worktree::CheckoutInfo {
            root: repo.path().canonicalize().unwrap(),
            common_dir: repo.path().join(".git").canonicalize().unwrap(),
        };
        let second = vmux_git::worktree::CheckoutInfo {
            root: next_root.canonicalize().unwrap(),
            common_dir: repo.path().join(".git").canonicalize().unwrap(),
        };
        let mut cache = HashMap::new();

        let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
            calls.set(calls.get() + 1);
            Some(first.clone())
        })
        .unwrap();
        assert_eq!(resolved, first);
        let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
            calls.set(calls.get() + 1);
            Some(second.clone())
        })
        .unwrap();
        assert_eq!(resolved, first);
        std::fs::rename(repo.path().join(".git"), repo.path().join(".git-old")).unwrap();
        std::fs::create_dir(repo.path().join(".git")).unwrap();
        let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
            calls.set(calls.get() + 1);
            Some(first.clone())
        })
        .unwrap();
        assert_eq!(resolved, first);
        let resolved = cached_checkout_info(&mut cache, tab, &next_startup_dir, |_| {
            calls.set(calls.get() + 1);
            Some(second.clone())
        })
        .unwrap();

        assert_eq!(resolved, second);
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn cached_checkout_info_resolves_again_after_commondir_changes() {
        let root = tempfile::tempdir().unwrap();
        let admin = tempfile::tempdir().unwrap();
        let first_common = tempfile::tempdir().unwrap();
        let second_common = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join(".git"),
            format!("gitdir: {}\n", admin.path().display()),
        )
        .unwrap();
        std::fs::write(
            admin.path().join("commondir"),
            first_common.path().to_string_lossy().as_bytes(),
        )
        .unwrap();
        let startup_dir = root.path().to_string_lossy().into_owned();
        let tab = Entity::from_bits(1);
        let calls = Cell::new(0);
        let first = vmux_git::worktree::CheckoutInfo {
            root: root.path().canonicalize().unwrap(),
            common_dir: first_common.path().canonicalize().unwrap(),
        };
        let second = vmux_git::worktree::CheckoutInfo {
            root: root.path().canonicalize().unwrap(),
            common_dir: second_common.path().canonicalize().unwrap(),
        };
        let mut cache = HashMap::new();

        let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
            calls.set(calls.get() + 1);
            Some(first.clone())
        })
        .unwrap();
        assert_eq!(resolved, first);
        let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
            calls.set(calls.get() + 1);
            Some(second.clone())
        })
        .unwrap();
        assert_eq!(resolved, first);
        assert_eq!(calls.get(), 1);
        std::fs::write(
            admin.path().join("commondir"),
            second_common.path().to_string_lossy().as_bytes(),
        )
        .unwrap();
        let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
            calls.set(calls.get() + 1);
            Some(second.clone())
        })
        .unwrap();

        assert_eq!(resolved, second);
        assert_eq!(calls.get(), 2);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn observation_ignores_non_utf8_checkout_root() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let current = init_repo();
        let observed_parent = tempfile::tempdir().unwrap();
        let observed = observed_parent
            .path()
            .join(OsString::from_vec(b"repo-\xff".to_vec()));
        std::fs::create_dir(&observed).unwrap();
        git(&observed, &["init", "-q", "-b", "main"]);
        git(&observed, &["config", "user.email", "t@example.com"]);
        git(&observed, &["config", "user.name", "Test"]);
        git(&observed, &["config", "commit.gpgsign", "false"]);
        std::fs::write(observed.join("seed.txt"), "seed\n").unwrap();
        git(&observed, &["add", "seed.txt"]);
        git(&observed, &["commit", "-qm", "init"]);
        let original = current.path().to_string_lossy().into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(original.clone()),
            })
            .id();

        observe_edit(&mut app, tab, &observed.join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(original.as_str())
        );
    }

    #[test]
    fn observation_ignores_unrelated_and_invalid_paths() {
        let repo = init_repo();
        let other = init_repo();
        let non_git = tempfile::tempdir().unwrap();
        let non_git_file = non_git.path().join("file.txt");
        std::fs::write(&non_git_file, "x").unwrap();
        let missing = repo.path().join("missing.txt");
        let original = repo.path().to_string_lossy().into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(original.clone()),
            })
            .id();

        observe(&mut app, tab, &other.path().join("seed.txt"));
        observe(&mut app, tab, &non_git_file);
        observe(&mut app, tab, &missing);

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(original.as_str())
        );
    }

    #[test]
    fn observation_rebinds_to_different_repo_on_edit() {
        let current = init_repo();
        let observed = init_repo();
        let expected = observed
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(current.path().to_string_lossy().into_owned()),
            })
            .id();

        observe_edit(&mut app, tab, &observed.path().join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn observation_rebinds_from_non_git_directory_on_edit() {
        let current = tempfile::tempdir().unwrap();
        let observed = init_repo();
        let expected = observed
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(current.path().to_string_lossy().into_owned()),
            })
            .id();

        observe_edit(&mut app, tab, &observed.path().join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn observation_keeps_non_git_directory_on_read() {
        let current = tempfile::tempdir().unwrap();
        let observed = init_repo();
        let original = current
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(original.clone()),
            })
            .id();

        observe(&mut app, tab, &observed.path().join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(original.as_str())
        );
    }

    #[test]
    fn relative_observation_is_ignored() {
        assert_eq!(observed_start_dir(Path::new(".")), None);
    }

    #[test]
    fn observation_keeps_missing_current_directory_on_edit() {
        let current = tempfile::tempdir().unwrap();
        let original = current.path().to_string_lossy().into_owned();
        drop(current);
        let observed = init_repo();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(original.clone()),
            })
            .id();

        observe_edit(&mut app, tab, &observed.path().join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(original.as_str())
        );
    }
}
