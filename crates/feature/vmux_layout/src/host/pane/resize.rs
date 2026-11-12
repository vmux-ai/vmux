pub use crate::{PaneSize, PaneSplitGaps};
use bevy::{ecs::relationship::Relationship, prelude::*};
use moonshine_save::prelude::*;
use vmux_api::open_target::PaneDirection;
use vmux_ecs::store::StoreAppExt;
use vmux_flex::prelude::*;
#[cfg(test)]
use vmux_history::LastActivatedAt;

use crate::settings::LayoutSettings;

use super::{Pane, PaneResize, PaneSplit, PaneSplitDirection, ResizeRequest};
#[cfg(test)]
use crate::stack::Stack;
use crate::{
    host::command::LayoutRequestSet,
    stack::{ActiveTabParam, LayoutFocus},
};

const MIN_PANE_PX: f32 = 60.0;
const RESIZE_STEP: f32 = 0.05;

pub(super) struct ResizePlugin;

impl Plugin for ResizePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ResizeRequest>()
            .register_required_components::<PaneSize, Save>()
            .register_stored::<PaneSize>()
            .add_systems(Update, from_commands.in_set(LayoutRequestSet::Handle))
            .add_systems(Update, from_pointer.after(crate::window::WindowFocusSet))
            .add_systems(PostUpdate, sync_split_gaps);
    }
}

#[derive(Component)]
pub struct PaneDrag {
    previous: Entity,
    next: Entity,
    start_position: f32,
    previous_grow: f32,
    next_grow: f32,
}

fn from_commands(
    mut reader: MessageReader<ResizeRequest>,
    active_tab: ActiveTabParam,
    all_children: Query<&Children>,
    focus: LayoutFocus,
    parents: Query<&ChildOf>,
    splits: Query<&PaneSplit>,
    mut sizes: ParamSet<(Query<&mut Node>, Query<&mut PaneSize>, Query<&ComputedNode>)>,
) {
    for request in reader.read() {
        let resize = request.0;
        let (_, Some(active), _) = focus.resolve(active_tab.get()) else {
            continue;
        };

        match resize {
            PaneResize::Equalize => {
                equalize_siblings(active, &parents, &splits, &all_children, &mut sizes);
            }
            PaneResize::Direction(direction) => {
                resize_active_pane(
                    active,
                    direction,
                    &parents,
                    &splits,
                    &all_children,
                    &mut sizes,
                );
            }
        }
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
    direction: PaneDirection,
    parents: &Query<&ChildOf>,
    splits: &Query<&PaneSplit>,
    all_children: &Query<&Children>,
    sizes: &mut ParamSet<(Query<&mut Node>, Query<&mut PaneSize>, Query<&ComputedNode>)>,
) {
    let axis = match direction {
        PaneDirection::Left | PaneDirection::Right => PaneSplitDirection::Row,
        PaneDirection::Top | PaneDirection::Bottom => PaneSplitDirection::Column,
    };
    let after = matches!(direction, PaneDirection::Right | PaneDirection::Bottom);
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
    let sibling = if after {
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

fn from_pointer(
    windows: Query<&Window>,
    focused_window: crate::window::FocusedWindow,
    hierarchy: crate::window::WindowHierarchy,
    splits: Query<(Entity, &PaneSplit, &Children), Without<PaneDrag>>,
    active_drags: Query<(Entity, &PaneDrag, &PaneSplit)>,
    child_layouts: Query<&ComputedNode>,
    parent_layouts: Query<&ComputedNode>,
    mut nodes: Query<&mut Node>,
    mut pane_sizes: Query<&mut PaneSize>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut was_pressed: Local<bool>,
    mut commands: Commands,
) {
    let pressed = match vmux_input::NativePointer::snapshot() {
        Some(pointer) => pointer.buttons.left,
        None => mouse.pressed(MouseButton::Left),
    };
    let just_pressed = pressed && !*was_pressed;
    *was_pressed = pressed;
    let current_window = focused_window.entity();
    for (split, _, _) in &active_drags {
        if !pressed || hierarchy.get(split) != current_window {
            commands.entity(split).remove::<PaneDrag>();
        }
    }
    if !pressed {
        return;
    }
    let Some(window_entity) = current_window else {
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        return;
    };
    if !window.focused {
        for (split, _, _) in &active_drags {
            commands.entity(split).remove::<PaneDrag>();
        }
        return;
    }
    let Some(cursor) = vmux_input::NativePointer::position(window_entity, window) else {
        return;
    };

    if let Some((split_entity, drag, split)) = active_drags
        .iter()
        .find(|(entity, _, _)| hierarchy.get(*entity) == Some(window_entity))
    {
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
        return;
    }

    'splits: for (split_entity, split, children) in &splits {
        if hierarchy.get(split_entity) != Some(window_entity) {
            continue;
        }
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
            if just_pressed {
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
        split.apply_gaps(&mut node, crate::event::PANE_GAP_PX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tab::Tab;
    use bevy::ecs::message::Messages;
    use bevy_cef::prelude::HostWindow;
    use std::sync::{Mutex, MutexGuard};

    static NATIVE_POINTER_TEST: Mutex<()> = Mutex::new(());

    struct ResizeFixture {
        app: App,
        left: Entity,
        right: Entity,
        split: Entity,
        window: Option<Entity>,
        pointer_guard: Option<MutexGuard<'static, ()>>,
    }

    impl ResizeFixture {
        fn new(left_grow: f32, right_grow: f32) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ResizePlugin))
                .init_resource::<ButtonInput<MouseButton>>()
                .insert_resource(LayoutSettings::default());
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
            Self {
                app,
                left,
                right,
                split,
                window: None,
                pointer_guard: None,
            }
        }

        fn native(direction: PaneSplitDirection) -> Self {
            let guard = NATIVE_POINTER_TEST.lock().unwrap();
            vmux_input::NativePointer::publish(Vec2::ZERO, Default::default(), false);
            let mut fixture = Self::new(1.0, 1.0);
            fixture.pointer_guard = Some(guard);
            let window = fixture
                .app
                .world_mut()
                .spawn((Window::default(), vmux_ecs::Active))
                .id();
            fixture.window = Some(window);
            let (parent, previous, next) = match direction {
                PaneSplitDirection::Row => (
                    Vec2::new(1000.0, 500.0),
                    Vec2::new(248.0, 250.0),
                    Vec2::new(752.0, 250.0),
                ),
                PaneSplitDirection::Column => (
                    Vec2::new(500.0, 1000.0),
                    Vec2::new(250.0, 248.0),
                    Vec2::new(250.0, 752.0),
                ),
            };
            let size = match direction {
                PaneSplitDirection::Row => Vec2::new(496.0, 500.0),
                PaneSplitDirection::Column => Vec2::new(500.0, 496.0),
            };
            fixture.app.world_mut().entity_mut(fixture.split).insert((
                HostWindow(window),
                Node::default(),
                PaneSplit { direction },
                ComputedNode {
                    size: parent,
                    center: parent / 2.0,
                    ..default()
                },
            ));
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.left)
                .insert(ComputedNode {
                    size,
                    center: previous,
                    ..default()
                });
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.right)
                .insert(ComputedNode {
                    size,
                    center: next,
                    ..default()
                });
            fixture
        }

        fn pointer(&mut self, left: bool, position: Option<Vec2>) {
            self.app
                .world_mut()
                .get_mut::<Window>(self.window.unwrap())
                .unwrap()
                .set_physical_cursor_position(position.map(|position| position.as_dvec2()));
            let mut buttons = vmux_input::NativePointer::snapshot().unwrap().buttons;
            buttons.left = left;
            vmux_input::NativePointer::publish(position.unwrap_or_default(), buttons, true);
            self.app.update();
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

        fn send(&mut self, resize: PaneResize) {
            self.app
                .world_mut()
                .resource_mut::<Messages<ResizeRequest>>()
                .write(ResizeRequest(resize));
            self.app.update();
        }

        fn grows(&self, pane: Entity) -> (f32, f32) {
            (
                self.app.world().get::<Node>(pane).unwrap().flex_grow,
                self.app.world().get::<PaneSize>(pane).unwrap().flex_grow,
            )
        }
    }

    impl Drop for ResizeFixture {
        fn drop(&mut self) {
            if self.pointer_guard.is_some() {
                vmux_input::NativePointer::publish(Vec2::ZERO, Default::default(), false);
            }
        }
    }

    #[test]
    fn native_pointer_resizes_each_axis_and_releases_without_a_winit_mouse_event() {
        for direction in [PaneSplitDirection::Row, PaneSplitDirection::Column] {
            let mut fixture = ResizeFixture::native(direction);
            let (start, moved) = match direction {
                PaneSplitDirection::Row => (Vec2::new(500.0, 250.0), Vec2::new(600.0, 250.0)),
                PaneSplitDirection::Column => (Vec2::new(250.0, 500.0), Vec2::new(250.0, 600.0)),
            };
            fixture.pointer(false, Some(start));
            fixture.pointer(true, Some(start));
            assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_some());
            assert!(
                !fixture
                    .app
                    .world()
                    .resource::<ButtonInput<MouseButton>>()
                    .pressed(MouseButton::Left)
            );
            fixture.pointer(true, Some(moved));
            let previous = fixture.grows(fixture.left);
            let next = fixture.grows(fixture.right);
            assert!((previous.0 - 1.2).abs() < 0.0001);
            assert!((next.0 - 0.8).abs() < 0.0001);
            assert_eq!(previous.0, previous.1);
            assert_eq!(next.0, next.1);
            fixture.pointer(false, None);
            assert!(fixture.app.world().get::<PaneDrag>(fixture.split).is_none());
            assert_eq!(fixture.grows(fixture.left), previous);
        }
    }

    #[test]
    fn pointer_cannot_start_a_resize_in_another_window() {
        let mut fixture = ResizeFixture::native(PaneSplitDirection::Row);
        let window = fixture.app.world_mut().spawn(Window::default()).id();
        let split = fixture
            .app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                Node::default(),
                ComputedNode {
                    size: Vec2::new(800.0, 500.0),
                    center: Vec2::new(400.0, 250.0),
                    ..default()
                },
                HostWindow(window),
            ))
            .id();
        let previous = ResizeFixture::pane(&mut fixture.app, split, 1.0, 1);
        let next = ResizeFixture::pane(&mut fixture.app, split, 1.0, 1);
        fixture
            .app
            .world_mut()
            .entity_mut(previous)
            .insert(ComputedNode {
                size: Vec2::new(396.0, 500.0),
                center: Vec2::new(198.0, 250.0),
                ..default()
            });
        fixture
            .app
            .world_mut()
            .entity_mut(next)
            .insert(ComputedNode {
                size: Vec2::new(396.0, 500.0),
                center: Vec2::new(602.0, 250.0),
                ..default()
            });
        fixture.pointer(false, Some(Vec2::new(400.0, 250.0)));
        fixture.pointer(true, Some(Vec2::new(400.0, 250.0)));
        assert!(fixture.app.world().get::<PaneDrag>(split).is_none());
        assert_eq!(fixture.grows(previous), (1.0, 1.0));
        assert_eq!(fixture.grows(next), (1.0, 1.0));
    }

    #[test]
    fn split_gap_only_applies_on_split_axis() {
        let row = PaneSplitDirection::Row.gaps(8.0);
        let column = PaneSplitDirection::Column.gaps(8.0);

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

        split.apply_gaps(&mut node, 8.0);

        assert_eq!(node.column_gap, Val::Px(8.0));
        assert_eq!(node.row_gap, Val::Px(0.0));
    }

    #[test]
    fn resize_command_updates_node_and_persisted_size() {
        let mut fixture = ResizeFixture::new(1.0, 1.0);

        fixture.send(PaneResize::Direction(PaneDirection::Right));

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

        fixture.send(PaneResize::Equalize);

        assert_eq!(fixture.grows(fixture.left), (1.0, 1.0));
        assert_eq!(fixture.grows(fixture.right), (1.0, 1.0));
    }
}
