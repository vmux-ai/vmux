use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[cfg(unix)]
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use sha2::{Digest, Sha256};

use crate::tab::{Tab, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_ecs::{PageOpenDeferred, PageOpenError};
use vmux_git::worktree::{self, CheckoutInfo};

impl Plugin for WorktreePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ManagedWorktreeRoot>()
            .add_message::<TabDirectoryObserved>()
            .add_systems(
                Update,
                (
                    ensure_tab_workspaces,
                    queue_added_tab_worktrees,
                    start_reconcile,
                    finish_reconcile,
                    resume_page_open,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                rebind_tab_directories
                    .in_set(TabDirectoryRebindSet)
                    .after(finish_reconcile),
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

#[derive(Component)]
pub struct TabWorktreePending;

#[derive(Component)]
struct TabWorktreeTask {
    startup_dir: Option<String>,
    workspace: TabWorkspace,
    metadata: TabWorktree,
    task: Task<Result<TabWorktreeActivation, String>>,
}

#[derive(Component, Clone, Copy)]
pub struct PageOpenWaitForWorktree {
    pub tab: Entity,
}

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

impl TabDirectoryObserved {
    fn directory(&self) -> Option<ObservedDirectory> {
        ObservedDirectory::from_path(&self.path)
    }
}

pub struct WorktreeName;

impl WorktreeName {
    fn sanitize(name: &str) -> String {
        let mut slug = String::new();
        let mut previous_dash = false;
        for character in name.trim().chars() {
            if character.is_ascii_alphanumeric() {
                slug.push(character.to_ascii_lowercase());
                previous_dash = false;
            } else if !previous_dash {
                slug.push('-');
                previous_dash = true;
            }
        }
        let slug = slug.trim_matches('-').to_string();
        if slug.is_empty() {
            "task".to_string()
        } else {
            slug
        }
    }

    pub fn is_generated(name: &str) -> bool {
        name.is_empty()
            || name.strip_prefix("Tab ").is_some_and(|suffix| {
                !suffix.is_empty() && suffix.chars().all(|character| character.is_ascii_digit())
            })
    }

    pub fn hint(tab_name: &str, project_dir: &Path) -> String {
        if Self::is_generated(tab_name) {
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
}

impl TabWorktreeActivation {
    fn repository_storage_dir(managed_root: &Path, checkout: &CheckoutInfo) -> PathBuf {
        let repository_name = checkout
            .common_dir
            .parent()
            .and_then(Path::file_name)
            .or_else(|| checkout.root.file_name())
            .and_then(|name| name.to_str())
            .map(WorktreeName::sanitize)
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
        let repository_dir = Self::repository_storage_dir(managed_root, checkout);
        std::fs::create_dir_all(&repository_dir)
            .map_err(|error| format!("failed to create worktree directory: {error}"))?;
        let repository_dir = repository_dir
            .canonicalize()
            .map_err(|error| format!("invalid repository storage directory: {error}"))?;
        let destination = Self::normalize_missing_path(destination)?;
        if destination.parent() != Some(repository_dir.as_path()) {
            return Err(
                "managed worktree path escapes its repository storage directory".to_string(),
            );
        }
        Ok(destination)
    }

    fn prepare_recovery_destination(
        managed_root: &Path,
        checkout: &CheckoutInfo,
        destination: &Path,
        branch: &str,
    ) -> Result<PathBuf, String> {
        let registrations = checkout.worktree_registrations().map_err(|error| error.0)?;
        if let Ok(destination) = Self::normalize_missing_path(destination)
            && registrations.iter().any(|registration| {
                registration.path == destination && registration.branch.as_deref() == Some(branch)
            })
        {
            return Ok(destination);
        }
        Self::prepare_managed_destination(managed_root, checkout, destination)
    }

    fn canonical_execution_dir(
        checkout_root: &Path,
        relative_dir: &Path,
    ) -> Result<PathBuf, String> {
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

    fn plan_worktree(
        checkout: &CheckoutInfo,
        managed_root: &Path,
        slug_hint: &str,
    ) -> (PathBuf, String) {
        let base = WorktreeName::sanitize(slug_hint);
        let repository_dir = Self::repository_storage_dir(managed_root, checkout);
        let existing = checkout.worktrees().unwrap_or_default();
        let branches = checkout.local_branches().unwrap_or_default();
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

    fn plan_existing_worktree_path(
        checkout: &CheckoutInfo,
        managed_root: &Path,
        branch: &str,
    ) -> PathBuf {
        let base = WorktreeName::sanitize(branch.strip_prefix("vmux/").unwrap_or(branch));
        let repository_dir = Self::repository_storage_dir(managed_root, checkout);
        let registrations = checkout.worktrees().unwrap_or_default();
        let mut slug = base.clone();
        let mut n = 2;
        loop {
            let path = repository_dir.join(&slug);
            if !path.exists() && !registrations.iter().any(|held| held == &path) {
                return path;
            }
            slug = format!("{base}-{n}");
            n += 1;
        }
    }

    fn activate_added_worktree(
        base_dir: &Path,
        checkout: &CheckoutInfo,
        relative_dir: &Path,
        info: &worktree::WorktreeInfo,
    ) -> Result<TabWorktreeActivation, String> {
        let managed_checkout =
            CheckoutInfo::try_from(info.path.as_path()).map_err(|error| error.0)?;
        if managed_checkout.common_dir != checkout.common_dir {
            return Err("managed worktree belongs to a different repository".to_string());
        }
        let execution_dir = Self::canonical_execution_dir(&managed_checkout.root, relative_dir)?;
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
        let info = checkout
            .add_worktree(checkout_dir, branch, base_ref)
            .map_err(|error| error.0)?;
        let activation = Self::activate_added_worktree(base_dir, checkout, relative_dir, &info);
        if activation.is_err() {
            let _ = checkout.remove_worktree(&info.path, &info.branch, false);
        }
        activation
    }

    pub fn create(base_dir: &Path, slug_hint: &str, managed_root: &Path) -> Result<Self, String> {
        let base_dir = base_dir
            .canonicalize()
            .map_err(|error| format!("invalid project directory: {error}"))?;
        let checkout = CheckoutInfo::try_from(base_dir.as_path()).map_err(|error| error.0)?;
        checkout.ensure_initial_commit().map_err(|error| error.0)?;
        let relative_dir = base_dir
            .strip_prefix(&checkout.root)
            .map_err(|_| "project directory is outside its checkout".to_string())?;
        let base_ref = checkout.head_ref().map_err(|error| error.0)?;
        let (checkout_dir, branch) = Self::plan_worktree(&checkout, managed_root, slug_hint);
        let checkout_dir =
            Self::prepare_managed_destination(managed_root, &checkout, &checkout_dir)?;
        Self::add_managed_worktree(
            &base_dir,
            &checkout,
            relative_dir,
            &checkout_dir,
            &branch,
            &base_ref,
        )
    }

    pub fn create_branch(
        base_dir: &Path,
        branch: &str,
        managed_root: &Path,
    ) -> Result<Self, String> {
        let base_dir = base_dir
            .canonicalize()
            .map_err(|error| format!("invalid project directory: {error}"))?;
        let checkout = CheckoutInfo::try_from(base_dir.as_path()).map_err(|error| error.0)?;
        checkout
            .validate_branch_name(branch)
            .map_err(|error| error.0)?;
        checkout.ensure_initial_commit().map_err(|error| error.0)?;
        if let Some(registration) = checkout
            .worktree_registrations()
            .map_err(|error| error.0)?
            .into_iter()
            .find(|registration| registration.branch.as_deref() == Some(branch))
        {
            return Err(format!(
                "Branch {branch} is already checked out at {}",
                registration.path.display()
            ));
        }
        if checkout
            .local_branches()
            .map_err(|error| error.0)?
            .iter()
            .any(|existing| existing == branch)
        {
            return Err(format!("Branch {branch} already exists"));
        }
        let relative_dir = base_dir
            .strip_prefix(&checkout.root)
            .map_err(|_| "project directory is outside its checkout".to_string())?;
        let base_ref = checkout.head_ref().map_err(|error| error.0)?;
        let slug = WorktreeName::sanitize(branch.strip_prefix("vmux/").unwrap_or(branch));
        let checkout_dir = Self::repository_storage_dir(managed_root, &checkout).join(slug);
        let checkout_dir =
            Self::prepare_managed_destination(managed_root, &checkout, &checkout_dir)?;
        Self::add_managed_worktree(
            &base_dir,
            &checkout,
            relative_dir,
            &checkout_dir,
            branch,
            &base_ref,
        )
    }

    pub fn checkout_branch(
        base_dir: &Path,
        branch: &str,
        managed_root: &Path,
    ) -> Result<Self, String> {
        let base_dir = base_dir
            .canonicalize()
            .map_err(|error| format!("invalid project directory: {error}"))?;
        let checkout = CheckoutInfo::try_from(base_dir.as_path()).map_err(|error| error.0)?;
        checkout
            .validate_branch_name(branch)
            .map_err(|error| error.0)?;
        let relative_dir = base_dir
            .strip_prefix(&checkout.root)
            .map_err(|_| "project directory is outside its checkout".to_string())?;
        if let Some(registration) = checkout
            .worktree_registrations()
            .map_err(|error| error.0)?
            .into_iter()
            .find(|registration| registration.branch.as_deref() == Some(branch))
        {
            let info = worktree::WorktreeInfo {
                path: registration.path,
                branch: branch.to_string(),
                base_ref: worktree::BaseRef::resolve(&checkout.root)
                    .map(|base| base.branch().to_string())
                    .unwrap_or_default(),
                repo_root: checkout.root.clone(),
            };
            return Self::activate_added_worktree(&base_dir, &checkout, relative_dir, &info);
        }
        let base_ref = worktree::BaseRef::resolve(&checkout.root)
            .map(|base| base.branch().to_string())
            .unwrap_or_default();
        let checkout_dir = Self::plan_existing_worktree_path(&checkout, managed_root, branch);
        let checkout_dir =
            Self::prepare_managed_destination(managed_root, &checkout, &checkout_dir)?;
        let info = checkout
            .add_existing_worktree(&checkout_dir, branch, &base_ref)
            .map_err(|error| error.0)?;
        Self::activate_added_worktree(&base_dir, &checkout, relative_dir, &info)
    }

    pub fn restore(
        tab: &Tab,
        workspace: &TabWorkspace,
        metadata: &TabWorktree,
        managed_root: &Path,
    ) -> Result<Self, String> {
        let project_dir = Path::new(&workspace.project_dir)
            .canonicalize()
            .map_err(|error| format!("project directory unavailable: {error}"))?;
        let source = CheckoutInfo::try_from(project_dir.as_path()).map_err(|error| error.0)?;
        let relative_dir = project_dir
            .strip_prefix(&source.root)
            .map_err(|_| "project directory is outside its checkout".to_string())?;
        let mut checkout_dir = if metadata.checkout_dir.is_empty() {
            let startup_dir = tab
                .startup_dir
                .as_deref()
                .ok_or_else(|| "managed worktree checkout path is missing".to_string())?;
            CheckoutInfo::try_from(Path::new(startup_dir))
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
            checkout_dir = Self::prepare_recovery_destination(
                managed_root,
                &source,
                &checkout_dir,
                &metadata.branch,
            )?;
            source
                .add_existing_worktree(&checkout_dir, &metadata.branch, &metadata.base_ref)
                .map_err(|error| format!("failed to recover managed worktree: {}", error.0))?;
        }
        let checkout = CheckoutInfo::try_from(checkout_dir.as_path()).map_err(|error| error.0)?;
        if checkout.common_dir != source.common_dir {
            return Err("managed worktree belongs to a different repository".to_string());
        }
        if !CheckoutInfo::is_linked(&checkout.root) {
            return Err("managed worktree directory is not a linked worktree".to_string());
        }
        let branch = checkout.head_ref().map_err(|error| error.0)?;
        if branch != metadata.branch {
            return Err(format!(
                "managed worktree is on branch {branch}, expected {}",
                metadata.branch
            ));
        }
        let execution_dir = Self::canonical_execution_dir(&checkout.root, relative_dir)?;
        let mut normalized = metadata.clone();
        normalized.repo_root = source.root.to_string_lossy().into_owned();
        normalized.checkout_dir = checkout.root.to_string_lossy().into_owned();
        let ready = TabWorktreeReady::new(
            &execution_dir,
            &workspace.project_dir,
            &normalized,
            &checkout,
        )?;
        Ok(Self {
            execution_dir,
            metadata: normalized,
            ready,
        })
    }
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
    mut commands: Commands,
) {
    for (entity, ready) in &worktrees {
        if ready.is_none() {
            commands.entity(entity).insert(TabWorktreePending);
        }
    }
}

fn start_reconcile(
    pending: Query<Entity, (With<TabWorktreePending>, Without<TabWorktreeTask>)>,
    running: Query<(), With<TabWorktreeTask>>,
    tabs: Query<(&Tab, &TabWorkspace, &TabWorktree), Without<TabWorktreeReady>>,
    managed_root: Res<ManagedWorktreeRoot>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    if !running.is_empty() {
        return;
    }
    for entity in &pending {
        let Ok((tab, workspace, metadata)) = tabs.get(entity) else {
            commands.entity(entity).remove::<TabWorktreePending>();
            continue;
        };
        let startup_dir = tab.startup_dir.clone();
        let tab = Tab {
            name: tab.name.clone(),
            startup_dir: tab.startup_dir.clone(),
        };
        let workspace = workspace.clone();
        let metadata = metadata.clone();
        let task_workspace = workspace.clone();
        let task_metadata = metadata.clone();
        let root = managed_root.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result =
                TabWorktreeActivation::restore(&tab, &task_workspace, &task_metadata, &root);
            if let Some(wake) = wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands.entity(entity).insert(TabWorktreeTask {
            startup_dir,
            workspace,
            metadata,
            task,
        });
        break;
    }
}

fn finish_reconcile(
    mut pending: Query<(Entity, &mut TabWorktreeTask)>,
    mut tabs: Query<(&mut Tab, Option<&TabWorkspace>, Option<&TabWorktree>)>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut pending {
        let Some(result) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        let Ok((mut tab, workspace, metadata)) = tabs.get_mut(entity) else {
            commands
                .entity(entity)
                .remove::<(TabWorktreeTask, TabWorktreePending)>();
            continue;
        };
        if tab.startup_dir != pending.startup_dir
            || workspace != Some(&pending.workspace)
            || metadata != Some(&pending.metadata)
        {
            commands
                .entity(entity)
                .remove::<TabWorktreeTask>()
                .insert(TabWorktreePending);
            continue;
        }
        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<(TabWorktreeTask, TabWorktreePending)>();
        match result {
            Ok(activation) => {
                let startup_dir = activation.execution_dir.to_string_lossy().into_owned();
                if tab.startup_dir.as_deref() != Some(&startup_dir) {
                    tab.startup_dir = Some(startup_dir);
                }
                if metadata != Some(&activation.metadata) {
                    entity_commands.insert(activation.metadata);
                }
                entity_commands
                    .insert(activation.ready)
                    .remove::<TabWorktreeUnavailable>();
            }
            Err(message) => {
                entity_commands.insert(TabWorktreeUnavailable { message });
            }
        }
    }
}

fn resume_page_open(
    waiting: Query<(Entity, &PageOpenWaitForWorktree)>,
    tabs: Query<(
        Has<TabWorktreePending>,
        Has<TabWorktreeTask>,
        Option<&TabWorktreeReady>,
        Option<&TabWorktreeUnavailable>,
    )>,
    mut commands: Commands,
) {
    for (entity, waiting) in &waiting {
        let Ok((pending, running, ready, unavailable)) = tabs.get(waiting.tab) else {
            commands
                .entity(entity)
                .remove::<(PageOpenWaitForWorktree, PageOpenDeferred)>()
                .insert(PageOpenError {
                    message: "tab closed while preparing its worktree".to_string(),
                });
            continue;
        };
        if pending || running {
            continue;
        }
        if let Some(unavailable) = unavailable {
            commands
                .entity(entity)
                .remove::<(PageOpenWaitForWorktree, PageOpenDeferred)>()
                .insert(PageOpenError {
                    message: unavailable.message.clone(),
                });
            continue;
        }
        if ready.is_none() {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(PageOpenWaitForWorktree, PageOpenDeferred)>();
    }
}

#[derive(Clone)]
struct CachedCheckoutInfo {
    startup_dir: String,
    info: CheckoutInfo,
    fingerprint: CheckoutFingerprint,
}

#[derive(Default)]
struct CheckoutCache {
    entries: HashMap<Entity, CachedCheckoutInfo>,
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

struct ObservedDirectory(PathBuf);

impl CachedCheckoutInfo {
    fn new(startup_dir: String, info: &CheckoutInfo) -> Option<Self> {
        Some(Self {
            startup_dir,
            info: info.clone(),
            fingerprint: CheckoutFingerprint::read(info)?,
        })
    }

    fn is_current(&self, startup_dir: &str) -> bool {
        self.startup_dir == startup_dir
            && CheckoutFingerprint::read(&self.info).as_ref() == Some(&self.fingerprint)
    }
}

impl CheckoutCache {
    fn remove(&mut self, tab: Entity) {
        self.entries.remove(&tab);
    }

    fn store(&mut self, tab: Entity, startup_dir: String, info: &CheckoutInfo) {
        let Some(cached) = CachedCheckoutInfo::new(startup_dir, info) else {
            self.remove(tab);
            return;
        };
        self.entries.insert(tab, cached);
    }

    fn resolve(
        &mut self,
        tab: Entity,
        startup_dir: &str,
        resolve: impl FnOnce(&Path) -> Option<CheckoutInfo>,
    ) -> Option<CheckoutInfo> {
        if let Some(cached) = self.entries.get(&tab)
            && cached.is_current(startup_dir)
        {
            return Some(cached.info.clone());
        }
        self.remove(tab);
        let info = resolve(Path::new(startup_dir))?;
        self.store(tab, startup_dir.to_string(), &info);
        Some(info)
    }
}

impl PathFingerprint {
    fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        })
    }
}

impl CheckoutFingerprint {
    fn read(info: &CheckoutInfo) -> Option<Self> {
        let dot_git_path = info.root.join(".git");
        let dot_git = PathFingerprint::read(&dot_git_path)?;
        let admin_dir = Self::git_admin_dir(&info.root)?;
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
        Some(Self {
            dot_git,
            admin_dir,
            common_dir,
            commondir,
            gitdir,
            head,
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
}

impl ObservedDirectory {
    fn from_path(path: &Path) -> Option<Self> {
        if !path.is_absolute() || !path.exists() {
            return None;
        }
        let start = if path.is_dir() { path } else { path.parent()? };
        Some(Self(start.canonicalize().ok()?))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn is_within(&self, root: &Path) -> bool {
        self.0.starts_with(root)
            && !self
                .0
                .ancestors()
                .take_while(|ancestor| *ancestor != root)
                .any(|ancestor| ancestor.join(".git").exists())
    }
}

impl TabWorktreeReady {
    pub fn new(
        execution_dir: &Path,
        project_dir: &str,
        metadata: &TabWorktree,
        checkout: &CheckoutInfo,
    ) -> Result<Self, String> {
        let checkout_fingerprint = CheckoutFingerprint::read(checkout)
            .ok_or_else(|| "failed to fingerprint managed worktree".to_string())?;
        let execution_fingerprint = PathFingerprint::read(execution_dir)
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
            && CheckoutFingerprint::read(&self.checkout).as_ref()
                == Some(&self.checkout_fingerprint)
            && PathFingerprint::read(Path::new(&self.startup_dir)).as_ref()
                == Some(&self.execution_fingerprint)
    }
}

fn rebind_tab_directories(
    mut reader: MessageReader<TabDirectoryObserved>,
    mut tabs: Query<&mut Tab>,
    mut workspaces: Query<&mut TabWorkspace>,
    managed: Query<(), With<TabWorktree>>,
    mut removed_tabs: RemovedComponents<Tab>,
    mut checkout_cache: Local<CheckoutCache>,
    mut commands: Commands,
) {
    for tab in removed_tabs.read() {
        checkout_cache.remove(tab);
    }
    for observed in reader.read() {
        let Some(observed_dir) = observed.directory() else {
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
        if observed_dir.is_within(&current_dir) {
            continue;
        }
        let Ok(observed_info) = CheckoutInfo::try_from(observed_dir.path()) else {
            continue;
        };
        let current_info = checkout_cache.resolve(observed.tab, &current, |path| {
            CheckoutInfo::try_from(path).ok()
        });
        if current_info.is_none()
            && current_dir
                .ancestors()
                .any(|ancestor| ancestor.join(".git").exists())
        {
            continue;
        }
        if current_info
            .as_ref()
            .is_some_and(|current_info| observed_dir.is_within(&current_info.root))
        {
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
        checkout_cache.store(observed.tab, startup_dir, &observed_info);
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
    use std::process::Command;

    #[cfg(all(unix, not(target_os = "macos")))]
    use std::ffi::OsString;
    #[cfg(all(unix, not(target_os = "macos")))]
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;

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
        assert_eq!(WorktreeName::sanitize("Auth Refactor!"), "auth-refactor");
        assert_eq!(WorktreeName::sanitize("  a//b  "), "a-b");
        assert_eq!(WorktreeName::sanitize("***"), "task");
        assert_eq!(WorktreeName::sanitize(""), "task");
    }

    #[test]
    fn create_worktree_blocking_uses_repository_hashed_global_root() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let activation =
            TabWorktreeActivation::create(repo.path(), "Auth Refactor", managed_root.path())
                .unwrap();
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
    fn create_worktree_for_branch_uses_exact_valid_branch() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();

        let activation = TabWorktreeActivation::create_branch(
            repo.path(),
            "vmux/fix-dashboard-tests",
            managed_root.path(),
        )
        .unwrap();

        assert_eq!(activation.metadata.branch, "vmux/fix-dashboard-tests");
        assert!(activation.execution_dir.ends_with("fix-dashboard-tests"));
        assert_eq!(
            CheckoutInfo::try_from(activation.execution_dir.as_path())
                .unwrap()
                .head_ref()
                .unwrap(),
            "vmux/fix-dashboard-tests"
        );
    }

    #[test]
    fn create_worktree_for_branch_initializes_unborn_repository() {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-q", "-b", "main"]);
        git(repo.path(), &["config", "user.email", "t@example.com"]);
        git(repo.path(), &["config", "user.name", "Test"]);
        git(repo.path(), &["config", "commit.gpgsign", "false"]);
        let managed_root = tempfile::tempdir().unwrap();

        let activation = TabWorktreeActivation::create_branch(
            repo.path(),
            "feat/izakaya-website",
            managed_root.path(),
        )
        .unwrap();

        assert_eq!(activation.metadata.base_ref, "main");
        assert_eq!(
            CheckoutInfo::try_from(activation.execution_dir.as_path())
                .unwrap()
                .head_ref()
                .unwrap(),
            "feat/izakaya-website"
        );
        assert_eq!(
            CheckoutInfo::try_from(repo.path())
                .unwrap()
                .head_ref()
                .unwrap(),
            "main"
        );
    }

    #[test]
    fn create_worktree_from_repository_marked_bare() {
        let repo = init_repo();
        git(repo.path(), &["config", "core.bare", "true"]);
        let managed_root = tempfile::tempdir().unwrap();

        let activation = TabWorktreeActivation::create_branch(
            repo.path(),
            "vmux/bare-source",
            managed_root.path(),
        )
        .unwrap();

        assert_eq!(activation.metadata.branch, "vmux/bare-source");
        assert_eq!(
            CheckoutInfo::try_from(activation.execution_dir.as_path())
                .unwrap()
                .head_ref()
                .unwrap(),
            "vmux/bare-source"
        );
    }

    #[test]
    fn create_worktree_for_branch_rejects_existing_or_invalid_branch() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        git(repo.path(), &["branch", "vmux/existing"]);

        let existing =
            TabWorktreeActivation::create_branch(repo.path(), "vmux/existing", managed_root.path())
                .unwrap_err();
        let invalid =
            TabWorktreeActivation::create_branch(repo.path(), "bad branch", managed_root.path())
                .unwrap_err();

        assert!(existing.contains("already exists"));
        assert!(!invalid.is_empty());
    }

    #[test]
    fn existing_branch_becomes_a_reusable_managed_worktree() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        git(repo.path(), &["branch", "feature"]);

        let first =
            TabWorktreeActivation::checkout_branch(repo.path(), "feature", managed_root.path())
                .unwrap();
        let second =
            TabWorktreeActivation::checkout_branch(repo.path(), "feature", managed_root.path())
                .unwrap();

        assert_eq!(first.execution_dir, second.execution_dir);
        assert_eq!(first.metadata.branch, "feature");
        assert_eq!(
            CheckoutInfo::try_from(first.execution_dir.as_path())
                .unwrap()
                .head_ref()
                .unwrap(),
            "feature"
        );
    }

    #[test]
    fn generated_tab_names_use_project_name_as_slug_hint() {
        assert_eq!(
            WorktreeName::hint("Tab 2", Path::new("/repo/dashboard")),
            "dashboard"
        );
        assert_eq!(
            WorktreeName::hint("Auth Refactor", Path::new("/repo/dashboard")),
            "Auth Refactor"
        );
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

        let activation =
            TabWorktreeActivation::create(&nested, "nested", managed_root.path()).unwrap();

        assert!(activation.execution_dir.ends_with("nested/crates/app"));
        assert!(activation.execution_dir.join("main.rs").is_file());
    }

    #[test]
    fn plan_worktree_skips_existing_branch_name() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        git(repo.path(), &["branch", "vmux/feat"]);
        let checkout = CheckoutInfo::try_from(repo.path()).unwrap();
        let (path, branch) =
            TabWorktreeActivation::plan_worktree(&checkout, managed_root.path(), "feat");
        assert_eq!(branch, "vmux/feat-2");
        assert!(path.starts_with(managed_root.path()));
        assert!(path.ends_with("feat-2"), "{path:?}");
    }

    #[test]
    fn reconcile_recovers_missing_worktree_without_dropping_metadata() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let activation =
            TabWorktreeActivation::create(repo.path(), "recover", managed_root.path()).unwrap();
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

        for _ in 0..200 {
            app.update();
            if app.world().get::<TabWorktreeReady>(tab).is_some()
                || app.world().get::<TabWorktreeUnavailable>(tab).is_some()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(checkout_dir.is_dir());
        assert!(app.world().get::<TabWorktree>(tab).is_some());
        assert!(app.world().get::<TabWorktreeUnavailable>(tab).is_none());
    }

    #[test]
    fn recovery_recreates_pruned_managed_registration() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let activation =
            TabWorktreeActivation::create(repo.path(), "recover", managed_root.path()).unwrap();
        std::fs::remove_dir_all(&activation.metadata.checkout_dir).unwrap();
        git(repo.path(), &["worktree", "prune", "--expire", "now"]);
        let tab = Tab {
            name: "recover".into(),
            startup_dir: Some(activation.execution_dir.to_string_lossy().into_owned()),
        };
        let workspace = TabWorkspace {
            project_dir: repo.path().to_string_lossy().into_owned(),
        };

        let recovered = TabWorktreeActivation::restore(
            &tab,
            &workspace,
            &activation.metadata,
            managed_root.path(),
        )
        .unwrap();

        assert!(recovered.execution_dir.is_dir());
        assert_eq!(
            CheckoutInfo::try_from(recovered.execution_dir.as_path())
                .unwrap()
                .head_ref()
                .unwrap(),
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

        for _ in 0..200 {
            app.update();
            if app.world().get::<TabWorktreeUnavailable>(tab).is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(app.world().get::<TabWorktree>(tab).is_some());
        assert!(app.world().get::<TabWorktreeUnavailable>(tab).is_some());
    }

    #[test]
    fn recovery_rejects_unregistered_path_outside_managed_root() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let activation =
            TabWorktreeActivation::create(repo.path(), "managed", managed_root.path()).unwrap();
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

        let error =
            TabWorktreeActivation::restore(&tab, &workspace, &metadata, managed_root.path())
                .unwrap_err();

        assert!(error.contains("repository storage directory"));
        assert!(!outside.exists());
    }

    #[cfg(unix)]
    #[test]
    fn managed_project_directory_cannot_escape_through_symlink() {
        let repo = init_repo();
        let nested = repo.path().join("crates/app");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("main.rs"), "fn main() {}\n").unwrap();
        git(repo.path(), &["add", "crates/app/main.rs"]);
        git(repo.path(), &["commit", "-qm", "nested project"]);
        let managed_root = tempfile::tempdir().unwrap();
        let activation =
            TabWorktreeActivation::create(&nested, "managed", managed_root.path()).unwrap();
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

        let error = TabWorktreeActivation::restore(
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
        let first =
            TabWorktreeActivation::create(repo.path(), "first", managed_root.path()).unwrap();
        let second =
            TabWorktreeActivation::create(repo.path(), "second", managed_root.path()).unwrap();
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

        for _ in 0..200 {
            app.update();
            let ready = app
                .world()
                .iter_entities()
                .filter(|entity| entity.contains::<TabWorktreeReady>())
                .count();
            if ready == 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            app.world()
                .iter_entities()
                .filter(|entity| entity.contains::<TabWorktreeReady>())
                .count(),
            1
        );

        for _ in 0..200 {
            app.update();
            let ready = app
                .world()
                .iter_entities()
                .filter(|entity| entity.contains::<TabWorktreeReady>())
                .count();
            if ready == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
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
            TabWorktreeActivation::create(repo.path(), "managed", managed_root.path()).unwrap();
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
            TabWorktreeActivation::create(repo.path(), "managed", managed_root.path()).unwrap();
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
        let first =
            TabWorktreeActivation::create(repo.path(), "first", managed_root.path()).unwrap();
        let second_path = repo.path().join(".worktrees/second");
        CheckoutInfo::try_from(repo.path())
            .unwrap()
            .add_worktree(&second_path, "vmux/second", "main")
            .unwrap();
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
        CheckoutInfo::try_from(repo.path())
            .unwrap()
            .add_worktree(&linked_path, "vmux/linked", "main")
            .unwrap();
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
        let mut cache = CheckoutCache::default();

        let resolved = cache
            .resolve(tab, &startup_dir, |_| {
                calls.set(calls.get() + 1);
                Some(first.clone())
            })
            .unwrap();
        assert_eq!(resolved, first);
        let resolved = cache
            .resolve(tab, &startup_dir, |_| {
                calls.set(calls.get() + 1);
                Some(second.clone())
            })
            .unwrap();
        assert_eq!(resolved, first);
        std::fs::rename(repo.path().join(".git"), repo.path().join(".git-old")).unwrap();
        std::fs::create_dir(repo.path().join(".git")).unwrap();
        let resolved = cache
            .resolve(tab, &startup_dir, |_| {
                calls.set(calls.get() + 1);
                Some(first.clone())
            })
            .unwrap();
        assert_eq!(resolved, first);
        let resolved = cache
            .resolve(tab, &next_startup_dir, |_| {
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
        let mut cache = CheckoutCache::default();

        let resolved = cache
            .resolve(tab, &startup_dir, |_| {
                calls.set(calls.get() + 1);
                Some(first.clone())
            })
            .unwrap();
        assert_eq!(resolved, first);
        let resolved = cache
            .resolve(tab, &startup_dir, |_| {
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
        let resolved = cache
            .resolve(tab, &startup_dir, |_| {
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
        assert!(ObservedDirectory::from_path(Path::new(".")).is_none());
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
