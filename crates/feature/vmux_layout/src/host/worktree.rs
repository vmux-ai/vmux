use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use bevy::prelude::*;

use crate::tab::{Tab, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_ecs::{PageOpenDeferred, PageOpenError};
use vmux_git::worktree::CheckoutInfo;

pub use super::checkout::{
    CancelCheckout, CheckoutFailed, CheckoutPlugin, CheckoutPrepared, CheckoutRecoveryComplete,
    CheckoutTargetRevision, CommitCheckout, CreateBranchCheckout, CreateCheckout, PreparedCheckout,
    RecoverCheckouts, RestoreCheckout, SelectBranchCheckout,
};

impl Plugin for WorktreePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CheckoutPlugin>() {
            app.add_plugins(CheckoutPlugin);
        }
        app.init_resource::<ManagedWorktreeRoot>()
            .init_resource::<vmux_git::GitRuntime>()
            .add_message::<TabDirectoryObserved>()
            .add_systems(Startup, recover_checkouts)
            .add_systems(
                Update,
                (
                    retire_checkout_observations,
                    ensure_tab_workspaces,
                    queue_added_tab_worktrees,
                    retire_reconcile,
                    start_reconcile,
                    finish_reconcile,
                    commit_reconcile,
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

fn recover_checkouts(
    root: Res<ManagedWorktreeRoot>,
    mut requests: MessageWriter<RecoverCheckouts>,
    mut commands: Commands,
) {
    let operation = commands
        .spawn((
            Name::new("Managed checkout journal recovery"),
            vmux_ecs::Operation::bundle("checkout.recovery").expect("fixed checkout recovery kind"),
        ))
        .id();
    requests.write(RecoverCheckouts {
        operation,
        managed_root: root.0.clone(),
    });
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
    operation: Entity,
    revision: u64,
}

#[derive(Component)]
struct ReconcileApplied;

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

#[derive(Component)]
pub struct WorktreeName(pub(crate) String);

impl WorktreeName {
    pub(crate) fn sanitize(name: &str) -> String {
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

fn retire_reconcile(
    pending: Query<(
        Entity,
        &TabWorktreeTask,
        Option<&Tab>,
        Option<&TabWorkspace>,
        Option<&TabWorktree>,
        Option<&CheckoutTargetRevision>,
        Has<super::page_context::PendingWorkspaceSelection>,
    )>,
    mut cancels: MessageWriter<CancelCheckout>,
    mut commands: Commands,
) {
    for (tab, task, current, workspace, metadata, revision, selecting) in &pending {
        if !selecting
            && current.is_some_and(|tab| tab.startup_dir == task.startup_dir)
            && workspace == Some(&task.workspace)
            && metadata == Some(&task.metadata)
            && revision.is_some_and(|revision| revision.0 == task.revision)
        {
            continue;
        }
        cancels.write(CancelCheckout {
            operation: task.operation,
        });
        commands
            .entity(tab)
            .remove::<(TabWorktreeTask, ReconcileApplied)>();
    }
}

fn start_reconcile(
    pending: Query<Entity, (With<TabWorktreePending>, Without<TabWorktreeTask>)>,
    tabs: Query<
        (
            &Tab,
            &TabWorkspace,
            &TabWorktree,
            Option<&CheckoutTargetRevision>,
        ),
        (
            Without<TabWorktreeReady>,
            Without<super::page_context::PendingWorkspaceSelection>,
        ),
    >,
    managed_root: Res<ManagedWorktreeRoot>,
    mut requests: MessageWriter<RestoreCheckout>,
    mut commands: Commands,
) {
    for entity in &pending {
        let Ok((tab, workspace, metadata, current)) = tabs.get(entity) else {
            continue;
        };
        let checkout = if metadata.checkout_dir.is_empty() {
            match &tab.startup_dir {
                Some(directory) => PathBuf::from(directory),
                None => {
                    commands
                        .entity(entity)
                        .remove::<TabWorktreePending>()
                        .insert(TabWorktreeUnavailable {
                            message: "managed worktree checkout path is missing".into(),
                        });
                    continue;
                }
            }
        } else {
            PathBuf::from(&metadata.checkout_dir)
        };
        let revision = current.map_or(1, |current| current.0.wrapping_add(1).max(1));
        let operation = commands
            .spawn((
                Name::new("Restore saved tab checkout"),
                vmux_ecs::Operation::bundle("tab.checkout.restore").expect("fixed restore kind"),
                vmux_ecs::OperationTarget::new(entity),
                vmux_ecs::OperationTargetRevision(revision),
            ))
            .id();
        commands.entity(entity).insert((
            CheckoutTargetRevision(revision),
            TabWorktreeTask {
                startup_dir: tab.startup_dir.clone(),
                workspace: workspace.clone(),
                metadata: metadata.clone(),
                operation,
                revision,
            },
        ));
        requests.write(RestoreCheckout {
            operation,
            directory: workspace.project_dir.clone().into(),
            managed_root: managed_root.0.clone(),
            checkout,
            branch: metadata.branch.clone(),
            base_ref: metadata.base_ref.clone(),
        });
    }
}

fn finish_reconcile(
    mut prepared: MessageReader<CheckoutPrepared>,
    mut failures: MessageReader<CheckoutFailed>,
    resolved: Query<&PreparedCheckout>,
    mut pending: Query<(Entity, &mut TabWorktreeTask)>,
    mut tabs: Query<&mut Tab>,
    states: Query<&vmux_ecs::OperationState>,
    mut commands: Commands,
) {
    for message in prepared.read() {
        let Ok(prepared) = resolved.get(message.operation) else {
            continue;
        };
        for (entity, mut task) in &mut pending {
            if task.operation != message.operation {
                continue;
            }
            let Ok(mut tab) = tabs.get_mut(entity) else {
                continue;
            };
            let activation = &prepared.activation;
            tab.startup_dir = Some(activation.execution_dir.to_string_lossy().into_owned());
            task.startup_dir = tab.startup_dir.clone();
            task.metadata = activation.metadata.clone();
            task.workspace.project_dir = activation.ready.project_dir.clone();
            commands
                .entity(entity)
                .insert((
                    task.workspace.clone(),
                    activation.metadata.clone(),
                    activation.ready.clone(),
                    ReconcileApplied,
                ))
                .remove::<TabWorktreeUnavailable>();
        }
    }
    for failure in failures.read() {
        for (entity, task) in &pending {
            if task.operation != failure.operation {
                continue;
            }
            commands
                .entity(entity)
                .remove::<(TabWorktreeTask, TabWorktreePending, ReconcileApplied)>()
                .insert(TabWorktreeUnavailable {
                    message: failure.error.clone(),
                });
        }
    }
    for (entity, task) in &pending {
        if states
            .get(task.operation)
            .is_ok_and(|state| *state == vmux_ecs::OperationState::Succeeded)
        {
            commands
                .entity(entity)
                .remove::<(TabWorktreeTask, TabWorktreePending, ReconcileApplied)>();
        } else if states
            .get(task.operation)
            .is_ok_and(|state| *state == vmux_ecs::OperationState::Canceled)
        {
            commands
                .entity(entity)
                .remove::<(TabWorktreeTask, ReconcileApplied)>();
        }
    }
}

fn commit_reconcile(
    tabs: Query<(Entity, &TabWorktreeTask), Added<ReconcileApplied>>,
    mut commits: MessageWriter<CommitCheckout>,
) {
    for (tab, task) in &tabs {
        commits.write(CommitCheckout {
            operation: task.operation,
            target: tab,
            revision: task.revision,
        });
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

#[derive(Component, Default)]
pub(crate) struct CheckoutObservation(Option<CheckoutIdentity>);

#[derive(Clone)]
struct CheckoutIdentity {
    startup_dir: String,
    info: CheckoutInfo,
    fingerprint: CheckoutFingerprint,
}

#[derive(bevy::ecs::system::SystemParam)]
struct CheckoutObservations<'w, 's> {
    values: Query<'w, 's, &'static mut CheckoutObservation>,
    commands: Commands<'w, 's>,
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

impl CheckoutIdentity {
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

impl CheckoutObservations<'_, '_> {
    fn current(&self, tab: Entity, startup_dir: &str) -> Option<CheckoutInfo> {
        let observation = self.values.get(tab).ok()?.0.as_ref()?;
        if !observation.is_current(startup_dir) {
            return None;
        }
        Some(observation.info.clone())
    }

    fn store(&mut self, tab: Entity, startup_dir: String, info: Option<&CheckoutInfo>) {
        let next = info.and_then(|info| CheckoutIdentity::new(startup_dir, info));
        match self.values.get_mut(tab) {
            Ok(mut observation) => observation.0 = next,
            Err(_) => {
                self.commands.entity(tab).insert(CheckoutObservation(next));
            }
        }
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

fn retire_checkout_observations(
    mut removed: RemovedComponents<Tab>,
    tabs: Query<(), With<Tab>>,
    mut observations: Query<&mut CheckoutObservation>,
    mut commands: Commands,
) {
    for entity in removed.read() {
        let Ok(mut observation) = observations.get_mut(entity) else {
            continue;
        };
        if tabs.contains(entity) {
            observation.0 = None;
        } else {
            commands.entity(entity).remove::<CheckoutObservation>();
        }
    }
}

fn rebind_tab_directories(
    mut reader: MessageReader<TabDirectoryObserved>,
    mut tabs: Query<&mut Tab>,
    mut workspaces: Query<&mut TabWorkspace>,
    managed: Query<(), With<TabWorktree>>,
    mut checkouts: CheckoutObservations,
    mut commands: Commands,
) {
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
        let current_info = match checkouts.current(observed.tab, &current) {
            Some(info) => Some(info),
            None => {
                let info = CheckoutInfo::try_from(Path::new(&current)).ok();
                checkouts.store(observed.tab, current, info.as_ref());
                info
            }
        };
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
        checkouts.store(observed.tab, startup_dir, Some(&observed_info));
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

    struct CheckoutFixture {
        app: App,
        directory: PathBuf,
        managed: PathBuf,
        target: Entity,
        revision: u64,
        operation: Option<Entity>,
        failures: bevy::ecs::message::MessageCursor<CheckoutFailed>,
    }

    impl CheckoutFixture {
        fn new(directory: &Path, managed: &Path) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, CheckoutPlugin));
            let target = app.world_mut().spawn(Tab::default()).id();
            Self {
                app,
                directory: directory.to_path_buf(),
                managed: managed.to_path_buf(),
                target,
                revision: 0,
                operation: None,
                failures: Default::default(),
            }
        }

        fn operation(&mut self) -> Entity {
            self.revision += 1;
            self.app
                .world_mut()
                .entity_mut(self.target)
                .insert(CheckoutTargetRevision(self.revision));
            let operation = self
                .app
                .world_mut()
                .spawn((
                    Name::new("Fixture checkout request"),
                    vmux_ecs::Operation::bundle("fixture.checkout").unwrap(),
                    vmux_ecs::OperationTarget::new(self.target),
                    vmux_ecs::OperationTargetRevision(self.revision),
                ))
                .id();
            self.operation = Some(operation);
            operation
        }

        fn create(&mut self, hint: &str) -> Result<TabWorktreeActivation, String> {
            let operation = self.operation();
            self.app.world_mut().write_message(CreateCheckout {
                operation,
                directory: self.directory.clone(),
                managed_root: self.managed.clone(),
                slug_hint: hint.into(),
            });
            self.resolve()
        }

        fn branch(&mut self, branch: &str) -> Result<TabWorktreeActivation, String> {
            let operation = self.operation();
            self.app.world_mut().write_message(CreateBranchCheckout {
                operation,
                directory: self.directory.clone(),
                managed_root: self.managed.clone(),
                branch: branch.into(),
            });
            self.resolve()
        }

        fn restore(
            &mut self,
            tab: &Tab,
            metadata: &TabWorktree,
        ) -> Result<TabWorktreeActivation, String> {
            let operation = self.operation();
            let checkout = if metadata.checkout_dir.is_empty() {
                tab.startup_dir.clone().unwrap_or_default()
            } else {
                metadata.checkout_dir.clone()
            };
            self.app.world_mut().write_message(RestoreCheckout {
                operation,
                directory: self.directory.clone(),
                managed_root: self.managed.clone(),
                checkout: checkout.into(),
                branch: metadata.branch.clone(),
                base_ref: metadata.base_ref.clone(),
            });
            self.resolve()
        }

        fn resolve(&mut self) -> Result<TabWorktreeActivation, String> {
            let operation = self.operation.unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let activation = loop {
                self.app.update();
                for failure in self
                    .failures
                    .read(self.app.world().resource::<Messages<CheckoutFailed>>())
                {
                    if failure.operation == operation {
                        return Err(failure.error.clone());
                    }
                }
                if let Some(prepared) = self.app.world().get::<PreparedCheckout>(operation) {
                    break prepared.activation.clone();
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "checkout fixture did not resolve"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            };
            self.app
                .world_mut()
                .entity_mut(self.target)
                .insert(activation.metadata.clone());
            self.app.world_mut().write_message(CommitCheckout {
                operation,
                target: self.target,
                revision: self.revision,
            });
            while self.app.world().get::<vmux_ecs::OperationState>(operation)
                != Some(&vmux_ecs::OperationState::Succeeded)
            {
                self.app.update();
                for failure in self
                    .failures
                    .read(self.app.world().resource::<Messages<CheckoutFailed>>())
                {
                    if failure.operation == operation {
                        return Err(failure.error.clone());
                    }
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "checkout fixture did not commit"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Ok(activation)
        }
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
        let activation = CheckoutFixture::new(repo.path(), managed_root.path())
            .create("Auth Refactor")
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

        let activation = CheckoutFixture::new(repo.path(), managed_root.path())
            .branch("vmux/fix-dashboard-tests")
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

        let activation = CheckoutFixture::new(repo.path(), managed_root.path())
            .branch("feat/izakaya-website")
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

        let activation = CheckoutFixture::new(repo.path(), managed_root.path())
            .branch("vmux/bare-source")
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

        let existing = CheckoutFixture::new(repo.path(), managed_root.path())
            .branch("vmux/existing")
            .unwrap_err();
        let invalid = CheckoutFixture::new(repo.path(), managed_root.path())
            .branch("bad branch")
            .unwrap_err();

        assert!(existing.contains("already exists"));
        assert!(!invalid.is_empty());
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

        let activation = CheckoutFixture::new(&nested, managed_root.path())
            .create("nested")
            .unwrap();

        assert!(activation.execution_dir.ends_with("nested/crates/app"));
        assert!(activation.execution_dir.join("main.rs").is_file());
    }

    #[test]
    fn checkout_allocation_skips_existing_branch_name() {
        let repo = init_repo();
        let managed = tempfile::tempdir().unwrap();
        git(repo.path(), &["branch", "vmux/feat"]);
        let activation = CheckoutFixture::new(repo.path(), managed.path())
            .create("feat")
            .unwrap();
        assert_eq!(
            CheckoutInfo::try_from(activation.execution_dir.as_path())
                .unwrap()
                .head_ref()
                .unwrap(),
            "vmux/feat-2"
        );
    }

    #[test]
    fn reconcile_recovers_missing_worktree_without_dropping_metadata() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let activation = CheckoutFixture::new(repo.path(), managed_root.path())
            .create("recover")
            .unwrap();
        let checkout_dir = PathBuf::from(&activation.metadata.checkout_dir);
        std::fs::remove_dir_all(&checkout_dir).unwrap();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin)
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()));
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

        for _ in 0..1000 {
            app.update();
            if app.world().get::<TabWorktreeReady>(tab).is_some()
                || app.world().get::<TabWorktreeUnavailable>(tab).is_some()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(
            checkout_dir.is_dir(),
            "{:?}",
            app.world().get::<TabWorktreeUnavailable>(tab)
        );
        assert!(app.world().get::<TabWorktree>(tab).is_some());
        assert!(app.world().get::<TabWorktreeUnavailable>(tab).is_none());
    }

    #[test]
    fn recovery_recreates_pruned_managed_registration() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let activation = CheckoutFixture::new(repo.path(), managed_root.path())
            .create("recover")
            .unwrap();
        std::fs::remove_dir_all(&activation.metadata.checkout_dir).unwrap();
        git(repo.path(), &["worktree", "prune", "--expire", "now"]);
        let tab = Tab {
            name: "recover".into(),
            startup_dir: Some(activation.execution_dir.to_string_lossy().into_owned()),
        };
        let workspace = TabWorkspace {
            project_dir: repo.path().to_string_lossy().into_owned(),
        };

        let recovered =
            CheckoutFixture::new(Path::new(&workspace.project_dir), managed_root.path())
                .restore(&tab, &activation.metadata)
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
        let activation = CheckoutFixture::new(repo.path(), managed_root.path())
            .create("managed")
            .unwrap();
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

        let error = CheckoutFixture::new(Path::new(&workspace.project_dir), managed_root.path())
            .restore(&tab, &metadata)
            .unwrap_err();

        assert!(!error.is_empty());
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
        let activation = CheckoutFixture::new(&nested, managed_root.path())
            .create("managed")
            .unwrap();
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

        let error = CheckoutFixture::new(Path::new(&workspace.project_dir), managed_root.path())
            .restore(&tab, &activation.metadata)
            .unwrap_err();

        assert!(error.contains("escapes worktree"));
    }

    #[test]
    fn restore_reconciles_at_most_one_worktree_per_frame() {
        let repo = init_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let first = CheckoutFixture::new(repo.path(), managed_root.path())
            .create("first")
            .unwrap();
        let second = CheckoutFixture::new(repo.path(), managed_root.path())
            .create("second")
            .unwrap();
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

        for _ in 0..1000 {
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

        for _ in 0..1000 {
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
        let managed = CheckoutFixture::new(repo.path(), managed_root.path())
            .create("managed")
            .unwrap();
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
        let managed = CheckoutFixture::new(repo.path(), managed_root.path())
            .create("managed")
            .unwrap();
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
        let first = CheckoutFixture::new(repo.path(), managed_root.path())
            .create("first")
            .unwrap();
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
    fn checkout_observation_revalidates_the_canonical_startup_directory() {
        let first = init_repo();
        let second = init_repo();
        let linked = second.path().join(".worktrees/next");
        git(
            second.path(),
            &["worktree", "add", "-qb", "next", linked.to_str().unwrap()],
        );
        let first_dir = first
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let second_dir = second
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
                startup_dir: Some(first_dir.clone()),
            })
            .id();

        observe(&mut app, tab, &second.path().join("seed.txt"));
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(first_dir.as_str())
        );
        app.world_mut().get_mut::<Tab>(tab).unwrap().startup_dir = Some(second_dir.clone());
        observe(&mut app, tab, &first.path().join("seed.txt"));
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(second_dir.as_str())
        );
        observe(&mut app, tab, &linked.join("seed.txt"));
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            linked.canonicalize().unwrap().to_str()
        );
    }

    #[test]
    fn checkout_observation_revalidates_changed_git_common_directory() {
        let first = init_repo();
        let second = init_repo();
        let linked = first.path().join(".worktrees/linked");
        git(
            first.path(),
            &["worktree", "add", "-qb", "linked", linked.to_str().unwrap()],
        );
        let linked_dir = linked
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
                startup_dir: Some(linked_dir.clone()),
            })
            .id();

        observe(&mut app, tab, &second.path().join("seed.txt"));
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(linked_dir.as_str())
        );
        let marker = std::fs::read_to_string(linked.join(".git")).unwrap();
        let admin = PathBuf::from(marker.trim().strip_prefix("gitdir: ").unwrap());
        std::fs::write(
            admin.join("commondir"),
            second
                .path()
                .join(".git")
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .as_bytes(),
        )
        .unwrap();
        observe(&mut app, tab, &second.path().join("seed.txt"));
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            second.path().canonicalize().unwrap().to_str()
        );
    }

    #[test]
    fn removing_a_tab_retires_its_checkout_observation_before_reuse() {
        for same_frame in [false, true] {
            let first = init_repo();
            let second = init_repo();
            let first_dir = first
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
                    name: "old".into(),
                    startup_dir: Some(first_dir.clone()),
                })
                .id();

            observe_edit(&mut app, tab, &second.path().join("seed.txt"));
            assert_eq!(
                app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
                second.path().canonicalize().unwrap().to_str()
            );
            assert!(
                app.world()
                    .get::<CheckoutObservation>(tab)
                    .unwrap()
                    .0
                    .is_some()
            );
            app.world_mut().entity_mut(tab).remove::<Tab>();
            if !same_frame {
                app.update();
                assert!(app.world().get::<CheckoutObservation>(tab).is_none());
            }
            app.world_mut().entity_mut(tab).insert(Tab {
                name: "new".into(),
                startup_dir: Some(first_dir.clone()),
            });
            app.update();
            assert!(
                app.world()
                    .get::<CheckoutObservation>(tab)
                    .unwrap()
                    .0
                    .is_none(),
                "same-frame replacement: {same_frame}"
            );
            observe(&mut app, tab, &second.path().join("seed.txt"));
            assert_eq!(
                app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
                Some(first_dir.as_str())
            );
        }
    }

    #[test]
    fn repeated_directory_observations_keep_each_tabs_latest_checkout() {
        let first = init_repo();
        let second = init_repo();
        let third = init_repo();
        let linked = third.path().join(".worktrees/next");
        git(
            third.path(),
            &["worktree", "add", "-qb", "next", linked.to_str().unwrap()],
        );
        let first_dir = first
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
                name: "changing".into(),
                startup_dir: Some(first_dir.clone()),
            })
            .id();
        let untouched = app
            .world_mut()
            .spawn(Tab {
                name: "untouched".into(),
                startup_dir: Some(first_dir.clone()),
            })
            .id();
        app.update();
        for repository in [&second, &third] {
            app.world_mut()
                .resource_mut::<Messages<TabDirectoryObserved>>()
                .write(TabDirectoryObserved {
                    tab,
                    path: repository.path().join("seed.txt"),
                    kind: TabDirectoryObservationKind::Edit,
                });
        }
        app.update();
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            third.path().canonicalize().unwrap().to_str()
        );
        assert_eq!(
            app.world().get::<TabWorkspace>(tab).unwrap().project_dir,
            third.path().canonicalize().unwrap().to_string_lossy()
        );
        observe(&mut app, tab, &second.path().join("seed.txt"));
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            third.path().canonicalize().unwrap().to_str()
        );
        observe(&mut app, tab, &linked.join("seed.txt"));
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            linked.canonicalize().unwrap().to_str()
        );
        assert_eq!(
            app.world()
                .get::<Tab>(untouched)
                .unwrap()
                .startup_dir
                .as_deref(),
            Some(first_dir.as_str())
        );
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
