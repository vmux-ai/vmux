pub(crate) use crate::NewStackContext;
use crate::cef::Browser;
use crate::{
    Header,
    pane::{Pane, PaneSplit},
    side_sheet::SideSheet,
    stack::{Stack, active_among, collect_leaf_panes, focused_stack},
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
    CommandBarReadyEvent, CommandBarRenderedEvent, CommandBarSpace, CommandBarTab,
    PATH_COMPLETE_RESPONSE, PathCompleteRequest, PathCompleteResponse, PathEntry,
};
use vmux_command::open::OpenCommand;
use vmux_command::open_target::OpenTarget;
use vmux_command::snapshot::{
    AgentProviderSummary, CommandBarAgentsSnapshot, CommandBarSettingsSnapshot,
    CommandBarSpacesSnapshot, CommandBarTerminalsSnapshot,
};
use vmux_command::{
    AppCommand, BrowserBarCommand, BrowserCommand, LayoutCommand, PaneCommand, ReadAppCommands,
    SpaceCommand, StackCommand,
};
use vmux_core::agent::{PageAgentAttachRequest, PageAgentSpawnStackRequest};
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

#[derive(Component)]
pub struct PendingCommandBarReveal {
    frames: u8,
    open_id: u64,
    payload: Option<String>,
}

const COMMAND_BAR_REVEAL_FRAMES: u8 = 2;
const COMMAND_BAR_REVEAL_FALLBACK_FRAMES: u8 = 10;

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
            )>::default())
            .add_observer(on_command_bar_action)
            .add_observer(on_path_complete_request)
            .add_observer(on_command_bar_ready)
            .add_observer(on_command_bar_rendered)
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

pub fn command_list(
    cli_agent_entries: Vec<AgentProviderSummary>,
    app_agent_entries: Vec<AppAgentEntry>,
) -> Vec<CommandBarEntry> {
    let mut entries: Vec<CommandBarEntry> = AppCommand::command_bar_entries()
        .into_iter()
        .map(|(id, name, shortcut)| CommandBarEntry {
            id: id.to_string(),
            name: name.to_string(),
            shortcut: shortcut.to_string(),
        })
        .collect();
    entries.extend(cli_agent_entries.into_iter().map(|entry| CommandBarEntry {
        id: entry.id,
        name: entry.name,
        shortcut: String::new(),
    }));
    entries.extend(app_agent_entries.into_iter().map(|entry| CommandBarEntry {
        id: entry.id,
        name: entry.name,
        shortcut: String::new(),
    }));
    entries
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
    payload: Option<&str>,
    rendered_open_id: Option<u64>,
) -> bool {
    open_id != 0 && payload.is_some() && rendered_open_id != Some(open_id)
}

fn should_requeue_command_bar_open_after_emit(_command_bar_ready: bool) -> bool {
    false
}

fn on_command_bar_ready(trigger: On<BinReceive<CommandBarReadyEvent>>, mut commands: Commands) {
    commands
        .entity(trigger.event().webview)
        .insert(CommandBarReady);
}

fn on_command_bar_rendered(
    trigger: On<BinReceive<CommandBarRenderedEvent>>,
    mut commands: Commands,
) {
    commands
        .entity(trigger.event().webview)
        .insert(CommandBarRenderedOpen(trigger.event().payload.open_id));
}

#[derive(Default)]
struct CommandBarOpenRequest {
    should_toggle: bool,
    should_dismiss: bool,
    should_dismiss_nav: bool,
    url_override: Option<String>,
}

fn command_bar_open_request(
    commands: impl IntoIterator<Item = AppCommand>,
    spaces_page_url: &str,
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
                request.url_override = Some(spaces_page_url.to_string());
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
        ),
        With<Modal>,
    >,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
    browsers: NonSend<Browsers>,
    tab_q: Query<(Entity, &LastActivatedAt), With<Tab>>,
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
    )>,
    mut commands: Commands,
) {
    let active_stack_count = stack_q.iter().count();
    let spaces_snapshot = snapshot_params.p1().clone();
    let space_name = spaces_snapshot.active_space_name.clone();
    let agents_snap = snapshot_params.p0().clone();
    let agent_entries: Vec<AgentProviderSummary> = agents_snap.providers;
    let app_agent_entries: Vec<AppAgentEntry> = agents_snap
        .strategies
        .iter()
        .map(|s| AppAgentEntry {
            id: app_agent_id(&s.provider, &s.model),
            name: format!("New {}/{} chat (App)", s.provider, s.model),
        })
        .collect();
    let mut new_stack_ctx = snapshot_params.p2();

    let request =
        command_bar_open_request(reader.read().cloned(), &spaces_snapshot.spaces_page_url);
    let mut should_open = false;
    let should_toggle = request.should_toggle;
    let should_dismiss = request.should_dismiss;
    let should_dismiss_nav = request.should_dismiss_nav;
    let url_override = request.url_override;

    if should_dismiss {
        let is_open = modal_q
            .single()
            .map(|(_, n, _, has_keyboard_target, _, _, _)| {
                command_bar_modal_is_open(n.display, has_keyboard_target)
            })
            .unwrap_or(false);
        if is_open {
            let Ok((modal_e, mut modal_node, mut modal_vis, _, _, _, _)) = modal_q.single_mut()
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
                    &tab_q,
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
            .map(|(_, n, _, has_keyboard_target, _, _, _)| {
                command_bar_modal_is_open(n.display, has_keyboard_target)
            })
            .unwrap_or(false);
        if is_open {
            let Ok((modal_e, mut modal_node, mut modal_vis, _, _, _, _)) = modal_q.single_mut()
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
            new_stack_ctx.needs_open = false;
            return;
        }
    }

    if new_stack_ctx.needs_open {
        should_open = true;
        new_stack_ctx.needs_open = false;
    }

    if should_toggle {
        let is_open = modal_q
            .single()
            .map(|(_, n, visibility, has_keyboard_target, _, _, _)| {
                command_bar_modal_is_visible(n.display, *visibility, has_keyboard_target)
            })
            .unwrap_or(false);
        if !is_open {
            should_open = true;
        }
        // If already open, do nothing — the shortcut should not close the bar.
        // Users can dismiss with Escape or click-outside.
    }

    if !should_open {
        return;
    }

    let Ok((
        modal_e,
        mut modal_node,
        mut modal_vis,
        has_keyboard_target,
        command_bar_ready,
        rendered_open,
        modal_pending_reveal,
    )) = modal_q.single_mut()
    else {
        return;
    };

    let is_new_stack = new_stack_ctx.stack.is_some();
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
        .insert(CefPointerTarget);

    // Command bar is a CEF webview — allow keyboard forwarding
    suppress.0 = false;

    // Gather current URL (empty for new tab mode)
    let current_url = if let Some(override_url) = url_override {
        override_url
    } else if is_new_stack {
        String::new()
    } else {
        let (_, _, active_stack) = focused_stack(
            &tab_q,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );
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

    // Gather all tabs
    let active_tab = active_among(tab_q.iter());
    let mut bar_tabs = Vec::new();
    if let Some(active_tab_e) = active_tab {
        let (_, _, active_stack) = focused_stack(
            &tab_q,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );
        let active_pane = active_stack.and_then(|t| child_of_q.get(t).ok().map(|co| co.get()));
        let mut tab_panes = Vec::new();
        collect_leaf_panes(active_tab_e, &all_children, &leaf_panes, &mut tab_panes);
        for &pane_e in &tab_panes {
            let is_active_pane = active_pane == Some(pane_e);
            if let Ok(children) = pane_children.get(pane_e) {
                let mut tab_index = 0usize;
                for child in children.iter() {
                    if !stack_q.contains(child) {
                        continue;
                    }
                    let stack_is_active = active_stack == Some(child) && is_active_pane;
                    if let Ok(tab_kids) = all_children.get(child) {
                        for browser_e in tab_kids.iter() {
                            if let Ok(meta) = browser_meta.get(browser_e) {
                                bar_tabs.push(CommandBarTab {
                                    title: meta.title.clone(),
                                    url: meta.url.clone(),
                                    pane_id: pane_e.to_bits(),
                                    tab_index: tab_index as u32,
                                    is_active: stack_is_active,
                                });
                            }
                        }
                    }
                    tab_index += 1;
                }
            }
        }
    }

    // Build command list
    let bar_commands: Vec<CommandBarCommandEntry> = command_list(agent_entries, app_agent_entries)
        .into_iter()
        .map(|e| CommandBarCommandEntry {
            id: e.id,
            name: e.name,
            shortcut: e.shortcut,
        })
        .collect();

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
        new_stack_ctx.needs_open = true;
        return;
    }

    let bar_spaces = spaces_snapshot
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

    let open_id = now_millis() as u64;
    let reveal_start_frames = command_bar_reveal_start_frames(
        modal_pending_reveal.is_some_and(|pending| pending.open_id == 0),
    );
    let target = if is_new_stack {
        Some(vmux_command::open_target::OpenTarget::InNewStack)
    } else {
        None
    };
    let payload = command_bar_open_payload(
        open_id,
        space_name,
        current_url,
        bar_spaces,
        bar_tabs,
        bar_commands,
        target,
    );
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    let ron_body_len = ron_body.len();
    commands.trigger(BinHostEmitEvent::from_rkyv(
        modal_e,
        COMMAND_BAR_OPEN_EVENT,
        &payload,
    ));
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
                payload: Some(ron_body.clone()),
            });
    } else {
        commands
            .entity(modal_e)
            .remove::<CommandBarRenderedOpen>()
            .remove::<CommandBarPaintedOpen>()
            .insert(PendingCommandBarReveal {
                frames: reveal_start_frames,
                open_id,
                payload: Some(ron_body),
            });
    }
    webview_debug_log(format!(
        "command_bar emit open entity={modal_e:?} payload_len={} tabs={} commands={}",
        ron_body_len,
        payload.tabs.len(),
        payload.commands.len()
    ));
    if should_requeue_command_bar_open_after_emit(command_bar_ready) {
        new_stack_ctx.needs_open = true;
    }
}

fn command_bar_open_payload(
    open_id: u64,
    space_name: String,
    url: String,
    spaces: Vec<CommandBarSpace>,
    tabs: Vec<CommandBarTab>,
    commands: Vec<CommandBarCommandEntry>,
    target: Option<vmux_command::open_target::OpenTarget>,
) -> CommandBarOpenEvent {
    CommandBarOpenEvent {
        open_id,
        url,
        space_name,
        spaces,
        tabs,
        commands,
        target,
    }
}

#[derive(SystemParam)]
struct CommandBarActionQueries<'w, 's> {
    tab_q: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Tab>>,
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
    if value.contains("://") {
        value.to_string()
    } else if value.contains('.') && !value.contains(' ') {
        format!("https://{}", value)
    } else {
        format!("https://www.google.com/search?q={}", value)
    }
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
        Res<CommandBarSettingsSnapshot>,
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
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let evt = &trigger.event().payload;
    let terminals_snapshot = resource_params.p2().clone();
    let terminal_page_url = terminals_snapshot.terminal_page_url.clone();
    let pid_to_entity = terminals_snapshot.pid_to_entity.clone();
    let mut empty_stack = new_stack_ctx.stack;
    let previous_stack = new_stack_ctx.previous_stack;
    let mut custom_keyboard_restore = false;

    match evt.action.as_str() {
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
                            &queries.tab_q,
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
                    let open_cmd = build_open_command(target, url);
                    writer_params
                        .p0()
                        .write(AppCommand::Browser(BrowserCommand::Open(open_cmd)));
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
                        &queries.tab_q,
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
                        writer_params
                            .p0()
                            .write(AppCommand::Browser(BrowserCommand::Open(
                                OpenCommand::InNewStack {
                                    url: Some("vmux://terminal/".into()),
                                },
                            )));
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
                        &queries.tab_q,
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
                .p3()
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
                    writer_params
                        .p0()
                        .write(AppCommand::Browser(BrowserCommand::Open(
                            build_open_command(target, url),
                        )));
                }
                custom_keyboard_restore = true;
            } else if let Some(cmd) = match_command(&evt.value) {
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
                commands.entity(target_pane).insert(LastActivatedAt::now());
                if let Ok(children) = queries.pane_children.get(target_pane) {
                    let stack_q = stack_params.p0();
                    let stacks: Vec<Entity> =
                        children.iter().filter(|&e| stack_q.contains(e)).collect();
                    if let Some(&target_stack) = stacks.get(tab_index) {
                        commands.entity(target_stack).insert(LastActivatedAt::now());
                    }
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
            &queries.tab_q,
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
    if let Some(next) = pick_tab_after_close(tab, &siblings) {
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

fn pick_tab_after_close(active: Entity, siblings: &[Entity]) -> Option<Entity> {
    if siblings.len() <= 1 {
        return None;
    }
    let idx = siblings.iter().position(|e| *e == active)?;
    let next_idx = if idx + 1 < siblings.len() { idx + 1 } else { 0 };
    let target = siblings[next_idx];
    if target == active { None } else { Some(target) }
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
        ),
        With<Modal>,
    >,
) {
    for (entity, mut vis, mut pending, rendered, painted) in &mut query {
        let rendered_open_id = rendered.map(|rendered| rendered.0);
        let painted_open_id = painted.map(|painted| painted.0);
        match next_command_bar_reveal_frames(
            pending.frames,
            pending.open_id,
            rendered_open_id,
            painted_open_id,
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
        commands.trigger(HostEmitEvent::new(entity, COMMAND_BAR_OPEN_EVENT, &payload));
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
        std::path::PathBuf::from(&home)
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

        let full_path = if parent_str.is_empty() {
            display_name.clone()
        } else {
            format!("{}{}", parent_str, display_name)
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

    #[test]
    fn command_bar_open_does_not_block_on_command_bar_listener() {
        assert!(command_bar_open_delivery_ready(true, true, false));
        assert!(command_bar_open_delivery_ready(true, true, true));
    }

    #[test]
    fn command_bar_open_payload_retries_until_rendered_ack() {
        assert!(should_retry_command_bar_open_payload(
            7,
            Some("payload"),
            None
        ));
        assert!(should_retry_command_bar_open_payload(
            7,
            Some("payload"),
            Some(6)
        ));
        assert!(!should_retry_command_bar_open_payload(
            7,
            Some("payload"),
            Some(7)
        ));
        assert!(!should_retry_command_bar_open_payload(
            0,
            Some("payload"),
            None
        ));
        assert!(!should_retry_command_bar_open_payload(7, None, None));
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
                    payload: Some("payload".to_string()),
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
                buffer: std::sync::Arc::new(vec![0, 0, 0, 255]),
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
            "Work".to_string(),
            "https://example.com".to_string(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
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
            "Work".to_string(),
            "vmux://spaces/".to_string(),
            spaces.clone(),
            Vec::new(),
            Vec::new(),
            None,
        );

        assert_eq!(payload.spaces, spaces);
    }

    #[test]
    fn space_open_command_prefills_spaces_url() {
        let spaces_url = "vmux://spaces/";
        let request = command_bar_open_request(
            [AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open))],
            spaces_url,
        );

        assert!(request.should_toggle);
        assert_eq!(request.url_override, Some(spaces_url.to_string()));
    }

    #[test]
    fn open_in_new_stack_does_not_dismiss_command_bar() {
        let request = command_bar_open_request(
            [AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewStack { url: None },
            ))],
            "vmux://spaces/",
        );

        assert!(!request.should_dismiss);
    }

    #[test]
    fn open_command_bar_forces_empty_url_override() {
        let request = command_bar_open_request(
            [AppCommand::Browser(BrowserCommand::Bar(
                BrowserBarCommand::OpenCommandBar,
            ))],
            "vmux://spaces/",
        );

        assert!(request.should_toggle);
        assert_eq!(request.url_override, Some(String::new()));
    }

    #[test]
    fn open_page_in_command_bar_leaves_url_override_unset_so_current_url_is_prefilled() {
        let request = command_bar_open_request(
            [AppCommand::Browser(BrowserCommand::Bar(
                BrowserBarCommand::OpenPageInCommandBar,
            ))],
            "vmux://spaces/",
        );

        assert!(request.should_toggle);
        assert_eq!(request.url_override, None);
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
}
