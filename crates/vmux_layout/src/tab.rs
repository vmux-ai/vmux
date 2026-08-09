use crate::event::TabsCommandEvent;
use crate::{
    TabLayoutSpawnContent, TabLayoutSpawnRequest,
    swap::{find_kind_index, resolve_next, resolve_prev, swap_siblings},
};
use bevy::{
    ecs::{message::Messages, relationship::Relationship},
    prelude::*,
    ui::UiSystems,
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use std::time::Instant;
use vmux_command::open::OpenCommand;
use vmux_command::{AppCommand, BrowserCommand, LayoutCommand, ReadAppCommands, TabCommand};
use vmux_core::Order;
use vmux_history::LastActivatedAt;

impl Plugin for TabPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Tab>()
            .register_type::<Option<String>>()
            .register_type::<TabWorkspace>()
            .register_type::<TabWorktree>()
            .register_type::<TabDirDecided>()
            .init_resource::<LastTabCloseAt>()
            .add_message::<CloseTabRequest>()
            .add_message::<crate::NewTabRequest>()
            .add_plugins(BinEventEmitterPlugin::<(TabsCommandEvent,)>::for_hosts(&[
                "layout",
            ]))
            .add_observer(on_tabs_command_emit)
            .add_systems(
                Update,
                handle_tab_commands
                    .in_set(ReadAppCommands)
                    .in_set(TabCommandSet)
                    .after(crate::settings::EffectiveStartupDirSet),
            )
            .add_systems(
                Update,
                crate::archive::handle_close_tab_requests
                    .in_set(ReadAppCommands)
                    .after(TabCommandSet)
                    .after(crate::stack::StackCommandSet),
            )
            .add_systems(PostUpdate, sync_tab_visibility.before(UiSystems::Layout))
            .add_systems(PostUpdate, sync_tab_order);
    }
}

pub struct TabPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TabCommandSet;

#[derive(Message, Clone, Copy)]
pub struct CloseTabRequest {
    pub tab: Entity,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct Tab {
    pub name: String,
    pub startup_dir: Option<String>,
}

/// Stable project directory for a tab. Unlike [`Tab::startup_dir`], this does not change when
/// the tab is rebound to a managed worktree.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct TabWorkspace {
    pub project_dir: String,
}

/// Present iff a tab's `startup_dir` points at a vmux-managed git worktree.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct TabWorktree {
    pub repo_root: String,
    #[reflect(default)]
    pub checkout_dir: String,
    pub branch: String,
    pub base_ref: String,
}

/// Runtime failure state for a persisted managed worktree. Ownership metadata remains attached.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct TabWorktreeUnavailable {
    pub message: String,
}

/// Marks that the worktree/work-here decision has been made for a tab, so the isolate offer
/// never fires again for it.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct TabDirDecided;

/// Walk up from `entity` to its ancestor [`Tab`] and return that tab's `startup_dir` override.
///
/// Everything spawned inside a tab (the ACP agent session and the user's terminals) shares the
/// tab's working directory; this resolves that override for a given stack/pane entity.
pub fn ancestor_tab_startup_dir(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<&Tab>,
) -> Option<String> {
    let mut cur = entity;
    loop {
        if let Ok(tab) = tabs.get(cur) {
            return tab.startup_dir.clone();
        }
        cur = child_of.get(cur).ok()?.parent();
    }
}

#[derive(Resource, Default)]
pub struct LastTabCloseAt(pub Option<Instant>);

pub fn tab_bundle() -> impl Bundle {
    (
        Tab::default(),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::default(),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn handle_tab_commands(
    mut reader: MessageReader<AppCommand>,
    mut new_tabs: MessageReader<crate::NewTabRequest>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    active_tab_param: crate::stack::ActiveTabParam,
    tab_q: Query<Entity, With<Tab>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    effective_startup_dir: Option<Res<crate::settings::EffectiveStartupDir>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    mut close_requests: MessageWriter<CloseTabRequest>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let active_tab = active_tab_param.get();

        match cmd {
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewTab { url })) => {
                let Some((space, startup_dir)) = effective_startup_dir
                    .as_deref()
                    .and_then(|effective| effective.0.clone())
                else {
                    continue;
                };
                let count = tabs.iter().count();
                let name = format!("Tab {}", count + 1);
                let requested = url.as_deref().filter(|url| !url.is_empty()).or_else(|| {
                    effective_startup_url
                        .as_deref()
                        .map(|startup| startup.0.as_str())
                        .filter(|startup| !startup.is_empty())
                });
                let content = match requested {
                    Some(url) => TabLayoutSpawnContent::Url {
                        url: url.to_string(),
                        pending_prompt: None,
                    },
                    None => TabLayoutSpawnContent::StartupUrlOrPrompt,
                };
                layout_requests.write(TabLayoutSpawnRequest {
                    space,
                    primary_window: *primary_window,
                    name: Some(name),
                    startup_dir: startup_dir.clone(),
                    content,
                    clear_pending_stack: true,
                    focus: true,
                });
            }
            AppCommand::Layout(LayoutCommand::Tab(tab_cmd)) => match tab_cmd {
                TabCommand::Close => {
                    let Some(active) = active_tab else { continue };
                    close_requests.write(CloseTabRequest { tab: active });
                }
                TabCommand::New => {
                    let Some((space, startup_dir)) = effective_startup_dir
                        .as_deref()
                        .and_then(|effective| effective.0.clone())
                    else {
                        continue;
                    };
                    let name = format!("Tab {}", tabs.iter().count() + 1);
                    layout_requests.write(TabLayoutSpawnRequest {
                        space,
                        primary_window: *primary_window,
                        name: Some(name),
                        startup_dir: startup_dir.clone(),
                        content: TabLayoutSpawnContent::StartupUrlOrPrompt,
                        clear_pending_stack: true,
                        focus: true,
                    });
                }
                TabCommand::Next | TabCommand::Previous => {
                    let Some(active) = active_tab else { continue };
                    let siblings = active_tab_siblings(active, &child_of_q, &all_children, &tab_q);
                    if siblings.len() <= 1 {
                        continue;
                    }
                    let Some(idx) = siblings.iter().position(|e| *e == active) else {
                        continue;
                    };
                    let target_idx = if *tab_cmd == TabCommand::Next {
                        (idx + 1) % siblings.len()
                    } else {
                        (idx + siblings.len() - 1) % siblings.len()
                    };
                    let target = siblings[target_idx];
                    if target != active {
                        commands.entity(target).insert(LastActivatedAt::now());
                    }
                }
                TabCommand::Rename => {}
                TabCommand::SelectIndex1
                | TabCommand::SelectIndex2
                | TabCommand::SelectIndex3
                | TabCommand::SelectIndex4
                | TabCommand::SelectIndex5
                | TabCommand::SelectIndex6
                | TabCommand::SelectIndex7
                | TabCommand::SelectIndex8
                | TabCommand::SelectLast => {
                    let Some(active) = active_tab else { continue };
                    let siblings = active_tab_siblings(active, &child_of_q, &all_children, &tab_q);
                    if siblings.is_empty() {
                        continue;
                    }
                    let target_idx = match tab_cmd {
                        TabCommand::SelectIndex1 => 0,
                        TabCommand::SelectIndex2 => 1,
                        TabCommand::SelectIndex3 => 2,
                        TabCommand::SelectIndex4 => 3,
                        TabCommand::SelectIndex5 => 4,
                        TabCommand::SelectIndex6 => 5,
                        TabCommand::SelectIndex7 => 6,
                        TabCommand::SelectIndex8 => 7,
                        TabCommand::SelectLast => siblings.len() - 1,
                        _ => continue,
                    };
                    if target_idx >= siblings.len() {
                        continue;
                    }
                    let target = siblings[target_idx];
                    if target != active {
                        commands.entity(target).insert(LastActivatedAt::now());
                    }
                }
                TabCommand::SwapPrev | TabCommand::SwapNext => {
                    let Some(active) = active_tab else { continue };
                    let Ok(co) = child_of_q.get(active) else {
                        continue;
                    };
                    let parent = co.get();
                    let Ok(children) = all_children.get(parent) else {
                        continue;
                    };
                    let kind_positions: Vec<usize> = children
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| tab_q.contains(*e))
                        .map(|(i, _)| i)
                        .collect();
                    let Some(active_idx) = find_kind_index(active, children, &kind_positions)
                    else {
                        continue;
                    };
                    let pair = if *tab_cmd == TabCommand::SwapPrev {
                        resolve_prev(active_idx)
                    } else {
                        resolve_next(active_idx, kind_positions.len())
                    };
                    if let Some((a, b)) = pair {
                        swap_siblings(&mut commands, parent, children, &kind_positions, a, b);
                    }
                }
            },
            _ => continue,
        }
    }

    for request in new_tabs.read() {
        let Some((space, startup_dir)) = effective_startup_dir
            .as_deref()
            .and_then(|effective| effective.0.clone())
        else {
            continue;
        };
        let name = format!("Tab {}", tabs.iter().count() + 1);
        layout_requests.write(TabLayoutSpawnRequest {
            space,
            primary_window: *primary_window,
            name: Some(name),
            startup_dir,
            content: TabLayoutSpawnContent::Url {
                url: request.url.clone(),
                pending_prompt: request.pending_prompt.clone(),
            },
            clear_pending_stack: true,
            focus: true,
        });
    }
}

pub fn active_tab_siblings(
    active: Entity,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
    tab_q: &Query<Entity, With<Tab>>,
) -> Vec<Entity> {
    let Ok(co) = child_of_q.get(active) else {
        return vec![active];
    };
    let parent = co.get();
    let Ok(children) = all_children.get(parent) else {
        return vec![active];
    };
    children
        .iter()
        .filter(|e| tab_q.contains(*e))
        .collect::<Vec<_>>()
}

pub(crate) fn pick_after_close(active: Entity, siblings: &[Entity]) -> Option<Entity> {
    if siblings.len() <= 1 {
        return None;
    }
    let idx = siblings.iter().position(|e| *e == active)?;
    let next_idx = if idx + 1 < siblings.len() {
        idx + 1
    } else {
        idx - 1
    };
    let target = siblings[next_idx];
    if target == active { None } else { Some(target) }
}

fn sync_tab_visibility(
    mut tabs: Query<(&mut Node, &mut Visibility, Has<vmux_core::Active>), With<Tab>>,
) {
    for (mut node, mut vis, active) in &mut tabs {
        let target_display = if active { Display::Flex } else { Display::None };
        if node.display != target_display {
            node.display = target_display;
        }
        let target_vis = if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target_vis {
            *vis = target_vis;
        }
    }
}

fn sync_tab_order(
    spaces: Query<&Children, (With<crate::space::Space>, Changed<Children>)>,
    tab_q: Query<(), With<Tab>>,
    mut order_q: Query<&mut Order>,
    mut commands: Commands,
) {
    for children in &spaces {
        let mut idx = 0u32;
        for child in children.iter() {
            if !tab_q.contains(child) {
                continue;
            }
            match order_q.get_mut(child) {
                Ok(mut order) => {
                    if order.0 != idx {
                        order.0 = idx;
                    }
                }
                Err(_) => {
                    commands.entity(child).insert(Order(idx));
                }
            }
            idx += 1;
        }
    }
}

fn on_tabs_command_emit(
    trigger: On<BinReceive<TabsCommandEvent>>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    active_tab_param: crate::stack::ActiveTabParam,
    mut messages: ResMut<Messages<AppCommand>>,
    mut issued: ResMut<Messages<vmux_command::CommandIssued>>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
    mut close_requests: MessageWriter<CloseTabRequest>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;
    let active_tab = active_tab_param.get();
    let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
    match evt.command.as_str() {
        "new" => {
            let cmd =
                AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewTab { url: None }));
            issued.write(vmux_command::CommandIssued {
                caller,
                command: cmd.clone(),
            });
            messages.write(cmd);
        }
        "close" => {
            let target = tab_target(evt.tab_id.as_deref(), tabs.iter().map(|(entity, _)| entity))
                .or(active_tab);
            let Some(target) = target else { return };
            close_requests.write(CloseTabRequest { tab: target });
        }
        "switch" => {
            let Some(id_str) = evt.tab_id.as_deref() else {
                return;
            };
            let Ok(bits) = id_str.parse::<u64>() else {
                return;
            };
            let Some((target, _)) = tabs.iter().find(|(e, _)| e.to_bits() == bits) else {
                return;
            };
            commands.entity(target).insert(LastActivatedAt::now());
        }
        _ => {}
    }
}

fn tab_target(id: Option<&str>, tabs: impl IntoIterator<Item = Entity>) -> Option<Entity> {
    let bits = id?.parse::<u64>().ok()?;
    tabs.into_iter().find(|e| e.to_bits() == bits)
}

#[cfg(test)]
#[path = "tab.test.rs"]
mod tests;
