use crate::{
    browser::Browser,
    command::{
        AppCommand, BrowserCommand, PaneCommand, ReadAppCommands, SessionCommand, TabCommand,
        TerminalCommand,
    },
    layout::{
        pane::{Pane, PaneSplit},
        side_sheet::SideSheet,
        space::Space,
        tab::{Tab, active_among, collect_leaf_panes, focused_tab},
        window::Modal,
    },
    processes_monitor::ProcessesMonitor,
    sessions::{ActiveSession, SessionsView},
    settings::AppSettings,
    terminal::Terminal,
};
use bevy::{
    ecs::message::MessageReader, ecs::relationship::Relationship, picking::Pickable, prelude::*,
    ui::UiSystems,
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::webview_debug_log;
use vmux_command_bar::event::{
    COMMAND_BAR_OPEN_EVENT, CommandBarActionEvent, CommandBarCommandEntry, CommandBarOpenEvent,
    CommandBarReadyEvent, CommandBarRenderedEvent, CommandBarSession, CommandBarTab,
    PATH_COMPLETE_RESPONSE, PathCompleteRequest, PathCompleteResponse, PathEntry,
};
use vmux_core::PageMetadata;
use vmux_history::{LastActivatedAt, now_millis};
pub(crate) use vmux_layout::NewTabContext;
use vmux_layout::{
    Header,
    event::{PROCESSES_WEBVIEW_URL, TERMINAL_WEBVIEW_URL},
};
use vmux_service::protocol::ProcessId;
use vmux_sessions::event::{SESSIONS_WEBVIEW_URL, SessionCommandEvent};

/// Try to extract a process UUID from `vmux://terminal/{uuid}`.
fn parse_process_id_from_url(url: &str) -> Option<ProcessId> {
    let suffix = url.strip_prefix(TERMINAL_WEBVIEW_URL)?;
    suffix.parse::<ProcessId>().ok()
}

/// Deferred visibility for the command bar modal. Counts frames after Display::Flex
/// so CEF can resize the webview before the modal becomes visible.
#[derive(Component)]
struct CommandBarReady;

#[derive(Component)]
struct CommandBarRenderedOpen(u64);

#[derive(Component)]
pub(crate) struct PendingCommandBarReveal {
    frames: u8,
    open_id: u64,
}

pub(crate) struct CommandBarInputPlugin;

impl Plugin for CommandBarInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NewTabContext>()
            .add_plugins(JsEmitEventPlugin::<CommandBarActionEvent>::default())
            .add_plugins(JsEmitEventPlugin::<PathCompleteRequest>::default())
            .add_plugins(JsEmitEventPlugin::<CommandBarReadyEvent>::default())
            .add_plugins(JsEmitEventPlugin::<CommandBarRenderedEvent>::default())
            .add_observer(on_command_bar_action)
            .add_observer(on_path_complete_request)
            .add_observer(on_command_bar_ready)
            .add_observer(on_command_bar_rendered)
            .add_systems(
                Update,
                handle_open_command_bar
                    .in_set(ReadAppCommands)
                    .after(crate::layout::space::SpaceCommandSet),
            )
            .add_systems(
                Update,
                deferred_dismiss_modal
                    .after(ReadAppCommands)
                    .before(crate::layout::tab::ComputeFocusSet),
            )
            .add_systems(PostUpdate, reveal_command_bar.after(UiSystems::Layout));
    }
}

pub struct CommandBarEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub shortcut: &'static str,
}

pub fn command_list() -> Vec<CommandBarEntry> {
    AppCommand::command_bar_entries()
        .into_iter()
        .map(|(id, name, shortcut)| CommandBarEntry { id, name, shortcut })
        .collect()
}

pub fn match_command(id: &str) -> Option<AppCommand> {
    AppCommand::from_menu_id(id)
}

/// Returns true when the command bar modal is currently visible.
pub fn is_command_bar_open(modal_q: &Query<&Node, With<Modal>>) -> bool {
    modal_q.iter().any(|n| n.display != Display::None)
}

fn command_bar_open_delivery_ready(
    has_browser: bool,
    host_emit_ready: bool,
    command_bar_ready: bool,
) -> bool {
    has_browser && host_emit_ready && command_bar_ready
}

fn command_bar_reveal_ready(
    has_browser: bool,
    host_emit_ready: bool,
    command_bar_ready: bool,
    rendered_open: bool,
) -> bool {
    has_browser && host_emit_ready && command_bar_ready && rendered_open
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

fn on_command_bar_ready(trigger: On<Receive<CommandBarReadyEvent>>, mut commands: Commands) {
    commands
        .entity(trigger.event().webview)
        .insert(CommandBarReady);
}

fn on_command_bar_rendered(trigger: On<Receive<CommandBarRenderedEvent>>, mut commands: Commands) {
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
            AppCommand::Session(SessionCommand::Open) => {
                request.should_toggle = true;
                request.url_override = Some(SESSIONS_WEBVIEW_URL.to_string());
            }
            AppCommand::Tab(TabCommand::New) | AppCommand::Tab(TabCommand::Close) => {
                request.should_dismiss = true;
            }
            AppCommand::Tab(TabCommand::Next | TabCommand::Previous)
            | AppCommand::Pane(
                PaneCommand::SelectLeft
                | PaneCommand::SelectRight
                | PaneCommand::SelectUp
                | PaneCommand::SelectDown,
            ) => {
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
            Has<CommandBarReady>,
            Option<&CommandBarRenderedOpen>,
            Has<PendingCommandBarReveal>,
        ),
        With<Modal>,
    >,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
    browsers: NonSend<Browsers>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    tab_q: Query<Entity, With<Tab>>,
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
    mut session_params: ParamSet<(Res<ActiveSession>, ResMut<NewTabContext>)>,
    mut commands: Commands,
) {
    let active_tab_count = tab_q.iter().count();
    let active_session = session_params.p0().clone();
    let session_name = active_session.record.name.clone();
    let mut new_tab_ctx = session_params.p1();

    let request = command_bar_open_request(reader.read().copied());
    let mut should_open = false;
    let should_toggle = request.should_toggle;
    let should_dismiss = request.should_dismiss;
    let should_dismiss_nav = request.should_dismiss_nav;
    let url_override = request.url_override;

    // Dismiss command bar when Cmd+T or Cmd+W fires while open
    if should_dismiss {
        let is_open = modal_q
            .single()
            .map(|(_, n, _, _, _, _)| n.display != Display::None)
            .unwrap_or(false);
        if is_open {
            let Ok((modal_e, mut modal_node, mut modal_vis, _, _, _)) = modal_q.single_mut() else {
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
                .remove::<PendingCommandBarReveal>();
            // Discard empty tab created by a previous Cmd+T
            if let Some(tab_e) = new_tab_ctx.tab.take() {
                commands.entity(tab_e).despawn();
                if let Some(prev) = new_tab_ctx.previous_tab.take()
                    && let Ok(children) = all_children.get(prev)
                {
                    for child in children.iter() {
                        if content_browsers.contains(child) {
                            commands.entity(child).insert(CefKeyboardTarget);
                        }
                    }
                }
            } else {
                let (_, _, active_tab) = focused_tab(
                    &spaces,
                    &all_children,
                    &leaf_panes,
                    &pane_ts,
                    &pane_children,
                    &tab_ts,
                );
                if let Some(tab) = active_tab {
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
            new_tab_ctx.needs_open = false;
            return;
        }
    }

    // Navigation dismiss: close modal only, leave empty tab for
    // handle_tab_commands / on_pane_select to clean up.
    if should_dismiss_nav {
        let is_open = modal_q
            .single()
            .map(|(_, n, _, _, _, _)| n.display != Display::None)
            .unwrap_or(false);
        if is_open {
            let Ok((modal_e, mut modal_node, mut modal_vis, _, _, _)) = modal_q.single_mut() else {
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
                .remove::<PendingCommandBarReveal>();
            new_tab_ctx.needs_open = false;
            return;
        }
    }

    if new_tab_ctx.needs_open {
        should_open = true;
        new_tab_ctx.needs_open = false;
    }

    if should_toggle {
        let is_open = modal_q
            .single()
            .map(|(_, n, _, _, _, _)| n.display != Display::None)
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
        command_bar_ready,
        rendered_open,
        modal_pending_reveal,
    )) = modal_q.single_mut()
    else {
        return;
    };

    let is_new_tab = new_tab_ctx.tab.is_some();
    let was_open = modal_node.display != Display::None;

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
    } else if is_new_tab {
        String::new()
    } else {
        let (_, _, active_tab) = focused_tab(
            &spaces,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &tab_ts,
        );
        active_tab
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
    let active_space = active_among(spaces.iter());
    let mut bar_tabs = Vec::new();
    if let Some(space) = active_space {
        let (_, _, active_tab) = focused_tab(
            &spaces,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &tab_ts,
        );
        let active_pane = active_tab.and_then(|t| child_of_q.get(t).ok().map(|co| co.get()));
        let mut space_panes = Vec::new();
        collect_leaf_panes(space, &all_children, &leaf_panes, &mut space_panes);
        for &pane_e in &space_panes {
            let is_active_pane = active_pane == Some(pane_e);
            if let Ok(children) = pane_children.get(pane_e) {
                let mut tab_index = 0usize;
                for child in children.iter() {
                    if !tab_q.contains(child) {
                        continue;
                    }
                    let tab_is_active = active_tab == Some(child) && is_active_pane;
                    if let Ok(tab_kids) = all_children.get(child) {
                        for browser_e in tab_kids.iter() {
                            if let Ok(meta) = browser_meta.get(browser_e) {
                                bar_tabs.push(CommandBarTab {
                                    title: meta.title.clone(),
                                    url: meta.url.clone(),
                                    pane_id: pane_e.to_bits(),
                                    tab_index,
                                    is_active: tab_is_active,
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
    let bar_commands: Vec<CommandBarCommandEntry> = command_list()
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
        "command_bar open entity={modal_e:?} was_open={was_open} has_browser={has_browser} host_emit_ready={host_emit_ready} command_bar_ready={command_bar_ready} rendered={rendered_matches} pending_reveal={modal_pending_reveal} visibility={:?} new_tab={is_new_tab}",
        *modal_vis
    ));

    if !command_bar_open_delivery_ready(has_browser, host_emit_ready, command_bar_ready) {
        new_tab_ctx.needs_open = true;
        return;
    }

    let bar_sessions = crate::sessions::active_session_rows(&active_session, active_tab_count)
        .into_iter()
        .map(|session| CommandBarSession {
            id: session.id,
            name: session.name,
            profile: session.profile,
            is_active: session.is_active,
            tab_count: session.tab_count,
        })
        .collect();

    let open_id = now_millis() as u64;
    let payload = command_bar_open_payload(
        open_id,
        session_name,
        current_url,
        bar_sessions,
        bar_tabs,
        bar_commands,
        is_new_tab,
    );
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    commands.trigger(HostEmitEvent::new(
        modal_e,
        COMMAND_BAR_OPEN_EVENT,
        &ron_body,
    ));
    if should_start_command_bar_reveal(
        has_browser,
        host_emit_ready,
        command_bar_ready,
        rendered_open.is_some_and(|rendered| rendered.0 == open_id),
        modal_pending_reveal,
        *modal_vis,
    ) {
        commands
            .entity(modal_e)
            .insert(PendingCommandBarReveal { frames: 0, open_id });
    } else {
        commands
            .entity(modal_e)
            .remove::<CommandBarRenderedOpen>()
            .insert(PendingCommandBarReveal { frames: 0, open_id });
    }
    webview_debug_log(format!(
        "command_bar emit open entity={modal_e:?} payload_len={} tabs={} commands={}",
        ron_body.len(),
        payload.tabs.len(),
        payload.commands.len()
    ));
    if !command_bar_ready {
        new_tab_ctx.needs_open = true;
    }
}

fn command_bar_open_payload(
    open_id: u64,
    session_name: String,
    url: String,
    sessions: Vec<CommandBarSession>,
    tabs: Vec<CommandBarTab>,
    commands: Vec<CommandBarCommandEntry>,
    new_tab: bool,
) -> CommandBarOpenEvent {
    CommandBarOpenEvent {
        open_id,
        url,
        session_name,
        sessions,
        tabs,
        commands,
        new_tab,
    }
}

fn on_command_bar_action(
    trigger: On<Receive<CommandBarActionEvent>>,
    mut modal_q: Query<(Entity, &mut Node, &mut Visibility), With<Modal>>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    tab_q: Query<Entity, With<Tab>>,
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
    settings: Res<AppSettings>,
    mut new_tab_ctx: ResMut<NewTabContext>,
    mut writer: MessageWriter<AppCommand>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let webview = trigger.event().webview;
    let evt = &trigger.event().payload;
    let empty_tab = new_tab_ctx.tab;
    let previous_tab = new_tab_ctx.previous_tab;
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
                if let Some(tab_e) = empty_tab {
                    commands.entity(tab_e).insert(PageMetadata {
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
                            ChildOf(tab_e),
                        ))
                        .id();
                    commands.entity(term_e).insert(CefKeyboardTarget);
                    new_tab_ctx.tab = None;
                    new_tab_ctx.previous_tab = None;
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

                if let Some(tab_e) = empty_tab {
                    // New tab mode: attach content to the empty tab
                    if url.starts_with("vmux://terminal") {
                        let term_e = if let Some(pid) = parse_process_id_from_url(&url) {
                            commands
                                .spawn((
                                    Terminal::reattach(&mut meshes, &mut webview_mt, pid),
                                    ChildOf(tab_e),
                                ))
                                .id()
                        } else {
                            commands.entity(tab_e).insert(PageMetadata {
                                url: TERMINAL_WEBVIEW_URL.to_string(),
                                title: "Terminal".to_string(),
                                ..default()
                            });
                            commands
                                .spawn((
                                    Terminal::new(&mut meshes, &mut webview_mt, &settings),
                                    ChildOf(tab_e),
                                ))
                                .id()
                        };
                        commands.entity(term_e).insert(CefKeyboardTarget);
                    } else if url.starts_with(PROCESSES_WEBVIEW_URL.trim_end_matches('/')) {
                        commands.entity(tab_e).insert(PageMetadata {
                            url: PROCESSES_WEBVIEW_URL.to_string(),
                            title: "Background Services".to_string(),
                            ..default()
                        });
                        commands.spawn((
                            ProcessesMonitor::new(&mut meshes, &mut webview_mt),
                            ChildOf(tab_e),
                        ));
                    } else if url.starts_with(SESSIONS_WEBVIEW_URL.trim_end_matches('/')) {
                        commands.entity(tab_e).insert(PageMetadata {
                            url: SESSIONS_WEBVIEW_URL.to_string(),
                            title: "Sessions".to_string(),
                            ..default()
                        });
                        commands.spawn((
                            SessionsView::new(&mut meshes, &mut webview_mt),
                            ChildOf(tab_e),
                        ));
                    } else {
                        let browser_e = commands
                            .spawn((
                                Browser::new(&mut meshes, &mut webview_mt, &url),
                                ChildOf(tab_e),
                            ))
                            .id();
                        commands.entity(browser_e).insert(CefKeyboardTarget);
                    }
                    new_tab_ctx.tab = None;
                    new_tab_ctx.previous_tab = None;
                    custom_keyboard_restore = true;
                } else {
                    // Normal mode: navigate or spawn terminal in current tab
                    if let Some(pid) = parse_process_id_from_url(&url) {
                        // Reattach to existing service-managed process in a new tab
                        let (_, active_pane_opt, _) = focused_tab(
                            &spaces,
                            &all_children,
                            &leaf_panes,
                            &pane_ts,
                            &pane_children,
                            &tab_ts,
                        );
                        if let Some(pane_e) = active_pane_opt {
                            let tab_e = commands
                                .spawn((
                                    crate::layout::tab::tab_bundle(),
                                    LastActivatedAt::now(),
                                    ChildOf(pane_e),
                                ))
                                .id();
                            let term_e = commands
                                .spawn((
                                    Terminal::reattach(&mut meshes, &mut webview_mt, pid),
                                    ChildOf(tab_e),
                                ))
                                .id();
                            commands.entity(term_e).insert(CefKeyboardTarget);
                        }
                    } else if url.starts_with("vmux://terminal") {
                        writer.write(AppCommand::Terminal(TerminalCommand::New));
                    } else if url.starts_with(PROCESSES_WEBVIEW_URL.trim_end_matches('/')) {
                        use crate::command::ServiceCommand;
                        writer.write(AppCommand::Service(ServiceCommand::Open));
                    } else if url.starts_with(SESSIONS_WEBVIEW_URL.trim_end_matches('/')) {
                        let (_, active_pane_opt, _) = focused_tab(
                            &spaces,
                            &all_children,
                            &leaf_panes,
                            &pane_ts,
                            &pane_children,
                            &tab_ts,
                        );
                        if let Some(pane_e) = active_pane_opt {
                            let tab_e = commands
                                .spawn((
                                    crate::layout::tab::tab_bundle(),
                                    LastActivatedAt::now(),
                                    ChildOf(pane_e),
                                ))
                                .id();
                            commands.entity(tab_e).insert(PageMetadata {
                                url: SESSIONS_WEBVIEW_URL.to_string(),
                                title: "Sessions".to_string(),
                                ..default()
                            });
                            commands.spawn((
                                SessionsView::new(&mut meshes, &mut webview_mt),
                                ChildOf(tab_e),
                            ));
                        }
                    } else {
                        let (_, _, active_tab) = focused_tab(
                            &spaces,
                            &all_children,
                            &leaf_panes,
                            &pane_ts,
                            &pane_children,
                            &tab_ts,
                        );
                        if let Some(tab) = active_tab {
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
            // Check if value is a vmux://terminal/{id} URL — reattach
            if let Some(pid) = parse_process_id_from_url(&evt.value) {
                if let Some(tab_e) = empty_tab {
                    let term_e = commands
                        .spawn((
                            Terminal::reattach(&mut meshes, &mut webview_mt, pid),
                            ChildOf(tab_e),
                        ))
                        .id();
                    commands.entity(term_e).insert(CefKeyboardTarget);
                    new_tab_ctx.tab = None;
                    new_tab_ctx.previous_tab = None;
                    custom_keyboard_restore = true;
                } else {
                    let (_, active_pane_opt, _) = focused_tab(
                        &spaces,
                        &all_children,
                        &leaf_panes,
                        &pane_ts,
                        &pane_children,
                        &tab_ts,
                    );
                    if let Some(pane_e) = active_pane_opt {
                        let tab_e = commands
                            .spawn((
                                crate::layout::tab::tab_bundle(),
                                LastActivatedAt::now(),
                                ChildOf(pane_e),
                            ))
                            .id();
                        let term_e = commands
                            .spawn((
                                Terminal::reattach(&mut meshes, &mut webview_mt, pid),
                                ChildOf(tab_e),
                            ))
                            .id();
                        commands.entity(term_e).insert(CefKeyboardTarget);
                    }
                }
            } else {
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
                if let Some(tab_e) = empty_tab {
                    commands.entity(tab_e).insert(PageMetadata {
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
                            ChildOf(tab_e),
                        ))
                        .id();
                    commands.entity(term_e).insert(CefKeyboardTarget);
                    new_tab_ctx.tab = None;
                    new_tab_ctx.previous_tab = None;
                    custom_keyboard_restore = true;
                } else {
                    let (_, active_pane_opt, _) = focused_tab(
                        &spaces,
                        &all_children,
                        &leaf_panes,
                        &pane_ts,
                        &pane_children,
                        &tab_ts,
                    );
                    if let Some(pane_e) = active_pane_opt {
                        let tab_e = commands
                            .spawn((
                                crate::layout::tab::tab_bundle(),
                                LastActivatedAt::now(),
                                ChildOf(pane_e),
                            ))
                            .id();
                        commands.entity(tab_e).insert(PageMetadata {
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
                                ChildOf(tab_e),
                            ))
                            .id();
                        commands.entity(term_e).insert(CefKeyboardTarget);
                    } else {
                        writer.write(AppCommand::Terminal(TerminalCommand::New));
                    }
                }
            } // end reattach else
        }
        "command" => {
            if let Some(cmd) = match_command(&evt.value) {
                writer.write(cmd);
            }
            // If in new-tab mode and a command was executed, clean up the empty tab
            if let Some(tab_e) = empty_tab {
                commands.entity(tab_e).despawn();
                new_tab_ctx.tab = None;
                new_tab_ctx.previous_tab = None;
            }
        }
        "session" => {
            custom_keyboard_restore = true;
            if !evt.value.is_empty() {
                commands.trigger(Receive {
                    webview,
                    payload: SessionCommandEvent {
                        command: "attach".to_string(),
                        session_id: Some(evt.value.clone()),
                        name: None,
                    },
                });
            }
            if let Some(tab_e) = empty_tab {
                commands.entity(tab_e).despawn();
                new_tab_ctx.tab = None;
                new_tab_ctx.previous_tab = None;
            }
        }
        "switch_tab" => {
            // Despawn empty tab if in new-tab mode
            if let Some(tab_e) = empty_tab {
                commands.entity(tab_e).despawn();
                new_tab_ctx.tab = None;
                new_tab_ctx.previous_tab = None;
            }
            if let Some((pane_bits, tab_idx)) = evt.value.split_once(':')
                && let (Ok(pane_id), Ok(tab_index)) =
                    (pane_bits.parse::<u64>(), tab_idx.parse::<usize>())
                && let Some(target_pane) = leaf_panes.iter().find(|e| e.to_bits() == pane_id)
            {
                commands.entity(target_pane).insert(LastActivatedAt::now());
                if let Ok(children) = pane_children.get(target_pane) {
                    let tabs: Vec<Entity> =
                        children.iter().filter(|&e| tab_q.contains(e)).collect();
                    if let Some(&target_tab) = tabs.get(tab_index) {
                        commands.entity(target_tab).insert(LastActivatedAt::now());
                    }
                }
            }
        }
        _ => {
            // "dismiss" and unknown actions
            if let Some(tab_e) = empty_tab {
                let closed_space = close_space_if_only_pending_tab(
                    tab_e,
                    &spaces,
                    &child_of_q,
                    &all_children,
                    &tab_q,
                    &mut commands,
                );
                if !closed_space {
                    commands.entity(tab_e).despawn();
                }
                new_tab_ctx.tab = None;
                if !closed_space {
                    // Restore keyboard to previous tab's browser
                    if let Some(prev) = previous_tab
                        && let Ok(children) = all_children.get(prev)
                    {
                        for child in children.iter() {
                            if content_browsers.contains(child) {
                                commands.entity(child).insert(CefKeyboardTarget);
                            }
                        }
                    }
                }
                new_tab_ctx.previous_tab = None;
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
            .remove::<PendingCommandBarReveal>();
    }
    if !custom_keyboard_restore {
        let (_, _, active_tab) = focused_tab(
            &spaces,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &tab_ts,
        );
        if let Some(tab) = active_tab {
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

fn close_space_if_only_pending_tab(
    tab: Entity,
    spaces: &Query<(Entity, &LastActivatedAt), With<Space>>,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
    tab_q: &Query<Entity, With<Tab>>,
    commands: &mut Commands,
) -> bool {
    let Some(space) = ancestor_space(tab, spaces, child_of_q) else {
        return false;
    };
    if entity_tree_contains_tab_other_than(space, tab, all_children, tab_q) {
        return false;
    }
    let siblings = sibling_spaces(space, spaces, child_of_q, all_children);
    if siblings.len() <= 1 {
        return false;
    }
    if let Some(next) = pick_space_after_close(space, &siblings) {
        commands.entity(next).insert(LastActivatedAt::now());
    }
    commands.entity(space).despawn();
    true
}

fn ancestor_space(
    entity: Entity,
    spaces: &Query<(Entity, &LastActivatedAt), With<Space>>,
    child_of_q: &Query<&ChildOf>,
) -> Option<Entity> {
    let mut current = entity;
    while let Ok(parent) = child_of_q.get(current).map(Relationship::get) {
        if spaces.get(parent).is_ok() {
            return Some(parent);
        }
        current = parent;
    }
    None
}

fn entity_tree_contains_tab_other_than(
    entity: Entity,
    ignored_tab: Entity,
    all_children: &Query<&Children>,
    tab_q: &Query<Entity, With<Tab>>,
) -> bool {
    (tab_q.contains(entity) && entity != ignored_tab)
        || all_children.get(entity).is_ok_and(|children| {
            children.iter().any(|child| {
                entity_tree_contains_tab_other_than(child, ignored_tab, all_children, tab_q)
            })
        })
}

fn sibling_spaces(
    space: Entity,
    spaces: &Query<(Entity, &LastActivatedAt), With<Space>>,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
) -> Vec<Entity> {
    let Ok(parent) = child_of_q.get(space).map(Relationship::get) else {
        return vec![space];
    };
    let Ok(children) = all_children.get(parent) else {
        return vec![space];
    };
    children.iter().filter(|e| spaces.get(*e).is_ok()).collect()
}

fn pick_space_after_close(active: Entity, siblings: &[Entity]) -> Option<Entity> {
    if siblings.len() <= 1 {
        return None;
    }
    let idx = siblings.iter().position(|e| *e == active)?;
    let next_idx = if idx + 1 < siblings.len() { idx + 1 } else { 0 };
    let target = siblings[next_idx];
    if target == active { None } else { Some(target) }
}

/// Closes the command bar modal when `NewTabContext::dismiss_modal` is set.
/// Runs after `ReadAppCommands` so that `handle_tab_commands` can validate
/// the target index before requesting a dismiss.
fn deferred_dismiss_modal(
    mut new_tab_ctx: ResMut<NewTabContext>,
    mut modal_q: Query<(Entity, &mut Node, &mut Visibility), With<Modal>>,
    mut commands: Commands,
) {
    if !new_tab_ctx.dismiss_modal {
        return;
    }
    new_tab_ctx.dismiss_modal = false;
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
            .remove::<PendingCommandBarReveal>();
    }
}

/// Waits 2 frames after `Display::Flex` before revealing the command bar so that
/// Bevy UI layout + CEF resize can run while the webview is still invisible.
fn reveal_command_bar(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut Visibility,
            &mut PendingCommandBarReveal,
            Option<&CommandBarRenderedOpen>,
        ),
        With<Modal>,
    >,
) {
    for (entity, mut vis, mut pending, rendered) in &mut query {
        let rendered = rendered.is_some_and(|rendered| rendered.0 == pending.open_id);
        if !rendered {
            continue;
        }
        if pending.frames >= 2 {
            *vis = Visibility::Inherited;
            commands.entity(entity).remove::<PendingCommandBarReveal>();
            webview_debug_log(format!("command_bar reveal entity={entity:?}"));
        } else {
            pending.frames += 1;
        }
    }
}

fn on_path_complete_request(
    trigger: On<Receive<PathCompleteRequest>>,
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
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    commands.trigger(HostEmitEvent::new(
        modal_e,
        PATH_COMPLETE_RESPONSE,
        &ron_body,
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
        }
    }

    #[test]
    fn command_bar_open_waits_for_command_bar_listener() {
        assert!(!command_bar_open_delivery_ready(true, true, false));
        assert!(command_bar_open_delivery_ready(true, true, true));
    }

    #[test]
    fn command_bar_reveal_waits_for_main_frame_and_ui_listener() {
        assert!(!command_bar_reveal_ready(true, false, true, true));
        assert!(!command_bar_reveal_ready(true, true, false, true));
        assert!(command_bar_reveal_ready(true, true, true, true));
    }

    #[test]
    fn command_bar_reveal_waits_for_rendered_open_payload() {
        assert!(!command_bar_reveal_ready(true, true, true, false));
        assert!(command_bar_reveal_ready(true, true, true, true));
    }

    #[test]
    fn command_bar_open_requires_browser_main_frame() {
        assert!(!command_bar_open_delivery_ready(false, true, true));
        assert!(!command_bar_open_delivery_ready(true, false, true));
        assert!(command_bar_open_delivery_ready(true, true, true));
    }

    #[test]
    fn command_bar_reveal_does_not_start_before_ui_listener() {
        assert!(!should_start_command_bar_reveal(
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
            true,
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
    fn command_bar_payload_includes_session_name() {
        let payload = command_bar_open_payload(
            7,
            "Work".to_string(),
            "https://example.com".to_string(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            false,
        );

        assert_eq!(payload.session_name, "Work");
        assert_eq!(payload.open_id, 7);
    }

    #[test]
    fn command_bar_payload_includes_sessions() {
        let sessions = vec![CommandBarSession {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: "default".to_string(),
            is_active: true,
            tab_count: 2,
        }];

        let payload = command_bar_open_payload(
            8,
            "Work".to_string(),
            "vmux://sessions/".to_string(),
            sessions.clone(),
            Vec::new(),
            Vec::new(),
            false,
        );

        assert_eq!(payload.sessions, sessions);
    }

    #[test]
    fn session_open_command_prefills_sessions_url() {
        let request = command_bar_open_request([AppCommand::Session(SessionCommand::Open)]);

        assert!(request.should_toggle);
        assert_eq!(request.url_override, Some(SESSIONS_WEBVIEW_URL.to_string()));
    }

    #[test]
    fn sessions_url_from_command_bar_opens_sessions_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_observer(on_command_bar_action);
        app.init_resource::<NewTabContext>();
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
            .spawn((Space::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(space)))
            .id();
        app.world_mut()
            .spawn((Tab::default(), LastActivatedAt::now(), ChildOf(pane)));

        app.world_mut()
            .entity_mut(modal)
            .trigger(|webview| Receive {
                webview,
                payload: CommandBarActionEvent {
                    action: "navigate".to_string(),
                    value: SESSIONS_WEBVIEW_URL.to_string(),
                },
            });
        app.update();

        let mut sessions_query = app.world_mut().query::<&SessionsView>();
        let sessions_count = sessions_query.iter(app.world()).count();

        assert_eq!(sessions_count, 1);
    }

    #[derive(Resource, Default)]
    struct CapturedSessionCommand(Option<SessionCommandEvent>);

    fn capture_session_command(
        trigger: On<Receive<SessionCommandEvent>>,
        mut captured: ResMut<CapturedSessionCommand>,
    ) {
        captured.0 = Some(trigger.event().payload.clone());
    }

    #[test]
    fn session_action_forwards_attach_event() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_observer(on_command_bar_action);
        app.add_observer(capture_session_command);
        app.init_resource::<NewTabContext>();
        app.init_resource::<CapturedSessionCommand>();
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
            .spawn((Space::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(space)))
            .id();
        app.world_mut()
            .spawn((Tab::default(), LastActivatedAt::now(), ChildOf(pane)));

        app.world_mut()
            .entity_mut(modal)
            .trigger(|webview| Receive {
                webview,
                payload: CommandBarActionEvent {
                    action: "session".to_string(),
                    value: "work".to_string(),
                },
            });
        app.update();

        let captured = app
            .world()
            .resource::<CapturedSessionCommand>()
            .0
            .clone()
            .unwrap();
        assert_eq!(captured.command, "attach");
        assert_eq!(captured.session_id.as_deref(), Some("work"));
    }

    #[test]
    fn session_action_does_not_restore_keyboard_to_old_browser() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_observer(on_command_bar_action);
        app.init_resource::<NewTabContext>();
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
            .spawn((Space::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(space)))
            .id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        let browser = app.world_mut().spawn((Browser, ChildOf(tab))).id();

        app.world_mut()
            .entity_mut(modal)
            .trigger(|webview| Receive {
                webview,
                payload: CommandBarActionEvent {
                    action: "session".to_string(),
                    value: "work".to_string(),
                },
            });
        app.update();

        assert!(app.world().get::<CefKeyboardTarget>(browser).is_none());
    }

    #[test]
    fn session_action_disables_modal_picking() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_observer(on_command_bar_action);
        app.init_resource::<NewTabContext>();
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
            .spawn((Space::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(space)))
            .id();
        app.world_mut()
            .spawn((Tab::default(), LastActivatedAt::now(), ChildOf(pane)));

        app.world_mut()
            .entity_mut(modal)
            .trigger(|webview| Receive {
                webview,
                payload: CommandBarActionEvent {
                    action: "session".to_string(),
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
    fn dismissing_only_pending_tab_in_new_space_closes_space() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_observer(on_command_bar_action);
        app.init_resource::<NewTabContext>();
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
            .spawn((Space::default(), LastActivatedAt(1), ChildOf(root)))
            .id();
        let old_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(old_space)))
            .id();
        let old_tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(old_pane)))
            .id();
        let new_space = app
            .world_mut()
            .spawn((Space::default(), LastActivatedAt(2), ChildOf(root)))
            .id();
        let new_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(2), ChildOf(new_space)))
            .id();
        let pending_tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(2), ChildOf(new_pane)))
            .id();
        app.world_mut().resource_mut::<NewTabContext>().tab = Some(pending_tab);

        app.world_mut()
            .entity_mut(modal)
            .trigger(|webview| Receive {
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
        assert_eq!(app.world().resource::<NewTabContext>().tab, None);
    }
}
