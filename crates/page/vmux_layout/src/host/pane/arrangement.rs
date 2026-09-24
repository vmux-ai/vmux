use bevy::{ecs::relationship::Relationship, prelude::*};
use vmux_history::LastActivatedAt;

use crate::host::swap::{find_kind_index, resolve_next, resolve_prev, swap_siblings};

use super::{ArrangeRequest, ArrangementSet, Pane, PaneArrangement, PaneSplit, PaneSplitDirection};
use crate::{
    host::command::LayoutRequestSet,
    stack::{ActiveTabParam, Stack, focused_stack},
    target::SiblingDirection,
};

pub(super) struct ArrangementPlugin;

impl Plugin for ArrangementPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ArrangeRequest>().add_systems(
            Update,
            arrange_from_commands
                .in_set(LayoutRequestSet::Handle)
                .in_set(ArrangementSet),
        );
    }
}

fn arrange_from_commands(
    mut reader: MessageReader<ArrangeRequest>,
    active_tab: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_activity: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_activity: Query<(Entity, &LastActivatedAt), With<Stack>>,
    parents: Query<&ChildOf>,
    splits: Query<&PaneSplit>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let arrangement = request.0;
        let tab = active_tab.get();
        let (_, Some(active), _) = focused_stack(
            tab,
            &all_children,
            &leaf_panes,
            &pane_activity,
            &pane_children,
            &stack_activity,
        ) else {
            continue;
        };

        match arrangement {
            PaneArrangement::Swap(direction) => {
                let Ok(parent) = parents.get(active).map(Relationship::get) else {
                    continue;
                };
                if !splits.contains(parent) {
                    continue;
                }
                let Ok(children) = all_children.get(parent) else {
                    continue;
                };
                let pane_positions: Vec<usize> = children
                    .iter()
                    .enumerate()
                    .filter(|(_, entity)| leaf_panes.contains(*entity) || splits.contains(*entity))
                    .map(|(index, _)| index)
                    .collect();
                let Some(active_index) = find_kind_index(active, children, &pane_positions) else {
                    continue;
                };
                let pair = if direction == SiblingDirection::Previous {
                    resolve_prev(active_index)
                } else {
                    resolve_next(active_index, pane_positions.len())
                };
                if let Some((from, to)) = pair {
                    swap_siblings(&mut commands, parent, children, &pane_positions, from, to);
                }
            }
            PaneArrangement::Rotate(direction) => {
                let Some(tab) = tab else {
                    continue;
                };
                PaneArrangement::rotate(
                    tab,
                    direction == SiblingDirection::Next,
                    &all_children,
                    &leaf_panes,
                    &mut commands,
                );
            }
            PaneArrangement::Mirror(direction) => {
                let Some(tab) = tab else {
                    continue;
                };
                PaneArrangement::mirror(tab, direction, &all_children, &splits, &mut commands);
            }
        }
    }
}

impl PaneArrangement {
    fn rotate(
        tab: Entity,
        forward: bool,
        children: &Query<&Children>,
        leaves: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
        commands: &mut Commands,
    ) {
        let mut panes = Vec::new();
        Self::collect_leaves(tab, children, leaves, &mut panes);
        if panes.len() <= 1 {
            return;
        }
        let groups = panes
            .iter()
            .map(|pane| {
                children
                    .get(*pane)
                    .map(|children| children.iter().collect::<Vec<_>>())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        for group in &groups {
            for child in group {
                commands.entity(*child).remove::<ChildOf>();
            }
        }
        for (index, group) in groups.into_iter().enumerate() {
            let destination = if forward {
                (index + 1) % panes.len()
            } else {
                (index + panes.len() - 1) % panes.len()
            };
            for child in group {
                commands.entity(child).insert(ChildOf(panes[destination]));
            }
        }
    }

    fn mirror(
        root: Entity,
        direction: Option<PaneSplitDirection>,
        children: &Query<&Children>,
        splits: &Query<&PaneSplit>,
        commands: &mut Commands,
    ) {
        let Ok(descendants) = children.get(root) else {
            return;
        };
        let descendants = descendants.iter().collect::<Vec<_>>();
        for child in descendants {
            let Ok(split) = splits.get(child) else {
                continue;
            };
            if direction.is_none_or(|direction| split.direction == direction)
                && let Ok(split_children) = children.get(child)
            {
                let mut reversed = split_children.iter().collect::<Vec<_>>();
                reversed.reverse();
                for entity in &reversed {
                    commands.entity(*entity).remove::<ChildOf>();
                }
                for entity in &reversed {
                    commands.entity(*entity).insert(ChildOf(child));
                }
            }
            Self::mirror(child, direction, children, splits, commands);
        }
    }

    fn collect_leaves(
        root: Entity,
        children: &Query<&Children>,
        leaves: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
        output: &mut Vec<Entity>,
    ) {
        if leaves.contains(root) {
            output.push(root);
            return;
        }
        let Ok(descendants) = children.get(root) else {
            return;
        };
        for child in descendants.iter() {
            Self::collect_leaves(child, children, leaves, output);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn rotate_moves_each_stack_to_the_next_leaf() {
        let mut app = App::new();
        let tab = app.world_mut().spawn_empty().id();
        let left = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let middle = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let right = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let a = app.world_mut().spawn(ChildOf(left)).id();
        let b = app.world_mut().spawn(ChildOf(middle)).id();
        let c = app.world_mut().spawn(ChildOf(right)).id();

        app.world_mut()
            .run_system_once(
                move |children: Query<&Children>,
                      leaves: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
                      mut commands: Commands| {
                    PaneArrangement::rotate(tab, true, &children, &leaves, &mut commands);
                },
            )
            .unwrap();

        assert_eq!(app.world().get::<ChildOf>(a).unwrap().parent(), middle);
        assert_eq!(app.world().get::<ChildOf>(b).unwrap().parent(), right);
        assert_eq!(app.world().get::<ChildOf>(c).unwrap().parent(), left);
    }

    #[test]
    fn horizontal_mirror_reverses_only_row_splits() {
        let mut app = App::new();
        let tab = app.world_mut().spawn_empty().id();
        let row = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let left = app.world_mut().spawn((Pane, ChildOf(row))).id();
        let right = app.world_mut().spawn((Pane, ChildOf(row))).id();

        app.world_mut()
            .run_system_once(
                move |children: Query<&Children>,
                      splits: Query<&PaneSplit>,
                      mut commands: Commands| {
                    PaneArrangement::mirror(
                        tab,
                        Some(PaneSplitDirection::Row),
                        &children,
                        &splits,
                        &mut commands,
                    );
                },
            )
            .unwrap();

        assert_eq!(
            app.world()
                .get::<Children>(row)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            [right, left]
        );
    }

    #[test]
    fn mirror_reverses_every_split_axis() {
        let mut app = App::new();
        let tab = app.world_mut().spawn_empty().id();
        let row = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let left = app.world_mut().spawn((Pane, ChildOf(row))).id();
        let column = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                ChildOf(row),
            ))
            .id();
        let top = app.world_mut().spawn((Pane, ChildOf(column))).id();
        let bottom = app.world_mut().spawn((Pane, ChildOf(column))).id();

        app.world_mut()
            .run_system_once(
                move |children: Query<&Children>,
                      splits: Query<&PaneSplit>,
                      mut commands: Commands| {
                    PaneArrangement::mirror(tab, None, &children, &splits, &mut commands);
                },
            )
            .unwrap();

        assert_eq!(
            app.world()
                .get::<Children>(row)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            [column, left]
        );
        assert_eq!(
            app.world()
                .get::<Children>(column)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            [bottom, top]
        );
    }
}
