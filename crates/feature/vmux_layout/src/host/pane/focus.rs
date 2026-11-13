use bevy::ecs::system::Commands;
use bevy::prelude::*;
use bevy::window::Window;
#[cfg(test)]
use bevy_cef::prelude::HostWindow;
use std::time::Instant;
use vmux_api::input::KeyModifiers;
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use super::{
    FocusRequest, Pane, PaneDrag, PaneFocus, PaneSplit, PointerDevice, PointerDevicePlugin,
};
#[cfg(test)]
use crate::stack::Stack;
use crate::{
    host::command::LayoutRequestSet,
    stack::{ActiveTabParam, LayoutFocus},
};

const HOVER_COOLDOWN_MS: u64 = 300;

pub(super) struct FocusPlugin;

impl Plugin for FocusPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<PointerDevicePlugin>() {
            app.add_plugins(PointerDevicePlugin);
        }
        if !app.is_plugin_added::<vmux_ecs::PrimitivesPlugin>() {
            app.add_plugins(vmux_ecs::PrimitivesPlugin);
        }
        app.add_message::<FocusRequest>()
            .add_message::<crate::SelectRequest>()
            .configure_sets(
                Update,
                crate::PaneSet::Select.in_set(LayoutRequestSet::Handle),
            )
            .add_systems(
                Update,
                select
                    .in_set(LayoutRequestSet::Handle)
                    .before(crate::PaneSet::Select),
            )
            .add_systems(PostUpdate, warp_cursor_to_active);
        app.add_systems(
            Update,
            from_pointer
                .after(crate::window::WindowFocusSet)
                .after(super::resize::PointerResizeSet)
                .before(crate::PaneSet::Select)
                .before(crate::active::ActiveSystemSet::Descendants)
                .before(crate::stack::ComputeFocusSet),
        );
    }
}

#[derive(Component)]
pub struct PaneHoverCooldown(Instant);

impl PaneHoverCooldown {
    pub fn start() -> Self {
        Self(Instant::now())
    }

    fn active(&self) -> bool {
        self.0.elapsed().as_millis() < HOVER_COOLDOWN_MS as u128
    }
}

#[derive(Component)]
pub struct PendingCursorWarp;

fn select(
    mut reader: MessageReader<FocusRequest>,
    active_tab_param: ActiveTabParam,
    focus_query: LayoutFocus,
    pane_activity: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_layout: Query<&ComputedNode, With<Pane>>,
    mut commands: Commands,
    mut selections: MessageWriter<crate::SelectRequest>,
) {
    for request in reader.read() {
        let focus = &request.0;

        let Some(tab) = active_tab_param.get() else {
            continue;
        };
        let panes = focus_query.leaves(tab);
        if panes.len() < 2 {
            continue;
        }
        let Some(current) = focus_query.pane(tab) else {
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
                let Some(target) = LastActivatedAt::latest(
                    candidates
                        .iter()
                        .filter_map(|&entity| pane_activity.get(entity).ok())
                        .map(|(entity, activated_at)| (entity, *activated_at)),
                ) else {
                    continue;
                };
                target
            }
        };
        selections.write(crate::SelectRequest {
            root: tab,
            pane: target,
        });
        commands
            .entity(target)
            .insert((PaneHoverCooldown::start(), PendingCursorWarp));
    }
}

#[derive(Component)]
pub(super) struct PointerFocus {
    sequence: u64,
    position: Option<Vec2>,
    buttons: u8,
    modifiers: Option<KeyModifiers>,
}

impl Default for PointerFocus {
    fn default() -> Self {
        Self {
            sequence: vmux_input::NativePointer::snapshot().map_or(0, |pointer| pointer.sequence),
            position: None,
            buttons: 0,
            modifiers: None,
        }
    }
}

impl PointerFocus {
    fn sample(
        &mut self,
        window_entity: Entity,
        window: &Window,
        mouse: &ButtonInput<MouseButton>,
    ) -> Option<(Vec2, bool, bool)> {
        #[cfg(target_os = "macos")]
        let (position, buttons, moved, clicked, modifiers) = {
            let _ = (window_entity, window, mouse);
            let pointer = vmux_input::NativePointer::snapshot()?;
            if pointer.sequence == self.sequence {
                return None;
            }
            let clicked = pointer.press_sequence > self.sequence;
            let moved = pointer.motion_sequence == pointer.sequence;
            self.sequence = pointer.sequence;
            let buttons = u8::from(pointer.buttons.left)
                | (u8::from(pointer.buttons.right) << 1)
                | (u8::from(pointer.buttons.middle) << 2);
            let position = if clicked {
                pointer.press_position_px
            } else {
                pointer.position_px
            };
            let modifiers = if clicked {
                pointer.press_modifiers
            } else {
                pointer.modifiers
            };
            (position, buttons, moved, clicked, modifiers)
        };
        #[cfg(not(target_os = "macos"))]
        let (position, buttons, moved, clicked, modifiers) = {
            let _ = self.sequence;
            let position = vmux_input::NativePointer::position(window_entity, window)?;
            let buttons = u8::from(mouse.pressed(MouseButton::Left))
                | (u8::from(mouse.pressed(MouseButton::Right)) << 1)
                | (u8::from(mouse.pressed(MouseButton::Middle)) << 2);
            (
                position,
                buttons,
                self.position != Some(position),
                buttons & !self.buttons != 0,
                None,
            )
        };
        self.position = Some(position);
        self.buttons = buttons;
        self.modifiers = modifiers;
        Some((position, clicked, moved && buttons == 0))
    }

    fn held(&self, keys: &ButtonInput<KeyCode>) -> bool {
        if let Some(modifiers) = self.modifiers {
            return modifiers.shift;
        }
        keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
    }

    fn active(&self, keys: &ButtonInput<KeyCode>) -> bool {
        if let Some(modifiers) = self.modifiers {
            return modifiers.ctrl || modifiers.shift;
        }
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) || self.held(keys)
    }
}

fn from_pointer(
    windows: Query<&Window>,
    focused_window: crate::window::FocusedWindow,
    hierarchy: crate::window::WindowHierarchy,
    leaf_panes: Query<&ComputedNode, (With<Pane>, Without<PaneSplit>)>,
    pane_cooldowns: Query<&PaneHoverCooldown>,
    active_tab: ActiveTabParam,
    focus: LayoutFocus,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    active_drags: Query<(), With<PaneDrag>>,
    mut pointer: Single<&mut PointerFocus, With<PointerDevice>>,
    mut selections: MessageWriter<crate::SelectRequest>,
    mut commands: Commands,
) {
    let Some(window_entity) = focused_window.entity() else {
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        return;
    };
    let Some((position, clicked, moved)) = pointer.sample(window_entity, window, &mouse) else {
        return;
    };
    if !window.focused || !active_drags.is_empty() || pointer.held(&keys) {
        return;
    }
    if !clicked && (!moved || pointer.active(&keys)) {
        return;
    }
    let Some(tab) = active_tab.get() else {
        return;
    };
    let mut target = None;
    for pane in focus.leaves(tab) {
        if hierarchy.get(pane) != Some(window_entity) {
            continue;
        }
        if let Ok(layout) = leaf_panes.get(pane)
            && !layout.is_empty()
            && layout.contains(position)
        {
            target = Some(pane);
            break;
        }
    }
    let Some(target) = target else {
        return;
    };
    let current = focus.pane(tab);
    if let Some(current) = current
        && let Ok(cooldown) = pane_cooldowns.get(current)
    {
        if !clicked && cooldown.active() {
            return;
        }
        commands.entity(current).remove::<PaneHoverCooldown>();
    }
    if current == Some(target) {
        return;
    }
    selections.write(crate::SelectRequest {
        root: tab,
        pane: target,
    });
}

fn warp_cursor_to_active(
    pane_layout: Query<
        (Entity, &ComputedNode),
        (With<Pane>, Without<PaneSplit>, With<PendingCursorWarp>),
    >,
    hierarchy: crate::window::WindowHierarchy,
    mut windows: Query<&mut Window>,
    mut commands: Commands,
) {
    for (target, layout) in &pane_layout {
        if layout.is_empty() {
            continue;
        }
        let Some(window_entity) = hierarchy.get(target) else {
            commands.entity(target).remove::<PendingCursorWarp>();
            continue;
        };
        if let Ok(mut window) = windows.get_mut(window_entity) {
            window.set_physical_cursor_position(Some(layout.center.as_dvec2()));
        }
        commands.entity(target).remove::<PendingCursorWarp>();
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
            app.add_plugins((MinimalPlugins, crate::LayoutPlugin, FocusPlugin))
                .insert_resource(ButtonInput::<KeyCode>::default())
                .insert_resource(ButtonInput::<MouseButton>::default())
                .insert_resource(crate::settings::LayoutSettings::default());
            app.world_mut().spawn(PrimaryWindow);
            Self { app }
        }

        fn hover() -> Self {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                crate::LayoutPlugin,
                crate::contract::LayoutContractPlugin,
                crate::host::command::LayoutRequestPlugin,
                FocusPlugin,
                crate::active::ActivePlugin,
                crate::stack::StackPlugin,
            ))
            .add_message::<crate::tab::CloseTabRequest>()
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .insert_resource(crate::settings::LayoutSettings::default());
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
                .resource_mut::<Messages<FocusRequest>>()
                .write(FocusRequest(PaneFocus::Direction(direction)));
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
    fn shift_prevents_hover_from_activating_another_pane() {
        let mut fixture = HoverFixture::new();
        fixture
            .app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);

        fixture.app.update();

        assert_eq!(
            fixture
                .app
                .world()
                .get::<LastActivatedAt>(fixture.left)
                .unwrap()
                .0,
            1
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LastActivatedAt>(fixture.left_stack)
                .unwrap()
                .0,
            1
        );
    }

    struct HoverFixture {
        app: App,
        window: Entity,
        tab: Entity,
        left: Entity,
        left_stack: Entity,
        left_branch: Entity,
        right: Entity,
        right_stack: Entity,
        _pointer_guard: std::sync::MutexGuard<'static, ()>,
    }

    impl HoverFixture {
        fn new() -> Self {
            let guard = super::super::NATIVE_POINTER_TEST.lock().unwrap();
            vmux_input::NativePointer::publish(Vec2::new(-10.0, -10.0), Default::default(), false);
            let mut fixture = FocusFixture::hover();
            let window = fixture
                .app
                .world_mut()
                .spawn((Window::default(), PrimaryWindow, vmux_ecs::Active))
                .id();
            let root = fixture.app.world_mut().spawn(HostWindow(window)).id();
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
            let left_branch = fixture
                .app
                .world_mut()
                .spawn((
                    Pane,
                    PaneSplit {
                        direction: PaneSplitDirection::Column,
                    },
                    LastActivatedAt(1),
                    ChildOf(row),
                ))
                .id();
            let left = fixture.pane(
                left_branch,
                Vec2::new(400.0, 225.0),
                Vec2::new(800.0, 450.0),
            );
            let bottom = fixture.pane(
                left_branch,
                Vec2::new(400.0, 675.0),
                Vec2::new(800.0, 450.0),
            );
            let right = fixture.pane(row, Vec2::new(1200.0, 450.0), Vec2::new(800.0, 900.0));
            for (pane, timestamp) in [(left, 1), (bottom, 2), (right, 10)] {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(pane)
                    .insert(LastActivatedAt(timestamp));
                let stack = Self::stack(&fixture.app, pane);
                fixture
                    .app
                    .world_mut()
                    .entity_mut(stack)
                    .insert(LastActivatedAt(timestamp));
            }
            let left_stack = Self::stack(&fixture.app, left);
            let right_stack = Self::stack(&fixture.app, right);
            let mut result = Self {
                app: fixture.app,
                window,
                tab,
                left,
                left_stack,
                left_branch,
                right,
                right_stack,
                _pointer_guard: guard,
            };
            result.app.update();
            result.pointer(Vec2::new(400.0, 225.0), false);
            result
        }

        fn stack(app: &App, pane: Entity) -> Entity {
            app.world()
                .get::<Children>(pane)
                .unwrap()
                .iter()
                .find(|&entity| app.world().get::<Stack>(entity).is_some())
                .unwrap()
        }

        fn pointer(&mut self, position: Vec2, pressed: bool) {
            self.pointer_with_modifiers(position, pressed, None);
        }

        fn pointer_with_modifiers(
            &mut self,
            position: Vec2,
            pressed: bool,
            modifiers: Option<KeyModifiers>,
        ) {
            vmux_input::NativePointer::publish_with_modifiers(
                position,
                bevy_cef_core::prelude::NativeMouseButtons {
                    left: pressed,
                    ..default()
                },
                !pressed,
                modifiers,
            );
            self.app
                .world_mut()
                .entity_mut(self.window)
                .get_mut::<Window>()
                .unwrap()
                .set_physical_cursor_position(Some(position.as_dvec2()));
            let mut mouse = self
                .app
                .world_mut()
                .resource_mut::<ButtonInput<MouseButton>>();
            if pressed {
                mouse.press(MouseButton::Left);
            } else {
                mouse.release(MouseButton::Left);
            }
        }

        fn focused(&mut self) -> crate::active_pane::ActiveStack {
            let world = self.app.world_mut();
            let mut query = world.query::<&crate::active_pane::ActiveStack>();
            *query.single(world).unwrap()
        }
    }

    impl Drop for HoverFixture {
        fn drop(&mut self) {
            vmux_input::NativePointer::publish(Vec2::ZERO, Default::default(), false);
        }
    }

    #[test]
    fn nested_hover_updates_active_ancestors_and_focused_stack_in_one_frame() {
        let mut fixture = HoverFixture::new();
        assert_eq!(fixture.focused().stack, Some(fixture.right_stack));
        fixture.app.update();
        assert_eq!(fixture.focused().pane, Some(fixture.left));
        assert_eq!(fixture.focused().stack, Some(fixture.left_stack));
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::Active>(fixture.left_branch)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::Active>(fixture.left)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::Active>(fixture.right)
                .is_none()
        );
    }

    #[test]
    fn click_bypasses_control_hover_lock_and_cooldown() {
        let mut fixture = HoverFixture::new();
        fixture
            .app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        fixture.app.update();
        assert_eq!(fixture.focused().stack, Some(fixture.right_stack));
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.right)
            .insert(PaneHoverCooldown::start());
        fixture.pointer(Vec2::new(400.0, 225.0), true);
        fixture.app.update();
        assert_eq!(fixture.focused().stack, Some(fixture.left_stack));
    }

    #[test]
    fn shift_holds_pointer_selection_and_does_not_replay_a_held_click() {
        for key in [KeyCode::ShiftLeft, KeyCode::ShiftRight] {
            let mut fixture = HoverFixture::new();
            fixture
                .app
                .world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            fixture.app.update();
            fixture.pointer(Vec2::new(400.0, 225.0), true);
            fixture.app.update();
            assert_eq!(fixture.focused().stack, Some(fixture.right_stack));
            fixture.pointer(Vec2::new(400.0, 225.0), false);
            fixture.app.update();
            fixture
                .app
                .world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .release(key);
            fixture.app.update();
            assert_eq!(fixture.focused().stack, Some(fixture.right_stack));
            fixture.pointer(Vec2::new(401.0, 225.0), false);
            fixture.app.update();
            assert_eq!(fixture.focused().stack, Some(fixture.left_stack));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_shift_holds_selection_without_winit_modifier_events() {
        let mut fixture = HoverFixture::new();
        let shift = Some(KeyModifiers {
            shift: true,
            ..default()
        });
        fixture.pointer_with_modifiers(Vec2::new(400.0, 225.0), false, shift);
        fixture.app.update();
        assert_eq!(fixture.focused().stack, Some(fixture.right_stack));
        fixture.pointer_with_modifiers(Vec2::new(400.0, 225.0), true, shift);
        fixture.pointer_with_modifiers(Vec2::new(400.0, 225.0), false, Some(default()));
        fixture.app.update();
        assert_eq!(fixture.focused().stack, Some(fixture.right_stack));
        fixture.app.update();
        assert_eq!(fixture.focused().stack, Some(fixture.right_stack));
        fixture
            .app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        fixture.pointer_with_modifiers(Vec2::new(401.0, 225.0), false, Some(default()));
        fixture.app.update();
        assert_eq!(fixture.focused().stack, Some(fixture.left_stack));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn quick_native_click_is_selected_after_release_without_an_intervening_frame() {
        let mut fixture = HoverFixture::new();
        fixture.pointer(Vec2::new(400.0, 225.0), true);
        fixture.pointer(Vec2::new(1200.0, 450.0), false);
        fixture.app.update();
        assert_eq!(fixture.focused().stack, Some(fixture.left_stack));
    }

    #[test]
    fn hidden_tab_geometry_cannot_receive_pointer_focus() {
        let mut fixture = HoverFixture::new();
        let parent = fixture
            .app
            .world()
            .get::<ChildOf>(fixture.tab)
            .unwrap()
            .parent();
        let hidden_tab = fixture
            .app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(0), ChildOf(parent)))
            .id();
        fixture.app.world_mut().spawn((
            Pane,
            ChildOf(hidden_tab),
            ComputedNode {
                center: Vec2::new(400.0, 225.0),
                size: Vec2::new(800.0, 450.0),
                ..default()
            },
        ));
        fixture.app.update();
        assert_eq!(fixture.focused().tab, Some(fixture.tab));
        assert_eq!(fixture.focused().stack, Some(fixture.left_stack));
    }
}
