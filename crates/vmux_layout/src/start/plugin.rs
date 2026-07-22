use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{
    BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers, CefKeyboardTarget,
    WebviewExtendStandardMaterial, WebviewSource,
};
use vmux_command::event::{COMMAND_BAR_OPEN_EVENT, CommandBarOpenEvent, CommandBarPromptContext};
use vmux_command::open_target::OpenTarget;
use vmux_command::snapshot::{
    CommandBarAgentsSnapshot, CommandBarPagesSnapshot, CommandBarSpacesSnapshot,
    CommandBarWorkSnapshot,
};
use vmux_core::{
    CefPageAttachRequest, PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask,
};

use crate::cef::Browser;
use crate::command_bar::handler::{
    TabGatherParams, build_command_bar_open_payload, gather_command_bar_tabs,
};
use crate::start::START_PAGE_URL;
use crate::start::event::{
    START_FOCUS_INPUT_EVENT, StartDataRequest, StartFocusInput, StartSelectWorkspace,
};
use crate::tab::{Tab, TabWorkspace, TabWorktree};
use crate::window::VmuxWindow;

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

/// How many prewarmed `vmux://start/` webviews to keep ready. Each new tab / stack /
/// pane / space is an independent persistent start page, so a singleton (like the
/// command-bar modal) cannot serve them — a small pool is refilled after each claim.
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

    fn context(&self, tab: Option<Entity>) -> CommandBarPromptContext {
        let Some(tab) = tab else {
            return default();
        };
        let Ok((tab, workspace, worktree)) = self.tabs.get(tab) else {
            return default();
        };
        let cwd = tab
            .startup_dir
            .clone()
            .or_else(|| {
                workspace
                    .as_ref()
                    .map(|workspace| workspace.project_dir.clone())
            })
            .unwrap_or_default();
        if cwd.is_empty() {
            return default();
        }
        let path = std::path::Path::new(&cwd);
        let info = vmux_git::worktree::repo_info(path);
        CommandBarPromptContext {
            workspace_name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| cwd.clone()),
            cwd,
            is_git_repo: info.is_some(),
            is_worktree: info.as_ref().is_some_and(|info| info.is_worktree),
            branch: info
                .as_ref()
                .map(|info| info.branch.clone())
                .unwrap_or_default(),
            base_ref: worktree
                .as_ref()
                .map(|worktree| worktree.base_ref.clone())
                .unwrap_or_default(),
            uncommitted: info.as_ref().map(|info| info.uncommitted).unwrap_or(0),
            ahead: info.as_ref().map(|info| info.ahead).unwrap_or(0),
        }
    }
}

/// Bevy plugin for `vmux://start/`: spawns the page manifest, keeps a warm pool of
/// prewarmed launcher webviews, claims start page-open tasks (reusing a warm spare when
/// available), and answers [`StartDataRequest`] with the shared command-bar payload.
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
    let initial_dir = std::path::PathBuf::from(&trigger.event().payload.current_dir)
        .canonicalize()
        .ok()
        .filter(|path| path.is_dir())
        .or_else(|| std::env::current_dir().ok().filter(|path| path.is_dir()))
        .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from))
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| std::path::PathBuf::from("/"));
    let task = IoTaskPool::get().spawn(async move {
        let selected = rfd::AsyncFileDialog::new()
            .set_title("Create or select workspace")
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
                            "This workspace is not a Git repository. Initialize Git now?",
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
    agents_snapshot: Res<CommandBarAgentsSnapshot>,
    pages_snapshot: Res<CommandBarPagesSnapshot>,
    work_snapshot: Res<CommandBarWorkSnapshot>,
    focused: Res<crate::stack::FocusedStack>,
    starts: Query<
        (
            Entity,
            &WebviewSource,
            Has<StartWorkSynced>,
            Has<CefKeyboardTarget>,
        ),
        Without<crate::start::StartAgentTransitionView>,
    >,
    added_keyboard_targets: Query<(), Added<CefKeyboardTarget>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let focus_changed = focused.is_changed();
    let changed = should_refresh_start_payload(
        spaces_snapshot.is_changed(),
        agents_snapshot.is_changed(),
        pages_snapshot.is_changed(),
        work_snapshot.is_changed(),
        focus_changed,
    ) || prompt_context.changed(tab_gather.active_tab.get());
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
        &agents_snapshot,
        &pages_snapshot,
        &work_snapshot,
        &prompt_context,
    );
    for (e, focus_requested) in targets {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            e,
            COMMAND_BAR_OPEN_EVENT,
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
    agents_changed: bool,
    pages_changed: bool,
    work_changed: bool,
    focus_changed: bool,
) -> bool {
    spaces_changed || agents_changed || pages_changed || work_changed || focus_changed
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
    agents_snapshot: Res<CommandBarAgentsSnapshot>,
    pages_snapshot: Res<CommandBarPagesSnapshot>,
    work_snapshot: Res<CommandBarWorkSnapshot>,
    mut commands: Commands,
) {
    for ev in revealed.read() {
        let payload = build_start_payload(
            &tab_gather,
            &spaces_snapshot,
            &agents_snapshot,
            &pages_snapshot,
            &work_snapshot,
            &prompt_context,
        );
        commands.trigger(BinHostEmitEvent::from_rkyv(
            ev.webview,
            COMMAND_BAR_OPEN_EVENT,
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
    agents_snapshot: Res<CommandBarAgentsSnapshot>,
    pages_snapshot: Res<CommandBarPagesSnapshot>,
    work_snapshot: Res<CommandBarWorkSnapshot>,
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
        &agents_snapshot,
        &pages_snapshot,
        &work_snapshot,
        &prompt_context,
    );
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        COMMAND_BAR_OPEN_EVENT,
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
    agents_snapshot: &CommandBarAgentsSnapshot,
    pages_snapshot: &CommandBarPagesSnapshot,
    work_snapshot: &CommandBarWorkSnapshot,
    prompt_context: &StartPromptContextParams,
) -> CommandBarOpenEvent {
    let active_stack_count = tab_gather.stack_q.iter().count();
    let space_name = spaces_snapshot.active_space_name.clone();
    let tabs = gather_command_bar_tabs(
        tab_gather.active_tab.get(),
        &tab_gather.all_children,
        &tab_gather.leaf_panes,
        &tab_gather.pane_ts,
        &tab_gather.pane_children,
        &tab_gather.stack_ts,
        &tab_gather.stack_q,
        &tab_gather.browser_meta,
        &tab_gather.child_of_q,
        &space_name,
    );
    let mut payload = build_command_bar_open_payload(
        0,
        false,
        space_name,
        String::new(),
        spaces_snapshot,
        agents_snapshot,
        pages_snapshot,
        work_snapshot,
        active_stack_count,
        tabs,
        Some(OpenTarget::InPlace),
    );
    payload.prompt_context = prompt_context.context(tab_gather.active_tab.get());
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
mod tests {
    use super::*;
    use bevy_cef::prelude::BinReceive;
    use vmux_core::PageOpenId;
    use vmux_core::page::PageManifest;

    #[derive(Resource, Default)]
    struct EmittedIds(Vec<String>);

    fn capture_emit(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<EmittedIds>) {
        emitted.0.push(trigger.id.clone());
    }

    fn start_ready_app() -> App {
        let mut app = App::new();
        app.init_resource::<CommandBarSpacesSnapshot>()
            .init_resource::<CommandBarAgentsSnapshot>()
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
        assert!(crate::start::supports_inline_agent_transition(
            "vmux://agent/codex"
        ));
        assert!(crate::start::supports_inline_agent_transition(
            "vmux://agent/openai/gpt-5/session"
        ));
        assert!(!crate::start::supports_inline_agent_transition(
            "vmux://agent/codex/cli"
        ));
        assert!(!crate::start::supports_inline_agent_transition(
            "vmux://agent/vibe/setup"
        ));
        assert!(crate::start::supports_inline_agent_transition(
            "vmux://agent/cliff"
        ));
        assert!(crate::start::supports_inline_agent_transition(
            "vmux://agent/setupwizard"
        ));
    }

    #[test]
    fn page_mount_does_not_start_focus_retry() {
        let source = include_str!("page.rs");
        let setup_effect = source
            .split_once("use_effect(|| {")
            .expect("start page setup effect")
            .1
            .split_once("});")
            .expect("end of start page setup effect")
            .0;

        assert!(setup_effect.contains("install_window_focus_refocus();"));
        assert!(setup_effect.contains("install_keep_input_focused_on_click();"));
        assert!(!setup_effect.contains("focus_start_input();"));
    }

    #[test]
    fn cold_start_focuses_after_page_ready() {
        let mut app = start_ready_app();
        let webview = app.world_mut().spawn(CefKeyboardTarget).id();

        emit_start_ready(&mut app, webview);

        let emitted = &app.world().resource::<EmittedIds>().0;
        assert_eq!(emitted, &[COMMAND_BAR_OPEN_EVENT, START_FOCUS_INPUT_EVENT]);
    }

    #[test]
    fn warm_start_waits_for_reveal_before_focusing() {
        let mut app = start_ready_app();
        let webview = app.world_mut().spawn(WarmStartSpare).id();

        emit_start_ready(&mut app, webview);

        assert!(app.world().get::<WarmStartReady>(webview).is_some());
        let emitted = &app.world().resource::<EmittedIds>().0;
        assert_eq!(emitted, &[COMMAND_BAR_OPEN_EVENT]);
    }

    #[test]
    fn inactive_cold_start_waits_for_activation_before_focusing() {
        let mut app = start_ready_app();
        let webview = app.world_mut().spawn_empty().id();

        emit_start_ready(&mut app, webview);

        let emitted = &app.world().resource::<EmittedIds>().0;
        assert_eq!(emitted, &[COMMAND_BAR_OPEN_EVENT]);
    }

    #[test]
    fn start_sync_focuses_only_active_pages_on_first_sync_or_activation() {
        assert!(!should_focus_start_sync(false, false, false, false));
        assert!(should_focus_start_sync(false, true, false, false));
        assert!(should_focus_start_sync(true, true, true, false));
        assert!(should_focus_start_sync(true, true, false, true));
        assert!(!should_focus_start_sync(true, true, false, false));
    }

    #[test]
    fn start_sync_refreshes_when_agent_recency_changes() {
        assert!(should_refresh_start_payload(
            false, true, false, false, false
        ));
        assert!(!should_refresh_start_payload(
            false, false, false, false, false
        ));
    }

    fn start_task(stack: Entity) -> PageOpenTask {
        PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: START_PAGE_URL.to_string(),
            request_id: None,
        }
    }

    #[test]
    fn warm_claim_reuses_spare() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<CefPageAttachRequest>()
            .add_message::<StartSpareRevealed>()
            .add_systems(Update, handle_start_page_open);
        let stack = app.world_mut().spawn_empty().id();
        let spare = app.world_mut().spawn((WarmStartSpare, WarmStartReady)).id();
        let task = app.world_mut().spawn(start_task(stack)).id();
        app.update();

        assert_eq!(
            app.world().get::<ChildOf>(spare).map(|c| c.parent()),
            Some(stack),
            "spare reparented into the target stack"
        );
        assert!(
            app.world().get::<WarmStartSpare>(spare).is_none(),
            "spare marker removed on claim"
        );
        let meta = app
            .world()
            .get::<PageMetadata>(stack)
            .expect("stack received start metadata");
        assert_eq!(meta.url, START_PAGE_URL);
        assert!(app.world().get::<PageOpenHandled>(task).is_some());

        let attaches = app
            .world_mut()
            .resource_mut::<Messages<CefPageAttachRequest>>()
            .drain()
            .count();
        assert_eq!(attaches, 0, "warm claim must not spawn a cold webview");
        let reveals: Vec<StartSpareRevealed> = app
            .world_mut()
            .resource_mut::<Messages<StartSpareRevealed>>()
            .drain()
            .collect();
        assert_eq!(reveals.len(), 1);
        assert_eq!(reveals[0].webview, spare);
    }

    #[test]
    fn not_ready_spare_is_not_claimed() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<CefPageAttachRequest>()
            .add_message::<StartSpareRevealed>()
            .add_systems(Update, handle_start_page_open);
        let stack = app.world_mut().spawn_empty().id();
        let spare = app.world_mut().spawn(WarmStartSpare).id();
        let task = app.world_mut().spawn(start_task(stack)).id();
        app.update();

        assert!(
            app.world().get::<ChildOf>(spare).is_none(),
            "an unready spare must not be reparented"
        );
        assert!(
            app.world().get::<WarmStartSpare>(spare).is_some(),
            "an unready spare stays in the pool"
        );
        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        let attaches = app
            .world_mut()
            .resource_mut::<Messages<CefPageAttachRequest>>()
            .drain()
            .count();
        assert_eq!(attaches, 1, "unready spare falls back to the cold path");
    }

    #[test]
    fn cold_fallback_when_pool_empty() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<CefPageAttachRequest>()
            .add_message::<StartSpareRevealed>()
            .add_systems(Update, handle_start_page_open);
        let stack = app.world_mut().spawn_empty().id();
        let task = app.world_mut().spawn(start_task(stack)).id();
        app.update();

        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        let attaches: Vec<CefPageAttachRequest> = app
            .world_mut()
            .resource_mut::<Messages<CefPageAttachRequest>>()
            .drain()
            .collect();
        assert_eq!(attaches.len(), 1);
        assert_eq!(attaches[0].url, START_PAGE_URL);
        let reveals = app
            .world_mut()
            .resource_mut::<Messages<StartSpareRevealed>>()
            .drain()
            .count();
        assert_eq!(reveals, 0, "cold fallback emits no reveal");
    }

    #[test]
    fn pool_fills_to_target() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, maintain_warm_start_pool);
        app.world_mut().spawn(VmuxWindow);
        app.update();
        app.update();

        let count = app
            .world_mut()
            .query_filtered::<(), With<WarmStartSpare>>()
            .iter(app.world())
            .count();
        assert_eq!(count, WARM_START_POOL_SIZE);
    }
}
