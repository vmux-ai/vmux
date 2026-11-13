use crate::window_interaction::{
    TitlebarClick, TitlebarClicks, WindowFrame, WindowResizeDrag, WindowTitlebarGesture, WindowZoom,
};
use bevy::prelude::*;
use bevy::winit::WINIT_WINDOWS;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSEvent, NSEventType, NSWindowStyleMask,
};
use objc2_foundation::{NSRect, NSString, NSUserDefaults};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::ptr::NonNull;
use std::rc::Rc;
use vmux_flex::ComputedNode;
use vmux_input::{PointerCapture, PointerSet, WindowGesture};

pub(super) struct WindowInputPlugin;

impl Plugin for WindowInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<WindowInputInbox>()
            .add_message::<vmux_ecs::overlay::PopupDismissRequest>()
            .add_message::<WindowInput>()
            .add_message::<WindowEffect>()
            .add_message::<WindowGestureFinished>()
            .configure_sets(
                PreUpdate,
                WindowInputSet
                    .after(PointerSet::Apply)
                    .before(PointerSet::Route),
            )
            .add_systems(
                PreUpdate,
                (
                    initialize,
                    ApplyDeferred,
                    cancel,
                    receive,
                    gestures,
                    effects,
                    finish,
                )
                    .chain()
                    .in_set(WindowInputSet),
            )
            .add_systems(Last, publish.after(PointerSet::Publish));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct WindowInputSet;

#[derive(Clone, Default)]
pub(super) struct WindowInputInbox(Rc<RefCell<WindowInputMailbox>>);

#[derive(Default)]
struct WindowInputMailbox {
    next: u64,
    pending: VecDeque<QueuedWindowInput>,
    handles: HashMap<u64, Retained<NSEvent>>,
    captured: Vec<Entity>,
    suppressed: Vec<Entity>,
    windowed: Vec<Entity>,
    resize_regions: Vec<(Entity, WindowFrame)>,
    layout: Vec<Entity>,
    pane_grips: Vec<(Entity, ComputedNode)>,
    reset: bool,
    deactivated: bool,
}

struct QueuedWindowInput {
    window: Entity,
    token: u64,
    event: Option<Retained<NSEvent>>,
    live_resize: Option<bool>,
}

impl WindowInputInbox {
    pub(super) fn physical_position(event: &NSEvent) -> Option<(f32, f32)> {
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
        let scale = window.backingScaleFactor();
        let x = point.x * scale;
        let y = if content.isFlipped() {
            point.y
        } else {
            content.bounds().size.height - point.y
        } * scale;
        if x.is_finite() && y.is_finite() {
            Some((x as f32, y as f32))
        } else {
            None
        }
    }

    pub(super) fn was_windowed(&self, window: Entity) -> bool {
        self.0.borrow().windowed.contains(&window)
    }

    pub(super) fn layout_inside(&self, window: Entity) -> bool {
        self.0.borrow().layout.contains(&window)
    }

    pub(super) fn grip_inside(&self, window: Entity, position: Vec2) -> bool {
        self.0
            .borrow()
            .pane_grips
            .iter()
            .any(|(owner, grip)| *owner == window && grip.contains(position))
    }

    pub(super) fn cancel_captured(&self) {
        let mut mailbox = self.0.borrow_mut();
        mailbox.pending.clear();
        mailbox.suppressed.clear();
        mailbox.reset = true;
    }

    pub(super) fn deactivate(&self) {
        let mut mailbox = self.0.borrow_mut();
        mailbox.pending.clear();
        mailbox.handles.clear();
        mailbox.suppressed.clear();
        mailbox.reset = true;
        mailbox.deactivated = true;
    }

    pub(super) fn queue(&self, window: Entity, event: &NSEvent) -> u64 {
        if !matches!(
            event.r#type(),
            NSEventType::LeftMouseDown | NSEventType::LeftMouseDragged | NSEventType::LeftMouseUp
        ) {
            return 0;
        }
        let Some(event) = (unsafe { Retained::retain(event as *const NSEvent as *mut NSEvent) })
        else {
            return 0;
        };
        let mut mailbox = self.0.borrow_mut();
        mailbox.next = mailbox.next.wrapping_add(1).max(1);
        let token = mailbox.next;
        if mailbox.pending.len() == 256 {
            mailbox.pending.clear();
            mailbox.reset = true;
        }
        mailbox.pending.push_back(QueuedWindowInput {
            window,
            token,
            event: Some(event),
            live_resize: None,
        });
        token
    }

    pub(super) fn queue_resize(&self, window: Entity, active: bool) {
        let mut mailbox = self.0.borrow_mut();
        if mailbox.pending.len() == 256 {
            mailbox.pending.clear();
            mailbox.reset = true;
        }
        mailbox.pending.push_back(QueuedWindowInput {
            window,
            token: 0,
            event: None,
            live_resize: Some(active),
        });
    }

    pub(super) fn suppress(&self, window: Entity, event: &NSEvent, position: Option<Vec2>) -> bool {
        let mut mailbox = self.0.borrow_mut();
        match event.r#type() {
            NSEventType::LeftMouseDown => {
                let drag = position.is_some_and(|position| {
                    vmux_browser::WindowDragRegion::contains_point(window, position.x, position.y)
                });
                let cursor = NSEvent::mouseLocation();
                let edge = mailbox.resize_regions.iter().any(|(owner, frame)| {
                    *owner == window
                        && cursor.x >= frame.x
                        && cursor.x <= frame.x + frame.width
                        && cursor.y >= frame.y
                        && cursor.y <= frame.y + frame.height
                });
                let pane = position.is_some_and(|position| {
                    mailbox
                        .pane_grips
                        .iter()
                        .any(|(owner, grip)| *owner == window && grip.contains(position))
                });
                if drag || edge || pane {
                    if !mailbox.suppressed.contains(&window) {
                        mailbox.suppressed.push(window);
                    }
                    return true;
                }
                mailbox.suppressed.retain(|owner| *owner != window);
                false
            }
            NSEventType::LeftMouseDragged => {
                mailbox.captured.contains(&window) || mailbox.suppressed.contains(&window)
            }
            NSEventType::LeftMouseUp => {
                let suppressed =
                    mailbox.captured.contains(&window) || mailbox.suppressed.contains(&window);
                mailbox.suppressed.retain(|owner| *owner != window);
                suppressed
            }
            _ => false,
        }
    }
}

#[derive(Message, Clone)]
struct WindowInput {
    window: Entity,
    token: u64,
    kind: WindowInputKind,
    frame: WindowFrame,
    screen: WindowFrame,
    cursor: Vec2,
    position: Vec2,
    minimum: Vec2,
    timestamp: f64,
    interval: f64,
    fullscreen: bool,
    preference: Option<String>,
}

#[derive(Clone, Copy)]
enum WindowInputKind {
    Down,
    Drag,
    Up,
    Cancel,
    LiveResize(bool),
}

#[derive(Message, Clone, Copy)]
struct WindowEffect {
    window: Entity,
    token: u64,
    gesture: WindowTitlebarGesture,
    frame: Option<WindowFrame>,
}

#[derive(Component, Default)]
struct WindowNativeDrag(Option<u64>);

#[derive(Message)]
struct WindowGestureFinished {
    window: Entity,
    token: u64,
    buttons: bevy_cef_core::prelude::NativeMouseButtons,
}

fn initialize(
    windows: Query<Entity, (With<Window>, Without<WindowResizeDrag>)>,
    mut commands: Commands,
) {
    for window in &windows {
        commands.entity(window).insert((
            WindowResizeDrag::default(),
            TitlebarClicks::default(),
            WindowZoom::default(),
            WindowNativeDrag::default(),
        ));
    }
}

fn cancel(
    mut windows: Query<(
        &vmux_input::PointerState,
        &mut WindowResizeDrag,
        &mut TitlebarClicks,
        &mut PointerCapture,
        &mut WindowGesture,
        &mut WindowNativeDrag,
    )>,
) {
    for (pointer, mut resize, mut clicks, mut capture, mut gesture, mut drag) in &mut windows {
        if !pointer.cancelled {
            continue;
        }
        resize.active = false;
        clicks.forget();
        capture.clear();
        gesture.live_resize = false;
        drag.0 = None;
    }
}

fn receive(
    inbox: NonSend<WindowInputInbox>,
    windows: Query<Entity, With<Window>>,
    mut inputs: MessageWriter<WindowInput>,
    mut dismiss: MessageWriter<vmux_ecs::overlay::PopupDismissRequest>,
) {
    let windows = windows.iter().collect::<Vec<_>>();
    let (messages, deactivated) = inbox.drain(&windows);
    if deactivated {
        dismiss.write_default();
    }
    for input in messages {
        inputs.write(input);
    }
}

impl WindowInputInbox {
    fn drain(&self, windows: &[Entity]) -> (Vec<WindowInput>, bool) {
        let mut mailbox = self.0.borrow_mut();
        let mut output = Vec::new();
        let deactivated = std::mem::take(&mut mailbox.deactivated);
        if std::mem::take(&mut mailbox.reset) {
            for &window in windows {
                output.push(WindowInput {
                    window,
                    token: 0,
                    kind: WindowInputKind::Cancel,
                    frame: default(),
                    screen: default(),
                    cursor: Vec2::ZERO,
                    position: Vec2::ZERO,
                    minimum: Vec2::ONE,
                    timestamp: 0.0,
                    interval: 0.0,
                    fullscreen: false,
                    preference: None,
                });
            }
        }
        let queued = mailbox.pending.drain(..).collect::<Vec<_>>();
        for queued in queued {
            if !windows.contains(&queued.window) {
                continue;
            }
            let mut input = WindowInput {
                window: queued.window,
                token: queued.token,
                kind: WindowInputKind::Cancel,
                frame: default(),
                screen: default(),
                cursor: Vec2::ZERO,
                position: Vec2::ZERO,
                minimum: Vec2::ONE,
                timestamp: 0.0,
                interval: 0.0,
                fullscreen: false,
                preference: None,
            };
            if let Some(active) = queued.live_resize {
                input.kind = WindowInputKind::LiveResize(active);
                output.push(input);
                continue;
            }
            let Some(event) = queued.event else {
                continue;
            };
            let Some(mtm) = objc2::MainThreadMarker::new() else {
                continue;
            };
            let Some(window) = event.window(mtm) else {
                continue;
            };
            input.kind = match event.r#type() {
                NSEventType::LeftMouseDown => WindowInputKind::Down,
                NSEventType::LeftMouseDragged => WindowInputKind::Drag,
                NSEventType::LeftMouseUp => WindowInputKind::Up,
                _ => continue,
            };
            input.frame = window.frame().into();
            input.screen = window
                .screen()
                .map(|screen| screen.visibleFrame().into())
                .unwrap_or(input.frame);
            let cursor = window.convertPointToScreen(event.locationInWindow());
            input.cursor = Vec2::new(cursor.x as f32, cursor.y as f32);
            let Some(position) = Self::physical_position(&event) else {
                continue;
            };
            input.position = Vec2::new(position.0, position.1);
            let minimum = window.minSize();
            input.minimum = Vec2::new(
                minimum.width.max(1.0) as f32,
                minimum.height.max(1.0) as f32,
            );
            input.timestamp = event.timestamp();
            input.interval = NSEvent::doubleClickInterval();
            input.fullscreen = window.styleMask().contains(NSWindowStyleMask::FullScreen);
            let preference = NSUserDefaults::standardUserDefaults()
                .stringForKey(&NSString::from_str("AppleActionOnDoubleClick"))
                .map(|value| value.to_string());
            input.preference = preference;
            mailbox.handles.insert(queued.token, event);
            output.push(input);
        }
        (output, deactivated)
    }
}

fn gestures(
    mut inputs: MessageReader<WindowInput>,
    mut windows: Query<
        (
            &mut WindowResizeDrag,
            &mut TitlebarClicks,
            &mut WindowZoom,
            &mut PointerCapture,
            &mut WindowGesture,
            &mut WindowNativeDrag,
            &vmux_input::PointerState,
        ),
        With<Window>,
    >,
    regions: vmux_browser::WindowDragRegions,
    mut effects: MessageWriter<WindowEffect>,
) {
    for input in inputs.read() {
        let Ok((
            mut resize,
            mut clicks,
            mut zoom,
            mut capture,
            mut gesture,
            mut native_drag,
            pointer,
        )) = windows.get_mut(input.window)
        else {
            continue;
        };
        if pointer.cancelled && !matches!(input.kind, WindowInputKind::Cancel) {
            continue;
        }
        match input.kind {
            WindowInputKind::Cancel => {
                resize.active = false;
                native_drag.0 = None;
                clicks.forget();
                capture.clear();
                gesture.live_resize = false;
            }
            WindowInputKind::LiveResize(active) => gesture.live_resize = active,
            WindowInputKind::Down => {
                native_drag.0 = None;
                let edges = input.frame.resize_edges(
                    input.cursor.x as f64,
                    input.cursor.y as f64,
                    WindowResizeDrag::GRIP_PX,
                );
                if !input.fullscreen && edges.any() {
                    *resize = WindowResizeDrag {
                        active: true,
                        frame: input.frame,
                        cursor_x: input.cursor.x as f64,
                        cursor_y: input.cursor.y as f64,
                        min_width: input.minimum.x as f64,
                        min_height: input.minimum.y as f64,
                        edges,
                    };
                    capture.claim(input.window, MouseButton::Left);
                    capture.consume(input.token);
                    gesture.live_resize = true;
                    clicks.forget();
                    continue;
                }
                if !regions.contains(input.window, input.position) {
                    clicks.forget();
                    continue;
                }
                let count = clicks.count(
                    TitlebarClick {
                        at: input.timestamp,
                        x: input.position.x,
                        y: input.position.y,
                    },
                    input.interval,
                    8.0,
                );
                let kind = WindowTitlebarGesture::resolve(count, input.preference.as_deref());
                let frame = if kind == WindowTitlebarGesture::Zoom {
                    Some(zoom.toggled(input.frame, input.screen))
                } else {
                    None
                };
                capture.claim(input.window, MouseButton::Left);
                capture.consume(input.token);
                if kind == WindowTitlebarGesture::Drag {
                    native_drag.0 = Some(input.token);
                }
                effects.write(WindowEffect {
                    window: input.window,
                    token: input.token,
                    gesture: kind,
                    frame,
                });
            }
            WindowInputKind::Drag | WindowInputKind::Up => {
                if matches!(input.kind, WindowInputKind::Up) {
                    native_drag.0 = None;
                }
                if resize.active {
                    capture.consume(input.token);
                    effects.write(WindowEffect {
                        window: input.window,
                        token: input.token,
                        gesture: WindowTitlebarGesture::Ignore,
                        frame: Some(
                            resize.resized_frame(input.cursor.x as f64, input.cursor.y as f64),
                        ),
                    });
                    if matches!(input.kind, WindowInputKind::Up) {
                        resize.active = false;
                        capture.release(MouseButton::Left);
                        gesture.live_resize = false;
                    }
                } else if capture.owner() == Some(input.window) {
                    capture.consume(input.token);
                    if matches!(input.kind, WindowInputKind::Up) {
                        capture.release(MouseButton::Left);
                    }
                }
            }
        }
    }
}

fn effects(
    mut requests: MessageReader<WindowEffect>,
    inbox: NonSend<WindowInputInbox>,
    drags: Query<&WindowNativeDrag>,
    mut windows: Query<(&mut Window, &vmux_input::PointerState)>,
    mut finished: MessageWriter<WindowGestureFinished>,
) {
    for request in requests.read() {
        if windows
            .get(request.window)
            .is_ok_and(|(_, pointer)| pointer.cancelled)
        {
            continue;
        }
        let Some(event) = inbox.0.borrow_mut().handles.remove(&request.token) else {
            continue;
        };
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            continue;
        };
        let Some(window) = event.window(mtm) else {
            continue;
        };
        let valid = WINIT_WINDOWS.with_borrow(|native| {
            let Some(native) = native.get_window(request.window) else {
                return false;
            };
            let Ok(handle) = native.window_handle() else {
                return false;
            };
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                return false;
            };
            let view: &objc2_app_kit::NSView =
                unsafe { &*handle.ns_view.as_ptr().cast::<objc2_app_kit::NSView>() };
            view.window()
                .is_some_and(|current| std::ptr::eq(&*current, &*window))
        });
        if !valid {
            continue;
        }
        if let Some(frame) = request.frame {
            let target: NSRect = frame.into();
            if request.gesture == WindowTitlebarGesture::Zoom {
                let duration = window.animationResizeTime(target);
                let changes = block2::RcBlock::new(move |context: NonNull<NSAnimationContext>| {
                    unsafe { context.as_ref() }.setDuration(duration);
                    window.animator().setFrame_display(target, true);
                });
                NSAnimationContext::runAnimationGroup(&changes);
            } else {
                let style = window.styleMask();
                if !style.contains(NSWindowStyleMask::Resizable) {
                    window.setStyleMask(style | NSWindowStyleMask::Resizable);
                }
                window.setFrame_display(target, true);
                if let (Some(content), Ok((mut canonical, _))) =
                    (window.contentView(), windows.get_mut(request.window))
                {
                    let bounds = content.bounds();
                    let scale = window.backingScaleFactor();
                    canonical.resolution.set_physical_resolution(
                        (bounds.size.width * scale).round().max(1.0) as u32,
                        (bounds.size.height * scale).round().max(1.0) as u32,
                    );
                }
            }
        } else {
            match request.gesture {
                WindowTitlebarGesture::Drag => {
                    if !drags
                        .get(request.window)
                        .is_ok_and(|drag| drag.0 == Some(request.token))
                    {
                        continue;
                    }
                    if drags
                        .get(request.window)
                        .is_ok_and(|drag| drag.0 == Some(request.token))
                        && NSEvent::pressedMouseButtons() & 1 != 0
                    {
                        window.performWindowDragWithEvent(&event);
                    }
                    let buttons = NSEvent::pressedMouseButtons();
                    finished.write(WindowGestureFinished {
                        window: request.window,
                        token: request.token,
                        buttons: bevy_cef_core::prelude::NativeMouseButtons {
                            left: buttons & 1 != 0,
                            right: buttons & 2 != 0,
                            middle: buttons & 4 != 0,
                        },
                    });
                }
                WindowTitlebarGesture::Miniaturize => window.miniaturize(None),
                WindowTitlebarGesture::Zoom | WindowTitlebarGesture::Ignore => {}
            }
        }
    }
    inbox.0.borrow_mut().handles.clear();
}

fn finish(
    mut events: MessageReader<WindowGestureFinished>,
    mut windows: Query<(
        &mut PointerCapture,
        &mut vmux_input::PointerState,
        &mut WindowNativeDrag,
    )>,
    mut updated: MessageWriter<vmux_input::PointerUpdated>,
) {
    for event in events.read() {
        let Ok((mut capture, mut pointer, mut native_drag)) = windows.get_mut(event.window) else {
            continue;
        };
        if native_drag.0 != Some(event.token) {
            continue;
        }
        capture.release(MouseButton::Left);
        native_drag.0 = None;
        let mut input = vmux_input::PointerInput::buttons(event.window, event.buttons);
        input.native_token = event.token;
        updated.write(pointer.update(input));
    }
}

fn publish(
    inbox: NonSend<WindowInputInbox>,
    windows: Query<
        (
            Entity,
            &PointerCapture,
            Option<&vmux_browser::WindowPointerRegions>,
            Option<&vmux_layout::pane::ResizeGripMasks>,
        ),
        With<Window>,
    >,
    drags: Query<(), With<vmux_layout::pane::PaneDrag>>,
) {
    let mut mailbox = inbox.0.borrow_mut();
    mailbox.captured.clear();
    mailbox.windowed.clear();
    mailbox.layout.clear();
    mailbox.resize_regions.clear();
    mailbox.pane_grips.clear();
    WINIT_WINDOWS.with_borrow(|native| {
        for (entity, _, _, _) in &windows {
            let Some(window) = native.get_window(entity) else {
                continue;
            };
            let Ok(handle) = window.window_handle() else {
                continue;
            };
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                continue;
            };
            let view: &objc2_app_kit::NSView =
                unsafe { &*handle.ns_view.as_ptr().cast::<objc2_app_kit::NSView>() };
            let Some(window) = view.window() else {
                continue;
            };
            if window.styleMask().contains(NSWindowStyleMask::FullScreen) {
                continue;
            }
            let frame = window.frame();
            let grip = WindowResizeDrag::GRIP_PX;
            let x = frame.origin.x;
            let y = frame.origin.y;
            let width = frame.size.width;
            let height = frame.size.height;
            for rect in [
                WindowFrame {
                    x: x - grip,
                    y: y - grip,
                    width: grip * 2.0,
                    height: height + grip * 2.0,
                },
                WindowFrame {
                    x: x + width - grip,
                    y: y - grip,
                    width: grip * 2.0,
                    height: height + grip * 2.0,
                },
                WindowFrame {
                    x: x - grip,
                    y: y - grip,
                    width: width + grip * 2.0,
                    height: grip * 2.0,
                },
                WindowFrame {
                    x: x - grip,
                    y: y + height - grip,
                    width: width + grip * 2.0,
                    height: grip * 2.0,
                },
            ] {
                mailbox.resize_regions.push((entity, rect));
            }
        }
    });
    for (window, capture, regions, masks) in &windows {
        if capture
            .owner()
            .is_some_and(|owner| owner == window || drags.contains(owner))
        {
            mailbox.captured.push(window);
        }
        if let Some(masks) = masks {
            for grip in &masks.0 {
                mailbox.pane_grips.push((window, *grip));
            }
        }
        if let Some(regions) = regions {
            if regions.windowed {
                mailbox.windowed.push(window);
            }
            if regions.layout {
                mailbox.layout.push(window);
            }
        }
    }
    let captured = mailbox.captured.clone();
    mailbox
        .suppressed
        .retain(|window| captured.contains(window));
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NativeInputFixture {
        app: App,
        first: Entity,
        second: Entity,
    }

    impl NativeInputFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                bevy::input::InputPlugin,
                vmux_input::PointerPlugin,
                WindowInputPlugin,
            ))
            .add_message::<bevy::window::CursorMoved>()
            .add_message::<bevy::window::WindowFocused>();
            let first = app.world_mut().spawn(Window::default()).id();
            let second = app.world_mut().spawn(Window::default()).id();
            app.update();
            Self { app, first, second }
        }

        fn input(&self, kind: WindowInputKind, token: u64, cursor: Vec2) -> WindowInput {
            WindowInput {
                window: self.first,
                token,
                kind,
                frame: WindowFrame {
                    x: 100.0,
                    y: 100.0,
                    width: 800.0,
                    height: 600.0,
                },
                screen: default(),
                cursor,
                position: Vec2::ZERO,
                minimum: Vec2::new(200.0, 120.0),
                timestamp: token as f64,
                interval: 0.5,
                fullscreen: false,
                preference: None,
            }
        }
    }

    #[test]
    fn native_window_resize_preserves_same_frame_order_and_window_scope() {
        let mut fixture = NativeInputFixture::new();
        for input in [
            fixture.input(WindowInputKind::Down, 1, Vec2::splat(100.0)),
            fixture.input(WindowInputKind::Drag, 2, Vec2::new(850.0, 650.0)),
            fixture.input(WindowInputKind::Up, 3, Vec2::new(850.0, 650.0)),
        ] {
            fixture.app.world_mut().write_message(input);
        }
        fixture.app.update();
        let effects = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<WindowEffect>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(effects.len(), 2);
        for effect in effects {
            assert_eq!(effect.window, fixture.first);
            assert_eq!(
                effect.frame,
                Some(WindowFrame {
                    x: 700.0,
                    y: 580.0,
                    width: 200.0,
                    height: 120.0
                })
            );
        }
        assert!(
            !fixture
                .app
                .world()
                .get::<WindowResizeDrag>(fixture.first)
                .unwrap()
                .active
        );
        assert!(
            !fixture
                .app
                .world()
                .get::<WindowResizeDrag>(fixture.second)
                .unwrap()
                .active
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<PointerCapture>(fixture.first)
                .unwrap()
                .owner(),
            None
        );
        assert!(
            !fixture
                .app
                .world()
                .get::<WindowGesture>(fixture.first)
                .unwrap()
                .live_resize
        );
    }

    #[test]
    fn native_window_input_cancellation_releases_the_resize_owner() {
        let mut fixture = NativeInputFixture::new();
        let input = fixture.input(WindowInputKind::Down, 1, Vec2::splat(100.0));
        fixture.app.world_mut().write_message(input);
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<WindowResizeDrag>(fixture.first)
                .unwrap()
                .active
        );
        fixture
            .app
            .world()
            .non_send::<WindowInputInbox>()
            .cancel_captured();
        fixture.app.update();
        assert!(
            !fixture
                .app
                .world()
                .get::<WindowResizeDrag>(fixture.first)
                .unwrap()
                .active
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<PointerCapture>(fixture.first)
                .unwrap()
                .owner(),
            None
        );
    }
}
