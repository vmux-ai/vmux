pub(crate) use crate::NewStackContext;
use std::time::{Duration, Instant};

use crate::cef::{Browser, LayoutCef};
use crate::command_bar::panel::CommandBarPanelActive;
use crate::command_bar::state::{CommandBarStateQuery, command_bar_state};
use crate::command_bar::work_snapshot::{update_recent_files_snapshot, update_work_dirs_snapshot};
use crate::event::{
    CommandBarPanelCloseEvent, LAYOUT_COMMAND_BAR_CLOSE_EVENT, LAYOUT_COMMAND_BAR_OPEN_EVENT,
};
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
    PathCompleteResponse, PathEntry, SearchEngine, SearchEngineSetting,
};
use vmux_command::open::OpenCommand;
use vmux_command::open_target::OpenTarget;
use vmux_command::snapshot::{
    CommandBarContributions, CommandBarPagesSnapshot, CommandBarSpacesSnapshot,
    CommandBarTerminalsSnapshot, WriteCommandBarSnapshots,
};
use vmux_command::{
    AppCommand, BrowserBarCommand, BrowserCommand, LayoutCommand, PaneCommand, ReadAppCommands,
    SpaceCommand, StackCommand,
};
use vmux_core::event::space::SpaceCommandEvent;
use vmux_core::page::{SettingsPageSpawnRequest, SpacesPageSpawnRequest};
use vmux_core::terminal::{Terminal, TerminalSpawnRequest};
use vmux_core::{
    PageMetadata, PageOpenRequest, PageOpenTarget, PendingPrompt, PendingPromptAttachments,
};
use vmux_history::{LastActivatedAt, now_millis};
use vmux_ui::i18n::{TranslationValue, requested_locale, translate_for, translate_for_with};

use crate::settings::ResolvedLocale;

pub(crate) use vmux_core::focus_pane_entity;

pub(crate) struct CommandBarInputPlugin;

impl Plugin for CommandBarInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NewStackContext>()
            .add_message::<crate::ContributedCommandChosen>()
            .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
            .add_message::<SettingsPageSpawnRequest>()
            .add_message::<SpacesPageSpawnRequest>()
            .add_plugins(BinEventEmitterPlugin::<(
                CommandBarActionEvent,
                PathCompleteRequest,
                CommandBarReadyEvent,
                CommandBarRenderedEvent,
                CommandBarSizeEvent,
            )>::for_hosts(&[
                "command-bar",
                "start",
                "layout",
            ]))
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

#[derive(Component)]
struct CommandBarOpenedOnce;

#[derive(Component)]
struct CommandBarRecreating;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CommandBarNativeSize {
    pub width: f32,
    pub height: f32,
    pub shell_left: f32,
    pub shell_top: f32,
    pub shell_width: f32,
    pub shell_height: f32,
}

#[derive(Component)]
pub struct PendingCommandBarReveal {
    frames: u8,
    open_id: u64,
    payload: Option<Vec<u8>>,
    started_at: Option<Instant>,
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
const COMMAND_BAR_NATIVE_REVEAL_TIMEOUT: Duration = Duration::from_secs(2);
const COMMAND_BAR_OPEN_RETRY_INTERVAL: Duration = Duration::from_millis(100);

pub struct CommandBarEntry {
    pub id: String,
    pub name: String,
    pub shortcut: String,
}

/// Command ids surfaced through a page entry instead of a command row: the
/// Services page (vmux://services/) replaces "Open Service Monitor", and the
/// History page shows the History shortcut. Their menu items + shortcuts stay.
const COMMAND_BAR_SKIP_IDS: &[&str] = &["service_open", "browser_open_history"];

/// Built-in command rows plus whatever other crates contributed, already named.
pub fn command_list(locale: &str, contributed: Vec<CommandBarEntry>) -> Vec<CommandBarEntry> {
    let mut entries: Vec<CommandBarEntry> = AppCommand::command_bar_entries()
        .into_iter()
        .filter(|(id, _, _)| !COMMAND_BAR_SKIP_IDS.contains(id))
        .map(|(id, name, shortcut)| CommandBarEntry {
            id: id.to_string(),
            name: localized_command_name(locale, id, name),
            shortcut: shortcut.to_string(),
        })
        .collect();
    entries.extend(contributed);
    entries
}

/// Resolve a command-bar menu path for the requested locale.
pub fn localized_command_name(locale: &str, id: &str, fallback: String) -> String {
    let message_id = format!("command-{}", id.replace('_', "-"));
    let translated = translate_for(locale, &message_id);
    if translated == message_id {
        return fallback;
    }
    let Some((root_id, group_id)) = command_hierarchy_ids(id) else {
        return translated;
    };
    let mut segments = translated
        .split(" > ")
        .map(str::to_string)
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return translated;
    }
    segments[0] = translate_for(locale, root_id);
    if let Some(group_id) = group_id
        && segments.len() > 2
    {
        segments[1] = translate_for(locale, group_id);
    }
    segments.join(" > ")
}

fn command_hierarchy_ids(id: &str) -> Option<(&'static str, Option<&'static str>)> {
    if id.starts_with("interactive_mode_") {
        Some(("menu-scene", Some("command-group-interactive-mode")))
    } else if id == "minimize_window" {
        Some(("menu-layout", Some("command-group-window")))
    } else if id == "toggle_layout" {
        Some(("menu-layout", Some("menu-layout")))
    } else if matches!(
        id,
        "close_tab" | "new_task" | "next_tab" | "prev_tab" | "rename_tab"
    ) || id.starts_with("tab_select_")
    {
        Some(("menu-layout", Some("command-group-tab")))
    } else if id.starts_with("open_in_") {
        Some(("menu-browser", Some("command-group-open")))
    } else if id.contains("pane") {
        Some(("menu-layout", Some("command-group-pane")))
    } else if id.starts_with("stack_") {
        Some(("menu-layout", Some("command-group-stack")))
    } else if id == "space_open" {
        Some(("menu-layout", Some("command-group-space")))
    } else if id.starts_with("terminal_") {
        Some(("menu-terminal", None))
    } else if matches!(
        id,
        "browser_prev_page" | "browser_next_page" | "browser_reload" | "browser_hard_reload"
    ) {
        Some(("menu-browser", Some("command-group-navigation")))
    } else if matches!(
        id,
        "browser_zoom_in" | "browser_zoom_out" | "browser_zoom_reset" | "browser_dev_tools"
    ) {
        Some(("menu-browser", Some("command-group-view")))
    } else if id.starts_with("browser_open_") {
        Some(("menu-browser", Some("command-group-bar")))
    } else if id == "service_open" {
        Some(("menu-service", None))
    } else if id.starts_with("bookmark_") {
        Some(("menu-bookmark", None))
    } else {
        None
    }
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

pub fn match_command(id: &str) -> Option<AppCommand> {
    AppCommand::from_menu_id(id)
}

pub fn is_command_bar_open(modal_q: &CommandBarStateQuery) -> bool {
    command_bar_state(modal_q).owns_input()
}

pub fn is_command_bar_visible(modal_q: &CommandBarStateQuery) -> bool {
    command_bar_state(modal_q).is_shown()
}

fn prepare_command_bar_surface(
    modal_node: &mut Node,
    modal_vis: &mut Visibility,
    native_overlay: bool,
) {
    modal_node.display = Display::Flex;
    *modal_vis = if native_overlay {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
}

fn close_command_bar_surface(
    modal_node: &mut Node,
    modal_vis: &mut Visibility,
    native_overlay: bool,
) {
    if native_overlay {
        prepare_command_bar_surface(modal_node, modal_vis, true);
    } else {
        modal_node.display = Display::None;
        *modal_vis = Visibility::Hidden;
    }
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
            Has<WebviewNativeOverlay>,
        ),
        With<Modal>,
    >,
) {
    let Ok((
        modal_e,
        mut modal_node,
        mut modal_vis,
        has_keyboard_target,
        pending_reveal,
        native_overlay,
    )) = modal_q.single_mut()
    else {
        return;
    };
    if has_keyboard_target || pending_reveal {
        return;
    }
    prepare_command_bar_surface(&mut modal_node, &mut modal_vis, native_overlay);
    commands
        .entity(modal_e)
        .insert(Pickable::IGNORE)
        .insert(PendingCommandBarReveal {
            frames: 0,
            open_id: 0,
            payload: None,
            started_at: None,
        });
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
    native_overlay: bool,
    frames: u8,
    open_id: u64,
    rendered_open_id: Option<u64>,
    painted_open_id: Option<u64>,
    has_native_size: bool,
) -> Option<u8> {
    if (native_windowed || native_overlay)
        && open_id != 0
        && (rendered_open_id != Some(open_id) || (native_windowed && !has_native_size))
    {
        return Some(frames.saturating_add(1));
    }
    next_command_bar_reveal_frames(frames, open_id, rendered_open_id, painted_open_id)
}

fn native_command_bar_reveal_timed_out(
    native_windowed: bool,
    native_overlay: bool,
    elapsed: Duration,
    open_id: u64,
    rendered_open_id: Option<u64>,
    has_native_size: bool,
) -> bool {
    (native_windowed || native_overlay)
        && open_id != 0
        && elapsed >= COMMAND_BAR_NATIVE_REVEAL_TIMEOUT
        && (rendered_open_id != Some(open_id) || (native_windowed && !has_native_size))
}

fn should_retry_command_bar_open_payload(
    open_id: u64,
    payload: Option<&[u8]>,
    rendered_open_id: Option<u64>,
) -> bool {
    open_id != 0 && payload.is_some() && rendered_open_id != Some(open_id)
}

fn on_command_bar_ready(
    trigger: On<BinReceive<CommandBarReadyEvent>>,
    mut pending_q: Query<&mut PendingCommandBarReveal>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if let Ok(mut pending) = pending_q.get_mut(webview)
        && pending.open_id != 0
        && pending.started_at.is_none()
    {
        pending.started_at = Some(Instant::now());
    }
    commands
        .entity(webview)
        .insert(CommandBarReady)
        .remove::<CommandBarRecreating>();
}

fn on_command_bar_rendered(
    trigger: On<BinReceive<CommandBarRenderedEvent>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    browsers.set_windowed_focus(&webview, true);
    browsers.execute_js(
        &webview,
        "const input = document.getElementById('command-bar-input'); if (input) { input.focus({ preventScroll: true }); }",
    );
    webview_debug_log(format!(
        "command_bar rendered entity={:?} open_id={}",
        webview,
        trigger.event().payload.open_id
    ));
    commands.entity(webview).insert((
        CommandBarRenderedOpen(trigger.event().payload.open_id),
        CommandBarOpenedOnce,
    ));
}

fn on_command_bar_size(
    trigger: On<BinReceive<CommandBarSizeEvent>>,
    browsers: NonSend<Browsers>,
    state: Query<(
        &Visibility,
        Option<&PendingCommandBarReveal>,
        Option<&CommandBarNativeSize>,
        Has<WebviewWindowed>,
    )>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((visibility, pending_reveal, current_size, native_windowed)) = state.get(webview) else {
        return;
    };
    if !command_bar_size_should_apply(*visibility, pending_reveal) {
        webview_debug_log(format!(
            "command_bar size ignored entity={webview:?} visibility={visibility:?} pending={}",
            pending_reveal.is_some()
        ));
        return;
    }
    let payload = trigger.event().payload;
    if native_windowed
        && let Some(open_id) = pending_reveal
            .filter(|pending| pending.open_id != 0)
            .map(|pending| pending.open_id)
    {
        browsers.set_windowed_focus(&webview, true);
        browsers.execute_js(
            &webview,
            "const input = document.getElementById('command-bar-input'); if (input) { input.focus({ preventScroll: true }); }",
        );
        commands
            .entity(webview)
            .insert((CommandBarRenderedOpen(open_id), CommandBarOpenedOnce));
    }
    if current_size.is_some_and(|size| {
        size.width == payload.width.max(1) as f32
            && size.height == payload.height.max(1) as f32
            && size.shell_left == payload.shell_left as f32
            && size.shell_top == payload.shell_top as f32
            && size.shell_width == payload.shell_width.max(1) as f32
            && size.shell_height == payload.shell_height.max(1) as f32
    }) {
        return;
    }
    webview_debug_log(format!(
        "command_bar size entity={webview:?} width={} height={}",
        payload.width, payload.height
    ));
    commands.entity(webview).insert(CommandBarNativeSize {
        width: payload.width.max(1) as f32,
        height: payload.height.max(1) as f32,
        shell_left: payload.shell_left as f32,
        shell_top: payload.shell_top as f32,
        shell_width: payload.shell_width.max(1) as f32,
        shell_height: payload.shell_height.max(1) as f32,
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

fn command_bar_should_focus_start(
    is_new_stack: bool,
    space_switch: bool,
    active_page_is_start: bool,
    replace_active_stack: bool,
) -> bool {
    !replace_active_stack && !is_new_stack && !space_switch && active_page_is_start
}

fn command_bar_toggle_should_open(is_open: bool, space_switch: bool) -> bool {
    !is_open || space_switch
}

fn handle_open_command_bar(
    mut reader: MessageReader<AppCommand>,
    layout_q: Query<(Entity, Has<CommandBarPanelActive>), With<LayoutCef>>,
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
        Res<CommandBarContributions>,
        Res<CommandBarSpacesSnapshot>,
        ResMut<NewStackContext>,
        Option<Res<crate::settings::EffectiveStartupUrl>>,
        MessageWriter<PageOpenRequest>,
        Res<CommandBarPagesSnapshot>,
        Res<vmux_command::snapshot::CommandBarWorkSnapshot>,
        Option<Res<ResolvedLocale>>,
    )>,
    mut commands: Commands,
) {
    let Ok((layout_e, is_open)) = layout_q.single() else {
        return;
    };
    let active_stack_count = stack_q.iter().count();
    let spaces_snapshot = snapshot_params.p1().clone();
    let space_name = spaces_snapshot.active_space_name.clone();
    let contributions = snapshot_params.p0().clone();
    let startup_url = snapshot_params.p3().map(|url| url.0.clone());
    let pages_snap = snapshot_params.p5().clone();
    let work_snap = snapshot_params.p6().clone();
    let locale = snapshot_params
        .p7()
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(|| requested_locale(None));

    let request = command_bar_open_request(reader.read().cloned());
    let mut should_open = false;
    let should_toggle = request.should_toggle;
    let should_dismiss = request.should_dismiss;
    let should_dismiss_nav = request.should_dismiss_nav;
    let replace_active_stack = request.replace_active_stack;
    let url_override = request.url_override;
    let space_switch = request.space_switch;

    // `Cmd+K` on an open bar closes it, and that has to run the same cleanup as an explicit
    // dismiss: a pending `Cmd+T` stack left alive is an orphan tab, and no browser reclaims
    // `CefKeyboardTarget`.
    let toggle_closes = should_toggle && !command_bar_toggle_should_open(is_open, space_switch);

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

    if (should_dismiss || toggle_closes) && is_open {
        close_command_bar_panel(layout_e, &mut commands);
        let mut new_stack_ctx = snapshot_params.p2();
        // Discard empty tab created by a previous Cmd+T
        if let Some(stack_e) = new_stack_ctx.stack.take() {
            commands.entity(stack_e).despawn();
            if let Some(prev) = new_stack_ctx.previous_stack.take()
                && let Ok(children) = all_children.get(prev)
            {
                for child in children.iter() {
                    if content_browsers.contains(child) {
                        commands.entity(child).try_insert(CefKeyboardTarget);
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
                        commands.entity(browser_e).try_insert(CefKeyboardTarget);
                    }
                }
            }
        }
        new_stack_ctx.needs_open = false;
        return;
    }

    // Navigation dismiss: close the panel only, leave empty tab for
    // handle_tab_commands / on_pane_select to clean up.
    if should_dismiss_nav && is_open {
        close_command_bar_panel(layout_e, &mut commands);
        snapshot_params.p2().needs_open = false;
        return;
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

    if should_toggle && !toggle_closes {
        should_open = true;
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
        if command_bar_should_focus_start(
            is_new_stack,
            space_switch,
            start_browser.is_some(),
            replace_active_stack,
        ) && let Some(browser_e) = start_browser
        {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                browser_e,
                START_FOCUS_INPUT_EVENT,
                &StartFocusInput,
            ));
            return;
        }
    }

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
        &locale,
    );

    let target = if replace_active_stack {
        Some(vmux_command::open_target::OpenTarget::InPlace)
    } else if is_new_stack {
        Some(vmux_command::open_target::OpenTarget::InNewStack)
    } else {
        None
    };
    let mut payload = build_command_bar_open_payload(
        now_millis() as u64,
        false,
        space_name,
        current_url,
        &spaces_snapshot,
        &contributions,
        &pages_snap,
        &work_snap,
        &locale,
        active_stack_count,
        bar_tabs,
        target,
    );
    payload.space_switch = space_switch;
    commands.trigger(BinHostEmitEvent::from_rkyv(
        layout_e,
        LAYOUT_COMMAND_BAR_OPEN_EVENT,
        &payload,
    ));
}

/// Asks the layout page to unmount the panel.
///
/// The host cannot clear `CommandBarPanelActive` itself: the page owns the marker and removes it
/// on unmount, so clearing it here would hand the keyboard back to the pane a frame before the
/// panel actually goes away.
fn close_command_bar_panel(layout: Entity, commands: &mut Commands) {
    commands.trigger(BinHostEmitEvent::from_rkyv(
        layout,
        LAYOUT_COMMAND_BAR_CLOSE_EVENT,
        &CommandBarPanelCloseEvent,
    ));
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
    search_engines: Vec<SearchEngine>,
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
        search_engines,
        prompt_context: default(),
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
    locale: &str,
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
            let pane_number = pane_pos as i64 + 1;
            let stack_number = tab_index as i64 + 1;
            let location = if space_name.is_empty() {
                translate_for_with(
                    locale,
                    "command-pane-stack-location",
                    &[
                        ("pane", TranslationValue::Number(pane_number)),
                        ("stack", TranslationValue::Number(stack_number)),
                    ],
                )
            } else {
                translate_for_with(
                    locale,
                    "command-space-pane-stack-location",
                    &[
                        ("space", TranslationValue::String(space_name)),
                        ("pane", TranslationValue::Number(pane_number)),
                        ("stack", TranslationValue::Number(stack_number)),
                    ],
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
    contributions: &CommandBarContributions,
    pages_snapshot: &CommandBarPagesSnapshot,
    work_snapshot: &vmux_command::snapshot::CommandBarWorkSnapshot,
    locale: &str,
    active_stack_count: usize,
    tabs: Vec<CommandBarTab>,
    target: Option<OpenTarget>,
) -> CommandBarOpenEvent {
    let mut contributed = Vec::with_capacity(contributions.commands.len());
    for command in &contributions.commands {
        let args: Vec<(&str, TranslationValue<'_>)> = command
            .args
            .iter()
            .map(|(name, value)| (name.as_str(), TranslationValue::String(value)))
            .collect();
        contributed.push(CommandBarEntry {
            id: command.id.clone(),
            name: translate_for_with(locale, &command.message_id, &args),
            shortcut: String::new(),
        });
    }
    let mut pages = pages_snapshot.pages.clone();
    for page in &mut pages {
        if let Some(message_id) = page_title_message_id(&page.host) {
            page.title = translate_for(locale, message_id);
        }
    }
    pages.extend(contributions.pages.iter().map(|entry| entry.page.clone()));
    let history_shortcut = command_shortcut("browser_open_history");
    if !history_shortcut.is_empty()
        && let Some(page) = pages.iter_mut().find(|page| page.host == "history")
    {
        page.shortcut = history_shortcut;
    }
    let commands: Vec<CommandBarCommandEntry> = command_list(locale, contributed)
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
        work_snapshot.search_engines.clone(),
    )
}

fn page_title_message_id(host: &str) -> Option<&'static str> {
    match host {
        "agents" => Some("agents-title"),
        "extensions" => Some("extensions-title"),
        "history" => Some("history-title"),
        "lsp" => Some("lsp-title"),
        "services" => Some("services-title"),
        "settings" => Some("settings-title"),
        "spaces" => Some("spaces-title"),
        "start" => Some("start-title"),
        "team" => Some("team-title"),
        "terminal" => Some("command-terminal"),
        _ => None,
    }
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
    webview_sources: Query<'w, 's, &'static WebviewSource>,
}

fn inline_transition_stack_for(
    webview: Entity,
    queries: &CommandBarActionQueries,
) -> Option<Entity> {
    let WebviewSource::Url(url) = queries.webview_sources.get(webview).ok()? else {
        return None;
    };
    if !url.starts_with(crate::start::START_PAGE_URL) {
        return None;
    }
    queries.child_of_q.get(webview).ok().map(|parent| parent.0)
}

fn mark_inline_transition(stack: Entity, webview: Entity, commands: &mut Commands) {
    commands
        .entity(stack)
        .insert(crate::start::StartInlineTransition { webview });
    commands
        .entity(webview)
        .insert(crate::start::StartInlineTransitionView);
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

fn normalize_url(value: &str, search_engine: SearchEngine) -> String {
    let value = value.trim();
    if vmux_command::event::is_data_uri(value)
        || (value.contains("://") && vmux_command::event::looks_like_url(value))
    {
        value.to_string()
    } else if vmux_command::event::looks_like_url(value) {
        format!("https://{}", value)
    } else {
        search_engine.search_url(value)
    }
}

fn on_command_bar_action(
    trigger: On<BinReceive<CommandBarActionEvent>>,
    search_engine: Option<Res<SearchEngineSetting>>,
    mut modal_q: Query<
        (
            Entity,
            &mut Node,
            &mut Visibility,
            Has<WebviewNativeOverlay>,
        ),
        With<Modal>,
    >,
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
        Res<CommandBarContributions>,
        Option<Res<ResolvedLocale>>,
    )>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut writer_params: ParamSet<(
        MessageWriter<AppCommand>,
        MessageWriter<PageOpenRequest>,
        MessageWriter<TerminalSpawnRequest>,
    )>,
    mut chosen_writer: MessageWriter<crate::ContributedCommandChosen>,
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
    let inline_transition_stack = inline_transition_stack_for(webview, &queries);
    let locale = resource_params
        .p3()
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(|| requested_locale(None));
    match evt.action.as_str() {
        "prompt" => {
            let prompt = evt.value.trim();
            let attachments = evt
                .attachments
                .iter()
                .filter(|attachment| !attachment.path.is_empty())
                .map(|attachment| vmux_wire::protocol::AgentAttachment {
                    path: attachment.path.clone(),
                    name: attachment.name.clone(),
                    mime_type: attachment.mime_type.clone(),
                    size: attachment.size,
                })
                .collect::<Vec<_>>();
            if !prompt.is_empty() || !attachments.is_empty() {
                let (_, _, focused_stack) = focused_stack(
                    queries.active_tab_param.get(),
                    &queries.all_children,
                    &queries.leaf_panes,
                    &queries.pane_ts,
                    &queries.pane_children,
                    &queries.stack_ts,
                );
                if let Some(stack) = empty_stack.or(focused_stack)
                    && let Some(url) = resource_params.p2().prompt_url(evt.target_url.as_deref())
                {
                    if inline_transition_stack == Some(stack)
                        && crate::start::supports_inline_agent_transition(&url)
                    {
                        mark_inline_transition(stack, webview, &mut commands);
                    }
                    commands
                        .entity(stack)
                        .insert(PendingPrompt(prompt.to_string()));
                    if !attachments.is_empty() {
                        commands
                            .entity(stack)
                            .insert(PendingPromptAttachments(attachments));
                    } else {
                        commands.entity(stack).remove::<PendingPromptAttachments>();
                    }
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
                        title: translate_for_with(
                            &locale,
                            "command-terminal-path",
                            &[("path", TranslationValue::String(&dir.display().to_string()))],
                        ),
                        ..default()
                    });
                    writer_params.p2().write(TerminalSpawnRequest {
                        cwd: Some(dir.to_path_buf()),
                        target_stack: Some(stack_e),
                    });
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                    custom_keyboard_restore = true;
                }
            } else {
                let url = normalize_url(
                    &evt.value,
                    search_engine.map(|setting| setting.0).unwrap_or_default(),
                );
                let inline_transition = if matches!(evt.target, None | Some(OpenTarget::InPlace))
                    && crate::start::supports_inline_agent_transition(&url)
                    && let Some(stack) = inline_transition_stack
                {
                    mark_inline_transition(stack, webview, &mut commands);
                    true
                } else {
                    false
                };
                if !inline_transition && resource_params.p2().claims_url(&url) {
                    if let Some(stack_e) = empty_stack {
                        chosen_writer.write(crate::ContributedCommandChosen {
                            id: url.clone(),
                            stack: Some(stack_e),
                            pane: None,
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
                            chosen_writer.write(crate::ContributedCommandChosen {
                                id: url.clone(),
                                stack: None,
                                pane: Some(pane_e),
                            });
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
                        title: translate_for(&locale, "command-terminal"),
                        ..default()
                    });
                    writer_params.p2().write(TerminalSpawnRequest {
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
                            title: translate_for(&locale, "command-terminal"),
                            ..default()
                        });
                        writer_params.p2().write(TerminalSpawnRequest {
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
            let is_contributed = resource_params
                .p2()
                .commands
                .iter()
                .any(|command| command.id == evt.value);
            if is_contributed {
                let pane = match empty_stack {
                    Some(_) => None,
                    None => {
                        let (_, active_pane_opt, _) = focused_stack(
                            queries.active_tab_param.get(),
                            &queries.all_children,
                            &queries.leaf_panes,
                            &queries.pane_ts,
                            &queries.pane_children,
                            &queries.stack_ts,
                        );
                        active_pane_opt
                    }
                };
                if let Some(stack_e) = empty_stack {
                    commands.entity(stack_e).insert(LastActivatedAt::now());
                    if let Ok(parent) = queries.child_of_q.get(stack_e) {
                        commands.entity(parent.0).insert(LastActivatedAt::now());
                    }
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                }
                if empty_stack.is_some() || pane.is_some() {
                    chosen_writer.write(crate::ContributedCommandChosen {
                        id: evt.value.clone(),
                        stack: empty_stack,
                        pane,
                    });
                    custom_keyboard_restore = true;
                }
            } else if let Some(url) = resource_params.p2().page_url(&evt.value) {
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
                                commands.entity(child).try_insert(CefKeyboardTarget);
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
    if let Ok((modal_e, mut modal_node, mut modal_vis, native_overlay)) = modal_q.single_mut() {
        close_command_bar_surface(&mut modal_node, &mut modal_vis, native_overlay);
        commands
            .entity(modal_e)
            .insert(Pickable::IGNORE)
            .remove::<CefKeyboardTarget>()
            .remove::<CefPointerTarget>()
            .remove::<CommandBarRenderedOpen>()
            .remove::<CommandBarPaintedOpen>()
            .remove::<PendingCommandBarReveal>()
            .remove::<CommandBarRecreating>();
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
                    commands.entity(browser_e).try_insert(CefKeyboardTarget);
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
    mut modal_q: Query<
        (
            Entity,
            &mut Node,
            &mut Visibility,
            Has<WebviewNativeOverlay>,
        ),
        With<Modal>,
    >,
    mut commands: Commands,
) {
    if !new_stack_ctx.dismiss_modal {
        return;
    }
    new_stack_ctx.dismiss_modal = false;
    if let Ok((modal_e, mut modal_node, mut modal_vis, native_overlay)) = modal_q.single_mut()
        && modal_node.display != Display::None
    {
        close_command_bar_surface(&mut modal_node, &mut modal_vis, native_overlay);
        commands
            .entity(modal_e)
            .insert(Pickable::IGNORE)
            .remove::<CefKeyboardTarget>()
            .remove::<CefPointerTarget>()
            .remove::<CommandBarRenderedOpen>()
            .remove::<CommandBarPaintedOpen>()
            .remove::<PendingCommandBarReveal>()
            .remove::<CommandBarRecreating>();
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
            Has<WebviewNativeOverlay>,
        ),
        With<Modal>,
    >,
) {
    for (
        entity,
        mut vis,
        mut pending,
        rendered,
        painted,
        native_size,
        native_windowed,
        native_overlay,
    ) in &mut query
    {
        let rendered_open_id = rendered.map(|rendered| rendered.0);
        let painted_open_id = painted.map(|painted| painted.0);
        let elapsed = pending
            .started_at
            .map(|started_at| started_at.elapsed())
            .unwrap_or_default();
        if native_command_bar_reveal_timed_out(
            native_windowed,
            native_overlay,
            elapsed,
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
                    target_url: None,
                    attachments: Vec::new(),
                },
            });
            continue;
        }
        match next_command_bar_reveal_frames_for_backend(
            native_windowed,
            native_overlay,
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
    mut query: Query<
        (
            Entity,
            &mut PendingCommandBarReveal,
            Option<&CommandBarRenderedOpen>,
            Has<CommandBarRecreating>,
        ),
        With<Modal>,
    >,
    mut last_emit: Local<std::collections::HashMap<Entity, Instant>>,
) {
    let now = Instant::now();
    for (entity, mut pending, rendered, recreating) in &mut query {
        if recreating {
            continue;
        }
        let rendered_open_id = rendered.map(|rendered| rendered.0);
        let Some(payload) = pending.payload.as_deref() else {
            continue;
        };
        if !should_retry_command_bar_open_payload(pending.open_id, Some(payload), rendered_open_id)
        {
            last_emit.remove(&entity);
            continue;
        }
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        if last_emit
            .get(&entity)
            .is_some_and(|last| now.duration_since(*last) < COMMAND_BAR_OPEN_RETRY_INTERVAL)
        {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_bytes(
            entity,
            COMMAND_BAR_OPEN_EVENT,
            payload.to_vec(),
        ));
        pending.started_at.get_or_insert(now);
        last_emit.insert(entity, now);
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
#[path = "handler.test.rs"]
mod tests;
