use crate::{
    LayoutChrome,
    event::{FooterCommandEvent, SPACES_EVENT, SpaceRow, SpacesHostEvent},
};
use crate::{
    NewTabContext,
    pane::{Pane, PaneSplit, PaneSplitDirection, leaf_pane_bundle, pane_split_gaps},
    settings::LayoutSettings,
    swap::{find_kind_index, resolve_next, resolve_prev, swap_siblings},
    tab::tab_bundle,
    window::Main as MainNode,
};
use bevy::{
    ecs::{message::Messages, relationship::Relationship},
    prelude::*,
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_command::{AppCommand, ReadAppCommands, SpaceCommand};
use vmux_history::{CreatedAt, LastActivatedAt};
use vmux_webview_app::UiReady;

pub struct SpacePlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpaceCommandSet;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Space>()
            .add_plugins(JsEmitEventPlugin::<FooterCommandEvent>::default())
            .add_observer(on_footer_command_emit)
            .add_systems(
                Update,
                handle_space_commands
                    .in_set(ReadAppCommands)
                    .in_set(SpaceCommandSet),
            )
            .add_systems(Update, push_spaces_host_emit)
            .add_systems(PostUpdate, sync_space_visibility);
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::space"]
#[require(Save)]
pub struct Space {
    pub name: String,
}

pub fn space_bundle() -> impl Bundle {
    (
        Space::default(),
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

/// Spawn a new Space (with a leaf pane + empty tab) under `Main` and
/// activate it. The empty tab triggers the command bar via `NewTabContext`.
fn spawn_new_space(
    main: Entity,
    pw: Entity,
    name: String,
    settings: &LayoutSettings,
    new_tab_ctx: &mut NewTabContext,
    commands: &mut Commands,
) -> Entity {
    let space = commands
        .spawn((
            Space { name },
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
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(main),
        ))
        .id();

    let gap = pane_split_gaps(PaneSplitDirection::Row, settings.pane.gap);
    let split_root = commands
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            HostWindow(pw),
            ZIndex(0),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: gap.column_gap,
                row_gap: gap.row_gap,
                ..default()
            },
            ChildOf(space),
        ))
        .id();

    let leaf = commands
        .spawn((
            leaf_pane_bundle(),
            LastActivatedAt::now(),
            ChildOf(split_root),
        ))
        .id();

    let tab = commands
        .spawn((
            tab_bundle(),
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(leaf),
        ))
        .id();

    if let Some(old_tab) = new_tab_ctx.tab.take() {
        commands.entity(old_tab).despawn();
    }
    new_tab_ctx.tab = Some(tab);
    new_tab_ctx.previous_tab = None;
    new_tab_ctx.needs_open = true;
    new_tab_ctx.dismiss_modal = false;

    space
}

#[allow(clippy::too_many_arguments)]
fn handle_space_commands(
    mut reader: MessageReader<AppCommand>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    space_q: Query<Entity, With<Space>>,
    main_q: Query<Entity, With<MainNode>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    settings: Res<LayoutSettings>,
    mut new_tab_ctx: ResMut<NewTabContext>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Space(space_cmd) = *cmd else {
            continue;
        };

        let active_space = spaces.iter().max_by_key(|(_, ts)| ts.0).map(|(e, _)| e);

        match space_cmd {
            SpaceCommand::New => {
                let Ok(main) = main_q.single() else { continue };
                let count = spaces.iter().count();
                let name = format!("Space {}", count + 1);
                spawn_new_space(
                    main,
                    *primary_window,
                    name,
                    &settings,
                    &mut new_tab_ctx,
                    &mut commands,
                );
            }
            SpaceCommand::Close => {
                let Some(active) = active_space else { continue };
                close_space_entity(
                    active,
                    active_space,
                    spaces.iter().count(),
                    &space_q,
                    &main_q,
                    *primary_window,
                    &child_of_q,
                    &all_children,
                    &settings,
                    &mut new_tab_ctx,
                    &mut commands,
                );
            }
            SpaceCommand::Next | SpaceCommand::Previous => {
                let Some(active) = active_space else { continue };
                let siblings = active_space_siblings(active, &child_of_q, &all_children, &space_q);
                if siblings.len() <= 1 {
                    continue;
                }
                let Some(idx) = siblings.iter().position(|e| *e == active) else {
                    continue;
                };
                let target_idx = if space_cmd == SpaceCommand::Next {
                    (idx + 1) % siblings.len()
                } else {
                    (idx + siblings.len() - 1) % siblings.len()
                };
                let target = siblings[target_idx];
                if target != active {
                    commands.entity(target).insert(LastActivatedAt::now());
                }
            }
            SpaceCommand::Rename => {
                // Reserved: command bar prompt for rename will land in a follow-up.
            }
            SpaceCommand::SwapPrev | SpaceCommand::SwapNext => {
                let Some(active) = active_space else { continue };
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
                    .filter(|(_, e)| space_q.contains(*e))
                    .map(|(i, _)| i)
                    .collect();
                let Some(active_idx) = find_kind_index(active, children, &kind_positions) else {
                    continue;
                };
                let pair = if space_cmd == SpaceCommand::SwapPrev {
                    resolve_prev(active_idx)
                } else {
                    resolve_next(active_idx, kind_positions.len())
                };
                if let Some((a, b)) = pair {
                    swap_siblings(&mut commands, parent, children, &kind_positions, a, b);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn close_space_entity(
    target: Entity,
    active_space: Option<Entity>,
    space_count: usize,
    space_q: &Query<Entity, With<Space>>,
    main_q: &Query<Entity, With<MainNode>>,
    primary_window: Entity,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
    settings: &LayoutSettings,
    new_tab_ctx: &mut NewTabContext,
    commands: &mut Commands,
) {
    let siblings = active_space_siblings(target, child_of_q, all_children, space_q);
    if siblings.len() <= 1 {
        let Ok(main) = main_q.single() else { return };
        let name = format!("Space {}", space_count + 1);
        spawn_new_space(main, primary_window, name, settings, new_tab_ctx, commands);
    } else if active_space == Some(target)
        && let Some(next) = pick_after_close(target, &siblings)
    {
        commands.entity(next).insert(LastActivatedAt::now());
    }
    commands.entity(target).despawn();
}

/// Returns sibling Space entities under the same parent in child order.
fn active_space_siblings(
    active: Entity,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
    space_q: &Query<Entity, With<Space>>,
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
        .filter(|e| space_q.contains(*e))
        .collect::<Vec<_>>()
}

/// When closing `active`, return the entity that should become active.
fn pick_after_close(active: Entity, siblings: &[Entity]) -> Option<Entity> {
    if siblings.len() <= 1 {
        return None;
    }
    let idx = siblings.iter().position(|e| *e == active)?;
    let next_idx = if idx + 1 < siblings.len() { idx + 1 } else { 0 };
    let target = siblings[next_idx];
    if target == active { None } else { Some(target) }
}

fn sync_space_visibility(
    mut spaces: Query<(Entity, &LastActivatedAt, &mut Node, &mut Visibility), With<Space>>,
) {
    let active = spaces
        .iter()
        .max_by_key(|(_, ts, _, _)| ts.0)
        .map(|(e, _, _, _)| e);
    for (entity, _, mut node, mut vis) in &mut spaces {
        let is_active = Some(entity) == active;
        let target_display = if is_active {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != target_display {
            node.display = target_display;
        }
        let target_vis = if is_active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target_vis {
            *vis = target_vis;
        }
    }
}

/// Push the current set of spaces (and the active marker) to the footer
/// webview whenever the payload changes.
#[allow(clippy::too_many_arguments)]
fn push_spaces_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    chrome_q: Query<(Entity, Ref<UiReady>), With<LayoutChrome>>,
    spaces: Query<(Entity, &Space, &LastActivatedAt)>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    space_q: Query<Entity, With<Space>>,
    mut last: Local<String>,
) {
    let Ok((chrome_e, ui_ready)) = chrome_q.single() else {
        return;
    };
    if !browsers.has_browser(chrome_e) || !browsers.host_emit_ready(&chrome_e) {
        return;
    }

    let active_space = spaces.iter().max_by_key(|(_, _, ts)| ts.0).map(|t| t.0);

    // Stable sibling order: pick any space, walk its parent's children.
    let ordered = if let Some(any) = spaces.iter().next() {
        active_space_siblings(any.0, &child_of_q, &all_children, &space_q)
    } else {
        Vec::new()
    };

    let rows: Vec<SpaceRow> = ordered
        .iter()
        .filter_map(|e| spaces.get(*e).ok())
        .map(|(entity, space, _)| SpaceRow {
            id: entity.to_bits().to_string(),
            name: if space.name.is_empty() {
                "Space".to_string()
            } else {
                space.name.clone()
            },
            is_active: Some(entity) == active_space,
        })
        .collect();

    let payload = SpacesHostEvent { spaces: rows };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !ui_ready.is_changed() && body == *last {
        return;
    }
    commands.trigger(HostEmitEvent::new(chrome_e, SPACES_EVENT, &body));
    *last = body;
}

fn on_footer_command_emit(
    trigger: On<Receive<FooterCommandEvent>>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    space_q: Query<Entity, With<Space>>,
    main_q: Query<Entity, With<MainNode>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    settings: Res<LayoutSettings>,
    mut new_tab_ctx: ResMut<NewTabContext>,
    mut messages: ResMut<Messages<AppCommand>>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;
    let active_space = spaces.iter().max_by_key(|(_, ts)| ts.0).map(|(e, _)| e);
    match evt.command.as_str() {
        "new" => {
            messages.write(AppCommand::Space(SpaceCommand::New));
        }
        "close" => {
            let target = footer_target_space(
                evt.space_id.as_deref(),
                spaces.iter().map(|(entity, _)| entity),
            )
            .or(active_space);
            let Some(target) = target else { return };
            close_space_entity(
                target,
                active_space,
                spaces.iter().count(),
                &space_q,
                &main_q,
                *primary_window,
                &child_of_q,
                &all_children,
                &settings,
                &mut new_tab_ctx,
                &mut commands,
            );
        }
        "switch" => {
            let Some(id_str) = evt.space_id.as_deref() else {
                return;
            };
            let Ok(bits) = id_str.parse::<u64>() else {
                return;
            };
            let Some((target, _)) = spaces.iter().find(|(e, _)| e.to_bits() == bits) else {
                return;
            };
            commands.entity(target).insert(LastActivatedAt::now());
        }
        _ => {}
    }
}

fn footer_target_space(
    id: Option<&str>,
    spaces: impl IntoIterator<Item = Entity>,
) -> Option<Entity> {
    let bits = id?.parse::<u64>().ok()?;
    spaces.into_iter().find(|space| space.to_bits() == bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_target_space_uses_event_space_id() {
        let target = Entity::from_bits(42);
        let other = Entity::from_bits(7);
        let id = target.to_bits().to_string();

        assert_eq!(
            footer_target_space(Some(&id), [other, target]),
            Some(target)
        );
    }
}
