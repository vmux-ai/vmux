use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use vmux_api::chat::{SlashCommand, SlashCommandEntry};
use vmux_api::command_bar::{CommandBarOpenEvent, CommandBarPromptContext, OpenId};
use vmux_api::space::ProjectBranch;
use vmux_command::open_target::OpenTarget;
use vmux_command::snapshot::{
    ClaimedUrl, CommandBarProjection, ContributedCommand, ContributedPage,
};
use vmux_command::{CommandBarOpenProjection, CommandBarProjector};
use vmux_ecs::KeyboardOwner;
use vmux_ecs::PageMetadata;
use vmux_ecs::host::manifest::FeaturePlugin;
use vmux_ui::i18n::Locale;

use crate::event::StartSelectWorkspace;
use vmux_ecs::launcher::{HostsLauncher, InlineTransitionRequested};
use vmux_layout::settings::ResolvedLocale;
use vmux_layout::tab::{Tab, TabWorkspace, TabWorktree};
use vmux_layout::workspace_snapshot::TabGather;

#[vmux_native::page]
pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        #[cfg(ui)]
        app.add_plugins(crate::ui::StartPage::plugin());
        app.add_plugins(Self::MANIFEST.plugin().hosted(
            vmux_ecs::host::page::NativelyHosted::page(Self::URL, "Start"),
        ))
        .add_message::<InlineTransitionRequested>()
        .add_systems(Update, (mark_launcher, begin_inline));
        app.add_plugins(UiEventPlugin::<(
            StartSelectWorkspace,
            vmux_api::command_bar::StartBranchesRequest,
            vmux_api::command_bar::StartGoToBranch,
        )>::default())
            .add_observer(select_workspace)
            .add_observer(branches_request)
            .add_observer(go_to_branch)
            .add_observer(apply_chosen_project)
            .add_observer(focus_command_bar)
            .add_systems(
                Update,
                (
                    sync_pages,
                    finish_workspace_pickers,
                    read_branches,
                    finish_branch_reads,
                ),
            );
    }
}

#[derive(Component)]
struct StartWorkSynced;

#[derive(Component, Default)]
struct CommandBarFocusRevision(u64);

impl CommandBarFocusRevision {
    fn next(&mut self) -> vmux_api::command_bar::CommandBarFocusEffect {
        self.0 = self.0.wrapping_add(1).max(1);
        vmux_api::command_bar::CommandBarFocusEffect { revision: self.0 }
    }
}

#[derive(EntityEvent)]
struct CommandBarFocusRequested {
    #[event_target]
    webview: Entity,
}

#[derive(Component)]
struct PendingStartWorkspacePicker {
    tab: Entity,
    task: Task<Option<(std::path::PathBuf, bool)>>,
}

#[derive(SystemParam)]
struct StartPromptContext<'w, 's> {
    tabs: Query<
        'w,
        's,
        (
            Ref<'static, Tab>,
            Option<Ref<'static, TabWorkspace>>,
            Option<Ref<'static, TabWorktree>>,
        ),
    >,
    command_bar: Single<'w, 's, Ref<'static, CommandBarProjection>>,
    warmed_branches_for: Local<'s, String>,
}

impl StartPromptContext<'_, '_> {
    fn unrooted() -> CommandBarPromptContext {
        CommandBarPromptContext {
            slash_commands: vec![
                SlashCommandEntry {
                    command: SlashCommand::Upload,
                    description: "Attach files".to_string(),
                },
                SlashCommandEntry {
                    command: SlashCommand::Resume,
                    description: "Resume a past session".to_string(),
                },
                SlashCommandEntry {
                    command: SlashCommand::Mcp,
                    description: String::new(),
                },
            ],
            ..Default::default()
        }
    }

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
            return Self::unrooted();
        };
        let Ok((_, _, worktree)) = self.tabs.get(tab) else {
            return Self::unrooted();
        };
        let cwd = self.cwd(Some(tab));
        if cwd.is_empty() {
            return Self::unrooted();
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
            ..Self::unrooted()
        }
    }

    fn project(
        &self,
        projector: &CommandBarProjector,
        tabs: &TabGather,
        active_tab: Option<Entity>,
        git: Option<&vmux_git::worktree::RepoInfo>,
        projects: Vec<vmux_api::space::ProjectRow>,
        agent_models: Vec<vmux_api::command_bar::AgentModels>,
        agent_modes: Vec<vmux_api::command_bar::AgentModes>,
        locale: &Locale,
    ) -> CommandBarOpenEvent {
        let active_stack_count = tabs.stack_q.iter().count();
        let space_name = self.command_bar.spaces.active_space_name.clone();
        let tab_rows = tabs.tabs(active_tab, &space_name, locale);
        let mut payload = projector.project(CommandBarOpenProjection {
            open_id: OpenId::NONE,
            native_windowed: false,
            space_name,
            url: String::new(),
            spaces: self.command_bar.spaces.clone(),
            terminal_page_url: self.command_bar.terminals.terminal_page_url.clone(),
            pages: self.command_bar.pages.clone(),
            work: self.command_bar.work.clone(),
            locale: locale.clone(),
            active_stack_count,
            tabs: tab_rows,
            target: Some(OpenTarget::InPlace),
        });
        payload.prompt_context = self.context(active_tab, git);
        payload.prompt_context.projects = projects;
        payload.agent_models = agent_models;
        payload.agent_modes = agent_modes;
        payload
    }
}

fn select_workspace(
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
    let projects_dir = vmux_ecs::profile::ProfilePaths::current().projects();
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

fn finish_workspace_pickers(
    mut pending: Query<(Entity, &mut PendingStartWorkspacePicker)>,
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
            commands.trigger(ChosenProject {
                tab: picker.tab,
                path,
                worktree: None,
            });
        }
        commands.entity(entity).despawn();
    }
}

#[derive(Component)]
struct StartBranchQuery {
    webview: Entity,
    project: String,
}

#[derive(Component)]
struct StartBranchRead {
    webview: Entity,
    project: String,
    task: Task<Vec<ProjectBranch>>,
}

fn read_branches(
    queries: Query<(Entity, &StartBranchQuery), Added<StartBranchQuery>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, query) in &queries {
        let project = query.project.trim().to_string();
        if project.is_empty() {
            commands.entity(entity).despawn();
            continue;
        }
        let root = std::path::PathBuf::from(&project);
        let wake = vmux_ecs::host::wake::Wake::beside(proxy.as_deref());
        let task = IoTaskPool::get().spawn(async move {
            let _wake = wake;
            let mut branches = Vec::new();
            if let Ok(holders) = vmux_git::worktree::branch_holders(&root) {
                branches.reserve(holders.len());
                for holder in holders {
                    let checkout = holder.checkout_path();
                    let label = holder.checkout_label();
                    branches.push(ProjectBranch {
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
        commands.entity(entity).insert(StartBranchRead {
            webview: query.webview,
            project,
            task,
        });
        commands.entity(entity).remove::<StartBranchQuery>();
    }
}

fn branches_request(
    trigger: On<UiInput<vmux_api::command_bar::StartBranchesRequest>>,
    mut commands: Commands,
) {
    commands.spawn(StartBranchQuery {
        webview: trigger.event().webview,
        project: trigger.event().payload.project.clone(),
    });
}

fn finish_branch_reads(
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
        commands.trigger(vmux_ecs::host::UiStateWrite::<
            vmux_api::command_bar::CommandBarUiState,
        >::from_event(
            read.webview,
            &vmux_api::command_bar::StartProjectBranches {
                project: read.project.clone(),
                branches,
            },
        ));
    }
}

fn go_to_branch(
    trigger: On<UiInput<vmux_api::command_bar::StartGoToBranch>>,
    child_of: Query<&ChildOf>,
    tab_query: Query<(), With<Tab>>,
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
        commands.trigger(ChosenProject {
            tab,
            path,
            worktree: None,
        });
        return;
    }
    let Ok(root) = std::path::PathBuf::from(&evt.project).canonicalize() else {
        return;
    };
    let worktree = (!evt.branch.trim().is_empty()).then(|| TabWorktree {
        repo_root: root.to_string_lossy().into_owned(),
        checkout_dir: String::new(),
        branch: evt.branch.clone(),
        base_ref: String::new(),
    });
    commands.trigger(ChosenProject {
        tab,
        path: root,
        worktree,
    });
}

#[derive(EntityEvent)]
struct ChosenProject {
    #[event_target]
    tab: Entity,
    path: std::path::PathBuf,
    worktree: Option<TabWorktree>,
}

fn apply_chosen_project(
    trigger: On<ChosenProject>,
    mut tabs: Query<&mut Tab>,
    mut commands: Commands,
) {
    let request = trigger.event();
    let Ok(mut tab) = tabs.get_mut(request.tab) else {
        return;
    };
    let dir = request.path.to_string_lossy().into_owned();
    tab.startup_dir = Some(dir.clone());
    if vmux_layout::worktree::is_generated_tab_name(&tab.name)
        && let Some(name) = request.path.file_name().and_then(|name| name.to_str())
        && !name.is_empty()
    {
        tab.name = name.to_string();
    }
    let mut entity = commands.entity(request.tab);
    entity
        .insert((
            TabWorkspace { project_dir: dir },
            vmux_layout::tab::TabDirDecided,
        ))
        .remove::<(
            TabWorktree,
            vmux_layout::worktree::TabWorktreeReady,
            vmux_layout::tab::TabWorktreeUnavailable,
        )>();
    if let Some(worktree) = &request.worktree {
        entity.insert(worktree.clone());
    }
}

fn sync_pages(
    tab_gather: TabGather,
    mut prompt_context: StartPromptContext,
    contributions: (
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
    focused: vmux_layout::stack::FocusedStack,
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
    mut repo_info: Option<Single<&mut vmux_git::RepoInfoCache>>,
    mut last_git: Local<(String, Option<vmux_git::worktree::RepoInfo>)>,
    space_projects: vmux_space::SpaceProjects,
    projector: CommandBarProjector,
    mut commands: Commands,
) {
    let (contribution_changes, mut removed_pages, mut removed_commands, mut removed_claims) =
        contributions;
    let cwd = prompt_context.cwd(tab_gather.active_tab.get());
    let git_info = (!cwd.is_empty())
        .then(|| {
            repo_info.as_mut().and_then(|cache| {
                cache
                    .bypass_change_detection()
                    .lookup(std::path::Path::new(&cwd))
            })
        })
        .flatten();
    let git_changed = last_git.0 != cwd || last_git.1 != git_info;
    let focus_changed = focused.is_changed();
    let contributions_changed = !contribution_changes.is_empty()
        || removed_pages.read().next().is_some()
        || removed_commands.read().next().is_some()
        || removed_claims.read().next().is_some();
    let changed = prompt_context.command_bar.is_changed()
        || contributions_changed
        || focus_changed
        || prompt_context.changed(tab_gather.active_tab.get())
        || git_changed
        || locale.as_ref().is_some_and(|locale| locale.is_changed());
    let locale = locale
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(Locale::preferred);
    let targets: Vec<(Entity, bool)> = starts
        .iter()
        .filter_map(|(e, meta, synced, keyboard_target)| {
            if !meta.url.starts_with(StartPlugin::URL) {
                return None;
            }
            if !browsers.can_emit_to(&e) {
                return None;
            }
            let focus_requested =
                keyboard_target && (!synced || added_keyboard_targets.contains(e) || focus_changed);
            (changed || !synced || focus_requested).then_some((e, focus_requested))
        })
        .collect();
    if targets.is_empty() {
        return;
    }
    if git_changed {
        *last_git = (cwd.clone(), git_info.clone());
    }
    let payload = prompt_context.project(
        &projector,
        &tab_gather,
        tab_gather.active_tab.get(),
        git_info.as_ref(),
        space_projects.rows(tab_gather.active_tab.get().unwrap_or(Entity::PLACEHOLDER)),
        prompt_context.command_bar.agent_models.agents.clone(),
        prompt_context.command_bar.agent_modes.agents.clone(),
        &locale,
    );
    let project = payload
        .prompt_context
        .projects
        .iter()
        .find(|project| project.is_active)
        .map(|project| project.path.clone())
        .unwrap_or_else(|| payload.prompt_context.cwd.clone());
    let warm_branches = !project.is_empty() && *prompt_context.warmed_branches_for != project;
    if warm_branches {
        *prompt_context.warmed_branches_for = project.clone();
    }
    for (e, focus_requested) in targets {
        if warm_branches {
            commands.spawn(StartBranchQuery {
                webview: e,
                project: project.clone(),
            });
        }
        commands.trigger(vmux_ecs::host::UiStateWrite::<
            vmux_api::command_bar::CommandBarUiState,
        >::from_event(e, &payload));
        if focus_requested {
            commands.trigger(CommandBarFocusRequested { webview: e });
        }
        commands.entity(e).try_insert(StartWorkSynced);
    }
}

fn focus_command_bar(
    trigger: On<CommandBarFocusRequested>,
    mut revisions: Query<&mut CommandBarFocusRevision>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let effect = match revisions.get_mut(webview) {
        Ok(mut revision) => revision.next(),
        Err(_) => {
            let mut revision = CommandBarFocusRevision::default();
            let effect = revision.next();
            commands.entity(webview).insert(revision);
            effect
        }
    };
    commands.trigger(vmux_ecs::host::UiStateWrite::<
        vmux_api::command_bar::CommandBarUiState,
    >::from_event(webview, &effect));
}

fn mark_launcher(
    starts: Query<(Entity, &PageMetadata), Without<HostsLauncher>>,
    mut commands: Commands,
) {
    for (entity, meta) in starts.iter() {
        if meta.url.starts_with(StartPlugin::URL) {
            commands.entity(entity).try_insert((
                HostsLauncher,
                vmux_command::snapshot::CommandBarUiStateUpdates::default(),
            ));
        }
    }
}

fn begin_inline(
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
    use vmux_api::command_bar::CommandBarUiState;
    use vmux_ecs::host::UiStateWrite;
    use vmux_ecs::page::PageManifest;

    #[derive(Resource, Default)]
    struct EmittedIds(Vec<(&'static str, u64)>);

    fn capture_state(
        trigger: On<UiStateWrite<CommandBarUiState>>,
        mut emitted: ResMut<EmittedIds>,
    ) {
        let patch = trigger.event().patch();
        let entry = if patch.snapshot.is_some() {
            ("snapshot", 0)
        } else if let Some(effect) = &patch.focus {
            ("focus", effect.revision)
        } else {
            ("other", 0)
        };
        emitted.0.push(entry);
    }

    fn start_focus_app() -> App {
        let mut app = App::new();
        app.init_resource::<EmittedIds>()
            .add_observer(focus_command_bar)
            .add_observer(capture_state);
        app
    }

    fn request_start_focus(app: &mut App, webview: Entity) {
        app.world_mut()
            .trigger(CommandBarFocusRequested { webview });
        app.update();
    }

    #[test]
    fn start_plugin_spawns_manifest() {
        let mut app = App::new();
        app.add_plugins(StartPlugin);
        app.world_mut().run_schedule(PreStartup);
        let mut q = app.world_mut().query::<&PageManifest>();
        assert!(q.iter(app.world()).any(|m| m.url == StartPlugin::URL));
    }

    #[test]
    fn a_transition_whose_page_already_closed_is_skipped() {
        let mut app = App::new();
        app.add_message::<InlineTransitionRequested>()
            .add_systems(Update, begin_inline);
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
    fn first_focus_effect_starts_at_one() {
        let mut app = start_focus_app();
        let webview = app.world_mut().spawn_empty().id();

        request_start_focus(&mut app, webview);

        let emitted = &app.world().resource::<EmittedIds>().0;
        assert_eq!(emitted, &[("focus", 1)]);
    }

    #[test]
    fn repeated_focus_effects_advance_the_revision() {
        let mut app = start_focus_app();
        let webview = app.world_mut().spawn_empty().id();

        request_start_focus(&mut app, webview);
        request_start_focus(&mut app, webview);

        let emitted = &app.world().resource::<EmittedIds>().0;
        assert_eq!(emitted, &[("focus", 1), ("focus", 2)]);
    }
}
