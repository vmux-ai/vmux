use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ecs::page::PageReady;
use vmux_ecs::{
    Operation, OperationError, OperationPlugin, OperationState, OperationTarget,
    OperationTargetRevision, PageMetadata,
};
use vmux_git::state::{GitPageContext, GitWorkspaceChanged};

use crate::presentation::Retiring;
use crate::tab::{
    Tab, TabDirDecided, TabHierarchy, TabWorkspace, TabWorktree, TabWorktreeUnavailable,
};
use crate::worktree::{
    ManagedCheckoutCleanup, ManagedCheckoutWait, ManagedWorktreeRoot, TabWorktreeCheckoutResult,
    TabWorktreeReady,
};

pub struct PageContextPlugin;

impl Plugin for PageContextPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.init_resource::<vmux_git::GitRuntime>()
            .init_resource::<ProjectActivations>()
            .add_plugins(UiEventPlugin::<(ProjectActivateRequest,)>::default())
            .add_observer(git_page_ready)
            .add_observer(git_page_activated)
            .add_observer(project_activate)
            .add_systems(Update, sync_git_pages)
            .add_systems(Update, bootstrap_git_pages)
            .add_systems(
                Update,
                poll_project_activations.after(crate::stack::ComputeFocusSet),
            );
    }
}

#[derive(SystemParam)]
pub struct Context<'w, 's> {
    tabs: TabHierarchy<'w, 's>,
    hierarchy: crate::space::SpaceHierarchy<'w, 's>,
    spaces: Query<'w, 's, &'static crate::space::EffectiveStartupDir>,
    workspaces: Query<'w, 's, (Option<&'static TabWorkspace>, Option<&'static TabWorktree>)>,
}

impl Context<'_, '_> {
    pub fn tab(&self, entity: Entity) -> Option<Entity> {
        self.tabs.entity(entity)
    }

    pub fn directory(&self, entity: Entity) -> Option<PathBuf> {
        let startup = self
            .tabs
            .startup_dir(entity)
            .filter(|directory| !directory.is_empty());
        if let Some(directory) = startup.as_ref()
            && directory != "/"
        {
            return Some(PathBuf::from(directory));
        }
        if let Some(tab) = self.tabs.entity(entity)
            && let Ok((workspace, worktree)) = self.workspaces.get(tab)
        {
            if let Some(worktree) = worktree
                && !worktree.checkout_dir.is_empty()
            {
                return Some(PathBuf::from(&worktree.checkout_dir));
            }
            if let Some(workspace) = workspace
                && !workspace.project_dir.is_empty()
            {
                return Some(PathBuf::from(&workspace.project_dir));
            }
        }
        self.hierarchy
            .get(entity)
            .and_then(|space| self.spaces.get(space).ok())
            .and_then(|directory| directory.0.clone())
            .or_else(|| startup.map(PathBuf::from))
            .or_else(|| std::env::current_dir().ok())
    }

    pub fn project_directory(&self, entity: Entity) -> Option<PathBuf> {
        if let Some(tab) = self.tabs.entity(entity)
            && let Ok((workspace, worktree)) = self.workspaces.get(tab)
        {
            if let Some(workspace) = workspace
                && !workspace.project_dir.is_empty()
            {
                return Some(PathBuf::from(&workspace.project_dir));
            }
            if let Some(worktree) = worktree
                && !worktree.repo_root.is_empty()
            {
                return Some(PathBuf::from(&worktree.repo_root));
            }
        }
        None
    }
}

struct TabWorkspaceSelection {
    startup_dir: String,
    project_dir: String,
    branch: String,
    worktree: Option<(TabWorktree, TabWorktreeReady)>,
    cleanup: Option<ManagedCheckoutCleanup>,
}

#[derive(Component)]
struct ProjectActivation {
    webview: Entity,
    branch: String,
    request: ProjectActivationRequest,
    work: ProjectActivationWork,
}

#[derive(Clone)]
struct ProjectActivationRequest {
    path: String,
    branch: String,
    checkout: String,
    managed_root: PathBuf,
}

enum TabWorkspaceResolution {
    Ready(Box<TabWorkspaceSelection>),
    Waiting(ManagedCheckoutWait),
}

impl TabWorkspaceResolution {
    fn ready(selection: TabWorkspaceSelection) -> Self {
        Self::Ready(Box::new(selection))
    }
}

enum ProjectActivationWork {
    Running(vmux_git::GitTask<Result<TabWorkspaceResolution, String>>),
    Waiting(ManagedCheckoutWait),
    Rejected {
        observed: bool,
        error: String,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    },
}

enum ProjectActivationPoll {
    Pending,
    Ready(std::thread::Result<Result<TabWorkspaceResolution, String>>),
    Retry,
    Rejected(String),
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PendingWorkspaceSelection {
    project_dir: String,
    branch: String,
    revision: u64,
}

impl PendingWorkspaceSelection {
    pub fn new(project_dir: String, branch: String, revision: u64) -> Self {
        Self {
            project_dir: Self::normalize_project_dir(&project_dir),
            branch: branch.trim().to_string(),
            revision,
        }
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn project_dir(&self) -> &str {
        &self.project_dir
    }

    pub fn matches_project_dir(&self, project_dir: &str) -> bool {
        self.project_dir == Self::normalize_project_dir(project_dir)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn normalize_project_dir(project_dir: &str) -> String {
        let project_dir = project_dir.trim();
        if project_dir.is_empty() {
            return String::new();
        }
        vmux_path::PathIdentity::lexical(project_dir)
            .into_path_buf()
            .to_string_lossy()
            .into_owned()
    }
}

#[derive(Resource, Default)]
struct ProjectActivations {
    next_revision: u64,
}

struct ProjectActivationInstall {
    tab: Entity,
    revision: u64,
    activation: ProjectActivation,
}

impl Command for ProjectActivationInstall {
    type Out = ();

    fn apply(self, world: &mut World) {
        let tab = self.tab;
        let revision = self.revision;
        let mut activation = self.activation;
        let mut operations = world.query::<(Entity, &OperationTarget, &mut ProjectActivation)>();
        let mut canceled = Vec::new();
        for (entity, target, mut activation) in operations.iter_mut(world) {
            if target.entity() == tab {
                activation.cancel();
                canceled.push(entity);
            }
        }
        for entity in canceled {
            world
                .entity_mut(entity)
                .remove::<ProjectActivation>()
                .insert(OperationState::Canceled);
        }
        let operation = world.spawn((
            Operation::bundle("project.activate").unwrap(),
            OperationTargetRevision(revision),
        ));
        let entity = operation.id();
        if world.get::<Tab>(tab).is_none() {
            activation.cancel();
            world.entity_mut(entity).insert(OperationState::Canceled);
            return;
        }
        world.entity_mut(entity).insert((
            OperationState::Running,
            OperationTarget::new(tab),
            activation,
        ));
    }
}

impl ProjectActivation {
    fn poll(&mut self) -> ProjectActivationPoll {
        match &mut self.work {
            ProjectActivationWork::Running(task) => task
                .poll()
                .map(ProjectActivationPoll::Ready)
                .unwrap_or(ProjectActivationPoll::Pending),
            ProjectActivationWork::Waiting(wait) => {
                if wait.is_ready() {
                    ProjectActivationPoll::Retry
                } else {
                    ProjectActivationPoll::Pending
                }
            }
            ProjectActivationWork::Rejected {
                observed,
                error,
                wake,
            } => {
                if !*observed {
                    *observed = true;
                    if let Some(wake) = wake {
                        let _ = wake.send_event(WinitUserEvent::WakeUp);
                    }
                    return ProjectActivationPoll::Pending;
                }
                ProjectActivationPoll::Rejected(error.clone())
            }
        }
    }

    fn cancel(&mut self) {
        if let ProjectActivationWork::Running(task) = &mut self.work {
            task.cancel();
        }
    }

    fn wait(
        &mut self,
        wait: ManagedCheckoutWait,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) {
        wait.subscribe(wake);
        self.work = ProjectActivationWork::Waiting(wait);
    }

    fn retry(
        &mut self,
        runtime: &vmux_git::GitRuntime,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) {
        self.work = self.request.start(runtime, wake);
    }
}

impl ProjectActivationRequest {
    fn start(
        &self,
        runtime: &vmux_git::GitRuntime,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> ProjectActivationWork {
        let request = self.clone();
        let worker_wake = wake.clone();
        let Some(task) = runtime.spawn_operation(worker_wake, move || request.resolve()) else {
            if let Some(wake) = wake.as_ref() {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            return ProjectActivationWork::Rejected {
                observed: false,
                error: "Git operation queue is busy".to_string(),
                wake,
            };
        };
        ProjectActivationWork::Running(task)
    }

    fn resolve(&self) -> Result<TabWorkspaceResolution, String> {
        TabWorkspaceSelection::resolve(&self.path, &self.branch, &self.checkout, &self.managed_root)
    }
}

impl ProjectActivations {
    fn next_revision(&mut self) -> u64 {
        self.next_revision = self.next_revision.wrapping_add(1).max(1);
        self.next_revision
    }
}

struct ProjectActivationOutcome {
    path: String,
    branch: String,
    error: String,
}

impl ProjectActivationOutcome {
    fn from_result(branch: String, result: Result<(String, String), String>) -> Self {
        match result {
            Ok((path, branch)) => Self {
                path,
                branch,
                error: String::new(),
            },
            Err(error) => Self {
                path: String::new(),
                branch,
                error,
            },
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct ProjectActivationContext<'w, 's> {
    tabs: Query<'w, 's, &'static mut Tab, Without<Retiring>>,
    child_of: Query<'w, 's, &'static ChildOf>,
    pages: Query<'w, 's, (Entity, &'static PageMetadata)>,
    commands: Commands<'w, 's>,
}

impl ProjectActivationContext<'_, '_> {
    fn publish(&mut self, outcome: ProjectActivationOutcome, webview: Entity, tab: Option<Entity>) {
        let event = GitWorkspaceChanged {
            path: outcome.path,
            branch: outcome.branch,
            error: outcome.error,
        };
        let Some(tab) = tab else {
            self.commands.trigger(
                vmux_ecs::UiStateWrite::<vmux_git::state::GitUiState>::from_event(webview, &event),
            );
            return;
        };
        for (page, metadata) in &self.pages {
            if !vmux_git::GitUrl::is_page(&metadata.url) {
                continue;
            }
            let mut current = page;
            loop {
                if current == tab {
                    self.commands.trigger(
                        vmux_ecs::UiStateWrite::<vmux_git::state::GitUiState>::from_event(
                            page, &event,
                        ),
                    );
                    break;
                }
                let Ok(parent) = self.child_of.get(current) else {
                    break;
                };
                current = parent.parent();
            }
        }
    }

    fn apply(
        &mut self,
        selection: TabWorkspaceSelection,
        tab_entity: Entity,
    ) -> Result<(String, String), String> {
        let TabWorkspaceSelection {
            startup_dir,
            project_dir,
            branch,
            worktree,
            cleanup,
        } = selection;
        let mut tab = self
            .tabs
            .get_mut(tab_entity)
            .map_err(|error| error.to_string())?;
        tab.startup_dir = Some(startup_dir.clone());
        if crate::worktree::WorktreeName::is_generated(&tab.name)
            && let Some(name) = std::path::Path::new(&project_dir)
                .file_name()
                .and_then(|name| name.to_str())
            && !name.is_empty()
        {
            tab.name = name.to_string();
        }
        let mut entity = self.commands.entity(tab_entity);
        entity.insert((TabWorkspace { project_dir }, TabDirDecided));
        match worktree {
            Some((metadata, ready)) => {
                entity
                    .insert((metadata, ready))
                    .remove::<TabWorktreeUnavailable>();
            }
            None => {
                entity.remove::<(TabWorktree, TabWorktreeReady, TabWorktreeUnavailable)>();
            }
        }
        if let Some(mut cleanup) = cleanup {
            let checkout_dir = cleanup.path().to_string_lossy().into_owned();
            let cleanup_branch = cleanup.branch().to_string();
            self.commands.queue(move |world: &mut World| {
                let applied = world
                    .get::<TabWorktree>(tab_entity)
                    .is_some_and(|worktree| {
                        worktree.checkout_dir == checkout_dir && worktree.branch == cleanup_branch
                    });
                if applied {
                    cleanup.commit();
                }
            });
        }
        Ok((startup_dir, branch))
    }
}

impl TabWorkspaceSelection {
    fn resolve(
        path: &str,
        branch: &str,
        checkout: &str,
        managed_root: &std::path::Path,
    ) -> Result<TabWorkspaceResolution, String> {
        let project_dir = std::path::Path::new(path.trim())
            .canonicalize()
            .map_err(|error| format!("invalid project directory: {error}"))?;
        if !project_dir.is_dir() {
            return Err("project path is not a directory".to_string());
        }
        if !checkout.trim().is_empty() {
            let checkout = std::path::Path::new(checkout);
            if checkout.is_dir() {
                return Self::from_checkout(
                    &project_dir,
                    checkout,
                    (!branch.trim().is_empty()).then_some(branch.trim()),
                )
                .map(TabWorkspaceResolution::ready);
            }
        }
        if !branch.trim().is_empty() {
            let activation = match crate::worktree::TabWorktreeActivation::checkout_branch(
                &project_dir,
                branch.trim(),
                managed_root,
            )? {
                TabWorktreeCheckoutResult::Ready(activation) => *activation,
                TabWorktreeCheckoutResult::Retry(wait) => {
                    return Ok(TabWorkspaceResolution::Waiting(wait));
                }
            };
            let (activation, cleanup) = activation.into_parts();
            return Ok(TabWorkspaceResolution::ready(Self {
                startup_dir: activation.execution_dir.to_string_lossy().into_owned(),
                project_dir: project_dir.to_string_lossy().into_owned(),
                branch: activation.metadata.branch.clone(),
                worktree: Some((activation.metadata, activation.ready)),
                cleanup,
            }));
        }
        Self::from_checkout(&project_dir, &project_dir, None).map(TabWorkspaceResolution::ready)
    }

    fn from_checkout(
        project_dir: &std::path::Path,
        checkout_dir: &std::path::Path,
        expected_branch: Option<&str>,
    ) -> Result<Self, String> {
        let project_dir = project_dir
            .canonicalize()
            .map_err(|error| format!("invalid project directory: {error}"))?;
        let checkout_dir = checkout_dir
            .canonicalize()
            .map_err(|error| format!("invalid checkout directory: {error}"))?;
        let Some(info) = vmux_git::worktree::RepoInfo::read(&checkout_dir) else {
            if expected_branch.is_some() {
                return Err("branch checkout is no longer available".to_string());
            }
            return Ok(Self {
                startup_dir: checkout_dir.to_string_lossy().into_owned(),
                project_dir: checkout_dir.to_string_lossy().into_owned(),
                branch: String::new(),
                worktree: None,
                cleanup: None,
            });
        };
        if let Some(expected_branch) = expected_branch
            && info.branch != expected_branch
        {
            return Err(format!(
                "branch {expected_branch} is no longer checked out at {}",
                checkout_dir.display()
            ));
        }
        let source = vmux_git::worktree::CheckoutInfo::try_from(project_dir.as_path())
            .map_err(|error| error.0)?;
        let relative_dir = project_dir
            .strip_prefix(&source.root)
            .map_err(|_| "project directory is outside its checkout".to_string())?;
        let project_dir = info
            .project_root()
            .join(relative_dir)
            .canonicalize()
            .map_err(|error| format!("invalid project directory: {error}"))?;
        let checkout = vmux_git::worktree::CheckoutInfo::try_from(checkout_dir.as_path())
            .map_err(|error| error.0)?;
        if source.common_dir != checkout.common_dir {
            return Err("worktree belongs to a different repository".to_string());
        }
        let execution_dir = checkout
            .root
            .join(relative_dir)
            .canonicalize()
            .map_err(|error| format!("project directory is missing from checkout: {error}"))?;
        if !execution_dir.is_dir() || !execution_dir.starts_with(&checkout.root) {
            return Err("project directory escapes checkout".to_string());
        }
        let project_dir_text = project_dir.to_string_lossy().into_owned();
        if !info.is_worktree {
            return Ok(Self {
                startup_dir: execution_dir.to_string_lossy().into_owned(),
                project_dir: project_dir_text,
                branch: info.branch,
                worktree: None,
                cleanup: None,
            });
        }
        let branch = info.branch.clone();
        let metadata = TabWorktree {
            repo_root: info.project_root().to_string_lossy().into_owned(),
            checkout_dir: checkout.root.to_string_lossy().into_owned(),
            branch: info.branch,
            base_ref: info.base_ref,
        };
        let ready = TabWorktreeReady::new(&execution_dir, &project_dir_text, &metadata, &checkout)?;
        Ok(Self {
            startup_dir: execution_dir.to_string_lossy().into_owned(),
            project_dir: project_dir_text,
            branch,
            worktree: Some((metadata, ready)),
            cleanup: None,
        })
    }
}

#[derive(SystemParam)]
struct GitPageContextWriter<'w, 's> {
    owners: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static crate::page_slot::Current>,
            Option<&'static crate::page_slot::Transition>,
        ),
        With<crate::stack::Stack>,
    >,
    workspace: Context<'w, 's>,
    spares: Query<'w, 's, (), With<crate::warm_page::WarmPageSpare>>,
    pages: Query<'w, 's, &'static PageMetadata>,
    commands: Commands<'w, 's>,
}

impl GitPageContextWriter<'_, '_> {
    fn context(&self, webview: Entity, allow_spare: bool) -> Option<GitPageContext> {
        let owner = self
            .owners
            .iter()
            .find_map(|(entity, current, transition)| {
                (current.is_some_and(|current| current.surface() == Some(webview))
                    || transition.is_some_and(|transition| transition.incoming() == webview))
                .then_some(entity)
            });
        if !allow_spare && self.spares.contains(webview) && owner.is_none() {
            return None;
        }
        if owner.is_none() && self.workspace.tab(webview).is_none() {
            return None;
        }
        let Ok(page) = self.pages.get(webview) else {
            return None;
        };
        if !vmux_git::GitUrl::is_page(&page.url) {
            return None;
        }
        let path = self.workspace.directory(owner.unwrap_or(webview))?;
        let path = path.to_string_lossy().to_string();
        Some(GitPageContext {
            working_directory: path,
            page_url: page.url.clone(),
        })
    }

    fn publish(&mut self, webview: Entity, allow_spare: bool) {
        let Some(context) = self.context(webview, allow_spare) else {
            return;
        };
        self.commands.trigger(
            vmux_ecs::UiStateWrite::<vmux_git::state::GitUiState>::from_event(webview, &context),
        );
    }
}

fn bootstrap_git_pages(
    pages: Query<
        (
            Entity,
            Option<&vmux_ecs::UiState<vmux_git::state::GitUiState>>,
        ),
        (With<crate::cef::Browser>, With<PageReady>),
    >,
    mut writer: GitPageContextWriter,
) {
    for (page, state) in &pages {
        let Some(expected) = writer.context(page, false) else {
            continue;
        };
        let current = state
            .and_then(|state| state.current())
            .and_then(|state| state.context.as_ref());
        if current.is_some_and(|current| {
            current.working_directory == expected.working_directory
                && current.page_url == expected.page_url
        }) {
            continue;
        }
        writer.publish(page, false);
    }
}

fn git_page_ready(trigger: On<UiInput<PageReady>>, mut writer: GitPageContextWriter) {
    writer.publish(trigger.event().webview, false);
}

fn git_page_activated(
    trigger: On<vmux_ecs::page::PageActivated>,
    mut writer: GitPageContextWriter,
) {
    writer.publish(trigger.event().webview(), true);
}

fn sync_git_pages(
    pages: Query<Entity, (With<crate::cef::Browser>, Changed<PageMetadata>)>,
    mut writer: GitPageContextWriter,
) {
    for webview in &pages {
        writer.publish(webview, false);
    }
}

fn project_activate(
    trigger: On<UiInput<ProjectActivateRequest>>,
    pane_entities: Query<Entity, With<crate::pane::Pane>>,
    scope: crate::window::WindowScope,
    managed_root: Res<ManagedWorktreeRoot>,
    runtime: Res<vmux_git::GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut activations: ResMut<ProjectActivations>,
    mut context: ProjectActivationContext,
) {
    let webview = trigger.event().webview;
    let ProjectActivateRequest {
        path,
        branch,
        checkout,
        pane_id,
    } = &trigger.event().payload;
    let mut current = match pane_id {
        Some(pane_id) => {
            let Some(pane) = pane_entities.iter().find(|entity| {
                entity.to_bits() == *pane_id && scope.contains_current_space(webview, *entity)
            }) else {
                context.publish(
                    ProjectActivationOutcome::from_result(
                        branch.clone(),
                        Err("target pane is unavailable".to_string()),
                    ),
                    webview,
                    None,
                );
                return;
            };
            pane
        }
        None => webview,
    };
    let tab_entity = loop {
        if context.tabs.contains(current) {
            break Some(current);
        }
        let Ok(parent) = context.child_of.get(current) else {
            break None;
        };
        current = parent.parent();
    };
    let Some(tab_entity) = tab_entity else {
        context.publish(
            ProjectActivationOutcome::from_result(
                branch.clone(),
                Err("tab workspace is unavailable".to_string()),
            ),
            webview,
            None,
        );
        return;
    };
    let revision = activations.next_revision();
    context
        .commands
        .entity(tab_entity)
        .insert(PendingWorkspaceSelection::new(
            path.trim().to_string(),
            branch.clone(),
            revision,
        ));
    let request = ProjectActivationRequest {
        path: path.clone(),
        branch: branch.clone(),
        checkout: checkout.clone(),
        managed_root: managed_root.0.clone(),
    };
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    let work = request.start(&runtime, wake);
    let activation = ProjectActivation {
        webview,
        branch: request.branch.clone(),
        request,
        work,
    };
    context.commands.queue(ProjectActivationInstall {
        tab: tab_entity,
        revision,
        activation,
    });
}

fn poll_project_activations(
    runtime: Res<vmux_git::GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    pending: Query<&PendingWorkspaceSelection>,
    mut operations: Query<(
        Entity,
        Option<&OperationTarget>,
        &OperationTargetRevision,
        &mut ProjectActivation,
    )>,
    mut context: ProjectActivationContext,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, target, revision, mut activation) in &mut operations {
        let Some(target) = target else {
            activation.cancel();
            context
                .commands
                .entity(entity)
                .remove::<ProjectActivation>()
                .insert(OperationState::Canceled);
            continue;
        };
        let tab = target.entity();
        let current_revision = pending
            .get(tab)
            .ok()
            .map(PendingWorkspaceSelection::revision);
        if !revision.accepts(current_revision) || !context.tabs.contains(tab) {
            activation.cancel();
            context
                .commands
                .entity(entity)
                .remove::<ProjectActivation>()
                .insert(OperationState::Canceled);
            continue;
        }
        let mut result = match activation.poll() {
            ProjectActivationPoll::Pending => continue,
            ProjectActivationPoll::Retry => {
                activation.retry(&runtime, wake.clone());
                continue;
            }
            ProjectActivationPoll::Ready(output) => match output {
                Ok(Ok(TabWorkspaceResolution::Ready(selection))) => context.apply(*selection, tab),
                Ok(Ok(TabWorkspaceResolution::Waiting(wait))) => {
                    activation.wait(wait, wake.clone());
                    continue;
                }
                Ok(Err(error)) => Err(error),
                Err(_) => Err("Git workspace worker panicked".to_string()),
            },
            ProjectActivationPoll::Rejected(error) => Err(error),
        };
        if let Err(error) = &mut result
            && error.trim().is_empty()
        {
            *error = "Git workspace operation failed".to_string();
        }
        let mut operation = context.commands.entity(entity);
        operation.remove::<ProjectActivation>();
        match &result {
            Ok(_) => {
                operation.insert(OperationState::Succeeded);
            }
            Err(error) => {
                operation.insert(OperationState::Failed);
                if let Ok(error) = OperationError::new(error.clone()) {
                    operation.insert(error);
                }
            }
        }
        context
            .commands
            .entity(tab)
            .remove::<PendingWorkspaceSelection>();
        context.publish(
            ProjectActivationOutcome::from_result(activation.branch.clone(), result),
            activation.webview,
            Some(tab),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct ContextWrites(Vec<GitPageContext>);

    #[derive(Resource, Default)]
    struct WorkspaceWrites(Vec<Entity>, Vec<String>);

    fn capture_context(
        trigger: On<vmux_ecs::UiStateWrite<vmux_git::state::GitUiState>>,
        mut writes: ResMut<ContextWrites>,
    ) {
        if let Some(context) = &trigger.event().update().context {
            writes.0.push(context.clone());
        }
    }

    fn capture_workspace(
        trigger: On<vmux_ecs::UiStateWrite<vmux_git::state::GitUiState>>,
        mut writes: ResMut<WorkspaceWrites>,
    ) {
        if let Some(workspace) = &trigger.event().update().workspace {
            writes.0.push(trigger.event().webview());
            writes.1.push(workspace.error.clone());
        }
    }

    struct GitPage;

    impl GitPage {
        fn spawn(app: &mut App, browser: bool) -> Entity {
            let tab = app
                .world_mut()
                .spawn(Tab {
                    startup_dir: Some("/tmp/repo".to_string()),
                    ..Default::default()
                })
                .id();
            let mut page = app.world_mut().spawn((
                PageMetadata {
                    url: "git://tmp/repo".to_string(),
                    ..Default::default()
                },
                ChildOf(tab),
            ));
            if browser {
                page.insert(crate::cef::Browser);
            }
            page.id()
        }
    }

    #[test]
    fn a_ready_git_surface_receives_context_from_its_slot_even_before_parent_binding() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ContextWrites>()
            .add_observer(capture_context)
            .add_systems(Update, bootstrap_git_pages);
        let tab = app
            .world_mut()
            .spawn(Tab {
                startup_dir: Some("/tmp/owning-checkout".to_string()),
                ..Default::default()
            })
            .id();
        let page = app
            .world_mut()
            .spawn((
                crate::cef::Browser,
                PageReady,
                PageMetadata {
                    url: "git://".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        app.world_mut()
            .spawn((crate::stack::Stack::bundle(), ChildOf(tab)))
            .insert(crate::page_slot::Current::new(
                Some(page),
                PageMetadata::default(),
            ));
        app.update();
        let writes = &app.world().resource::<ContextWrites>().0;
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].working_directory, "/tmp/owning-checkout");
        assert_eq!(writes[0].page_url, "git://");
    }

    #[test]
    fn git_page_ready_publishes_page_context() {
        let mut app = App::new();
        app.init_resource::<ContextWrites>()
            .add_observer(git_page_ready)
            .add_observer(capture_context);
        let webview = GitPage::spawn(&mut app, false);

        app.world_mut().trigger(UiInput {
            webview,
            payload: PageReady {},
        });
        app.update();

        let writes = &app.world().resource::<ContextWrites>().0;
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].page_url, "git://tmp/repo");
    }

    #[test]
    fn git_page_metadata_publishes_context_before_page_ready() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ContextWrites>()
            .add_observer(capture_context)
            .add_systems(Update, sync_git_pages);
        GitPage::spawn(&mut app, true);

        app.update();

        let writes = &app.world().resource::<ContextWrites>().0;
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].page_url, "git://tmp/repo");
    }

    #[test]
    fn activated_git_page_publishes_page_context() {
        let mut app = App::new();
        app.init_resource::<ContextWrites>()
            .add_observer(git_page_activated)
            .add_observer(capture_context);
        let webview = GitPage::spawn(&mut app, false);

        app.world_mut()
            .trigger(vmux_ecs::page::PageActivated::new(webview));
        app.update();

        let writes = &app.world().resource::<ContextWrites>().0;
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].page_url, "git://tmp/repo");
    }

    #[test]
    fn project_activation_updates_the_owning_tab_workspace() {
        let project = tempfile::tempdir().unwrap();
        let managed_root = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PageContextPlugin))
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()));
        let tab = app.world_mut().spawn(Tab::default()).id();
        let webview = app.world_mut().spawn(ChildOf(tab)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ProjectActivateRequest {
                path: project.path().to_string_lossy().into_owned(),
                branch: String::new(),
                checkout: String::new(),
                pane_id: None,
            },
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while app.world().get::<TabWorkspace>(tab).is_none() {
            app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }

        let expected = project.path().canonicalize().unwrap();
        let tab_state = app.world().get::<Tab>(tab).unwrap();
        let workspace = app.world().get::<TabWorkspace>(tab).unwrap();
        assert_eq!(
            tab_state.startup_dir.as_deref(),
            Some(expected.to_string_lossy().as_ref())
        );
        assert_eq!(
            workspace.project_dir,
            expected.to_string_lossy().into_owned()
        );
        assert!(app.world().get::<TabDirDecided>(tab).is_some());
    }

    #[test]
    fn branch_activation_switches_the_tab_to_the_selected_checkout() {
        let repository = tempfile::tempdir().unwrap();
        std::fs::write(repository.path().join("README.md"), "vmux\n").unwrap();
        let source = vmux_git::worktree::CheckoutInfo::initialize(repository.path()).unwrap();
        source.ensure_initial_snapshot("Initialize").unwrap();
        let checkout = repository.path().join(".worktrees/feature");
        source.add_worktree(&checkout, "feature", "HEAD").unwrap();
        let managed_root = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PageContextPlugin))
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()));
        let tab = app.world_mut().spawn(Tab::default()).id();
        let pane = app
            .world_mut()
            .spawn((crate::pane::Pane, ChildOf(tab)))
            .id();
        let webview = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ProjectActivateRequest {
                path: repository.path().to_string_lossy().into_owned(),
                branch: "feature".into(),
                checkout: checkout.to_string_lossy().into_owned(),
                pane_id: Some(pane.to_bits()),
            },
        });
        app.world_mut().flush();
        assert_eq!(
            app.world()
                .get::<PendingWorkspaceSelection>(tab)
                .map(PendingWorkspaceSelection::branch),
            Some("feature")
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.world().get::<TabWorktree>(tab).is_none() {
            app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        let expected = checkout.canonicalize().unwrap();
        let tab_state = app.world().get::<Tab>(tab).unwrap();
        let worktree = app.world().get::<TabWorktree>(tab).unwrap();
        assert_eq!(
            tab_state.startup_dir.as_deref(),
            Some(expected.to_string_lossy().as_ref())
        );
        assert_eq!(worktree.branch, "feature");
        assert!(app.world().get::<PendingWorkspaceSelection>(tab).is_none());
    }

    #[test]
    fn same_frame_branch_activations_keep_only_the_latest_selection() {
        let managed_root = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PageContextPlugin))
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()));
        let tab = app.world_mut().spawn(Tab::default()).id();
        let pane = app
            .world_mut()
            .spawn((crate::pane::Pane, ChildOf(tab)))
            .id();
        let webview = app.world_mut().spawn_empty().id();

        for branch in ["first", "second"] {
            app.world_mut().trigger(UiInput {
                webview,
                payload: ProjectActivateRequest {
                    path: "/no/such/vmux-project".into(),
                    branch: branch.into(),
                    checkout: String::new(),
                    pane_id: Some(pane.to_bits()),
                },
            });
        }
        app.world_mut().flush();

        let world = app.world_mut();
        let mut active = world.query::<(&ProjectActivation, &OperationTarget)>();
        let active = active.iter(world).collect::<Vec<_>>();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].0.branch, "second");
        assert_eq!(active[0].1.entity(), tab);
        let mut states = world.query::<&OperationState>();
        let states = states.iter(world).cloned().collect::<Vec<_>>();
        assert_eq!(states.len(), 2);
        assert!(states.contains(&OperationState::Canceled));
        assert!(states.contains(&OperationState::Running));
        assert_eq!(
            app.world()
                .get::<PendingWorkspaceSelection>(tab)
                .map(PendingWorkspaceSelection::branch),
            Some("second")
        );
    }

    #[test]
    fn pending_selection_matches_equivalent_project_paths() {
        let project = tempfile::tempdir().unwrap();
        let nested = project.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let equivalent = nested.join("..").join("nested");
        let pending = PendingWorkspaceSelection::new(
            nested.to_string_lossy().into_owned(),
            "feature".into(),
            1,
        );

        assert!(pending.matches_project_dir(equivalent.to_string_lossy().as_ref()));
    }

    #[test]
    fn failed_branch_activation_rolls_back_the_pending_selection() {
        let managed_root = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PageContextPlugin))
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()));
        let tab = app
            .world_mut()
            .spawn(Tab {
                startup_dir: Some("/before".into()),
                ..Default::default()
            })
            .id();
        let pane = app
            .world_mut()
            .spawn((crate::pane::Pane, ChildOf(tab)))
            .id();
        let webview = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ProjectActivateRequest {
                path: "/no/such/vmux-project".into(),
                branch: "feature".into(),
                checkout: String::new(),
                pane_id: Some(pane.to_bits()),
            },
        });
        app.world_mut().flush();
        assert_eq!(
            app.world()
                .get::<PendingWorkspaceSelection>(tab)
                .map(PendingWorkspaceSelection::branch),
            Some("feature")
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let world = app.world_mut();
            let mut operations = world.query::<&ProjectActivation>();
            let running = operations.iter(world).next().is_some();
            if !running {
                break;
            }
            app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }

        assert!(app.world().get::<PendingWorkspaceSelection>(tab).is_none());
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some("/before")
        );
    }

    #[test]
    fn rejected_activation_exposes_then_rolls_back_optimistic_branch() {
        for (error, expected) in [
            ("Git operation queue is busy", "Git operation queue is busy"),
            ("", "Git workspace operation failed"),
            (" \t", "Git workspace operation failed"),
        ] {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, PageContextPlugin))
                .init_resource::<WorkspaceWrites>()
                .add_observer(capture_workspace);
            let tab = app
                .world_mut()
                .spawn((
                    Tab::default(),
                    PendingWorkspaceSelection::new("/project".into(), "feature".into(), 1),
                ))
                .id();
            let webview = app
                .world_mut()
                .spawn((
                    PageMetadata {
                        url: "vmux://git/".into(),
                        ..Default::default()
                    },
                    ChildOf(tab),
                ))
                .id();
            let operation = app
                .world_mut()
                .spawn((
                    Operation,
                    OperationState::Running,
                    OperationTarget::new(tab),
                    OperationTargetRevision(1),
                    ProjectActivation {
                        webview,
                        branch: "feature".into(),
                        request: ProjectActivationRequest {
                            path: "/project".into(),
                            branch: "feature".into(),
                            checkout: String::new(),
                            managed_root: PathBuf::new(),
                        },
                        work: ProjectActivationWork::Rejected {
                            observed: false,
                            error: error.to_string(),
                            wake: None,
                        },
                    },
                ))
                .id();

            app.update();
            assert!(app.world().get::<PendingWorkspaceSelection>(tab).is_some());
            assert!(app.world().get::<ProjectActivation>(operation).is_some());

            app.update();
            assert!(app.world().get::<PendingWorkspaceSelection>(tab).is_none());
            assert!(app.world().get::<ProjectActivation>(operation).is_none());
            assert_eq!(
                app.world().get::<OperationState>(operation),
                Some(&OperationState::Failed)
            );
            assert_eq!(
                app.world()
                    .get::<OperationError>(operation)
                    .unwrap()
                    .message(),
                expected
            );
            assert_eq!(
                app.world().resource::<WorkspaceWrites>().1,
                vec![expected.to_string()]
            );
        }
    }

    #[test]
    fn removing_the_selection_or_its_tab_cancels_the_stale_activation() {
        for remove_tab in [false, true] {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, PageContextPlugin));
            let tab = app.world_mut().spawn(Tab::default()).id();
            let operation = app
                .world_mut()
                .spawn((
                    Operation,
                    OperationState::Running,
                    OperationTarget::new(tab),
                    OperationTargetRevision(1),
                    ProjectActivation {
                        webview: Entity::PLACEHOLDER,
                        branch: "feature".into(),
                        request: ProjectActivationRequest {
                            path: "/project".into(),
                            branch: "feature".into(),
                            checkout: String::new(),
                            managed_root: PathBuf::new(),
                        },
                        work: ProjectActivationWork::Rejected {
                            observed: false,
                            error: "cancelled".into(),
                            wake: None,
                        },
                    },
                ))
                .id();

            if remove_tab {
                app.world_mut().despawn(tab);
                assert!(app.world().get::<OperationTarget>(operation).is_none());
            }
            app.update();

            assert!(app.world().get::<ProjectActivation>(operation).is_none());
            assert!(app.world().get::<TabWorkspace>(tab).is_none());
            assert_eq!(
                app.world().get::<OperationState>(operation),
                Some(&OperationState::Canceled)
            );
        }
    }

    #[test]
    fn discarded_branch_selection_removes_its_orphan_checkout() {
        let repository = tempfile::tempdir().unwrap();
        std::fs::write(repository.path().join("README.md"), "vmux\n").unwrap();
        let source = vmux_git::worktree::CheckoutInfo::initialize(repository.path()).unwrap();
        source.ensure_initial_snapshot("Initialize").unwrap();
        assert!(
            std::process::Command::new("git")
                .current_dir(repository.path())
                .args(["branch", "feature"])
                .status()
                .unwrap()
                .success()
        );
        let managed_root = tempfile::tempdir().unwrap();

        let resolution = TabWorkspaceSelection::resolve(
            repository.path().to_string_lossy().as_ref(),
            "feature",
            "",
            managed_root.path(),
        )
        .unwrap();
        let TabWorkspaceResolution::Ready(selection) = resolution else {
            panic!("checkout unexpectedly waited for cleanup");
        };
        let checkout = PathBuf::from(&selection.startup_dir);
        assert!(checkout.is_dir());

        drop(selection);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while checkout.exists() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(!checkout.exists());
        assert!(
            source
                .worktree_registrations()
                .unwrap()
                .iter()
                .all(|registration| registration.branch.as_deref() != Some("feature"))
        );
    }

    #[test]
    fn project_activation_notifies_only_git_pages_in_the_tab() {
        let project = tempfile::tempdir().unwrap();
        let managed_root = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PageContextPlugin))
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()))
            .init_resource::<WorkspaceWrites>()
            .add_observer(capture_workspace);
        let tab = app.world_mut().spawn(Tab::default()).id();
        let requester = app
            .world_mut()
            .spawn((
                ChildOf(tab),
                PageMetadata {
                    url: "vmux://projects/".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        let git = app
            .world_mut()
            .spawn((
                ChildOf(tab),
                PageMetadata {
                    url: "git://tmp/repo".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        app.world_mut().spawn((
            ChildOf(tab),
            PageMetadata {
                url: "vmux://sessions/".to_string(),
                ..Default::default()
            },
        ));

        app.world_mut().trigger(UiInput {
            webview: requester,
            payload: ProjectActivateRequest {
                path: project.path().to_string_lossy().into_owned(),
                branch: String::new(),
                checkout: String::new(),
                pane_id: None,
            },
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while app.world().resource::<WorkspaceWrites>().0.is_empty() {
            app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }

        assert_eq!(app.world().resource::<WorkspaceWrites>().0, vec![git]);
    }

    #[test]
    fn branch_checkout_preserves_nested_project_directory() {
        let repository = tempfile::tempdir().unwrap();
        let nested = repository.path().join("crates/app");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("main.rs"), "fn main() {}\n").unwrap();
        let source = vmux_git::worktree::CheckoutInfo::initialize(repository.path()).unwrap();
        source.ensure_initial_snapshot("Initialize").unwrap();
        let worktrees = tempfile::tempdir().unwrap();
        let checkout = worktrees.path().join("feature");
        source.add_worktree(&checkout, "feature", "HEAD").unwrap();
        let managed_root = tempfile::tempdir().unwrap();

        let resolution = TabWorkspaceSelection::resolve(
            nested.to_string_lossy().as_ref(),
            "feature",
            checkout.join("crates/app").to_string_lossy().as_ref(),
            managed_root.path(),
        )
        .unwrap();
        let TabWorkspaceResolution::Ready(selection) = resolution else {
            panic!("checkout unexpectedly waited for cleanup");
        };

        let expected_project = nested.canonicalize().unwrap();
        let expected_checkout = checkout.join("crates/app").canonicalize().unwrap();
        let (metadata, _) = selection.worktree.unwrap();
        assert_eq!(selection.project_dir, expected_project.to_string_lossy());
        assert_eq!(selection.startup_dir, expected_checkout.to_string_lossy());
        assert_eq!(
            metadata.repo_root,
            repository.path().canonicalize().unwrap().to_string_lossy()
        );
        assert_eq!(
            metadata.checkout_dir,
            checkout.canonicalize().unwrap().to_string_lossy()
        );
        assert_eq!(metadata.branch, "feature");
    }

    #[test]
    fn branch_checkout_rejects_stale_checkout_ownership() {
        let repository = tempfile::tempdir().unwrap();
        std::fs::write(repository.path().join("README.md"), "vmux\n").unwrap();
        let source = vmux_git::worktree::CheckoutInfo::initialize(repository.path()).unwrap();
        source.ensure_initial_snapshot("Initialize").unwrap();
        let worktrees = tempfile::tempdir().unwrap();
        let checkout = worktrees.path().join("feature");
        source.add_worktree(&checkout, "feature", "HEAD").unwrap();
        let managed_root = tempfile::tempdir().unwrap();

        let error = TabWorkspaceSelection::resolve(
            repository.path().to_string_lossy().as_ref(),
            "other",
            checkout.to_string_lossy().as_ref(),
            managed_root.path(),
        )
        .err()
        .unwrap();

        assert!(error.contains("no longer checked out"));
    }
}
