use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{
    BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers, CefKeyboardTarget,
    WebviewExtendStandardMaterial, WebviewSource,
};
use vmux_command::event::{CommandBarOpenEvent, CommandBarPromptContext};
use vmux_command::open_target::OpenTarget;
use vmux_command::snapshot::{
    CommandBarContributions, CommandBarPagesSnapshot, CommandBarSpacesSnapshot,
    CommandBarWorkSnapshot,
};
use vmux_core::{
    CefPageAttachRequest, PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask,
};

use crate::cef::Browser;
use crate::command_bar::handler::{
    TabGatherParams, build_command_bar_open_payload, gather_command_bar_tabs,
};
use crate::settings::ResolvedLocale;
use crate::start::START_PAGE_URL;
use crate::start::event::{
    START_COMMAND_BAR_OPEN_EVENT, START_FOCUS_INPUT_EVENT, StartDataRequest, StartFocusInput,
    StartSelectWorkspace,
};
use crate::tab::{Tab, TabWorkspace, TabWorktree};
use crate::window::VmuxWindow;

/// Bevy plugin for `vmux://start/`: spawns the page manifest, claims start page-open tasks,
/// and answers [`StartDataRequest`] with the shared command-bar payload.
pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::start::PAGE_MANIFEST);
        app.add_plugins(BinEventEmitterPlugin::<(
            StartDataRequest,
            StartSelectWorkspace,
        )>::for_hosts(&["start"]))
            .add_message::<StartSpareRevealed>()
            .add_observer(on_start_data_request)
            .add_observer(on_start_select_workspace)
            .add_systems(
                Update,
                (
                    handle_start_page_open.in_set(PageOpenSet::HandleKnownPages),
                    maintain_warm_start_pool,
                    on_start_spare_revealed.after(PageOpenSet::HandleKnownPages),
                    sync_live_start_pages,
                    drain_start_workspace_pickers,
                ),
            );
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

/// How many prewarmed `vmux://start/` webviews to keep ready.
const WARM_START_POOL_SIZE: usize = 1;

/// Marks a prewarmed, parked `vmux://start/` webview waiting to be claimed by the next
/// start open. Removed when the spare is reparented into a real stack.
#[derive(Component)]
struct WarmStartSpare;

/// Set on a warm spare once its page has actually mounted (it emitted [`StartDataRequest`]),
/// so a claim only reuses a spare that is genuinely warm — never one whose CEF browser or
/// WASM is still loading (which would defeat the near-instant path and fall to a cold paint).
#[derive(Component)]
struct WarmStartReady;

/// The hidden, zero-size holding node the warm spares are parked under so they keep their
/// CEF browser + WASM warm without compositing (a `Visibility::Hidden` ancestor makes them
/// non-renderable, so `sync_children_to_ui` collapses them and CEF hides the native view).
#[derive(Component)]
struct WarmStartPoolNode;

/// Marks a live `vmux://start/` page that has received the current launcher payload.
/// Cleared implicitly by re-pushing whenever a launcher snapshot changes, so a page that
/// becomes ready after snapshots were populated still gets the data.
#[derive(Component)]
struct StartWorkSynced;

/// Host-internal signal that a warm spare was just revealed into a stack, so its launcher
/// data must be refreshed (it captured boot-time tabs/spaces) and its input refocused.
#[derive(Message)]
struct StartSpareRevealed {
    webview: Entity,
}

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
                if crate::worktree::is_generated_tab_name(&tab.name)
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
                        crate::tab::TabDirDecided,
                    ))
                    .remove::<(
                        TabWorktree,
                        crate::worktree::TabWorktreeReady,
                        crate::tab::TabWorktreeUnavailable,
                    )>();
            }
        }
        commands.entity(entity).despawn();
    }
}

/// Keep every live `vmux://start/` page's launcher payload current, so open-pane dirs,
/// recent files, agent order, spaces, and pages auto-update without a reopen. Pushes to a ready
/// start page when a launcher snapshot changed this frame, or when newly ready and not yet synced
/// (covers panes that spawn before the start page's CEF is ready). Uses `open_id: 0`,
/// which does not reset the palette's input/selection.
fn sync_live_start_pages(
    tab_gather: TabGatherParams,
    prompt_context: StartPromptContextParams,
    spaces_snapshot: Res<CommandBarSpacesSnapshot>,
    contributions: Res<CommandBarContributions>,
    pages_snapshot: Res<CommandBarPagesSnapshot>,
    work_snapshot: Res<CommandBarWorkSnapshot>,
    locale: Option<Res<ResolvedLocale>>,
    focused: Res<crate::stack::FocusedStack>,
    starts: Query<
        (
            Entity,
            &WebviewSource,
            Has<StartWorkSynced>,
            Has<CefKeyboardTarget>,
        ),
        Without<crate::start::StartInlineTransitionView>,
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
        contributions.is_changed(),
        pages_snapshot.is_changed(),
        work_snapshot.is_changed(),
        focus_changed,
    ) || prompt_context.changed(tab_gather.active_tab.get())
        || git_changed
        || locale.as_ref().is_some_and(|locale| locale.is_changed());
    let locale = locale
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(|| vmux_ui::i18n::requested_locale(None));
    let targets: Vec<(Entity, bool)> = starts
        .iter()
        .filter_map(|(e, src, synced, keyboard_target)| {
            let WebviewSource::Url(url) = src else {
                return None;
            };
            if !url.starts_with(START_PAGE_URL) {
                return None;
            }
            if !browsers.has_browser(e) || !browsers.host_emit_ready(&e) {
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

/// Claim `vmux://start/` page-open tasks. When a warm spare is available it is reparented
/// into the target stack for a near-instant paint; otherwise it falls back to spawning a
/// cold launcher webview via [`CefPageAttachRequest`].
fn handle_start_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    spares: Query<Entity, (With<WarmStartSpare>, With<WarmStartReady>)>,
    children_q: Query<&Children>,
    mut attach: MessageWriter<CefPageAttachRequest>,
    mut revealed: MessageWriter<StartSpareRevealed>,
    mut commands: Commands,
) {
    let mut available: Vec<Entity> = spares.iter().collect();
    for (entity, task) in &tasks {
        if task.url != START_PAGE_URL {
            continue;
        }
        if let Some(spare) = available.pop() {
            clear_stack_children(task.stack, &children_q, &mut commands);
            commands.entity(task.stack).insert(PageMetadata {
                url: START_PAGE_URL.to_string(),
                title: "Start".to_string(),
                ..default()
            });
            commands
                .entity(spare)
                .insert((ChildOf(task.stack), CefKeyboardTarget))
                .remove::<(WarmStartSpare, WarmStartReady)>();
            revealed.write(StartSpareRevealed { webview: spare });
        } else {
            attach.write(CefPageAttachRequest {
                stack: task.stack,
                url: START_PAGE_URL.to_string(),
                title: "Start".to_string(),
                bg_color: None,
            });
        }
        commands.entity(entity).insert(PageOpenHandled);
    }
}

/// Keep the warm-start pool topped up to [`WARM_START_POOL_SIZE`]. Spares are parked under a
/// hidden holding node (created lazily once the window exists) so their CEF browser + WASM
/// load ahead of time without compositing.
fn maintain_warm_start_pool(
    pool_node: Query<Entity, With<WarmStartPoolNode>>,
    vmux_window: Query<Entity, With<VmuxWindow>>,
    spares: Query<(), With<WarmStartSpare>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let Ok(window) = vmux_window.single() else {
        return;
    };
    let node = match pool_node.single() {
        Ok(node) => node,
        Err(_) => commands
            .spawn((
                WarmStartPoolNode,
                Node {
                    width: Val::Px(0.0),
                    height: Val::Px(0.0),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Visibility::Hidden,
                ChildOf(window),
            ))
            .id(),
    };
    for _ in spares.iter().count()..WARM_START_POOL_SIZE {
        commands.spawn((
            Browser::new_with_title(&mut meshes, &mut webview_mt, START_PAGE_URL, "Start"),
            WarmStartSpare,
            ChildOf(node),
        ));
    }
}

/// Refresh a freshly-revealed warm spare: push current launcher data (the spare captured
/// boot-time state) and refocus its input, matching a cold open.
fn on_start_spare_revealed(
    mut revealed: MessageReader<StartSpareRevealed>,
    tab_gather: TabGatherParams,
    prompt_context: StartPromptContextParams,
    spaces_snapshot: Res<CommandBarSpacesSnapshot>,
    contributions: Res<CommandBarContributions>,
    pages_snapshot: Res<CommandBarPagesSnapshot>,
    work_snapshot: Res<CommandBarWorkSnapshot>,
    locale: Option<Res<ResolvedLocale>>,
    mut commands: Commands,
) {
    for ev in revealed.read() {
        let locale = locale
            .as_deref()
            .map(|locale| locale.0.clone())
            .unwrap_or_else(|| vmux_ui::i18n::requested_locale(None));
        let payload = build_start_payload(
            &tab_gather,
            &spaces_snapshot,
            &contributions,
            &pages_snapshot,
            &work_snapshot,
            &prompt_context,
            tab_gather.active_tab.get(),
            None,
            &locale,
        );
        commands.trigger(BinHostEmitEvent::from_rkyv(
            ev.webview,
            START_COMMAND_BAR_OPEN_EVENT,
            &payload,
        ));
        commands.trigger(BinHostEmitEvent::from_rkyv(
            ev.webview,
            START_FOCUS_INPUT_EVENT,
            &StartFocusInput,
        ));
    }
}

/// Answer the `vmux://start/` page's on-mount [`StartDataRequest`] with the shared
/// command-bar launcher payload (opening selections in place).
fn on_start_data_request(
    trigger: On<BinReceive<StartDataRequest>>,
    spares: Query<(), With<WarmStartSpare>>,
    keyboard_targets: Query<(), With<CefKeyboardTarget>>,
    tab_gather: TabGatherParams,
    prompt_context: StartPromptContextParams,
    spaces_snapshot: Res<CommandBarSpacesSnapshot>,
    contributions: Res<CommandBarContributions>,
    pages_snapshot: Res<CommandBarPagesSnapshot>,
    work_snapshot: Res<CommandBarWorkSnapshot>,
    locale: Option<Res<ResolvedLocale>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let is_spare = spares.contains(webview);
    if is_spare {
        commands.entity(webview).insert(WarmStartReady);
    }
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
            .unwrap_or_else(|| vmux_ui::i18n::requested_locale(None)),
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
    contributions: &CommandBarContributions,
    pages_snapshot: &CommandBarPagesSnapshot,
    work_snapshot: &CommandBarWorkSnapshot,
    prompt_context: &StartPromptContextParams,
    active_tab: Option<Entity>,
    git_info: Option<&vmux_git::worktree::RepoInfo>,
    locale: &str,
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
        0,
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

/// Despawn a stack's existing webview children before attaching new content.
fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
}

#[cfg(test)]
#[path = "plugin.test.rs"]
mod tests;
