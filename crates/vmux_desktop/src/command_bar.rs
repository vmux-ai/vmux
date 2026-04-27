use crate::{
    browser::Browser,
    command::{AppCommand, BrowserCommand, PaneCommand, ReadAppCommands, TabCommand},
    layout::{
        pane::{Pane, PaneSplit},
        side_sheet::SideSheet,
        space::Space,
        tab::{Tab, active_among, collect_leaf_panes, focused_tab},
        window::Modal,
    },
    settings::AppSettings,
    terminal::Terminal,
};
use bevy::{
    ecs::message::MessageReader, ecs::relationship::Relationship, prelude::*, ui::UiSystems,
};
use bevy_cef::prelude::*;
use vmux_command_bar::event::{
    COMMAND_BAR_OPEN_EVENT, CommandBarActionEvent, CommandBarCommandEntry, CommandBarOpenEvent,
    CommandBarTab, PATH_COMPLETE_RESPONSE, PathCompleteRequest, PathCompleteResponse, PathEntry,
    looks_like_explicit_path,
};
use vmux_header::{Header, PageMetadata};
use vmux_history::LastActivatedAt;
use vmux_terminal::event::TERMINAL_WEBVIEW_URL;

/// Deferred visibility for the command bar modal. Counts frames after Display::Flex
/// so CEF can resize the webview before the modal becomes visible.
#[derive(Component)]
struct PendingCommandBarReveal(u8);

/// Tracks an empty tab spawned by Cmd+T that is waiting for the user
/// to choose content via the command bar.
#[derive(Resource, Default)]
pub(crate) struct NewTabContext {
    /// The empty tab entity waiting for a Browser or Terminal child.
    pub tab: Option<Entity>,
    pub previous_tab: Option<Entity>,

    pub needs_open: bool,
    /// Set by handle_tab_commands when SelectIndex dismisses the empty
    /// tab; a PostUpdate system reads this to close the modal.
    pub dismiss_modal: bool,
}

pub(crate) struct CommandBarInputPlugin;

impl Plugin for CommandBarInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NewTabContext>()
            .add_plugins(JsEmitEventPlugin::<CommandBarActionEvent>::default())
            .add_plugins(JsEmitEventPlugin::<PathCompleteRequest>::default())
            .add_observer(on_command_bar_action)
            .add_observer(on_path_complete_request)
            .add_systems(Update, handle_open_command_bar.in_set(ReadAppCommands))
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

fn handle_open_command_bar(
    mut reader: MessageReader<AppCommand>,
    mut modal_q: Query<(Entity, &mut Node, &mut Visibility), With<Modal>>,
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
    mut new_tab_ctx: ResMut<NewTabContext>,
    mut commands: Commands,
) {
    // Determine whether to open: either via FocusAddressBar command or NewTabContext
    let mut should_open = false;
    let mut should_toggle = false;
    let mut should_dismiss = false;
    // Navigation commands (tab/pane switch) only close the modal; the empty
    // tab is cleaned up by handle_tab_commands / on_pane_select to avoid
    // deferred-command conflicts.
    let mut should_dismiss_nav = false;
    let mut url_override: Option<String> = None;

    for cmd in reader.read() {
        match *cmd {
            AppCommand::Browser(BrowserCommand::FocusAddressBar) => {
                should_toggle = true;
            }
            AppCommand::Browser(BrowserCommand::OpenCommandBar) => {
                should_toggle = true;
                url_override = Some(String::new());
            }
            AppCommand::Browser(BrowserCommand::OpenPathBar) => {
                should_toggle = true;
                url_override = Some("/".to_string());
            }
            AppCommand::Browser(BrowserCommand::OpenCommands) => {
                should_toggle = true;
                url_override = Some(">".to_string());
            }
            AppCommand::Tab(TabCommand::New) | AppCommand::Tab(TabCommand::Close) => {
                should_dismiss = true;
            }
            // Dismiss command bar when navigating tabs or panes.
            // SelectIndex / SelectLast are NOT included here; they are
            // handled by handle_tab_commands which only dismisses when
            // the target index actually exists.
            AppCommand::Tab(TabCommand::Next | TabCommand::Previous)
            | AppCommand::Pane(
                PaneCommand::SelectLeft
                | PaneCommand::SelectRight
                | PaneCommand::SelectUp
                | PaneCommand::SelectDown,
            ) => {
                should_dismiss_nav = true;
            }
            _ => {}
        }
    }

    // Dismiss command bar when Cmd+T or Cmd+W fires while open
    if should_dismiss {
        let is_open = modal_q
            .single()
            .map(|(_, n, _)| n.display != Display::None)
            .unwrap_or(false);
        if is_open {
            let Ok((modal_e, mut modal_node, mut modal_vis)) = modal_q.single_mut() else {
                return;
            };
            modal_node.display = Display::None;
            *modal_vis = Visibility::Hidden;
            commands
                .entity(modal_e)
                .remove::<CefKeyboardTarget>()
                .remove::<CefPointerTarget>()
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
            .map(|(_, n, _)| n.display != Display::None)
            .unwrap_or(false);
        if is_open {
            let Ok((modal_e, mut modal_node, mut modal_vis)) = modal_q.single_mut() else {
                return;
            };
            modal_node.display = Display::None;
            *modal_vis = Visibility::Hidden;
            commands
                .entity(modal_e)
                .remove::<CefKeyboardTarget>()
                .remove::<CefPointerTarget>()
                .remove::<PendingCommandBarReveal>();
            new_tab_ctx.needs_open = false;
            return;
        }
    }

    if new_tab_ctx.needs_open {
        info!("[handle_open_command_bar] needs_open=true, will open");
        should_open = true;
        new_tab_ctx.needs_open = false;
    }

    if should_toggle {
        let is_open = modal_q
            .single()
            .map(|(_, n, _)| n.display != Display::None)
            .unwrap_or(false);
        if !is_open {
            should_open = true;
        }
    }

    if !should_open {
        return;
    }

    info!("[handle_open_command_bar] opening modal");
    let Ok((modal_e, mut modal_node, _)) = modal_q.single_mut() else {
        info!("[handle_open_command_bar] no modal entity found!");
        return;
    };

    let is_new_tab = new_tab_ctx.tab.is_some();

    // Open command bar — keep hidden until CEF resizes (see reveal_command_bar)
    modal_node.display = Display::Flex;
    commands.entity(modal_e).insert(PendingCommandBarReveal(0));

    // Remove keyboard target from all content browsers
    for browser_e in &content_browsers {
        commands.entity(browser_e).remove::<CefKeyboardTarget>();
    }
    commands
        .entity(modal_e)
        .insert(CefKeyboardTarget)
        .insert(CefPointerTarget);

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

    // Send open event to command bar webview
    if browsers.has_browser(modal_e) && browsers.host_emit_ready(&modal_e) {
        let payload = CommandBarOpenEvent {
            url: current_url,
            tabs: bar_tabs,
            commands: bar_commands,
            new_tab: is_new_tab,
        };
        let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
        commands.trigger(HostEmitEvent::new(
            modal_e,
            COMMAND_BAR_OPEN_EVENT,
            &ron_body,
        ));
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
    let evt = &trigger.event().payload;
    let empty_tab = new_tab_ctx.tab;
    let previous_tab = new_tab_ctx.previous_tab;
    let mut custom_keyboard_restore = false;

    let current_tab = || {
        focused_tab(
            &spaces,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &tab_ts,
        )
    };

    let current_tab_has_browser = |tab: Entity| -> Option<Entity> {
        let children = all_children.get(tab).ok()?;
        children.iter().find(|&e| content_browsers.contains(e))
    };

    let current_tab_has_terminal = |tab: Entity| -> bool {
        all_children
            .get(tab)
            .ok()
            .map(|children| {
                let has_children = !children.is_empty();
                let has_browser = children.iter().any(|e| content_browsers.contains(e));
                has_children && !has_browser
            })
            .unwrap_or(false)
    };

    match evt.action.as_str() {
        "navigate" => {
            let looks_like_path = looks_like_explicit_path(&evt.value);
            let expanded = if evt.value.starts_with('~') {
                std::env::var("HOME")
                    .ok()
                    .map(|h| {
                        std::path::PathBuf::from(h).join(evt.value[1..].trim_start_matches('/'))
                    })
                    .unwrap_or_else(|| std::path::PathBuf::from(&evt.value))
            } else {
                std::path::PathBuf::from(&evt.value)
            };
            let is_path = looks_like_path && expanded.exists();

            if is_path {
                let dir = if expanded.is_dir() {
                    &expanded
                } else {
                    expanded.parent().unwrap_or(&expanded)
                };
                if evt.new_tab {
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
                    let (_, active_pane, active_tab) = current_tab();
                    if let Some(tab) = active_tab {
                        if let Some(browser_e) = current_tab_has_browser(tab) {
                            commands.entity(browser_e).despawn();
                            commands.entity(tab).insert(PageMetadata {
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
                                    ChildOf(tab),
                                ))
                                .id();
                            commands.entity(term_e).insert(CefKeyboardTarget);
                            custom_keyboard_restore = true;
                        } else if current_tab_has_terminal(tab)
                            && let Some(pane_e) = active_pane
                        {
                            let new_tab_e = commands
                                .spawn((
                                    crate::layout::tab::tab_bundle(),
                                    LastActivatedAt::now(),
                                    ChildOf(pane_e),
                                ))
                                .id();
                            commands.entity(new_tab_e).insert(PageMetadata {
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
                                    ChildOf(new_tab_e),
                                ))
                                .id();
                            commands.entity(term_e).insert(CefKeyboardTarget);
                            custom_keyboard_restore = true;
                        }
                    } else if let Some(pane_e) = active_pane {
                        let new_tab_e = commands
                            .spawn((
                                crate::layout::tab::tab_bundle(),
                                LastActivatedAt::now(),
                                ChildOf(pane_e),
                            ))
                            .id();
                        commands.entity(new_tab_e).insert(PageMetadata {
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
                                ChildOf(new_tab_e),
                            ))
                            .id();
                        commands.entity(term_e).insert(CefKeyboardTarget);
                        custom_keyboard_restore = true;
                    }
                }
            } else {
                let url = if evt.value.contains("://") {
                    evt.value.clone()
                } else if evt.value.contains('.') && !evt.value.contains(' ') {
                    format!("https://{}", evt.value)
                } else {
                    format!("https://www.google.com/search?q={}", evt.value)
                };

                if evt.new_tab {
                    if let Some(tab_e) = empty_tab {
                        if url.starts_with("vmux://terminal") {
                            commands.entity(tab_e).insert(PageMetadata {
                                url: TERMINAL_WEBVIEW_URL.to_string(),
                                title: "Terminal (Session: -)".to_string(),
                                ..default()
                            });
                            let term_e = commands
                                .spawn((
                                    Terminal::new(&mut meshes, &mut webview_mt, &settings),
                                    ChildOf(tab_e),
                                ))
                                .id();
                            commands.entity(term_e).insert(CefKeyboardTarget);
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
                    }
                } else {
                    let (_, active_pane, active_tab) = current_tab();
                    if let Some(tab) = active_tab {
                        if url.starts_with("vmux://terminal") {
                            if let Some(browser_e) = current_tab_has_browser(tab) {
                                commands.entity(browser_e).despawn();
                                commands.entity(tab).insert(PageMetadata {
                                    url: TERMINAL_WEBVIEW_URL.to_string(),
                                    title: "Terminal (Session: -)".to_string(),
                                    ..default()
                                });
                                let term_e = commands
                                    .spawn((
                                        Terminal::new(&mut meshes, &mut webview_mt, &settings),
                                        ChildOf(tab),
                                    ))
                                    .id();
                                commands.entity(term_e).insert(CefKeyboardTarget);
                                custom_keyboard_restore = true;
                            } else if current_tab_has_terminal(tab)
                                && let Some(pane_e) = active_pane
                            {
                                let new_tab_e = commands
                                    .spawn((
                                        crate::layout::tab::tab_bundle(),
                                        LastActivatedAt::now(),
                                        ChildOf(pane_e),
                                    ))
                                    .id();
                                commands.entity(new_tab_e).insert(PageMetadata {
                                    url: TERMINAL_WEBVIEW_URL.to_string(),
                                    title: "Terminal (Session: -)".to_string(),
                                    ..default()
                                });
                                let term_e = commands
                                    .spawn((
                                        Terminal::new(&mut meshes, &mut webview_mt, &settings),
                                        ChildOf(new_tab_e),
                                    ))
                                    .id();
                                commands.entity(term_e).insert(CefKeyboardTarget);
                                custom_keyboard_restore = true;
                            }
                        } else if let Some(browser_e) = current_tab_has_browser(tab) {
                            commands.entity(browser_e).insert(WebviewSource::new(&url));
                        } else if current_tab_has_terminal(tab)
                            && let Some(pane_e) = active_pane
                        {
                            let new_tab_e = commands
                                .spawn((
                                    crate::layout::tab::tab_bundle(),
                                    LastActivatedAt::now(),
                                    ChildOf(pane_e),
                                ))
                                .id();
                            commands.entity(new_tab_e).insert(PageMetadata {
                                url: url.clone(),
                                title: url.clone(),
                                ..default()
                            });
                            let browser_e = commands
                                .spawn((
                                    Browser::new(&mut meshes, &mut webview_mt, &url),
                                    ChildOf(new_tab_e),
                                ))
                                .id();
                            commands.entity(browser_e).insert(CefKeyboardTarget);
                            custom_keyboard_restore = true;
                        }
                    } else if let Some(pane_e) = active_pane {
                        let new_tab_e = commands
                            .spawn((
                                crate::layout::tab::tab_bundle(),
                                LastActivatedAt::now(),
                                ChildOf(pane_e),
                            ))
                            .id();
                        if url.starts_with("vmux://terminal") {
                            commands.entity(new_tab_e).insert(PageMetadata {
                                url: TERMINAL_WEBVIEW_URL.to_string(),
                                title: "Terminal (Session: -)".to_string(),
                                ..default()
                            });
                            let term_e = commands
                                .spawn((
                                    Terminal::new(&mut meshes, &mut webview_mt, &settings),
                                    ChildOf(new_tab_e),
                                ))
                                .id();
                            commands.entity(term_e).insert(CefKeyboardTarget);
                        } else {
                            commands.entity(new_tab_e).insert(PageMetadata {
                                url: url.clone(),
                                title: url.clone(),
                                ..default()
                            });
                            let browser_e = commands
                                .spawn((
                                    Browser::new(&mut meshes, &mut webview_mt, &url),
                                    ChildOf(new_tab_e),
                                ))
                                .id();
                            commands.entity(browser_e).insert(CefKeyboardTarget);
                        }
                        custom_keyboard_restore = true;
                    }
                }
            }
        }
        "terminal" => {
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
            if evt.new_tab {
                if let Some(tab_e) = empty_tab {
                    commands.entity(tab_e).insert(PageMetadata {
                        url: TERMINAL_WEBVIEW_URL.to_string(),
                        title: "Terminal (Session: -)".to_string(),
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
                }
            } else {
                let (_, active_pane, active_tab) = current_tab();
                if let Some(tab) = active_tab {
                    if let Some(browser_e) = current_tab_has_browser(tab) {
                        commands.entity(browser_e).despawn();
                        commands.entity(tab).insert(PageMetadata {
                            url: TERMINAL_WEBVIEW_URL.to_string(),
                            title: "Terminal (Session: -)".to_string(),
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
                                ChildOf(tab),
                            ))
                            .id();
                        commands.entity(term_e).insert(CefKeyboardTarget);
                        custom_keyboard_restore = true;
                    } else if current_tab_has_terminal(tab)
                        && let Some(pane_e) = active_pane
                    {
                        let new_tab_e = commands
                            .spawn((
                                crate::layout::tab::tab_bundle(),
                                LastActivatedAt::now(),
                                ChildOf(pane_e),
                            ))
                            .id();
                        commands.entity(new_tab_e).insert(PageMetadata {
                            url: TERMINAL_WEBVIEW_URL.to_string(),
                            title: "Terminal (Session: -)".to_string(),
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
                                ChildOf(new_tab_e),
                            ))
                            .id();
                        commands.entity(term_e).insert(CefKeyboardTarget);
                        custom_keyboard_restore = true;
                    }
                } else if let Some(pane_e) = active_pane {
                    let new_tab_e = commands
                        .spawn((
                            crate::layout::tab::tab_bundle(),
                            LastActivatedAt::now(),
                            ChildOf(pane_e),
                        ))
                        .id();
                    commands.entity(new_tab_e).insert(PageMetadata {
                        url: TERMINAL_WEBVIEW_URL.to_string(),
                        title: "Terminal (Session: -)".to_string(),
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
                            ChildOf(new_tab_e),
                        ))
                        .id();
                    commands.entity(term_e).insert(CefKeyboardTarget);
                    custom_keyboard_restore = true;
                }
            }
        }
        "command" => {
            if let Some(cmd) = match_command(&evt.value) {
                writer.write(cmd);
            }
            if let Some(tab_e) = empty_tab {
                commands.entity(tab_e).despawn();
                new_tab_ctx.tab = None;
                new_tab_ctx.previous_tab = None;
            }
        }
        "switch_tab" => {
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
            if let Some(tab_e) = empty_tab {
                commands.entity(tab_e).despawn();
                new_tab_ctx.tab = None;
                if let Some(prev) = previous_tab
                    && let Ok(children) = all_children.get(prev)
                {
                    for child in children.iter() {
                        if content_browsers.contains(child) {
                            commands.entity(child).insert(CefKeyboardTarget);
                        }
                    }
                }
                new_tab_ctx.previous_tab = None;
                custom_keyboard_restore = true;
            }
        }
    }

    if let Ok((modal_e, mut modal_node, mut modal_vis)) = modal_q.single_mut() {
        modal_node.display = Display::None;
        *modal_vis = Visibility::Hidden;
        commands
            .entity(modal_e)
            .remove::<CefKeyboardTarget>()
            .remove::<CefPointerTarget>()
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
            .remove::<CefKeyboardTarget>()
            .remove::<CefPointerTarget>()
            .remove::<PendingCommandBarReveal>();
    }
}

/// Waits 2 frames after `Display::Flex` before revealing the command bar so that
/// Bevy UI layout + CEF resize can run while the webview is still invisible.
fn reveal_command_bar(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Visibility, &mut PendingCommandBarReveal), With<Modal>>,
) {
    for (entity, mut vis, mut pending) in &mut query {
        if pending.0 >= 2 {
            *vis = Visibility::Inherited;
            commands.entity(entity).remove::<PendingCommandBarReveal>();
        } else {
            pending.0 += 1;
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
    let query_is_dir = {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let resolved = if let Some(stripped) = query.strip_prefix("~/") {
            std::path::PathBuf::from(&home).join(stripped)
        } else if query.starts_with('/') {
            std::path::PathBuf::from(query)
        } else {
            std::path::PathBuf::from(&home).join(query)
        };
        resolved.is_dir()
    };
    let payload = PathCompleteResponse {
        completions,
        query_is_dir,
    };
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
