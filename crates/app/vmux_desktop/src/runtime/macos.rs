//! macOS runtime: AppKit event monitors that keep the winit loop awake, native window
//! activation, and the live-resize/render-demand state they publish back to
//! [`super::sync_winit_power_mode`].

use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::WebviewWindowed;
use vmux_layout::{cef::LayoutCef, window::Modal};

use super::RenderFrameDemand;

/// The macOS half of [`super::RuntimePlugin`]: installs the AppKit monitors winit does not own, and gates rendering on demand.
pub(super) struct RuntimePlatformPlugin;

impl Plugin for RuntimePlatformPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, activate_app_during_boot)
            .add_systems(Update, grab_key_window_on_pane_hover)
            .add_systems(Last, sync_render_frame_demand)
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

use super::native::{
    NativeWindowFrame, NativeWindowResizeDrag, native_resize_edges, native_scroll_should_wake,
    render_frame_should_run, resized_native_window_frame, windowed_pointer_inside_after_event,
};

const NATIVE_MOUSE_MOVE_WAKE_INTERVAL: Duration = Duration::from_millis(33);
const NATIVE_MOUSE_DRAG_WAKE_INTERVAL: Duration = Duration::from_millis(16);
const NATIVE_LAYOUT_ACTIVITY_SETTLE: Duration = Duration::from_millis(300);
/// How often Bevy has to run while the layout webview is being scrolled.
///
/// The layout is composited as a native overlay: CEF paints into an `IOSurface` and the presenter
/// hands it to the `CALayer` on the main dispatch queue, so no Bevy frame stands between a scrolled
/// pixel and the screen. The app only has to tick often enough to keep the frame-rate governor from
/// falling back to the idle rate, which it does after `LAYOUT_INPUT_BURST`.
const NATIVE_LAYOUT_SCROLL_WAKE_INTERVAL: Duration = Duration::from_millis(100);

static NATIVE_MOUSE_WAKE_MONITOR_INSTALLED: AtomicBool = AtomicBool::new(false);
static IN_LIVE_RESIZE: AtomicBool = AtomicBool::new(false);
static LIVE_RESIZE_MONITOR_INSTALLED: AtomicBool = AtomicBool::new(false);
static HOVER_OVER_PANE: AtomicBool = AtomicBool::new(false);
static NATIVE_WINDOWED_POINTER_INSIDE: AtomicBool = AtomicBool::new(false);

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

fn grab_key_window_on_pane_hover(
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
    let Ok(window_entity) = primary_window.single() else {
        return;
    };
    ensure_native_window_active(window_entity);
}

fn activate_native_window(window_entity: Entity) {
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
const APP_ACTIVATION_BUDGET: Duration = Duration::from_secs(10);

/// Bring the app to the foreground (app level only — no window). Returns `true` once active.
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

type NativeThrottle = Arc<dyn Fn(Duration) + Send + Sync>;

type NativeDebounce = Arc<dyn Fn() + Send + Sync>;

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

fn native_debounce(
    name: &'static str,
    delay: Duration,
    action: impl Fn() + Send + 'static,
) -> NativeDebounce {
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            while rx.recv().is_ok() {
                loop {
                    match rx.recv_timeout(delay) {
                        Ok(()) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            action();
                            break;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
        })
        .unwrap_or_else(|error| panic!("failed to spawn {name}: {error}"));
    Arc::new(move || {
        let _ = tx.try_send(());
    })
}

fn begin_native_window_resize(event: &objc2_app_kit::NSEvent) -> Option<NativeWindowResizeDrag> {
    use objc2_app_kit::{NSEvent, NSWindowStyleMask};

    let mtm = objc2::MainThreadMarker::new()?;
    let window = event.window(mtm)?;
    let style = window.styleMask();
    if style.contains(NSWindowStyleMask::FullScreen) {
        return None;
    }
    if !style.contains(NSWindowStyleMask::Resizable) {
        window.setStyleMask(style | NSWindowStyleMask::Resizable);
    }
    let frame = window.frame();
    let cursor = NSEvent::mouseLocation();
    let frame = NativeWindowFrame {
        x: frame.origin.x,
        y: frame.origin.y,
        width: frame.size.width,
        height: frame.size.height,
    };
    let edges = native_resize_edges(frame, cursor.x, cursor.y, 8.0);
    if !edges.any() {
        return None;
    }
    let min_size = window.minSize();
    Some(NativeWindowResizeDrag {
        frame,
        cursor_x: cursor.x,
        cursor_y: cursor.y,
        min_width: min_size.width.max(1.0),
        min_height: min_size.height.max(1.0),
        edges,
    })
}

fn update_native_window_resize(event: &objc2_app_kit::NSEvent, drag: NativeWindowResizeDrag) {
    use objc2_app_kit::NSEvent;
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let Some(window) = event.window(mtm) else {
        return;
    };
    let cursor = NSEvent::mouseLocation();
    let frame = resized_native_window_frame(drag, cursor.x, cursor.y);
    window.setFrame_display(
        NSRect::new(
            NSPoint::new(frame.x, frame.y),
            NSSize::new(frame.width, frame.height),
        ),
        true,
    );
}

fn install_native_mouse_wake_monitor(proxy: Option<Res<EventLoopProxyWrapper>>) {
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};

    let Some(proxy) = proxy else {
        return;
    };
    if NATIVE_MOUSE_WAKE_MONITOR_INSTALLED.load(Ordering::Relaxed) {
        return;
    }
    let proxy = (**proxy).clone();
    let settle_proxy = proxy.clone();
    let wake = native_throttle("native-mouse-wake-throttle", move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
    let settle_layout = native_debounce(
        "native-layout-activity-settle",
        NATIVE_LAYOUT_ACTIVITY_SETTLE,
        move || {
            vmux_browser::set_native_layout_activity(false);
            let _ = settle_proxy.send_event(WinitUserEvent::WakeUp);
        },
    );
    let flush_layout = native_throttle("native-layout-pointer-throttle", || {
        dispatch2::DispatchQueue::main().exec_async(|| {
            vmux_browser::NativeLayout::flush_pointer_move();
        });
    });
    let resize_drag = Arc::new(Mutex::new(None::<NativeWindowResizeDrag>));
    let local_resize_drag = Arc::clone(&resize_drag);
    let local_wake = wake.clone();
    let local_block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        let ev = unsafe { event.as_ref() };
        let event_type = ev.r#type();
        let capture_window_resize = match event_type {
            NSEventType::LeftMouseDown => {
                let drag = begin_native_window_resize(ev);
                *local_resize_drag
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = drag;
                if drag.is_some() {
                    IN_LIVE_RESIZE.store(true, Ordering::Relaxed);
                    true
                } else {
                    false
                }
            }
            NSEventType::LeftMouseDragged => {
                let drag = *local_resize_drag
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(drag) = drag {
                    update_native_window_resize(ev, drag);
                    true
                } else {
                    false
                }
            }
            NSEventType::LeftMouseUp => {
                let drag = local_resize_drag
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take();
                if let Some(drag) = drag {
                    update_native_window_resize(ev, drag);
                    IN_LIVE_RESIZE.store(false, Ordering::Relaxed);
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
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
        let scroll = event_type == NSEventType::ScrollWheel;
        let location = event_location_in_window_physical_px(ev);
        let pointer_position_changed = motion || button_event;
        let was_over_windowed_page = NATIVE_WINDOWED_POINTER_INSIDE.load(Ordering::Relaxed);
        let sampled_over_windowed_page = location
            .is_some_and(|(x, y)| vmux_browser::NativeBridge::windowed_page_contains_point(x, y));
        let over_windowed_page = windowed_pointer_inside_after_event(
            pointer_position_changed,
            was_over_windowed_page,
            sampled_over_windowed_page,
        );
        if pointer_position_changed {
            NATIVE_WINDOWED_POINTER_INSIDE.store(over_windowed_page, Ordering::Relaxed);
        }
        let buttons = native_mouse_buttons();
        if pointer_position_changed && let Some((x, y)) = location {
            vmux_layout::native_pointer::publish(Vec2::new(x, y), buttons, motion);
        }
        let layout_pointer = pointer_position_changed
            .then(|| {
                location.map(|(x, y)| vmux_browser::NativeLayout::queue_pointer_move(x, y, buttons))
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
                let activity_started = vmux_browser::set_native_layout_activity(true);
                settle_layout();
                if result.region_changed {
                    vmux_browser::NativeLayout::flush_pointer_move();
                    local_wake(interval);
                } else if result.pending {
                    flush_layout(interval);
                }
                if activity_started && !result.region_changed {
                    local_wake(interval);
                }
            } else {
                vmux_browser::set_native_layout_activity(false);
                if !over_windowed_page || !was_over_windowed_page || !event_window_is_key(ev) {
                    HOVER_OVER_PANE.store(true, Ordering::Relaxed);
                    local_wake(interval);
                }
            }
        } else if scroll {
            let forwarded = location.is_some_and(|(x, y)| {
                let delta = Vec2::new(ev.scrollingDeltaX() as f32, ev.scrollingDeltaY() as f32);
                vmux_browser::NativeLayout::forward_scroll(Vec2::new(x, y), delta)
            });
            if forwarded {
                local_wake(NATIVE_LAYOUT_SCROLL_WAKE_INTERVAL);
                return std::ptr::null_mut();
            }
            if native_scroll_should_wake(
                vmux_browser::NativeLayout::pointer_is_inside(),
                sampled_over_windowed_page,
            ) {
                local_wake(NATIVE_MOUSE_DRAG_WAKE_INTERVAL);
            }
        } else {
            // A click over a native windowed pane never reaches winit, so the layout overlay's own
            // DOM — result rows, the dismiss backdrop — would never see it.
            if button_event
                && sampled_over_windowed_page
                && let Some((x, y)) = location
                && let Some(button) = native_pointer_button(event_type)
            {
                let mouse_up = matches!(
                    event_type,
                    NSEventType::LeftMouseUp
                        | NSEventType::RightMouseUp
                        | NSEventType::OtherMouseUp
                );
                if vmux_browser::NativeLayout::forward_click(Vec2::new(x, y), button, mouse_up) {
                    local_wake(NATIVE_MOUSE_DRAG_WAKE_INTERVAL);
                    return std::ptr::null_mut();
                }
            }
            if let Some(result) = layout_pointer
                && result.owns_pointer
                && result.presenter_active
            {
                vmux_browser::set_native_layout_activity(true);
                settle_layout();
                if result.pending {
                    vmux_browser::NativeLayout::flush_pointer_move();
                }
            }
            local_wake(NATIVE_MOUSE_DRAG_WAKE_INTERVAL);
        }
        if capture_window_resize {
            return std::ptr::null_mut();
        }
        event.as_ptr()
    });
    let global_resize_drag = Arc::clone(&resize_drag);
    let global_wake = wake.clone();
    let global_block = block2::RcBlock::new(move |event: NonNull<NSEvent>| {
        let event_type = unsafe { event.as_ref() }.r#type();
        if event_type == NSEventType::LeftMouseDown {
            vmux_browser::set_native_left_mouse_down(true);
        } else if event_type == NSEventType::LeftMouseUp {
            vmux_browser::set_native_left_mouse_down(false);
            global_resize_drag
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take();
            IN_LIVE_RESIZE.store(false, Ordering::Relaxed);
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

/// Track macOS live-resize so [`super::foreground_winit_settings`] can pace the loop at ~60Hz
/// during the drag. `NSWindow` posts these notifications once per drag; the blocks set
/// [`IN_LIVE_RESIZE`] and wake the loop so the reactive mode switches immediately.
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

fn event_window_is_key(event: &objc2_app_kit::NSEvent) -> bool {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return false;
    };
    event.window(mtm).is_some_and(|window| window.isKeyWindow())
}

fn native_mouse_buttons() -> bevy_cef_core::prelude::NativeMouseButtons {
    let pressed = objc2_app_kit::NSEvent::pressedMouseButtons();
    bevy_cef_core::prelude::NativeMouseButtons {
        left: pressed & 1 != 0,
        right: pressed & (1 << 1) != 0,
        middle: pressed & (1 << 2) != 0,
    }
}

/// Which pointer button an `NSEvent` type carries, if any.
fn native_pointer_button(
    event_type: objc2_app_kit::NSEventType,
) -> Option<bevy::picking::pointer::PointerButton> {
    use bevy::picking::pointer::PointerButton;
    use objc2_app_kit::NSEventType;
    match event_type {
        NSEventType::LeftMouseDown | NSEventType::LeftMouseUp => Some(PointerButton::Primary),
        NSEventType::RightMouseDown | NSEventType::RightMouseUp => Some(PointerButton::Secondary),
        NSEventType::OtherMouseDown | NSEventType::OtherMouseUp => Some(PointerButton::Middle),
        _ => None,
    }
}

fn sync_render_frame_demand(
    pages: Query<
        &Transform,
        (
            With<vmux_layout::Browser>,
            With<WebviewWindowed>,
            Without<LayoutCef>,
            Without<Modal>,
        ),
    >,
    mut demand: ResMut<RenderFrameDemand>,
) {
    let visible_windowed_page = pages.iter().any(|transform| transform.scale.x > 1.0e-3);
    demand.0 = render_frame_should_run(
        IN_LIVE_RESIZE.load(Ordering::Relaxed),
        visible_windowed_page,
    );
}

/// Whether an `NSWindow` live-resize drag is in flight.
pub(super) fn live_resize_active() -> bool {
    IN_LIVE_RESIZE.load(Ordering::Relaxed)
}

/// Whether the pointer sits over native CEF content that owns it, so winit should stop
/// waking on window events.
pub(super) fn native_pointer_inside() -> bool {
    vmux_browser::NativeLayout::pointer_is_inside()
        || NATIVE_WINDOWED_POINTER_INSIDE.load(Ordering::Relaxed)
}
