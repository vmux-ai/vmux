//! Getting the window's pointer and keyboard into the right webview.
//!
//! Bevy sees the events first, so everything here runs after `InputSystems` and decides who the
//! event was meant for before forwarding it. The chain matters: the pointer target is resolved
//! before any click is delivered against it, and a click that lands outside the command bar has
//! to dismiss it rather than reach the page underneath.

use bevy::{
    ecs::relationship::Relationship,
    input::{
        ButtonState, InputSystems,
        keyboard::KeyboardInput,
        mouse::{MouseButtonInput, MouseWheel},
    },
    prelude::*,
    window::{CursorMoved, PrimaryWindow},
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
        app.init_resource::<RecentBrowserInteraction>()
            .add_systems(
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
    if !OverlayState::of_any(&overlay_q).owns_input() {
        return;
    }
    for event in events.read() {
        if event.state == ButtonState::Pressed {
            bevy::log::info!(key = ?event.key_code, repeat = event.repeat, "command bar keyboard received");
        }
    }
}

/// Publish whether the pointer is inside one of the layout's interactive regions.
///
/// The atomic is the whole output, and it is read off the Bevy thread: the AppKit monitor asks it
/// whether a scroll should wake the loop, and `sync_winit_power_mode` how hard to idle.
///
/// This used to also put a `CefPointerTarget` on the layout, from the days when the layout was an
/// offscreen browser and the marker told CEF where to forward a wheel event. Nothing forwarded
/// one by the end, and nothing but this function ever read the marker back.
fn publish_layout_pointer_inside(
    windows: Query<&Window, With<PrimaryWindow>>,
    layout_q: Query<(), With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
) {
    if layout_q.single().is_err() {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        return;
    }
    #[cfg(target_os = "macos")]
    let inside = !pointer_capture_q.is_empty()
        || windows
            .single()
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
    let inside = !pointer_capture_q.is_empty()
        || windows
            .single()
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
    layout_q: Query<Entity, With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    mut was_in_region: Local<bool>,
) {
    if suppress.0 {
        for _ in events.read() {}
        *was_in_region = false;
        return;
    }
    let Ok(layout) = layout_q.single() else {
        for _ in events.read() {}
        *was_in_region = false;
        return;
    };
    for event in events.read() {
        let in_region = !pointer_capture_q.is_empty()
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
    layout_q: Query<Entity, With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    mut captured: Local<bool>,
) {
    if suppress.0 {
        for _ in events.read() {}
        *captured = false;
        return;
    }
    let Ok(layout) = layout_q.single() else {
        for _ in events.read() {}
        *captured = false;
        return;
    };
    for event in events.read() {
        let Some(button) = pointer_button_from_mouse_button(event.button) else {
            continue;
        };
        let Ok(window) = windows.get(event.window) else {
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
        let inside =
            !pointer_capture_q.is_empty() || cef_pointer_regions_contains(position, &cef_regions);
        if event.state == ButtonState::Pressed && inside {
            *captured = true;
        }
        if inside || *captured {
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
            *captured = false;
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct RecentBrowserInteraction {
    pub(crate) stack: Option<Entity>,
    pub(crate) at: Option<std::time::Instant>,
}

impl RecentBrowserInteraction {
    pub(crate) fn active(&self, stack: Entity) -> bool {
        self.stack == Some(stack)
            && self
                .at
                .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(2))
    }
}

fn track_browser_interaction(
    mut mouse_buttons: MessageReader<MouseButtonInput>,
    mut mouse_wheels: MessageReader<MouseWheel>,
    mut keyboard: MessageReader<KeyboardInput>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    browsers: Query<&ChildOf, With<Browser>>,
    mut recent: ResMut<RecentBrowserInteraction>,
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
        recent.stack = Some(stack);
        recent.at = Some(std::time::Instant::now());
    }
}
