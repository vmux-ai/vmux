use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy::window::{Monitor, Window};
use bevy::winit::{EventLoopProxyWrapper, UpdateMode, WinitSettings, WinitUserEvent};
use bevy_cef_core::prelude::{
    Browsers, MessageLoopWakePolicy, windowless_frame_interval_from_refresh_millihertz,
};
#[cfg(target_os = "macos")]
use std::ptr::NonNull;
#[cfg(target_os = "macos")]
use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
#[cfg(target_os = "macos")]
use std::time::Instant;

use vmux_layout::scene::InteractionMode;
#[cfg(feature = "tray")]
use vmux_terminal as terminal;
#[cfg(feature = "tray")]
use vmux_terminal::{PtyExited, Terminal};

const FOCUSED_FRAME_INTERVAL: Duration = Duration::from_secs(1);
const UNFOCUSED_FRAME_INTERVAL: Duration = Duration::from_secs(1);
const HIDDEN_FRAME_INTERVAL: Duration = Duration::from_secs(60);
const BACKGROUND_CEF_WAKE_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(target_os = "macos")]
const NATIVE_MOUSE_MOVE_WAKE_INTERVAL: Duration = Duration::from_millis(33);
#[cfg(target_os = "macos")]
const NATIVE_MOUSE_DRAG_WAKE_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Message, Debug, Clone, Copy)]
pub enum LifecycleEvent {
    HideAllWindows,
    #[cfg(feature = "tray")]
    ShowAllWindows,
    #[cfg(feature = "tray")]
    QuitVmux,
}

pub struct BackgroundLifecyclePlugin;

impl Plugin for BackgroundLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LifecycleEvent>()
            .add_systems(Update, handle_lifecycle_events)
            .add_systems(Update, sync_winit_power_mode.after(handle_lifecycle_events))
            .add_systems(Update, activate_app_during_boot)
            .add_systems(Update, keep_awake_while_revealing)
            .add_systems(
                Update,
                keep_awake_while_command_bar_opening.after(vmux_command::ReadAppCommands),
            )
            .add_systems(Update, grab_key_window_on_pane_hover)
            .add_systems(Last, keep_awake_while_player_active)
            .add_systems(
                Startup,
                (
                    install_native_mouse_wake_monitor,
                    install_live_resize_monitor,
                    activate_primary_window_on_startup,
                ),
            );
    }
}

#[cfg(target_os = "macos")]
static NATIVE_MOUSE_WAKE_MONITOR_INSTALLED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static IN_LIVE_RESIZE: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static LIVE_RESIZE_MONITOR_INSTALLED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static HOVER_OVER_PANE: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
fn activate_primary_window_on_startup(
    primary_window: Query<(Entity, &Window), With<bevy::window::PrimaryWindow>>,
) {
    let Ok((window_entity, window)) = primary_window.single() else {
        return;
    };
    if !window.visible {
        return;
    }
    activate_native_window(window_entity);
}

#[cfg(not(target_os = "macos"))]
fn activate_primary_window_on_startup() {}

#[cfg(target_os = "macos")]
fn grab_key_window_on_pane_hover(
    intent: Res<vmux_browser::HostFocusIntent>,
    primary_window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    panes: Query<
        (&ComputedNode, &bevy::ui::UiGlobalTransform),
        (
            With<vmux_layout::pane::Pane>,
            Without<vmux_layout::pane::PaneSplit>,
        ),
    >,
) {
    if !HOVER_OVER_PANE.swap(false, Ordering::Relaxed) {
        return;
    }
    let Some(pointer) = vmux_layout::native_pointer::snapshot() else {
        return;
    };
    let over_pane = panes.iter().any(|(node, transform)| {
        let center = transform.transform_point2(Vec2::ZERO);
        let half = node.size * 0.5;
        let min = center - half;
        let max = center + half;
        pointer.position_px.x >= min.x
            && pointer.position_px.x <= max.x
            && pointer.position_px.y >= min.y
            && pointer.position_px.y <= max.y
    });
    if !over_pane {
        return;
    }
    if *intent == vmux_browser::HostFocusIntent::Unmanaged {
        return;
    }
    let Ok(window_entity) = primary_window.single() else {
        return;
    };
    ensure_native_window_active(window_entity);
}

#[cfg(not(target_os = "macos"))]
fn grab_key_window_on_pane_hover() {}

#[cfg(target_os = "macos")]
pub(crate) fn activate_native_window(window_entity: Entity) {
    use bevy::winit::WINIT_WINDOWS;
    use objc2_app_kit::{NSApp, NSView};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(winit_window) = winit_windows.get_window(window_entity) else {
            return;
        };
        let Ok(handle) = winit_window.window_handle() else {
            return;
        };
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return;
        };
        let view: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
        let Some(window) = view.window() else {
            return;
        };
        let app = NSApp(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        window.makeKeyAndOrderFront(None);
    });
}

/// Re-assert app activation + window key status, returning `true` once it has taken effect.
///
/// `activateIgnoringOtherApps` is asynchronous: the app does not report active until a later
/// runloop tick, so a single one-shot at reveal does not stick — the window shows but the app
/// stays in the background, and keystrokes (including menu key-equivalents) go nowhere until a
/// click activates the app. Callers retry this each frame until it returns `true`.
#[cfg(target_os = "macos")]
pub(crate) fn ensure_native_window_active(window_entity: Entity) -> bool {
    use bevy::winit::WINIT_WINDOWS;
    use objc2_app_kit::{NSApp, NSView};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return false;
    };
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(winit_window) = winit_windows.get_window(window_entity) else {
            return false;
        };
        let Ok(handle) = winit_window.window_handle() else {
            return false;
        };
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return false;
        };
        let view: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
        let Some(window) = view.window() else {
            return false;
        };
        let app = NSApp(mtm);
        if app.isActive() && window.isKeyWindow() {
            return true;
        }
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        window.makeKeyAndOrderFront(None);
        false
    })
}

/// Stop re-asserting boot activation after this long, so a degenerate case cannot wake the loop
/// forever.
#[cfg(target_os = "macos")]
const APP_ACTIVATION_BUDGET: Duration = Duration::from_secs(10);

/// Bring the app to the foreground (app level only — no window). Returns `true` once active.
#[cfg(target_os = "macos")]
fn activate_app() -> bool {
    use objc2_app_kit::NSApp;

    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return false;
    };
    let app = NSApp(mtm);
    if app.isActive() {
        return true;
    }
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    false
}

/// When launched from a terminal, the launching app stays frontmost and macOS takes ~1-2s to honor
/// our activation request. Start asking the moment boot begins so that latency overlaps the splash
/// wait — by the time the window reveals the app is already active and becoming key is instant,
/// instead of the user watching the UI for a second before keys register.
#[cfg(target_os = "macos")]
fn activate_app_during_boot(
    mut confirmed: Local<bool>,
    mut started_at: Local<Option<Instant>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    if *confirmed {
        return;
    }
    let started = *started_at.get_or_insert_with(Instant::now);
    if activate_app() || started.elapsed() >= APP_ACTIVATION_BUDGET {
        *confirmed = true;
    } else if let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

#[cfg(not(target_os = "macos"))]
fn activate_app_during_boot() {}

#[cfg(target_os = "macos")]
type NativeThrottle = Arc<dyn Fn(Duration) + Send + Sync>;

#[cfg(target_os = "macos")]
fn native_throttle(name: &'static str, action: impl Fn() + Send + 'static) -> NativeThrottle {
    let pending_interval_ns = Arc::new(AtomicU64::new(u64::MAX));
    let thread_pending_interval_ns = Arc::clone(&pending_interval_ns);
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let mut last_fire: Option<Instant> = None;
            while rx.recv().is_ok() {
                let mut interval_ns = thread_pending_interval_ns.swap(u64::MAX, Ordering::AcqRel);
                if interval_ns == u64::MAX {
                    continue;
                }
                loop {
                    let interval = Duration::from_nanos(interval_ns);
                    if let Some(last) = last_fire {
                        let elapsed = Instant::now().saturating_duration_since(last);
                        if elapsed < interval {
                            match rx.recv_timeout(interval - elapsed) {
                                Ok(()) => {
                                    interval_ns = interval_ns.min(
                                        thread_pending_interval_ns.swap(u64::MAX, Ordering::AcqRel),
                                    );
                                    continue;
                                }
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                            }
                        }
                    }
                    action();
                    last_fire = Some(Instant::now());
                    interval_ns = thread_pending_interval_ns.swap(u64::MAX, Ordering::AcqRel);
                    if interval_ns == u64::MAX {
                        break;
                    }
                }
            }
        })
        .unwrap_or_else(|error| panic!("failed to spawn {name}: {error}"));
    Arc::new(move |min_interval: Duration| {
        let min_interval = min_interval.as_nanos().min(u64::MAX as u128) as u64;
        pending_interval_ns.fetch_min(min_interval, Ordering::Relaxed);
        let _ = tx.try_send(());
    })
}

#[cfg(target_os = "macos")]
fn install_native_mouse_wake_monitor(proxy: Option<Res<EventLoopProxyWrapper>>) {
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};

    let Some(proxy) = proxy else {
        return;
    };
    if NATIVE_MOUSE_WAKE_MONITOR_INSTALLED.load(Ordering::Relaxed) {
        return;
    }
    let proxy = (**proxy).clone();
    let wake = native_throttle("native-mouse-wake-throttle", move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
    let flush_layout = native_throttle("native-layout-pointer-throttle", || {
        dispatch2::DispatchQueue::main().exec_async(|| {
            vmux_browser::flush_native_layout_pointer_move();
        });
    });
    let local_wake = wake.clone();
    let local_block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        let ev = unsafe { event.as_ref() };
        let event_type = ev.r#type();
        if event_type == NSEventType::LeftMouseDown {
            vmux_browser::set_native_left_mouse_down(true);
        } else if event_type == NSEventType::LeftMouseUp {
            vmux_browser::set_native_left_mouse_down(false);
        }
        let motion = matches!(
            event_type,
            NSEventType::MouseMoved
                | NSEventType::LeftMouseDragged
                | NSEventType::RightMouseDragged
                | NSEventType::OtherMouseDragged
        );
        let button_event = matches!(
            event_type,
            NSEventType::LeftMouseDown
                | NSEventType::LeftMouseUp
                | NSEventType::RightMouseDown
                | NSEventType::RightMouseUp
                | NSEventType::OtherMouseDown
                | NSEventType::OtherMouseUp
        );
        let location = event_location_in_window_physical_px(ev);
        let buttons = native_mouse_buttons();
        if let Some((x, y)) = location {
            vmux_layout::native_pointer::publish(Vec2::new(x, y), buttons, motion);
        }
        let layout_pointer = (motion || button_event)
            .then(|| {
                location.map(|(x, y)| vmux_browser::queue_native_layout_pointer_move(x, y, buttons))
            })
            .flatten();
        if event_type == NSEventType::LeftMouseDown
            && let Some((x_px, y_px)) = location
        {
            vmux_browser::request_native_command_bar_dismiss_for_mouse_down(x_px, y_px);
        }
        if motion {
            let interval = if event_type == NSEventType::MouseMoved {
                NATIVE_MOUSE_MOVE_WAKE_INTERVAL
            } else {
                NATIVE_MOUSE_DRAG_WAKE_INTERVAL
            };
            if let Some(result) = layout_pointer
                && result.owns_pointer
                && result.presenter_active
            {
                if result.region_changed {
                    vmux_browser::flush_native_layout_pointer_move();
                    local_wake(interval);
                } else if result.pending {
                    flush_layout(interval);
                }
            } else {
                HOVER_OVER_PANE.store(true, Ordering::Relaxed);
                local_wake(interval);
            }
        } else {
            if layout_pointer.is_some_and(|result| {
                result.owns_pointer && result.presenter_active && result.pending
            }) {
                vmux_browser::flush_native_layout_pointer_move();
            }
            local_wake(NATIVE_MOUSE_DRAG_WAKE_INTERVAL);
        }
        if event_type == NSEventType::LeftMouseDragged
            && vmux_layout::native_pointer::layout_drag_active()
        {
            return std::ptr::null_mut();
        }
        event.as_ptr()
    });
    let global_wake = wake.clone();
    let global_block = block2::RcBlock::new(move |event: NonNull<NSEvent>| {
        let event_type = unsafe { event.as_ref() }.r#type();
        if event_type == NSEventType::LeftMouseDown {
            vmux_browser::set_native_left_mouse_down(true);
        } else if event_type == NSEventType::LeftMouseUp {
            vmux_browser::set_native_left_mouse_down(false);
        }
        vmux_layout::native_pointer::publish_buttons(native_mouse_buttons());
        global_wake(NATIVE_MOUSE_MOVE_WAKE_INTERVAL);
    });
    let mouse_mask = NSEventMask::MouseMoved
        | NSEventMask::LeftMouseDown
        | NSEventMask::LeftMouseUp
        | NSEventMask::LeftMouseDragged
        | NSEventMask::RightMouseDown
        | NSEventMask::RightMouseUp
        | NSEventMask::RightMouseDragged
        | NSEventMask::OtherMouseDown
        | NSEventMask::OtherMouseUp
        | NSEventMask::OtherMouseDragged;
    let local_mask = mouse_mask | NSEventMask::ScrollWheel;
    let global_mask = NSEventMask::LeftMouseDown
        | NSEventMask::LeftMouseUp
        | NSEventMask::RightMouseDown
        | NSEventMask::RightMouseUp
        | NSEventMask::OtherMouseDown
        | NSEventMask::OtherMouseUp;
    let local_token =
        unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(local_mask, &local_block) };
    let global_token =
        NSEvent::addGlobalMonitorForEventsMatchingMask_handler(global_mask, &global_block);
    if local_token.is_some() || global_token.is_some() {
        NATIVE_MOUSE_WAKE_MONITOR_INSTALLED.store(true, Ordering::Relaxed);
        if let Some(token) = local_token {
            std::mem::forget(token);
        }
        if let Some(token) = global_token {
            std::mem::forget(token);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn install_native_mouse_wake_monitor() {}

/// Track macOS live-resize so [`foreground_winit_settings`] can pace the loop at ~60Hz during the
/// drag. `NSWindow` posts these notifications once per drag; the blocks set [`IN_LIVE_RESIZE`] and
/// wake the loop so the reactive mode switches immediately.
#[cfg(target_os = "macos")]
fn install_live_resize_monitor(proxy: Option<Res<EventLoopProxyWrapper>>) {
    use objc2_app_kit::{
        NSWindowDidEndLiveResizeNotification, NSWindowWillStartLiveResizeNotification,
    };
    use objc2_foundation::{NSNotification, NSNotificationCenter};

    if LIVE_RESIZE_MONITOR_INSTALLED.load(Ordering::Relaxed) {
        return;
    }
    let Some(proxy) = proxy else {
        return;
    };
    let (start_name, end_name) = unsafe {
        (
            NSWindowWillStartLiveResizeNotification,
            NSWindowDidEndLiveResizeNotification,
        )
    };
    let center = NSNotificationCenter::defaultCenter();
    let start_proxy = (**proxy).clone();
    let start_block = block2::RcBlock::new(move |_n: NonNull<NSNotification>| {
        IN_LIVE_RESIZE.store(true, Ordering::Relaxed);
        let _ = start_proxy.send_event(WinitUserEvent::WakeUp);
    });
    let end_proxy = (**proxy).clone();
    let end_block = block2::RcBlock::new(move |_n: NonNull<NSNotification>| {
        IN_LIVE_RESIZE.store(false, Ordering::Relaxed);
        let _ = end_proxy.send_event(WinitUserEvent::WakeUp);
    });
    let start_token = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(start_name),
            None,
            None,
            &start_block,
        )
    };
    let end_token = unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(end_name), None, None, &end_block)
    };
    std::mem::forget(start_token);
    std::mem::forget(end_token);
    LIVE_RESIZE_MONITOR_INSTALLED.store(true, Ordering::Relaxed);
}

#[cfg(not(target_os = "macos"))]
fn install_live_resize_monitor() {}

#[cfg(target_os = "macos")]
fn event_location_in_window_physical_px(event: &objc2_app_kit::NSEvent) -> Option<(f32, f32)> {
    let mtm = objc2::MainThreadMarker::new()?;
    let window = event.window(mtm)?;
    let content = window.contentView()?;
    let point = content.convertPoint_fromView(event.locationInWindow(), None);
    let scale = window.backingScaleFactor();
    let x = point.x * scale;
    let y = point.y * scale;
    if x.is_finite() && y.is_finite() {
        Some((x as f32, y as f32))
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn native_mouse_buttons() -> bevy_cef_core::prelude::NativeMouseButtons {
    let pressed = objc2_app_kit::NSEvent::pressedMouseButtons();
    bevy_cef_core::prelude::NativeMouseButtons {
        left: pressed & 1 != 0,
        right: pressed & (1 << 1) != 0,
        middle: pressed & (1 << 2) != 0,
    }
}

/// `react_to_device_events` is off in browse (User) mode: native CEF views own scroll/input, so only
/// Player mode's free camera consumes `AccumulatedMouseMotion`.
///
/// At rest window events wake rendering except while native layout chrome owns the pointer. Layout
/// hover and scroll then use the explicit 30Hz native monitor wake instead of rendering once per
/// raw macOS pointer event. During live resize the 16ms timer caps rendering near 60Hz.
pub(crate) fn foreground_winit_settings(
    player: bool,
    live_resize: bool,
    layout_pointer_inside: bool,
) -> WinitSettings {
    let focused_mode = if live_resize {
        UpdateMode::Reactive {
            wait: Duration::from_millis(16),
            react_to_device_events: player,
            react_to_user_events: true,
            react_to_window_events: false,
        }
    } else {
        UpdateMode::Reactive {
            wait: FOCUSED_FRAME_INTERVAL,
            react_to_device_events: player,
            react_to_user_events: true,
            react_to_window_events: player || !layout_pointer_inside,
        }
    };
    WinitSettings {
        focused_mode,
        unfocused_mode: UpdateMode::reactive_low_power(UNFOCUSED_FRAME_INTERVAL),
    }
}

fn hidden_winit_settings() -> WinitSettings {
    WinitSettings {
        focused_mode: UpdateMode::reactive_low_power(HIDDEN_FRAME_INTERVAL),
        unfocused_mode: UpdateMode::reactive_low_power(HIDDEN_FRAME_INTERVAL),
    }
}

fn sync_winit_power_mode(
    mut settings: ResMut<WinitSettings>,
    wake_policy: Option<Res<MessageLoopWakePolicy>>,
    mode: Res<InteractionMode>,
    windows: Query<&Window>,
    monitors: Query<&Monitor>,
) {
    let all_hidden = windows.iter().all(|w| !w.visible);
    let any_visible = windows.iter().any(|w| w.visible);
    let any_focused = windows.iter().any(|w| w.visible && w.focused);
    #[cfg(target_os = "macos")]
    let live_resize = IN_LIVE_RESIZE.load(Ordering::Relaxed);
    #[cfg(not(target_os = "macos"))]
    let live_resize = false;
    #[cfg(target_os = "macos")]
    let layout_pointer_inside = vmux_browser::native_layout_pointer_is_inside();
    #[cfg(not(target_os = "macos"))]
    let layout_pointer_inside = false;
    let next = if all_hidden {
        hidden_winit_settings()
    } else {
        foreground_winit_settings(
            *mode == InteractionMode::Player,
            live_resize,
            layout_pointer_inside,
        )
    };
    if settings.focused_mode != next.focused_mode || settings.unfocused_mode != next.unfocused_mode
    {
        *settings = next;
    }
    if let Some(policy) = wake_policy {
        policy.set_min_wake_interval(cef_wake_interval(
            all_hidden,
            any_visible,
            any_focused,
            foreground_cef_wake_interval(monitors.iter().map(|m| m.refresh_rate_millihertz)),
        ));
    }
}

fn foreground_cef_wake_interval(refresh_rates: impl IntoIterator<Item = Option<u32>>) -> Duration {
    windowless_frame_interval_from_refresh_millihertz(refresh_rates.into_iter().flatten().max())
}

fn cef_wake_interval(
    all_hidden: bool,
    any_visible: bool,
    any_focused: bool,
    foreground_interval: Duration,
) -> Duration {
    if all_hidden || !any_visible || !any_focused {
        BACKGROUND_CEF_WAKE_INTERVAL
    } else {
        foreground_interval
    }
}

/// Keep the winit loop ticking while any webview is mid-reveal. Native pages don't wake Bevy (no OSR
/// paints) and browse mode disables raw device events, so the 2-frame reveal counter
/// ([`vmux_layout::PendingWebviewReveal`]) would otherwise stall at ~1 tick/s — newly split or opened
/// panes take seconds to appear. Route the missing wake explicitly (see AGENTS.md). Self-terminating:
/// once all reveals complete the query is empty and we stop waking.
fn keep_awake_while_revealing(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    pending: Query<(), With<vmux_layout::PendingWebviewReveal>>,
) {
    if pending.is_empty() {
        return;
    }
    if let Some(proxy) = proxy {
        let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
    }
}

fn player_frame_should_wake(
    mode: InteractionMode,
    transition_active: bool,
    window_active: bool,
) -> bool {
    window_active && (mode == InteractionMode::Player || transition_active)
}

fn keep_awake_while_player_active(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mode: Res<InteractionMode>,
    transition: Option<Res<vmux_layout::scene::ModeTransition>>,
    windows: Query<&Window>,
) {
    let window_active = windows
        .iter()
        .any(|window| window.visible && window.focused);
    if !player_frame_should_wake(*mode, transition.is_some(), window_active) {
        return;
    }
    if let Some(proxy) = proxy {
        let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
    }
}

fn command_bar_should_wake(needs_open: bool, has_active_reveal: bool) -> bool {
    needs_open || has_active_reveal
}

/// The command bar opens across several reactive frames: the first shortcut may defer
/// (`NewStackContext::needs_open`) until the CEF webview is ready, then a reveal
/// (`PendingCommandBarReveal`) waits for the rendered/sized ack. Without an explicit wake the loop
/// idles after the keystroke and the open stalls until the next input — the user has to press
/// Cmd+K/Cmd+L twice. Mirror [`keep_awake_while_revealing`] for the modal. Runs after
/// `ReadAppCommands` so `needs_open` set this frame is observed. Self-terminating: once revealed,
/// `needs_open` clears and the placeholder reveal is `open_id == 0` (inactive), so we stop waking.
fn keep_awake_while_command_bar_opening(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    new_stack_ctx: Option<Res<vmux_layout::NewStackContext>>,
    pending: Query<&vmux_layout::PendingCommandBarReveal>,
) {
    let needs_open = new_stack_ctx.map(|ctx| ctx.needs_open).unwrap_or(false);
    let has_active_reveal = pending
        .iter()
        .any(vmux_layout::PendingCommandBarReveal::is_active);
    if !command_bar_should_wake(needs_open, has_active_reveal) {
        return;
    }
    if let Some(proxy) = proxy {
        let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
    }
}

fn handle_lifecycle_events(world: &mut World) {
    let drained: Vec<LifecycleEvent> = {
        let mut events = world.resource_mut::<Messages<LifecycleEvent>>();
        events.drain().collect()
    };

    for event in drained {
        match event {
            LifecycleEvent::HideAllWindows => {
                let mut q = world.query::<&mut Window>();
                for mut w in q.iter_mut(world) {
                    w.visible = false;
                }
                hide_all_osr_webviews(world);
            }
            #[cfg(feature = "tray")]
            LifecycleEvent::ShowAllWindows => {
                let mut q = world.query::<&mut Window>();
                for mut w in q.iter_mut(world) {
                    w.visible = true;
                }
            }
            #[cfg(feature = "tray")]
            LifecycleEvent::QuitVmux => {
                let live = {
                    let mut q = world.query_filtered::<(), (With<Terminal>, Without<PtyExited>)>();
                    q.iter(world).count()
                };
                if live > 0 && !terminal::confirm_quit_dialog(live) {
                    continue;
                }
                world
                    .resource_mut::<Messages<AppExit>>()
                    .write(AppExit::Success);
            }
        }
    }
}

fn hide_all_osr_webviews(world: &mut World) {
    if let Some(browsers) = world.get_non_send::<Browsers>() {
        browsers.set_all_osr_hidden();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_frame_demand_only_runs_for_player_or_transition() {
        assert!(!player_frame_should_wake(
            InteractionMode::User,
            false,
            true
        ));
        assert!(player_frame_should_wake(
            InteractionMode::Player,
            false,
            true
        ));
        assert!(player_frame_should_wake(InteractionMode::User, true, true));
        assert!(player_frame_should_wake(
            InteractionMode::Player,
            true,
            true
        ));
        assert!(!player_frame_should_wake(
            InteractionMode::Player,
            false,
            false
        ));
        assert!(!player_frame_should_wake(
            InteractionMode::User,
            true,
            false
        ));
    }

    #[test]
    fn player_frame_demand_runs_in_last() {
        let source = include_str!("background_lifecycle.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();
        let plugin_build = source
            .split("impl Plugin for BackgroundLifecyclePlugin")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(target_os = \"macos\")]").next())
            .unwrap_or_default();

        assert!(plugin_build.contains(".add_systems(Last, keep_awake_while_player_active)"));
    }

    #[test]
    fn command_bar_wake_covers_defer_and_active_reveal() {
        assert!(command_bar_should_wake(true, false));
        assert!(command_bar_should_wake(false, true));
        assert!(command_bar_should_wake(true, true));
        assert!(!command_bar_should_wake(false, false));
    }

    #[test]
    fn handle_lifecycle_events_uses_world_for_confirm_dialog() {
        let source = include_str!("background_lifecycle.rs");
        let exclusive_marker = ["world", ": ", "&mut", " World"].concat();
        assert!(
            source.contains(&exclusive_marker),
            "handle_lifecycle_events must be an exclusive &mut World system to call confirm_quit_dialog"
        );
        let confirm_call = ["confirm", "_quit_dialog"].concat();
        assert!(
            source.contains(&confirm_call),
            "QuitVmux arm must call terminal::confirm_quit_dialog"
        );
    }

    #[test]
    fn no_continuous_update_mode_anywhere_in_workspace() {
        use std::path::Path;
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        let banned = ["UpdateMode", "::", "Continuous"].concat();
        let mut offenders = Vec::new();
        for root in ["crates", "patches"] {
            let dir = workspace_root.join(root);
            if !dir.exists() {
                continue;
            }
            walk_rs_files(&dir, &mut |path, source| {
                if path.ends_with("background_lifecycle.rs") {
                    return;
                }
                for (lineno, line) in source.lines().enumerate() {
                    let stripped = line.trim_start();
                    if stripped.starts_with("//") || stripped.starts_with("///") {
                        continue;
                    }
                    if line.contains(&banned) {
                        offenders.push(format!(
                            "{}:{}: {}",
                            path.display(),
                            lineno + 1,
                            line.trim()
                        ));
                    }
                }
            });
        }
        assert!(
            offenders.is_empty(),
            "Bevy `UpdateMode::Continuous` is banned in vmux (causes 100-200% idle CPU). Use `UpdateMode::Reactive` and route missing wake sources via `EventLoopProxy::send_event(WinitUserEvent::WakeUp)`. See AGENTS.md. Offenders:\n{}",
            offenders.join("\n")
        );
    }

    fn walk_rs_files(dir: &std::path::Path, visit: &mut dyn FnMut(&std::path::Path, &str)) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                    continue;
                }
                walk_rs_files(&path, visit);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
                && let Ok(source) = std::fs::read_to_string(&path)
            {
                visit(&path, &source);
            }
        }
    }

    #[test]
    fn foreground_power_mode_is_reactive_when_focused() {
        let settings = foreground_winit_settings(false, false, false);

        let UpdateMode::Reactive {
            wait,
            react_to_device_events,
            react_to_user_events,
            react_to_window_events,
        } = settings.focused_mode
        else {
            panic!(
                "focused mode must be Reactive, got {:?}",
                settings.focused_mode
            );
        };
        assert_eq!(wait, Duration::from_secs(1));
        assert!(react_to_user_events);
        assert_eq!(
            settings.unfocused_mode,
            UpdateMode::reactive_low_power(Duration::from_secs(1))
        );
        // At rest, window-event wakes are on so the layout mesh + camera respond to window events;
        // device-event wakes stay off in browse mode.
        assert!(!react_to_device_events);
        assert!(react_to_window_events);

        // During a live resize, the loop is paced by a ~16ms timer (window-event reaction off) to cap
        // the render rate to ~60Hz instead of the 120Hz display refresh.
        let UpdateMode::Reactive {
            wait: resize_wait,
            react_to_window_events: resize_window,
            ..
        } = foreground_winit_settings(false, true, false).focused_mode
        else {
            panic!("focused mode must be Reactive");
        };
        assert_eq!(resize_wait, Duration::from_millis(16));
        assert!(!resize_window);

        let player = foreground_winit_settings(true, false, false);
        let UpdateMode::Reactive {
            react_to_device_events: player_device,
            react_to_window_events: player_window,
            ..
        } = player.focused_mode
        else {
            panic!("focused mode must be Reactive");
        };
        assert!(player_device);
        assert!(player_window);

        let layout_hover = foreground_winit_settings(false, false, true);
        let UpdateMode::Reactive {
            react_to_window_events: layout_window,
            ..
        } = layout_hover.focused_mode
        else {
            panic!("focused mode must be Reactive");
        };
        assert!(!layout_window);
    }

    #[test]
    fn native_mouse_motion_publishes_latest_sample_before_waking() {
        let source = include_str!("background_lifecycle.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn foreground_winit_settings").next())
            .unwrap_or_default();

        assert!(monitor.contains("NSEventMask::MouseMoved"));
        assert!(monitor.contains("NSEventMask::LeftMouseDown"));
        assert!(monitor.contains("WinitUserEvent::WakeUp"));
        assert!(monitor.contains("vmux_layout::native_pointer::publish"));
        assert!(monitor.contains("vmux_browser::queue_native_layout_pointer_move"));
        assert!(monitor.contains("flush_layout(interval)"));
        assert!(monitor.contains("if result.region_changed"));
        assert!(monitor.contains("vmux_browser::flush_native_layout_pointer_move()"));
        assert!(!monitor.contains("forward_native_layout_pointer_move"));
        assert!(!monitor.contains("vmux_layout::pane::wake_on_move"));
        assert!(monitor.contains("let global_mask = NSEventMask::LeftMouseDown"));
    }

    #[test]
    fn native_mouse_wake_throttle_has_a_trailing_wake() {
        let source = include_str!("background_lifecycle.rs");
        let throttle = source
            .split("fn native_throttle")
            .nth(1)
            .and_then(|tail| tail.split("fn install_native_mouse_wake_monitor").next())
            .unwrap_or_default();

        assert!(throttle.contains("sync_channel::<()>(1)"));
        assert!(throttle.contains("recv_timeout"));
        assert!(throttle.contains("pending_interval_ns.fetch_min"));
        assert!(!throttle.contains("while wake_rx.try_recv().is_ok()"));
        assert!(!throttle.contains("thread_pending_interval_ns.store"));
        assert!(!throttle.contains("LAST_NATIVE_MOUSE_WAKE.lock()"));
    }

    #[test]
    fn native_layout_motion_skips_bevy_wake_after_entry() {
        let source = include_str!("background_lifecycle.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn foreground_winit_settings").next())
            .unwrap_or_default();

        assert!(monitor.contains("result.owns_pointer"));
        assert!(monitor.contains("result.presenter_active"));
        assert!(monitor.contains(
            "if result.region_changed {\n                    vmux_browser::flush_native_layout_pointer_move();\n                    local_wake(interval);\n                } else if result.pending {\n                    flush_layout(interval);\n                }"
        ));
        assert!(monitor.contains("layout_pointer.is_some_and"));
    }

    #[test]
    fn native_mouse_motion_wakes_before_window_is_key() {
        let source = include_str!("background_lifecycle.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn foreground_winit_settings").next())
            .unwrap_or_default();

        assert!(monitor.contains("addLocalMonitorForEventsMatchingMask_handler"));
        assert!(monitor.contains("addGlobalMonitorForEventsMatchingMask_handler"));
    }

    #[test]
    fn native_mouse_monitor_tracks_left_button_state() {
        let source = include_str!("background_lifecycle.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn foreground_winit_settings").next())
            .unwrap_or_default();

        assert!(monitor.contains("vmux_browser::set_native_left_mouse_down(true)"));
        assert!(monitor.contains("vmux_browser::set_native_left_mouse_down(false)"));
    }

    #[test]
    fn native_tab_drag_does_not_move_window() {
        let source = include_str!("background_lifecycle.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn foreground_winit_settings").next())
            .unwrap_or_default();

        assert!(monitor.contains("event_type == NSEventType::LeftMouseDragged"));
        assert!(monitor.contains("vmux_layout::native_pointer::layout_drag_active()"));
        assert!(monitor.contains("return std::ptr::null_mut();"));
    }

    #[test]
    fn native_layout_scroll_wakes_reactive_loop() {
        let source = include_str!("background_lifecycle.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn foreground_winit_settings").next())
            .unwrap_or_default();

        assert!(monitor.contains("NSEventMask::ScrollWheel"));
    }

    #[test]
    fn startup_activates_primary_window_on_macos() {
        let source = include_str!("background_lifecycle.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();
        let plugin_build = source
            .split("impl Plugin for BackgroundLifecyclePlugin")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(target_os = \"macos\")]").next())
            .unwrap_or_default();

        assert!(plugin_build.contains("install_native_mouse_wake_monitor"));
        assert!(plugin_build.contains("activate_primary_window_on_startup"));
        assert!(source.contains("activateIgnoringOtherApps"));
        assert!(source.contains("makeKeyAndOrderFront"));
    }

    #[test]
    fn native_mouse_monitor_does_not_wait_for_window_creation() {
        let source = include_str!("background_lifecycle.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn foreground_winit_settings").next())
            .unwrap_or_default();

        assert!(monitor.contains("proxy: Option<Res<EventLoopProxyWrapper>>"));
        assert!(!monitor.contains("PrimaryWindow"));
        assert!(!monitor.contains("appkit_window_ptr"));
    }

    #[test]
    fn startup_activation_waits_for_visible_window() {
        let source = include_str!("background_lifecycle.rs")
            .split("fn activate_primary_window_on_startup")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(not(target_os = \"macos\"))]").next())
            .unwrap_or_default();

        assert!(source.contains("if !window.visible"));
    }

    #[test]
    fn app_activation_starts_during_boot() {
        let source = include_str!("background_lifecycle.rs");
        let plugin_build = source
            .split("impl Plugin for BackgroundLifecyclePlugin")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(target_os = \"macos\")]").next())
            .unwrap_or_default();
        assert!(plugin_build.contains("activate_app_during_boot"));

        let boot = source
            .split("fn activate_app_during_boot")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(not(target_os = \"macos\"))]").next())
            .unwrap_or_default();
        assert!(boot.contains("APP_ACTIVATION_BUDGET"));
        assert!(boot.contains("WinitUserEvent::WakeUp"));
    }

    #[test]
    fn native_mouse_down_requests_command_bar_dismiss() {
        let source = include_str!("background_lifecycle.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn foreground_winit_settings").next())
            .unwrap_or_default();

        assert!(monitor.contains("event_location_in_window_physical_px"));
        assert!(monitor.contains("request_native_command_bar_dismiss_for_mouse_down"));
    }

    #[test]
    fn hidden_power_mode_ignores_stale_window_focus() {
        let settings = hidden_winit_settings();

        assert_eq!(
            settings.focused_mode,
            UpdateMode::reactive_low_power(Duration::from_secs(60))
        );
        assert_eq!(
            settings.unfocused_mode,
            UpdateMode::reactive_low_power(Duration::from_secs(60))
        );
    }

    #[test]
    fn cef_wake_policy_matches_display_refresh() {
        assert_eq!(
            foreground_cef_wake_interval([Some(60_000)]),
            Duration::from_nanos(16_666_666)
        );
        assert!(foreground_cef_wake_interval([Some(144_000)]) < Duration::from_millis(8));
        assert_eq!(
            cef_wake_interval(false, true, true, Duration::from_millis(7)),
            Duration::from_millis(7)
        );
    }

    #[test]
    fn cef_wake_policy_throttles_visible_unfocused() {
        assert_eq!(
            cef_wake_interval(false, true, false, Duration::from_millis(7)),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn cef_wake_policy_throttles_hidden() {
        assert_eq!(
            cef_wake_interval(false, false, true, Duration::from_millis(7)),
            Duration::from_secs(1)
        );
        assert_eq!(
            cef_wake_interval(true, true, true, Duration::from_millis(7)),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn hide_lifecycle_suspends_osr_webviews() {
        let source = include_str!("background_lifecycle.rs");

        assert!(source.contains("hide_all_osr_webviews(world)"));
        assert!(source.contains("set_all_osr_hidden"));
    }
}
