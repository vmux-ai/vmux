//! Refreshing what the pointer is over, and deciding how fast the layout webview redraws.
//!
//! Both run in `Last`, after everything that could have moved a pane or emitted to the page, so
//! the hover state and the frame rate describe the frame that just happened rather than the one
//! before it. The webview idles slowly and bursts back to full rate when the host emits to it.

use bevy::{
    input::{
        keyboard::KeyboardInput,
        mouse::{MouseButton, MouseButtonInput, MouseWheel},
    },
    prelude::*,
    window::{CursorMoved, PrimaryWindow},
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use std::sync::atomic::Ordering;
use vmux_command::event::LAYOUT_COMMAND_BAR_OPEN_EVENT;
use vmux_core::overlay::WindowOverlay;
use vmux_core::overlay::{OverlayState, OverlayStateQuery};
use vmux_layout::Browser;
use vmux_layout::{
    Header, LayoutCef,
    event::{STACKS_EVENT, TABS_EVENT},
    side_sheet::SideSheet,
};

#[cfg(not(target_os = "macos"))]
use crate::{
    CefPointerRegionQuery, LayoutHoverRefreshState, LayoutPointerCapture,
    cef_pointer_regions_contains, reset_layout_cef_hover,
};
use crate::{
    LAYOUT_INPUT_BURST, LayoutFrameRateState, NATIVE_LAYOUT_POINTER_INSIDE, NativeLayout,
    WindowedHoverRefreshState, native_left_mouse_down,
};
use vmux_flex::prelude::*;
pub(crate) struct FrameRatePlugin;

impl Plugin for FrameRatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LayoutFrameRateBurst>()
            .add_observer(request_layout_frame_burst)
            .add_systems(
                Last,
                (
                    refresh_layout_cef_hover,
                    refresh_active_windowed_hover,
                    sync_layout_cef_frame_rate,
                    keep_asset_replies_moving,
                )
                    .chain(),
            );
    }
}

/// Asks for another frame while a page is still waiting for an asset.
///
/// A page's `vmux://` request is answered by a Bevy system, so the reply needs the schedule to
/// run: once to pick the request up, and again to notice the load finished. Requesting the frame
/// covers the first; nothing covered the second, so a stylesheet asked for from an idle app
/// waited for whatever frame happened along next — measured between 170ms and 780ms, with both
/// stylesheets released on the same one.
fn keep_asset_replies_moving(
    pending: Query<(), With<CefResponseHandle>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    if pending.is_empty() {
        return;
    }
    if let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

/// Lowers the hover flag for the frames where nothing can be hovered.
///
/// The AppKit monitor samples the pointer itself, so `sync_layout_cef_pointer_target` already
/// answers the ordinary frame from that sample; what it does not know about is the host
/// suppressing pointer input, which is why this still runs.
#[cfg(target_os = "macos")]
fn refresh_layout_cef_hover(
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    suppress: Res<CefSuppressPointerInput>,
    layout_q: Query<Entity, With<LayoutCef>>,
    modal_pointer_targets: Query<(), (With<WindowOverlay>, With<CefPointerTarget>)>,
) {
    if layout_q.single().is_err() || suppress.0 || !modal_pointer_targets.is_empty() {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        return;
    }
    let Ok(window_entity) = primary_window.single() else {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        return;
    };
    let scale = window.resolution.scale_factor();
    if !scale.is_finite() || scale <= 0.0 {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
    }
}

#[cfg(not(target_os = "macos"))]
fn refresh_layout_cef_hover(
    browsers: NonSend<Browsers>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    suppress: Res<CefSuppressPointerInput>,
    layout_q: Query<Entity, With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    modal_pointer_targets: Query<(), (With<WindowOverlay>, With<CefPointerTarget>)>,
    mut state: Local<LayoutHoverRefreshState>,
) {
    let Ok(layout) = layout_q.single() else {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        *state = LayoutHoverRefreshState::default();
        return;
    };
    if suppress.0 || !modal_pointer_targets.is_empty() {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        reset_layout_cef_hover(&browsers, &buttons, layout, &mut state);
        return;
    }
    let Ok(window_entity) = primary_window.single() else {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        reset_layout_cef_hover(&browsers, &buttons, layout, &mut state);
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        reset_layout_cef_hover(&browsers, &buttons, layout, &mut state);
        return;
    };
    let scale = window.resolution.scale_factor();
    if !scale.is_finite() || scale <= 0.0 {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        reset_layout_cef_hover(&browsers, &buttons, layout, &mut state);
        return;
    }
    let pointer_capture = !pointer_capture_q.is_empty();
    let Some(cursor_px) = vmux_layout::pane::pane_hover_cursor_position(window_entity, window)
    else {
        reset_layout_cef_hover(&browsers, &buttons, layout, &mut state);
        return;
    };
    let sequence = 0;
    let position = cursor_px / scale;
    let in_region = pointer_capture || cef_pointer_regions_contains(position, &cef_regions);
    NATIVE_LAYOUT_POINTER_INSIDE.store(in_region, Ordering::Relaxed);
    let unchanged = state.sequence == sequence
        && state.position == Some(position)
        && state.in_region == in_region;
    if unchanged {
        return;
    }
    if in_region {
        browsers.send_mouse_move(&layout, buttons.get_pressed(), position, false);
    } else if state.in_region {
        browsers.send_mouse_move(&layout, buttons.get_pressed(), position, true);
    }
    state.sequence = sequence;
    state.position = Some(position);
    state.in_region = in_region;
}

fn refresh_active_windowed_hover(
    browsers: NonSend<Browsers>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    overlay_q: OverlayStateQuery,
    active_q: Query<
        (Entity, &Transform, &ComputedNode, Option<&HostWindow>),
        (
            With<Browser>,
            With<WebviewWindowed>,
            With<CefKeyboardTarget>,
            Without<LayoutCef>,
            Without<WindowOverlay>,
            Without<Header>,
            Without<SideSheet>,
        ),
    >,
    mut state: Local<WindowedHoverRefreshState>,
) {
    if OverlayState::of_any(&overlay_q).owns_input() {
        *state = WindowedHoverRefreshState::default();
        return;
    }
    if native_left_mouse_down() {
        *state = WindowedHoverRefreshState::default();
        return;
    }
    let Some((entity, transform, &frame, host_window)) = active_q.iter().next() else {
        *state = WindowedHoverRefreshState::default();
        return;
    };
    if transform.scale.x <= 1.0e-3 {
        *state = WindowedHoverRefreshState::default();
        return;
    }
    let Some(window_entity) = host_window
        .map(|host| host.0)
        .or_else(|| primary_window.single().ok())
    else {
        *state = WindowedHoverRefreshState::default();
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        *state = WindowedHoverRefreshState::default();
        return;
    };
    let Some(cursor_px) = vmux_layout::pane::pane_hover_cursor_position(window_entity, window)
    else {
        *state = WindowedHoverRefreshState::default();
        return;
    };
    if frame.is_empty() {
        *state = WindowedHoverRefreshState::default();
        return;
    }
    let Some(position) = frame.local_point(cursor_px) else {
        *state = WindowedHoverRefreshState::default();
        return;
    };
    if state.entity == Some(entity) && state.position == Some(position) {
        return;
    }
    browsers.send_mouse_move(&entity, buttons.get_pressed(), position, false);
    state.entity = Some(entity);
    state.position = Some(position);
}

const LAYOUT_IDLE_FRAME_RATE: i32 = 10;
const LAYOUT_ACTIVE_FRAME_RATE: i32 = 60;
#[derive(Resource, Default)]
struct LayoutFrameRateBurst {
    pub(crate) last_emit: Option<std::time::Instant>,
}

fn request_layout_frame_burst(
    trigger: On<BinHostEmitEvent>,
    mut layouts: Query<&mut WebviewMaxFrameRate, With<LayoutCef>>,
    browsers: NonSend<Browsers>,
    mut burst: ResMut<LayoutFrameRateBurst>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    if !matches!(
        trigger.id.as_str(),
        TABS_EVENT | STACKS_EVENT | LAYOUT_COMMAND_BAR_OPEN_EVENT
    ) {
        return;
    }
    let Ok(mut cap) = layouts.get_mut(trigger.webview) else {
        return;
    };
    cap.0 = LAYOUT_ACTIVE_FRAME_RATE;
    browsers.set_windowless_frame_rate(&trigger.webview, LAYOUT_ACTIVE_FRAME_RATE);
    burst.last_emit = Some(std::time::Instant::now());
    if let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn layout_frame_rate(
    now: std::time::Instant,
    last_input: Option<std::time::Instant>,
    dragging: bool,
) -> i32 {
    if dragging
        || last_input.is_some_and(|last| now.saturating_duration_since(last) < LAYOUT_INPUT_BURST)
    {
        LAYOUT_ACTIVE_FRAME_RATE
    } else {
        LAYOUT_IDLE_FRAME_RATE
    }
}

fn sync_layout_cef_frame_rate(
    mut cursor_events: MessageReader<CursorMoved>,
    mut button_events: MessageReader<MouseButtonInput>,
    mut wheel_events: MessageReader<MouseWheel>,
    mut key_events: MessageReader<KeyboardInput>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut layout_q: Query<(&mut WebviewMaxFrameRate, Has<CefKeyboardTarget>), With<LayoutCef>>,
    burst: Res<LayoutFrameRateBurst>,
    mut state: Local<LayoutFrameRateState>,
) {
    let owns_keyboard = layout_q.iter().any(|(_, keyboard_target)| keyboard_target);
    let inside = NativeLayout::pointer_is_inside();
    let pointer = vmux_layout::native_pointer::snapshot();
    let native_changed = pointer.is_some_and(|pointer| {
        if pointer.sequence == state.native_sequence {
            return false;
        }
        state.native_sequence = pointer.sequence;
        true
    });
    let pointer_moved = native_changed || cursor_events.read().count() > 0;
    let button_changed = button_events.read().count() > 0;
    let wheel_changed = wheel_events.read().count() > 0;
    // Typing into the command bar panel or the bookmark field moves no pointer, so without this
    // the layout webview paints those keystrokes at the idle rate and they look dropped.
    let key_changed = key_events.read().count() > 0;
    let input_changed = pointer_moved || button_changed || wheel_changed;
    let now = std::time::Instant::now();
    if (inside || state.dragging_layout) && (button_changed || wheel_changed)
        || (owns_keyboard && key_changed)
    {
        state.last_input = Some(now);
    }
    let native_dragging = pointer.is_some_and(|pointer| {
        pointer.buttons.left || pointer.buttons.right || pointer.buttons.middle
    });
    let any_pressed = native_dragging || buttons.get_pressed().next().is_some();
    if !any_pressed {
        state.dragging_layout = false;
    } else if inside && input_changed {
        state.dragging_layout = true;
    }
    let desired = layout_frame_rate(
        now,
        state.last_input.max(burst.last_emit),
        state.dragging_layout,
    );
    let Ok((mut cap, _)) = layout_q.single_mut() else {
        return;
    };
    if cap.0 != desired {
        cap.0 = desired;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_layout::event::PANE_TREE_EVENT;

    #[test]
    fn layout_frame_rate_bursts_after_input() {
        let now = std::time::Instant::now();
        assert_eq!(layout_frame_rate(now, None, false), LAYOUT_IDLE_FRAME_RATE);
        assert_eq!(
            layout_frame_rate(now, Some(now), false),
            LAYOUT_ACTIVE_FRAME_RATE
        );
        assert_eq!(layout_frame_rate(now, None, true), LAYOUT_ACTIVE_FRAME_RATE);
        assert_eq!(
            layout_frame_rate(now, Some(now - LAYOUT_INPUT_BURST), false),
            LAYOUT_IDLE_FRAME_RATE
        );
    }

    #[test]
    fn layout_host_emit_requests_frame_burst() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<LayoutFrameRateBurst>()
            .add_observer(request_layout_frame_burst);
        app.world_mut().insert_non_send(Browsers::default());
        let other = app.world_mut().spawn_empty().id();
        app.world_mut()
            .trigger(BinHostEmitEvent::from_bytes(other, "other", Vec::new()));
        assert!(
            app.world()
                .resource::<LayoutFrameRateBurst>()
                .last_emit
                .is_none()
        );

        let layout = app
            .world_mut()
            .spawn((LayoutCef, WebviewMaxFrameRate(LAYOUT_IDLE_FRAME_RATE)))
            .id();
        app.world_mut().trigger(BinHostEmitEvent::from_bytes(
            layout,
            PANE_TREE_EVENT,
            Vec::new(),
        ));
        assert!(
            app.world()
                .resource::<LayoutFrameRateBurst>()
                .last_emit
                .is_none()
        );
        assert_eq!(
            app.world().get::<WebviewMaxFrameRate>(layout).unwrap().0,
            LAYOUT_IDLE_FRAME_RATE
        );
        app.world_mut()
            .trigger(BinHostEmitEvent::from_bytes(layout, "tabs", Vec::new()));
        assert!(
            app.world()
                .resource::<LayoutFrameRateBurst>()
                .last_emit
                .is_some()
        );
        assert_eq!(
            app.world().get::<WebviewMaxFrameRate>(layout).unwrap().0,
            LAYOUT_ACTIVE_FRAME_RATE
        );

        // The command bar panel animates open on the layout surface, so it has to burst too or
        // the reveal plays at the idle rate.
        app.world_mut()
            .entity_mut(layout)
            .insert(WebviewMaxFrameRate(LAYOUT_IDLE_FRAME_RATE));
        app.world_mut().trigger(BinHostEmitEvent::from_bytes(
            layout,
            LAYOUT_COMMAND_BAR_OPEN_EVENT,
            Vec::new(),
        ));
        assert_eq!(
            app.world().get::<WebviewMaxFrameRate>(layout).unwrap().0,
            LAYOUT_ACTIVE_FRAME_RATE
        );
    }
}
