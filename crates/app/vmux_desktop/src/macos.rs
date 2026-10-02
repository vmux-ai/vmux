use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WINIT_WINDOWS, WinitUserEvent};
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSApp, NSEvent, NSEventMask, NSEventType,
    NSView, NSWindowDidEndLiveResizeNotification, NSWindowStyleMask,
    NSWindowWillStartLiveResizeNotification,
};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize, NSString, NSUserDefaults,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use vmux_flex::prelude::*;
use vmux_setting::{ResolvedScheme, SystemAppearance};

use crate::window_interaction::{
    TitlebarClick, TitlebarClicks, WindowFrame, WindowPointerPolicy, WindowResizeDrag,
    WindowTitlebarGesture, WindowZoom,
};

pub(super) struct RuntimePlatformPlugin;

impl Plugin for RuntimePlatformPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, seed)
            .add_systems(Update, activate_app_during_boot)
            .add_systems(Update, grab_key_window_on_pane_hover)
            .add_systems(
                Startup,
                (
                    install_mouse_wake_monitor,
                    install_live_resize_monitor,
                    activate_primary_window,
                ),
            );
    }
}

fn seed(_non_send: bevy::ecs::system::NonSendMarker, mut system: Single<&mut SystemAppearance>) {
    if system.0.is_some() {
        return;
    }
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let appearance = NSApp(mtm).effectiveAppearance().name();
    system.0 = Some(if appearance.to_string().contains("Dark") {
        ResolvedScheme::Dark
    } else {
        ResolvedScheme::Light
    });
}

const MOUSE_MOVE_WAKE_INTERVAL: Duration = Duration::from_millis(33);
const MOUSE_DRAG_WAKE_INTERVAL: Duration = Duration::from_millis(16);

static MOUSE_WAKE_MONITOR_INSTALLED: AtomicBool = AtomicBool::new(false);
static IN_LIVE_RESIZE: AtomicBool = AtomicBool::new(false);
static LIVE_RESIZE_MONITOR_INSTALLED: AtomicBool = AtomicBool::new(false);
static HOVER_OVER_PANE: AtomicBool = AtomicBool::new(false);
static WINDOWED_POINTER_INSIDE: AtomicBool = AtomicBool::new(false);

fn activate_primary_window(
    primary_window: Query<(Entity, &Window), With<bevy::window::PrimaryWindow>>,
) {
    let Ok((window_entity, window)) = primary_window.single() else {
        return;
    };
    if !window.visible {
        return;
    }
    activate_window(window_entity);
}

fn grab_key_window_on_pane_hover(
    windows: Query<(Entity, &Window)>,
    focused_window: vmux_layout::window::FocusedWindow,
    panes: Query<
        &ComputedNode,
        (
            With<vmux_layout::pane::Pane>,
            Without<vmux_layout::pane::PaneSplit>,
        ),
    >,
) {
    if !HOVER_OVER_PANE.swap(false, Ordering::Relaxed) {
        return;
    }
    if !app_is_frontmost() {
        return;
    }
    let Some(pointer) = vmux_input::NativePointer::snapshot() else {
        return;
    };
    let mut over_pane = false;
    for node in panes.iter() {
        if node.contains(pointer.position_px) {
            over_pane = true;
            break;
        }
    }
    if !over_pane {
        return;
    }
    let Some(window_entity) = focused_window.entity() else {
        return;
    };
    let Ok((_, window)) = windows.get(window_entity) else {
        return;
    };
    if !window.visible {
        return;
    }
    ensure_key_window(window_entity);
}

fn app_is_frontmost() -> bool {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return false;
    };
    NSApp(mtm).isActive()
}

fn activate_window(window_entity: Entity) {
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

pub(crate) fn ensure_key_window(window_entity: Entity) -> bool {
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

const APP_ACTIVATION_BUDGET: Duration = Duration::from_secs(10);

fn activate_app() -> bool {
    if app_is_frontmost() {
        return true;
    }
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return false;
    };
    #[allow(deprecated)]
    NSApp(mtm).activateIgnoringOtherApps(true);
    false
}

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

type WakeThrottle = Arc<dyn Fn(Duration) + Send + Sync>;

fn wake_throttle(name: &'static str, callback: impl Fn() + Send + 'static) -> WakeThrottle {
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
                    callback();
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

impl From<NSRect> for WindowFrame {
    fn from(rect: NSRect) -> Self {
        Self {
            x: rect.origin.x,
            y: rect.origin.y,
            width: rect.size.width,
            height: rect.size.height,
        }
    }
}

impl From<WindowFrame> for NSRect {
    fn from(frame: WindowFrame) -> Self {
        NSRect::new(
            NSPoint::new(frame.x, frame.y),
            NSSize::new(frame.width, frame.height),
        )
    }
}

fn begin_window_resize(event: &NSEvent) -> Option<WindowResizeDrag> {
    let mtm = objc2::MainThreadMarker::new()?;
    let window = event.window(mtm)?;
    let style = window.styleMask();
    if style.contains(NSWindowStyleMask::FullScreen) {
        return None;
    }
    if !style.contains(NSWindowStyleMask::Resizable) {
        window.setStyleMask(style | NSWindowStyleMask::Resizable);
    }
    let cursor = NSEvent::mouseLocation();
    let frame = WindowFrame::from(window.frame());
    let edges = frame.resize_edges(cursor.x, cursor.y, 8.0);
    if !edges.any() {
        return None;
    }
    let min_size = window.minSize();
    Some(WindowResizeDrag {
        frame,
        cursor_x: cursor.x,
        cursor_y: cursor.y,
        min_width: min_size.width.max(1.0),
        min_height: min_size.height.max(1.0),
        edges,
    })
}

fn update_window_resize(event: &NSEvent, drag: WindowResizeDrag) {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let Some(window) = event.window(mtm) else {
        return;
    };
    let cursor = NSEvent::mouseLocation();
    let frame = drag.resized_frame(cursor.x, cursor.y);
    window.setFrame_display(frame.into(), true);
}

const TITLEBAR_DOUBLE_CLICK_SLOP_PX: f32 = 8.0;

impl WindowTitlebarGesture {
    fn capture(
        event: &NSEvent,
        clicks: &mut TitlebarClicks,
        zoom: &mut WindowZoom,
    ) -> Option<Self> {
        let mtm = objc2::MainThreadMarker::new()?;
        let Some((x, y)) = event_location_in_window_physical_px(event) else {
            clicks.forget();
            return None;
        };
        if !vmux_browser::WindowDragRegion::contains_point(x, y) {
            clicks.forget();
            return None;
        }
        let window = event.window(mtm)?;
        let count = clicks.count(
            TitlebarClick {
                at: event.timestamp(),
                x,
                y,
            },
            NSEvent::doubleClickInterval(),
            TITLEBAR_DOUBLE_CLICK_SLOP_PX,
        );
        let gesture = Self::resolve(count, Self::double_click_action().as_deref());
        match gesture {
            Self::Drag => window.performWindowDragWithEvent(event),
            Self::Zoom => zoom.animate(window),
            Self::Miniaturize => window.miniaturize(None),
            Self::Ignore => {}
        }
        Some(gesture)
    }

    fn double_click_action() -> Option<String> {
        let defaults = NSUserDefaults::standardUserDefaults();
        let behavior = defaults.stringForKey(&NSString::from_str("AppleActionOnDoubleClick"))?;
        Some(behavior.to_string())
    }
}

impl WindowZoom {
    fn animate(&mut self, window: objc2::rc::Retained<objc2_app_kit::NSWindow>) {
        let Some(screen) = window.screen() else {
            return;
        };
        let target = self.toggled(
            WindowFrame::from(window.frame()),
            WindowFrame::from(screen.visibleFrame()),
        );
        let target = target.into();
        let duration = window.animationResizeTime(target);
        let changes = block2::RcBlock::new(move |context: NonNull<NSAnimationContext>| {
            unsafe { context.as_ref() }.setDuration(duration);
            window.animator().setFrame_display(target, true);
        });
        NSAnimationContext::runAnimationGroup(&changes);
    }
}

fn install_mouse_wake_monitor(proxy: Option<Res<EventLoopProxyWrapper>>) {
    let Some(proxy) = proxy else {
        return;
    };
    if MOUSE_WAKE_MONITOR_INSTALLED.load(Ordering::Relaxed) {
        return;
    }
    let proxy = (**proxy).clone();
    let wake = wake_throttle("mouse-wake-throttle", move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
    let resize_drag = Arc::new(Mutex::new(None::<WindowResizeDrag>));
    let local_resize_drag = Arc::clone(&resize_drag);
    let titlebar_clicks = Arc::new(Mutex::new(TitlebarClicks::default()));
    let window_zoom = Arc::new(Mutex::new(WindowZoom::default()));
    let local_wake = wake.clone();
    let local_block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        let ev = unsafe { event.as_ref() };
        let event_type = ev.r#type();
        let mut titlebar_gesture = None;
        let capture_window_gesture = match event_type {
            NSEventType::LeftMouseDown if !event_belongs_to_main_window_frame(ev) => {
                titlebar_clicks
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .forget();
                false
            }
            NSEventType::LeftMouseDown => {
                let drag = begin_window_resize(ev);
                *local_resize_drag
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = drag;
                if drag.is_some() {
                    IN_LIVE_RESIZE.store(true, Ordering::Relaxed);
                    true
                } else {
                    let mut clicks = titlebar_clicks
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let mut zoom = window_zoom
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    titlebar_gesture = WindowTitlebarGesture::capture(ev, &mut clicks, &mut zoom);
                    titlebar_gesture.is_some()
                }
            }
            NSEventType::LeftMouseDragged => {
                let drag = *local_resize_drag
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(drag) = drag {
                    update_window_resize(ev, drag);
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
                    update_window_resize(ev, drag);
                    IN_LIVE_RESIZE.store(false, Ordering::Relaxed);
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        if event_type == NSEventType::LeftMouseDown {
            vmux_input::NativePointer::set_window_gesture(true);
        } else if event_type == NSEventType::LeftMouseUp {
            vmux_input::NativePointer::set_window_gesture(false);
        }
        if titlebar_gesture == Some(WindowTitlebarGesture::Drag) {
            vmux_input::NativePointer::set_window_gesture(false);
        }
        let motion = matches!(
            event_type,
            NSEventType::MouseMoved
                | NSEventType::LeftMouseDragged
                | NSEventType::RightMouseDragged
                | NSEventType::OtherMouseDragged
        );
        let event_belongs_to_main_window = event_belongs_to_main_window_frame(ev);
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
        let was_over_windowed_page = WINDOWED_POINTER_INSIDE.load(Ordering::Relaxed);
        let sampled_over_windowed_page = location
            .is_some_and(|(x, y)| vmux_browser::NativeBridge::windowed_page_contains_point(x, y));
        let over_windowed_page = WindowPointerPolicy::windowed_presence(
            pointer_position_changed,
            was_over_windowed_page,
            sampled_over_windowed_page,
        );
        if pointer_position_changed {
            WINDOWED_POINTER_INSIDE.store(over_windowed_page, Ordering::Relaxed);
        }
        let buttons = mouse_buttons();
        if pointer_position_changed && let Some((x, y)) = location {
            vmux_input::NativePointer::publish(Vec2::new(x, y), buttons, motion);
        }
        if motion && event_belongs_to_main_window {
            let interval = if event_type == NSEventType::MouseMoved {
                MOUSE_MOVE_WAKE_INTERVAL
            } else {
                MOUSE_DRAG_WAKE_INTERVAL
            };
            if !over_windowed_page || !was_over_windowed_page || !event_window_is_key(ev) {
                HOVER_OVER_PANE.store(true, Ordering::Relaxed);
                local_wake(interval);
            }
        } else if scroll {
            let wake_for_scroll = WindowPointerPolicy::scroll_should_wake(
                vmux_browser::NativeLayout::pointer_is_inside(),
                sampled_over_windowed_page,
            );
            if wake_for_scroll {
                local_wake(MOUSE_DRAG_WAKE_INTERVAL);
            }
        } else {
            local_wake(MOUSE_DRAG_WAKE_INTERVAL);
        }
        if capture_window_gesture {
            return std::ptr::null_mut();
        }
        event.as_ptr()
    });
    let global_resize_drag = Arc::clone(&resize_drag);
    let global_wake = wake.clone();
    let global_block = block2::RcBlock::new(move |event: NonNull<NSEvent>| {
        let event_type = unsafe { event.as_ref() }.r#type();
        if event_type == NSEventType::LeftMouseDown {
            vmux_input::NativePointer::set_window_gesture(true);
        } else if event_type == NSEventType::LeftMouseUp {
            vmux_input::NativePointer::set_window_gesture(false);
            global_resize_drag
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take();
            IN_LIVE_RESIZE.store(false, Ordering::Relaxed);
        }
        vmux_input::NativePointer::publish_buttons(mouse_buttons());
        global_wake(MOUSE_MOVE_WAKE_INTERVAL);
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
        MOUSE_WAKE_MONITOR_INSTALLED.store(true, Ordering::Relaxed);
        if let Some(token) = local_token {
            std::mem::forget(token);
        }
        if let Some(token) = global_token {
            std::mem::forget(token);
        }
    }
}

fn install_live_resize_monitor(proxy: Option<Res<EventLoopProxyWrapper>>) {
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

fn event_location_in_window_physical_px(event: &NSEvent) -> Option<(f32, f32)> {
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

fn event_window_is_key(event: &NSEvent) -> bool {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return false;
    };
    event.window(mtm).is_some_and(|window| window.isKeyWindow())
}

fn event_belongs_to_main_window_frame(event: &NSEvent) -> bool {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return false;
    };
    event
        .window(mtm)
        .is_some_and(|window| window.canBecomeMainWindow())
}

fn mouse_buttons() -> bevy_cef_core::prelude::NativeMouseButtons {
    let pressed = NSEvent::pressedMouseButtons();
    bevy_cef_core::prelude::NativeMouseButtons {
        left: pressed & 1 != 0,
        right: pressed & (1 << 1) != 0,
        middle: pressed & (1 << 2) != 0,
    }
}

pub(super) fn live_resize_active() -> bool {
    IN_LIVE_RESIZE.load(Ordering::Relaxed)
}

pub(super) fn pointer_inside_windowed_page() -> bool {
    vmux_browser::NativeLayout::pointer_is_inside()
        || WINDOWED_POINTER_INSIDE.load(Ordering::Relaxed)
}
