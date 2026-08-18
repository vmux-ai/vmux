use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{
    BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers, CefKeyboardTarget, WebviewSource,
};
use vmux_command::event::{CommandBarOpenEvent, CommandBarPromptContext, OpenId};
use vmux_command::open_target::OpenTarget;
use vmux_command::snapshot::{
    CommandBarPagesSnapshot, CommandBarSpacesSnapshot, CommandBarWorkSnapshot, Contributions,
    ContributionsChanged,
};
use vmux_core::PageMetadata;
use vmux_ui::i18n::Locale;

use crate::START_PAGE_URL;
use crate::event::{
    START_COMMAND_BAR_OPEN_EVENT, START_FOCUS_INPUT_EVENT, StartDataRequest, StartFocusInput,
    StartSelectWorkspace,
};
use vmux_command::build_command_bar_open_payload;
use vmux_core::launcher::{FocusLauncherInput, HostsLauncher, InlineTransitionRequested};
use vmux_layout::settings::ResolvedLocale;
use vmux_layout::tab::{Tab, TabWorkspace, TabWorktree};
use vmux_layout::workspace_snapshot::{TabGatherParams, gather_command_bar_tabs};

/// Bevy plugin for `vmux://start/`: spawns the page manifest, claims start page-open tasks,
/// and answers [`StartDataRequest`] with the shared command-bar payload.
pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn((
            crate::PAGE_MANIFEST,
            vmux_core::host::page::NativelyHosted {
                url: START_PAGE_URL,
                title: "Start",
            },
        ));
        app.add_message::<FocusLauncherInput>()
            .add_message::<InlineTransitionRequested>()
            .add_systems(
                Update,
                (
                    mark_start_pages_as_launcher_hosts,
                    focus_start_input_on_request,
                    begin_requested_inline_transition,
                ),
            );
        // Without this an in-place navigation to the URL takes the plain-navigate branch and asks
        // a CEF browser to load it, which for a natively-hosted page means loading nothing. Every
        // route to the launcher — typed, bookmarked, or the startup url — comes through
        // `handle_native_page_open` instead.
        vmux_core::register_host_spawn(app, "start");
        app.add_plugins(BinEventEmitterPlugin::<(
            StartDataRequest,
            StartSelectWorkspace,
        )>::for_hosts(&["start"]))
            .add_observer(on_start_data_request)
            .add_observer(on_start_select_workspace)
            .add_systems(
                Update,
                (sync_live_start_pages, drain_start_workspace_pickers),
            );
    }
}

/// Marks a live `vmux://start/` page that has received the current launcher payload.
/// Cleared implicitly by re-pushing whenever a launcher snapshot changes, so a page that
/// becomes ready after snapshots were populated still gets the data.
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
}

impl StartPromptContextParams<'_, '_> {
    fn changed(&self, tab: Option<Entity>) -> bool {
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
            return default();
        };
        let Ok((_, _, worktree)) = self.tabs.get(tab) else {
            return default();
        };
        let cwd = self.cwd(Some(tab));
        if cwd.is_empty() {
            return default();
        }
        let path = std::path::Path::new(&cwd);
        CommandBarPromptContext {
            workspace_name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| cwd.clone()),
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
        }
    }
}

fn on_start_select_workspace(
    trigger: On<BinReceive<StartSelectWorkspace>>,
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
    let workspace_dir = vmux_core::profile::workspace_dir();
    let initial_dir = std::fs::create_dir_all(&workspace_dir)
        .ok()
        .map(|_| workspace_dir)
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
            if let Ok(mut tab) = tabs.get_mut(picker.tab) {
                tab.startup_dir = Some(path.to_string_lossy().into_owned());
                if vmux_layout::worktree::is_generated_tab_name(&tab.name)
                    && let Some(name) = path.file_name().and_then(|name| name.to_str())
                    && !name.is_empty()
                {
                    tab.name = name.to_string();
                }
                commands
                    .entity(picker.tab)
                    .insert((
                        TabWorkspace {
                            project_dir: path.to_string_lossy().into_owned(),
                        },
                        vmux_layout::tab::TabDirDecided,
                    ))
                    .remove::<(
                        TabWorktree,
                        vmux_layout::worktree::TabWorktreeReady,
                        vmux_layout::tab::TabWorktreeUnavailable,
                    )>();
            }
        }
        commands.entity(entity).despawn();
    }
}

/// Keep every live `vmux://start/` page's launcher payload current, so open-pane dirs,
/// recent files, agent order, spaces, and pages auto-update without a reopen. Pushes to a ready
/// start page when a launcher snapshot changed this frame, or when newly ready and not yet synced
/// (covers panes that spawn before the start page's CEF is ready). Uses [`OpenId::NONE`],
/// which does not reset the palette's input/selection.
fn sync_live_start_pages(
    tab_gather: TabGatherParams,
    prompt_context: StartPromptContextParams,
    spaces_snapshot: Res<CommandBarSpacesSnapshot>,
    contributions: Contributions,
    mut contributions_changed: ContributionsChanged,
    pages_snapshot: Res<CommandBarPagesSnapshot>,
    work_snapshot: Res<CommandBarWorkSnapshot>,
    locale: Option<Res<ResolvedLocale>>,
    focused: Res<vmux_layout::stack::FocusedStack>,
    starts: Query<
        (
            Entity,
            &WebviewSource,
            Has<StartWorkSynced>,
            Has<CefKeyboardTarget>,
        ),
        Without<crate::StartInlineTransitionView>,
    >,
    added_keyboard_targets: Query<(), Added<CefKeyboardTarget>>,
    browsers: NonSend<Browsers>,
    mut repo_info: Option<ResMut<vmux_git::RepoInfoCache>>,
    mut last_git: Local<(String, Option<vmux_git::worktree::RepoInfo>)>,
    mut commands: Commands,
) {
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
    if git_changed {
        *last_git = (cwd, git_info.clone());
    }
    let focus_changed = focused.is_changed();
    let changed = should_refresh_start_payload(
        spaces_snapshot.is_changed(),
        contributions_changed.any(),
        pages_snapshot.is_changed(),
        work_snapshot.is_changed(),
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
        .filter_map(|(e, src, synced, keyboard_target)| {
            let WebviewSource(url) = src;
            if !url.starts_with(START_PAGE_URL) {
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
    let payload = build_start_payload(
        &tab_gather,
        &spaces_snapshot,
        &contributions,
        &pages_snapshot,
        &work_snapshot,
        &prompt_context,
        tab_gather.active_tab.get(),
        git_info.as_ref(),
        &locale,
    );
    for (e, focus_requested) in targets {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            e,
            START_COMMAND_BAR_OPEN_EVENT,
            &payload,
        ));
        if focus_requested {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                e,
                START_FOCUS_INPUT_EVENT,
                &StartFocusInput,
            ));
        }
        // The start page can be despawned this frame (e.g. selecting an agent opens in-place over
        // it) before this command applies — `try_insert` skips silently instead of panicking.
        commands.entity(e).try_insert(StartWorkSynced);
    }
}

fn should_refresh_start_payload(
    spaces_changed: bool,
    contributions_changed: bool,
    pages_changed: bool,
    work_changed: bool,
    focus_changed: bool,
) -> bool {
    spaces_changed || contributions_changed || pages_changed || work_changed || focus_changed
}

fn should_focus_start_sync(
    synced: bool,
    keyboard_target: bool,
    keyboard_target_added: bool,
    focus_changed: bool,
) -> bool {
    keyboard_target && (!synced || keyboard_target_added || focus_changed)
}

/// Answer the `vmux://start/` page's on-mount [`StartDataRequest`] with the shared
/// command-bar launcher payload (opening selections in place).
fn on_start_data_request(
    trigger: On<BinReceive<StartDataRequest>>,
    keyboard_targets: Query<(), With<CefKeyboardTarget>>,
    tab_gather: TabGatherParams,
    prompt_context: StartPromptContextParams,
    spaces_snapshot: Res<CommandBarSpacesSnapshot>,
    contributions: Contributions,
    pages_snapshot: Res<CommandBarPagesSnapshot>,
    work_snapshot: Res<CommandBarWorkSnapshot>,
    locale: Option<Res<ResolvedLocale>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let payload = build_start_payload(
        &tab_gather,
        &spaces_snapshot,
        &contributions,
        &pages_snapshot,
        &work_snapshot,
        &prompt_context,
        tab_gather.active_tab.get(),
        None,
        &locale
            .as_deref()
            .map(|locale| locale.0.clone())
            .unwrap_or_else(Locale::preferred),
    );
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        START_COMMAND_BAR_OPEN_EVENT,
        &payload,
    ));
    if keyboard_targets.contains(webview) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            START_FOCUS_INPUT_EVENT,
            &StartFocusInput,
        ));
    }
}

/// Build the launcher payload shared by the on-mount data feed and warm-spare refresh.
fn build_start_payload(
    tab_gather: &TabGatherParams,
    spaces_snapshot: &CommandBarSpacesSnapshot,
    contributions: &Contributions,
    pages_snapshot: &CommandBarPagesSnapshot,
    work_snapshot: &CommandBarWorkSnapshot,
    prompt_context: &StartPromptContextParams,
    active_tab: Option<Entity>,
    git_info: Option<&vmux_git::worktree::RepoInfo>,
    locale: &Locale,
) -> CommandBarOpenEvent {
    let active_stack_count = tab_gather.stack_q.iter().count();
    let space_name = spaces_snapshot.active_space_name.clone();
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
        spaces_snapshot,
        contributions,
        pages_snapshot,
        work_snapshot,
        locale,
        active_stack_count,
        tabs,
        Some(OpenTarget::InPlace),
    );
    payload.prompt_context = prompt_context.context(active_tab, git_info);
    payload
}

/// Claims [`HostsLauncher`] for every start page, so the command bar can ask what a page can do
/// instead of comparing its URL.
///
/// Read from `PageMetadata` rather than `WebviewSource`: a natively-hosted launcher has no source,
/// because no CEF browser is fetching anything for it, and one that never claimed this marker would
/// go quiet — no focus request, and nothing for the command bar to recognise.
///
/// `try_insert`, because a launcher is replaced by whatever the user picks from it: choosing an
/// agent clears the stack's children in the same frame this ran, and a plain `insert` applied
/// afterwards takes the whole app down with it.
fn mark_start_pages_as_launcher_hosts(
    starts: Query<(Entity, &PageMetadata), Without<HostsLauncher>>,
    mut commands: Commands,
) {
    for (entity, meta) in starts.iter() {
        if meta.url.starts_with(START_PAGE_URL) {
            commands.entity(entity).try_insert(HostsLauncher);
        }
    }
}

fn focus_start_input_on_request(
    mut requests: MessageReader<FocusLauncherInput>,
    starts: Query<(), With<HostsLauncher>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !starts.contains(request.webview) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            request.webview,
            crate::event::START_FOCUS_INPUT_EVENT,
            &crate::event::StartFocusInput,
        ));
    }
}

fn begin_requested_inline_transition(
    mut requests: MessageReader<InlineTransitionRequested>,
    mut commands: Commands,
) {
    for request in requests.read() {
        commands
            .entity(request.stack)
            .insert(crate::StartInlineTransition {
                webview: request.webview,
            });
        commands
            .entity(request.webview)
            .insert(crate::StartInlineTransitionView);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::BinReceive;
    use vmux_core::page::PageManifest;

    #[derive(Resource, Default)]
    struct EmittedIds(Vec<String>);

    fn capture_emit(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<EmittedIds>) {
        emitted.0.push(trigger.id.clone());
    }

    fn start_ready_app() -> App {
        let mut app = App::new();
        app.init_resource::<CommandBarSpacesSnapshot>()
            .init_resource::<CommandBarPagesSnapshot>()
            .init_resource::<CommandBarWorkSnapshot>()
            .init_resource::<EmittedIds>()
            .add_observer(on_start_data_request)
            .add_observer(capture_emit);
        app
    }

    fn emit_start_ready(app: &mut App, webview: Entity) {
        app.world_mut().trigger(BinReceive {
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
    fn inline_transition_only_supports_page_agents() {
        assert!(crate::supports_inline_agent_transition(
            "vmux://agent/codex"
        ));
        assert!(crate::supports_inline_agent_transition(
            "vmux://agent/openai/gpt-5/session"
        ));
        assert!(!crate::supports_inline_agent_transition(
            "vmux://agent/codex/cli"
        ));
        assert!(!crate::supports_inline_agent_transition(
            "vmux://agent/vibe/setup"
        ));
        assert!(crate::supports_inline_agent_transition(
            "vmux://agent/cliff"
        ));
        assert!(crate::supports_inline_agent_transition(
            "vmux://agent/setupwizard"
        ));
    }

    #[test]
    fn cold_start_focuses_after_page_ready() {
        let mut app = start_ready_app();
        let webview = app.world_mut().spawn(CefKeyboardTarget).id();

        emit_start_ready(&mut app, webview);

        let emitted = &app.world().resource::<EmittedIds>().0;
        assert_eq!(
            emitted,
            &[START_COMMAND_BAR_OPEN_EVENT, START_FOCUS_INPUT_EVENT]
        );
    }
}
