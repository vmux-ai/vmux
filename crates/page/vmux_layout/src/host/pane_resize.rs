use bevy::{ecs::relationship::Relationship, prelude::*};
use moonshine_save::prelude::*;
use vmux_command::{AppCommand, LayoutCommand, PaneCommand, ReadAppCommands};
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use crate::settings::LayoutSettings;

use super::{
    pane::{Pane, PaneSplit, PaneSplitDirection},
    stack::{ActiveTabParam, Stack, focused_stack},
};

const MIN_PANE_PX: f32 = 60.0;
const RESIZE_STEP: f32 = 0.05;

pub(super) struct ResizePlugin;

impl Plugin for ResizePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PaneSize>()
            .add_systems(Update, resize_from_commands.in_set(ReadAppCommands))
            .add_systems(Update, resize_from_pointer)
            .add_systems(PostUpdate, sync_split_gaps);
    }
}

#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct PaneSize {
    pub flex_grow: f32,
}

impl Default for PaneSize {
    fn default() -> Self {
        Self { flex_grow: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneSplitGaps {
    pub column_gap: Val,
    pub row_gap: Val,
}

#[derive(Component)]
pub struct PaneDrag {
    previous: Entity,
    next: Entity,
    start_position: f32,
    previous_grow: f32,
    next_grow: f32,
}

pub fn pane_split_gaps(direction: PaneSplitDirection, gap: f32) -> PaneSplitGaps {
    match direction {
        PaneSplitDirection::Row => PaneSplitGaps {
            column_gap: Val::Px(gap),
            row_gap: Val::Px(0.0),
        },
        PaneSplitDirection::Column => PaneSplitGaps {
            column_gap: Val::Px(0.0),
            row_gap: Val::Px(gap),
        },
    }
}

pub fn apply_pane_split_gaps(split: &PaneSplit, node: &mut Node, gap: f32) {
    let gaps = pane_split_gaps(split.direction, gap);
    node.column_gap = gaps.column_gap;
    node.row_gap = gaps.row_gap;
}

fn resize_from_commands(
    mut reader: MessageReader<AppCommand>,
    active_tab: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_activity: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_activity: Query<(Entity, &LastActivatedAt), With<Stack>>,
    parents: Query<&ChildOf>,
    splits: Query<&PaneSplit>,
    mut sizes: ParamSet<(Query<&mut Node>, Query<&mut PaneSize>, Query<&ComputedNode>)>,
) {
    for command in reader.read() {
        let AppCommand::Layout(LayoutCommand::Pane(command)) = *command else {
            continue;
        };
        if !matches!(
            command,
            PaneCommand::EqualizeSize
                | PaneCommand::ResizeLeft
                | PaneCommand::ResizeRight
                | PaneCommand::ResizeUp
                | PaneCommand::ResizeDown
        ) {
            continue;
        }
        let (_, Some(active), _) = focused_stack(
            active_tab.get(),
            &all_children,
            &leaf_panes,
            &pane_activity,
            &pane_children,
            &stack_activity,
        ) else {
            continue;
        };

        if command == PaneCommand::EqualizeSize {
            equalize_siblings(active, &parents, &splits, &all_children, &mut sizes);
            continue;
        }
        resize_active_pane(
            active,
            command,
            &parents,
            &splits,
            &all_children,
            &mut sizes,
        );
    }
}

fn equalize_siblings(
    active: Entity,
    parents: &Query<&ChildOf>,
    splits: &Query<&PaneSplit>,
    all_children: &Query<&Children>,
    sizes: &mut ParamSet<(Query<&mut Node>, Query<&mut PaneSize>, Query<&ComputedNode>)>,
) {
    let Ok(parent) = parents.get(active).map(Relationship::get) else {
        return;
    };
    if !splits.contains(parent) {
        return;
    }
    let Ok(children) = all_children.get(parent) else {
        return;
    };
    let children: Vec<Entity> = children.iter().collect();
    {
        let mut nodes = sizes.p0();
        for child in &children {
            if let Ok(mut node) = nodes.get_mut(*child) {
                node.flex_grow = 1.0;
            }
        }
    }
    let mut pane_sizes = sizes.p1();
    for child in children {
        if let Ok(mut size) = pane_sizes.get_mut(child) {
            size.flex_grow = 1.0;
        }
    }
}

fn resize_active_pane(
    active: Entity,
    command: PaneCommand,
    parents: &Query<&ChildOf>,
    splits: &Query<&PaneSplit>,
    all_children: &Query<&Children>,
    sizes: &mut ParamSet<(Query<&mut Node>, Query<&mut PaneSize>, Query<&ComputedNode>)>,
) {
    let axis = match command {
        PaneCommand::ResizeLeft | PaneCommand::ResizeRight => PaneSplitDirection::Row,
        PaneCommand::ResizeUp | PaneCommand::ResizeDown => PaneSplitDirection::Column,
        _ => return,
    };
    let grows = matches!(command, PaneCommand::ResizeRight | PaneCommand::ResizeDown);
    let mut child = active;
    let mut parent = None;
    for _ in 0..10 {
        let Ok(candidate) = parents.get(child).map(Relationship::get) else {
            break;
        };
        if splits
            .get(candidate)
            .is_ok_and(|split| split.direction == axis)
        {
            parent = Some(candidate);
            break;
        }
        child = candidate;
    }
    let Some(parent) = parent else {
        return;
    };
    let Ok(siblings) = all_children.get(parent) else {
        return;
    };
    let siblings: Vec<Entity> = siblings.iter().collect();
    let Some(index) = siblings.iter().position(|entity| *entity == child) else {
        return;
    };
    let sibling = if grows {
        let Some(sibling) = siblings.get(index + 1) else {
            return;
        };
        *sibling
    } else {
        let Some(index) = index.checked_sub(1) else {
            return;
        };
        siblings[index]
    };

    let parent_length = {
        let layouts = sizes.p2();
        let size = layouts
            .get(parent)
            .map(|layout| layout.size)
            .unwrap_or(Vec2::ZERO);
        match axis {
            PaneSplitDirection::Row => size.x,
            PaneSplitDirection::Column => size.y,
        }
    };
    let (pane_grow, sibling_grow) = {
        let nodes = sizes.p0();
        (
            nodes.get(child).map_or(1.0, |node| node.flex_grow),
            nodes.get(sibling).map_or(1.0, |node| node.flex_grow),
        )
    };
    let total_grow = pane_grow + sibling_grow;
    let (pane_grow, sibling_grow) = resized_pair(
        pane_grow,
        sibling_grow,
        RESIZE_STEP * total_grow,
        parent_length,
    );

    {
        let mut nodes = sizes.p0();
        if let Ok(mut node) = nodes.get_mut(child) {
            node.flex_grow = pane_grow;
        }
        if let Ok(mut node) = nodes.get_mut(sibling) {
            node.flex_grow = sibling_grow;
        }
    }
    let mut pane_sizes = sizes.p1();
    if let Ok(mut size) = pane_sizes.get_mut(child) {
        size.flex_grow = pane_grow;
    }
    if let Ok(mut size) = pane_sizes.get_mut(sibling) {
        size.flex_grow = sibling_grow;
    }
}

fn resized_pair(pane_grow: f32, sibling_grow: f32, delta: f32, parent_length: f32) -> (f32, f32) {
    let total = pane_grow + sibling_grow;
    let mut pane_grow = pane_grow + delta;
    let mut sibling_grow = sibling_grow - delta;
    let minimum = MIN_PANE_PX / parent_length.max(1.0) * total;
    pane_grow = pane_grow.max(minimum);
    sibling_grow = sibling_grow.max(minimum);
    let adjusted_total = pane_grow + sibling_grow;
    if adjusted_total > 0.0 {
        pane_grow = pane_grow / adjusted_total * total;
        sibling_grow = sibling_grow / adjusted_total * total;
    }
    (pane_grow, sibling_grow)
}

fn resize_from_pointer(
    windows: Query<&Window>,
    focused_window: Res<crate::window::FocusedWindow>,
    splits: Query<(Entity, &PaneSplit, &Children), Without<PaneDrag>>,
    active_drags: Query<(Entity, &PaneDrag, &PaneSplit)>,
    child_layouts: Query<&ComputedNode>,
    parent_layouts: Query<&ComputedNode>,
    mut nodes: Query<&mut Node>,
    mut pane_sizes: Query<&mut PaneSize>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut commands: Commands,
) {
    let Some(window_entity) = focused_window.0 else {
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        return;
    };
    let Some(position) = window.physical_cursor_position() else {
        return;
    };
    let cursor = Vec2::new(position.x, position.y);

    if let Ok((split_entity, drag, split)) = active_drags.single() {
        if mouse.pressed(MouseButton::Left) {
            let position = match split.direction {
                PaneSplitDirection::Row => cursor.x,
                PaneSplitDirection::Column => cursor.y,
            };
            let parent_size = parent_layouts
                .get(split_entity)
                .map(|layout| layout.size)
                .unwrap_or(Vec2::ONE);
            let parent_length = match split.direction {
                PaneSplitDirection::Row => parent_size.x,
                PaneSplitDirection::Column => parent_size.y,
            }
            .max(1.0);
            let delta = (position - drag.start_position) / parent_length
                * (drag.previous_grow + drag.next_grow);
            let (previous_grow, next_grow) =
                resized_pair(drag.previous_grow, drag.next_grow, delta, parent_length);

            if let Ok(mut node) = nodes.get_mut(drag.previous) {
                node.flex_grow = previous_grow;
            }
            if let Ok(mut node) = nodes.get_mut(drag.next) {
                node.flex_grow = next_grow;
            }
            if let Ok(mut size) = pane_sizes.get_mut(drag.previous) {
                size.flex_grow = previous_grow;
            }
            if let Ok(mut size) = pane_sizes.get_mut(drag.next) {
                size.flex_grow = next_grow;
            }
        } else {
            commands.entity(split_entity).remove::<PaneDrag>();
        }
        return;
    }

    'splits: for (split_entity, split, children) in &splits {
        let siblings: Vec<Entity> = children.iter().collect();
        for index in 0..siblings.len().saturating_sub(1) {
            let Ok(&previous) = child_layouts.get(siblings[index]) else {
                continue;
            };
            let Ok(&next) = child_layouts.get(siblings[index + 1]) else {
                continue;
            };
            let (gap_min, gap_max, cross_min, cross_max) = match split.direction {
                PaneSplitDirection::Row => (
                    previous.max().x,
                    next.min().x,
                    previous.min().y.min(next.min().y),
                    previous.max().y.max(next.max().y),
                ),
                PaneSplitDirection::Column => (
                    previous.max().y,
                    next.min().y,
                    previous.min().x.min(next.min().x),
                    previous.max().x.max(next.max().x),
                ),
            };
            let (position, cross) = match split.direction {
                PaneSplitDirection::Row => (cursor.x, cursor.y),
                PaneSplitDirection::Column => (cursor.y, cursor.x),
            };
            if position < gap_min || position > gap_max || cross < cross_min || cross > cross_max {
                continue;
            }
            if mouse.just_pressed(MouseButton::Left) {
                let previous_grow = nodes
                    .get(siblings[index])
                    .map(|node| node.flex_grow)
                    .unwrap_or(1.0);
                let next_grow = nodes
                    .get(siblings[index + 1])
                    .map(|node| node.flex_grow)
                    .unwrap_or(1.0);
                commands.entity(split_entity).insert(PaneDrag {
                    previous: siblings[index],
                    next: siblings[index + 1],
                    start_position: position,
                    previous_grow,
                    next_grow,
                });
            }
            break 'splits;
        }
    }
}

fn sync_split_gaps(
    settings: Res<LayoutSettings>,
    mut splits: Query<(&PaneSplit, &mut Node), With<Pane>>,
) {
    if !settings.is_changed() {
        return;
    }
    for (split, mut node) in &mut splits {
        apply_pane_split_gaps(split, &mut node, crate::event::PANE_GAP_PX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tab::Tab;
    use bevy::ecs::message::Messages;
    use vmux_command::{CommandPlugin, WriteAppCommands};

    struct ResizeFixture {
        app: App,
        left: Entity,
        right: Entity,
    }

    impl ResizeFixture {
        fn new(left_grow: f32, right_grow: f32) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, CommandPlugin))
                .add_systems(Update, resize_from_commands.in_set(WriteAppCommands));
            let tab = app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt::now()))
                .id();
            let split = app
                .world_mut()
                .spawn((
                    Pane,
                    PaneSplit {
                        direction: PaneSplitDirection::Row,
                    },
                    ComputedNode {
                        size: Vec2::new(1000.0, 500.0),
                        ..default()
                    },
                    ChildOf(tab),
                ))
                .id();
            let left = Self::pane(&mut app, split, left_grow, 10);
            let right = Self::pane(&mut app, split, right_grow, 1);
            Self { app, left, right }
        }

        fn pane(app: &mut App, parent: Entity, flex_grow: f32, activated_at: i64) -> Entity {
            let pane = app
                .world_mut()
                .spawn((
                    Pane,
                    Node {
                        flex_grow,
                        ..default()
                    },
                    PaneSize { flex_grow },
                    LastActivatedAt(activated_at),
                    ChildOf(parent),
                ))
                .id();
            app.world_mut().spawn((
                Stack::default(),
                LastActivatedAt(activated_at),
                ChildOf(pane),
            ));
            pane
        }

        fn send(&mut self, command: PaneCommand) {
            self.app
                .world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Layout(LayoutCommand::Pane(command)));
            self.app.update();
        }

        fn grows(&self, pane: Entity) -> (f32, f32) {
            (
                self.app.world().get::<Node>(pane).unwrap().flex_grow,
                self.app.world().get::<PaneSize>(pane).unwrap().flex_grow,
            )
        }
    }

    #[test]
    fn split_gap_only_applies_on_split_axis() {
        let row = pane_split_gaps(PaneSplitDirection::Row, 8.0);
        let column = pane_split_gaps(PaneSplitDirection::Column, 8.0);

        assert_eq!(row.column_gap, Val::Px(8.0));
        assert_eq!(row.row_gap, Val::Px(0.0));
        assert_eq!(column.column_gap, Val::Px(0.0));
        assert_eq!(column.row_gap, Val::Px(8.0));
    }

    #[test]
    fn applying_split_gap_clears_cross_axis_gap() {
        let split = PaneSplit {
            direction: PaneSplitDirection::Row,
        };
        let mut node = Node {
            column_gap: Val::Px(16.0),
            row_gap: Val::Px(16.0),
            ..default()
        };

        apply_pane_split_gaps(&split, &mut node, 8.0);

        assert_eq!(node.column_gap, Val::Px(8.0));
        assert_eq!(node.row_gap, Val::Px(0.0));
    }

    #[test]
    fn resize_command_updates_node_and_persisted_size() {
        let mut fixture = ResizeFixture::new(1.0, 1.0);

        fixture.send(PaneCommand::ResizeRight);

        let left = fixture.grows(fixture.left);
        let right = fixture.grows(fixture.right);
        assert!(left.0 > 1.0);
        assert!(right.0 < 1.0);
        assert_eq!(left.0, left.1);
        assert_eq!(right.0, right.1);
    }

    #[test]
    fn equalize_command_updates_node_and_persisted_size() {
        let mut fixture = ResizeFixture::new(2.0, 1.0);

        fixture.send(PaneCommand::EqualizeSize);

        assert_eq!(fixture.grows(fixture.left), (1.0, 1.0));
        assert_eq!(fixture.grows(fixture.right), (1.0, 1.0));
    }
}
