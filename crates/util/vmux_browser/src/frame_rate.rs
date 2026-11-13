use bevy::ecs::system::Commands;
use bevy::window::Window;
use bevy::{
    input::{keyboard::KeyboardInput, mouse::MouseWheel},
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::MessageLoopWakePolicy;
use vmux_api::BinEvent;
use vmux_api::command_bar::CommandBarUiState;
use vmux_ecs::overlay::Overlay;
use vmux_ecs::overlay::WindowOverlay;
use vmux_layout::Browser;
use vmux_layout::{Header, LayoutCef, side_sheet::SideSheet, state::LayoutUiState};

use crate::host::{
    CefPointerRegions, LAYOUT_INPUT_BURST, LayoutFrameRateState, LayoutPointerCapture,
};
use crate::input::{CefHoverPosition, PointerEffect, WindowPointerRegions};
use vmux_ecs::KeyboardOwner;
use vmux_flex::prelude::*;
use vmux_input::{PointerCapture, PointerSet, PointerState, PointerUpdated};
pub(crate) struct FrameRatePlugin;

impl Plugin for FrameRatePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PointerUpdated>()
            .add_observer(request_layout_frame_burst)
            .add_systems(Update, cap_uncapped_webview)
            .add_systems(
                Last,
                (
                    refresh_layout_hover,
                    refresh_active_windowed_hover,
                    sync_layout_cef,
                    keep_asset_replies_moving,
                )
                    .chain()
                    .in_set(PointerSet::Publish),
            );
    }
}

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

fn refresh_layout_hover(
    mut effects: MessageWriter<PointerEffect>,
    pointers: Query<(&Window, &PointerState, &PointerCapture)>,
    mut layouts: Query<(Entity, &HostWindow, &mut CefHoverPosition), With<LayoutCef>>,
    regions: CefPointerRegions,
    captures: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    suppress: Res<CefSuppressPointerInput>,
) {
    for (layout, host, mut hover) in &mut layouts {
        let Ok((window, pointer, capture)) = pointers.get(host.0) else {
            continue;
        };
        let enabled = !suppress.0
            && !pointer.cancelled
            && capture.owner().is_none_or(|owner| owner == layout);
        let position = pointer
            .position_px
            .map(|position| position / window.resolution.scale_factor());
        let inside = enabled
            && (captures.contains(layout)
                || pointer
                    .position_px
                    .is_some_and(|position| regions.contains(host.0, position)));
        if hover.inside == inside && hover.position == position {
            continue;
        }
        if (inside || hover.inside)
            && let Some(position) = position.or(hover.position)
        {
            effects.write(PointerEffect::Move {
                target: layout,
                buttons: pointer.buttons,
                position,
                leave: !inside,
            });
        }
        hover.inside = inside;
        hover.position = position;
    }
}

fn refresh_active_windowed_hover(
    mut effects: MessageWriter<PointerEffect>,
    pointers: Query<(&PointerState, &PointerCapture)>,
    focused_window: vmux_layout::window::FocusedWindow,
    overlay: Overlay,
    mut active_q: Query<
        (
            Entity,
            &Transform,
            &ComputedNode,
            Option<&HostWindow>,
            &mut CefHoverPosition,
        ),
        (
            With<Browser>,
            With<WebviewWindowed>,
            With<KeyboardOwner>,
            Without<LayoutCef>,
            Without<WindowOverlay>,
            Without<Header>,
            Without<SideSheet>,
        ),
    >,
) {
    for (entity, transform, frame, host, mut hover) in &mut active_q {
        let Some(window) = host.map(|host| host.0).or(focused_window.entity()) else {
            hover.position = None;
            continue;
        };
        let Ok((pointer, capture)) = pointers.get(window) else {
            hover.position = None;
            continue;
        };
        if overlay.owns_input()
            || capture.owner().is_some_and(|owner| owner != entity)
            || transform.scale.x <= 1.0e-3
            || frame.is_empty()
        {
            hover.position = None;
            continue;
        }
        let Some(cursor) = pointer.position_px else {
            hover.position = None;
            continue;
        };
        let Some(position) = frame.local_point(cursor) else {
            hover.position = None;
            continue;
        };
        if hover.position == Some(position) {
            continue;
        }
        effects.write(PointerEffect::Move {
            target: entity,
            buttons: pointer.buttons,
            position,
            leave: false,
        });
        hover.position = Some(position);
    }
}

const LAYOUT_IDLE_FRAME_RATE: i32 = 4;
const LAYOUT_ACTIVE_FRAME_RATE: i32 = 60;
const PAGE_MAX_FRAME_RATE: i32 = 60;

fn cap_uncapped_webview(
    uncapped: Query<Entity, (With<WebviewSource>, Without<WebviewMaxFrameRate>)>,
    layouts: Query<Entity, (With<LayoutCef>, Without<LayoutFrameRateState>)>,
    hover: Query<Entity, (With<Browser>, Without<CefHoverPosition>)>,
    mut commands: Commands,
) {
    for entity in &hover {
        commands.entity(entity).insert(CefHoverPosition::default());
    }
    for entity in &uncapped {
        commands
            .entity(entity)
            .insert(WebviewMaxFrameRate(PAGE_MAX_FRAME_RATE));
    }
    for entity in &layouts {
        commands
            .entity(entity)
            .insert(LayoutFrameRateState::default());
    }
}

fn request_layout_frame_burst(
    trigger: On<BinHostEmitEvent>,
    mut layouts: Query<
        (&mut WebviewMaxFrameRate, Option<&mut LayoutFrameRateState>),
        With<LayoutCef>,
    >,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    wake_policy: Option<Res<MessageLoopWakePolicy>>,
) {
    if trigger.id() != LayoutUiState::id() && trigger.id() != CommandBarUiState::id() {
        return;
    }
    let Ok((mut cap, state)) = layouts.get_mut(trigger.webview()) else {
        return;
    };
    let now = std::time::Instant::now();
    cap.0 = LAYOUT_ACTIVE_FRAME_RATE;
    browsers.set_windowless_frame_rate(&trigger.webview(), LAYOUT_ACTIVE_FRAME_RATE);
    match state {
        Some(mut state) => state.last_emit = Some(now),
        None => {
            commands
                .entity(trigger.webview())
                .insert(LayoutFrameRateState {
                    last_emit: Some(now),
                    ..default()
                });
        }
    }
    if let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
    if let Some(wake_policy) = wake_policy {
        wake_policy.begin_interaction();
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

fn sync_layout_cef(
    mut pointer_events: MessageReader<PointerUpdated>,
    mut wheel_events: MessageReader<MouseWheel>,
    mut key_events: MessageReader<KeyboardInput>,
    pointers: Query<(&PointerState, &WindowPointerRegions)>,
    mut layout_q: Query<
        (
            &HostWindow,
            &mut WebviewMaxFrameRate,
            &mut LayoutFrameRateState,
            Has<KeyboardOwner>,
        ),
        With<LayoutCef>,
    >,
    focused_window: vmux_layout::window::FocusedWindow,
) {
    let events = pointer_events.read().copied().collect::<Vec<_>>();
    let wheels = wheel_events
        .read()
        .map(|event| event.window)
        .collect::<Vec<_>>();
    let keys = key_events
        .read()
        .map(|event| event.window)
        .collect::<Vec<_>>();
    let now = std::time::Instant::now();
    for (host, mut cap, mut state, owns_keyboard) in &mut layout_q {
        let focused = Some(host.0) == focused_window.entity();
        let (pointer, regions) = match pointers.get(host.0) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let inside = regions.layout;
        let button_changed = events
            .iter()
            .any(|event| event.window == host.0 && event.previous_buttons != event.buttons);
        let cursor_moved = events
            .iter()
            .any(|event| event.window == host.0 && event.motion);
        let wheel_changed = wheels.contains(&host.0);
        let key_changed = keys.contains(&host.0);
        let any_pressed = pointer.buttons.left || pointer.buttons.right || pointer.buttons.middle;
        let native_changed = focused && {
            if pointer.sequence == state.native_sequence {
                false
            } else {
                state.native_sequence = pointer.sequence;
                true
            }
        };
        let input_changed = native_changed || cursor_moved || button_changed || wheel_changed;
        if focused
            && ((inside || state.dragging_layout) && (button_changed || wheel_changed)
                || (owns_keyboard && key_changed))
        {
            state.last_input = Some(now);
        }
        if focused {
            if !any_pressed {
                state.dragging_layout = false;
            } else if inside && input_changed {
                state.dragging_layout = true;
            }
        } else {
            state.dragging_layout = false;
        }
        let target = if focused {
            layout_frame_rate(
                now,
                state.last_input.max(state.last_emit),
                state.dragging_layout,
            )
        } else {
            LAYOUT_IDLE_FRAME_RATE
        };
        if cap.0 != target {
            cap.0 = target;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::command_bar::CommandBarOpenEvent;

    #[vmux_api::host_event]
    struct OtherEvent;

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
            .add_observer(request_layout_frame_burst);
        app.world_mut().insert_non_send(Browsers::default());
        let wake_policy = MessageLoopWakePolicy::default();
        wake_policy.set_min_wake_interval(std::time::Duration::from_millis(250));
        app.insert_resource(wake_policy.clone());
        let other = app.world_mut().spawn_empty().id();
        app.world_mut()
            .trigger(BinHostEmitEvent::from_event(other, &OtherEvent));
        assert!(app.world().get::<LayoutFrameRateState>(other).is_none());

        let layout = app
            .world_mut()
            .spawn((LayoutCef, WebviewMaxFrameRate(LAYOUT_IDLE_FRAME_RATE)))
            .id();
        app.world_mut().trigger(BinHostEmitEvent::from_event(
            layout,
            &LayoutUiState::default(),
        ));
        app.update();
        assert!(
            app.world()
                .get::<LayoutFrameRateState>(layout)
                .and_then(|state| state.last_emit)
                .is_some()
        );
        assert_eq!(
            app.world().get::<WebviewMaxFrameRate>(layout).unwrap().0,
            LAYOUT_ACTIVE_FRAME_RATE
        );
        assert_eq!(
            wake_policy.min_wake_interval(),
            std::time::Duration::from_millis(16)
        );

        app.world_mut()
            .entity_mut(layout)
            .insert(WebviewMaxFrameRate(LAYOUT_IDLE_FRAME_RATE));
        app.world_mut().trigger(BinHostEmitEvent::from_event(
            layout,
            &CommandBarUiState {
                snapshot: CommandBarOpenEvent::default(),
                ..Default::default()
            },
        ));
        assert_eq!(
            app.world().get::<WebviewMaxFrameRate>(layout).unwrap().0,
            LAYOUT_ACTIVE_FRAME_RATE
        );
    }
}
