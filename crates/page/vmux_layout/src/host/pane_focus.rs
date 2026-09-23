use bevy::prelude::*;
use bevy_cef::prelude::HostWindow;
use std::time::Instant;
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use super::{
    command::LayoutRequestSet,
    pane::{Pane, PaneDrag, PaneFocus, PaneRequest, PaneSplit},
    stack::{ActiveTabParam, Stack, active_among, active_pane_in_tab, active_stack_in_pane},
};

#[cfg_attr(target_os = "macos", allow(dead_code))]
const HOVER_COOLDOWN_MS: u64 = 300;

pub(super) struct FocusPlugin;

impl Plugin for FocusPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PaneRequest>()
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .add_systems(Update, on_pane_select.in_set(LayoutRequestSet::Handle))
            .add_systems(PostUpdate, warp_cursor_to_active_pane);
        #[cfg(target_os = "macos")]
        app.add_systems(
            Update,
            apply_pending_hover.before(crate::stack::ComputeFocusSet),
        );
        #[cfg(not(target_os = "macos"))]
        app.add_systems(
            Update,
            poll_cursor_pane_focus.before(crate::stack::ComputeFocusSet),
        );
    }
}

#[derive(Resource, Default)]
pub struct PaneHoverIntent {
    pub target: Option<Entity>,
    pub last_activation: Option<Instant>,
}

#[derive(Resource, Default)]
pub struct PendingCursorWarp {
    pub target: Option<Entity>,
}

fn on_pane_select(
    mut reader: MessageReader<PaneRequest>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_activity: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_layout: Query<&ComputedNode, With<Pane>>,
    mut hover_intent: ResMut<PaneHoverIntent>,
    mut pending_warp: ResMut<PendingCursorWarp>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let PaneRequest::Focus(focus) = request else {
            continue;
        };

        let Some(tab) = active_tab_param.get() else {
            continue;
        };
        let panes = collect_tab_leaf_panes(tab, &all_children, &leaf_panes);
        if panes.len() < 2 {
            continue;
        }
        let Some(current) = active_pane_in_tab(tab, &all_children, &leaf_panes, &pane_activity)
        else {
            continue;
        };
        let target = match focus {
            PaneFocus::Next => {
                let Some(index) = panes.iter().position(|pane| *pane == current) else {
                    continue;
                };
                panes[(index + 1) % panes.len()]
            }
            PaneFocus::Direction(direction) => {
                let direction = match direction {
                    vmux_api::open_target::PaneDirection::Left => Vec2::new(-1.0, 0.0),
                    vmux_api::open_target::PaneDirection::Right => Vec2::new(1.0, 0.0),
                    vmux_api::open_target::PaneDirection::Top => Vec2::new(0.0, -1.0),
                    vmux_api::open_target::PaneDirection::Bottom => Vec2::new(0.0, 1.0),
                };
                let Ok(&current_layout) = pane_layout.get(current) else {
                    continue;
                };
                let mut candidates = Vec::new();
                for pane in &panes {
                    if *pane == current {
                        continue;
                    }
                    let Ok(&candidate_layout) = pane_layout.get(*pane) else {
                        continue;
                    };
                    if (candidate_layout.center - current_layout.center).dot(direction) <= 0.0 {
                        continue;
                    }
                    let overlaps = if direction.x.abs() > 0.5 {
                        current_layout.overlaps_rows(candidate_layout)
                    } else {
                        current_layout.overlaps_columns(candidate_layout)
                    };
                    if overlaps {
                        candidates.push(*pane);
                    }
                }
                let Some(target) = active_among(
                    candidates
                        .iter()
                        .filter_map(|&entity| pane_activity.get(entity).ok()),
                ) else {
                    continue;
                };
                target
            }
        };
        hover_intent.target = None;
        hover_intent.last_activation = Some(Instant::now());
        commands.entity(target).insert(LastActivatedAt::now());
        pending_warp.target = Some(target);
    }
}

fn collect_tab_leaf_panes(
    root: Entity,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Vec<Entity> {
    let mut result = Vec::new();
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        if leaf_panes.contains(entity) {
            result.push(entity);
        }
        if let Ok(children) = all_children.get(entity) {
            for child in children.iter() {
                pending.push(child);
            }
        }
    }
    result
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn poll_cursor_pane_focus(
    windows: Query<(Entity, &Window)>,
    focused_window: Res<crate::window::FocusedWindow>,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
    leaf_panes: Query<(Entity, &ComputedNode), (With<Pane>, Without<PaneSplit>)>,
    pane_activity: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_activity: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut intent: ResMut<PaneHoverIntent>,
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    active_drags: Query<(), With<PaneDrag>>,
) {
    if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
        return;
    }
    if !active_drags.is_empty() {
        return;
    }
    if let Some(last) = intent.last_activation
        && last.elapsed().as_millis() < HOVER_COOLDOWN_MS as u128
    {
        return;
    }
    let Some(window_entity) = focused_window.0 else {
        return;
    };
    let Ok((_, window)) = windows.get(window_entity) else {
        return;
    };
    let Some(cursor) = pane_hover_cursor_position(window_entity, window) else {
        return;
    };

    let mut hovered_pane = None;
    for (entity, layout) in &leaf_panes {
        if crate::window::host_window_of(entity, &child_of, &host_windows) == Some(window_entity)
            && layout.contains(cursor)
        {
            hovered_pane = Some(entity);
            break;
        }
    }

    let Some(target) = hovered_pane else {
        intent.target = None;
        return;
    };
    let current = active_among(
        leaf_panes
            .iter()
            .filter_map(|(entity, _)| pane_activity.get(entity).ok()),
    );
    if current == Some(target) {
        intent.target = None;
        return;
    }

    commands.entity(target).insert(LastActivatedAt::now());
    if let Some(stack) = active_stack_in_pane(target, &pane_children, &stack_activity) {
        commands.entity(stack).insert(LastActivatedAt::now());
    }
    intent.target = None;
}

pub fn pane_hover_cursor_position(window_entity: Entity, window: &Window) -> Option<Vec2> {
    #[cfg(target_os = "macos")]
    {
        native_window_cursor_position(window_entity, window).or_else(|| {
            window
                .physical_cursor_position()
                .map(|position| Vec2::new(position.x, position.y))
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window_entity;
        window
            .physical_cursor_position()
            .map(|position| Vec2::new(position.x, position.y))
    }
}

#[cfg(target_os = "macos")]
fn native_window_cursor_position(window_entity: Entity, window: &Window) -> Option<Vec2> {
    use bevy::winit::WINIT_WINDOWS;
    use objc2_app_kit::{NSApplication, NSEvent, NSView};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let mtm = objc2::MainThreadMarker::new()?;
        if !NSApplication::sharedApplication(mtm).isActive() {
            return None;
        }
        let winit_window = winit_windows.get_window(window_entity)?;
        let handle = winit_window.window_handle().ok()?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return None;
        };
        let view: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
        let ns_window = view.window()?;
        let screen_point = NSEvent::mouseLocation();
        let window_point = ns_window.convertPointFromScreen(screen_point);
        let point = view.convertPoint_fromView(window_point, None);
        let bounds = view.bounds();
        let y = if view.isFlipped() {
            point.y
        } else {
            bounds.size.height - point.y
        };
        let scale = window.resolution.scale_factor() as f64;
        let x = point.x * scale;
        let y = y * scale;
        if x.is_finite() && y.is_finite() {
            Some(Vec2::new(x as f32, y as f32))
        } else {
            None
        }
    })
}

#[cfg(target_os = "macos")]
fn apply_pending_hover(
    focused_window: Res<crate::window::FocusedWindow>,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
    leaf_panes: Query<(Entity, &ComputedNode), (With<Pane>, Without<PaneSplit>)>,
    pane_activity: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_activity: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut commands: Commands,
    mut last_motion_sequence: Local<u64>,
) {
    let Some(pointer) = crate::native_pointer::snapshot() else {
        return;
    };
    if pointer.motion_sequence == 0 || pointer.motion_sequence == *last_motion_sequence {
        return;
    }
    *last_motion_sequence = pointer.motion_sequence;
    let Some(window_entity) = focused_window.0 else {
        return;
    };
    let mut target = None;
    for (entity, layout) in leaf_panes.iter() {
        if crate::window::host_window_of(entity, &child_of, &host_windows) == Some(window_entity)
            && layout.contains(pointer.position_px)
        {
            target = Some(entity);
            break;
        }
    }
    let Some(target) = target else {
        return;
    };
    let current = active_among(
        leaf_panes
            .iter()
            .filter_map(|(entity, _)| pane_activity.get(entity).ok()),
    );
    if current == Some(target) {
        return;
    }
    commands.entity(target).insert(LastActivatedAt::now());
    if let Some(stack) = active_stack_in_pane(target, &pane_children, &stack_activity) {
        commands.entity(stack).insert(LastActivatedAt::now());
    }
}

fn warp_cursor_to_active_pane(
    mut pending: ResMut<PendingCursorWarp>,
    pane_layout: Query<&ComputedNode, (With<Pane>, Without<PaneSplit>)>,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
    mut windows: Query<&mut Window>,
) {
    let Some(target) = pending.target else {
        return;
    };
    let Ok(&layout) = pane_layout.get(target) else {
        return;
    };
    if layout.is_empty() {
        return;
    }
    pending.target = None;
    let Some(window_entity) = crate::window::host_window_of(target, &child_of, &host_windows)
    else {
        return;
    };
    if let Ok(mut window) = windows.get_mut(window_entity) {
        window.set_physical_cursor_position(Some(layout.center.as_dvec2()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{pane::PaneSplitDirection, tab::Tab};
    use bevy::{ecs::message::Messages, window::PrimaryWindow};
    use vmux_api::open_target::PaneDirection;

    struct FocusFixture {
        app: App,
    }

    impl FocusFixture {
        fn selection() -> Self {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<PaneRequest>()
                .init_resource::<PaneHoverIntent>()
                .init_resource::<PendingCursorWarp>()
                .add_systems(Update, on_pane_select);
            app.world_mut().spawn(PrimaryWindow);
            Self { app }
        }

        fn hover() -> Self {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .init_resource::<PaneHoverIntent>()
                .insert_resource(ButtonInput::<KeyCode>::default())
                .add_systems(Update, poll_cursor_pane_focus);
            Self { app }
        }

        fn pane(&mut self, parent: Entity, center: Vec2, size: Vec2) -> Entity {
            let pane = self
                .app
                .world_mut()
                .spawn((
                    Pane,
                    Node::default(),
                    LastActivatedAt::now(),
                    ChildOf(parent),
                    ComputedNode {
                        size,
                        center,
                        ..default()
                    },
                ))
                .id();
            self.app
                .world_mut()
                .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));
            pane
        }

        fn select(&mut self, direction: PaneDirection) {
            self.app
                .world_mut()
                .resource_mut::<Messages<PaneRequest>>()
                .write(PaneRequest::Focus(PaneFocus::Direction(direction)));
            self.app.update();
        }
    }

    #[test]
    fn select_right_picks_most_recently_active_among_overlapping_neighbors() {
        let mut fixture = FocusFixture::selection();
        let tab = fixture
            .app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let row = fixture
            .app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let left = fixture.pane(row, Vec2::new(399.5, 450.0), Vec2::new(791.0, 892.0));
        let column = fixture
            .app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                ChildOf(row),
            ))
            .id();
        let top = fixture.pane(column, Vec2::new(1199.5, 225.0), Vec2::new(793.0, 442.0));
        let bottom = fixture.pane(column, Vec2::new(1199.5, 675.0), Vec2::new(793.0, 442.0));

        fixture
            .app
            .world_mut()
            .entity_mut(bottom)
            .insert(LastActivatedAt::now());
        std::thread::sleep(std::time::Duration::from_millis(2));
        fixture
            .app
            .world_mut()
            .entity_mut(top)
            .insert(LastActivatedAt::now());
        std::thread::sleep(std::time::Duration::from_millis(2));
        fixture
            .app
            .world_mut()
            .entity_mut(left)
            .insert(LastActivatedAt::now());

        let previous_top = fixture.app.world().get::<LastActivatedAt>(top).unwrap().0;
        let previous_bottom = fixture
            .app
            .world()
            .get::<LastActivatedAt>(bottom)
            .unwrap()
            .0;
        assert!(previous_top > previous_bottom);

        fixture.select(PaneDirection::Right);

        assert!(fixture.app.world().get::<LastActivatedAt>(top).unwrap().0 > previous_top);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LastActivatedAt>(bottom)
                .unwrap()
                .0,
            previous_bottom
        );
    }

    #[test]
    fn select_left_picks_full_height_neighbor_from_sub_split_pane() {
        let mut fixture = FocusFixture::selection();
        let tab = fixture
            .app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let row = fixture
            .app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let left = fixture.pane(row, Vec2::new(399.5, 450.0), Vec2::new(791.0, 892.0));
        let column = fixture
            .app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                ChildOf(row),
            ))
            .id();
        let top = fixture.pane(column, Vec2::new(1199.5, 225.0), Vec2::new(793.0, 442.0));
        let bottom = fixture.pane(column, Vec2::new(1199.5, 675.0), Vec2::new(793.0, 442.0));

        fixture
            .app
            .world_mut()
            .entity_mut(left)
            .insert(LastActivatedAt(1));
        fixture
            .app
            .world_mut()
            .entity_mut(top)
            .insert(LastActivatedAt(10));
        fixture
            .app
            .world_mut()
            .entity_mut(bottom)
            .insert(LastActivatedAt(0));
        let previous_left = fixture.app.world().get::<LastActivatedAt>(left).unwrap().0;

        fixture.select(PaneDirection::Left);

        assert!(fixture.app.world().get::<LastActivatedAt>(left).unwrap().0 > previous_left);
    }

    #[test]
    fn select_left_picks_left_neighbor_in_horizontal_split() {
        let mut fixture = FocusFixture::selection();
        let tab = fixture
            .app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let row = fixture
            .app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let left = fixture.pane(row, Vec2::new(400.0, 450.0), Vec2::new(800.0, 900.0));
        let right = fixture.pane(row, Vec2::new(1200.0, 450.0), Vec2::new(800.0, 900.0));

        fixture
            .app
            .world_mut()
            .entity_mut(right)
            .insert(LastActivatedAt::now());
        std::thread::sleep(std::time::Duration::from_millis(2));

        fixture.select(PaneDirection::Left);

        let left_activity = fixture.app.world().get::<LastActivatedAt>(left).unwrap().0;
        let right_activity = fixture.app.world().get::<LastActivatedAt>(right).unwrap().0;
        assert!(left_activity > right_activity);
    }

    #[test]
    fn pane_hover_activates_hovered_pane_in_single_update() {
        let mut fixture = FocusFixture::hover();
        let window = fixture
            .app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        fixture
            .app
            .insert_resource(crate::window::FocusedWindow(Some(window)));
        let root = fixture.app.world_mut().spawn(HostWindow(window)).id();
        fixture
            .app
            .world_mut()
            .entity_mut(window)
            .get_mut::<Window>()
            .unwrap()
            .set_physical_cursor_position(Some(bevy::math::DVec2::new(400.0, 450.0)));
        let tab = fixture
            .app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(root)))
            .id();
        let row = fixture
            .app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let left = fixture.pane(row, Vec2::new(400.0, 450.0), Vec2::new(800.0, 900.0));
        let right = fixture.pane(row, Vec2::new(1200.0, 450.0), Vec2::new(800.0, 900.0));
        fixture
            .app
            .world_mut()
            .entity_mut(left)
            .insert(LastActivatedAt(1));
        fixture
            .app
            .world_mut()
            .entity_mut(right)
            .insert(LastActivatedAt(10));
        let left_stack = fixture
            .app
            .world()
            .get::<Children>(left)
            .unwrap()
            .iter()
            .find(|&entity| fixture.app.world().get::<Stack>(entity).is_some())
            .unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(left_stack)
            .insert(LastActivatedAt(1));

        fixture.app.update();

        assert!(fixture.app.world().get::<LastActivatedAt>(left).unwrap().0 > 10);
        assert!(
            fixture
                .app
                .world()
                .get::<LastActivatedAt>(left_stack)
                .unwrap()
                .0
                > 1
        );
    }
}
