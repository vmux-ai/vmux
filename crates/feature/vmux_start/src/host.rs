use std::path::Path;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::chat::{SlashCommand, SlashCommandEntry};
use vmux_api::command_bar::{CommandBarOpenEvent, CommandBarPromptContext, OpenId};
use vmux_api::open_target::OpenTarget;
use vmux_command::{
    ClaimedUrl, CommandBarContextSnapshot, CommandBarPagesSnapshot, CommandBarProjectRoots,
    CommandBarWorkSnapshot, ContributedAgentModels, ContributedAgentModes, ContributedCommand,
    ContributedPage,
};
use vmux_command::{CommandBarOpenProjection, CommandBarProjector};
use vmux_ecs::KeyboardOwner;
use vmux_ecs::PageMetadata;
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ui::i18n::Locale;

use vmux_ecs::launcher::{HostsLauncher, InlineTransitionRequested};
use vmux_layout::settings::ResolvedLocale;
use vmux_layout::tab::{Tab, TabWorkspace, TabWorktree};
use vmux_layout::workspace_snapshot::TabGather;

#[vmux_page::page]
pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        #[cfg(ui)]
        app.add_plugins(crate::ui::StartPage::plugin());
        app.add_plugins(
            Self::MANIFEST
                .plugin()
                .hosted(vmux_ecs::page::HostedPage::page(Self::URL, "Start")),
        )
        .add_message::<InlineTransitionRequested>()
        .add_systems(Startup, prewarm)
        .add_systems(
            Update,
            (
                mark_launcher.before(crate::workspace::Binding),
                begin_inline,
                ApplyDeferred,
                sync_pages.after(crate::workspace::Binding),
            )
                .chain(),
        );
        app.add_plugins((crate::workspace::Plugin, crate::branches::Plugin))
            .add_observer(focus_command_bar)
            .add_observer(focus_ready);
    }
}

fn prewarm(mut commands: Commands) {
    commands.spawn(vmux_ecs::page::PrewarmPage::host(
        "start",
        StartPlugin::URL,
        "Start",
        2,
    ));
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

#[derive(SystemParam)]
struct StartPromptContext<'w, 's> {
    directories: Query<'w, 's, Ref<'static, vmux_command::CommandBarWorkDirectory>>,
    parents: Query<'w, 's, &'static ChildOf>,
    tabs: Query<
        'w,
        's,
        (
            Ref<'static, Tab>,
            Option<Ref<'static, TabWorkspace>>,
            Option<Ref<'static, TabWorktree>>,
        ),
    >,
    context: Single<'w, 's, Ref<'static, CommandBarContextSnapshot>>,
    pages: Single<'w, 's, Ref<'static, CommandBarPagesSnapshot>>,
    projects: Single<'w, 's, Ref<'static, CommandBarProjectRoots>>,
    work: Single<'w, 's, Ref<'static, CommandBarWorkSnapshot>>,
}

impl StartPromptContext<'_, '_> {
    fn unrooted() -> CommandBarPromptContext {
        CommandBarPromptContext {
            slash_commands: vec![
                SlashCommandEntry {
                    command: SlashCommand::Upload,
                    description: "Attach files".to_string(),
                    delegated: false,
                },
                SlashCommandEntry {
                    command: SlashCommand::Resume,
                    description: "Resume a past session".to_string(),
                    delegated: true,
                },
                SlashCommandEntry {
                    command: SlashCommand::Mcp,
                    description: vmux_ui::i18n::translate("mcp-command-description"),
                    delegated: true,
                },
            ],
            ..Default::default()
        }
    }

    fn tab(&self, webview: Entity) -> Option<Entity> {
        let mut current = webview;
        for _ in 0..32 {
            if self.tabs.contains(current) {
                return Some(current);
            }
            current = self.parents.get(current).ok()?.parent();
        }
        None
    }

    fn changed(&self, webview: Entity, tab: Option<Entity>) -> bool {
        if self
            .directories
            .get(webview)
            .is_ok_and(|cwd| cwd.is_changed())
            || self.context.is_changed()
            || self.pages.is_changed()
            || self.projects.is_changed()
            || self.work.is_changed()
        {
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

    fn cwd(&self, webview: Entity, tab: Option<Entity>) -> String {
        if let Ok(cwd) = self.directories.get(webview) {
            return cwd.0.clone();
        }
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
        webview: Entity,
        tab: Option<Entity>,
        info: Option<&vmux_git::worktree::RepoInfo>,
    ) -> CommandBarPromptContext {
        let cwd = self.cwd(webview, tab);
        if cwd.is_empty() {
            return Self::unrooted();
        }
        let path = Path::new(&cwd);
        let is_git_repo = info.is_some() || path.join(".git").exists();
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
            is_git_repo,
            is_worktree: info.is_some_and(|info| info.is_worktree),
            branch: info.map(|info| info.branch.clone()).unwrap_or_default(),
            base_ref: info.map(|info| info.base_ref.clone()).unwrap_or_default(),
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
        webview: Entity,
        git: Option<&vmux_git::worktree::RepoInfo>,
        mut projects: Vec<vmux_api::space::ProjectRow>,
        locale: &Locale,
    ) -> CommandBarOpenEvent {
        let context_label = self.context.label.clone();
        let tab_rows = tabs.tabs(active_tab, &context_label, locale);
        let mut payload = projector.project(CommandBarOpenProjection {
            open_id: OpenId::NONE,
            native_windowed: false,
            context: (**self.context).clone(),
            url: String::new(),
            pages: (**self.pages).clone(),
            projects: (**self.projects).clone(),
            work: (**self.work).clone(),
            locale: locale.clone(),
            tabs: tab_rows,
            target: Some(OpenTarget::InPlace),
        });
        payload.prompt_context = self.context(webview, active_tab, git);
        let directory = git
            .map(|info| info.project_root().to_string_lossy().into_owned())
            .unwrap_or_else(|| payload.prompt_context.cwd.clone());
        let selected = vmux_api::space::ProjectRow::for_directory(&projects, &directory)
            .map(|project| project.path.clone());
        for project in &mut projects {
            project.is_active = selected.as_ref() == Some(&project.path);
        }
        payload.prompt_context.projects = projects;
        payload
    }
}

fn sync_pages(
    tab_gather: TabGather,
    prompt_context: StartPromptContext,
    contributions: (
        Query<
            (),
            Or<(
                Changed<ContributedPage>,
                Changed<ContributedCommand>,
                Changed<ClaimedUrl>,
                Changed<ContributedAgentModels>,
                Changed<ContributedAgentModes>,
            )>,
        >,
        RemovedComponents<ContributedPage>,
        RemovedComponents<ContributedCommand>,
        RemovedComponents<ClaimedUrl>,
        RemovedComponents<ContributedAgentModels>,
        RemovedComponents<ContributedAgentModes>,
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
        (
            With<HostsLauncher>,
            Without<crate::StartInlineTransitionView>,
        ),
    >,
    added_keyboard_targets: Query<(), Added<KeyboardOwner>>,
    mut repo_info: Option<Single<&mut vmux_git::RepoInfoCache>>,
    mut last_git: Local<
        std::collections::HashMap<Entity, (String, Option<vmux_git::worktree::RepoInfo>)>,
    >,
    space_projects: vmux_space::SpaceProjects,
    active_project_rows: Res<vmux_space::ActiveProjectRows>,
    projector: CommandBarProjector,
    mut commands: Commands,
) {
    let (
        contribution_changes,
        mut removed_pages,
        mut removed_commands,
        mut removed_claims,
        mut removed_models,
        mut removed_modes,
    ) = contributions;
    let focus_changed = focused.is_changed();
    let page_removed = removed_pages.read().count() > 0;
    let command_removed = removed_commands.read().count() > 0;
    let claim_removed = removed_claims.read().count() > 0;
    let model_removed = removed_models.read().count() > 0;
    let mode_removed = removed_modes.read().count() > 0;
    let contributions_changed = !contribution_changes.is_empty()
        || page_removed
        || command_removed
        || claim_removed
        || model_removed
        || mode_removed;
    let changed = contributions_changed
        || focus_changed
        || active_project_rows.is_changed()
        || locale.as_ref().is_some_and(|locale| locale.is_changed());
    let locale = locale
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(Locale::preferred);
    last_git.retain(|entity, _| starts.contains(*entity));
    for (e, meta, synced, keyboard_target) in &starts {
        if !meta.url.starts_with(StartPlugin::URL) {
            continue;
        }
        let tab = prompt_context.tab(e);
        if !keyboard_target && synced && !changed && !prompt_context.changed(e, tab) {
            if let Some(cache) = repo_info.as_mut() {
                cache.bypass_change_detection().release(e);
            }
            continue;
        }
        let cwd = prompt_context.cwd(e, tab);
        let git_info = repo_info.as_mut().and_then(|cache| {
            let cache = cache.bypass_change_detection();
            if cwd.is_empty() {
                cache.release(e);
                return None;
            }
            if keyboard_target {
                cache.observe_active(e, Path::new(&cwd))
            } else {
                cache.release(e);
                cache.snapshot(Path::new(&cwd))
            }
        });
        let git_changed = last_git
            .get(&e)
            .is_none_or(|(old_cwd, info)| *old_cwd != cwd || *info != git_info);
        let focus_requested =
            keyboard_target && (!synced || added_keyboard_targets.contains(e) || focus_changed);
        if !changed && synced && !focus_requested && !git_changed && !prompt_context.changed(e, tab)
        {
            continue;
        }
        last_git.insert(e, (cwd, git_info.clone()));
        let payload = prompt_context.project(
            &projector,
            &tab_gather,
            tab,
            e,
            git_info.as_ref(),
            space_projects.selector_rows(e, &active_project_rows),
            &locale,
        );
        commands.trigger(vmux_ecs::UiStateWrite::<
            vmux_api::command_bar::CommandBarUiState,
        >::from_event(e, &payload));
        if focus_requested {
            commands.trigger(CommandBarFocusRequested { webview: e });
        }
        commands.entity(e).try_insert(StartWorkSynced);
    }
}

fn focus_ready(
    event: On<UiInput<vmux_ecs::page::PageReady>>,
    starts: Query<
        &PageMetadata,
        (
            With<HostsLauncher>,
            With<KeyboardOwner>,
            Without<crate::StartInlineTransitionView>,
        ),
    >,
    mut commands: Commands,
) {
    if starts
        .get(event.webview)
        .is_ok_and(|metadata| metadata.url.starts_with(StartPlugin::URL))
    {
        commands.trigger(CommandBarFocusRequested {
            webview: event.webview,
        });
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
            commands.entity(webview).try_insert(revision);
            effect
        }
    };
    commands.trigger(vmux_ecs::UiStateWrite::<
        vmux_api::command_bar::CommandBarUiState,
    >::from_event(webview, &effect));
}

fn mark_launcher(
    starts: Query<(
        Entity,
        &PageMetadata,
        Has<HostsLauncher>,
        Has<StartWorkSynced>,
        Has<crate::StartInlineTransitionView>,
    )>,
    mut commands: Commands,
) {
    for (entity, meta, launcher, synced, transitioning) in starts.iter() {
        if meta.url.starts_with(StartPlugin::URL) && !launcher {
            commands
                .entity(entity)
                .try_insert((HostsLauncher, vmux_command::CommandBarState::default()));
        } else if !meta.url.starts_with(StartPlugin::URL) && synced && !transitioning {
            commands.entity(entity).remove::<(
                HostsLauncher,
                vmux_command::CommandBarState,
                StartWorkSynced,
            )>();
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
                navigation: None,
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
    #[cfg(ui)]
    use vmux_api::input::{KeyClaimsUiState, KeyContextRequest};
    #[cfg(ui)]
    use vmux_api::{BinEvent, UiEventPermissions};
    use vmux_ecs::UiStateWrite;
    use vmux_ecs::page::PageManifest;

    #[derive(Resource, Default)]
    struct EmittedIds(Vec<(&'static str, u64)>);

    fn capture_state(
        trigger: On<UiStateWrite<CommandBarUiState>>,
        mut emitted: ResMut<EmittedIds>,
    ) {
        let patch = trigger.event().update();
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
            .add_observer(focus_ready)
            .add_observer(capture_state);
        app
    }

    fn request_start_focus(app: &mut App, webview: Entity) {
        app.world_mut()
            .trigger(CommandBarFocusRequested { webview });
        app.update();
    }

    #[test]
    fn start_plugin_registers_manifest_before_startup() {
        let mut app = App::new();
        app.add_plugins(StartPlugin);
        let mut q = app.world_mut().query::<&PageManifest>();
        assert!(q.iter(app.world()).any(|m| m.url == StartPlugin::URL));
    }

    #[test]
    fn completed_session_transitions_release_the_hidden_launcher() {
        let mut app = App::new();
        app.add_plugins((bevy::prelude::MinimalPlugins, StartPlugin))
            .insert_non_send(bevy_cef::prelude::Browsers::default());
        let page = app
            .world_mut()
            .spawn((
                HostsLauncher,
                StartWorkSynced,
                crate::StartInlineTransitionView,
                PageMetadata {
                    url: "vmux://sessions/failed-resume".into(),
                    ..Default::default()
                },
            ))
            .id();
        app.world_mut().run_schedule(Update);
        assert!(app.world().get::<HostsLauncher>(page).is_some());
        app.world_mut()
            .entity_mut(page)
            .remove::<crate::StartInlineTransitionView>();
        app.world_mut().run_schedule(Update);
        assert!(app.world().get::<HostsLauncher>(page).is_none());
        assert!(app.world().get::<StartWorkSynced>(page).is_none());
        app.world_mut().get_mut::<PageMetadata>(page).unwrap().url = StartPlugin::URL.into();
        app.world_mut().run_schedule(Update);
        assert!(app.world().get::<HostsLauncher>(page).is_some());
    }

    #[test]
    #[cfg(ui)]
    fn start_page_allows_keyboard_routing() {
        let mut app = App::new();
        app.add_plugins(StartPlugin);
        app.world_mut().run_schedule(PreStartup);
        let mut permissions = app.world_mut().query::<&UiEventPermissions>();

        assert!(UiEventPermissions::allows_page(
            permissions.iter(app.world()),
            StartPlugin::URL,
            KeyContextRequest::PERMISSION,
        ));
        assert!(UiEventPermissions::allows_page(
            permissions.iter(app.world()),
            StartPlugin::URL,
            KeyClaimsUiState::PERMISSION,
        ));
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
    fn selected_start_requests_prompt_focus_after_its_document_is_ready() {
        let mut app = start_focus_app();
        let metadata = PageMetadata {
            url: StartPlugin::URL.to_string(),
            ..Default::default()
        };
        let selected = app
            .world_mut()
            .spawn((HostsLauncher, KeyboardOwner, metadata.clone()))
            .id();
        let dormant = app.world_mut().spawn((HostsLauncher, metadata)).id();
        let another = app
            .world_mut()
            .spawn((
                HostsLauncher,
                KeyboardOwner,
                PageMetadata {
                    url: "vmux://sessions/".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        for webview in [selected, dormant, another] {
            app.world_mut().trigger(UiInput {
                webview,
                payload: vmux_ecs::page::PageReady,
            });
        }
        app.update();
        assert_eq!(app.world().resource::<EmittedIds>().0, [("focus", 1)]);
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
