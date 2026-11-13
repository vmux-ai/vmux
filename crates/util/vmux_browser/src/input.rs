use crate::host::{CefPointerRegions, LayoutPointerCapture};
use bevy::ecs::system::Commands;
use bevy::window::Window;
use bevy::{
    ecs::relationship::Relationship,
    input::{
        ButtonState,
        keyboard::KeyboardInput,
        mouse::{MouseButtonInput, MouseWheel},
    },
    prelude::*,
};
use bevy_cef::prelude::*;
use vmux_ecs::overlay::Overlay;
use vmux_flex::prelude::{ComputedNode, Visibility};
use vmux_input::{PointerCapture, PointerSet, PointerState, PointerUpdated};
use vmux_layout::overlay::{LayoutOverlayActive, LayoutOverlayDismissRevision};
use vmux_layout::{Browser, LayoutCef};

pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PointerUpdated>()
            .add_message::<PointerEffect>()
            .add_systems(
                Update,
                (initialize, ApplyDeferred, forward, project)
                    .chain()
                    .in_set(PointerSet::Route),
            )
            .add_systems(PreUpdate, log_command_bar_keyboard)
            .add_systems(Update, track_interaction)
            .add_systems(Last, project.in_set(PointerSet::Publish))
            .add_systems(Last, send.after(PointerSet::Publish));
    }
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct WindowPointerRegions {
    pub layout: bool,
    pub windowed: bool,
}

#[derive(Component, Default)]
pub(crate) struct CefHoverPosition {
    pub(crate) position: Option<Vec2>,
    pub(crate) inside: bool,
}

#[derive(Message)]
pub(crate) enum PointerEffect {
    Move {
        target: Entity,
        buttons: bevy_cef_core::prelude::NativeMouseButtons,
        position: Vec2,
        leave: bool,
    },
    Click {
        target: Entity,
        position: Vec2,
        button: PointerButton,
        released: bool,
    },
}

fn initialize(
    windows: Query<Entity, (With<Window>, Without<WindowPointerRegions>)>,
    layouts: Query<Entity, (With<LayoutCef>, Without<CefHoverPosition>)>,
    mut commands: Commands,
) {
    for window in &windows {
        commands
            .entity(window)
            .insert(WindowPointerRegions::default());
    }
    for layout in &layouts {
        commands.entity(layout).insert(CefHoverPosition::default());
    }
}

fn forward(
    mut events: MessageReader<PointerUpdated>,
    mut windows: Query<(&Window, &mut PointerCapture)>,
    suppress: Res<CefSuppressPointerInput>,
    mut effects: MessageWriter<PointerEffect>,
    mut layouts: Query<
        (
            Entity,
            &HostWindow,
            &mut LayoutOverlayDismissRevision,
            &mut CefHoverPosition,
        ),
        With<LayoutCef>,
    >,
    overlays: Query<&LayoutOverlayActive>,
    captures: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    regions: CefPointerRegions,
    mut commands: Commands,
) {
    for event in events.read() {
        let Ok((window, mut capture)) = windows.get_mut(event.window) else {
            continue;
        };
        if capture.consumes(event.native_token) || capture.suppresses(event.sequence) {
            continue;
        }
        let Some(position) = event.position_px else {
            continue;
        };
        let scale = window.resolution.scale_factor();
        if !scale.is_finite() || scale <= 0.0 {
            continue;
        }
        let position = position / scale;
        let Some((layout, _, mut revision, mut hover)) = layouts
            .iter_mut()
            .find(|(_, host, _, _)| host.0 == event.window)
        else {
            continue;
        };
        let in_region = regions.contains(event.window, position * scale);
        let inside = !suppress.0
            && !event.cancelled
            && (capture.owner() == Some(layout) || captures.contains(layout) || in_region);
        if (event.motion || (event.cancelled && hover.inside)) && (inside || hover.inside) {
            effects.write(PointerEffect::Move {
                target: layout,
                buttons: event.buttons,
                position,
                leave: !inside,
            });
            hover.position = Some(position);
        }
        hover.inside = inside;
        if inside || capture.owner() == Some(layout) {
            capture.route(event.sequence);
        }
        for (button, native, before, after) in [
            (
                MouseButton::Left,
                PointerButton::Primary,
                event.previous_buttons.left,
                event.buttons.left,
            ),
            (
                MouseButton::Right,
                PointerButton::Secondary,
                event.previous_buttons.right,
                event.buttons.right,
            ),
            (
                MouseButton::Middle,
                PointerButton::Middle,
                event.previous_buttons.middle,
                event.buttons.middle,
            ),
        ] {
            if before == after {
                continue;
            }
            if after
                && !suppress.0
                && overlays
                    .get(layout)
                    .is_ok_and(LayoutOverlayActive::dismisses_background_click)
                && !in_region
            {
                commands.trigger(
                    vmux_ecs::UiStateWrite::<vmux_layout::state::LayoutUiState>::from_event(
                        layout,
                        &revision.next_effect(),
                    ),
                );
            }
            if after && inside {
                capture.claim(layout, button);
            }
            if inside
                || capture.owner() == Some(layout)
                || (event.cancelled && event.capture_owner == Some(layout))
            {
                effects.write(PointerEffect::Move {
                    target: layout,
                    buttons: event.buttons,
                    position,
                    leave: !inside,
                });
                effects.write(PointerEffect::Click {
                    target: layout,
                    position,
                    button: native,
                    released: !after,
                });
            }
            if !after {
                capture.release(button);
            }
        }
        if suppress.0 || event.cancelled {
            capture.clear();
        }
    }
}

fn send(mut effects: MessageReader<PointerEffect>, browsers: NonSend<Browsers>) {
    for effect in effects.read() {
        match effect {
            PointerEffect::Move {
                target,
                buttons,
                position,
                leave,
            } => {
                browsers.send_native_mouse_move(target, *buttons, *position, *leave);
            }
            PointerEffect::Click {
                target,
                position,
                button,
                released,
            } => {
                browsers.send_mouse_click(target, *position, *button, *released);
            }
        }
    }
}

fn project(
    mut windows: Query<(Entity, &PointerState, &mut WindowPointerRegions)>,
    layouts: Query<(Entity, &HostWindow), With<LayoutCef>>,
    captures: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    pages: Query<
        (Entity, &ComputedNode, Option<&Visibility>),
        (With<Browser>, With<WebviewWindowed>, Without<LayoutCef>),
    >,
    hierarchy: vmux_layout::window::WindowHierarchy,
    regions: CefPointerRegions,
    suppress: Res<CefSuppressPointerInput>,
) {
    for (window, pointer, mut state) in &mut windows {
        let layout = layouts
            .iter()
            .find_map(|(layout, host)| (host.0 == window).then_some(layout));
        let position = pointer.position_px;
        state.layout = !suppress.0
            && (layout.is_some_and(|layout| captures.contains(layout))
                || position.is_some_and(|position| regions.contains(window, position)));
        state.windowed = false;
        if let Some(position) = position {
            for (page, frame, visibility) in &pages {
                if hierarchy.get(page) == Some(window)
                    && !matches!(visibility, Some(Visibility::Hidden))
                    && frame.contains(position)
                {
                    state.windowed = true;
                    break;
                }
            }
        }
    }
}

fn log_command_bar_keyboard(mut events: MessageReader<KeyboardInput>, overlay: Overlay) {
    if !overlay.owns_input() {
        return;
    }
    for event in events.read() {
        if event.state == ButtonState::Pressed {
            bevy::log::info!(key = ?event.key_code, repeat = event.repeat, "command bar keyboard received");
        }
    }
}

#[derive(Component)]
pub(crate) struct RecentBrowserInteraction {
    at: std::time::Instant,
}

impl RecentBrowserInteraction {
    pub(crate) fn now() -> Self {
        Self {
            at: std::time::Instant::now(),
        }
    }

    pub(crate) fn active(&self) -> bool {
        self.at.elapsed() < std::time::Duration::from_secs(2)
    }
}

fn track_interaction(
    mut mouse_buttons: MessageReader<MouseButtonInput>,
    mut mouse_wheels: MessageReader<MouseWheel>,
    mut keyboard: MessageReader<KeyboardInput>,
    focus: vmux_layout::stack::FocusedStack,
    browsers: Query<&ChildOf, With<Browser>>,
    mut commands: Commands,
) {
    let mut mouse_pressed = false;
    for event in mouse_buttons.read() {
        mouse_pressed |= event.state == ButtonState::Pressed;
    }
    let mouse_wheel = mouse_wheels.read().count() > 0;
    let mut key_pressed = false;
    for event in keyboard.read() {
        key_pressed |= event.state == ButtonState::Pressed;
    }
    let interacted = mouse_pressed || mouse_wheel || key_pressed;
    if !interacted {
        return;
    }
    let Some(stack) = focus.stack else { return };
    if browsers.iter().any(|child_of| child_of.get() == stack) {
        commands
            .entity(stack)
            .insert(RecentBrowserInteraction::now());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use bevy_cef_core::prelude::NativeMouseButtons;
    use vmux_layout::event::LayoutOverlayKind;
    use vmux_layout::state::LayoutUiState;

    #[derive(Resource, Default)]
    struct Dismissals(usize);

    fn record(
        event: On<vmux_ecs::UiStateWrite<LayoutUiState>>,
        mut dismissals: ResMut<Dismissals>,
    ) {
        if event.update().overlay_dismiss.is_some() {
            dismissals.0 += 1;
        }
    }

    #[test]
    fn consumed_pointer_sequences_do_not_click_cef_after_capture_release() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin, InputPlugin))
            .init_resource::<CefSuppressPointerInput>()
            .init_non_send::<Browsers>();
        let window = app
            .world_mut()
            .spawn((
                Window::default(),
                PointerState::default(),
                PointerCapture::default(),
                vmux_ecs::Active,
            ))
            .id();
        app.world_mut().spawn((
            LayoutCef::bundle(window),
            LayoutOverlayActive::new("test", LayoutOverlayKind::Panel),
        ));
        app.update();
        for (suppressed, expected_clicks) in [(false, 2), (true, 0)] {
            for pressed in [true, false] {
                let event = app
                    .world_mut()
                    .get_mut::<PointerState>(window)
                    .unwrap()
                    .update(vmux_input::PointerInput::sample(
                        window,
                        Vec2::new(400.0, 300.0),
                        NativeMouseButtons {
                            left: pressed,
                            ..default()
                        },
                        false,
                    ));
                if suppressed {
                    app.world_mut()
                        .get_mut::<PointerCapture>(window)
                        .unwrap()
                        .suppress(event.sequence);
                }
                app.world_mut().write_message(event);
            }
            app.update();
            let clicks = app
                .world_mut()
                .resource_mut::<Messages<PointerEffect>>()
                .drain()
                .filter(|effect| matches!(effect, PointerEffect::Click { .. }))
                .count();
            assert_eq!(clicks, expected_clicks);
        }
    }

    #[test]
    fn captured_layout_receives_outside_motion_and_idle_cancel_emits_leave() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::input::InputPlugin,
            InputPlugin,
            crate::frame_rate::FrameRatePlugin,
        ))
        .init_resource::<CefSuppressPointerInput>()
        .init_non_send::<Browsers>();
        let window = app
            .world_mut()
            .spawn((
                Window::default(),
                PointerState::default(),
                PointerCapture::default(),
            ))
            .id();
        let layout = app.world_mut().spawn(LayoutCef::bundle(window)).id();
        app.update();
        app.world_mut()
            .get_mut::<PointerCapture>(window)
            .unwrap()
            .claim(layout, MouseButton::Left);
        for position in [Vec2::new(400.0, 300.0), Vec2::new(600.0, 500.0)] {
            let event = app
                .world_mut()
                .get_mut::<PointerState>(window)
                .unwrap()
                .update(vmux_input::PointerInput::sample(
                    window,
                    position,
                    NativeMouseButtons {
                        left: true,
                        ..default()
                    },
                    true,
                ));
            app.world_mut().write_message(event);
            app.update();
            let effects = app
                .world_mut()
                .resource_mut::<Messages<PointerEffect>>()
                .drain()
                .collect::<Vec<_>>();
            assert!(effects.iter().any(|effect| matches!(
                effect,
                PointerEffect::Move { target, position: actual, leave: false, .. }
                    if *target == layout && *actual == position
            )));
            assert!(app.world().get::<CefHoverPosition>(layout).unwrap().inside);
        }
        app.world_mut()
            .get_mut::<PointerCapture>(window)
            .unwrap()
            .clear();
        app.world_mut()
            .entity_mut(layout)
            .insert(LayoutOverlayActive::new("test", LayoutOverlayKind::Panel));
        let event = app
            .world_mut()
            .get_mut::<PointerState>(window)
            .unwrap()
            .update(vmux_input::PointerInput::sample(
                window,
                Vec2::new(600.0, 500.0),
                NativeMouseButtons::default(),
                true,
            ));
        app.world_mut().write_message(event);
        app.update();
        app.world_mut()
            .resource_mut::<Messages<PointerEffect>>()
            .clear();
        let event = app
            .world_mut()
            .get_mut::<PointerState>(window)
            .unwrap()
            .update(vmux_input::PointerInput {
                cancelled: true,
                ..vmux_input::PointerInput::buttons(window, NativeMouseButtons::default())
            });
        app.world_mut().write_message(event);
        app.update();
        let effects = app
            .world_mut()
            .resource_mut::<Messages<PointerEffect>>()
            .drain()
            .collect::<Vec<_>>();
        assert!(matches!(
            effects.as_slice(),
            [PointerEffect::Move { target, leave: true, .. }] if *target == layout
        ));
        assert!(!app.world().get::<CefHoverPosition>(layout).unwrap().inside);
    }

    #[test]
    fn native_background_clicks_leave_popover_items_to_their_dom_handlers() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::input::InputPlugin,
            vmux_input::PointerPlugin,
            InputPlugin,
        ))
        .add_message::<bevy::window::CursorMoved>()
        .add_message::<bevy::window::WindowFocused>()
        .init_resource::<Dismissals>()
        .init_resource::<CefSuppressPointerInput>()
        .init_non_send::<Browsers>()
        .add_observer(record);
        let window = app
            .world_mut()
            .spawn((Window::default(), vmux_ecs::Active))
            .id();
        let layout = app.world_mut().spawn(LayoutCef::bundle(window)).id();
        for (kind, expected) in [
            (LayoutOverlayKind::Popover, 0),
            (LayoutOverlayKind::Panel, 1),
        ] {
            app.world_mut()
                .entity_mut(layout)
                .insert(LayoutOverlayActive::new("test", kind));
            app.world_mut()
                .write_message(vmux_input::PointerInput::sample(
                    window,
                    Vec2::new(400.0, 300.0),
                    NativeMouseButtons::default(),
                    true,
                ));
            app.update();
            app.world_mut()
                .write_message(vmux_input::PointerInput::sample(
                    window,
                    Vec2::new(400.0, 300.0),
                    NativeMouseButtons {
                        left: true,
                        ..Default::default()
                    },
                    true,
                ));
            app.update();
            assert_eq!(app.world().resource::<Dismissals>().0, expected);
        }
        app.world_mut()
            .write_message(vmux_input::PointerInput::sample(
                window,
                Vec2::new(400.0, 300.0),
                NativeMouseButtons::default(),
                true,
            ));
    }
}
