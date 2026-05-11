use crate::{
    agent::{AgentCommandEntry, AgentLaunchRequested, AgentProviders},
    browser::Browser,
    command::{
        AppCommand, BrowserCommand, LayoutCommand, PaneCommand, ReadAppCommands, SpaceCommand,
        StackCommand, TerminalCommand,
    },
    layout::{
        pane::{Pane, PaneSplit},
        side_sheet::SideSheet,
        stack::{Stack, active_among, collect_leaf_panes, focused_stack},
        tab::Tab,
        window::{Main, Modal},
    },
    processes_monitor::ProcessesMonitor,
    settings::AppSettings,
    spaces::{ActiveSpace, SpacesView},
    terminal::Terminal,
};
use bevy::{
    ecs::message::MessageReader, ecs::relationship::Relationship, picking::Pickable, prelude::*,
    ui::UiSystems, window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::{RenderTextureMessage, webview_debug_log};
use vmux_command::event::{
    COMMAND_BAR_OPEN_EVENT, CommandBarActionEvent, CommandBarCommandEntry, CommandBarOpenEvent,
    CommandBarReadyEvent, CommandBarRenderedEvent, CommandBarSpace, CommandBarTab,
    PATH_COMPLETE_RESPONSE, PathCompleteRequest, PathCompleteResponse, PathEntry,
};
use vmux_core::PageMetadata;
use vmux_history::{LastActivatedAt, now_millis};
pub(crate) use vmux_layout::NewStackContext;
use vmux_layout::{
    Header,
    event::{PROCESSES_WEBVIEW_URL, TERMINAL_WEBVIEW_URL},
};
use vmux_space::event::{SPACES_WEBVIEW_URL, SpaceCommandEvent};

pub(crate) use crate::terminal::pid::focus_pane_entity;

pub(crate) fn parse_pid_from_url(url: &str) -> Option<u32> {
    let suffix = url.strip_prefix(TERMINAL_WEBVIEW_URL)?;
    if suffix.is_empty() {
        return None;
    }
    suffix.parse::<u32>().ok()
}

/// Deferred visibility for the command bar modal. Counts frames after Display::Flex
/// so CEF can resize the webview before the modal becomes visible.
#[derive(Component)]
struct CommandBarReady;

#[derive(Component)]
struct CommandBarRenderedOpen(u64);

#[derive(Component)]
struct CommandBarPaintedOpen(u64);

#[derive(Component)]
pub(crate) struct PendingCommandBarReveal {
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
            .init_resource::<AgentProviders>()
            .init_resource::<Messages<AgentLaunchRequested>>()
            .add_plugins(BinJsEmitEventPlugin::<CommandBarActionEvent>::default())
            .add_plugins(BinJsEmitEventPlugin::<PathCompleteRequest>::default())
            .add_plugins(BinJsEmitEventPlugin::<CommandBarReadyEvent>::default())
            .add_plugins(BinJsEmitEventPlugin::<CommandBarRenderedEvent>::default())
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
                    .after(crate::layout::tab::TabCommandSet)
                    .after(crate::layout::stack::StackCommandSet),
            )
            .add_systems(
                Update,
                retry_pending_command_bar_open.after(handle_open_command_bar),
            )
            .add_systems(
                Update,
                deferred_dismiss_modal
                    .after(ReadAppCommands)
                    .before(crate::layout::stack::ComputeFocusSet),
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
    pub id: &'static str,
    pub name: &'static str,
    pub shortcut: &'static str,
}

pub fn command_list(agent_entries: Vec<AgentCommandEntry>) -> Vec<CommandBarEntry> {
    let mut entries: Vec<CommandBarEntry> = AppCommand::command_bar_entries()
        .into_iter()
        .map(|(id, name, shortcut)| CommandBarEntry { id, name, shortcut })
        .collect();
    entries.extend(agent_entries.into_iter().map(|entry| CommandBarEntry {
        id: entry.id,
        name: entry.name,
        shortcut: entry.shortcut,
    }));
    entries
}

pub fn match_command(id: &str) -> Option<AppCommand> {
    AppCommand::from_menu_id(id)
}

/// Returns true when the command bar modal is currently visible.
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
) -> CommandBarOpenRequest {
    let mut request = CommandBarOpenRequest::default();
    for cmd in commands {
        match cmd {
            AppCommand::Browser(BrowserCommand::FocusAddressBar) => {
                request.should_toggle = true;
            }
            AppCommand::Browser(BrowserCommand::OpenCommandBar) => {
                request.should_toggle = true;
                request.url_override = Some(String::new());
            }
            AppCommand::Browser(BrowserCommand::OpenPathBar) => {
                request.should_toggle = true;
                request.url_override = Some("/".to_string());
            }
            AppCommand::Browser(BrowserCommand::OpenCommands) => {
                request.should_toggle = true;
                request.url_override = Some(">".to_string());
            }
            AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open)) => {
                request.should_toggle = true;
                request.url_override = Some(SPACES_WEBVIEW_URL.to_string());
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
    mut space_params: ParamSet<(
        Res<ActiveSpace>,
        Option<Res<AgentProviders>>,
        ResMut<NewStackContext>,
    )>,
    mut commands: Commands,
) {
    let active_stack_count = stack_q.iter().count();
    let active_space = space_params.p0().clone();
    let space_name = active_space.record.name.clone();
    let agent_entries = space_params
        .p1()
        .map(|providers| providers.command_entries())
        .unwrap_or_default();
    let mut new_stack_ctx = space_params.p2();

    let request = command_bar_open_request(reader.read().copied());
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
    let bar_commands: Vec<CommandBarCommandEntry> = command_list(agent_entries)
        .into_iter()
        .map(|e| CommandBarCommandEntry {
            id: e.id.into(),
            name: e.name.into(),
            shortcut: e.shortcut.into(),
        })
        .collect();

    let has_browser = browsers.has_browser(modal_e);
    let host_emit_ready = browsers.host_emit_ready(&modal_e);
    let rendered_matches = rendered_open.is_some_and(|rendered| rendered.0 != 0);
    webview_debug_log(format!(
        "command_bar open entity={modal_e:?} was_open={was_open} has_browser={has_browser} host_emit_ready={host_emit_ready} command_bar_ready={command_bar_ready} rendered={rendered_matches} pending_reveal={} visibility={:?} new_tab={is_new_stack}",
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

    let bar_spaces = crate::spaces::active_space_rows(&active_space, active_stack_count)
        .into_iter()
        .map(|space| CommandBarSpace {
            id: space.id,
            name: space.name,
            profile: space.profile,
            is_active: space.is_active,
            tab_count: space.tab_count,
        })
        .collect();

    let open_id = now_millis() as u64;
    let reveal_start_frames = command_bar_reveal_start_frames(
        modal_pending_reveal.is_some_and(|pending| pending.open_id == 0),
    );
    let payload = command_bar_open_payload(
        open_id,
        space_name,
        current_url,
        bar_spaces,
        bar_tabs,
        bar_commands,
        is_new_stack,
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
    new_tab: bool,
) -> CommandBarOpenEvent {
    CommandBarOpenEvent {
        open_id,
        url,
        space_name,
        spaces,
        tabs,
        commands,
        new_tab,
    }
}

fn attach_spaces_page_to_tab(
    tab: Entity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    commands.entity(tab).insert(PageMetadata {
        url: SPACES_WEBVIEW_URL.to_string(),
        title: "Spaces".to_string(),
        ..default()
    });
    commands.spawn((SpacesView::new(meshes, webview_mt), ChildOf(tab)));
}

#[allow(clippy::too_many_arguments)]
fn spawn_spaces_page_layout_from_command_bar(
    main: Option<Entity>,
    primary_window: Option<Entity>,
    settings: &AppSettings,
    new_stack_ctx: &mut NewStackContext,
    focus: Option<&mut crate::layout::stack::FocusedStack>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> bool {
    let Some(main) = main else {
        return false;
    };
    let Some(primary_window) = primary_window else {
        return false;
    };
    let spawned = crate::layout::window::spawn_default_space_layout(
        main,
        primary_window,
        &settings.layout,
        new_stack_ctx,
        commands,
    );
    if let Some(focus) = focus {
        focus.tab = Some(spawned.tab);
        focus.pane = Some(spawned.pane);
        focus.stack = Some(spawned.stack);
    }
    let Some(tab) = new_stack_ctx.stack.take() else {
        return false;
    };
    new_stack_ctx.previous_stack = None;
    new_stack_ctx.needs_open = false;
    new_stack_ctx.dismiss_modal = false;
    attach_spaces_page_to_tab(tab, commands, meshes, webview_mt);
    true
}

fn on_command_bar_action(
    trigger: On<BinReceive<CommandBarActionEvent>>,
    mut modal_q: Query<(Entity, &mut Node, &mut Visibility), With<Modal>>,
    tab_q: Query<(Entity, &LastActivatedAt), With<Tab>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut stack_params: ParamSet<(
        Query<Entity, With<Stack>>,
        Query<Entity, With<Main>>,
        Query<Entity, With<PrimaryWindow>>,
        Option<ResMut<crate::layout::stack::FocusedStack>>,
    )>,
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
    mut resource_params: ParamSet<(
        Res<AppSettings>,
        Option<Res<AgentProviders>>,
        Option<Res<crate::terminal::pid::PidToEntity>>,
    )>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut writer_params: ParamSet<(
        MessageWriter<AppCommand>,
        Option<MessageWriter<AgentLaunchRequested>>,
    )>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let webview = trigger.event().webview;
    let evt = &trigger.event().payload;
    let settings = resource_params.p0().clone();
    let pid_to_entity = resource_params
        .p2()
        .as_deref()
        .map(|map| map.0.clone())
        .unwrap_or_default();
    let empty_stack = new_stack_ctx.stack;
    let previous_stack = new_stack_ctx.previous_stack;
    // Track whether we handle keyboard restore ourselves
    let mut custom_keyboard_restore = false;

    match evt.action.as_str() {
        "navigate" => {
            // Detect filesystem paths — open terminal with cwd instead of browser
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
                        url: TERMINAL_WEBVIEW_URL.to_string(),
                        title: format!("Terminal ({})", dir.display()),
                        ..default()
                    });
                    let term_e = commands
                        .spawn((
                            Terminal::new_with_cwd(
                                &mut meshes,
                                &mut webview_mt,
                                &settings,
                                Some(dir),
                            ),
                            ChildOf(stack_e),
                        ))
                        .id();
                    commands.entity(term_e).insert(CefKeyboardTarget);
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                    custom_keyboard_restore = true;
                }
            } else {
                let url = if evt.value.contains("://") {
                    evt.value.clone()
                } else if evt.value.contains('.') && !evt.value.contains(' ') {
                    format!("https://{}", evt.value)
                } else {
                    format!("https://www.google.com/search?q={}", evt.value)
                };

                if let Some(stack_e) = empty_stack {
                    // New tab mode: attach content to the empty tab
                    if url.starts_with("vmux://terminal") {
                        let known =
                            parse_pid_from_url(&url).and_then(|p| pid_to_entity.get(&p).copied());
                        if let Some(entity) = known {
                            focus_pane_entity(entity, &mut commands, &child_of_q);
                        } else {
                            if let Some(pid) = parse_pid_from_url(&url) {
                                bevy::log::warn!("no terminal pane for pid {pid}; spawning new");
                            }
                            commands.entity(stack_e).insert(PageMetadata {
                                url: TERMINAL_WEBVIEW_URL.to_string(),
                                title: "Terminal".to_string(),
                                ..default()
                            });
                            let term_e = commands
                                .spawn((
                                    Terminal::new(&mut meshes, &mut webview_mt, &settings),
                                    ChildOf(stack_e),
                                ))
                                .id();
                            commands.entity(term_e).insert(CefKeyboardTarget);
                        }
                    } else if url.starts_with(PROCESSES_WEBVIEW_URL.trim_end_matches('/')) {
                        commands.entity(stack_e).insert(PageMetadata {
                            url: PROCESSES_WEBVIEW_URL.to_string(),
                            title: "Background Services".to_string(),
                            ..default()
                        });
                        commands.spawn((
                            ProcessesMonitor::new(&mut meshes, &mut webview_mt),
                            ChildOf(stack_e),
                        ));
                    } else if url.starts_with(SPACES_WEBVIEW_URL.trim_end_matches('/')) {
                        attach_spaces_page_to_tab(
                            stack_e,
                            &mut commands,
                            &mut meshes,
                            &mut webview_mt,
                        );
                    } else {
                        let browser_e = commands
                            .spawn((
                                Browser::new(&mut meshes, &mut webview_mt, &url),
                                ChildOf(stack_e),
                            ))
                            .id();
                        commands.entity(browser_e).insert(CefKeyboardTarget);
                    }
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                    custom_keyboard_restore = true;
                } else {
                    // Normal mode: navigate or spawn terminal in current tab
                    let known_terminal =
                        parse_pid_from_url(&url).and_then(|p| pid_to_entity.get(&p).copied());
                    if let Some(entity) = known_terminal {
                        focus_pane_entity(entity, &mut commands, &child_of_q);
                    } else if url.starts_with("vmux://terminal") {
                        if let Some(pid) = parse_pid_from_url(&url) {
                            bevy::log::warn!("no terminal pane for pid {pid}; spawning new");
                        }
                        writer_params
                            .p0()
                            .write(AppCommand::Terminal(TerminalCommand::New));
                    } else if url.starts_with(PROCESSES_WEBVIEW_URL.trim_end_matches('/')) {
                        use crate::command::ServiceCommand;
                        writer_params
                            .p0()
                            .write(AppCommand::Service(ServiceCommand::Open));
                    } else if url.starts_with(SPACES_WEBVIEW_URL.trim_end_matches('/')) {
                        let (_, active_pane_opt, _) = focused_stack(
                            &tab_q,
                            &all_children,
                            &leaf_panes,
                            &pane_ts,
                            &pane_children,
                            &stack_ts,
                        );
                        if let Some(pane_e) = active_pane_opt {
                            let stack_e = commands
                                .spawn((
                                    crate::layout::stack::stack_bundle(),
                                    LastActivatedAt::now(),
                                    ChildOf(pane_e),
                                ))
                                .id();
                            attach_spaces_page_to_tab(
                                stack_e,
                                &mut commands,
                                &mut meshes,
                                &mut webview_mt,
                            );
                            custom_keyboard_restore = true;
                        } else {
                            let main = stack_params.p1().single().ok();
                            let primary_window = stack_params.p2().single().ok();
                            let mut focus = stack_params.p3();
                            if spawn_spaces_page_layout_from_command_bar(
                                main,
                                primary_window,
                                &settings,
                                &mut new_stack_ctx,
                                focus.as_deref_mut(),
                                &mut commands,
                                &mut meshes,
                                &mut webview_mt,
                            ) {
                                custom_keyboard_restore = true;
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
                                    commands.entity(browser_e).insert(WebviewSource::new(&url));
                                }
                            }
                        }
                    }
                }
            }
        }
        "terminal" => {
            let known_terminal =
                parse_pid_from_url(&evt.value).and_then(|p| pid_to_entity.get(&p).copied());
            if let Some(entity) = known_terminal {
                focus_pane_entity(entity, &mut commands, &child_of_q);
                new_stack_ctx.stack = None;
                new_stack_ctx.previous_stack = None;
                custom_keyboard_restore = true;
            } else {
                if let Some(pid) = parse_pid_from_url(&evt.value) {
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
                        url: TERMINAL_WEBVIEW_URL.to_string(),
                        title: "Terminal".to_string(),
                        ..default()
                    });
                    let term_e = commands
                        .spawn((
                            Terminal::new_with_cwd(
                                &mut meshes,
                                &mut webview_mt,
                                &settings,
                                cwd.as_deref(),
                            ),
                            ChildOf(stack_e),
                        ))
                        .id();
                    commands.entity(term_e).insert(CefKeyboardTarget);
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                    custom_keyboard_restore = true;
                } else {
                    let (_, active_pane_opt, _) = focused_stack(
                        &tab_q,
                        &all_children,
                        &leaf_panes,
                        &pane_ts,
                        &pane_children,
                        &stack_ts,
                    );
                    if let Some(pane_e) = active_pane_opt {
                        let stack_e = commands
                            .spawn((
                                crate::layout::stack::stack_bundle(),
                                LastActivatedAt::now(),
                                ChildOf(pane_e),
                            ))
                            .id();
                        commands.entity(stack_e).insert(PageMetadata {
                            url: TERMINAL_WEBVIEW_URL.to_string(),
                            title: "Terminal".to_string(),
                            ..default()
                        });
                        let term_e = commands
                            .spawn((
                                Terminal::new_with_cwd(
                                    &mut meshes,
                                    &mut webview_mt,
                                    &settings,
                                    cwd.as_deref(),
                                ),
                                ChildOf(stack_e),
                            ))
                            .id();
                        commands.entity(term_e).insert(CefKeyboardTarget);
                    } else {
                        writer_params
                            .p0()
                            .write(AppCommand::Terminal(TerminalCommand::New));
                    }
                }
            } // end reattach else
        }
        "command" => {
            if resource_params
                .p1()
                .is_some_and(|providers| providers.contains(&evt.value))
            {
                let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
                if let Some(mut agent_launches) = writer_params.p1() {
                    agent_launches.write(AgentLaunchRequested {
                        provider_id: evt.value.clone(),
                        cwd,
                    });
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
                && let Some(target_pane) = leaf_panes.iter().find(|e| e.to_bits() == pane_id)
            {
                commands.entity(target_pane).insert(LastActivatedAt::now());
                if let Ok(children) = pane_children.get(target_pane) {
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
                    &tab_q,
                    &child_of_q,
                    &all_children,
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
                        && let Ok(children) = all_children.get(prev)
                    {
                        for child in children.iter() {
                            if content_browsers.contains(child) {
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

/// Closes the command bar modal when `NewStackContext::dismiss_modal` is set.
/// Runs after `ReadAppCommands` so that `handle_tab_commands` can validate
/// the target index before requesting a dismiss.
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
    use crate::{
        command::CommandPlugin,
        settings::{
            AppSettings, BrowserSettings, FocusRingSettings, LayoutSettings, PaneSettings,
            ShortcutSettings, SideSheetSettings, WindowSettings,
        },
    };
    use bevy::{
        ecs::schedule::{NodeId, Schedules, SystemSet},
        window::PrimaryWindow,
    };

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings {
                    gap: 0.0,
                    radius: 0.0,
                },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            startup_url: None,
        }
    }

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
        app.add_plugins(MinimalPlugins);
        app.init_resource::<CapturedCommandBarOpen>();
        app.add_systems(Update, capture_command_bar_open);
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
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, prewarm_command_bar_modal);
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
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, prewarm_command_bar_modal);
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
        app.add_plugins(MinimalPlugins);
        app.add_message::<RenderTextureMessage>();
        app.add_systems(
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
            false,
        );

        assert_eq!(payload.space_name, "Work");
        assert_eq!(payload.open_id, 7);
    }

    #[test]
    fn command_bar_payload_includes_spaces() {
        let spaces = vec![CommandBarSpace {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: "default".to_string(),
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
            false,
        );

        assert_eq!(payload.spaces, spaces);
    }

    #[test]
    fn space_open_command_prefills_spaces_url() {
        let request = command_bar_open_request([AppCommand::Layout(LayoutCommand::Space(
            SpaceCommand::Open,
        ))]);

        assert!(request.should_toggle);
        assert_eq!(request.url_override, Some(SPACES_WEBVIEW_URL.to_string()));
    }

    #[test]
    fn tab_new_command_does_not_dismiss_command_bar() {
        let request =
            command_bar_open_request([AppCommand::Layout(LayoutCommand::Stack(StackCommand::New))]);

        assert!(!request.should_dismiss);
    }

    #[test]
    fn command_bar_open_runs_after_tab_commands() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_plugins(vmux_layout::stack::StackPlugin);
        app.add_plugins(CommandBarInputPlugin);

        let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
        let mut update = schedules.remove(Update).unwrap();
        update.initialize(app.world_mut()).unwrap();
        let graph = update.graph();
        let tab_command_set = graph
            .system_sets
            .get_key(vmux_layout::stack::StackCommandSet.intern())
            .unwrap();
        let read_command_systems = graph.systems_in_set(ReadAppCommands.intern()).unwrap();
        let tab_command_systems = graph
            .systems_in_set(vmux_layout::stack::StackCommandSet.intern())
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
    fn command_bar_open_bootstraps_hidden_modal_before_js_ready() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.init_resource::<NewStackContext>();
        app.init_resource::<ActiveSpace>();
        app.insert_resource(bevy_cef::prelude::CefSuppressKeyboardInput::default());
        app.insert_non_send_resource(Browsers::default());
        app.add_systems(Update, handle_open_command_bar.in_set(ReadAppCommands));

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
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::FocusAddressBar));

        app.update();

        let node = app.world().get::<Node>(modal).unwrap();
        let reveal = app.world().get::<PendingCommandBarReveal>(modal).unwrap();

        assert_eq!(node.display, Display::Flex);
        assert_eq!(reveal.open_id, 0);
        assert!(app.world().resource::<NewStackContext>().needs_open);
    }

    #[test]
    fn command_bar_open_retries_hidden_keyboard_targeted_modal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.init_resource::<NewStackContext>();
        app.init_resource::<ActiveSpace>();
        app.insert_resource(bevy_cef::prelude::CefSuppressKeyboardInput::default());
        app.insert_non_send_resource(Browsers::default());
        app.add_systems(Update, handle_open_command_bar.in_set(ReadAppCommands));

        app.world_mut().spawn((
            Modal,
            Node {
                display: Display::Flex,
                ..default()
            },
            Visibility::Hidden,
            CefKeyboardTarget,
            CefPointerTarget,
            PendingCommandBarReveal {
                frames: 0,
                open_id: 0,
                payload: None,
            },
        ));
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open)));

        app.update();

        assert!(app.world().resource::<NewStackContext>().needs_open);
    }

    #[test]
    fn spaces_url_from_command_bar_opens_spaces_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_observer(on_command_bar_action);
        app.init_resource::<NewStackContext>();
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let modal = app
            .world_mut()
            .spawn((
                Modal,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                Visibility::Inherited,
            ))
            .id();
        let space = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(space)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));

        app.world_mut()
            .entity_mut(modal)
            .trigger(|webview| BinReceive {
                webview,
                payload: CommandBarActionEvent {
                    action: "navigate".to_string(),
                    value: SPACES_WEBVIEW_URL.to_string(),
                },
            });
        app.update();

        let mut spaces_query = app.world_mut().query::<&SpacesView>();
        let spaces_count = spaces_query.iter(app.world()).count();

        assert_eq!(spaces_count, 1);
    }

    #[test]
    fn spaces_url_without_active_pane_spawns_spaces_page_layout() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_observer(on_command_bar_action);
        app.init_resource::<NewStackContext>();
        app.insert_resource(crate::layout::stack::FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let modal = app
            .world_mut()
            .spawn((
                Modal,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                Visibility::Inherited,
            ))
            .id();
        app.world_mut().spawn(PrimaryWindow);
        app.world_mut().spawn(crate::layout::window::Main);

        app.world_mut()
            .entity_mut(modal)
            .trigger(|webview| BinReceive {
                webview,
                payload: CommandBarActionEvent {
                    action: "navigate".to_string(),
                    value: SPACES_WEBVIEW_URL.to_string(),
                },
            });
        app.update();

        let mut spaces_query = app.world_mut().query::<&SpacesView>();
        assert_eq!(spaces_query.iter(app.world()).count(), 1);

        let mut tabs = app.world_mut().query::<&Tab>();
        assert_eq!(tabs.iter(app.world()).count(), 1);

        let tabs = {
            let mut tab_q = app
                .world_mut()
                .query_filtered::<(Entity, &PageMetadata, &Children), With<Stack>>();
            tab_q
                .iter(app.world())
                .map(|(entity, meta, children)| {
                    let has_spaces_view = children
                        .iter()
                        .any(|child| app.world().get::<SpacesView>(child).is_some());
                    (entity, meta.url.clone(), has_spaces_view)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].1, SPACES_WEBVIEW_URL);
        assert!(tabs[0].2);

        let ctx = app.world().resource::<NewStackContext>();
        assert!(!ctx.needs_open);
        assert!(ctx.stack.is_none());

        let focus = app.world().resource::<crate::layout::stack::FocusedStack>();
        assert!(focus.tab.is_some());
        assert!(focus.pane.is_some());
        assert!(focus.stack.is_some());
    }

    #[derive(Resource, Default)]
    struct CapturedSpaceCommand(Option<SpaceCommandEvent>);

    fn capture_space_command(
        trigger: On<BinReceive<SpaceCommandEvent>>,
        mut captured: ResMut<CapturedSpaceCommand>,
    ) {
        captured.0 = Some(trigger.event().payload.clone());
    }

    #[test]
    fn space_action_forwards_attach_event() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_observer(on_command_bar_action);
        app.add_observer(capture_space_command);
        app.init_resource::<NewStackContext>();
        app.init_resource::<CapturedSpaceCommand>();
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let modal = app
            .world_mut()
            .spawn((
                Modal,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                Visibility::Inherited,
            ))
            .id();
        let space = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(space)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));

        app.world_mut()
            .entity_mut(modal)
            .trigger(|webview| BinReceive {
                webview,
                payload: CommandBarActionEvent {
                    action: "space".to_string(),
                    value: "work".to_string(),
                },
            });
        app.update();

        let captured = app
            .world()
            .resource::<CapturedSpaceCommand>()
            .0
            .clone()
            .unwrap();
        assert_eq!(captured.command, "attach");
        assert_eq!(captured.space_id.as_deref(), Some("work"));
    }

    #[test]
    fn space_action_does_not_restore_keyboard_to_old_browser() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_observer(on_command_bar_action);
        app.init_resource::<NewStackContext>();
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let modal = app
            .world_mut()
            .spawn((
                Modal,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                Visibility::Inherited,
            ))
            .id();
        let space = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(space)))
            .id();
        let tab = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        let browser = app.world_mut().spawn((Browser, ChildOf(tab))).id();

        app.world_mut()
            .entity_mut(modal)
            .trigger(|webview| BinReceive {
                webview,
                payload: CommandBarActionEvent {
                    action: "space".to_string(),
                    value: "work".to_string(),
                },
            });
        app.update();

        assert!(app.world().get::<CefKeyboardTarget>(browser).is_none());
    }

    #[test]
    fn space_action_disables_modal_picking() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_observer(on_command_bar_action);
        app.init_resource::<NewStackContext>();
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let modal = app
            .world_mut()
            .spawn((
                Modal,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                Visibility::Inherited,
                bevy::picking::Pickable::default(),
            ))
            .id();
        let space = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(space)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));

        app.world_mut()
            .entity_mut(modal)
            .trigger(|webview| BinReceive {
                webview,
                payload: CommandBarActionEvent {
                    action: "space".to_string(),
                    value: "work".to_string(),
                },
            });
        app.update();

        assert_eq!(
            app.world().get::<bevy::picking::Pickable>(modal),
            Some(&bevy::picking::Pickable::IGNORE)
        );
    }

    #[test]
    fn dismissing_only_pending_stack_in_new_space_closes_space() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_observer(on_command_bar_action);
        app.init_resource::<NewStackContext>();
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let modal = app
            .world_mut()
            .spawn((
                Modal,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                Visibility::Inherited,
            ))
            .id();
        let root = app.world_mut().spawn_empty().id();
        let old_space = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(root)))
            .id();
        let old_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(old_space)))
            .id();
        let old_tab = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(old_pane)))
            .id();
        let new_space = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(2), ChildOf(root)))
            .id();
        let new_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(2), ChildOf(new_space)))
            .id();
        let pending_stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(2), ChildOf(new_pane)))
            .id();
        app.world_mut().resource_mut::<NewStackContext>().stack = Some(pending_stack);

        app.world_mut()
            .entity_mut(modal)
            .trigger(|webview| BinReceive {
                webview,
                payload: CommandBarActionEvent {
                    action: "dismiss".to_string(),
                    value: String::new(),
                },
            });
        app.update();

        assert!(app.world().get_entity(new_space).is_err());
        assert!(app.world().get_entity(old_space).is_ok());
        assert!(app.world().get_entity(old_tab).is_ok());
        assert_eq!(app.world().resource::<NewStackContext>().stack, None);
    }

    #[test]
    fn parse_pid_from_url_accepts_numeric() {
        assert_eq!(parse_pid_from_url("vmux://terminal/12345"), Some(12345));
        assert_eq!(parse_pid_from_url("vmux://terminal/0"), Some(0));
    }

    #[test]
    fn parse_pid_from_url_rejects_uuid_form() {
        let uuid_url = "vmux://terminal/ae724a54-c387-5359-0687-ccfc155558b6";
        assert_eq!(parse_pid_from_url(uuid_url), None);
    }

    #[test]
    fn parse_pid_from_url_rejects_empty_path() {
        assert_eq!(parse_pid_from_url("vmux://terminal/"), None);
    }

    #[test]
    fn parse_pid_from_url_rejects_overflow() {
        assert_eq!(
            parse_pid_from_url("vmux://terminal/99999999999999999"),
            None
        );
    }
}
