use std::path::PathBuf;

use bevy::ecs::system::Commands;
use bevy::ecs::system::SystemParam;
use bevy::prelude::Changed;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ecs::page::PageReady;
use vmux_ecs::{
    Cwd, JobRequest, JobResult, Operation, OperationError, OperationPlugin, OperationState,
    OperationTarget, OperationTargetRevision, PageMetadata,
};
use vmux_git::state::{GitPageContext, GitWorkspaceChanged};

use crate::presentation::Retiring;
use crate::tab::{
    Tab, TabDirDecided, TabHierarchy, TabWorkspace, TabWorktree, TabWorktreeUnavailable,
};
use crate::worktree::{
    CancelCheckout, CheckoutFailed, CheckoutPlugin, CheckoutPrepared, CheckoutTargetRevision,
    CommitCheckout, ManagedWorktreeRoot, PreparedCheckout, SelectBranchCheckout, TabWorktreeReady,
};

pub struct PageContextPlugin;

impl Plugin for PageContextPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        if !app.is_plugin_added::<CheckoutPlugin>() {
            app.add_plugins(CheckoutPlugin);
        }
        app.init_resource::<ProjectActivations>()
            .init_resource::<ManagedWorktreeRoot>()
            .add_plugins(UiEventPlugin::<(ProjectActivateRequest,)>::default())
            .add_observer(git_page_ready)
            .add_observer(git_page_activated)
            .add_observer(project_activate)
            .add_observer(selection_changed)
            .add_systems(Update, sync_git_pages)
            .add_systems(Update, bootstrap_git_pages)
            .add_systems(
                Update,
                (
                    (
                        retire,
                        canonical_project,
                        validate_project,
                        choose_checkout,
                        canonical_checkout,
                        checkout_info,
                    )
                        .chain(),
                    (
                        source_info,
                        head,
                        git_directory,
                        project_root,
                        configured_project_root,
                        canonical_project_root,
                    )
                        .chain(),
                    (
                        execution_directory,
                        base_remote,
                        base_candidate,
                        base_configuration,
                        base_verify,
                    )
                        .chain(),
                    (
                        selection_ready,
                        checkout_prepared,
                        apply_selection,
                        commit_selection,
                        finish_selection,
                        cancel_drained,
                    )
                        .chain(),
                )
                    .chain()
                    .after(crate::stack::ComputeFocusSet),
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

#[derive(Component, Clone)]
struct ResolvedWorkspace {
    startup_dir: String,
    project_dir: String,
    branch: String,
    worktree: Option<(TabWorktree, TabWorktreeReady)>,
}

#[derive(Component)]
struct ProjectActivation {
    webview: Entity,
    branch: String,
    checkout: String,
}

#[derive(Component)]
struct ResolutionWork(Entity);
#[derive(Component)]
struct CheckoutWork(Entity);
#[derive(Component)]
struct TargetDirectory(PathBuf);
#[derive(Component)]
struct TargetCheckout(vmux_git::worktree::CheckoutInfo);
#[derive(Component)]
struct RelativeProject(PathBuf);
#[derive(Component)]
struct MainProject(PathBuf);
#[derive(Component)]
struct RepositoryRoot(PathBuf);
#[derive(Component)]
struct GitDirectory(PathBuf);
#[derive(Component)]
struct ObservedBranch(String);
#[derive(Component)]
struct DefaultBase(String);
#[derive(Component)]
struct ConfiguredBase(String);
#[derive(Component)]
struct BaseAttempt(usize);
#[derive(Component)]
struct ResolvedBase(String);
#[derive(Component)]
struct ExecutionDirectory(PathBuf);
#[derive(Component)]
struct ResolutionError(String);
#[derive(Component)]
struct ErrorObserved;
#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct ResolutionProgress(ResolvePhase);

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResolvePhase {
    CanonicalProject,
    ValidateProject,
    ChooseCheckout,
    CanonicalCheckout,
    CheckoutInfo,
    SourceInfo,
    Head,
    GitDirectory,
    ProjectRoot,
    ConfiguredProjectRoot,
    CanonicalProjectRoot,
    ExecutionDirectory,
    DefaultBase,
    BaseCandidate,
    BaseConfiguration,
    BaseVerify,
    Ready,
    AwaitCheckout,
    Applied,
    Committing,
    Failed,
    CancelDrain,
}

#[derive(bevy::ecs::system::SystemParam)]
struct ResolutionEffects<'w, 's> {
    commands: Commands<'w, 's>,
}

impl ResolutionEffects<'_, '_> {
    fn request<J: vmux_ecs::Job>(&mut self, operation: Entity, request: J) -> Entity {
        self.commands
            .spawn((
                Name::new("Workspace resolution IO"),
                Operation::bundle("workspace.resolve.io").expect("fixed resolution IO kind"),
                JobRequest(request),
                ChildOf(operation),
            ))
            .id()
    }

    fn finish(&mut self, operation: Entity, job: Entity, phase: ResolvePhase) {
        self.commands.entity(job).try_despawn();
        self.commands
            .entity(operation)
            .remove::<ResolutionWork>()
            .insert(ResolutionProgress(phase));
    }

    fn fail(&mut self, operation: Entity, job: Option<Entity>, error: String) {
        if let Some(job) = job {
            self.commands.entity(job).try_despawn();
        }
        self.commands
            .entity(operation)
            .remove::<ResolutionWork>()
            .insert((
                ResolutionError(error),
                ResolutionProgress(ResolvePhase::Failed),
            ));
    }
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

impl ProjectActivations {
    fn next_revision(&mut self, after: u64) -> u64 {
        self.next_revision = self.next_revision.max(after).wrapping_add(1).max(1);
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
        selection: ResolvedWorkspace,
        tab_entity: Entity,
    ) -> Result<(String, String), String> {
        let ResolvedWorkspace {
            startup_dir,
            project_dir,
            branch,
            worktree,
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
        Ok((startup_dir, branch))
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
    mut activations: ResMut<ProjectActivations>,
    target_revisions: Query<&CheckoutTargetRevision>,
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
    let revision = activations.next_revision(
        target_revisions
            .get(tab_entity)
            .map_or(0, |revision| revision.0),
    );
    context
        .commands
        .entity(tab_entity)
        .insert(PendingWorkspaceSelection::new(
            path.trim().to_string(),
            branch.clone(),
            revision,
        ));
    context
        .commands
        .entity(tab_entity)
        .insert(CheckoutTargetRevision(revision));
    context.commands.spawn((
        Name::new("Project workspace activation"),
        Operation::bundle("project.activate")
            .expect("fixed project activation kind")
            .running(),
        OperationTarget::new(tab_entity),
        OperationTargetRevision(revision),
        Cwd(PathBuf::from(path.trim())),
        ProjectActivation {
            webview,
            branch: branch.trim().to_string(),
            checkout: checkout.trim().to_string(),
        },
        ResolutionProgress(ResolvePhase::CanonicalProject),
    ));
}

fn selection_changed(
    changed: On<Insert, PendingWorkspaceSelection>,
    pending: Query<&PendingWorkspaceSelection>,
    operations: Query<
        (
            Entity,
            &OperationTarget,
            &OperationTargetRevision,
            Option<&ResolutionWork>,
            Option<&CheckoutWork>,
        ),
        With<ProjectActivation>,
    >,
    mut cancels: MessageWriter<CancelCheckout>,
    mut commands: Commands,
) {
    let Ok(current) = pending.get(changed.entity) else {
        return;
    };
    for (operation, target, revision, io, checkout) in &operations {
        if target.entity() != changed.entity || revision.0 == current.revision() {
            continue;
        }
        if let Some(checkout) = checkout {
            cancels.write(CancelCheckout {
                operation: checkout.0,
            });
        }
        if io.is_none() && checkout.is_none() {
            commands
                .entity(operation)
                .remove::<(ProjectActivation, ResolutionProgress)>()
                .insert(OperationState::Canceled);
        } else {
            commands
                .entity(operation)
                .insert(ResolutionProgress(ResolvePhase::CancelDrain));
        }
    }
}

fn retire(
    pending: Query<&PendingWorkspaceSelection>,
    tabs: Query<(), (With<Tab>, Without<Retiring>)>,
    operations: Query<
        (
            Entity,
            Option<&OperationTarget>,
            &OperationTargetRevision,
            &ResolutionProgress,
            Option<&CheckoutWork>,
        ),
        With<ProjectActivation>,
    >,
    mut cancels: MessageWriter<CancelCheckout>,
    mut commands: Commands,
) {
    for (operation, target, revision, progress, checkout) in &operations {
        if progress.0 == ResolvePhase::CancelDrain {
            continue;
        }
        if target.is_some_and(|target| {
            tabs.contains(target.entity())
                && pending
                    .get(target.entity())
                    .is_ok_and(|current| current.revision() == revision.0)
        }) {
            continue;
        }
        if let Some(checkout) = checkout {
            cancels.write(CancelCheckout {
                operation: checkout.0,
            });
        }
        commands
            .entity(operation)
            .insert(ResolutionProgress(ResolvePhase::CancelDrain));
    }
}

fn canonical_project(
    operations: Query<(Entity, &Cwd, &ResolutionProgress, Option<&ResolutionWork>)>,
    completed: Query<&JobResult<super::checkout::Canonicalize>>,
    mut effects: ResolutionEffects,
) {
    for (operation, cwd, progress, job) in &operations {
        if progress.0 != ResolvePhase::CanonicalProject {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(operation, super::checkout::Canonicalize(cwd.0.clone()));
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(path)) => {
                effects.commands.entity(operation).insert(Cwd(path.clone()));
                effects.finish(operation, job.0, ResolvePhase::ValidateProject);
            }
            Ok(Err(error)) | Err(error) => effects.fail(operation, Some(job.0), error.clone()),
        }
    }
}

fn validate_project(
    operations: Query<(Entity, &Cwd, &ResolutionProgress, Option<&ResolutionWork>)>,
    completed: Query<&JobResult<super::checkout::Inspect>>,
    mut effects: ResolutionEffects,
) {
    for (operation, cwd, progress, job) in &operations {
        if progress.0 != ResolvePhase::ValidateProject {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(operation, super::checkout::Inspect(cwd.0.clone()));
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(Some(metadata))) if metadata.is_dir() => {
                effects.finish(operation, job.0, ResolvePhase::ChooseCheckout)
            }
            Ok(Ok(_)) => effects.fail(
                operation,
                Some(job.0),
                "project path is not a directory".into(),
            ),
            Ok(Err(error)) | Err(error) => effects.fail(operation, Some(job.0), error.clone()),
        }
    }
}

fn choose_checkout(
    operations: Query<(
        Entity,
        &ProjectActivation,
        &Cwd,
        &OperationTarget,
        &OperationTargetRevision,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<super::checkout::Inspect>>,
    root: Res<ManagedWorktreeRoot>,
    mut requests: MessageWriter<SelectBranchCheckout>,
    mut effects: ResolutionEffects,
) {
    for (operation, request, project, target, revision, progress, job) in &operations {
        if progress.0 != ResolvePhase::ChooseCheckout {
            continue;
        }
        if !request.checkout.is_empty() {
            let Some(job) = job else {
                let task = effects.request(
                    operation,
                    super::checkout::Inspect(PathBuf::from(&request.checkout)),
                );
                effects
                    .commands
                    .entity(operation)
                    .insert(ResolutionWork(task));
                continue;
            };
            let Ok(result) = completed.get(job.0) else {
                continue;
            };
            if let Ok(Ok(Some(metadata))) = &result.0
                && (metadata.is_dir() || metadata.is_symlink())
            {
                effects
                    .commands
                    .entity(operation)
                    .insert(TargetDirectory(request.checkout.clone().into()));
                effects.finish(operation, job.0, ResolvePhase::CanonicalCheckout);
                continue;
            }
            effects.commands.entity(job.0).try_despawn();
            effects
                .commands
                .entity(operation)
                .remove::<ResolutionWork>();
        }
        if request.branch.is_empty() {
            effects.commands.entity(operation).insert((
                TargetDirectory(project.0.clone()),
                ResolutionProgress(ResolvePhase::CanonicalCheckout),
            ));
            continue;
        }
        let checkout = effects
            .commands
            .spawn((
                Name::new("Project branch acquisition"),
                Operation::bundle("project.checkout").expect("fixed project checkout kind"),
                OperationTarget::new(target.entity()),
                OperationTargetRevision(revision.0),
            ))
            .id();
        requests.write(SelectBranchCheckout {
            operation: checkout,
            directory: project.0.clone(),
            managed_root: root.0.clone(),
            branch: request.branch.clone(),
        });
        effects.commands.entity(operation).insert((
            CheckoutWork(checkout),
            ResolutionProgress(ResolvePhase::AwaitCheckout),
        ));
    }
}

fn canonical_checkout(
    operations: Query<(
        Entity,
        &TargetDirectory,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<super::checkout::Canonicalize>>,
    mut effects: ResolutionEffects,
) {
    for (operation, directory, progress, job) in &operations {
        if progress.0 != ResolvePhase::CanonicalCheckout {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                super::checkout::Canonicalize(directory.0.clone()),
            );
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(path)) => {
                effects
                    .commands
                    .entity(operation)
                    .insert(TargetDirectory(path.clone()));
                effects.finish(operation, job.0, ResolvePhase::CheckoutInfo);
            }
            Ok(Err(error)) | Err(error) => effects.fail(operation, Some(job.0), error.clone()),
        }
    }
}

fn checkout_info(
    operations: Query<(
        Entity,
        &TargetDirectory,
        &ProjectActivation,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<super::checkout::Identify>>,
    mut effects: ResolutionEffects,
) {
    for (operation, directory, request, progress, job) in &operations {
        if progress.0 != ResolvePhase::CheckoutInfo {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(operation, super::checkout::Identify(directory.0.clone()));
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(checkout)) => {
                effects
                    .commands
                    .entity(operation)
                    .insert(TargetCheckout(checkout.clone()));
                effects.finish(operation, job.0, ResolvePhase::SourceInfo);
            }
            _ if request.branch.is_empty() => {
                effects
                    .commands
                    .entity(operation)
                    .insert(ResolvedWorkspace {
                        startup_dir: directory.0.to_string_lossy().into_owned(),
                        project_dir: directory.0.to_string_lossy().into_owned(),
                        branch: String::new(),
                        worktree: None,
                    });
                effects.finish(operation, job.0, ResolvePhase::Ready);
            }
            _ => effects.fail(
                operation,
                Some(job.0),
                "branch checkout is no longer available".into(),
            ),
        }
    }
}

fn source_info(
    operations: Query<(
        Entity,
        &Cwd,
        &TargetCheckout,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<super::checkout::Identify>>,
    mut effects: ResolutionEffects,
) {
    for (operation, directory, checkout, progress, job) in &operations {
        if progress.0 != ResolvePhase::SourceInfo {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(operation, super::checkout::Identify(directory.0.clone()));
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(source)) if source.common_dir == checkout.0.common_dir => {
                let Ok(relative) = directory.0.strip_prefix(&source.root) else {
                    effects.fail(
                        operation,
                        Some(job.0),
                        "project directory is outside its checkout".into(),
                    );
                    continue;
                };
                effects
                    .commands
                    .entity(operation)
                    .insert(RelativeProject(relative.to_path_buf()));
                effects.finish(operation, job.0, ResolvePhase::Head);
            }
            Ok(Ok(_)) => effects.fail(
                operation,
                Some(job.0),
                "worktree belongs to a different repository".into(),
            ),
            Ok(Err(error)) | Err(error) => effects.fail(operation, Some(job.0), error.clone()),
        }
    }
}

fn head(
    operations: Query<(
        Entity,
        &TargetCheckout,
        &ProjectActivation,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: ResolutionEffects,
) {
    for (operation, checkout, request, progress, job) in &operations {
        if progress.0 != ResolvePhase::Head {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: checkout.0.root.clone(),
                    arguments: vec!["status".into(), "--porcelain=v2".into(), "--branch".into()],
                    guard: None,
                },
            );
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((output, _, true))) => {
                let mut branch = String::new();
                for line in output.lines() {
                    if let Some(value) = line.strip_prefix("# branch.head ")
                        && value != "(detached)"
                    {
                        branch = value.into();
                        break;
                    }
                }
                if !request.branch.is_empty() && request.branch != branch {
                    effects.fail(
                        operation,
                        Some(job.0),
                        format!(
                            "branch {} is no longer checked out at {}",
                            request.branch,
                            checkout.0.root.display()
                        ),
                    );
                    continue;
                }
                effects
                    .commands
                    .entity(operation)
                    .insert(ObservedBranch(branch));
                effects.finish(operation, job.0, ResolvePhase::GitDirectory);
            }
            Ok(Ok((_, error, _))) => effects.fail(operation, Some(job.0), error.trim().into()),
            Ok(Err(error)) => effects.fail(operation, Some(job.0), error.0.clone()),
            Err(error) => effects.fail(operation, Some(job.0), error.clone()),
        }
    }
}

fn git_directory(
    operations: Query<(
        Entity,
        &TargetCheckout,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: ResolutionEffects,
) {
    for (operation, checkout, progress, job) in &operations {
        if progress.0 != ResolvePhase::GitDirectory {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: checkout.0.root.clone(),
                    arguments: vec!["rev-parse".into(), "--absolute-git-dir".into()],
                    guard: None,
                },
            );
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((path, _, true))) => {
                effects
                    .commands
                    .entity(operation)
                    .insert(GitDirectory(PathBuf::from(
                        path.strip_suffix('\n').unwrap_or(path),
                    )));
                effects.finish(operation, job.0, ResolvePhase::ProjectRoot);
            }
            Ok(Ok((_, error, _))) => effects.fail(operation, Some(job.0), error.trim().into()),
            Ok(Err(error)) => effects.fail(operation, Some(job.0), error.0.clone()),
            Err(error) => effects.fail(operation, Some(job.0), error.clone()),
        }
    }
}

fn project_root(
    operations: Query<(
        Entity,
        &TargetCheckout,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: ResolutionEffects,
) {
    for (operation, checkout, progress, job) in &operations {
        if progress.0 != ResolvePhase::ProjectRoot {
            continue;
        }
        if checkout
            .0
            .common_dir
            .file_name()
            .is_some_and(|name| name == ".git")
        {
            let root = checkout.0.common_dir.parent().unwrap_or(&checkout.0.root);
            effects.commands.entity(operation).insert((
                MainProject(root.to_path_buf()),
                ResolutionProgress(ResolvePhase::CanonicalProjectRoot),
            ));
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: checkout.0.root.clone(),
                    arguments: vec![
                        "--git-dir".into(),
                        checkout.0.common_dir.to_string_lossy().into_owned(),
                        "config".into(),
                        "--path".into(),
                        "--get".into(),
                        "core.worktree".into(),
                    ],
                    guard: None,
                },
            );
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((root, _, true))) if !root.trim().is_empty() => {
                let mut root = PathBuf::from(root.strip_suffix('\n').unwrap_or(root));
                if !root.is_absolute() {
                    root = checkout.0.common_dir.join(root);
                }
                effects.commands.entity(operation).insert(MainProject(root));
                effects.finish(operation, job.0, ResolvePhase::ConfiguredProjectRoot);
            }
            _ => {
                effects
                    .commands
                    .entity(operation)
                    .insert(MainProject(checkout.0.root.clone()));
                effects.finish(operation, job.0, ResolvePhase::CanonicalProjectRoot);
            }
        }
    }
}

fn configured_project_root(
    operations: Query<(
        Entity,
        &MainProject,
        &TargetCheckout,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<super::checkout::Canonicalize>>,
    mut effects: ResolutionEffects,
) {
    for (operation, root, checkout, progress, job) in &operations {
        if progress.0 != ResolvePhase::ConfiguredProjectRoot {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(operation, super::checkout::Canonicalize(root.0.clone()));
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        let root = match &result.0 {
            Ok(Ok(root)) => root.clone(),
            _ => checkout.0.root.clone(),
        };
        effects.commands.entity(operation).insert(MainProject(root));
        effects.finish(operation, job.0, ResolvePhase::CanonicalProjectRoot);
    }
}

fn canonical_project_root(
    operations: Query<(
        Entity,
        &MainProject,
        &RelativeProject,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<super::checkout::Canonicalize>>,
    mut effects: ResolutionEffects,
) {
    for (operation, root, relative, progress, job) in &operations {
        if progress.0 != ResolvePhase::CanonicalProjectRoot {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                super::checkout::Canonicalize(root.0.join(&relative.0)),
            );
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(path)) => {
                effects
                    .commands
                    .entity(operation)
                    .insert((RepositoryRoot(root.0.clone()), MainProject(path.clone())));
                effects.finish(operation, job.0, ResolvePhase::ExecutionDirectory);
            }
            Ok(Err(error)) | Err(error) => effects.fail(operation, Some(job.0), error.clone()),
        }
    }
}

fn execution_directory(
    operations: Query<(
        Entity,
        &TargetCheckout,
        &GitDirectory,
        &RelativeProject,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<super::checkout::Canonicalize>>,
    mut effects: ResolutionEffects,
) {
    for (operation, checkout, git, relative, progress, job) in &operations {
        if progress.0 != ResolvePhase::ExecutionDirectory {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                super::checkout::Canonicalize(checkout.0.root.join(&relative.0)),
            );
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(path)) if path.starts_with(&checkout.0.root) => {
                effects
                    .commands
                    .entity(operation)
                    .insert(ExecutionDirectory(path.clone()));
                let next = if git.0 == checkout.0.common_dir {
                    ResolvePhase::Ready
                } else {
                    ResolvePhase::DefaultBase
                };
                effects.finish(operation, job.0, next);
            }
            Ok(Ok(_)) => effects.fail(
                operation,
                Some(job.0),
                "project directory escapes checkout".into(),
            ),
            Ok(Err(error)) | Err(error) => effects.fail(operation, Some(job.0), error.clone()),
        }
    }
}

fn selection_ready(
    operations: Query<
        (
            Entity,
            &TargetCheckout,
            &GitDirectory,
            &ExecutionDirectory,
            &MainProject,
            &RepositoryRoot,
            &ObservedBranch,
            Option<&ResolvedBase>,
            &ResolutionProgress,
        ),
        Without<ResolvedWorkspace>,
    >,
    mut effects: ResolutionEffects,
) {
    for (operation, checkout, git, directory, project, repository, branch, base, progress) in
        &operations
    {
        if progress.0 != ResolvePhase::Ready {
            continue;
        }
        let project = project.0.to_string_lossy().into_owned();
        let worktree = if git.0 != checkout.0.common_dir {
            let metadata = TabWorktree {
                repo_root: repository.0.to_string_lossy().into_owned(),
                checkout_dir: checkout.0.root.to_string_lossy().into_owned(),
                branch: branch.0.clone(),
                base_ref: base.map_or_else(String::new, |base| base.0.clone()),
            };
            let ready = match TabWorktreeReady::new(&directory.0, &project, &metadata, &checkout.0)
            {
                Ok(ready) => ready,
                Err(error) => {
                    effects.fail(operation, None, error);
                    continue;
                }
            };
            Some((metadata, ready))
        } else {
            None
        };
        effects
            .commands
            .entity(operation)
            .insert(ResolvedWorkspace {
                startup_dir: directory.0.to_string_lossy().into_owned(),
                project_dir: project,
                branch: branch.0.clone(),
                worktree,
            });
    }
}

fn checkout_prepared(
    mut prepared: MessageReader<CheckoutPrepared>,
    mut failed: MessageReader<CheckoutFailed>,
    operations: Query<(Entity, &CheckoutWork, &Cwd, &ResolutionProgress)>,
    checkouts: Query<&PreparedCheckout>,
    mut effects: ResolutionEffects,
) {
    for prepared in prepared.read() {
        let Ok(checkout) = checkouts.get(prepared.operation) else {
            continue;
        };
        for (operation, child, project, progress) in &operations {
            if child.0 != prepared.operation || progress.0 != ResolvePhase::AwaitCheckout {
                continue;
            }
            let activation = &checkout.activation;
            effects.commands.entity(operation).insert((
                ResolvedWorkspace {
                    startup_dir: activation.execution_dir.to_string_lossy().into_owned(),
                    project_dir: project.0.to_string_lossy().into_owned(),
                    branch: activation.metadata.branch.clone(),
                    worktree: Some((activation.metadata.clone(), activation.ready.clone())),
                },
                ResolutionProgress(ResolvePhase::Ready),
            ));
        }
    }
    for failure in failed.read() {
        for (operation, child, _, progress) in &operations {
            if child.0 == failure.operation
                && matches!(
                    progress.0,
                    ResolvePhase::AwaitCheckout | ResolvePhase::Committing
                )
            {
                effects.fail(operation, None, failure.error.clone());
            }
        }
    }
}

fn apply_selection(
    operations: Query<(
        Entity,
        &OperationTarget,
        &ResolvedWorkspace,
        &ResolutionProgress,
    )>,
    mut context: ProjectActivationContext,
) {
    for (operation, target, selection, progress) in &operations {
        if progress.0 != ResolvePhase::Ready {
            continue;
        }
        match context.apply(selection.clone(), target.entity()) {
            Ok(_) => {
                context
                    .commands
                    .entity(operation)
                    .insert(ResolutionProgress(ResolvePhase::Applied));
            }
            Err(error) => {
                context.commands.entity(operation).insert((
                    ResolutionError(error),
                    ResolutionProgress(ResolvePhase::Failed),
                ));
            }
        }
    }
}

fn commit_selection(
    operations: Query<(
        Entity,
        &OperationTarget,
        &OperationTargetRevision,
        Option<&CheckoutWork>,
        &ResolutionProgress,
    )>,
    mut commits: MessageWriter<CommitCheckout>,
    mut commands: Commands,
) {
    for (operation, target, revision, checkout, progress) in &operations {
        if progress.0 != ResolvePhase::Applied {
            continue;
        }
        if let Some(checkout) = checkout {
            commits.write(CommitCheckout {
                operation: checkout.0,
                target: target.entity(),
                revision: revision.0,
            });
            commands
                .entity(operation)
                .insert(ResolutionProgress(ResolvePhase::Committing));
        }
    }
}

fn finish_selection(
    operations: Query<(
        Entity,
        Option<&OperationTarget>,
        &ProjectActivation,
        &ResolutionProgress,
        Option<&CheckoutWork>,
        Option<&ResolvedWorkspace>,
        Option<&ResolutionError>,
        Has<ErrorObserved>,
    )>,
    states: Query<&OperationState>,
    mut context: ProjectActivationContext,
) {
    for (operation, target, activation, progress, checkout, selected, error, observed) in
        &operations
    {
        let result = match progress.0 {
            ResolvePhase::Applied if checkout.is_none() => selected
                .map(|selection| Ok((selection.startup_dir.clone(), selection.branch.clone()))),
            ResolvePhase::Committing => match checkout
                .and_then(|checkout| states.get(checkout.0).ok())
            {
                Some(OperationState::Succeeded) => selected
                    .map(|selection| Ok((selection.startup_dir.clone(), selection.branch.clone()))),
                Some(OperationState::Canceled) => {
                    context
                        .commands
                        .entity(operation)
                        .insert(ResolutionProgress(ResolvePhase::CancelDrain));
                    continue;
                }
                _ => None,
            },
            ResolvePhase::Failed if !observed => {
                context.commands.entity(operation).insert(ErrorObserved);
                continue;
            }
            ResolvePhase::Failed => {
                let error = error
                    .map(|error| error.0.trim())
                    .filter(|error| !error.is_empty())
                    .unwrap_or("Git workspace operation failed");
                Some(Err(error.to_string()))
            }
            _ => None,
        };
        let Some(result) = result else {
            continue;
        };
        let state = if result.is_ok() {
            OperationState::Succeeded
        } else {
            OperationState::Failed
        };
        context
            .commands
            .entity(operation)
            .insert(state)
            .remove::<(ProjectActivation, ResolutionProgress)>();
        if let Err(error) = &result
            && let Ok(error) = OperationError::new(error.clone())
        {
            context.commands.entity(operation).insert(error);
        }
        if let Some(target) = target {
            context
                .commands
                .entity(target.entity())
                .remove::<PendingWorkspaceSelection>();
        }
        context.publish(
            ProjectActivationOutcome::from_result(activation.branch.clone(), result),
            activation.webview,
            target.map(|target| target.entity()),
        );
    }
}

fn cancel_drained(
    operations: Query<(
        Entity,
        Option<&OperationTarget>,
        &OperationTargetRevision,
        &ResolutionProgress,
        Option<&ResolutionWork>,
        Option<&CheckoutWork>,
    )>,
    canonical: Query<&JobResult<super::checkout::Canonicalize>>,
    inspect: Query<&JobResult<super::checkout::Inspect>>,
    identity: Query<&JobResult<super::checkout::Identify>>,
    git: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    states: Query<&OperationState>,
    pending: Query<&PendingWorkspaceSelection>,
    mut commands: Commands,
) {
    for (operation, target, revision, progress, job, checkout) in &operations {
        if progress.0 != ResolvePhase::CancelDrain {
            continue;
        }
        if let Some(job) = job {
            if !canonical.contains(job.0)
                && !inspect.contains(job.0)
                && !identity.contains(job.0)
                && !git.contains(job.0)
            {
                continue;
            }
            commands.entity(job.0).try_despawn();
        }
        if let Some(checkout) = checkout
            && !states.get(checkout.0).is_ok_and(|state| {
                matches!(
                    state,
                    OperationState::Succeeded | OperationState::Failed | OperationState::Canceled
                )
            })
        {
            continue;
        }
        if let Some(target) = target
            && pending
                .get(target.entity())
                .is_ok_and(|current| current.revision() == revision.0)
        {
            commands
                .entity(target.entity())
                .remove::<PendingWorkspaceSelection>();
        }
        commands
            .entity(operation)
            .remove::<(ProjectActivation, ResolutionProgress, ResolutionWork)>()
            .insert(OperationState::Canceled);
    }
}

impl ResolvedBase {
    fn native(reference: &str) -> Self {
        let reference = reference
            .strip_prefix("refs/remotes/origin/")
            .or_else(|| reference.strip_prefix("refs/heads/"))
            .unwrap_or(reference);
        Self(reference.into())
    }
}

fn base_remote(
    operations: Query<(
        Entity,
        &TargetCheckout,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: ResolutionEffects,
) {
    for (operation, repository, progress, job) in &operations {
        if progress.0 != ResolvePhase::DefaultBase {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec!["symbolic-ref".into(), "refs/remotes/origin/HEAD".into()],
                    guard: None,
                },
            );
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((reference, _, true))) if !reference.trim().is_empty() => {
                effects
                    .commands
                    .entity(operation)
                    .insert(DefaultBase(reference.trim().to_string()));
                effects.finish(operation, job.0, ResolvePhase::BaseConfiguration);
            }
            _ => {
                effects.commands.entity(operation).insert(BaseAttempt(0));
                effects.finish(operation, job.0, ResolvePhase::BaseCandidate);
            }
        }
    }
}

fn base_candidate(
    operations: Query<(
        Entity,
        &TargetCheckout,
        &BaseAttempt,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: ResolutionEffects,
) {
    for (operation, repository, attempt, progress, job) in &operations {
        if progress.0 != ResolvePhase::BaseCandidate {
            continue;
        }
        let candidate = match attempt.0 {
            0 => "refs/remotes/origin/main",
            1 => "refs/remotes/origin/master",
            2 => "refs/heads/main",
            3 => "refs/heads/master",
            _ => {
                effects.commands.entity(operation).insert((
                    DefaultBase(String::new()),
                    ResolutionProgress(ResolvePhase::BaseConfiguration),
                ));
                continue;
            }
        };
        let Some(job) = job else {
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "rev-parse".into(),
                        "--verify".into(),
                        "--quiet".into(),
                        candidate.into(),
                    ],
                    guard: None,
                },
            );
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((_, _, true))) => {
                effects
                    .commands
                    .entity(operation)
                    .insert(DefaultBase(candidate.into()));
                effects.finish(operation, job.0, ResolvePhase::BaseConfiguration);
            }
            _ => {
                effects
                    .commands
                    .entity(operation)
                    .insert(BaseAttempt(attempt.0 + 1));
                effects.finish(operation, job.0, ResolvePhase::BaseCandidate);
            }
        }
    }
}

fn base_configuration(
    operations: Query<(
        Entity,
        &TargetCheckout,
        &DefaultBase,
        Option<&ObservedBranch>,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: ResolutionEffects,
) {
    for (operation, repository, default, branch, progress, job) in &operations {
        if progress.0 != ResolvePhase::BaseConfiguration {
            continue;
        }
        let Some(branch) = branch else {
            effects.commands.entity(operation).insert((
                ResolvedBase::native(&default.0),
                ResolutionProgress(ResolvePhase::Ready),
            ));
            continue;
        };
        if branch.0.is_empty() {
            effects.commands.entity(operation).insert((
                ResolvedBase::native(&default.0),
                ResolutionProgress(ResolvePhase::Ready),
            ));
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "config".into(),
                        "--get".into(),
                        format!("branch.{}.vmux-base", branch.0),
                    ],
                    guard: None,
                },
            );
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((reference, _, true))) if !reference.trim().is_empty() => {
                effects
                    .commands
                    .entity(operation)
                    .insert(ConfiguredBase(reference.trim().to_string()));
                effects.finish(operation, job.0, ResolvePhase::BaseVerify);
            }
            _ => {
                effects
                    .commands
                    .entity(operation)
                    .insert(ResolvedBase::native(&default.0));
                effects.finish(operation, job.0, ResolvePhase::Ready);
            }
        }
    }
}

fn base_verify(
    operations: Query<(
        Entity,
        &TargetCheckout,
        &DefaultBase,
        &ConfiguredBase,
        &ResolutionProgress,
        Option<&ResolutionWork>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: ResolutionEffects,
) {
    for (operation, repository, default, configured, progress, job) in &operations {
        if progress.0 != ResolvePhase::BaseVerify {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "rev-parse".into(),
                        "--verify".into(),
                        "--quiet".into(),
                        configured.0.clone(),
                    ],
                    guard: None,
                },
            );
            effects
                .commands
                .entity(operation)
                .insert(ResolutionWork(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        let reference = match &result.0 {
            Ok(Ok((_, _, true))) => &configured.0,
            _ => &default.0,
        };
        effects
            .commands
            .entity(operation)
            .insert(ResolvedBase::native(reference));
        effects.finish(operation, job.0, ResolvePhase::Ready);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

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

    struct WorkspaceProbe {
        app: App,
        tab: Entity,
        webview: Entity,
    }

    impl WorkspaceProbe {
        fn new(root: &std::path::Path) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, PageContextPlugin))
                .insert_resource(ManagedWorktreeRoot(root.to_path_buf()));
            let tab = app.world_mut().spawn(Tab::default()).id();
            let webview = app.world_mut().spawn(ChildOf(tab)).id();
            Self { app, tab, webview }
        }

        fn request(&mut self, path: &std::path::Path, branch: &str, checkout: &str) {
            self.app.world_mut().trigger(UiInput {
                webview: self.webview,
                payload: ProjectActivateRequest {
                    path: path.to_string_lossy().into_owned(),
                    branch: branch.into(),
                    checkout: checkout.into(),
                    pane_id: None,
                },
            });
            self.app.world_mut().flush();
        }

        fn settle(&mut self) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self
                .app
                .world()
                .get::<PendingWorkspaceSelection>(self.tab)
                .is_some()
            {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "workspace activation did not finish"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
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
    fn removing_the_selection_or_its_tab_cancels_the_stale_activation() {
        for remove_tab in [false, true] {
            let managed = tempfile::tempdir().unwrap();
            let project = tempfile::tempdir().unwrap();
            let mut probe = WorkspaceProbe::new(managed.path());
            probe.request(project.path(), "", "");
            if remove_tab {
                probe.app.world_mut().despawn(probe.tab);
            } else {
                probe
                    .app
                    .world_mut()
                    .entity_mut(probe.tab)
                    .remove::<PendingWorkspaceSelection>();
            }
            probe.app.update();
            let mut states = probe
                .app
                .world_mut()
                .query_filtered::<&OperationState, With<OperationTargetRevision>>();
            assert!(
                states
                    .iter(probe.app.world())
                    .all(|state| *state == OperationState::Canceled)
            );
            assert!(probe.app.world().get::<TabWorkspace>(probe.tab).is_none());
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
        let managed = tempfile::tempdir().unwrap();
        let hook = repository.path().join(".git/hooks/post-checkout");
        std::fs::write(&hook, "#!/bin/sh\ncommon=$(git rev-parse --path-format=absolute --git-common-dir) || exit 1\ntouch \"$common/started\"\nwhile [ ! -f \"$common/release\" ]; do sleep 0.01; done\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut probe = WorkspaceProbe::new(managed.path());
        probe.request(repository.path(), "feature", "");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !repository.path().join(".git/started").exists() {
            probe.app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let checkout = source
            .worktree_registrations()
            .unwrap()
            .into_iter()
            .find(|row| row.branch.as_deref() == Some("feature"))
            .unwrap()
            .path;
        let world = probe.app.world_mut();
        let mut work = world.query::<&CheckoutWork>();
        let operation = work.single(world).unwrap().0;
        probe.app.world_mut().despawn(probe.tab);
        std::fs::write(repository.path().join(".git/release"), []).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while probe.app.world().get::<OperationState>(operation) != Some(&OperationState::Canceled)
        {
            probe.app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(!checkout.exists());
        assert!(
            source
                .worktree_registrations()
                .unwrap()
                .iter()
                .all(|row| row.branch.as_deref() != Some("feature"))
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

        let mut probe = WorkspaceProbe::new(managed_root.path());
        probe.request(
            &nested,
            "feature",
            checkout.join("crates/app").to_str().unwrap(),
        );
        probe.settle();
        let selection = probe.app.world().get::<TabWorkspace>(probe.tab).unwrap();
        let metadata = probe.app.world().get::<TabWorktree>(probe.tab).unwrap();
        let startup = probe
            .app
            .world()
            .get::<Tab>(probe.tab)
            .unwrap()
            .startup_dir
            .as_deref()
            .unwrap();
        let expected_project = nested.canonicalize().unwrap();
        let expected_checkout = checkout.join("crates/app").canonicalize().unwrap();
        assert_eq!(selection.project_dir, expected_project.to_string_lossy());
        assert_eq!(startup, expected_checkout.to_string_lossy());
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

        let mut probe = WorkspaceProbe::new(managed_root.path());
        probe.request(repository.path(), "other", checkout.to_str().unwrap());
        probe.settle();
        let mut errors = probe.app.world_mut().query::<&OperationError>();
        let error = errors.iter(probe.app.world()).next().unwrap().message();
        assert!(error.contains("no longer checked out"));
    }
}
