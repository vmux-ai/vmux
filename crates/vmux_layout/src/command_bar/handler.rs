pub(crate) use crate::NewStackContext;
use crate::cef::Browser;
use crate::command_bar::work_snapshot::{update_recent_files_snapshot, update_work_dirs_snapshot};
use crate::start::event::{START_FOCUS_INPUT_EVENT, StartFocusInput};
use crate::{
    Header,
    pane::{Pane, PaneSplit},
    side_sheet::SideSheet,
    stack::{ActiveTabParam, Stack, collect_leaf_panes, focused_stack},
    tab::Tab,
    window::{Main, Modal},
};
use bevy::{
    ecs::message::MessageReader, ecs::relationship::Relationship, ecs::system::SystemParam,
    picking::Pickable, prelude::*, ui::UiSystems, window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::{RenderTextureMessage, webview_debug_log};
use vmux_command::event::{
    COMMAND_BAR_OPEN_EVENT, CommandBarActionEvent, CommandBarCommandEntry, CommandBarOpenEvent,
    CommandBarPage, CommandBarReadyEvent, CommandBarRenderedEvent, CommandBarSizeEvent,
    CommandBarSpace, CommandBarTab, PATH_COMPLETE_RESPONSE, PathCompleteRequest,
    PathCompleteResponse, PathEntry,
};
use vmux_command::open::OpenCommand;
use vmux_command::open_target::OpenTarget;
use vmux_command::snapshot::{
    AgentPromptTarget, CommandBarAgentsSnapshot, CommandBarPagesSnapshot, CommandBarSpacesSnapshot,
    CommandBarTerminalsSnapshot, WriteCommandBarSnapshots,
};
use vmux_command::{
    AppCommand, BrowserBarCommand, BrowserCommand, LayoutCommand, PaneCommand, ReadAppCommands,
    SpaceCommand, StackCommand,
};
use vmux_core::agent::{PageAgentAttachRequest, PageAgentSpawnStackRequest, PendingAgentPrompt};
use vmux_core::event::space::SpaceCommandEvent;
use vmux_core::page::{SettingsPageSpawnRequest, SpacesPageSpawnRequest};
use vmux_core::terminal::{ProcessesMonitorSpawnRequest, Terminal, TerminalSpawnRequest};
use vmux_core::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_history::{LastActivatedAt, now_millis};

pub(crate) use vmux_core::focus_pane_entity;

pub(crate) fn parse_pid_from_url(url: &str, terminal_page_url: &str) -> Option<u32> {
    let suffix = url.strip_prefix(terminal_page_url)?;
    if suffix.is_empty() {
        return None;
    }
    suffix.parse::<u32>().ok()
}

#[derive(Component)]
struct CommandBarReady;

#[derive(Component)]
struct CommandBarRenderedOpen(u64);

#[derive(Component)]
struct CommandBarPaintedOpen(u64);

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CommandBarNativeSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Component)]
pub struct PendingCommandBarReveal {
    frames: u8,
    open_id: u64,
    payload: Option<Vec<u8>>,
}

impl PendingCommandBarReveal {
    /// True once a real open is in flight (open_id != 0). The prewarm placeholder
    /// (open_id == 0) is idle and must not keep the event loop awake.
    pub fn is_active(&self) -> bool {
        self.open_id != 0
    }
}

const COMMAND_BAR_REVEAL_FRAMES: u8 = 2;
const COMMAND_BAR_REVEAL_FALLBACK_FRAMES: u8 = 10;
const COMMAND_BAR_NATIVE_REVEAL_TIMEOUT_FRAMES: u8 = 120;

pub(crate) struct CommandBarInputPlugin;

impl Plugin for CommandBarInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NewStackContext>()
            .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
            .add_message::<PageAgentAttachRequest>()
            .add_message::<PageAgentSpawnStackRequest>()
            .add_message::<vmux_core::agent::PageAgentSpawnDefaultRequest>()
            .add_message::<vmux_core::agent::PageAgentAttachDefaultRequest>()
            .add_message::<SettingsPageSpawnRequest>()
            .add_message::<SpacesPageSpawnRequest>()
            .add_plugins(BinEventEmitterPlugin::<(
                CommandBarActionEvent,
                PathCompleteRequest,
                CommandBarReadyEvent,
                CommandBarRenderedEvent,
                CommandBarSizeEvent,
            )>::for_hosts(&["command-bar", "start"]))
            .add_observer(on_command_bar_action)
            .add_observer(on_path_complete_request)
            .add_observer(on_command_bar_ready)
            .add_observer(on_command_bar_rendered)
            .add_observer(on_command_bar_size)
            .add_systems(
                Update,
                prewarm_command_bar_modal.before(CefSystems::CreateAndResize),
            )
            .add_systems(
                Update,
                handle_open_command_bar
                    .in_set(ReadAppCommands)
                    .after(prewarm_command_bar_modal)
                    .after(crate::tab::TabCommandSet)
                    .after(crate::stack::StackCommandSet),
            )
            .add_systems(
                Update,
                retry_pending_command_bar_open.after(handle_open_command_bar),
            )
            .add_systems(
                Update,
                (update_work_dirs_snapshot, update_recent_files_snapshot)
                    .in_set(WriteCommandBarSnapshots),
            )
            .add_systems(
                Update,
                deferred_dismiss_modal
                    .after(ReadAppCommands)
                    .before(crate::stack::ComputeFocusSet),
            )
            .add_systems(
                PostUpdate,
                (mark_command_bar_painted, reveal_command_bar)
                    .chain()
                    .after(UiSystems::Layout),
            );
    }
}

pub struct CommandBarEntry {
    pub id: String,
    pub name: String,
    pub shortcut: String,
}

pub struct AppAgentEntry {
    pub id: String,
    pub name: String,
}

pub fn app_agent_id(provider: &str, model: &str) -> String {
    format!("app_{provider}_{model}_new")
}

pub fn parse_app_agent_id(id: &str) -> Option<(String, String)> {
    let body = id.strip_prefix("app_")?.strip_suffix("_new")?;
    let parts: Vec<&str> = body.splitn(2, '_').collect();
    if parts.len() != 2 {
        return None;
    }
    Some((parts[0].to_string(), parts[1].to_string()))
}

/// Command ids surfaced through a page entry instead of a command row: the
/// Services page (vmux://services/) replaces "Open Service Monitor", and the
/// History page shows the History shortcut. Their menu items + shortcuts stay.
const COMMAND_BAR_SKIP_IDS: &[&str] = &["service_open", "browser_open_history"];

pub fn command_list(app_agent_entries: Vec<AppAgentEntry>) -> Vec<CommandBarEntry> {
    let mut entries: Vec<CommandBarEntry> = AppCommand::command_bar_entries()
        .into_iter()
        .filter(|(id, _, _)| !COMMAND_BAR_SKIP_IDS.contains(id))
        .map(|(id, name, shortcut)| CommandBarEntry {
            id: id.to_string(),
            name,
            shortcut: shortcut.to_string(),
        })
        .collect();
    entries.extend(app_agent_entries.into_iter().map(|entry| CommandBarEntry {
        id: entry.id,
        name: entry.name,
        shortcut: String::new(),
    }));
    entries
}

/// Display string for a command's shortcut, looked up by menu id. Used to show
/// a page's keybinding (e.g. History) on its page entry after the command itself
/// is hidden from the command list.
fn command_shortcut(id: &str) -> String {
    AppCommand::command_bar_entries()
        .into_iter()
        .find(|(entry_id, _, _)| *entry_id == id)
        .map(|(_, _, shortcut)| shortcut.to_string())
        .unwrap_or_default()
}

/// Launcher entries for installed ACP and CLI agents, most recently used first.
fn agent_pages(agents_snapshot: &CommandBarAgentsSnapshot) -> Vec<CommandBarPage> {
    let mut pages: Vec<CommandBarPage> = agents_snapshot
        .acp
        .iter()
        .map(|agent| CommandBarPage {
            host: "agent".to_string(),
            url: agent.url.clone(),
            title: agent.name.clone(),
            keywords: vec![agent.id.clone(), "acp".to_string(), "agent".to_string()],
            icon: if agent.icon.is_empty() {
                vmux_core::PageIcon::None
            } else {
                vmux_core::PageIcon::Favicon(agent.icon.clone())
            },
            shortcut: String::new(),
        })
        .collect();
    pages.extend(
        agents_snapshot
            .providers
            .iter()
            .map(|agent| CommandBarPage {
                host: "agent".to_string(),
                url: agent.url.clone(),
                title: format!("{} (CLI)", agent.name),
                keywords: vec![agent.id.clone(), "cli".to_string(), "agent".to_string()],
                icon: vmux_core::PageIcon::None,
                shortcut: String::new(),
            }),
    );
    let recent_rank: std::collections::HashMap<String, usize> = agents_snapshot
        .recent
        .iter()
        .enumerate()
        .map(|(rank, target)| {
            let url = match target {
                AgentPromptTarget::Cli(kind) => format!("{}cli", kind.cli_url_prefix()),
                AgentPromptTarget::Acp { id } => format!("vmux://agent/{id}"),
            };
            (url, rank)
        })
        .collect();
    pages.sort_by(|a, b| {
        recent_rank
            .get(&a.url)
            .copied()
            .unwrap_or(usize::MAX)
            .cmp(&recent_rank.get(&b.url).copied().unwrap_or(usize::MAX))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    pages
}

pub fn match_command(id: &str) -> Option<AppCommand> {
    AppCommand::from_menu_id(id)
}

pub fn is_command_bar_open(modal_q: &Query<(&Node, Has<CefKeyboardTarget>), With<Modal>>) -> bool {
    modal_q
        .iter()
        .any(|(n, has_keyboard_target)| command_bar_modal_is_open(n.display, has_keyboard_target))
}

fn command_bar_modal_is_open(display: Display, has_keyboard_target: bool) -> bool {
    display != Display::None && has_keyboard_target
}

fn command_bar_modal_is_visible(
    display: Display,
    visibility: Visibility,
    has_keyboard_target: bool,
) -> bool {
    display != Display::None && visibility != Visibility::Hidden && has_keyboard_target
}

fn prewarm_command_bar_modal(
    mut commands: Commands,
    mut modal_q: Query<
        (
            Entity,
            &mut Node,
            &mut Visibility,
            Has<CefKeyboardTarget>,
            Has<PendingCommandBarReveal>,
        ),
        With<Modal>,
    >,
) {
    let Ok((modal_e, mut modal_node, mut modal_vis, has_keyboard_target, pending_reveal)) =
        modal_q.single_mut()
    else {
        return;
    };
    if has_keyboard_target || pending_reveal {
        return;
    }
    modal_node.display = Display::Flex;
    *modal_vis = Visibility::Hidden;
    commands
        .entity(modal_e)
        .insert(Pickable::IGNORE)
        .insert(PendingCommandBarReveal {
            frames: 0,
            open_id: 0,
            payload: None,
        });
}

fn command_bar_open_delivery_ready(
    has_browser: bool,
    host_emit_ready: bool,
    _command_bar_ready: bool,
) -> bool {
    has_browser && host_emit_ready
}

fn command_bar_reveal_ready(
    has_browser: bool,
    _host_emit_ready: bool,
    _command_bar_ready: bool,
    rendered_open: bool,
) -> bool {
    has_browser && rendered_open
}

fn next_command_bar_reveal_frames(
    frames: u8,
    open_id: u64,
    rendered_open_id: Option<u64>,
    _painted_open_id: Option<u64>,
) -> Option<u8> {
    if open_id == 0 {
        return Some(frames);
    }
    if rendered_open_id != Some(open_id) {
        if frames >= COMMAND_BAR_REVEAL_FALLBACK_FRAMES {
            return None;
        }
        return Some(frames + 1);
    }
    if frames >= COMMAND_BAR_REVEAL_FRAMES {
        None
    } else {
        Some(frames + 1)
    }
}

fn next_command_bar_reveal_frames_for_backend(
    native_windowed: bool,
    frames: u8,
    open_id: u64,
    rendered_open_id: Option<u64>,
    painted_open_id: Option<u64>,
    has_native_size: bool,
) -> Option<u8> {
    if native_windowed && open_id != 0 && (rendered_open_id != Some(open_id) || !has_native_size) {
        return Some(frames.saturating_add(1));
    }
    next_command_bar_reveal_frames(frames, open_id, rendered_open_id, painted_open_id)
}

fn native_command_bar_reveal_timed_out(
    native_windowed: bool,
    frames: u8,
    open_id: u64,
    rendered_open_id: Option<u64>,
    has_native_size: bool,
) -> bool {
    native_windowed
        && open_id != 0
        && frames >= COMMAND_BAR_NATIVE_REVEAL_TIMEOUT_FRAMES
        && (rendered_open_id != Some(open_id) || !has_native_size)
}

fn command_bar_reveal_start_frames(was_prewarmed: bool) -> u8 {
    if was_prewarmed {
        COMMAND_BAR_REVEAL_FRAMES
    } else {
        0
    }
}

fn should_start_command_bar_reveal(
    has_browser: bool,
    host_emit_ready: bool,
    command_bar_ready: bool,
    rendered_open: bool,
    pending_reveal: bool,
    visibility: Visibility,
) -> bool {
    command_bar_reveal_ready(
        has_browser,
        host_emit_ready,
        command_bar_ready,
        rendered_open,
    ) && !pending_reveal
        && visibility == Visibility::Hidden
}

fn should_retry_command_bar_open_payload(
    open_id: u64,
    payload: Option<&[u8]>,
    rendered_open_id: Option<u64>,
) -> bool {
    open_id != 0 && payload.is_some() && rendered_open_id != Some(open_id)
}

fn should_requeue_command_bar_open_after_emit(_command_bar_ready: bool) -> bool {
    false
}

fn on_command_bar_ready(trigger: On<BinReceive<CommandBarReadyEvent>>, mut commands: Commands) {
    webview_debug_log(format!(
        "command_bar ready entity={:?}",
        trigger.event().webview
    ));
    commands
        .entity(trigger.event().webview)
        .insert(CommandBarReady);
}

fn on_command_bar_rendered(
    trigger: On<BinReceive<CommandBarRenderedEvent>>,
    mut commands: Commands,
) {
    webview_debug_log(format!(
        "command_bar rendered entity={:?} open_id={}",
        trigger.event().webview,
        trigger.event().payload.open_id
    ));
    commands
        .entity(trigger.event().webview)
        .insert(CommandBarRenderedOpen(trigger.event().payload.open_id));
}

fn on_command_bar_size(
    trigger: On<BinReceive<CommandBarSizeEvent>>,
    state: Query<(&Visibility, Option<&PendingCommandBarReveal>)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if let Ok((visibility, pending_reveal)) = state.get(webview)
        && !command_bar_size_should_apply(*visibility, pending_reveal)
    {
        webview_debug_log(format!(
            "command_bar size ignored entity={webview:?} visibility={visibility:?} pending={}",
            pending_reveal.is_some()
        ));
        return;
    }
    let payload = trigger.event().payload;
    webview_debug_log(format!(
        "command_bar size entity={webview:?} width={} height={}",
        payload.width, payload.height
    ));
    commands.entity(webview).insert(CommandBarNativeSize {
        width: payload.width.max(1) as f32,
        height: payload.height.max(1) as f32,
    });
}

fn command_bar_size_should_apply(
    visibility: Visibility,
    pending_reveal: Option<&PendingCommandBarReveal>,
) -> bool {
    visibility != Visibility::Hidden
        || pending_reveal.is_some_and(|pending| pending.open_id != 0 && pending.payload.is_some())
}

#[derive(Default)]
struct CommandBarOpenRequest {
    should_toggle: bool,
    should_dismiss: bool,
    should_dismiss_nav: bool,
    replace_active_stack: bool,
    url_override: Option<String>,
    space_switch: bool,
}

fn command_bar_open_request(
    commands: impl IntoIterator<Item = AppCommand>,
) -> CommandBarOpenRequest {
    let mut request = CommandBarOpenRequest::default();
    for cmd in commands {
        match cmd {
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar)) => {
                request.should_toggle = true;
                request.url_override = Some(String::new());
            }
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenPageInCommandBar)) => {
                request.should_toggle = true;
                request.replace_active_stack = true;
            }
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenPathBar)) => {
                request.should_toggle = true;
                request.url_override = Some("/".to_string());
            }
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommands)) => {
                request.should_toggle = true;
                request.url_override = Some(">".to_string());
            }
            AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open)) => {
                request.should_toggle = true;
                request.space_switch = true;
                request.url_override = Some(String::new());
            }
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close)) => {
                request.should_dismiss = true;
            }
            AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Next | StackCommand::Previous,
            ))
            | AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectLeft
                | PaneCommand::SelectRight
                | PaneCommand::SelectUp
                | PaneCommand::SelectDown,
            )) => {
                request.should_dismiss_nav = true;
            }
            _ => {}
        }
    }
    request
}

fn pending_stack_startup_url_request(
    new_stack_ctx: &mut NewStackContext,
    startup_url: Option<&str>,
) -> Option<PageOpenRequest> {
    if !new_stack_ctx.needs_open {
        return None;
    }
    let stack = new_stack_ctx.stack?;
    let url = startup_url.filter(|url| !url.is_empty())?;
    new_stack_ctx.stack = None;
    new_stack_ctx.previous_stack = None;
    new_stack_ctx.needs_open = false;
    Some(PageOpenRequest {
        target: PageOpenTarget::Stack(stack),
        url: url.to_string(),
        request_id: None,
    })
}

fn command_bar_should_open_pending_stack(
    new_stack_ctx: &mut NewStackContext,
    explicit_toggle: bool,
) -> bool {
    if explicit_toggle {
        new_stack_ctx.needs_open = false;
        return false;
    }
    if new_stack_ctx.needs_open {
        new_stack_ctx.needs_open = false;
        true
    } else {
        false
    }
}

fn command_bar_cancel_pending_stack_for_active_open(
    new_stack_ctx: &mut NewStackContext,
    replace_active_stack: bool,
) -> Option<(Entity, Option<Entity>)> {
    if !replace_active_stack {
        return None;
    }
    new_stack_ctx.needs_open = false;
    let previous_stack = new_stack_ctx.previous_stack.take();
    let stack = new_stack_ctx.stack.take()?;
    Some((stack, previous_stack))
}

/// Whether an open should focus the `vmux://start/` launcher input instead of opening
/// the modal command bar. True only for a normal open (not a new-stack open) whose active
/// page is the start launcher. A space-switch open (`<leader> s`) always uses the modal so
/// the switcher behaves the same on the start page as everywhere else.
fn command_bar_should_focus_start(
    is_new_stack: bool,
    space_switch: bool,
    active_page_is_start: bool,
) -> bool {
    !is_new_stack && !space_switch && active_page_is_start
}

/// Whether a toggle-style open should (re)open the modal command bar: when it is currently
/// closed, or always for a space-switch open so `<leader> s` re-drives space-switch mode
/// even when the command bar is already visible (rather than no-opping).
fn command_bar_toggle_should_open(is_open: bool, space_switch: bool) -> bool {
    !is_open || space_switch
}

fn handle_open_command_bar(
    mut reader: MessageReader<AppCommand>,
    mut modal_q: Query<
        (
            Entity,
            &mut Node,
            &mut Visibility,
            Has<CefKeyboardTarget>,
            Has<CommandBarReady>,
            Option<&CommandBarRenderedOpen>,
            Option<&PendingCommandBarReveal>,
            Has<WebviewWindowed>,
        ),
        With<Modal>,
    >,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
    browsers: NonSend<Browsers>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_q: Query<Entity, With<Stack>>,
    browser_meta: Query<&PageMetadata, With<Browser>>,
    child_of_q: Query<&ChildOf>,
    content_browsers: Query<
        Entity,
        (
            With<Browser>,
            Without<Header>,
            Without<SideSheet>,
            Without<Modal>,
        ),
    >,
    mut snapshot_params: ParamSet<(
        Res<CommandBarAgentsSnapshot>,
        Res<CommandBarSpacesSnapshot>,
        ResMut<NewStackContext>,
        Option<Res<crate::settings::EffectiveStartupUrl>>,
        MessageWriter<PageOpenRequest>,
        Res<CommandBarPagesSnapshot>,
        Res<vmux_command::snapshot::CommandBarWorkSnapshot>,
    )>,
    mut commands: Commands,
) {
    let active_stack_count = stack_q.iter().count();
    let spaces_snapshot = snapshot_params.p1().clone();
    let space_name = spaces_snapshot.active_space_name.clone();
    let agents_snap = snapshot_params.p0().clone();
    let startup_url = snapshot_params.p3().map(|url| url.0.clone());
    let pages_snap = snapshot_params.p5().clone();
    let work_snap = snapshot_params.p6().clone();

    let request = command_bar_open_request(reader.read().cloned());
    let mut should_open = false;
    let should_toggle = request.should_toggle;
    let should_dismiss = request.should_dismiss;
    let should_dismiss_nav = request.should_dismiss_nav;
    let replace_active_stack = request.replace_active_stack;
    let url_override = request.url_override;
    let space_switch = request.space_switch;

    let mut active_stack_override = None;
    let canceled_pending_stack = {
        let mut new_stack_ctx = snapshot_params.p2();
        command_bar_cancel_pending_stack_for_active_open(&mut new_stack_ctx, replace_active_stack)
    };
    if let Some((stack, previous_stack)) = canceled_pending_stack {
        commands.entity(stack).despawn();
        if let Some(previous_stack) = previous_stack {
            active_stack_override = Some(previous_stack);
            focus_pane_entity(previous_stack, &mut commands, &child_of_q);
        }
    }

    if should_dismiss {
        let is_open = modal_q
            .single()
            .map(|(_, n, _, has_keyboard_target, _, _, _, _)| {
                command_bar_modal_is_open(n.display, has_keyboard_target)
            })
            .unwrap_or(false);
        if is_open {
            let Ok((modal_e, mut modal_node, mut modal_vis, _, _, _, _, _)) = modal_q.single_mut()
            else {
                return;
            };
            modal_node.display = Display::None;
            *modal_vis = Visibility::Hidden;
            commands
                .entity(modal_e)
                .insert(Pickable::IGNORE)
                .remove::<CefKeyboardTarget>()
                .remove::<CefPointerTarget>()
                .remove::<CommandBarRenderedOpen>()
                .remove::<CommandBarPaintedOpen>()
                .remove::<PendingCommandBarReveal>();
            let mut new_stack_ctx = snapshot_params.p2();
            // Discard empty tab created by a previous Cmd+T
            if let Some(stack_e) = new_stack_ctx.stack.take() {
                commands.entity(stack_e).despawn();
                if let Some(prev) = new_stack_ctx.previous_stack.take()
                    && let Ok(children) = all_children.get(prev)
                {
                    for child in children.iter() {
                        if content_browsers.contains(child) {
                            commands.entity(child).insert(CefKeyboardTarget);
                        }
                    }
                }
            } else {
                let (_, _, active_stack) = focused_stack(
                    active_tab_param.get(),
                    &all_children,
                    &leaf_panes,
                    &pane_ts,
                    &pane_children,
                    &stack_ts,
                );
                if let Some(tab) = active_stack {
                    for browser_e in &content_browsers {
                        let is_child = child_of_q
                            .get(browser_e)
                            .ok()
                            .map(|co| co.get() == tab)
                            .unwrap_or(false);
                        if is_child {
                            commands.entity(browser_e).insert(CefKeyboardTarget);
                        }
                    }
                }
            }
            new_stack_ctx.needs_open = false;
            return;
        }
    }

    // Navigation dismiss: close modal only, leave empty tab for
    // handle_tab_commands / on_pane_select to clean up.
    if should_dismiss_nav {
        let is_open = modal_q
            .single()
            .map(|(_, n, _, has_keyboard_target, _, _, _, _)| {
                command_bar_modal_is_open(n.display, has_keyboard_target)
            })
            .unwrap_or(false);
        if is_open {
            let Ok((modal_e, mut modal_node, mut modal_vis, _, _, _, _, _)) = modal_q.single_mut()
            else {
                return;
            };
            modal_node.display = Display::None;
            *modal_vis = Visibility::Hidden;
            commands
                .entity(modal_e)
                .insert(Pickable::IGNORE)
                .remove::<CefKeyboardTarget>()
                .remove::<CefPointerTarget>()
                .remove::<CommandBarRenderedOpen>()
                .remove::<CommandBarPaintedOpen>()
                .remove::<PendingCommandBarReveal>();
            let mut new_stack_ctx = snapshot_params.p2();
            new_stack_ctx.needs_open = false;
            return;
        }
    }

    let startup_request = {
        let mut new_stack_ctx = snapshot_params.p2();
        pending_stack_startup_url_request(&mut new_stack_ctx, startup_url.as_deref())
    };
    if let Some(request) = startup_request {
        snapshot_params.p4().write(request);
        return;
    }

    let should_open_pending_stack = {
        let mut new_stack_ctx = snapshot_params.p2();
        command_bar_should_open_pending_stack(&mut new_stack_ctx, should_toggle)
    };
    if should_open_pending_stack {
        should_open = true;
    }

    if should_toggle {
        let is_open = modal_q
            .single()
            .map(|(_, n, visibility, has_keyboard_target, _, _, _, _)| {
                command_bar_modal_is_visible(n.display, *visibility, has_keyboard_target)
            })
            .unwrap_or(false);
        if command_bar_toggle_should_open(is_open, space_switch) {
            should_open = true;
        }
        // If already open, do nothing — the shortcut should not close the bar.
        // Users can dismiss with Escape or click-outside.
    }

    if !should_open {
        return;
    }

    let is_new_stack = snapshot_params.p2().stack.is_some();

    if !is_new_stack {
        let active_stack = active_stack_override.or_else(|| {
            let (_, _, active_stack) = focused_stack(
                active_tab_param.get(),
                &all_children,
                &leaf_panes,
                &pane_ts,
                &pane_children,
                &stack_ts,
            );
            active_stack
        });
        let start_browser = active_stack.and_then(|stack| {
            all_children.get(stack).ok().and_then(|children| {
                children.iter().find_map(|e| {
                    browser_meta
                        .get(e)
                        .ok()
                        .filter(|meta| meta.url == crate::start::START_PAGE_URL)
                        .map(|_| e)
                })
            })
        });
        if command_bar_should_focus_start(is_new_stack, space_switch, start_browser.is_some())
            && let Some(browser_e) = start_browser
        {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                browser_e,
                START_FOCUS_INPUT_EVENT,
                &StartFocusInput,
            ));
            return;
        }
    }

    let Ok((
        modal_e,
        mut modal_node,
        mut modal_vis,
        has_keyboard_target,
        command_bar_ready,
        rendered_open,
        modal_pending_reveal,
        native_windowed,
    )) = modal_q.single_mut()
    else {
        return;
    };

    let was_open = command_bar_modal_is_open(modal_node.display, has_keyboard_target);

    if !was_open {
        // Open command bar — keep hidden until CEF resizes (see reveal_command_bar)
        modal_node.display = Display::Flex;
        *modal_vis = Visibility::Hidden;
    }

    if !was_open {
        for browser_e in &content_browsers {
            commands.entity(browser_e).remove::<CefKeyboardTarget>();
        }
    }
    commands
        .entity(modal_e)
        .insert(Pickable::default())
        .insert(CefKeyboardTarget)
        .insert(CefPointerTarget)
        .remove::<CommandBarNativeSize>();

    // Command bar is a CEF webview — allow keyboard forwarding
    suppress.0 = false;

    // Gather current URL (empty for new tab mode)
    let current_url = if let Some(override_url) = url_override {
        override_url
    } else if is_new_stack {
        String::new()
    } else {
        let active_stack = active_stack_override.or_else(|| {
            let (_, _, active_stack) = focused_stack(
                active_tab_param.get(),
                &all_children,
                &leaf_panes,
                &pane_ts,
                &pane_children,
                &stack_ts,
            );
            active_stack
        });
        active_stack
            .and_then(|tab| {
                let Ok(children) = all_children.get(tab) else {
                    return None;
                };
                children.iter().find_map(|e| browser_meta.get(e).ok())
            })
            .map(|meta| meta.url.clone())
            .unwrap_or_default()
    };

    let bar_tabs = gather_command_bar_tabs(
        active_tab_param.get(),
        &all_children,
        &leaf_panes,
        &pane_ts,
        &pane_children,
        &stack_ts,
        &stack_q,
        &browser_meta,
        &child_of_q,
        &space_name,
    );

    let has_browser = browsers.has_browser(modal_e);
    let host_emit_ready = browsers.host_emit_ready(&modal_e);
    let rendered_matches = rendered_open.is_some_and(|rendered| rendered.0 != 0);
    webview_debug_log(format!(
        "command_bar open entity={modal_e:?} was_open={was_open} has_browser={has_browser} host_emit_ready={host_emit_ready} command_bar_ready={command_bar_ready} rendered={rendered_matches} pending_reveal={} visibility={:?} is_new_stack={is_new_stack}",
        modal_pending_reveal.is_some(),
        *modal_vis
    ));

    if !command_bar_open_delivery_ready(has_browser, host_emit_ready, command_bar_ready) {
        commands
            .entity(modal_e)
            .remove::<CommandBarPaintedOpen>()
            .insert(PendingCommandBarReveal {
                frames: 0,
                open_id: 0,
                payload: None,
            });
        snapshot_params.p2().needs_open = true;
        return;
    }

    let open_id = now_millis() as u64;
    let reveal_start_frames = command_bar_reveal_start_frames(
        modal_pending_reveal.is_some_and(|pending| pending.open_id == 0),
    );
    let target = if replace_active_stack {
        Some(vmux_command::open_target::OpenTarget::InPlace)
    } else if is_new_stack {
        Some(vmux_command::open_target::OpenTarget::InNewStack)
    } else {
        None
    };
    let mut payload = build_command_bar_open_payload(
        open_id,
        native_windowed,
        space_name,
        current_url,
        &spaces_snapshot,
        &agents_snap,
        &pages_snap,
        &work_snap,
        active_stack_count,
        bar_tabs,
        target,
    );
    payload.space_switch = space_switch;
    let event = BinHostEmitEvent::from_rkyv(modal_e, COMMAND_BAR_OPEN_EVENT, &payload);
    let payload_bytes = event.payload.clone();
    let payload_bytes_len = payload_bytes.len();
    commands.trigger(event);
    if should_start_command_bar_reveal(
        has_browser,
        host_emit_ready,
        command_bar_ready,
        rendered_open.is_some_and(|rendered| rendered.0 == open_id),
        modal_pending_reveal.is_some(),
        *modal_vis,
    ) {
        commands
            .entity(modal_e)
            .remove::<CommandBarPaintedOpen>()
            .insert(PendingCommandBarReveal {
                frames: reveal_start_frames,
                open_id,
                payload: Some(payload_bytes.clone()),
            });
    } else {
        commands
            .entity(modal_e)
            .remove::<CommandBarRenderedOpen>()
            .remove::<CommandBarPaintedOpen>()
            .insert(PendingCommandBarReveal {
                frames: reveal_start_frames,
                open_id,
                payload: Some(payload_bytes),
            });
    }
    webview_debug_log(format!(
        "command_bar emit open entity={modal_e:?} payload_len={} tabs={} commands={}",
        payload_bytes_len,
        payload.tabs.len(),
        payload.commands.len()
    ));
    if should_requeue_command_bar_open_after_emit(command_bar_ready) {
        snapshot_params.p2().needs_open = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn command_bar_open_payload(
    open_id: u64,
    native_windowed: bool,
    space_name: String,
    url: String,
    spaces: Vec<CommandBarSpace>,
    tabs: Vec<CommandBarTab>,
    commands: Vec<CommandBarCommandEntry>,
    target: Option<vmux_command::open_target::OpenTarget>,
    pages: Vec<CommandBarPage>,
    work_dirs: Vec<vmux_command::event::CommandBarWorkDir>,
    recent_files: Vec<vmux_command::event::CommandBarRecentFile>,
) -> CommandBarOpenEvent {
    CommandBarOpenEvent {
        open_id,
        native_windowed,
        url,
        space_name,
        spaces,
        tabs,
        commands,
        pages,
        work_dirs,
        recent_files,
        target,
        space_switch: false,
    }
}

#[derive(SystemParam)]
/// Bundled ECS queries for walking the active tab's panes/stacks into command-bar tab entries.
pub(crate) struct TabGatherParams<'w, 's> {
    pub active_tab: ActiveTabParam<'w, 's>,
    pub all_children: Query<'w, 's, &'static Children>,
    pub leaf_panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    pub pane_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Pane>>,
    pub pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    pub stack_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Stack>>,
    pub stack_q: Query<'w, 's, Entity, With<Stack>>,
    pub browser_meta: Query<'w, 's, &'static PageMetadata, With<Browser>>,
    pub child_of_q: Query<'w, 's, &'static ChildOf>,
}

/// Collect the active tab's open stacks as [`CommandBarTab`] entries, shared by the
/// command-bar modal and the home launcher.
#[allow(clippy::too_many_arguments)]
pub(crate) fn gather_command_bar_tabs(
    active_tab: Option<Entity>,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: &Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_q: &Query<Entity, With<Stack>>,
    browser_meta: &Query<&PageMetadata, With<Browser>>,
    child_of_q: &Query<&ChildOf>,
    space_name: &str,
) -> Vec<CommandBarTab> {
    let mut bar_tabs = Vec::new();
    let Some(active_tab_e) = active_tab else {
        return bar_tabs;
    };
    let (_, _, active_stack) = focused_stack(
        active_tab,
        all_children,
        leaf_panes,
        pane_ts,
        pane_children,
        stack_ts,
    );
    let active_pane = active_stack.and_then(|t| child_of_q.get(t).ok().map(|co| co.get()));
    let mut tab_panes = Vec::new();
    collect_leaf_panes(active_tab_e, all_children, leaf_panes, &mut tab_panes);
    for (pane_pos, &pane_e) in tab_panes.iter().enumerate() {
        let is_active_pane = active_pane == Some(pane_e);
        let Ok(children) = pane_children.get(pane_e) else {
            continue;
        };
        let mut tab_index = 0usize;
        for child in children.iter() {
            if !stack_q.contains(child) {
                continue;
            }
            let stack_is_active = active_stack == Some(child) && is_active_pane;
            let location = if space_name.is_empty() {
                format!("pane {} / stack {}", pane_pos + 1, tab_index + 1)
            } else {
                format!(
                    "{space_name} / pane {} / stack {}",
                    pane_pos + 1,
                    tab_index + 1
                )
            };
            if let Ok(tab_kids) = all_children.get(child) {
                for browser_e in tab_kids.iter() {
                    if let Ok(meta) = browser_meta.get(browser_e) {
                        bar_tabs.push(CommandBarTab {
                            title: meta.title.clone(),
                            url: meta.url.clone(),
                            pane_id: pane_e.to_bits(),
                            tab_index: tab_index as u32,
                            is_active: stack_is_active,
                            location: location.clone(),
                        });
                    }
                }
            }
            tab_index += 1;
        }
    }
    bar_tabs
}

/// Assemble a [`CommandBarOpenEvent`] (pages, commands, spaces, tabs) for the command
/// bar and the home launcher, from the current snapshots and gathered tabs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_command_bar_open_payload(
    open_id: u64,
    native_windowed: bool,
    space_name: String,
    url: String,
    spaces_snapshot: &CommandBarSpacesSnapshot,
    agents_snapshot: &CommandBarAgentsSnapshot,
    pages_snapshot: &CommandBarPagesSnapshot,
    work_snapshot: &vmux_command::snapshot::CommandBarWorkSnapshot,
    active_stack_count: usize,
    tabs: Vec<CommandBarTab>,
    target: Option<OpenTarget>,
) -> CommandBarOpenEvent {
    let app_agent_entries: Vec<AppAgentEntry> = agents_snapshot
        .strategies
        .iter()
        .map(|s| AppAgentEntry {
            id: app_agent_id(&s.provider, &s.model),
            name: format!("New {}/{} chat (App)", s.provider, s.model),
        })
        .collect();
    let mut pages = pages_snapshot.pages.clone();
    pages.extend(agent_pages(agents_snapshot));
    let history_shortcut = command_shortcut("browser_open_history");
    if !history_shortcut.is_empty()
        && let Some(page) = pages.iter_mut().find(|page| page.host == "history")
    {
        page.shortcut = history_shortcut;
    }
    let commands: Vec<CommandBarCommandEntry> = command_list(app_agent_entries)
        .into_iter()
        .map(|e| CommandBarCommandEntry {
            id: e.id,
            name: e.name,
            shortcut: e.shortcut,
        })
        .collect();
    let spaces = spaces_snapshot
        .spaces
        .iter()
        .map(|s| {
            let is_active = s.id == spaces_snapshot.active_space_id;
            CommandBarSpace {
                id: s.id.clone(),
                name: s.name.clone(),
                profile: s.profile.clone(),
                is_active,
                tab_count: if is_active {
                    active_stack_count as u32
                } else {
                    0
                },
            }
        })
        .collect();
    command_bar_open_payload(
        open_id,
        native_windowed,
        space_name,
        url,
        spaces,
        tabs,
        commands,
        target,
        pages,
        work_snapshot.work_dirs.clone(),
        work_snapshot.recent_files.clone(),
    )
}

#[derive(SystemParam)]
struct CommandBarActionQueries<'w, 's> {
    tab_q: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Tab>>,
    active_tab_param: ActiveTabParam<'w, 's>,
    all_children: Query<'w, 's, &'static Children>,
    leaf_panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Pane>>,
    pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    stack_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Stack>>,
    child_of_q: Query<'w, 's, &'static ChildOf>,
    content_browsers: Query<
        'w,
        's,
        Entity,
        (
            With<Browser>,
            Without<Header>,
            Without<SideSheet>,
            Without<Modal>,
        ),
    >,
}

fn build_open_command(target: Option<OpenTarget>, url: String) -> OpenCommand {
    match target {
        Some(OpenTarget::InPlace) | None => OpenCommand::InPlace { url: Some(url) },
        Some(OpenTarget::InNewStack) => OpenCommand::InNewStack { url: Some(url) },
        Some(OpenTarget::InPane {
            direction,
            target,
            mode,
        }) => OpenCommand::InPane {
            direction,
            target,
            mode,
            url: Some(url),
        },
        Some(OpenTarget::InNewTab) => OpenCommand::InNewTab { url: Some(url) },
        Some(OpenTarget::InNewSpace) => OpenCommand::InNewSpace { url: Some(url) },
    }
}

fn normalize_url(value: &str) -> String {
    if value.contains("://") || vmux_command::event::is_data_uri(value) {
        value.to_string()
    } else if value.contains('.') && !value.contains(' ') {
        format!("https://{}", value)
    } else {
        format!("https://www.google.com/search?q={}", value)
    }
}

fn prompt_agent_url(
    snapshot: &CommandBarAgentsSnapshot,
    requested_url: Option<&str>,
) -> Option<String> {
    let pages = agent_pages(snapshot);
    requested_url
        .and_then(|requested| pages.iter().find(|page| page.url == requested))
        .or_else(|| pages.first())
        .map(|page| page.url.clone())
}

fn on_command_bar_action(
    trigger: On<BinReceive<CommandBarActionEvent>>,
    mut modal_q: Query<(Entity, &mut Node, &mut Visibility), With<Modal>>,
    queries: CommandBarActionQueries,
    mut stack_params: ParamSet<(
        Query<Entity, With<Stack>>,
        Query<Entity, With<Main>>,
        Query<Entity, With<PrimaryWindow>>,
        Option<ResMut<crate::stack::FocusedStack>>,
        Query<(), With<Terminal>>,
    )>,
    mut resource_params: ParamSet<(
        Res<CommandBarSpacesSnapshot>,
        Res<CommandBarTerminalsSnapshot>,
        Res<CommandBarAgentsSnapshot>,
    )>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut writer_params: ParamSet<(
        MessageWriter<AppCommand>,
        MessageWriter<PageOpenRequest>,
        MessageWriter<vmux_core::agent::SpawnAgentInStackRequest>,
        MessageWriter<TerminalSpawnRequest>,
        Option<MessageWriter<PageAgentAttachRequest>>,
        Option<MessageWriter<PageAgentSpawnStackRequest>>,
        MessageWriter<ProcessesMonitorSpawnRequest>,
        MessageWriter<SpacesPageSpawnRequest>,
    )>,
    mut page_default_spawn_writer: MessageWriter<vmux_core::agent::PageAgentSpawnDefaultRequest>,
    mut page_default_attach_writer: MessageWriter<vmux_core::agent::PageAgentAttachDefaultRequest>,
    mut issued: MessageWriter<vmux_command::CommandIssued>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let evt = &trigger.event().payload;
    let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
    let terminals_snapshot = resource_params.p1().clone();
    let terminal_page_url = terminals_snapshot.terminal_page_url.clone();
    let pid_to_entity = terminals_snapshot.pid_to_entity.clone();
    let mut empty_stack = new_stack_ctx.stack;
    let previous_stack = new_stack_ctx.previous_stack;
    let mut custom_keyboard_restore = false;

    match evt.action.as_str() {
        "prompt" => {
            let prompt = evt.value.trim();
            if !prompt.is_empty() {
                let (_, _, focused_stack) = focused_stack(
                    queries.active_tab_param.get(),
                    &queries.all_children,
                    &queries.leaf_panes,
                    &queries.pane_ts,
                    &queries.pane_children,
                    &queries.stack_ts,
                );
                if let Some(stack) = empty_stack.or(focused_stack)
                    && let Some(url) =
                        prompt_agent_url(&resource_params.p2(), evt.agent_url.as_deref())
                {
                    commands
                        .entity(stack)
                        .insert(PendingAgentPrompt(prompt.to_string()));
                    writer_params.p1().write(PageOpenRequest {
                        target: PageOpenTarget::Stack(stack),
                        url,
                        request_id: None,
                    });
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                    custom_keyboard_restore = true;
                }
            }
        }
        "open" => {
            let expanded = if evt.value.starts_with('~') {
                std::env::var("HOME")
                    .ok()
                    .map(|h| {
                        std::path::PathBuf::from(h).join(evt.value[1..].trim_start_matches('/'))
                    })
                    .unwrap_or_else(|| std::path::PathBuf::from(&evt.value))
            } else if evt.value.starts_with('/') {
                std::path::PathBuf::from(&evt.value)
            } else {
                std::env::var("HOME")
                    .ok()
                    .map(|h| std::path::PathBuf::from(h).join(&evt.value))
                    .unwrap_or_else(|| std::path::PathBuf::from(&evt.value))
            };
            let is_path = expanded.exists();

            if is_path {
                let dir = if expanded.is_dir() {
                    &expanded
                } else {
                    expanded.parent().unwrap_or(&expanded)
                };
                if let Some(stack_e) = empty_stack {
                    commands.entity(stack_e).insert(PageMetadata {
                        url: terminal_page_url.clone(),
                        title: format!("Terminal ({})", dir.display()),
                        ..default()
                    });
                    writer_params.p3().write(TerminalSpawnRequest {
                        cwd: Some(dir.to_path_buf()),
                        target_stack: Some(stack_e),
                    });
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                    custom_keyboard_restore = true;
                }
            } else {
                let url = normalize_url(&evt.value);
                if matches!(url.as_str(), "vmux://agent/" | "vmux://agent") {
                    if let Some(stack_e) = empty_stack {
                        page_default_attach_writer.write(
                            vmux_core::agent::PageAgentAttachDefaultRequest { stack: stack_e },
                        );
                        new_stack_ctx.stack = None;
                        new_stack_ctx.previous_stack = None;
                        custom_keyboard_restore = true;
                    } else {
                        let (_, active_pane_opt, _) = focused_stack(
                            queries.active_tab_param.get(),
                            &queries.all_children,
                            &queries.leaf_panes,
                            &queries.pane_ts,
                            &queries.pane_children,
                            &queries.stack_ts,
                        );
                        if let Some(pane_e) = active_pane_opt {
                            page_default_spawn_writer.write(
                                vmux_core::agent::PageAgentSpawnDefaultRequest { pane: pane_e },
                            );
                            custom_keyboard_restore = true;
                        }
                    }
                } else if let Some(stack_e) = empty_stack {
                    writer_params.p1().write(PageOpenRequest {
                        target: PageOpenTarget::Stack(stack_e),
                        url,
                        request_id: None,
                    });
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                    custom_keyboard_restore = true;
                } else {
                    let target = evt.target;
                    let cmd =
                        AppCommand::Browser(BrowserCommand::Open(build_open_command(target, url)));
                    issued.write(vmux_command::CommandIssued {
                        caller,
                        command: cmd.clone(),
                    });
                    writer_params.p0().write(cmd);
                }
            }
        }
        "terminal" => {
            let known_terminal = parse_pid_from_url(&evt.value, &terminal_page_url)
                .and_then(|p| pid_to_entity.get(&p).copied());
            if let Some(entity) = known_terminal {
                focus_pane_entity(entity, &mut commands, &queries.child_of_q);
                new_stack_ctx.stack = None;
                new_stack_ctx.previous_stack = None;
                custom_keyboard_restore = true;
            } else {
                if let Some(pid) = parse_pid_from_url(&evt.value, &terminal_page_url) {
                    bevy::log::warn!("no terminal pane for pid {pid}; spawning new");
                }
                let cwd = if evt.value.is_empty() || evt.value.contains("://") {
                    None
                } else {
                    let expanded = if evt.value.starts_with("~/") {
                        std::env::var("HOME")
                            .map(|h| std::path::PathBuf::from(h).join(&evt.value[2..]))
                            .unwrap_or_else(|_| std::path::PathBuf::from(&evt.value))
                    } else if evt.value.starts_with('/') {
                        std::path::PathBuf::from(&evt.value)
                    } else {
                        std::env::var("HOME")
                            .map(|h| std::path::PathBuf::from(h).join(&evt.value))
                            .unwrap_or_else(|_| std::path::PathBuf::from(&evt.value))
                    };
                    Some(expanded)
                };
                if let Some(stack_e) = empty_stack {
                    commands.entity(stack_e).insert(PageMetadata {
                        url: terminal_page_url.clone(),
                        title: "Terminal".to_string(),
                        ..default()
                    });
                    writer_params.p3().write(TerminalSpawnRequest {
                        cwd: cwd.clone(),
                        target_stack: Some(stack_e),
                    });
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                    custom_keyboard_restore = true;
                } else {
                    let (_, active_pane_opt, _) = focused_stack(
                        queries.active_tab_param.get(),
                        &queries.all_children,
                        &queries.leaf_panes,
                        &queries.pane_ts,
                        &queries.pane_children,
                        &queries.stack_ts,
                    );
                    if let Some(pane_e) = active_pane_opt {
                        let stack_e = commands
                            .spawn((
                                crate::stack::stack_bundle(),
                                LastActivatedAt::now(),
                                ChildOf(pane_e),
                            ))
                            .id();
                        commands.entity(stack_e).insert(PageMetadata {
                            url: terminal_page_url.clone(),
                            title: "Terminal".to_string(),
                            ..default()
                        });
                        writer_params.p3().write(TerminalSpawnRequest {
                            cwd: cwd.clone(),
                            target_stack: Some(stack_e),
                        });
                    } else {
                        let cmd =
                            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
                                url: Some("vmux://terminal/".into()),
                            }));
                        issued.write(vmux_command::CommandIssued {
                            caller,
                            command: cmd.clone(),
                        });
                        writer_params.p0().write(cmd);
                    }
                }
            } // end reattach else
        }
        "command" => {
            if let Some((provider, model)) = parse_app_agent_id(&evt.value) {
                let sid = uuid::Uuid::new_v4().to_string();
                if let Some(stack_e) = empty_stack {
                    if let Some(mut w) = writer_params.p4() {
                        w.write(PageAgentAttachRequest {
                            stack: stack_e,
                            provider,
                            model,
                            sid,
                        });
                    }
                    commands.entity(stack_e).insert(LastActivatedAt::now());
                    if let Ok(parent) = queries.child_of_q.get(stack_e) {
                        commands.entity(parent.0).insert(LastActivatedAt::now());
                    }
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                    custom_keyboard_restore = true;
                } else {
                    let (_, active_pane_opt, _) = focused_stack(
                        queries.active_tab_param.get(),
                        &queries.all_children,
                        &queries.leaf_panes,
                        &queries.pane_ts,
                        &queries.pane_children,
                        &queries.stack_ts,
                    );
                    if let Some(pane_e) = active_pane_opt {
                        if let Some(mut w) = writer_params.p5() {
                            w.write(PageAgentSpawnStackRequest {
                                pane: pane_e,
                                provider,
                                model,
                                sid,
                            });
                        }
                        custom_keyboard_restore = true;
                    }
                }
            } else if let Some(url) = resource_params
                .p2()
                .providers
                .iter()
                .find(|p| p.id == evt.value)
                .map(|p| p.url.clone())
            {
                if let Some(stack_e) = empty_stack {
                    writer_params.p1().write(PageOpenRequest {
                        target: PageOpenTarget::Stack(stack_e),
                        url,
                        request_id: None,
                    });
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                    empty_stack = None;
                } else {
                    let target = evt.target;
                    let cmd =
                        AppCommand::Browser(BrowserCommand::Open(build_open_command(target, url)));
                    issued.write(vmux_command::CommandIssued {
                        caller,
                        command: cmd.clone(),
                    });
                    writer_params.p0().write(cmd);
                }
                custom_keyboard_restore = true;
            } else if let Some(cmd) = match_command(&evt.value) {
                issued.write(vmux_command::CommandIssued {
                    caller,
                    command: cmd.clone(),
                });
                writer_params.p0().write(cmd);
            }
            // If in new-tab mode and a command was executed, clean up the empty tab
            if let Some(stack_e) = empty_stack {
                commands.entity(stack_e).despawn();
                new_stack_ctx.stack = None;
                new_stack_ctx.previous_stack = None;
            }
        }
        "space" => {
            custom_keyboard_restore = true;
            if !evt.value.is_empty() {
                commands.trigger(BinReceive {
                    webview,
                    payload: SpaceCommandEvent {
                        command: "attach".to_string(),
                        space_id: Some(evt.value.clone()),
                        name: None,
                    },
                });
            }
            if let Some(stack_e) = empty_stack {
                commands.entity(stack_e).despawn();
                new_stack_ctx.stack = None;
                new_stack_ctx.previous_stack = None;
            }
        }
        "switch_tab" => {
            // Despawn empty tab if in new-tab mode
            if let Some(stack_e) = empty_stack {
                commands.entity(stack_e).despawn();
                new_stack_ctx.stack = None;
                new_stack_ctx.previous_stack = None;
            }
            if let Some((pane_bits, tab_idx)) = evt.value.split_once(':')
                && let (Ok(pane_id), Ok(tab_index)) =
                    (pane_bits.parse::<u64>(), tab_idx.parse::<usize>())
                && let Some(target_pane) =
                    queries.leaf_panes.iter().find(|e| e.to_bits() == pane_id)
            {
                let target_stack = {
                    let stack_q = stack_params.p0();
                    queries
                        .pane_children
                        .get(target_pane)
                        .ok()
                        .and_then(|children| {
                            children
                                .iter()
                                .filter(|&e| stack_q.contains(e))
                                .nth(tab_index)
                        })
                };
                // Activate the whole chain (stack -> pane -> tab -> space), not just the
                // pane/stack, so switching to a page in another tab actually moves the
                // active-tab marker (ensure_active_tab derives Active from LastActivatedAt).
                if let Some(target_stack) = target_stack {
                    focus_pane_entity(target_stack, &mut commands, &queries.child_of_q);
                } else {
                    focus_pane_entity(target_pane, &mut commands, &queries.child_of_q);
                }
            }
        }
        _ => {
            // "dismiss" and unknown actions
            if let Some(stack_e) = empty_stack {
                let stack_q = stack_params.p0();
                let closed_tab = close_tab_if_only_pending_stack(
                    stack_e,
                    &queries.tab_q,
                    &queries.child_of_q,
                    &queries.all_children,
                    &stack_q,
                    &mut commands,
                );
                if !closed_tab {
                    commands.entity(stack_e).despawn();
                }
                new_stack_ctx.stack = None;
                if !closed_tab {
                    // Restore keyboard to previous tab's browser
                    if let Some(prev) = previous_stack
                        && let Ok(children) = queries.all_children.get(prev)
                    {
                        for child in children.iter() {
                            if queries.content_browsers.contains(child) {
                                commands.entity(child).insert(CefKeyboardTarget);
                            }
                        }
                    }
                }
                new_stack_ctx.previous_stack = None;
                custom_keyboard_restore = true;
            }
        }
    }

    // Close command bar and restore keyboard
    if let Ok((modal_e, mut modal_node, mut modal_vis)) = modal_q.single_mut() {
        modal_node.display = Display::None;
        *modal_vis = Visibility::Hidden;
        commands
            .entity(modal_e)
            .insert(Pickable::IGNORE)
            .remove::<CefKeyboardTarget>()
            .remove::<CefPointerTarget>()
            .remove::<CommandBarRenderedOpen>()
            .remove::<CommandBarPaintedOpen>()
            .remove::<PendingCommandBarReveal>();
    }
    if !custom_keyboard_restore {
        let (_, _, active_stack) = focused_stack(
            queries.active_tab_param.get(),
            &queries.all_children,
            &queries.leaf_panes,
            &queries.pane_ts,
            &queries.pane_children,
            &queries.stack_ts,
        );
        if let Some(tab) = active_stack {
            for browser_e in &queries.content_browsers {
                let is_child = queries
                    .child_of_q
                    .get(browser_e)
                    .ok()
                    .map(|co| co.get() == tab)
                    .unwrap_or(false);
                if is_child {
                    commands.entity(browser_e).insert(CefKeyboardTarget);
                }
            }
        }
    }
}

fn close_tab_if_only_pending_stack(
    stack: Entity,
    tab_q: &Query<(Entity, &LastActivatedAt), With<Tab>>,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
    stack_q: &Query<Entity, With<Stack>>,
    commands: &mut Commands,
) -> bool {
    let Some(tab) = ancestor_tab(stack, tab_q, child_of_q) else {
        return false;
    };
    if entity_tree_contains_stack_other_than(tab, stack, all_children, stack_q) {
        return false;
    }
    let siblings = sibling_tabs(tab, tab_q, child_of_q, all_children);
    if siblings.len() <= 1 {
        return false;
    }
    if let Some(next) = crate::tab::pick_after_close(tab, &siblings) {
        commands.entity(next).insert(LastActivatedAt::now());
    }
    commands.entity(tab).despawn();
    true
}

fn ancestor_tab(
    entity: Entity,
    tab_q: &Query<(Entity, &LastActivatedAt), With<Tab>>,
    child_of_q: &Query<&ChildOf>,
) -> Option<Entity> {
    let mut current = entity;
    while let Ok(parent) = child_of_q.get(current).map(Relationship::get) {
        if tab_q.get(parent).is_ok() {
            return Some(parent);
        }
        current = parent;
    }
    None
}

fn entity_tree_contains_stack_other_than(
    entity: Entity,
    ignored_stack: Entity,
    all_children: &Query<&Children>,
    stack_q: &Query<Entity, With<Stack>>,
) -> bool {
    (stack_q.contains(entity) && entity != ignored_stack)
        || all_children.get(entity).is_ok_and(|children| {
            children.iter().any(|child| {
                entity_tree_contains_stack_other_than(child, ignored_stack, all_children, stack_q)
            })
        })
}

fn sibling_tabs(
    tab: Entity,
    tab_q: &Query<(Entity, &LastActivatedAt), With<Tab>>,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
) -> Vec<Entity> {
    let Ok(parent) = child_of_q.get(tab).map(Relationship::get) else {
        return vec![tab];
    };
    let Ok(children) = all_children.get(parent) else {
        return vec![tab];
    };
    children.iter().filter(|e| tab_q.get(*e).is_ok()).collect()
}

fn deferred_dismiss_modal(
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut modal_q: Query<(Entity, &mut Node, &mut Visibility), With<Modal>>,
    mut commands: Commands,
) {
    if !new_stack_ctx.dismiss_modal {
        return;
    }
    new_stack_ctx.dismiss_modal = false;
    if let Ok((modal_e, mut modal_node, mut modal_vis)) = modal_q.single_mut()
        && modal_node.display != Display::None
    {
        modal_node.display = Display::None;
        *modal_vis = Visibility::Hidden;
        commands
            .entity(modal_e)
            .insert(Pickable::IGNORE)
            .remove::<CefKeyboardTarget>()
            .remove::<CefPointerTarget>()
            .remove::<CommandBarRenderedOpen>()
            .remove::<CommandBarPaintedOpen>()
            .remove::<PendingCommandBarReveal>();
    }
}

fn reveal_command_bar(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut Visibility,
            &mut PendingCommandBarReveal,
            Option<&CommandBarRenderedOpen>,
            Option<&CommandBarPaintedOpen>,
            Option<&CommandBarNativeSize>,
            Has<WebviewWindowed>,
        ),
        With<Modal>,
    >,
) {
    for (entity, mut vis, mut pending, rendered, painted, native_size, native_windowed) in
        &mut query
    {
        let rendered_open_id = rendered.map(|rendered| rendered.0);
        let painted_open_id = painted.map(|painted| painted.0);
        if native_command_bar_reveal_timed_out(
            native_windowed,
            pending.frames,
            pending.open_id,
            rendered_open_id,
            native_size.is_some(),
        ) {
            commands.entity(entity).remove::<PendingCommandBarReveal>();
            commands.trigger(BinReceive::<CommandBarActionEvent> {
                webview: entity,
                payload: CommandBarActionEvent {
                    action: "dismiss".to_string(),
                    value: String::new(),
                    target: None,
                    agent_url: None,
                },
            });
            continue;
        }
        match next_command_bar_reveal_frames_for_backend(
            native_windowed,
            pending.frames,
            pending.open_id,
            rendered_open_id,
            painted_open_id,
            native_size.is_some(),
        ) {
            Some(frames) => pending.frames = frames,
            None => {
                *vis = Visibility::Inherited;
                commands.entity(entity).remove::<PendingCommandBarReveal>();
                webview_debug_log(format!("command_bar reveal entity={entity:?}"));
            }
        }
    }
}

fn retry_pending_command_bar_open(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    query: Query<
        (
            Entity,
            &PendingCommandBarReveal,
            Option<&CommandBarRenderedOpen>,
        ),
        With<Modal>,
    >,
) {
    for (entity, pending, rendered) in &query {
        let rendered_open_id = rendered.map(|rendered| rendered.0);
        let Some(payload) = pending.payload.as_deref() else {
            continue;
        };
        if !should_retry_command_bar_open_payload(pending.open_id, Some(payload), rendered_open_id)
        {
            continue;
        }
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_bytes(
            entity,
            COMMAND_BAR_OPEN_EVENT,
            payload.to_vec(),
        ));
    }
}

fn mark_command_bar_painted(
    mut commands: Commands,
    mut textures: MessageReader<RenderTextureMessage>,
    query: Query<&PendingCommandBarReveal, With<Modal>>,
) {
    for texture in textures.read() {
        let Ok(pending) = query.get(texture.webview) else {
            continue;
        };
        if pending.open_id == 0 {
            continue;
        }
        commands
            .entity(texture.webview)
            .insert(CommandBarPaintedOpen(pending.open_id));
    }
}

fn on_path_complete_request(
    trigger: On<BinReceive<PathCompleteRequest>>,
    modal_q: Query<Entity, With<Modal>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let query = &trigger.event().payload.query;
    let Ok(modal_e) = modal_q.single() else {
        return;
    };
    if !browsers.has_browser(modal_e) || !browsers.host_emit_ready(&modal_e) {
        return;
    }

    let completions = complete_path(query);
    let payload = PathCompleteResponse { completions };
    commands.trigger(BinHostEmitEvent::from_rkyv(
        modal_e,
        PATH_COMPLETE_RESPONSE,
        &payload,
    ));
}

fn complete_path(query: &str) -> Vec<PathEntry> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());

    let (parent_str, prefix) = if let Some(pos) = query.rfind('/') {
        (&query[..=pos], &query[pos + 1..])
    } else {
        ("", query)
    };

    let resolved_parent = if parent_str.starts_with("~/") || parent_str == "~/" {
        std::path::PathBuf::from(&home).join(&parent_str[2..])
    } else if parent_str.starts_with('/') {
        std::path::PathBuf::from(parent_str)
    } else if parent_str.is_empty() {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(&home))
    } else {
        std::path::PathBuf::from(&home).join(parent_str)
    };

    let Ok(entries) = std::fs::read_dir(&resolved_parent) else {
        return Vec::new();
    };

    let prefix_lower = prefix.to_lowercase();
    let mut results: Vec<PathEntry> = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

        if !prefix.is_empty() && !name.to_lowercase().starts_with(&prefix_lower) {
            continue;
        }

        let display_name = if is_dir {
            format!("{}/", name)
        } else {
            name.clone()
        };

        // Absolute path so the file:// editor (and terminal cwd) can open it directly.
        let child = resolved_parent.join(&name);
        let full_path = if is_dir {
            format!("{}/", child.display())
        } else {
            child.display().to_string()
        };

        results.push(PathEntry {
            name: display_name,
            is_dir,
            full_path,
        });
    }

    results.sort_by(|a, b| {
        let a_hidden = a.name.starts_with('.');
        let b_hidden = b.name.starts_with('.');
        b.is_dir
            .cmp(&a.is_dir)
            .then(a_hidden.cmp(&b_hidden))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    results.truncate(20);
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::schedule::{NodeId, Schedules, SystemSet};
    use vmux_command::event::CommandBarSpace;
    use vmux_command::{CommandPlugin, ReadAppCommands};
    use vmux_core::agent::AgentKind;

    #[test]
    fn command_bar_open_does_not_block_on_command_bar_listener() {
        assert!(command_bar_open_delivery_ready(true, true, false));
        assert!(command_bar_open_delivery_ready(true, true, true));
    }

    #[test]
    fn build_payload_includes_commands_and_target() {
        let pages = CommandBarPagesSnapshot::default();
        let spaces = CommandBarSpacesSnapshot::default();
        let agents = CommandBarAgentsSnapshot::default();
        let work = vmux_command::snapshot::CommandBarWorkSnapshot::default();
        let payload = build_command_bar_open_payload(
            7,
            false,
            String::new(),
            String::new(),
            &spaces,
            &agents,
            &pages,
            &work,
            0,
            Vec::new(),
            Some(OpenTarget::InPlace),
        );
        assert_eq!(payload.open_id, 7);
        assert_eq!(payload.target, Some(OpenTarget::InPlace));
        assert!(!payload.commands.is_empty());
    }

    #[test]
    fn command_bar_open_payload_retries_until_rendered_ack() {
        assert!(should_retry_command_bar_open_payload(
            7,
            Some(b"payload"),
            None
        ));
        assert!(should_retry_command_bar_open_payload(
            7,
            Some(b"payload"),
            Some(6)
        ));
        assert!(!should_retry_command_bar_open_payload(
            7,
            Some(b"payload"),
            Some(7)
        ));
        assert!(!should_retry_command_bar_open_payload(
            0,
            Some(b"payload"),
            None
        ));
        assert!(!should_retry_command_bar_open_payload(7, None, None));
    }

    #[test]
    fn command_bar_open_retry_uses_binary_host_emit() {
        let source = include_str!("handler.rs");
        let retry_fn = source
            .split("fn retry_pending_command_bar_open")
            .nth(1)
            .and_then(|tail| tail.split("fn mark_command_bar_painted").next())
            .unwrap_or_default();

        assert!(retry_fn.contains("BinHostEmitEvent::from_bytes"));
        assert!(!retry_fn.contains("HostEmitEvent::new"));
    }

    #[test]
    fn command_bar_open_emit_does_not_requeue_without_ready_event() {
        assert!(!should_requeue_command_bar_open_after_emit(false));
        assert!(!should_requeue_command_bar_open_after_emit(true));
    }

    #[test]
    fn command_bar_reveal_does_not_block_on_host_or_ui_listener() {
        assert!(command_bar_reveal_ready(true, false, true, true));
        assert!(command_bar_reveal_ready(true, true, false, true));
        assert!(command_bar_reveal_ready(true, true, true, true));
    }

    #[test]
    fn command_bar_reveal_requires_rendered_open_payload() {
        assert!(!command_bar_reveal_ready(true, true, true, false));
        assert!(command_bar_reveal_ready(true, true, true, true));
    }

    #[test]
    fn command_bar_reveal_requires_browser() {
        assert!(!command_bar_reveal_ready(false, true, true, true));
    }

    #[derive(Resource, Default)]
    struct CapturedCommandBarOpen(bool);

    fn capture_command_bar_open(
        modal_q: Query<(&Node, Has<CefKeyboardTarget>), With<Modal>>,
        mut captured: ResMut<CapturedCommandBarOpen>,
    ) {
        captured.0 = is_command_bar_open(&modal_q);
    }

    #[test]
    fn hidden_prewarmed_modal_is_not_command_bar_open() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CapturedCommandBarOpen>()
            .add_systems(Update, capture_command_bar_open);
        app.world_mut().spawn((
            Modal,
            Node {
                display: Display::Flex,
                ..default()
            },
            Visibility::Hidden,
        ));

        app.update();

        assert!(!app.world().resource::<CapturedCommandBarOpen>().0);
    }

    #[test]
    fn command_bar_modal_prewarms_hidden_and_renderable() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, prewarm_command_bar_modal);
        let modal = app
            .world_mut()
            .spawn((
                Modal,
                Node {
                    display: Display::None,
                    ..default()
                },
                Visibility::Hidden,
            ))
            .id();

        app.update();

        let node = app.world().get::<Node>(modal).unwrap();
        let visibility = app.world().get::<Visibility>(modal).unwrap();
        let reveal = app.world().get::<PendingCommandBarReveal>(modal).unwrap();

        assert_eq!(node.display, Display::Flex);
        assert_eq!(*visibility, Visibility::Hidden);
        assert_eq!(reveal.open_id, 0);
        assert!(app.world().get::<CefKeyboardTarget>(modal).is_none());
        assert_eq!(
            app.world().get::<bevy::picking::Pickable>(modal),
            Some(&bevy::picking::Pickable::IGNORE)
        );
    }

    #[test]
    fn ready_command_bar_modal_still_prewarms_hidden_and_renderable() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, prewarm_command_bar_modal);
        let modal = app
            .world_mut()
            .spawn((
                Modal,
                CommandBarReady,
                Node {
                    display: Display::None,
                    ..default()
                },
                Visibility::Hidden,
            ))
            .id();

        app.update();

        let node = app.world().get::<Node>(modal).unwrap();
        let visibility = app.world().get::<Visibility>(modal).unwrap();
        let reveal = app.world().get::<PendingCommandBarReveal>(modal).unwrap();

        assert_eq!(node.display, Display::Flex);
        assert_eq!(*visibility, Visibility::Hidden);
        assert_eq!(reveal.open_id, 0);
    }

    #[test]
    fn command_bar_open_requires_browser_main_frame() {
        assert!(!command_bar_open_delivery_ready(false, true, true));
        assert!(!command_bar_open_delivery_ready(true, false, true));
        assert!(command_bar_open_delivery_ready(true, true, true));
    }

    #[test]
    fn command_bar_reveal_starts_before_ui_listener() {
        assert!(should_start_command_bar_reveal(
            true,
            true,
            false,
            true,
            false,
            Visibility::Hidden
        ));
    }

    #[test]
    fn command_bar_reveal_does_not_start_before_main_frame() {
        assert!(!should_start_command_bar_reveal(
            false,
            false,
            true,
            true,
            false,
            Visibility::Hidden
        ));
    }

    #[test]
    fn command_bar_retry_does_not_restart_pending_reveal() {
        assert!(!should_start_command_bar_reveal(
            true,
            true,
            true,
            true,
            true,
            Visibility::Hidden
        ));
    }

    #[test]
    fn command_bar_reveal_waits_for_matching_open_id() {
        assert_eq!(next_command_bar_reveal_frames(1, 7, None, None), Some(2));
        assert_eq!(
            next_command_bar_reveal_frames(1, 7, Some(6), Some(7)),
            Some(2)
        );
        assert_eq!(
            next_command_bar_reveal_frames(0, 7, Some(7), Some(7)),
            Some(1)
        );
        assert_eq!(next_command_bar_reveal_frames(2, 7, Some(7), Some(7)), None);
    }

    #[test]
    fn command_bar_reveal_falls_back_when_rendered_event_is_missing() {
        assert_eq!(next_command_bar_reveal_frames(0, 7, None, None), Some(1));
        assert_eq!(next_command_bar_reveal_frames(10, 7, None, None), None);
        assert_eq!(
            next_command_bar_reveal_frames(10, 7, Some(6), Some(7)),
            None
        );
    }

    #[test]
    fn command_bar_reveal_does_not_require_texture_after_rendered_event() {
        assert_eq!(next_command_bar_reveal_frames(2, 7, Some(7), None), None);
        assert_eq!(next_command_bar_reveal_frames(2, 7, Some(7), Some(7)), None);
    }

    #[test]
    fn native_command_bar_waits_for_size_and_rendered_ack() {
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(true, 10, 7, None, None, true),
            Some(11)
        );
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(true, 10, 7, Some(7), None, false),
            Some(11)
        );
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(true, 2, 7, Some(7), None, true),
            None
        );
    }

    #[test]
    fn native_command_bar_aborts_stalled_reveal() {
        assert!(!native_command_bar_reveal_timed_out(
            true,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT_FRAMES - 1,
            7,
            None,
            false,
        ));
        assert!(native_command_bar_reveal_timed_out(
            true,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT_FRAMES,
            7,
            None,
            false,
        ));
        assert!(native_command_bar_reveal_timed_out(
            true,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT_FRAMES,
            7,
            Some(7),
            false,
        ));
        assert!(!native_command_bar_reveal_timed_out(
            true,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT_FRAMES,
            7,
            Some(7),
            true,
        ));
        assert!(!native_command_bar_reveal_timed_out(
            false,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT_FRAMES,
            7,
            None,
            false,
        ));
    }

    #[test]
    fn native_command_bar_timeout_clears_pending_reveal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, reveal_command_bar);
        let modal = app
            .world_mut()
            .spawn((
                Modal,
                WebviewWindowed,
                Visibility::Hidden,
                PendingCommandBarReveal {
                    frames: COMMAND_BAR_NATIVE_REVEAL_TIMEOUT_FRAMES,
                    open_id: 7,
                    payload: Some(b"payload".to_vec()),
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<PendingCommandBarReveal>(modal).is_none());
        assert_eq!(
            app.world().get::<Visibility>(modal),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn native_command_bar_ignores_hidden_prewarm_size() {
        assert!(!command_bar_size_should_apply(Visibility::Hidden, None));
        assert!(command_bar_size_should_apply(Visibility::Inherited, None));
    }

    #[test]
    fn native_command_bar_accepts_hidden_open_size() {
        let pending = PendingCommandBarReveal {
            frames: 0,
            open_id: 7,
            payload: Some(Vec::new()),
        };

        assert!(command_bar_size_should_apply(
            Visibility::Hidden,
            Some(&pending)
        ));
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(true, 0, 7, None, None, true),
            Some(1)
        );
    }

    #[test]
    fn command_bar_paint_before_rendered_ack_still_allows_reveal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<RenderTextureMessage>()
            .add_systems(
                Update,
                (mark_command_bar_painted, reveal_command_bar).chain(),
            );

        let modal = app
            .world_mut()
            .spawn((
                Modal,
                Visibility::Hidden,
                PendingCommandBarReveal {
                    frames: 2,
                    open_id: 7,
                    payload: Some(b"payload".to_vec()),
                },
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<RenderTextureMessage>>()
            .write(RenderTextureMessage {
                webview: modal,
                ty: bevy_cef_core::prelude::RenderPaintElementType::View,
                width: 1,
                height: 1,
                patches: std::sync::Arc::new(
                    [bevy_cef_core::prelude::WebviewPaintPatch {
                        rect: bevy_cef_core::prelude::WebviewDirtyRect {
                            x: 0,
                            y: 0,
                            width: 1,
                            height: 1,
                        },
                        buffer: std::sync::Arc::new(vec![0, 0, 0, 255]),
                    }]
                    .into_iter()
                    .collect(),
                ),
                dirty: Default::default(),
            });

        app.update();
        app.world_mut()
            .entity_mut(modal)
            .insert(CommandBarRenderedOpen(7));
        app.update();

        assert!(app.world().get::<CommandBarPaintedOpen>(modal).is_some());
        assert!(app.world().get::<PendingCommandBarReveal>(modal).is_none());
        assert_eq!(
            app.world().get::<Visibility>(modal),
            Some(&Visibility::Inherited)
        );
    }

    #[test]
    fn prewarmed_command_bar_starts_reveal_at_ready_frame() {
        assert_eq!(
            command_bar_reveal_start_frames(true),
            COMMAND_BAR_REVEAL_FRAMES
        );
        assert_eq!(command_bar_reveal_start_frames(false), 0);
        assert_eq!(
            next_command_bar_reveal_frames(
                command_bar_reveal_start_frames(true),
                7,
                Some(7),
                Some(7)
            ),
            None
        );
    }

    #[test]
    fn command_bar_payload_includes_space_name() {
        let payload = command_bar_open_payload(
            7,
            false,
            "Work".to_string(),
            "https://example.com".to_string(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(payload.space_name, "Work");
        assert_eq!(payload.open_id, 7);
    }

    #[test]
    fn command_bar_payload_includes_spaces() {
        let spaces = vec![CommandBarSpace {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: "Personal".to_string(),
            is_active: true,
            tab_count: 2,
        }];

        let payload = command_bar_open_payload(
            8,
            true,
            "Work".to_string(),
            "vmux://spaces/".to_string(),
            spaces.clone(),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(payload.spaces, spaces);
        assert!(payload.native_windowed);
    }

    #[test]
    fn space_open_command_opens_space_switch_mode() {
        let request = command_bar_open_request([AppCommand::Layout(LayoutCommand::Space(
            SpaceCommand::Open,
        ))]);

        assert!(request.should_toggle);
        assert!(request.space_switch);
        assert_eq!(request.url_override, Some(String::new()));
    }

    #[test]
    fn command_bar_focuses_start_only_for_non_space_switch_open() {
        assert!(command_bar_should_focus_start(false, false, true));
        assert!(!command_bar_should_focus_start(false, true, true));
        assert!(!command_bar_should_focus_start(true, false, true));
        assert!(!command_bar_should_focus_start(false, false, false));
    }

    #[test]
    fn space_switch_reopens_command_bar_even_when_visible() {
        assert!(command_bar_toggle_should_open(false, false));
        assert!(!command_bar_toggle_should_open(true, false));
        assert!(command_bar_toggle_should_open(true, true));
        assert!(command_bar_toggle_should_open(false, true));
    }

    #[test]
    fn open_in_new_stack_does_not_dismiss_command_bar() {
        let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InNewStack { url: None },
        ))]);

        assert!(!request.should_dismiss);
    }

    #[test]
    fn open_command_bar_forces_empty_url_override() {
        let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Bar(
            BrowserBarCommand::OpenCommandBar,
        ))]);

        assert!(request.should_toggle);
        assert_eq!(request.url_override, Some(String::new()));
    }

    #[test]
    fn open_page_in_command_bar_leaves_url_override_unset_so_current_url_is_prefilled() {
        let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Bar(
            BrowserBarCommand::OpenPageInCommandBar,
        ))]);

        assert!(request.should_toggle);
        assert_eq!(request.url_override, None);
    }

    #[test]
    fn open_page_in_command_bar_marks_payload_as_in_place_target() {
        let source = include_str!("handler.rs");
        let open_fn = source
            .split("fn handle_open_command_bar")
            .nth(1)
            .and_then(|tail| tail.split("fn command_bar_open_payload").next())
            .unwrap_or_default();

        assert!(open_fn.contains("if replace_active_stack"));
        assert!(open_fn.contains("OpenTarget::InPlace"));
    }

    #[test]
    fn open_page_in_command_bar_cancels_pending_new_stack_context() {
        let pending_stack = Entity::from_bits(7);
        let previous_stack = Entity::from_bits(6);
        let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Bar(
            BrowserBarCommand::OpenPageInCommandBar,
        ))]);
        let mut ctx = NewStackContext {
            stack: Some(pending_stack),
            previous_stack: Some(previous_stack),
            needs_open: true,
            dismiss_modal: false,
        };

        assert!(request.replace_active_stack);
        assert!(!command_bar_should_open_pending_stack(&mut ctx, true));
        let canceled = command_bar_cancel_pending_stack_for_active_open(
            &mut ctx,
            request.replace_active_stack,
        );

        assert_eq!(canceled, Some((pending_stack, Some(previous_stack))));
        assert_eq!(ctx.stack, None);
        assert_eq!(ctx.previous_stack, None);
        assert!(!ctx.needs_open);
    }

    #[test]
    fn pending_stack_with_startup_url_dispatches_url_request() {
        let stack = Entity::from_bits(7);
        let previous_stack = Entity::from_bits(6);
        let mut ctx = NewStackContext {
            stack: Some(stack),
            previous_stack: Some(previous_stack),
            needs_open: true,
            dismiss_modal: false,
        };

        let request =
            pending_stack_startup_url_request(&mut ctx, Some("https://startup.test")).unwrap();

        match request.target {
            PageOpenTarget::Stack(target) => assert_eq!(target, stack),
            other => panic!("expected stack target, got {other:?}"),
        }
        assert_eq!(request.url, "https://startup.test");
        assert_eq!(ctx.stack, None);
        assert_eq!(ctx.previous_stack, None);
        assert!(!ctx.needs_open);
    }

    #[test]
    fn pending_stack_without_startup_url_keeps_prompt_pending() {
        let stack = Entity::from_bits(7);
        let mut ctx = NewStackContext {
            stack: Some(stack),
            previous_stack: None,
            needs_open: true,
            dismiss_modal: false,
        };

        let request = pending_stack_startup_url_request(&mut ctx, Some(""));

        assert!(request.is_none());
        assert_eq!(ctx.stack, Some(stack));
        assert!(ctx.needs_open);
    }

    #[test]
    fn dismiss_action_closes_command_bar_modal_in_one_pass() {
        use bevy::ecs::system::RunSystemOnce;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_plugins(crate::stack::StackPlugin)
            .add_plugins(CommandBarInputPlugin)
            .add_message::<TerminalSpawnRequest>()
            .add_message::<ProcessesMonitorSpawnRequest>()
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .init_resource::<crate::pane::PendingCursorWarp>()
            .insert_resource(bevy_cef::prelude::CefSuppressKeyboardInput::default());

        let modal = app
            .world_mut()
            .spawn((
                Modal,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                Visibility::Inherited,
                CefKeyboardTarget,
                CommandBarRenderedOpen(1),
            ))
            .id();

        app.world_mut()
            .trigger(BinReceive::<CommandBarActionEvent> {
                webview: modal,
                payload: CommandBarActionEvent {
                    action: "dismiss".to_string(),
                    value: String::new(),
                    target: None,
                    agent_url: None,
                },
            });
        app.world_mut().flush();

        let vis_after_close = *app.world().get::<Visibility>(modal).unwrap();
        let display_after_close = app.world().get::<Node>(modal).unwrap().display;
        let has_kb_after_close = app.world().get::<CefKeyboardTarget>(modal).is_some();
        let has_rendered_after_close = app.world().get::<CommandBarRenderedOpen>(modal).is_some();
        let has_painted_after_close = app.world().get::<CommandBarPaintedOpen>(modal).is_some();
        let has_pending_after_close = app.world().get::<PendingCommandBarReveal>(modal).is_some();

        assert_eq!(
            vis_after_close,
            Visibility::Hidden,
            "modal should be hidden after dismiss"
        );
        assert_eq!(
            display_after_close,
            Display::None,
            "modal should have display None after dismiss"
        );
        assert!(
            !has_kb_after_close,
            "CefKeyboardTarget should be removed after dismiss"
        );
        assert!(
            !has_rendered_after_close,
            "CommandBarRenderedOpen should be cleared after dismiss"
        );
        assert!(
            !has_painted_after_close,
            "CommandBarPaintedOpen should be cleared after dismiss"
        );
        assert!(
            !has_pending_after_close,
            "PendingCommandBarReveal should be cleared after dismiss"
        );

        app.world_mut()
            .run_system_once(prewarm_command_bar_modal)
            .unwrap();

        let vis_after_prewarm = *app.world().get::<Visibility>(modal).unwrap();
        let display_after_prewarm = app.world().get::<Node>(modal).unwrap().display;
        let has_kb_after_prewarm = app.world().get::<CefKeyboardTarget>(modal).is_some();
        let pending_open_id_after_prewarm = app
            .world()
            .get::<PendingCommandBarReveal>(modal)
            .map(|p| p.open_id);

        assert_eq!(
            vis_after_prewarm,
            Visibility::Hidden,
            "modal must stay hidden after prewarm"
        );
        assert!(
            !has_kb_after_prewarm,
            "CefKeyboardTarget must not return after prewarm"
        );
        assert!(
            !command_bar_modal_is_open(display_after_prewarm, has_kb_after_prewarm),
            "is_command_bar_open must report false after dismiss + prewarm"
        );
        if let Some(open_id) = pending_open_id_after_prewarm {
            assert_eq!(
                open_id, 0,
                "prewarm should re-arm reveal at open_id=0 (which never fires until handle_open_command_bar bumps it)"
            );
        }
    }

    #[test]
    fn command_bar_open_runs_after_tab_commands() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_plugins(crate::stack::StackPlugin)
            .add_plugins(CommandBarInputPlugin);

        let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
        let mut update = schedules.remove(Update).unwrap();
        update.initialize(app.world_mut()).unwrap();
        let graph = update.graph();
        let tab_command_set = graph
            .system_sets
            .get_key(crate::stack::StackCommandSet.intern())
            .unwrap();
        let read_command_systems = graph.systems_in_set(ReadAppCommands.intern()).unwrap();
        let tab_command_systems = graph
            .systems_in_set(crate::stack::StackCommandSet.intern())
            .unwrap();
        let command_bar_open_system = read_command_systems
            .iter()
            .copied()
            .find(|system| !tab_command_systems.contains(system))
            .unwrap();

        assert!(graph.dependency().graph().contains_edge(
            NodeId::Set(tab_command_set),
            NodeId::System(command_bar_open_system)
        ));
    }

    const TEST_TERMINAL_URL: &str = "vmux://terminal/";

    #[test]
    fn parse_pid_from_url_accepts_numeric() {
        assert_eq!(
            parse_pid_from_url("vmux://terminal/12345", TEST_TERMINAL_URL),
            Some(12345)
        );
        assert_eq!(
            parse_pid_from_url("vmux://terminal/0", TEST_TERMINAL_URL),
            Some(0)
        );
    }

    #[test]
    fn parse_pid_from_url_rejects_uuid_form() {
        let uuid_url = "vmux://terminal/ae724a54-c387-5359-0687-ccfc155558b6";
        assert_eq!(parse_pid_from_url(uuid_url, TEST_TERMINAL_URL), None);
    }

    #[test]
    fn parse_pid_from_url_rejects_empty_path() {
        assert_eq!(
            parse_pid_from_url("vmux://terminal/", TEST_TERMINAL_URL),
            None
        );
    }

    #[test]
    fn parse_pid_from_url_rejects_overflow() {
        assert_eq!(
            parse_pid_from_url("vmux://terminal/99999999999999999", TEST_TERMINAL_URL),
            None
        );
    }

    #[test]
    fn build_open_command_none_target_yields_in_place() {
        let cmd = build_open_command(None, "https://example.com".to_string());
        assert_eq!(
            cmd,
            OpenCommand::InPlace {
                url: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn build_open_command_in_place_target_yields_in_place() {
        let cmd = build_open_command(Some(OpenTarget::InPlace), "https://example.com".to_string());
        assert_eq!(
            cmd,
            OpenCommand::InPlace {
                url: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn build_open_command_in_new_stack_target() {
        let cmd = build_open_command(
            Some(OpenTarget::InNewStack),
            "https://example.com".to_string(),
        );
        assert_eq!(
            cmd,
            OpenCommand::InNewStack {
                url: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn build_open_command_in_new_tab_target() {
        let cmd = build_open_command(
            Some(OpenTarget::InNewTab),
            "https://example.com".to_string(),
        );
        assert_eq!(
            cmd,
            OpenCommand::InNewTab {
                url: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn build_open_command_in_new_space_target() {
        let cmd = build_open_command(
            Some(OpenTarget::InNewSpace),
            "https://example.com".to_string(),
        );
        assert_eq!(
            cmd,
            OpenCommand::InNewSpace {
                url: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn build_open_command_in_pane_target() {
        use vmux_command::open_target::{PaneDirection, PaneOpenMode, PaneTarget};
        let cmd = build_open_command(
            Some(OpenTarget::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
            }),
            "https://example.com".to_string(),
        );
        assert_eq!(
            cmd,
            OpenCommand::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: Some("https://example.com".to_string()),
            }
        );
    }

    #[test]
    fn normalize_url_adds_https_for_domain() {
        assert_eq!(normalize_url("google.com"), "https://google.com");
    }

    #[test]
    fn normalize_url_preserves_explicit_protocol() {
        assert_eq!(normalize_url("http://example.com"), "http://example.com");
        assert_eq!(normalize_url("https://example.com"), "https://example.com");
    }

    #[test]
    fn normalize_url_search_query_becomes_google() {
        let result = normalize_url("hello world");
        assert!(result.starts_with("https://www.google.com/search?q="));
        assert!(result.contains("hello world"));
    }

    #[test]
    fn normalize_url_preserves_vmux_protocol() {
        assert_eq!(normalize_url("vmux://terminal/123"), "vmux://terminal/123");
    }

    #[test]
    fn normalize_url_preserves_data_scheme() {
        let data = "data:text/html,<style>body{background:white}</style><h1>x</h1>";
        assert_eq!(normalize_url(data), data);
    }

    #[test]
    fn prompt_prefers_most_recent_installed_agent() {
        let snapshot = CommandBarAgentsSnapshot {
            recent: vec![AgentPromptTarget::Cli(AgentKind::Codex)],
            providers: vec![vmux_command::snapshot::AgentProviderSummary {
                id: "codex".to_string(),
                name: "Codex".to_string(),
                url: "vmux://agent/codex/cli".to_string(),
                icon: String::new(),
            }],
            acp: vec![vmux_command::snapshot::AgentProviderSummary {
                id: "claude-acp".to_string(),
                name: "Claude Agent".to_string(),
                url: "vmux://agent/claude".to_string(),
                icon: String::new(),
            }],
            ..Default::default()
        };

        assert_eq!(
            prompt_agent_url(&snapshot, None).as_deref(),
            Some("vmux://agent/codex/cli")
        );
    }

    #[test]
    fn prompt_falls_back_to_installed_agent() {
        let snapshot = CommandBarAgentsSnapshot {
            acp: vec![vmux_command::snapshot::AgentProviderSummary {
                id: "claude-acp".to_string(),
                name: "Claude Agent".to_string(),
                url: "vmux://agent/claude".to_string(),
                icon: String::new(),
            }],
            ..Default::default()
        };

        assert_eq!(
            prompt_agent_url(&snapshot, None).as_deref(),
            Some("vmux://agent/claude")
        );
        assert_eq!(
            prompt_agent_url(&CommandBarAgentsSnapshot::default(), None),
            None
        );
    }

    #[test]
    fn prompt_uses_selected_installed_agent_and_rejects_stale_url() {
        let snapshot = CommandBarAgentsSnapshot {
            providers: vec![vmux_command::snapshot::AgentProviderSummary {
                id: "codex".to_string(),
                name: "Codex".to_string(),
                url: "vmux://agent/codex/cli".to_string(),
                icon: String::new(),
            }],
            acp: vec![vmux_command::snapshot::AgentProviderSummary {
                id: "claude-acp".to_string(),
                name: "Claude Agent".to_string(),
                url: "vmux://agent/claude".to_string(),
                icon: String::new(),
            }],
            recent: vec![AgentPromptTarget::Cli(AgentKind::Codex)],
            ..Default::default()
        };

        assert_eq!(
            prompt_agent_url(&snapshot, Some("vmux://agent/claude")).as_deref(),
            Some("vmux://agent/claude")
        );
        assert_eq!(
            prompt_agent_url(&snapshot, Some("vmux://agent/uninstalled")).as_deref(),
            Some("vmux://agent/codex/cli")
        );
    }

    #[test]
    fn pending_reveal_is_active_only_with_real_open_id() {
        assert!(
            !PendingCommandBarReveal {
                frames: 0,
                open_id: 0,
                payload: None,
            }
            .is_active()
        );
        assert!(
            PendingCommandBarReveal {
                frames: 0,
                open_id: 7,
                payload: None,
            }
            .is_active()
        );
    }

    #[test]
    fn agent_pages_lists_only_snapshot_agents_in_recent_order() {
        let snapshot = CommandBarAgentsSnapshot {
            providers: vec![vmux_command::snapshot::AgentProviderSummary {
                id: "codex".to_string(),
                name: "Codex".to_string(),
                url: "vmux://agent/codex/cli".to_string(),
                icon: String::new(),
            }],
            acp: vec![vmux_command::snapshot::AgentProviderSummary {
                id: "claude-acp".to_string(),
                name: "Claude Agent".to_string(),
                url: "vmux://agent/claude".to_string(),
                icon: "https://cdn.example/claude-acp.svg".to_string(),
            }],
            recent: vec![
                AgentPromptTarget::Cli(AgentKind::Codex),
                AgentPromptTarget::Acp {
                    id: "claude".to_string(),
                },
            ],
            ..Default::default()
        };
        let pages = agent_pages(&snapshot);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].url, "vmux://agent/codex/cli");
        assert_eq!(pages[0].title, "Codex (CLI)");
        assert_eq!(pages[0].host, "agent");
        assert_eq!(pages[1].title, "Claude Agent");
        assert!(matches!(
            pages[1].icon,
            vmux_core::PageIcon::Favicon(ref u) if u == "https://cdn.example/claude-acp.svg"
        ));
    }
}
