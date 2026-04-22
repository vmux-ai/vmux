use crate::{
    browser::Browser,
    command::{AppCommand, BrowserCommand, ReadAppCommands, TabCommand},
    layout::{
        pane::{Pane, PaneSplit},
        space::Space,
        side_sheet::SideSheet,
        tab::{Tab, active_among, collect_leaf_panes, focused_tab},
        window::Modal,
    },
};
use bevy::{ecs::message::MessageReader, ecs::relationship::Relationship, prelude::*, ui::UiSystems};
use bevy_cef::prelude::*;
use vmux_command_bar::event::{
    CommandBarActionEvent, CommandBarCommandEntry, CommandBarOpenEvent, CommandBarTab,
    COMMAND_BAR_OPEN_EVENT,
};
use vmux_header::{Header, PageMetadata};
use vmux_history::LastActivatedAt;

/// Deferred visibility for the command bar modal. Counts frames after Display::Flex
/// so CEF can resize the webview before the modal becomes visible.
#[derive(Component)]
struct PendingCommandBarReveal(u8);

pub(crate) struct CommandBarPlugin;

impl Plugin for CommandBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsEmitEventPlugin::<CommandBarActionEvent>::default())
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
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Browser(BrowserCommand::FocusAddressBar) = *cmd else {
            continue;
        };

        let Ok((modal_e, mut modal_node, mut modal_vis)) = modal_q.single_mut() else {
            continue;
        };

        // Toggle: if already open, close it
        if modal_node.display != Display::None {
            modal_node.display = Display::None;
            *modal_vis = Visibility::Hidden;
            commands
                .entity(modal_e)
                .remove::<CefKeyboardTarget>()
                .remove::<CefPointerTarget>()
                .remove::<PendingCommandBarReveal>();
            // Restore keyboard to active content browser
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
            continue;
        }

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

        // Gather current URL
        let (_, _, active_tab) =
            focused_tab(&spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts);
        let current_url = active_tab
            .and_then(|tab| {
                let Ok(children) = all_children.get(tab) else {
                    return None;
                };
                children.iter().find_map(|e| browser_meta.get(e).ok())
            })
            .map(|meta| meta.url.clone())
            .unwrap_or_default();

        // Gather all tabs
        let active_space = active_among(spaces.iter());
        let mut bar_tabs = Vec::new();
        if let Some(space) = active_space {
            let mut space_panes = Vec::new();
            collect_leaf_panes(space, &all_children, &leaf_panes, &mut space_panes);
            let active_pane = active_tab.and_then(|t| {
                child_of_q.get(t).ok().map(|co| co.get())
            });
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
            };
            let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
            commands.trigger(HostEmitEvent::new(modal_e, COMMAND_BAR_OPEN_EVENT, &ron_body));
        }
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
    mut writer: MessageWriter<AppCommand>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;

    match evt.action.as_str() {
        "navigate" => {
            let url = if evt.value.contains("://") {
                evt.value.clone()
            } else if evt.value.contains('.') && !evt.value.contains(' ') {
                format!("https://{}", evt.value)
            } else {
                format!("https://www.google.com/search?q={}", evt.value)
            };

            // vmux://terminal/ requires a full Terminal entity with PTY
            if url.starts_with("vmux://terminal") {
                writer.write(AppCommand::Tab(TabCommand::NewTerminal));
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
        "command" => {
            if let Some(cmd) = match_command(&evt.value) {
                writer.write(cmd);
            }
        }
        "switch_tab" => {
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
        _ => {} // "dismiss" and unknown
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
