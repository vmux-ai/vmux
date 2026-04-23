use crate::{
    browser::Browser,
    command::{AppCommand, BrowserCommand, ReadAppCommands, TerminalCommand},
    layout::{
        pane::{Pane, PaneSplit},
        space::Space,
        side_sheet::SideSheet,
        tab::{Tab, active_among, collect_leaf_panes, focused_tab},
        window::Modal,
    },
    settings::AppSettings,
    terminal::Terminal,
};
use bevy::{ecs::message::MessageReader, ecs::relationship::Relationship, prelude::*, ui::UiSystems};
use bevy_cef::prelude::*;
use vmux_command_bar::event::{
    CommandBarActionEvent, CommandBarCommandEntry, CommandBarOpenEvent, CommandBarTab,
    COMMAND_BAR_OPEN_EVENT,
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
    /// The tab that was active before the empty tab was created.
    /// Used to restore keyboard focus on dismiss.
    pub previous_tab: Option<Entity>,
    /// When true, `handle_open_command_bar` should open the command bar
    /// in new-tab mode on the next frame.
    pub needs_open: bool,
}

pub(crate) struct CommandBarInputPlugin;

impl Plugin for CommandBarInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NewTabContext>()
            .add_plugins(JsEmitEventPlugin::<CommandBarActionEvent>::default())
            .add_observer(on_command_bar_action)
            .add_systems(Update, handle_open_command_bar.in_set(ReadAppCommands))
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

    for cmd in reader.read() {
        if matches!(*cmd, AppCommand::Browser(BrowserCommand::FocusAddressBar)) {
            should_toggle = true;
        }
    }

    if new_tab_ctx.needs_open {
        should_open = true;
        new_tab_ctx.needs_open = false;
    }

    if should_toggle {
        let is_open = modal_q
            .single()
            .map(|(_, n, _)| n.display != Display::None)
            .unwrap_or(false);
        if is_open {
            // Close command bar (toggle off)
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
            let (_, _, active_tab) =
                focused_tab(&spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts);
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
            return;
        } else {
            should_open = true;
        }
    }

    if !should_open {
        return;
    }

    let Ok((modal_e, mut modal_node, _)) = modal_q.single_mut() else {
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
    let current_url = if is_new_tab {
        String::new()
    } else {
        let (_, _, active_tab) =
            focused_tab(&spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts);
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
        let (_, _, active_tab) =
            focused_tab(&spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts);
        let active_pane = active_tab.and_then(|t| {
            child_of_q.get(t).ok().map(|co| co.get())
        });
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
        commands.trigger(HostEmitEvent::new(modal_e, COMMAND_BAR_OPEN_EVENT, &ron_body));
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
    // Track whether we handle keyboard restore ourselves
    let mut custom_keyboard_restore = false;

    match evt.action.as_str() {
        "navigate" => {
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
                commands.entity(tab_e).remove::<BackgroundColor>();
                new_tab_ctx.tab = None;
                new_tab_ctx.previous_tab = None;
                custom_keyboard_restore = true;
            } else {
                // Normal mode: navigate or spawn terminal in current tab
                if url.starts_with("vmux://terminal") {
                    writer.write(AppCommand::Terminal(TerminalCommand::New));
                } else {
                    let (_, _, active_tab) =
                        focused_tab(&spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts);
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
        "terminal" => {
            // New action: spawn terminal in the empty tab
            if let Some(tab_e) = empty_tab {
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
                commands.entity(tab_e).remove::<BackgroundColor>();
                new_tab_ctx.tab = None;
                new_tab_ctx.previous_tab = None;
                custom_keyboard_restore = true;
            } else {
                // Fallback: create a new terminal tab via existing flow
                writer.write(AppCommand::Terminal(TerminalCommand::New));
            }
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
        "switch_tab" => {
            // Despawn empty tab if in new-tab mode
            if let Some(tab_e) = empty_tab {
                commands.entity(tab_e).despawn();
                new_tab_ctx.tab = None;
                new_tab_ctx.previous_tab = None;
            }
            if let Some((pane_bits, tab_idx)) = evt.value.split_once(':') {
                if let (Ok(pane_id), Ok(tab_index)) =
                    (pane_bits.parse::<u64>(), tab_idx.parse::<usize>())
                {
                    if let Some(target_pane) =
                        leaf_panes.iter().find(|e| e.to_bits() == pane_id)
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
            }
        }
        _ => {
            // "dismiss" and unknown actions
            if let Some(tab_e) = empty_tab {
                commands.entity(tab_e).despawn();
                new_tab_ctx.tab = None;
                // Restore keyboard to previous tab's browser
                if let Some(prev) = previous_tab {
                    if let Ok(children) = all_children.get(prev) {
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
            .remove::<CefKeyboardTarget>()
            .remove::<CefPointerTarget>()
            .remove::<PendingCommandBarReveal>();
    }
    if !custom_keyboard_restore {
        let (_, _, active_tab) =
            focused_tab(&spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts);
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
