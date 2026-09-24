use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use vmux_command::event::{CommandBarOpenEvent, CommandBarPromptContext, OpenId};
use vmux_command::open_target::OpenTarget;
use vmux_command::snapshot::{
    ClaimedUrl, CommandBarProjection, ContributedCommand, ContributedPage,
};
use vmux_core::KeyboardOwner;
use vmux_core::PageMetadata;
use vmux_ui::i18n::Locale;

use crate::START_PAGE_URL;
use crate::event::{StartDataRequest, StartSelectWorkspace};
use vmux_command::build_command_bar_open_payload;
use vmux_core::launcher::{HostsLauncher, InlineTransitionRequested};
use vmux_layout::settings::ResolvedLocale;
use vmux_layout::tab::{Tab, TabWorkspace, TabWorktree};
use vmux_layout::workspace_snapshot::{TabGatherParams, gather_command_bar_tabs};

pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::StartPage::plugin());
        app.world_mut().spawn((
            crate::PAGE_MANIFEST,
            vmux_core::host::page::NativelyHosted::page(START_PAGE_URL, "Start"),
        ));
        app.init_resource::<CommandBarProjection>()
            .add_message::<InlineTransitionRequested>()
            .add_systems(
                Update,
                (
                    mark_start_pages_as_launcher_hosts,
                    begin_requested_inline_transition,
                ),
            );
        app.add_plugins(UiEventPlugin::<(
            StartDataRequest,
            StartSelectWorkspace,
            vmux_api::command_bar::StartBranchesRequest,
            vmux_api::command_bar::StartGoToBranch,
        )>::default())
            .add_observer(on_start_data_request)
            .add_observer(on_start_select_workspace)
            .add_observer(on_start_branches_request)
            .add_observer(on_start_go_to_branch)
            .add_systems(
                Update,
                (
                    sync_live_start_pages,
                    drain_start_workspace_pickers,
                    drain_start_branch_reads,
                ),
            );
    }
}

#[derive(Component)]
struct StartWorkSynced;

#[derive(Component)]
struct PendingStartWorkspacePicker {
    tab: Entity,
    task: Task<Option<(std::path::PathBuf, bool)>>,
}

#[derive(SystemParam)]
struct StartPromptContextParams<'w, 's> {
    tabs: Query<
        'w,
        's,
        (
            Ref<'static, Tab>,
            Option<Ref<'static, TabWorkspace>>,
            Option<Ref<'static, TabWorktree>>,
        ),
    >,
    command_bar: Res<'w, CommandBarProjection>,
    proxy: Option<Res<'w, bevy::winit::EventLoopProxyWrapper>>,
    warmed_branches_for: Local<'s, String>,
}

impl StartPromptContextParams<'_, '_> {
    fn changed(&self, tab: Option<Entity>) -> bool {
        if self.command_bar.is_changed() {
            return true;
        }
        let Some(tab) = tab else {
            return false;
        };
        self.tabs.get(tab).is_ok_and(|(tab, workspace, worktree)| {
            tab.is_changed()
                || workspace.as_ref().is_some_and(Ref::is_changed)
                || worktree.as_ref().is_some_and(Ref::is_changed)
        })
    }

    fn cwd(&self, tab: Option<Entity>) -> String {
        let Some(tab) = tab else {
            return String::new();
        };
        let Ok((tab, workspace, _)) = self.tabs.get(tab) else {
            return String::new();
        };
        tab.startup_dir
            .clone()
            .or_else(|| {
                workspace
                    .as_ref()
                    .map(|workspace| workspace.project_dir.clone())
            })
            .unwrap_or_default()
    }

    fn context(
        &self,
        tab: Option<Entity>,
        info: Option<&vmux_git::worktree::RepoInfo>,
    ) -> CommandBarPromptContext {
        let Some(tab) = tab else {
            return CommandBarPromptContext::unrooted();
        };
        let Ok((_, _, worktree)) = self.tabs.get(tab) else {
            return CommandBarPromptContext::unrooted();
        };
        let cwd = self.cwd(Some(tab));
        if cwd.is_empty() {
            return CommandBarPromptContext::unrooted();
        }
        let path = std::path::Path::new(&cwd);
        let named = match info {
            Some(info) => info.project_name(),
            None => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| cwd.clone()),
        };
        CommandBarPromptContext {
            workspace_name: named,
            cwd,
            is_git_repo: info.is_some(),
            is_worktree: info.is_some_and(|info| info.is_worktree),
            branch: info.map(|info| info.branch.clone()).unwrap_or_default(),
            base_ref: worktree
                .as_ref()
                .map(|worktree| worktree.base_ref.clone())
                .unwrap_or_default(),
            uncommitted: info.map(|info| info.uncommitted).unwrap_or(0),
            ahead: info.map(|info| info.ahead).unwrap_or(0),
            projects: Vec::new(),
            ..CommandBarPromptContext::unrooted()
        }
    }
}

fn on_start_select_workspace(
    trigger: On<UiInput<StartSelectWorkspace>>,
    child_of: Query<&ChildOf>,
    tabs: Query<(), With<Tab>>,
    pending: Query<&PendingStartWorkspacePicker>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let mut current = trigger.event().webview;
    let tab = loop {
        if tabs.contains(current) {
            break Some(current);
        }
        let Ok(parent) = child_of.get(current) else {
            break None;
        };
        current = parent.parent();
    };
    let Some(tab) = tab else {
        return;
    };
    if pending.iter().any(|picker| picker.tab == tab) {
        return;
    }
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let projects_dir = vmux_core::profile::projects_dir();
    let initial_dir = std::fs::create_dir_all(&projects_dir)
        .ok()
        .map(|_| projects_dir)
        .filter(|path| path.is_dir())
        .or_else(|| {
            std::path::PathBuf::from(&trigger.event().payload.current_dir)
                .canonicalize()
                .ok()
                .filter(|path| path.is_dir())
        })
        .or_else(|| std::env::current_dir().ok().filter(|path| path.is_dir()))
        .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from))
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| std::path::PathBuf::from("/"));
    let task = IoTaskPool::get().spawn(async move {
        let selected = rfd::AsyncFileDialog::new()
            .set_title("Choose existing project")
            .set_directory(initial_dir)
            .pick_folder()
            .await
            .map(|handle| handle.path().to_path_buf());
        let result = if let Some(path) = selected {
            let initialize_git = if path.join(".git").exists() {
                false
            } else {
                matches!(
                    rfd::AsyncMessageDialog::new()
                        .set_title("Initialize Git repository?")
                        .set_description(
                            "This project is not a Git repository. Initialize Git now?",
                        )
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show()
                        .await,
                    rfd::MessageDialogResult::Yes
                )
            };
            Some((path, initialize_git))
        } else {
            None
        };
        if let Some(wake) = wake {
            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
        result
    });
    commands.spawn(PendingStartWorkspacePicker { tab, task });
}

fn drain_start_workspace_pickers(
    mut pending: Query<(Entity, &mut PendingStartWorkspacePicker)>,
    mut tabs: Query<&mut Tab>,
    mut commands: Commands,
) {
    for (entity, mut picker) in &mut pending {
        let Some(selected) = future::block_on(future::poll_once(&mut picker.task)) else {
            continue;
        };
        if let Some((path, initialize_git)) = selected
            && let Ok(path) = path.canonicalize()
            && path.is_dir()
        {
            if initialize_git {
                let _ = vmux_git::worktree::repository_init(&path);
            }
            ChosenProject { path }.apply(picker.tab, &mut tabs, &mut commands);
        }
        commands.entity(entity).despawn();
    }
}

#[derive(Component)]
struct StartBranchRead {
    webview: Entity,
    project: String,
    task: bevy::tasks::Task<Vec<vmux_api::space::ProjectBranch>>,
}

impl StartBranchRead {
    fn start(webview: Entity, project: &str, wake: vmux_core::host::wake::Wake) -> Option<Self> {
        let project = project.trim().to_string();
        if project.is_empty() {
            return None;
        }
        let root = std::path::PathBuf::from(&project);
        let task = IoTaskPool::get().spawn(async move {
            let _wake = wake;
            let mut branches = Vec::new();
            if let Ok(holders) = vmux_git::worktree::branch_holders(&root) {
                branches.reserve(holders.len());
                for holder in holders {
                    let checkout = holder.checkout_path();
                    let label = holder.checkout_label();
                    branches.push(vmux_api::space::ProjectBranch {
                        branch: holder.branch,
                        checkout,
                        label,
                        insertions: holder.change.insertions,
                        deletions: holder.change.deletions,
                    });
                }
            }
            branches
        });
        Some(Self {
            webview,
            project,
            task,
        })
    }
}

fn on_start_branches_request(
    trigger: On<UiInput<vmux_api::command_bar::StartBranchesRequest>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some(read) = StartBranchRead::start(
        trigger.event().webview,
        &trigger.event().payload.project,
        vmux_core::host::wake::Wake::from_resource(proxy),
    ) else {
        return;
    };
    commands.spawn(read);
}

fn drain_start_branch_reads(
    mut reads: Query<(Entity, &mut StartBranchRead)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, mut read) in &mut reads {
        let Some(branches) = future::block_on(future::poll_once(&mut read.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if !browsers.can_emit_to(&read.webview) {
            continue;
        }
        vmux_command::snapshot::CommandBarUiStateUpdates::write(
            &mut commands,
            read.webview,
            &vmux_api::command_bar::StartProjectBranches {
                project: read.project.clone(),
                branches,
            },
        );
    }
}

fn on_start_go_to_branch(
    trigger: On<UiInput<vmux_api::command_bar::StartGoToBranch>>,
    child_of: Query<&ChildOf>,
    tab_query: Query<(), With<Tab>>,
    mut tabs: Query<&mut Tab>,
    mut commands: Commands,
) {
    let mut current = trigger.event().webview;
    let tab = loop {
        if tab_query.contains(current) {
            break Some(current);
        }
        let Ok(parent) = child_of.get(current) else {
            break None;
        };
        current = parent.parent();
    };
    let Some(tab) = tab else {
        return;
    };
    let evt = &trigger.event().payload;
    let checkout = evt.checkout.trim();
    if !checkout.is_empty() {
        let Ok(path) = std::path::PathBuf::from(checkout).canonicalize() else {
            return;
        };
        ChosenProject { path }.apply(tab, &mut tabs, &mut commands);
        return;
    }
    let Ok(root) = std::path::PathBuf::from(&evt.project).canonicalize() else {
        return;
    };
    ChosenProject { path: root.clone() }.apply(tab, &mut tabs, &mut commands);
    if evt.branch.trim().is_empty() {
        return;
    }
    commands.entity(tab).insert(TabWorktree {
        repo_root: root.to_string_lossy().into_owned(),
        checkout_dir: String::new(),
        branch: evt.branch.clone(),
        base_ref: String::new(),
    });
}

struct ChosenProject {
    path: std::path::PathBuf,
}

impl ChosenProject {
    fn apply(&self, tab_entity: Entity, tabs: &mut Query<&mut Tab>, commands: &mut Commands) {
        let Ok(mut tab) = tabs.get_mut(tab_entity) else {
            return;
        };
        let dir = self.path.to_string_lossy().into_owned();
        tab.startup_dir = Some(dir.clone());
        if vmux_layout::worktree::is_generated_tab_name(&tab.name)
            && let Some(name) = self.path.file_name().and_then(|name| name.to_str())
            && !name.is_empty()
        {
            tab.name = name.to_string();
        }
        commands
            .entity(tab_entity)
            .insert((
                TabWorkspace { project_dir: dir },
                vmux_layout::tab::TabDirDecided,
            ))
            .remove::<(
                TabWorktree,
                vmux_layout::worktree::TabWorktreeReady,
                vmux_layout::tab::TabWorktreeUnavailable,
            )>();
    }
}

fn sync_live_start_pages(
    tab_gather: TabGatherParams,
    mut prompt_context: StartPromptContextParams,
    contributions: (
        Query<&ContributedPage>,
        Query<&ContributedCommand>,
        Query<
            (),
            Or<(
                Changed<ContributedPage>,
                Changed<ContributedCommand>,
                Changed<ClaimedUrl>,
            )>,
        >,
        RemovedComponents<ContributedPage>,
        RemovedComponents<ContributedCommand>,
        RemovedComponents<ClaimedUrl>,
    ),
    locale: Option<Res<ResolvedLocale>>,
    focused: Res<vmux_layout::stack::FocusedStack>,
    starts: Query<
        (
            Entity,
            &PageMetadata,
            Has<StartWorkSynced>,
            Has<KeyboardOwner>,
        ),
        Without<crate::StartInlineTransitionView>,
    >,
    added_keyboard_targets: Query<(), Added<KeyboardOwner>>,
    browsers: NonSend<Browsers>,
    mut repo_info: Option<ResMut<vmux_git::RepoInfoCache>>,
    mut last_git: Local<(String, Option<vmux_git::worktree::RepoInfo>)>,
    space_projects: vmux_space::SpaceProjects,
    definitions: Query<&vmux_command::CommandDefinition>,
    mut commands: Commands,
) {
    let (
        contributed_pages,
        contributed_commands,
        contribution_changes,
        mut removed_pages,
        mut removed_commands,
        mut removed_claims,
    ) = contributions;
    let cwd = prompt_context.cwd(tab_gather.active_tab.get());
    let git_info = (!cwd.is_empty())
        .then(|| {
            repo_info.as_mut().and_then(|cache| {
                cache
                    .bypass_change_detection()
                    .get(std::path::Path::new(&cwd))
            })
        })
        .flatten();
    let git_changed = last_git.0 != cwd || last_git.1 != git_info;
    let focus_changed = focused.is_changed();
    let contributions_changed = !contribution_changes.is_empty()
        || removed_pages.read().next().is_some()
        || removed_commands.read().next().is_some()
        || removed_claims.read().next().is_some();
    let changed = should_refresh_start_payload(
        prompt_context.command_bar.is_changed(),
        contributions_changed,
        focus_changed,
    ) || prompt_context.changed(tab_gather.active_tab.get())
        || git_changed
        || locale.as_ref().is_some_and(|locale| locale.is_changed());
    let locale = locale
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(Locale::preferred);
    let targets: Vec<(Entity, bool)> = starts
        .iter()
        .filter_map(|(e, meta, synced, keyboard_target)| {
            if !meta.url.starts_with(START_PAGE_URL) {
                return None;
            }
            if !browsers.can_emit_to(&e) {
                return None;
            }
            let focus_requested = should_focus_start_sync(
                synced,
                keyboard_target,
                added_keyboard_targets.contains(e),
                focus_changed,
            );
            (changed || !synced || focus_requested).then_some((e, focus_requested))
        })
        .collect();
    if targets.is_empty() {
        return;
    }
    if git_changed {
        *last_git = (cwd.clone(), git_info.clone());
    }
    let definitions = definitions.iter().cloned().collect::<Vec<_>>();
    let payload = build_start_payload(
        &tab_gather,
        &prompt_context.command_bar,
        &contributed_pages,
        &contributed_commands,
        &prompt_context,
        tab_gather.active_tab.get(),
        git_info.as_ref(),
        space_projects.rows(tab_gather.active_tab.get().unwrap_or(Entity::PLACEHOLDER)),
        prompt_context.command_bar.agent_models.agents.clone(),
        prompt_context.command_bar.agent_modes.agents.clone(),
        &locale,
        &definitions,
    );
    let project = vmux_ui::launcher::palette::ActiveProject::resolve(&payload.prompt_context);
    let warm_branches = !project.is_empty() && *prompt_context.warmed_branches_for != project;
    if warm_branches {
        *prompt_context.warmed_branches_for = project.clone();
    }
    for (e, focus_requested) in targets {
        if warm_branches
            && let Some(read) = StartBranchRead::start(
                e,
                &project,
                vmux_core::host::wake::Wake::beside(prompt_context.proxy.as_deref()),
            )
        {
            commands.spawn(read);
        }
        vmux_command::snapshot::CommandBarUiStateUpdates::write(&mut commands, e, &payload);
        if focus_requested {
            vmux_command::snapshot::CommandBarUiStateUpdates::write(
                &mut commands,
                e,
                &vmux_api::command_bar::CommandBarFocusInput,
            );
        }
        commands.entity(e).try_insert(StartWorkSynced);
    }
}

fn should_refresh_start_payload(
    command_bar_changed: bool,
    contributions_changed: bool,
    focus_changed: bool,
) -> bool {
    command_bar_changed || contributions_changed || focus_changed
}

fn should_focus_start_sync(
    synced: bool,
    keyboard_target: bool,
    keyboard_target_added: bool,
    focus_changed: bool,
) -> bool {
    keyboard_target && (!synced || keyboard_target_added || focus_changed)
}

fn on_start_data_request(
    trigger: On<UiInput<StartDataRequest>>,
    keyboard_targets: Query<(), With<KeyboardOwner>>,
    tab_gather: TabGatherParams,
    prompt_context: StartPromptContextParams,
    contributed_pages: Query<&ContributedPage>,
    contributed_commands: Query<&ContributedCommand>,
    locale: Option<Res<ResolvedLocale>>,
    space_projects: vmux_space::SpaceProjects,
    definitions: Query<&vmux_command::CommandDefinition>,
    mut repo_info: Option<ResMut<vmux_git::RepoInfoCache>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let cwd = prompt_context.cwd(tab_gather.active_tab.get());
    let git_info = (!cwd.is_empty())
        .then(|| {
            repo_info.as_mut().and_then(|cache| {
                cache
                    .bypass_change_detection()
                    .get(std::path::Path::new(&cwd))
            })
        })
        .flatten();
    let definitions = definitions.iter().cloned().collect::<Vec<_>>();
    let payload = build_start_payload(
        &tab_gather,
        &prompt_context.command_bar,
        &contributed_pages,
        &contributed_commands,
        &prompt_context,
        tab_gather.active_tab.get(),
        git_info.as_ref(),
        space_projects.rows(tab_gather.active_tab.get().unwrap_or(Entity::PLACEHOLDER)),
        prompt_context.command_bar.agent_models.agents.clone(),
        prompt_context.command_bar.agent_modes.agents.clone(),
        &locale
            .as_deref()
            .map(|locale| locale.0.clone())
            .unwrap_or_else(Locale::preferred),
        &definitions,
    );
    vmux_command::snapshot::CommandBarUiStateUpdates::write(&mut commands, webview, &payload);
    if keyboard_targets.contains(webview) {
        vmux_command::snapshot::CommandBarUiStateUpdates::write(
            &mut commands,
            webview,
            &vmux_api::command_bar::CommandBarFocusInput,
        );
    }
}

fn build_start_payload(
    tab_gather: &TabGatherParams,
    command_bar: &CommandBarProjection,
    contributed_pages: &Query<&ContributedPage>,
    contributed_commands: &Query<&ContributedCommand>,
    prompt_context: &StartPromptContextParams,
    active_tab: Option<Entity>,
    git_info: Option<&vmux_git::worktree::RepoInfo>,
    projects: Vec<vmux_api::space::ProjectRow>,
    agent_models: Vec<vmux_api::command_bar::AgentModels>,
    agent_modes: Vec<vmux_api::command_bar::AgentModes>,
    locale: &Locale,
    definitions: &[vmux_command::CommandDefinition],
) -> CommandBarOpenEvent {
    let active_stack_count = tab_gather.stack_q.iter().count();
    let space_name = command_bar.spaces.active_space_name.clone();
    let tabs = gather_command_bar_tabs(
        active_tab,
        &tab_gather.all_children,
        &tab_gather.leaf_panes,
        &tab_gather.pane_ts,
        &tab_gather.pane_children,
        &tab_gather.stack_ts,
        &tab_gather.stack_q,
        &tab_gather.browser_meta,
        &tab_gather.child_of_q,
        &space_name,
        locale,
    );
    let mut payload = build_command_bar_open_payload(
        OpenId::NONE,
        false,
        space_name,
        String::new(),
        &command_bar.spaces,
        contributed_pages,
        contributed_commands,
        &command_bar.pages,
        &command_bar.work,
        locale,
        active_stack_count,
        tabs,
        Some(OpenTarget::InPlace),
        definitions,
    );
    payload.prompt_context = prompt_context.context(active_tab, git_info);
    payload.prompt_context.projects = projects;
    payload.agent_models = agent_models;
    payload.agent_modes = agent_modes;
    payload
}

fn mark_start_pages_as_launcher_hosts(
    starts: Query<(Entity, &PageMetadata), Without<HostsLauncher>>,
    mut commands: Commands,
) {
    for (entity, meta) in starts.iter() {
        if meta.url.starts_with(START_PAGE_URL) {
            commands.entity(entity).try_insert((
                HostsLauncher,
                vmux_command::snapshot::CommandBarUiStateUpdates::default(),
            ));
        }
    }
}

fn begin_requested_inline_transition(
    mut requests: MessageReader<InlineTransitionRequested>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let mut changed = false;
    for request in requests.read() {
        changed = true;
        commands
            .entity(request.stack)
            .try_insert(crate::StartInlineTransition {
                webview: request.webview,
            });
        commands
            .entity(request.webview)
            .try_insert(crate::StartInlineTransitionView);
    }
    if changed && let Some(proxy) = proxy {
        let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::UiInput;
    use vmux_api::command_bar::{CommandBarUiState, CommandBarUiStatePatch};
    use vmux_core::host::UiStateWrite;
    use vmux_core::page::PageManifest;

    #[derive(Resource, Default)]
    struct EmittedIds(Vec<&'static str>);

    fn capture_state(
        trigger: On<UiStateWrite<CommandBarUiState>>,
        mut emitted: ResMut<EmittedIds>,
    ) {
        let kind = match trigger.event().patch() {
            CommandBarUiStatePatch::Snapshot(_) => "snapshot",
            CommandBarUiStatePatch::FocusInput(_) => "focus",
            _ => "other",
        };
        emitted.0.push(kind);
    }

    fn start_ready_app() -> App {
        let mut app = App::new();
        app.init_resource::<CommandBarProjection>()
            .init_resource::<EmittedIds>()
            .add_observer(on_start_data_request)
            .add_observer(capture_state);
        app
    }

    fn emit_start_ready(app: &mut App, webview: Entity) {
        app.world_mut().trigger(UiInput {
            webview,
            payload: StartDataRequest,
        });
        app.update();
    }

    #[test]
    fn start_plugin_spawns_manifest() {
        let mut app = App::new();
        app.add_plugins(StartPlugin);
        let mut q = app.world_mut().query::<&PageManifest>();
        assert!(q.iter(app.world()).any(|m| m.host == "start"));
    }

    #[test]
    fn a_transition_whose_page_already_closed_is_skipped() {
        let mut app = App::new();
        app.add_message::<InlineTransitionRequested>()
            .add_systems(Update, begin_requested_inline_transition);
        let stack = app.world_mut().spawn_empty().id();
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().entity_mut(webview).despawn();

        app.world_mut()
            .write_message(InlineTransitionRequested { stack, webview });
        app.update();

        assert!(
            app.world()
                .get::<crate::StartInlineTransition>(stack)
                .is_some(),
            "the surviving half of the transition still applies"
        );
    }

    #[test]
    fn inline_transition_only_supports_page_agents() {
        assert!(crate::supports_inline_agent_transition(
            "vmux://sessions/codex"
        ));
        assert!(crate::supports_inline_agent_transition(
            "vmux://sessions/openai/gpt-5/session"
        ));
        assert!(!crate::supports_inline_agent_transition(
            "vmux://sessions/codex/cli"
        ));
        assert!(!crate::supports_inline_agent_transition(
            "vmux://sessions/vibe/setup"
        ));
        assert!(crate::supports_inline_agent_transition(
            "vmux://sessions/cliff"
        ));
        assert!(crate::supports_inline_agent_transition(
            "vmux://sessions/setupwizard"
        ));
    }

    #[test]
    fn cold_start_focuses_after_page_ready() {
        let mut app = start_ready_app();
        let webview = app.world_mut().spawn(KeyboardOwner).id();

        emit_start_ready(&mut app, webview);

        let emitted = &app.world().resource::<EmittedIds>().0;
        assert_eq!(emitted, &["snapshot", "focus"]);
    }
}
