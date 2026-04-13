use crate::{
    browser::{browser_bundle, Browser},
    command::{AppCommand, PaneCommand, ReadAppCommands, TabCommand},
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
        .add_observer(on_pane_hover);
    }
}

#[derive(Component)]
pub(crate) struct Pane;

#[derive(Component)]
pub(crate) struct PaneSplit;

#[derive(Component)]
pub(crate) struct Active;

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

fn handle_pane_commands(
    mut reader: MessageReader<AppCommand>,
    active_pane: Query<Entity, With<Active>>,
    pane_children: Query<&Children, With<Pane>>,
    child_of_q: Query<&ChildOf>,
    pane_q: Query<(), With<Pane>>,
    split_q: Query<(), With<PaneSplit>>,
    browser_filter: Query<Entity, With<Browser>>,
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

                let existing_browsers: Vec<Entity> = pane_children
                    .get(active)
                    .map(|c| c.iter().filter(|&e| browser_filter.contains(e)).collect())
                    .unwrap_or_default();

                let pane1 = spawn_leaf_pane(&mut commands, active);
                let pane2 = spawn_leaf_pane(&mut commands, active);

                for browser in existing_browsers {
                    commands.entity(browser).insert(ChildOf(pane1));
                }

                let startup_url = settings.browser.startup_url.as_str();
                let browser = browser_bundle(&mut meshes, &mut webview_mt, startup_url);
                commands.spawn((browser, ChildOf(pane2)));

                commands
                    .entity(active)
                    .insert(PaneSplit)
                    .remove::<Active>();
                let gap = Val::Px(settings.layout.pane.gap);
                commands
                    .entity(active)
                    .entry::<Node>()
                    .and_modify(move |mut n| {
                        n.flex_direction = direction;
                        n.column_gap = gap;
                        n.row_gap = gap;
                    });

                commands.entity(pane2).insert(Active);
            }
            PaneCommand::Close => {
                let Ok(child_of) = child_of_q.get(active) else {
                    continue;
                };
                let parent = child_of.get();

                if !split_q.contains(parent) {
                    commands.entity(active).despawn();
                    let startup_url = settings.browser.startup_url.as_str();
                    let leaf = spawn_leaf_pane(&mut commands, parent);
                    commands.spawn((
                        browser_bundle(&mut meshes, &mut webview_mt, startup_url),
                        ChildOf(leaf),
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

                let new_active = if split_q.contains(sibling) {
                    first_leaf_descendant(sibling, &pane_children, &leaf_panes)
                } else {
                    sibling
                };

                let sibling_children: Vec<Entity> = pane_children
                    .get(sibling)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();

                for child in sibling_children {
                    commands.entity(child).insert(ChildOf(parent));
                }

                if split_q.contains(sibling) {
                    commands.entity(sibling).remove::<ChildOf>();
                    commands.queue(move |world: &mut World| {
                        world.despawn(sibling);
                    });
                } else {
                    commands.entity(parent).remove::<PaneSplit>();
                    commands.entity(sibling).insert(ChildOf(parent));
                }

                commands.entity(active).despawn();
                commands.entity(new_active).insert(Active);
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
    active_pane: Query<Entity, With<Active>>,
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
        let Ok(current) = active_pane.single() else {
            continue;
        };
        let Some(pos) = panes.iter().position(|&e| e == current) else {
            continue;
        };
        let n = panes.len() as i32;
        let idx = (pos as i32 + delta).rem_euclid(n) as usize;
        let target = panes[idx];
        commands.entity(current).remove::<Active>();
        commands.entity(target).insert(Active);
    }
}

fn on_pane_added(trigger: On<Add, Pane>, mut commands: Commands) {
    commands.entity(trigger.entity).observe(on_pane_hover);
}

fn on_pane_hover(
    trigger: On<Pointer<Over>>,
    pane_q: Query<(), (With<Pane>, Without<PaneSplit>)>,
    active_q: Query<Entity, With<Active>>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    if !pane_q.contains(entity) {
        return;
    }
    if let Ok(current) = active_q.single() {
        if current == entity {
            return;
        }
        commands.entity(current).remove::<Active>();
    }
    commands.entity(entity).insert(Active);
}
