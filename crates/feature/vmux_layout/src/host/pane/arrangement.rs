use crate::host::swap::SiblingOrder;
use bevy::{
    ecs::{relationship::Relationship, system::SystemParam},
    prelude::*,
};

use super::{ArrangeRequest, ArrangementSet, Pane, PaneArrangement, PaneSplit, PaneSplitDirection};
use crate::{
    host::command::LayoutRequestSet,
    stack::{ActiveTabParam, LayoutFocus},
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
    focus: LayoutFocus,
    mut tree: PaneTree,
) {
    for request in reader.read() {
        let arrangement = request.0;
        let tab = active_tab.get();
        let (_, Some(active), _) = focus.resolve(tab) else {
            continue;
        };

        match arrangement {
            PaneArrangement::Swap(direction) => {
                let Ok(parent) = tree.parents.get(active).map(Relationship::get) else {
                    continue;
                };
                if !tree.splits.contains(parent) {
                    continue;
                }
                let Ok(children) = tree.children.get(parent) else {
                    continue;
                };
                let pane_positions: Vec<usize> = children
                    .iter()
                    .enumerate()
                    .filter(|(_, entity)| {
                        tree.leaves.contains(*entity) || tree.splits.contains(*entity)
                    })
                    .map(|(index, _)| index)
                    .collect();
                let Some(active_index) = SiblingOrder::index(active, children, &pane_positions)
                else {
                    continue;
                };
                let pair = if direction == SiblingDirection::Previous {
                    SiblingOrder::previous(active_index)
                } else {
                    SiblingOrder::next(active_index, pane_positions.len())
                };
                if let Some((from, to)) = pair
                    && let Some(order) =
                        SiblingOrder::swapped(parent, children, &pane_positions, from, to)
                {
                    tree.commands.queue(order);
                }
            }
            PaneArrangement::Rotate(direction) => {
                let Some(tab) = tab else {
                    continue;
                };
                tree.rotate(tab, direction == SiblingDirection::Next);
            }
            PaneArrangement::Mirror(direction) => {
                let Some(tab) = tab else {
                    continue;
                };
                tree.mirror(tab, direction);
            }
        }
    }
}

#[derive(SystemParam)]
struct PaneTree<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    leaves: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    parents: Query<'w, 's, &'static ChildOf>,
    splits: Query<'w, 's, &'static PaneSplit>,
    commands: Commands<'w, 's>,
}

impl PaneTree<'_, '_> {
    fn rotate(&mut self, tab: Entity, forward: bool) {
        let mut panes = Vec::new();
        self.collect_leaves(tab, &mut panes);
        if panes.len() <= 1 {
            return;
        }
        let groups = panes
            .iter()
            .map(|pane| {
                self.children
                    .get(*pane)
                    .map(|children| children.iter().collect::<Vec<_>>())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        for group in &groups {
            for child in group {
                self.commands.entity(*child).remove::<ChildOf>();
            }
        }
        for (index, group) in groups.into_iter().enumerate() {
            let destination = if forward {
                (index + 1) % panes.len()
            } else {
                (index + panes.len() - 1) % panes.len()
            };
            for child in group {
                self.commands
                    .entity(child)
                    .insert(ChildOf(panes[destination]));
            }
        }
    }

    fn mirror(&mut self, root: Entity, direction: Option<PaneSplitDirection>) {
        let Ok(descendants) = self.children.get(root) else {
            return;
        };
        let descendants = descendants.iter().collect::<Vec<_>>();
        for child in descendants {
            let Ok(split) = self.splits.get(child) else {
                continue;
            };
            if direction.is_none_or(|direction| split.direction == direction)
                && let Ok(split_children) = self.children.get(child)
            {
                let mut reversed = split_children.iter().collect::<Vec<_>>();
                reversed.reverse();
                for entity in &reversed {
                    self.commands.entity(*entity).remove::<ChildOf>();
                }
                for entity in &reversed {
                    self.commands.entity(*entity).insert(ChildOf(child));
                }
            }
            self.mirror(child, direction);
        }
    }

    fn collect_leaves(&self, root: Entity, output: &mut Vec<Entity>) {
        if self.leaves.contains(root) {
            output.push(root);
            return;
        }
        let Ok(descendants) = self.children.get(root) else {
            return;
        };
        for child in descendants.iter() {
            self.collect_leaves(child, output);
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
            .run_system_once(move |mut tree: PaneTree| tree.rotate(tab, true))
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
            .run_system_once(move |mut tree: PaneTree| {
                tree.mirror(tab, Some(PaneSplitDirection::Row));
            })
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
            .run_system_once(move |mut tree: PaneTree| tree.mirror(tab, None))
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
