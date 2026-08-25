use crate::CommandBar;
use crate::build_command_bar_open_payload;
use std::time::{Duration, Instant};
pub(crate) use vmux_core::launcher::PendingLaunch;
use vmux_core::launcher::{
    FocusLauncherInput, HostsLauncher, InlineTransitionRequested, PendingStackAbandoned,
    RendersLauncherPanel, RestoreKeyboardToStack, StackInPaneChosen,
};

use crate::command_bar::panel::CommandBarPanelActive;
use crate::command_bar::state::{CommandBarStateQuery, command_bar_state};
use crate::command_bar::work_snapshot::{update_recent_files_snapshot, update_work_dirs_snapshot};
use crate::event::{
    COMMAND_BAR_OPEN_EVENT, CommandBarActionEvent, CommandBarReadyEvent, CommandBarRenderedEvent,
    CommandBarSizeEvent, OpenId, PATH_COMPLETE_RESPONSE, PathCompleteRequest, PathCompleteResponse,
    PathEntry, SearchEngine, SearchEngineSetting,
};
use crate::event::{
    CommandBarPanelCloseEvent, LAYOUT_COMMAND_BAR_CLOSE_EVENT, LAYOUT_COMMAND_BAR_OPEN_EVENT,
};
use crate::open::OpenCommand;
use crate::open_target::OpenTarget;
use crate::snapshot::{
    CommandBarPagesSnapshot, CommandBarSpacesSnapshot, CommandBarTerminalsSnapshot,
    CommandBarWorkspaceSnapshot, Contributions, WriteCommandBarSnapshots,
};
use crate::{
    AppCommand, BrowserBarCommand, BrowserCommand, LayoutCommand, PaneCommand, ReadAppCommands,
    SpaceCommand, StackCommand,
};
use bevy::{ecs::message::MessageReader, ecs::system::SystemParam, prelude::*};
use bevy_cef::prelude::*;
use vmux_core::event::space::SpaceCommandEvent;
use vmux_core::host::page::HostsPage;
use vmux_core::page::{SettingsPageSpawnRequest, SpacesPageSpawnRequest};
use vmux_core::terminal::{TerminalSpawnRequest, TerminalSpawnTarget};
use vmux_core::{
    PageMetadata, PageOpenRequest, PageOpenTarget, PendingPrompt, PendingPromptAttachments,
};
use vmux_history::{LastActivatedAt, now_millis};
use vmux_ui::i18n::{Locale, TranslationValue};

use crate::ResolvedLocale;
use vmux_core::KeyboardOwner;
use vmux_flex::prelude::*;

pub(crate) use vmux_core::focus_pane_entity;

pub(crate) struct CommandBarInputPlugin;

impl Plugin for CommandBarInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingLaunch>()
            .add_message::<vmux_core::ContributedCommandChosen>()
            .add_message::<PendingStackAbandoned>()
            .add_message::<FocusLauncherInput>()
            .add_message::<InlineTransitionRequested>()
            .add_message::<StackInPaneChosen>()
            .add_message::<RestoreKeyboardToStack>()
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
                    .after(vmux_core::workspace::TabCommandSet)
                    .after(vmux_core::workspace::StackCommandSet),
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
                    .before(vmux_core::workspace::ComputeFocusSet),
            )
            .add_systems(
                PostUpdate,
                reveal_command_bar.chain().after(LayoutSystems::Layout),
            );
    }
}

#[derive(Component)]
struct CommandBarReady;

#[derive(Component)]
struct CommandBarRenderedOpen(OpenId);

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
    open_id: OpenId,
    payload: Option<Vec<u8>>,
    started_at: Option<Instant>,
}

impl PendingCommandBarReveal {
    pub fn is_active(&self) -> bool {
        self.open_id.is_open()
    }
}

const COMMAND_BAR_REVEAL_FRAMES: u8 = 2;
const COMMAND_BAR_REVEAL_FALLBACK_FRAMES: u8 = 10;
const COMMAND_BAR_NATIVE_REVEAL_TIMEOUT: Duration = Duration::from_secs(2);
const COMMAND_BAR_OPEN_RETRY_INTERVAL: Duration = Duration::from_millis(100);

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
        Visibility::Visible
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
            Has<KeyboardOwner>,
            Has<PendingCommandBarReveal>,
            Has<WebviewNativeOverlay>,
        ),
        With<CommandBar>,
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
    commands.entity(modal_e).insert(PendingCommandBarReveal {
        frames: 0,
        open_id: OpenId::NONE,
        payload: None,
        started_at: None,
    });
}

fn next_command_bar_reveal_frames(
    frames: u8,
    open_id: OpenId,
    rendered_open_id: Option<OpenId>,
) -> Option<u8> {
    if !open_id.is_open() {
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
    open_id: OpenId,
    rendered_open_id: Option<OpenId>,
    has_native_size: bool,
) -> Option<u8> {
    if (native_windowed || native_overlay)
        && open_id.is_open()
        && (rendered_open_id != Some(open_id) || (native_windowed && !has_native_size))
    {
        return Some(frames.saturating_add(1));
    }
    next_command_bar_reveal_frames(frames, open_id, rendered_open_id)
}

fn native_command_bar_reveal_timed_out(
    native_windowed: bool,
    native_overlay: bool,
    elapsed: Duration,
    open_id: OpenId,
    rendered_open_id: Option<OpenId>,
    has_native_size: bool,
) -> bool {
    (native_windowed || native_overlay)
        && open_id.is_open()
        && elapsed >= COMMAND_BAR_NATIVE_REVEAL_TIMEOUT
        && (rendered_open_id != Some(open_id) || (native_windowed && !has_native_size))
}

fn should_retry_command_bar_open_payload(
    open_id: OpenId,
    payload: Option<&[u8]>,
    rendered_open_id: Option<OpenId>,
) -> bool {
    open_id.is_open() && payload.is_some() && rendered_open_id != Some(open_id)
}

fn on_command_bar_ready(
    trigger: On<BinReceive<CommandBarReadyEvent>>,
    mut pending_q: Query<&mut PendingCommandBarReveal>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if let Ok(mut pending) = pending_q.get_mut(webview)
        && pending.open_id.is_open()
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
        return;
    }
    let payload = trigger.event().payload;
    if native_windowed
        && let Some(open_id) = pending_reveal
            .filter(|pending| pending.open_id.is_open())
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
        || pending_reveal
            .is_some_and(|pending| pending.open_id.is_open() && pending.payload.is_some())
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
    pending_launch: &mut PendingLaunch,
    startup_url: Option<&str>,
) -> Option<PageOpenRequest> {
    if !pending_launch.needs_open {
        return None;
    }
    let stack = pending_launch.stack?;
    let url = startup_url.filter(|url| !url.is_empty())?;
    pending_launch.stack = None;
    pending_launch.previous_stack = None;
    pending_launch.needs_open = false;
    Some(PageOpenRequest {
        target: PageOpenTarget::Stack(stack),
        url: url.to_string(),
        request_id: None,
    })
}

fn command_bar_should_open_pending_stack(
    pending_launch: &mut PendingLaunch,
    explicit_toggle: bool,
) -> bool {
    if explicit_toggle {
        pending_launch.needs_open = false;
        return false;
    }
    if pending_launch.needs_open {
        pending_launch.needs_open = false;
        true
    } else {
        false
    }
}

fn command_bar_cancel_pending_stack_for_active_open(
    pending_launch: &mut PendingLaunch,
    replace_active_stack: bool,
) -> Option<(Entity, Option<Entity>)> {
    if !replace_active_stack {
        return None;
    }
    pending_launch.needs_open = false;
    let previous_stack = pending_launch.previous_stack.take();
    let stack = pending_launch.stack.take()?;
    Some((stack, previous_stack))
}

#[derive(SystemParam)]
struct LauncherHosts<'w, 's> {
    pages: Query<'w, 's, (), With<HostsLauncher>>,
    focus: MessageWriter<'w, FocusLauncherInput>,
}

impl LauncherHosts<'_, '_> {
    fn hosts_one(&self, webview: Entity) -> bool {
        self.pages.contains(webview)
    }

    fn focus(&mut self, webview: Entity) {
        self.focus.write(FocusLauncherInput { webview });
    }
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
    layout_q: Query<(Entity, Has<CommandBarPanelActive>), With<RendersLauncherPanel>>,
    all_children: Query<&Children>,
    browser_meta: Query<&PageMetadata, Or<(With<WebviewSource>, With<HostsPage>)>>,
    focus: Res<CommandBarWorkspaceSnapshot>,
    mut restore_keyboard: MessageWriter<RestoreKeyboardToStack>,
    mut abandoned: MessageWriter<PendingStackAbandoned>,
    mut launcher_hosts: LauncherHosts,
    child_of_q: Query<&ChildOf>,
    contributions: Contributions,
    mut snapshot_params: ParamSet<(
        Res<CommandBarSpacesSnapshot>,
        ResMut<PendingLaunch>,
        Option<Res<vmux_core::EffectiveStartupUrl>>,
        MessageWriter<PageOpenRequest>,
        Res<CommandBarPagesSnapshot>,
        Res<crate::snapshot::CommandBarWorkSnapshot>,
        Option<Res<ResolvedLocale>>,
    )>,
    mut commands: Commands,
) {
    let Ok((layout_e, is_open)) = layout_q.single() else {
        return;
    };
    let active_stack_count = focus.stack_count;
    let spaces_snapshot = snapshot_params.p0().clone();
    let space_name = spaces_snapshot.active_space_name.clone();
    let startup_url = snapshot_params.p2().map(|url| url.0.clone());
    let pages_snap = snapshot_params.p4().clone();
    let work_snap = snapshot_params.p5().clone();
    let locale = snapshot_params
        .p6()
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(Locale::preferred);

    let request = command_bar_open_request(reader.read().cloned());
    let mut should_open = false;
    let should_toggle = request.should_toggle;
    let should_dismiss = request.should_dismiss;
    let should_dismiss_nav = request.should_dismiss_nav;
    let replace_active_stack = request.replace_active_stack;
    let url_override = request.url_override;
    let space_switch = request.space_switch;

    let toggle_closes = should_toggle && !command_bar_toggle_should_open(is_open, space_switch);

    let mut active_stack_override = None;
    let canceled_pending_stack = {
        let mut pending_launch = snapshot_params.p1();
        command_bar_cancel_pending_stack_for_active_open(&mut pending_launch, replace_active_stack)
    };
    if let Some((stack, previous_stack)) = canceled_pending_stack {
        abandoned.write(PendingStackAbandoned {
            stack,
            previous_stack,
        });
        if let Some(previous_stack) = previous_stack {
            active_stack_override = Some(previous_stack);
            focus_pane_entity(previous_stack, &mut commands, &child_of_q);
        }
    }

    if (should_dismiss || toggle_closes) && is_open {
        close_command_bar_panel(layout_e, &mut commands);
        let mut pending_launch = snapshot_params.p1();
        if let Some(stack_e) = pending_launch.stack.take() {
            abandoned.write(PendingStackAbandoned {
                stack: stack_e,
                previous_stack: pending_launch.previous_stack.take(),
            });
        } else {
            if let Some(stack) = focus.stack {
                restore_keyboard.write(RestoreKeyboardToStack { stack });
            }
        }
        pending_launch.needs_open = false;
        return;
    }

    if should_dismiss_nav && is_open {
        close_command_bar_panel(layout_e, &mut commands);
        snapshot_params.p1().needs_open = false;
        return;
    }

    let startup_request = {
        let mut pending_launch = snapshot_params.p1();
        pending_stack_startup_url_request(&mut pending_launch, startup_url.as_deref())
    };
    if let Some(request) = startup_request {
        snapshot_params.p3().write(request);
        return;
    }

    let should_open_pending_stack = {
        let mut pending_launch = snapshot_params.p1();
        command_bar_should_open_pending_stack(&mut pending_launch, should_toggle)
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

    let is_new_stack = snapshot_params.p1().stack.is_some();

    if !is_new_stack {
        let active_stack = active_stack_override.or(focus.stack);
        let start_browser = active_stack.and_then(|stack| {
            all_children
                .get(stack)
                .ok()
                .and_then(|children| children.iter().find(|e| launcher_hosts.hosts_one(*e)))
        });
        if command_bar_should_focus_start(
            is_new_stack,
            space_switch,
            start_browser.is_some(),
            replace_active_stack,
        ) && let Some(browser_e) = start_browser
        {
            launcher_hosts.focus(browser_e);
            return;
        }
    }

    let current_url = if let Some(override_url) = url_override {
        override_url
    } else if is_new_stack {
        String::new()
    } else {
        let active_stack = active_stack_override.or(focus.stack);
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

    let bar_tabs = focus.tabs.clone();

    let target = if replace_active_stack {
        Some(crate::open_target::OpenTarget::InPlace)
    } else if is_new_stack {
        Some(crate::open_target::OpenTarget::InNewStack)
    } else {
        None
    };
    let mut payload = build_command_bar_open_payload(
        OpenId(now_millis() as u64),
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

fn close_command_bar_panel(layout: Entity, commands: &mut Commands) {
    commands.trigger(BinHostEmitEvent::from_rkyv(
        layout,
        LAYOUT_COMMAND_BAR_CLOSE_EVENT,
        &CommandBarPanelCloseEvent,
    ));
}

#[derive(SystemParam)]
struct CommandBarActionQueries<'w, 's> {
    child_of_q: Query<'w, 's, &'static ChildOf>,
    launcher_hosts: Query<'w, 's, (), With<HostsLauncher>>,
    focus: Res<'w, CommandBarWorkspaceSnapshot>,
}

impl CommandBarActionQueries<'_, '_> {
    fn focused_stack(&self) -> Option<Entity> {
        self.focus.stack
    }

    fn focused_pane(&self) -> Option<Entity> {
        self.focus.pane
    }

    fn inline_transition_stack(&self, webview: Entity) -> Option<Entity> {
        if !self.launcher_hosts.contains(webview) {
            return None;
        }
        self.child_of_q.get(webview).ok().map(|parent| parent.0)
    }
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
    if crate::event::is_data_uri(value)
        || (value.contains("://") && crate::event::looks_like_url(value))
    {
        value.to_string()
    } else if crate::event::looks_like_url(value) {
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
        With<CommandBar>,
    >,
    queries: CommandBarActionQueries,
    mut resource_params: ParamSet<(
        Res<CommandBarSpacesSnapshot>,
        Res<CommandBarTerminalsSnapshot>,
        Contributions,
        Option<Res<ResolvedLocale>>,
    )>,
    mut pending_launch: ResMut<PendingLaunch>,
    mut writer_params: ParamSet<(
        MessageWriter<AppCommand>,
        MessageWriter<PageOpenRequest>,
        MessageWriter<TerminalSpawnRequest>,
    )>,
    mut chosen_writer: MessageWriter<vmux_core::ContributedCommandChosen>,
    mut abandoned_writer: MessageWriter<PendingStackAbandoned>,
    mut inline_transition: MessageWriter<InlineTransitionRequested>,
    mut stack_chosen: MessageWriter<StackInPaneChosen>,
    mut restore_keyboard: MessageWriter<RestoreKeyboardToStack>,
    mut issued: MessageWriter<crate::CommandIssued>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let evt = &trigger.event().payload;
    let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
    let terminals_snapshot = resource_params.p1().clone();
    let terminal_page_url = terminals_snapshot.terminal_page_url.clone();
    let running_terminals = terminals_snapshot.running.clone();
    let mut empty_stack = pending_launch.stack;
    let previous_stack = pending_launch.previous_stack;
    let mut custom_keyboard_restore = false;
    let inline_transition_stack = queries.inline_transition_stack(webview);
    let locale = resource_params
        .p3()
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(Locale::preferred);
    match evt {
        CommandBarActionEvent::Prompt {
            text,
            target_url,
            attachments: submitted,
        } => {
            let prompt = text.trim();
            let attachments = submitted
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
                let focused = queries.focused_stack();
                if let Some(stack) = empty_stack.or(focused)
                    && let Some(url) = resource_params.p2().prompt_url(target_url.as_deref())
                {
                    if inline_transition_stack == Some(stack)
                        && vmux_wire::agent::supports_inline_agent_transition(&url)
                    {
                        inline_transition.write(InlineTransitionRequested { stack, webview });
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
                    pending_launch.stack = None;
                    pending_launch.previous_stack = None;
                    custom_keyboard_restore = true;
                }
            }
        }
        CommandBarActionEvent::Open { value, open } => {
            let expanded = if value.starts_with('~') {
                std::env::var("HOME")
                    .ok()
                    .map(|h| std::path::PathBuf::from(h).join(value[1..].trim_start_matches('/')))
                    .unwrap_or_else(|| std::path::PathBuf::from(&value))
            } else if value.starts_with('/') {
                std::path::PathBuf::from(&value)
            } else {
                std::env::var("HOME")
                    .ok()
                    .map(|h| std::path::PathBuf::from(h).join(value))
                    .unwrap_or_else(|| std::path::PathBuf::from(&value))
            };
            let is_path = expanded.exists();

            if is_path {
                let dir = if expanded.is_dir() {
                    &expanded
                } else {
                    expanded.parent().unwrap_or(&expanded)
                };
                if let Some(stack_e) = empty_stack {
                    writer_params.p2().write(TerminalSpawnRequest {
                        cwd: Some(dir.to_path_buf()),
                        target: TerminalSpawnTarget::Stack(stack_e),
                        metadata: Some(PageMetadata {
                            url: terminal_page_url.clone(),
                            title: locale.translate_with(
                                "command-terminal-path",
                                &[("path", TranslationValue::String(&dir.display().to_string()))],
                            ),
                            ..default()
                        }),
                    });
                    pending_launch.stack = None;
                    pending_launch.previous_stack = None;
                    custom_keyboard_restore = true;
                }
            } else {
                let url = normalize_url(
                    value,
                    search_engine.map(|setting| setting.0).unwrap_or_default(),
                );
                let inline_transition = if matches!(open, None | Some(OpenTarget::InPlace))
                    && vmux_wire::agent::supports_inline_agent_transition(&url)
                    && let Some(stack) = inline_transition_stack
                {
                    inline_transition.write(InlineTransitionRequested { stack, webview });
                    true
                } else {
                    false
                };
                if !inline_transition && resource_params.p2().claims_url(&url) {
                    if let Some(stack_e) = empty_stack {
                        chosen_writer.write(vmux_core::ContributedCommandChosen {
                            id: url.clone(),
                            stack: Some(stack_e),
                            pane: None,
                        });
                        pending_launch.stack = None;
                        pending_launch.previous_stack = None;
                        custom_keyboard_restore = true;
                    } else {
                        let active_pane_opt = queries.focused_pane();
                        if let Some(pane_e) = active_pane_opt {
                            chosen_writer.write(vmux_core::ContributedCommandChosen {
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
                    pending_launch.stack = None;
                    pending_launch.previous_stack = None;
                    custom_keyboard_restore = true;
                } else {
                    let target = *open;
                    let cmd =
                        AppCommand::Browser(BrowserCommand::Open(build_open_command(target, url)));
                    issued.write(crate::CommandIssued {
                        caller,
                        command: cmd.clone(),
                    });
                    writer_params.p0().write(cmd);
                }
            }
        }
        CommandBarActionEvent::Terminal { value } => {
            let known_terminal = running_terminals.get(value).copied();
            if let Some(entity) = known_terminal {
                focus_pane_entity(entity, &mut commands, &queries.child_of_q);
                pending_launch.stack = None;
                pending_launch.previous_stack = None;
                custom_keyboard_restore = true;
            } else {
                if value.starts_with(&terminal_page_url) {
                    bevy::log::warn!("no terminal pane for {}; spawning new", value);
                }
                let cwd = if value.is_empty() || value.contains("://") {
                    None
                } else {
                    let expanded = if value.starts_with("~/") {
                        std::env::var("HOME")
                            .map(|h| std::path::PathBuf::from(h).join(&value[2..]))
                            .unwrap_or_else(|_| std::path::PathBuf::from(&value))
                    } else if value.starts_with('/') {
                        std::path::PathBuf::from(&value)
                    } else {
                        std::env::var("HOME")
                            .map(|h| std::path::PathBuf::from(h).join(value))
                            .unwrap_or_else(|_| std::path::PathBuf::from(&value))
                    };
                    Some(expanded)
                };
                if let Some(stack_e) = empty_stack {
                    writer_params.p2().write(TerminalSpawnRequest {
                        cwd: cwd.clone(),
                        target: TerminalSpawnTarget::Stack(stack_e),
                        metadata: Some(PageMetadata {
                            url: terminal_page_url.clone(),
                            title: locale.translate("command-terminal"),
                            ..default()
                        }),
                    });
                    pending_launch.stack = None;
                    pending_launch.previous_stack = None;
                    custom_keyboard_restore = true;
                } else {
                    let active_pane_opt = queries.focused_pane();
                    if let Some(pane_e) = active_pane_opt {
                        writer_params.p2().write(TerminalSpawnRequest {
                            cwd: cwd.clone(),
                            target: TerminalSpawnTarget::NewStackInPane(pane_e),
                            metadata: Some(PageMetadata {
                                url: terminal_page_url.clone(),
                                title: locale.translate("command-terminal"),
                                ..default()
                            }),
                        });
                    } else {
                        let cmd =
                            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
                                url: Some("vmux://terminal/".into()),
                            }));
                        issued.write(crate::CommandIssued {
                            caller,
                            command: cmd.clone(),
                        });
                        writer_params.p0().write(cmd);
                    }
                }
            }
        }
        CommandBarActionEvent::Command { id, open } => {
            let is_contributed = resource_params
                .p2()
                .commands()
                .any(|command| &command.id == id);
            if is_contributed {
                let pane = match empty_stack {
                    Some(_) => None,
                    None => queries.focused_pane(),
                };
                if let Some(stack_e) = empty_stack {
                    commands.entity(stack_e).insert(LastActivatedAt::now());
                    if let Ok(parent) = queries.child_of_q.get(stack_e) {
                        commands.entity(parent.0).insert(LastActivatedAt::now());
                    }
                    pending_launch.stack = None;
                    pending_launch.previous_stack = None;
                }
                if empty_stack.is_some() || pane.is_some() {
                    chosen_writer.write(vmux_core::ContributedCommandChosen {
                        id: id.clone(),
                        stack: empty_stack,
                        pane,
                    });
                    custom_keyboard_restore = true;
                }
            } else if let Some(url) = resource_params.p2().page_url(id) {
                if let Some(stack_e) = empty_stack {
                    writer_params.p1().write(PageOpenRequest {
                        target: PageOpenTarget::Stack(stack_e),
                        url,
                        request_id: None,
                    });
                    pending_launch.stack = None;
                    pending_launch.previous_stack = None;
                    empty_stack = None;
                } else {
                    let target = *open;
                    let cmd =
                        AppCommand::Browser(BrowserCommand::Open(build_open_command(target, url)));
                    issued.write(crate::CommandIssued {
                        caller,
                        command: cmd.clone(),
                    });
                    writer_params.p0().write(cmd);
                }
                custom_keyboard_restore = true;
            } else if let Some(cmd) = match_command(id) {
                issued.write(crate::CommandIssued {
                    caller,
                    command: cmd.clone(),
                });
                writer_params.p0().write(cmd);
            }
            if let Some(stack_e) = empty_stack {
                abandoned_writer.write(PendingStackAbandoned {
                    stack: stack_e,
                    previous_stack,
                });
                pending_launch.stack = None;
                pending_launch.previous_stack = None;
            }
        }
        CommandBarActionEvent::Space { id } => {
            custom_keyboard_restore = true;
            if !id.is_empty() {
                commands.trigger(BinReceive {
                    webview,
                    payload: SpaceCommandEvent {
                        command: "attach".to_string(),
                        space_id: Some(id.clone()),
                        name: None,
                    },
                });
            }
            if let Some(stack_e) = empty_stack {
                abandoned_writer.write(PendingStackAbandoned {
                    stack: stack_e,
                    previous_stack,
                });
                pending_launch.stack = None;
                pending_launch.previous_stack = None;
            }
        }
        CommandBarActionEvent::SwitchTab { pane, index } => {
            if let Some(stack_e) = empty_stack {
                abandoned_writer.write(PendingStackAbandoned {
                    stack: stack_e,
                    previous_stack,
                });
                pending_launch.stack = None;
                pending_launch.previous_stack = None;
            }
            stack_chosen.write(StackInPaneChosen {
                pane_bits: *pane,
                index: *index,
            });
        }
        CommandBarActionEvent::Dismiss => {
            if let Some(stack_e) = empty_stack {
                abandoned_writer.write(PendingStackAbandoned {
                    stack: stack_e,
                    previous_stack,
                });
                pending_launch.stack = None;
                pending_launch.previous_stack = None;
                custom_keyboard_restore = true;
            }
        }
    }

    if let Ok((modal_e, mut modal_node, mut modal_vis, native_overlay)) = modal_q.single_mut() {
        close_command_bar_surface(&mut modal_node, &mut modal_vis, native_overlay);
        commands
            .entity(modal_e)
            .remove::<KeyboardOwner>()
            .remove::<CommandBarRenderedOpen>()
            .remove::<PendingCommandBarReveal>()
            .remove::<CommandBarRecreating>();
    }
    if !custom_keyboard_restore && let Some(stack) = queries.focused_stack() {
        restore_keyboard.write(RestoreKeyboardToStack { stack });
    }
}

fn deferred_dismiss_modal(
    mut pending_launch: ResMut<PendingLaunch>,
    mut modal_q: Query<
        (
            Entity,
            &mut Node,
            &mut Visibility,
            Has<WebviewNativeOverlay>,
        ),
        With<CommandBar>,
    >,
    mut abandoned: MessageWriter<PendingStackAbandoned>,
    mut commands: Commands,
) {
    if !pending_launch.dismiss_modal {
        return;
    }
    pending_launch.dismiss_modal = false;
    // A deferred dismiss is a dismiss, so it gives back the empty stack the launcher staged the
    // way the explicit one does. Hiding the surface and leaving the stack staged is what let the
    // bar come straight back: the stack is still sitting there waiting to be filled, and the paths
    // that open the launcher onto a pending stack find one and open it again.
    if let Some(stack) = pending_launch.stack.take() {
        let previous_stack = pending_launch.previous_stack.take();
        abandoned.write(PendingStackAbandoned {
            stack,
            previous_stack,
        });
    }
    pending_launch.needs_open = false;
    if let Ok((modal_e, mut modal_node, mut modal_vis, native_overlay)) = modal_q.single_mut()
        && modal_node.display != Display::None
    {
        close_command_bar_surface(&mut modal_node, &mut modal_vis, native_overlay);
        commands
            .entity(modal_e)
            .remove::<KeyboardOwner>()
            .remove::<CommandBarRenderedOpen>()
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
            Option<&CommandBarNativeSize>,
            Has<WebviewWindowed>,
            Has<WebviewNativeOverlay>,
        ),
        With<CommandBar>,
    >,
) {
    for (entity, mut vis, mut pending, rendered, native_size, native_windowed, native_overlay) in
        &mut query
    {
        let rendered_open_id = rendered.map(|rendered| rendered.0);
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
                payload: CommandBarActionEvent::Dismiss,
            });
            continue;
        }
        match next_command_bar_reveal_frames_for_backend(
            native_windowed,
            native_overlay,
            pending.frames,
            pending.open_id,
            rendered_open_id,
            native_size.is_some(),
        ) {
            Some(frames) => pending.frames = frames,
            None => {
                *vis = Visibility::Visible;
                commands.entity(entity).remove::<PendingCommandBarReveal>();
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
        With<CommandBar>,
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
        if !browsers.can_emit_to(&entity) {
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

fn on_path_complete_request(
    trigger: On<BinReceive<PathCompleteRequest>>,
    modal_q: Query<Entity, With<CommandBar>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let query = &trigger.event().payload.query;
    let Ok(modal_e) = modal_q.single() else {
        return;
    };
    if !browsers.can_emit_to(&modal_e) {
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
    use crate::event::CommandBarOpenEvent;
    use crate::event::CommandBarSpace;
    use crate::{CommandPlugin, ReadAppCommands};
    use crate::{command_bar_open_payload, localized_command_name};
    use bevy::ecs::schedule::{NodeId, Schedules, SystemSet};
    use bevy::ecs::system::RunSystemOnce;
    use vmux_core::overlay::OverlayState;

    #[test]
    fn build_payload_includes_commands_and_target() {
        let mut world = World::new();
        let payload = world
            .run_system_once(|contributions: Contributions| {
                build_command_bar_open_payload(
                    OpenId(7),
                    false,
                    String::new(),
                    String::new(),
                    &CommandBarSpacesSnapshot::default(),
                    &contributions,
                    &CommandBarPagesSnapshot::default(),
                    &crate::snapshot::CommandBarWorkSnapshot::default(),
                    &Locale::from("en-US"),
                    0,
                    Vec::new(),
                    Some(OpenTarget::InPlace),
                )
            })
            .expect("payload system runs");
        assert_eq!(payload.open_id, OpenId(7));
        assert_eq!(payload.target, Some(OpenTarget::InPlace));
        assert!(!payload.commands.is_empty());
    }

    #[test]
    fn command_names_localize_every_hierarchy_segment() {
        assert_eq!(
            localized_command_name("ja", "browser_prev_page", "fallback".to_string()),
            "ブラウザ > ナビゲーション > 戻る"
        );
        assert_eq!(
            localized_command_name("ja", "close_pane", "fallback".to_string()),
            "レイアウト > ペイン > ペインを閉じる"
        );
    }

    #[test]
    fn command_bar_open_payload_retries_until_rendered_ack() {
        assert!(should_retry_command_bar_open_payload(
            OpenId(7),
            Some(b"payload"),
            None
        ));
        assert!(should_retry_command_bar_open_payload(
            OpenId(7),
            Some(b"payload"),
            Some(OpenId(6))
        ));
        assert!(!should_retry_command_bar_open_payload(
            OpenId(7),
            Some(b"payload"),
            Some(OpenId(7))
        ));
        assert!(!should_retry_command_bar_open_payload(
            OpenId::NONE,
            Some(b"payload"),
            None
        ));
        assert!(!should_retry_command_bar_open_payload(
            OpenId(7),
            None,
            None
        ));
    }

    #[test]
    fn command_bar_open_retry_uses_binary_host_emit() {
        let source = include_str!("handler.rs");
        let retry_fn = source
            .split("fn retry_pending_command_bar_open")
            .nth(1)
            .and_then(|tail| tail.split("fn reveal_command_bar").next())
            .unwrap_or_default();

        assert!(retry_fn.contains("BinHostEmitEvent::from_bytes"));
        assert!(!retry_fn.contains("HostEmitEvent::new"));
    }

    #[derive(Resource, Default)]
    struct CapturedCommandBarOpen(bool);

    fn capture_command_bar_open(
        modal_q: CommandBarStateQuery,
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
            CommandBar,
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
    fn closed_native_overlay_stays_renderable_without_being_open() {
        let mut node = Node::default();
        let mut visibility = Visibility::Hidden;

        close_command_bar_surface(&mut node, &mut visibility, true);

        assert_eq!(node.display, Display::Flex);
        assert_eq!(visibility, Visibility::Visible);
        assert!(!OverlayState::of(node.display, visibility, false, false).owns_input());
    }

    #[test]
    fn command_bar_modal_prewarms_hidden_and_renderable() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, prewarm_command_bar_modal);
        let modal = app
            .world_mut()
            .spawn((
                CommandBar,
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
        assert_eq!(reveal.open_id, OpenId::NONE);
        assert!(app.world().get::<KeyboardOwner>(modal).is_none());
    }

    #[test]
    fn ready_command_bar_modal_still_prewarms_hidden_and_renderable() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, prewarm_command_bar_modal);
        let modal = app
            .world_mut()
            .spawn((
                CommandBar,
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
        assert_eq!(reveal.open_id, OpenId::NONE);
    }

    #[test]
    fn command_bar_reveal_waits_for_matching_open_id() {
        assert_eq!(next_command_bar_reveal_frames(1, OpenId(7), None), Some(2));
        assert_eq!(
            next_command_bar_reveal_frames(1, OpenId(7), Some(OpenId(6))),
            Some(2)
        );
        assert_eq!(
            next_command_bar_reveal_frames(0, OpenId(7), Some(OpenId(7))),
            Some(1)
        );
        assert_eq!(
            next_command_bar_reveal_frames(2, OpenId(7), Some(OpenId(7))),
            None
        );
    }

    #[test]
    fn command_bar_reveal_falls_back_when_rendered_event_is_missing() {
        assert_eq!(next_command_bar_reveal_frames(0, OpenId(7), None), Some(1));
        assert_eq!(next_command_bar_reveal_frames(10, OpenId(7), None), None);
        assert_eq!(
            next_command_bar_reveal_frames(10, OpenId(7), Some(OpenId(6))),
            None
        );
    }

    #[test]
    fn command_bar_reveal_does_not_require_texture_after_rendered_event() {
        assert_eq!(
            next_command_bar_reveal_frames(2, OpenId(7), Some(OpenId(7))),
            None
        );
        assert_eq!(
            next_command_bar_reveal_frames(2, OpenId(7), Some(OpenId(7))),
            None
        );
    }

    #[test]
    fn native_command_bar_waits_for_size_and_rendered_ack() {
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(true, false, 10, OpenId(7), None, true),
            Some(11)
        );
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(
                true,
                false,
                10,
                OpenId(7),
                Some(OpenId(7)),
                false
            ),
            Some(11)
        );
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(
                true,
                false,
                2,
                OpenId(7),
                Some(OpenId(7)),
                true
            ),
            None
        );
    }

    #[test]
    fn native_command_bar_aborts_stalled_reveal() {
        assert!(!native_command_bar_reveal_timed_out(
            true,
            false,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT - Duration::from_millis(1),
            OpenId(7),
            None,
            false,
        ));
        assert!(native_command_bar_reveal_timed_out(
            true,
            false,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
            OpenId(7),
            None,
            false,
        ));
        assert!(native_command_bar_reveal_timed_out(
            true,
            false,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
            OpenId(7),
            Some(OpenId(7)),
            false,
        ));
        assert!(!native_command_bar_reveal_timed_out(
            true,
            false,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
            OpenId(7),
            Some(OpenId(7)),
            true,
        ));
        assert!(!native_command_bar_reveal_timed_out(
            false,
            false,
            COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
            OpenId(7),
            None,
            false,
        ));
    }

    #[test]
    fn native_overlay_waits_for_rendered_ack() {
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(false, true, 10, OpenId(7), None, false),
            Some(11)
        );
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(
                false,
                true,
                2,
                OpenId(7),
                Some(OpenId(7)),
                false
            ),
            None
        );
    }

    #[test]
    fn native_command_bar_stalled_reveal_stays_hidden() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, reveal_command_bar);
        let modal = app
            .world_mut()
            .spawn((
                CommandBar,
                WebviewWindowed,
                Visibility::Hidden,
                PendingCommandBarReveal {
                    frames: u8::MAX,
                    open_id: OpenId(7),
                    payload: Some(b"payload".to_vec()),
                    started_at: Some(Instant::now() - COMMAND_BAR_NATIVE_REVEAL_TIMEOUT),
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
    fn native_command_bar_does_not_timeout_from_rapid_updates() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, reveal_command_bar);
        let modal = app
            .world_mut()
            .spawn((
                CommandBar,
                WebviewWindowed,
                Visibility::Hidden,
                PendingCommandBarReveal {
                    frames: 0,
                    open_id: OpenId(7),
                    payload: Some(b"payload".to_vec()),
                    started_at: Some(Instant::now()),
                },
            ))
            .id();

        for _ in 0..256 {
            app.update();
        }

        assert!(app.world().get::<PendingCommandBarReveal>(modal).is_some());
        assert_eq!(
            app.world().get::<Visibility>(modal),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn native_command_bar_ignores_hidden_prewarm_size() {
        assert!(!command_bar_size_should_apply(Visibility::Hidden, None));
        assert!(command_bar_size_should_apply(Visibility::Visible, None));
    }

    #[test]
    fn native_command_bar_accepts_hidden_open_size() {
        let pending = PendingCommandBarReveal {
            frames: 0,
            open_id: OpenId(7),
            payload: Some(Vec::new()),
            started_at: Some(Instant::now()),
        };

        assert!(command_bar_size_should_apply(
            Visibility::Hidden,
            Some(&pending)
        ));
        assert_eq!(
            next_command_bar_reveal_frames_for_backend(true, false, 0, OpenId(7), None, true),
            Some(1)
        );
    }

    #[test]
    fn command_bar_payload_includes_space_name() {
        let payload = command_bar_open_payload(
            OpenId(7),
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
            Vec::new(),
        );

        assert_eq!(payload.space_name, "Work");
        assert_eq!(payload.open_id, OpenId(7));
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
            OpenId(8),
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
    fn command_bar_focuses_start_only_for_generic_open() {
        assert!(command_bar_should_focus_start(false, false, true, false));
        assert!(!command_bar_should_focus_start(false, true, true, false));
        assert!(!command_bar_should_focus_start(true, false, true, false));
        assert!(!command_bar_should_focus_start(false, false, false, false));
        assert!(!command_bar_should_focus_start(false, false, true, true));
    }

    #[test]
    fn duplicate_open_is_ignored_while_command_bar_is_visible() {
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

    #[derive(Resource, Default)]
    struct EmittedToPage(Vec<(Entity, String, Vec<u8>)>);

    fn capture_page_emit(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<EmittedToPage>) {
        emitted
            .0
            .push((trigger.webview, trigger.id.clone(), trigger.payload.clone()));
    }

    fn panel_app() -> App {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageOpenRequest>()
            .add_message::<FocusLauncherInput>()
            .add_message::<InlineTransitionRequested>()
            .add_message::<StackInPaneChosen>()
            .add_message::<RestoreKeyboardToStack>()
            .add_message::<PendingStackAbandoned>()
            .init_resource::<CommandBarWorkspaceSnapshot>()
            .init_resource::<CommandBarSpacesSnapshot>()
            .init_resource::<CommandBarPagesSnapshot>()
            .init_resource::<crate::snapshot::CommandBarWorkSnapshot>()
            .init_resource::<PendingLaunch>()
            .init_resource::<EmittedToPage>()
            .add_observer(capture_page_emit)
            .add_systems(Update, handle_open_command_bar);
        app
    }

    fn emitted_to_page(app: &App) -> Vec<(Entity, String)> {
        app.world()
            .resource::<EmittedToPage>()
            .0
            .iter()
            .map(|(webview, id, _)| (*webview, id.clone()))
            .collect()
    }

    fn open_payload(app: &App) -> CommandBarOpenEvent {
        let (_, _, bytes) = app
            .world()
            .resource::<EmittedToPage>()
            .0
            .iter()
            .find(|(_, id, _)| id == LAYOUT_COMMAND_BAR_OPEN_EVENT)
            .expect("no open payload emitted");
        rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(bytes)
            .expect("open payload should round-trip")
    }

    fn send(app: &mut App, command: AppCommand) {
        app.world_mut().write_message(command);
        app.update();
    }

    #[test]
    fn opening_the_command_bar_pushes_the_payload_to_the_layout_page() {
        let mut app = panel_app();
        let layout = app.world_mut().spawn(RendersLauncherPanel).id();

        send(
            &mut app,
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar)),
        );

        assert_eq!(
            emitted_to_page(&app),
            vec![(layout, LAYOUT_COMMAND_BAR_OPEN_EVENT.to_string())]
        );
    }

    #[test]
    fn toggling_an_open_command_bar_asks_the_page_to_close_it() {
        let mut app = panel_app();
        let layout = app
            .world_mut()
            .spawn((RendersLauncherPanel, CommandBarPanelActive))
            .id();

        send(
            &mut app,
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar)),
        );

        assert_eq!(
            emitted_to_page(&app),
            vec![(layout, LAYOUT_COMMAND_BAR_CLOSE_EVENT.to_string())]
        );
    }

    #[test]
    fn toggling_closed_discards_the_stack_a_pending_new_tab_staged() {
        let mut app = panel_app();
        let layout = app
            .world_mut()
            .spawn((RendersLauncherPanel, CommandBarPanelActive))
            .id();
        let pending = app.world_mut().spawn_empty().id();
        app.insert_resource(PendingLaunch {
            stack: Some(pending),
            previous_stack: None,
            needs_open: true,
            dismiss_modal: false,
        });

        send(
            &mut app,
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar)),
        );

        assert_eq!(
            emitted_to_page(&app),
            vec![(layout, LAYOUT_COMMAND_BAR_CLOSE_EVENT.to_string())]
        );
        let abandoned: Vec<Entity> = app
            .world_mut()
            .resource_mut::<Messages<PendingStackAbandoned>>()
            .drain()
            .map(|event| event.stack)
            .collect();
        assert_eq!(abandoned, vec![pending]);
        let ctx = app.world().resource::<PendingLaunch>();
        assert!(ctx.stack.is_none());
        assert!(!ctx.needs_open);
    }

    #[test]
    fn space_switch_reopens_an_already_open_command_bar() {
        let mut app = panel_app();
        let layout = app
            .world_mut()
            .spawn((RendersLauncherPanel, CommandBarPanelActive))
            .id();

        send(
            &mut app,
            AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open)),
        );

        assert_eq!(
            emitted_to_page(&app),
            vec![(layout, LAYOUT_COMMAND_BAR_OPEN_EVENT.to_string())]
        );
        assert!(open_payload(&app).space_switch);
    }

    #[test]
    fn open_page_in_command_bar_marks_payload_as_in_place_target() {
        let mut app = panel_app();
        app.world_mut().spawn(RendersLauncherPanel);

        send(
            &mut app,
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenPageInCommandBar)),
        );

        assert_eq!(open_payload(&app).target, Some(OpenTarget::InPlace));
    }

    #[test]
    fn open_page_in_command_bar_cancels_pending_new_stack_context() {
        let pending_stack = Entity::from_bits(7);
        let previous_stack = Entity::from_bits(6);
        let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Bar(
            BrowserBarCommand::OpenPageInCommandBar,
        ))]);
        let mut ctx = PendingLaunch {
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
        let mut ctx = PendingLaunch {
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
        let mut ctx = PendingLaunch {
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
            .add_plugins(CommandBarInputPlugin)
            .add_message::<TerminalSpawnRequest>()
            .add_message::<FocusLauncherInput>()
            .add_message::<InlineTransitionRequested>()
            .add_message::<StackInPaneChosen>()
            .add_message::<RestoreKeyboardToStack>()
            .add_message::<PendingStackAbandoned>()
            .add_message::<vmux_core::terminal::ProcessesMonitorSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>();

        let modal = app
            .world_mut()
            .spawn((
                CommandBar,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                Visibility::Visible,
                KeyboardOwner,
                CommandBarRenderedOpen(OpenId(1)),
            ))
            .id();

        app.world_mut()
            .trigger(BinReceive::<CommandBarActionEvent> {
                webview: modal,
                payload: CommandBarActionEvent::Dismiss,
            });
        app.world_mut().flush();

        let vis_after_close = *app.world().get::<Visibility>(modal).unwrap();
        let display_after_close = app.world().get::<Node>(modal).unwrap().display;
        let has_kb_after_close = app.world().get::<KeyboardOwner>(modal).is_some();
        let has_rendered_after_close = app.world().get::<CommandBarRenderedOpen>(modal).is_some();
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
            "KeyboardOwner should be removed after dismiss"
        );
        assert!(
            !has_rendered_after_close,
            "CommandBarRenderedOpen should be cleared after dismiss"
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
        let has_kb_after_prewarm = app.world().get::<KeyboardOwner>(modal).is_some();
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
            "KeyboardOwner must not return after prewarm"
        );
        assert!(
            !OverlayState::of(
                display_after_prewarm,
                Visibility::Hidden,
                has_kb_after_prewarm,
                false
            )
            .owns_input(),
            "is_command_bar_open must report false after dismiss + prewarm"
        );
        if let Some(open_id) = pending_open_id_after_prewarm {
            assert_eq!(
                open_id,
                OpenId::NONE,
                "prewarm should re-arm reveal at OpenId::NONE (which never fires until handle_open_command_bar bumps it)"
            );
        }
    }

    #[test]
    fn command_bar_open_runs_after_tab_commands() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_plugins(CommandBarInputPlugin);

        let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
        let mut update = schedules.remove(Update).unwrap();
        update.initialize(app.world_mut()).unwrap();
        let graph = update.graph();
        let tab_command_set = graph
            .system_sets
            .get_key(vmux_core::workspace::StackCommandSet.intern())
            .unwrap();
        let read_command_systems = graph.systems_in_set(ReadAppCommands.intern()).unwrap();
        let tab_command_systems = graph
            .systems_in_set(vmux_core::workspace::StackCommandSet.intern())
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
        use crate::open_target::{PaneDirection, PaneOpenMode, PaneTarget};
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
        assert_eq!(
            normalize_url("google.com", SearchEngine::Google),
            "https://google.com"
        );
    }

    #[test]
    fn normalize_url_preserves_explicit_protocol() {
        assert_eq!(
            normalize_url("http://example.com", SearchEngine::Google),
            "http://example.com"
        );
        assert_eq!(
            normalize_url("https://example.com", SearchEngine::Google),
            "https://example.com"
        );
    }

    #[test]
    fn normalize_url_search_query_becomes_google() {
        assert_eq!(
            normalize_url("hello world", SearchEngine::Google),
            "https://www.google.com/search?q=hello+world"
        );
    }

    #[test]
    fn normalize_url_uses_selected_search_engine() {
        assert_eq!(
            normalize_url("hello world", SearchEngine::DuckDuckGo),
            "https://duckduckgo.com/?q=hello+world"
        );
    }

    #[test]
    fn normalize_url_searches_multiline_text_with_embedded_url() {
        let prompt = "Continue DSK-627\nPR: https://github.com/example/repo/pull/1";
        assert_eq!(
            normalize_url(prompt, SearchEngine::Google),
            "https://www.google.com/search?q=Continue+DSK-627%0APR%3A+https%3A%2F%2Fgithub.com%2Fexample%2Frepo%2Fpull%2F1"
        );
    }

    #[test]
    fn normalize_url_preserves_vmux_protocol() {
        assert_eq!(
            normalize_url("vmux://terminal/123", SearchEngine::Google),
            "vmux://terminal/123"
        );
    }

    #[test]
    fn normalize_url_preserves_data_scheme() {
        let data = "data:text/html,<style>body{background:white}</style><h1>x</h1>";
        assert_eq!(normalize_url(data, SearchEngine::Google), data);
    }

    #[test]
    fn pending_reveal_is_active_only_with_real_open_id() {
        assert!(
            !PendingCommandBarReveal {
                frames: 0,
                open_id: OpenId::NONE,
                payload: None,
                started_at: None,
            }
            .is_active()
        );
        assert!(
            PendingCommandBarReveal {
                frames: 0,
                open_id: OpenId(7),
                payload: None,
                started_at: Some(Instant::now()),
            }
            .is_active()
        );
    }
}
