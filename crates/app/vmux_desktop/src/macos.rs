use bevy::window::Window;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WINIT_WINDOWS, WinitUserEvent};
use bevy_cef_core::prelude::MessageLoopWakePolicy;
use objc2::runtime::{AnyClass, NSObjectProtocol};
use objc2_app_kit::{
    NSApp, NSApplicationDidResignActiveNotification, NSEvent, NSEventMask, NSEventModifierFlags,
    NSEventPhase, NSEventType, NSView, NSWindowDidEndLiveResizeNotification,
    NSWindowWillStartLiveResizeNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use vmux_api::input::KeyModifiers;
use vmux_flex::prelude::*;
use vmux_setting::{ResolvedScheme, SystemAppearance};

use crate::window_interaction::{WindowFrame, WindowPointerPolicy};

pub(super) struct RuntimePlatformPlugin;

impl Plugin for RuntimePlatformPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<NativeInputHandles>()
            .add_plugins(crate::window_input::WindowInputPlugin)
            .add_systems(Startup, seed)
            .add_message::<vmux_ecs::overlay::PopupDismissRequest>()
            .add_message::<KeyWindowRequest>()
            .add_systems(Update, activate_app_during_boot)
            .add_systems(Update, (grab_key_window_on_pane_hover, key).chain())
            .add_systems(
                Startup,
                (
                    install_input_wake_monitor,
                    install_live_resize_monitor,
                    install_activation_monitor,
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

#[derive(Default)]
struct NativeInputHandles {
    pointers: Option<vmux_input::PointerInbox>,
    monitors: Vec<objc2::rc::Retained<objc2::runtime::AnyObject>>,
    observers: Vec<objc2::rc::Retained<objc2::runtime::AnyObject>>,
}

impl NativeInputHandles {
    fn observe(
        &mut self,
        token: objc2::rc::Retained<
            objc2::runtime::ProtocolObject<dyn objc2_foundation::NSObjectProtocol>,
        >,
    ) {
        let token = objc2::rc::Retained::into_raw(token) as *mut objc2::runtime::AnyObject;
        if let Some(token) = unsafe { objc2::rc::Retained::from_raw(token) } {
            self.observers.push(token);
        }
    }
}

impl Drop for NativeInputHandles {
    fn drop(&mut self) {
        if let Some(pointers) = &self.pointers {
            pointers.set_native_enabled(false);
        }
        for token in &self.monitors {
            unsafe {
                NSEvent::removeMonitor(token);
            }
        }
        let center = NSNotificationCenter::defaultCenter();
        for token in &self.observers {
            unsafe {
                center.removeObserver(token);
            }
        }
    }
}

fn install_activation_monitor(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut handles: NonSendMut<NativeInputHandles>,
    pointers: Res<vmux_input::PointerInbox>,
    windows: NonSend<crate::window_input::WindowInputInbox>,
    wheel: Res<vmux_input::WheelInbox>,
) {
    let Some(proxy) = proxy else {
        return;
    };
    let proxy = (**proxy).clone();
    let pointers = pointers.clone();
    let windows = windows.clone();
    let wheel = wheel.clone();
    let block = block2::RcBlock::new(move |_notification: NonNull<NSNotification>| {
        pointers.cancel();
        windows.deactivate();
        HistoryScroll::cancel(None, &wheel);
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
    let token = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSApplicationDidResignActiveNotification),
            None,
            None,
            &block,
        )
    };
    handles.observe(token);
}

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
    mut events: MessageReader<vmux_input::PointerUpdated>,
    windows: Query<&Window>,
    hierarchy: vmux_layout::window::WindowHierarchy,
    popup_windows: Query<
        Option<&bevy_cef::prelude::HostWindow>,
        (
            With<vmux_browser::PopupWebview>,
            With<vmux_ecs::KeyboardOwner>,
        ),
    >,
    panes: Query<
        (Entity, &ComputedNode),
        (
            With<vmux_layout::pane::Pane>,
            Without<vmux_layout::pane::PaneSplit>,
        ),
    >,
    mut requests: MessageWriter<KeyWindowRequest>,
) {
    if !app_is_frontmost() {
        return;
    }
    for event in events.read() {
        if !event.motion || event.cancelled {
            continue;
        }
        let Some(position) = event.position_px else {
            continue;
        };
        if !windows.get(event.window).is_ok_and(|window| window.visible) {
            continue;
        }
        if popup_windows
            .iter()
            .any(|host| host.is_none_or(|host| host.0 == event.window))
        {
            continue;
        }
        if panes.iter().any(|(pane, node)| {
            hierarchy.get(pane) == Some(event.window) && node.contains(position)
        }) {
            requests.write(KeyWindowRequest(event.window));
        }
    }
}

#[derive(Message)]
struct KeyWindowRequest(Entity);

fn key(mut requests: MessageReader<KeyWindowRequest>) {
    for request in requests.read() {
        MacWindow::ensure_key(request.0);
    }
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

pub(crate) struct MacWindow;

impl MacWindow {
    pub(crate) fn ensure_key(window_entity: Entity) -> bool {
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

fn install_input_wake_monitor(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    cef_wake_policy: Res<MessageLoopWakePolicy>,
    pointer_inbox: Res<vmux_input::PointerInbox>,
    wheel_inbox: Res<vmux_input::WheelInbox>,
    window_inputs: NonSend<crate::window_input::WindowInputInbox>,
    mut handles: NonSendMut<NativeInputHandles>,
) {
    let Some(proxy) = proxy else {
        return;
    };
    let proxy = (**proxy).clone();
    let wake = wake_throttle("mouse-wake-throttle", move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
    let local = InputMonitor {
        pointer: pointer_inbox.clone(),
        wheel: wheel_inbox.clone(),
        windows: window_inputs.clone(),
        cef_wake_policy: cef_wake_policy.clone(),
        wake: wake.clone(),
    };
    let local_block =
        block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent { local.local(event) });
    let global_pointer = pointer_inbox.clone();
    let global_window_inputs = window_inputs.clone();
    let global_wake = wake.clone();
    let global_block = block2::RcBlock::new(move |event: NonNull<NSEvent>| {
        let event_type = unsafe { event.as_ref() }.r#type();
        if event_type == NSEventType::LeftMouseUp {
            global_window_inputs.cancel_captured();
        }
        global_pointer.release_outside(mouse_buttons());
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
    let local_mask = mouse_mask
        | NSEventMask::ScrollWheel
        | NSEventMask::Magnify
        | NSEventMask::Rotate
        | NSEventMask::KeyDown
        | NSEventMask::KeyUp
        | NSEventMask::FlagsChanged;
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
    if let Some(token) = local_token {
        pointer_inbox.set_native_enabled(true);
        handles.pointers = Some(pointer_inbox.clone());
        handles.monitors.push(token);
    }
    if let Some(token) = global_token {
        handles.pointers = Some(pointer_inbox.clone());
        handles.monitors.push(token);
    }
}

struct InputMonitor {
    pointer: vmux_input::PointerInbox,
    wheel: vmux_input::WheelInbox,
    windows: crate::window_input::WindowInputInbox,
    cef_wake_policy: MessageLoopWakePolicy,
    wake: WakeThrottle,
}

impl InputMonitor {
    fn local(&self, event: NonNull<NSEvent>) -> *mut NSEvent {
        let input = WindowInput(unsafe { event.as_ref() });
        let Some((source, direct)) = input.origin(&self.pointer) else {
            (self.wake)(MOUSE_DRAG_WAKE_INTERVAL);
            return event.as_ptr();
        };
        let event_type = input.0.r#type();
        let scroll = event_type == NSEventType::ScrollWheel;
        let native_webview = input.targets_webview();
        if !scroll || !native_webview {
            self.cef_wake_policy.begin_interaction();
        }
        let location = crate::window_input::WindowInputInbox::physical_position(input.0);
        let position = location.map(|(x, y)| Vec2::new(x, y));
        let capture = direct && self.windows.suppress(source, input.0, position);
        let token = if direct {
            self.windows.queue(source, input.0)
        } else {
            0
        };
        if scroll {
            HistoryScroll::record(input.0, source, &self.wheel);
        } else if matches!(
            event_type,
            NSEventType::Magnify
                | NSEventType::Rotate
                | NSEventType::KeyDown
                | NSEventType::FlagsChanged
        ) {
            HistoryScroll::cancel(Some(source), &self.wheel);
        }
        let previous = self.pointer.position(source);
        let motion = self.pointer(&input, source, position, token);
        self.wake(&input, source, previous, position, native_webview, motion);
        if capture {
            return std::ptr::null_mut();
        }
        event.as_ptr()
    }

    fn pointer(
        &self,
        input: &WindowInput<'_>,
        source: Entity,
        position: Option<Vec2>,
        token: u64,
    ) -> bool {
        let event_type = input.0.r#type();
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
        let buttons = mouse_buttons();
        if motion || button_event || event_type == NSEventType::FlagsChanged {
            let mut sample = match position {
                Some(position) => {
                    vmux_input::PointerInput::sample(source, position, buttons, motion)
                }
                None => vmux_input::PointerInput::buttons(source, buttons),
            };
            sample.native_token = token;
            let flags = input.0.modifierFlags();
            sample.modifiers = Some(KeyModifiers {
                shift: flags.contains(NSEventModifierFlags::Shift),
                ctrl: flags.contains(NSEventModifierFlags::Control),
                alt: flags.contains(NSEventModifierFlags::Option),
                super_key: flags.contains(NSEventModifierFlags::Command),
            });
            self.pointer.send(sample);
        }
        motion
    }

    fn wake(
        &self,
        input: &WindowInput<'_>,
        source: Entity,
        previous: Option<Vec2>,
        position: Option<Vec2>,
        native_webview: bool,
        motion: bool,
    ) {
        let event_type = input.0.r#type();
        let page_changed = match (previous, position) {
            (Some(previous), Some(position)) => {
                vmux_browser::NativeBridge::windowed_page_changed(previous, position)
            }
            _ => false,
        };
        let scale = objc2::MainThreadMarker::new()
            .and_then(|marker| input.0.window(marker))
            .map_or(1.0, |window| window.backingScaleFactor() as f32);
        let near_edge = position.is_some_and(|position| {
            vmux_browser::NativeBridge::windowed_page_edge_contains(
                position,
                vmux_layout::pane::ResizeGrip::minimum_width(scale) * 0.5,
            )
        });
        let over_grip = position.is_some_and(|position| self.windows.grip_inside(source, position));
        let sampled = position.is_some_and(|position| {
            vmux_browser::NativeBridge::windowed_page_contains_point(position.x, position.y)
        });
        if motion {
            let interval = if event_type == NSEventType::MouseMoved {
                MOUSE_MOVE_WAKE_INTERVAL
            } else {
                MOUSE_DRAG_WAKE_INTERVAL
            };
            if WindowPointerPolicy::motion_should_wake(
                event_type != NSEventType::MouseMoved,
                sampled,
                self.windows.was_windowed(source),
                event_window_is_key(input.0),
                page_changed,
            ) || near_edge
                || over_grip
            {
                (self.wake)(interval);
            }
        } else if event_type == NSEventType::ScrollWheel {
            if WindowPointerPolicy::scroll_should_wake(
                self.windows.layout_inside(source),
                sampled,
                native_webview,
            ) {
                (self.wake)(MOUSE_DRAG_WAKE_INTERVAL);
            }
        } else {
            (self.wake)(MOUSE_DRAG_WAKE_INTERVAL);
        }
    }
}

struct WindowInput<'a>(&'a NSEvent);

impl WindowInput<'_> {
    fn origin(&self, pointer: &vmux_input::PointerInbox) -> Option<(Entity, bool)> {
        let native_window = objc2::MainThreadMarker::new().and_then(|mtm| self.0.window(mtm));
        let direct = native_window.as_ref().and_then(|window| {
            pointer.source((&**window as *const objc2_app_kit::NSWindow) as isize)
        });
        let mut source = direct;
        let mut parent = native_window;
        for _ in 0..8 {
            if source.is_some() {
                break;
            }
            let window = parent?;
            parent = window.parentWindow();
            source = parent.as_ref().and_then(|window| {
                pointer.source((&**window as *const objc2_app_kit::NSWindow) as isize)
            });
        }
        source.map(|source| (source, direct.is_some()))
    }

    fn targets_webview(&self) -> bool {
        let Some(marker) = objc2::MainThreadMarker::new() else {
            return false;
        };
        let Some(window) = self.0.window(marker) else {
            return false;
        };
        let Some(root) = window.contentView() else {
            return false;
        };
        let Some(class) = AnyClass::get(c"WKWebView") else {
            return false;
        };
        let position = match unsafe { root.superview() } {
            Some(parent) => parent.convertPoint_fromView(self.0.locationInWindow(), None),
            None => self.0.locationInWindow(),
        };
        let mut target = root.hitTest(position);
        while let Some(view) = target {
            if view.isKindOfClass(class) {
                return true;
            }
            target = unsafe { view.superview() };
        }
        false
    }
}

fn install_live_resize_monitor(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    pointers: Res<vmux_input::PointerInbox>,
    window_inputs: NonSend<crate::window_input::WindowInputInbox>,
    mut handles: NonSendMut<NativeInputHandles>,
) {
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
    let start_pointers = pointers.clone();
    let start_inputs = window_inputs.clone();
    let start_block = block2::RcBlock::new(move |notification: NonNull<NSNotification>| {
        if let Some(object) = unsafe { notification.as_ref() }.object()
            && let Some(window) =
                start_pointers.source((&*object as *const objc2::runtime::AnyObject) as isize)
        {
            start_inputs.queue_resize(window, true);
        }
        let _ = start_proxy.send_event(WinitUserEvent::WakeUp);
    });
    let end_proxy = (**proxy).clone();
    let end_pointers = pointers.clone();
    let end_inputs = window_inputs.clone();
    let end_block = block2::RcBlock::new(move |notification: NonNull<NSNotification>| {
        if let Some(object) = unsafe { notification.as_ref() }.object()
            && let Some(window) =
                end_pointers.source((&*object as *const objc2::runtime::AnyObject) as isize)
        {
            end_inputs.queue_resize(window, false);
        }
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
    handles.observe(start_token);
    handles.observe(end_token);
}

struct HistoryScroll;

impl HistoryScroll {
    fn record(event: &NSEvent, window: Entity, inbox: &vmux_input::WheelInbox) {
        let source = event.CGEvent();
        let flags = event.modifierFlags();
        let Some(position) = Self::position(event) else {
            Self::cancel(Some(window), inbox);
            return;
        };
        let phase = if event.momentumPhase() != NSEventPhase::None {
            vmux_input::WheelPhase::Momentum
        } else if event.phase().intersects(NSEventPhase::Cancelled) {
            vmux_input::WheelPhase::Cancel
        } else if event.phase().intersects(NSEventPhase::Ended) {
            vmux_input::WheelPhase::End
        } else if event
            .phase()
            .intersects(NSEventPhase::Began | NSEventPhase::MayBegin)
        {
            vmux_input::WheelPhase::Begin
        } else if event
            .phase()
            .intersects(NSEventPhase::Changed | NSEventPhase::Stationary)
        {
            vmux_input::WheelPhase::Changed
        } else {
            vmux_input::WheelPhase::Unsupported
        };
        let delta = Vec2::new(
            -event.scrollingDeltaX() as f32,
            -event.scrollingDeltaY() as f32,
        );
        inbox.record(vmux_input::NativeWheelInput {
            window: Some(window),
            position,
            delta,
            phase,
            at: Instant::now(),
            has_native_event: source.is_some(),
            precise: event.hasPreciseScrollingDeltas(),
            modifiers: KeyModifiers {
                shift: flags.contains(NSEventModifierFlags::Shift),
                ctrl: flags.contains(NSEventModifierFlags::Control),
                alt: flags.contains(NSEventModifierFlags::Option),
                super_key: flags.contains(NSEventModifierFlags::Command),
            },
        });
    }

    fn position(event: &NSEvent) -> Option<Vec2> {
        let mtm = objc2::MainThreadMarker::new()?;
        let mut window = event.window(mtm)?;
        let screen = window.convertPointToScreen(event.locationInWindow());
        for _ in 0..8 {
            let Some(parent) = window.parentWindow() else {
                break;
            };
            window = parent;
        }
        let content = window.contentView()?;
        let point = content.convertPoint_fromView(window.convertPointFromScreen(screen), None);
        let y = if content.isFlipped() {
            point.y
        } else {
            content.bounds().size.height - point.y
        };
        let scale = window.backingScaleFactor();
        let position = Vec2::new((point.x * scale) as f32, (y * scale) as f32);
        if !position.is_finite() || !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        Some(position)
    }

    fn cancel(window: Option<Entity>, inbox: &vmux_input::WheelInbox) {
        inbox.record(vmux_input::NativeWheelInput {
            window,
            position: Vec2::ZERO,
            delta: Vec2::ZERO,
            phase: vmux_input::WheelPhase::Cancel,
            at: Instant::now(),
            has_native_event: false,
            precise: false,
            modifiers: KeyModifiers::default(),
        });
    }
}

fn event_window_is_key(event: &NSEvent) -> bool {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return false;
    };
    event.window(mtm).is_some_and(|window| window.isKeyWindow())
}

fn mouse_buttons() -> bevy_cef_core::prelude::NativeMouseButtons {
    let pressed = NSEvent::pressedMouseButtons();
    bevy_cef_core::prelude::NativeMouseButtons {
        left: pressed & 1 != 0,
        right: pressed & (1 << 1) != 0,
        middle: pressed & (1 << 2) != 0,
    }
}
