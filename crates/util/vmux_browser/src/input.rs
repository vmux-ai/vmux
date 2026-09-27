use bevy::{
    ecs::relationship::Relationship,
    input::{
        ButtonState, InputSystems,
        keyboard::KeyboardInput,
        mouse::{MouseButtonInput, MouseWheel},
    },
    prelude::*,
    window::CursorMoved,
};
use bevy_cef::prelude::*;
use std::sync::atomic::Ordering;
use vmux_core::overlay::{OverlayState, OverlayStateQuery};
use vmux_layout::Browser;
use vmux_layout::LayoutCef;

use crate::{
    CefPointerRegionQuery, LayoutPointerCapture, NATIVE_LAYOUT_POINTER_INSIDE,
    cef_pointer_regions_contains, pointer_button_from_mouse_button,
};

pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            (
                publish_layout_pointer_inside,
                forward_layout_cef_cursor_move.run_if(on_message::<CursorMoved>),
                forward_layout_cef_mouse_button.run_if(on_message::<MouseButtonInput>),
            )
                .chain()
                .after(InputSystems),
        )
        .add_systems(PreUpdate, log_command_bar_keyboard_input)
        .add_systems(Update, track_browser_interaction);
    }
}

fn log_command_bar_keyboard_input(
    mut events: MessageReader<KeyboardInput>,
    overlay_q: OverlayStateQuery,
) {
    if !OverlayState::from_query(&overlay_q).owns_input() {
        return;
    }
    for event in events.read() {
        if event.state == ButtonState::Pressed {
            bevy::log::info!(key = ?event.key_code, repeat = event.repeat, "command bar keyboard received");
        }
    }
}

fn publish_layout_pointer_inside(
    windows: Query<&Window>,
    focused_window: Res<vmux_layout::window::FocusedWindow>,
    layout_q: Query<(Entity, &HostWindow), With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
) {
    let Some(window_entity) = focused_window.0 else {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        return;
    };
    let Some(layout) = layout_q
        .iter()
        .find_map(|(entity, host)| (host.0 == window_entity).then_some(entity))
    else {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        return;
    };
    #[cfg(target_os = "macos")]
    let inside = pointer_capture_q.contains(layout)
        || windows
            .get(window_entity)
            .ok()
            .and_then(|window| {
                let scale = window.resolution.scale_factor();
                (scale.is_finite() && scale > 0.0).then_some(scale)
            })
            .and_then(|scale| {
                vmux_layout::native_pointer::snapshot().map(|pointer| pointer.position_px / scale)
            })
            .is_some_and(|position| cef_pointer_regions_contains(position, &cef_regions));
    #[cfg(not(target_os = "macos"))]
    let inside = pointer_capture_q.contains(layout)
        || windows
            .get(window_entity)
            .ok()
            .and_then(Window::cursor_position)
            .is_some_and(|pos| cef_pointer_regions_contains(pos, &cef_regions));
    NATIVE_LAYOUT_POINTER_INSIDE.store(inside, Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
fn forward_layout_cef_cursor_move(mut events: MessageReader<CursorMoved>) {
    for _ in events.read() {}
}

#[cfg(not(target_os = "macos"))]
fn forward_layout_cef_cursor_move(
    mut events: MessageReader<CursorMoved>,
    buttons: Res<ButtonInput<MouseButton>>,
    suppress: Res<CefSuppressPointerInput>,
    browsers: NonSend<Browsers>,
    layout_q: Query<(Entity, &HostWindow), With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    mut was_in_region: Local<bool>,
) {
    if suppress.0 {
        for _ in events.read() {}
        *was_in_region = false;
        return;
    }
    for event in events.read() {
        let Some(layout) = layout_q
            .iter()
            .find_map(|(entity, host)| (host.0 == event.window).then_some(entity))
        else {
            *was_in_region = false;
            continue;
        };
        let in_region = pointer_capture_q.contains(layout)
            || cef_pointer_regions_contains(event.position, &cef_regions);
        if in_region {
            browsers.send_mouse_move(&layout, buttons.get_pressed(), event.position, false);
        } else if *was_in_region {
            browsers.send_mouse_move(&layout, buttons.get_pressed(), event.position, true);
        }
        *was_in_region = in_region;
    }
}

fn forward_layout_cef_mouse_button(
    mut events: MessageReader<MouseButtonInput>,
    windows: Query<&Window>,
    suppress: Res<CefSuppressPointerInput>,
    browsers: NonSend<Browsers>,
    layout_q: Query<(Entity, &HostWindow), With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    mut captured: Local<Option<Entity>>,
) {
    if suppress.0 {
        for _ in events.read() {}
        *captured = None;
        return;
    }
    for event in events.read() {
        let Some(button) = pointer_button_from_mouse_button(event.button) else {
            continue;
        };
        let Ok(window) = windows.get(event.window) else {
            continue;
        };
        let Some(layout) = layout_q
            .iter()
            .find_map(|(entity, host)| (host.0 == event.window).then_some(entity))
        else {
            continue;
        };
        #[cfg(target_os = "macos")]
        let native_pointer = vmux_layout::native_pointer::snapshot();
        #[cfg(target_os = "macos")]
        let position = native_pointer
            .map(|pointer| pointer.position_px / window.resolution.scale_factor())
            .or_else(|| window.cursor_position());
        #[cfg(not(target_os = "macos"))]
        let position = window.cursor_position();
        let Some(position) = position else {
            continue;
        };
        let inside = pointer_capture_q.contains(layout)
            || cef_pointer_regions_contains(position, &cef_regions);
        if event.state == ButtonState::Pressed && inside {
            *captured = Some(layout);
        }
        if inside || *captured == Some(layout) {
            #[cfg(target_os = "macos")]
            if let Some(pointer) = native_pointer {
                browsers.send_native_mouse_move(&layout, pointer.buttons, position, !inside);
            }
            browsers.send_mouse_click(
                &layout,
                position,
                button,
                event.state == ButtonState::Released,
            );
        }
        if event.state == ButtonState::Released {
            *captured = None;
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

fn track_browser_interaction(
    mut mouse_buttons: MessageReader<MouseButtonInput>,
    mut mouse_wheels: MessageReader<MouseWheel>,
    mut keyboard: MessageReader<KeyboardInput>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    browsers: Query<&ChildOf, With<Browser>>,
    mut commands: Commands,
) {
    let interacted = mouse_buttons
        .read()
        .any(|event| event.state == ButtonState::Pressed)
        || mouse_wheels.read().next().is_some()
        || keyboard
            .read()
            .any(|event| event.state == ButtonState::Pressed);
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
