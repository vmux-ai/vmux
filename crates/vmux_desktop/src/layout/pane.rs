use crate::{
    browser::browser_bundle,
    command::{AppCommand, PaneCommand, ReadAppCommands, TabCommand},
    layout::tab::{Active, Tab, tab_bundle},
    settings::AppSettings,
};
use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    ui::FlexDirection,
};
use bevy_cef::prelude::*;

pub(crate) struct PanePlugin;

impl Plugin for PanePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (on_pane_cycle, handle_pane_commands).in_set(ReadAppCommands),
        )
        .add_observer(on_pane_added)
        .add_observer(on_pane_click);
    }
}

#[derive(Component)]
pub(crate) struct Pane;

#[derive(Component)]
pub(crate) struct PaneSplit;

pub(crate) fn leaf_pane_bundle() -> impl Bundle {
    (
        Pane,
        Transform::default(),
        GlobalTransform::default(),
        Node {
            flex_grow: 1.0,
            flex_basis: Val::Px(0.0),
            align_items: AlignItems::Stretch,
            justify_content: JustifyContent::Stretch,
            ..default()
        },
    )
}

fn spawn_leaf_pane(commands: &mut Commands, parent: Entity) -> Entity {
    commands.spawn((leaf_pane_bundle(), ChildOf(parent))).id()
}

pub(crate) fn first_leaf_descendant(
    entity: Entity,
    children_q: &Query<&Children, With<Pane>>,
    leaf_q: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Entity {
    if leaf_q.contains(entity) {
        return entity;
    }
    if let Ok(children) = children_q.get(entity) {
        for child in children.iter() {
            if leaf_q.contains(child) {
                return child;
            }
            let found = first_leaf_descendant(child, children_q, leaf_q);
            if found != child || leaf_q.contains(found) {
                return found;
            }
        }
    }
    entity
}

pub(crate) fn first_tab_in_pane(
    pane: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    tab_q: &Query<Entity, With<Tab>>,
) -> Option<Entity> {
    let children = pane_children.get(pane).ok()?;
    children.iter().find(|&e| tab_q.contains(e))
}

pub(crate) fn active_tab_in_pane(
    pane: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    active_tabs: &Query<Entity, (With<Active>, With<Tab>)>,
) -> Option<Entity> {
    let children = pane_children.get(pane).ok()?;
    children.iter().find(|&e| active_tabs.contains(e))
}

fn handle_pane_commands(
    mut reader: MessageReader<AppCommand>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    pane_children: Query<&Children, With<Pane>>,
    child_of_q: Query<&ChildOf>,
    pane_q: Query<(), With<Pane>>,
    split_q: Query<(), With<PaneSplit>>,
    tab_filter: Query<Entity, With<Tab>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for cmd in reader.read() {
        let AppCommand::Pane(pane_cmd) = *cmd else {
            continue;
        };
        let Ok(active) = active_pane.single() else {
            continue;
        };

        match pane_cmd {
            PaneCommand::SplitV | PaneCommand::SplitH => {
                let direction = if pane_cmd == PaneCommand::SplitV {
                    FlexDirection::Row
                } else {
                    FlexDirection::Column
                };

                let existing_tabs: Vec<Entity> = pane_children
                    .get(active)
                    .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                    .unwrap_or_default();

                let pane1 = spawn_leaf_pane(&mut commands, active);
                let pane2 = spawn_leaf_pane(&mut commands, active);

                for tab in existing_tabs {
                    commands.entity(tab).insert(ChildOf(pane1));
                }

                let startup_url = settings.browser.startup_url.as_str();
                let new_tab = commands.spawn((tab_bundle(), Active, ChildOf(pane2))).id();
                commands.spawn((
                    browser_bundle(&mut meshes, &mut webview_mt, startup_url),
                    ChildOf(new_tab),
                ));

                commands.entity(active).insert(PaneSplit).remove::<Active>();
                let gap = Val::Px(settings.layout.pane.gap);
                commands.entity(active).insert(Node {
                    flex_grow: 1.0,
                    flex_direction: direction,
                    column_gap: gap,
                    row_gap: gap,
                    align_items: AlignItems::Stretch,
                    ..default()
                });

                commands.entity(pane2).insert(Active);
            }
            PaneCommand::Close => {
                let Ok(pane_co) = child_of_q.get(active) else {
                    continue;
                };
                let parent = pane_co.get();

                if !split_q.contains(parent) {
                    commands.entity(active).despawn();
                    let startup_url = settings.browser.startup_url.as_str();
                    let leaf = spawn_leaf_pane(&mut commands, parent);
                    let tab = commands.spawn((tab_bundle(), Active, ChildOf(leaf))).id();
                    commands.spawn((
                        browser_bundle(&mut meshes, &mut webview_mt, startup_url),
                        ChildOf(tab),
                    ));
                    commands.entity(leaf).insert(Active);
                    continue;
                }

                let Ok(siblings) = pane_children.get(parent) else {
                    continue;
                };
                let sibling = siblings
                    .iter()
                    .find(|&e| e != active && pane_q.contains(e));
                let Some(sibling) = sibling else {
                    continue;
                };

                let sibling_children: Vec<Entity> = pane_children
                    .get(sibling)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();

                for &child in &sibling_children {
                    commands.entity(child).insert(ChildOf(parent));
                }

                let new_active_pane;
                if split_q.contains(sibling) {
                    new_active_pane = first_leaf_descendant(sibling, &pane_children, &leaf_panes);
                    commands.entity(sibling).remove::<ChildOf>();
                    commands.queue(move |world: &mut World| {
                        world.despawn(sibling);
                    });
                } else {
                    new_active_pane = parent;
                    commands.entity(parent).remove::<PaneSplit>();
                    commands.entity(parent).insert(Node {
                        flex_grow: 1.0,
                        flex_basis: Val::Px(0.0),
                        align_items: AlignItems::Stretch,
                        justify_content: JustifyContent::Stretch,
                        ..default()
                    });
                    commands.entity(sibling).despawn();
                }

                commands.entity(active).despawn();
                commands.entity(new_active_pane).insert(Active);
                let tab = active_tab_in_pane(new_active_pane, &pane_children, &active_tabs)
                    .or_else(|| first_tab_in_pane(new_active_pane, &pane_children, &tab_filter))
                    .or_else(|| sibling_children.iter().copied().find(|&e| tab_filter.contains(e)));
                if let Some(tab) = tab {
                    commands.entity(tab).insert(Active);
                }
            }
            PaneCommand::Toggle => {}
            PaneCommand::Zoom => {}
            PaneCommand::SelectLeft => {}
            PaneCommand::SelectRight => {}
            PaneCommand::SelectUp => {}
            PaneCommand::SelectDown => {}
            PaneCommand::SwapPrev => {}
            PaneCommand::SwapNext => {}
            PaneCommand::RotateForward => {}
            PaneCommand::RotateBackward => {}
        }
    }
}

fn on_pane_cycle(
    mut reader: MessageReader<AppCommand>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let delta: i32 = match cmd {
            AppCommand::Tab(TabCommand::Next) => 1,
            AppCommand::Tab(TabCommand::Previous) => -1,
            _ => continue,
        };
        let mut panes: Vec<Entity> = leaf_panes.iter().collect();
        if panes.len() < 2 {
            continue;
        }
        panes.sort_by_key(|e| e.to_bits());
        let Ok(current_pane) = active_pane.single() else {
            continue;
        };
        let Some(pos) = panes.iter().position(|&e| e == current_pane) else {
            continue;
        };
        let n = panes.len() as i32;
        let idx = (pos as i32 + delta).rem_euclid(n) as usize;
        let target_pane = panes[idx];

        commands.entity(current_pane).remove::<Active>();
        commands.entity(target_pane).insert(Active);
    }
}

fn on_pane_added(trigger: On<Add, Pane>, mut commands: Commands) {
    commands.entity(trigger.entity).observe(on_pane_click);
}

fn on_pane_click(
    trigger: On<Pointer<Click>>,
    pane_q: Query<(), (With<Pane>, Without<PaneSplit>)>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    if !pane_q.contains(entity) {
        return;
    }
    if let Ok(current) = active_pane.single() {
        if current == entity {
            return;
        }
        commands.entity(current).remove::<Active>();
    }
    commands.entity(entity).insert(Active);
}
