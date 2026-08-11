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
use vmux_command::event::CommandBarActionEvent;
use vmux_layout::Browser;
use vmux_layout::command_bar::state::CommandBarStateQuery;
use vmux_layout::{LayoutCef, window::Modal};

use crate::{
    CefPointerRegionQuery, LayoutPointerCapture, NATIVE_LAYOUT_POINTER_INSIDE,
    cef_pointer_regions_contains, command_bar_windowed_click_should_dismiss,
    native_command_bar_route, pointer_button_from_mouse_button,
    take_native_command_bar_dismiss_requested,
};

pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RecentBrowserInteraction>()
            .add_systems(
                PreUpdate,
                (
                    sync_layout_cef_pointer_target,
                    dismiss_command_bar_from_native_monitor,
                    dismiss_windowed_command_bar_on_outside_click
                        .run_if(on_message::<MouseButtonInput>),
                    forward_layout_cef_cursor_move.run_if(on_message::<CursorMoved>),
                    forward_layout_cef_mouse_button.run_if(on_message::<MouseButtonInput>),
                )
                    .chain()
                    .after(InputSystems),
            )
            .add_systems(
                PreUpdate,
                log_command_bar_keyboard_input.after(bevy_cef::prelude::CefKeyboardInputSet),
            )
            .add_systems(Update, track_browser_interaction);
    }
}

fn log_command_bar_keyboard_input(
    mut events: MessageReader<KeyboardInput>,
    modal_q: CommandBarStateQuery,
) {
    if !vmux_layout::command_bar::handler::is_command_bar_open(&modal_q) {
        return;
    }
    for event in events.read() {
        if event.state == ButtonState::Pressed {
            bevy::log::info!(key = ?event.key_code, repeat = event.repeat, "command bar keyboard received");
        }
    }
}
fn sync_layout_cef_pointer_target(
    windows: Query<&Window, With<PrimaryWindow>>,
    layout_q: Query<(Entity, Has<CefPointerTarget>), With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    modal_pointer_targets: Query<(), (With<Modal>, With<CefPointerTarget>)>,
    mut commands: Commands,
) {
    let Ok((layout, has_target)) = layout_q.single() else {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        return;
    };
    #[cfg(target_os = "macos")]
    let should_target = {
        let inside = !pointer_capture_q.is_empty()
            || windows
                .single()
                .ok()
                .and_then(|window| {
                    let scale = window.resolution.scale_factor();
                    (scale.is_finite() && scale > 0.0).then_some(scale)
                })
                .and_then(|scale| {
                    vmux_layout::native_pointer::snapshot()
                        .map(|pointer| pointer.position_px / scale)
                })
                .is_some_and(|position| cef_pointer_regions_contains(position, &cef_regions));
        modal_pointer_targets.is_empty() && inside
    };
    #[cfg(not(target_os = "macos"))]
    let should_target = modal_pointer_targets.is_empty()
        && (!pointer_capture_q.is_empty()
            || windows
                .single()
                .ok()
                .and_then(Window::cursor_position)
                .is_some_and(|pos| cef_pointer_regions_contains(pos, &cef_regions)));
    NATIVE_LAYOUT_POINTER_INSIDE.store(should_target, Ordering::Relaxed);
    if should_target && !has_target {
        commands.entity(layout).insert(CefPointerTarget);
    } else if !should_target && has_target {
        commands.entity(layout).remove::<CefPointerTarget>();
    }
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
    modal_pointer_targets: Query<(), (With<Modal>, With<CefPointerTarget>)>,
    mut was_in_region: Local<bool>,
) {
    if suppress.0 || !modal_pointer_targets.is_empty() {
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
    modal_pointer_targets: Query<(), (With<Modal>, With<CefPointerTarget>)>,
    mut captured: Local<bool>,
) {
    if suppress.0 || !modal_pointer_targets.is_empty() {
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
fn dismiss_windowed_command_bar_on_outside_click(
    mut events: MessageReader<MouseButtonInput>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    modal_q: Query<(Entity, Option<&HostWindow>), (With<Modal>, With<WebviewWindowed>)>,
    mut commands: Commands,
) {
    let Ok((modal_e, host_window)) = modal_q.single() else {
        for _ in events.read() {}
        return;
    };
    // Read the same published rectangle the AppKit monitor tests against. Recomputing it here
    // gave one click two different answers depending on which path saw it first.
    let route = native_command_bar_route();
    let window_entity = host_window
        .map(|h| h.0)
        .or_else(|| primary_window.single().ok());
    let Some(window_entity) = window_entity else {
        for _ in events.read() {}
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        for _ in events.read() {}
        return;
    };
    for event in events.read() {
        if event.window != window_entity {
            continue;
        }
        let cursor = window
            .physical_cursor_position()
            .map(|pos| Vec2::new(pos.x, pos.y));
        if command_bar_windowed_click_should_dismiss(
            route.owns_input,
            event.button,
            event.state,
            cursor,
            route.frame,
        ) {
            commands.trigger(BinReceive::<CommandBarActionEvent> {
                webview: modal_e,
                payload: CommandBarActionEvent {
                    action: "dismiss".to_string(),
                    value: String::new(),
                    target: None,
                    target_url: None,
                    attachments: Vec::new(),
                },
            });
            break;
        }
    }
}
fn dismiss_command_bar_from_native_monitor(
    modal_q: Query<Entity, With<Modal>>,
    mut commands: Commands,
) {
    if !take_native_command_bar_dismiss_requested() {
        return;
    }
    let Ok(modal_e) = modal_q.single() else {
        return;
    };
    commands.trigger(BinReceive::<CommandBarActionEvent> {
        webview: modal_e,
        payload: CommandBarActionEvent {
            action: "dismiss".to_string(),
            value: String::new(),
            target: None,
            target_url: None,
            attachments: Vec::new(),
        },
    });
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
