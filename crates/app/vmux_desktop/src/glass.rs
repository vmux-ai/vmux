use std::collections::HashMap;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::winit::WINIT_WINDOWS;
use objc2::{
    ClassType, MainThreadMarker, MainThreadOnly,
    rc::Retained,
    runtime::{AnyClass, AnyObject},
};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSGlassEffectView,
    NSGlassEffectViewStyle, NSPanel, NSView, NSWindowCollectionBehavior,
    NSWindowDidMoveNotification, NSWindowDidResizeNotification, NSWindowOrderingMode,
    NSWindowStyleMask,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use vmux_layout::window::ToggleFullscreenRequest;

pub(crate) struct GlassPlugin;

impl Plugin for GlassPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<GlassHandles>()
            .add_message::<vmux_input::ExitFullscreenShortcut>()
            .add_systems(PreUpdate, install_window)
            .add_systems(
                Update,
                (
                    sync_window_visibility.in_set(vmux_ecs::WindowFullscreenSet),
                    keep_surface_background,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                toggle_fullscreen.in_set(vmux_command::ReadCommandRequests),
            )
            .add_systems(
                Last,
                (
                    reveal_window,
                    restore_fullscreen_after_reveal,
                    activate_revealed_window,
                )
                    .chain()
                    .in_set(crate::boot_status::WindowRevealSet),
            );
    }
}

#[derive(Component, Default)]
struct WindowGlassState {
    visible: bool,
    shadowed: bool,
    revealed: bool,
    revealed_at: Option<Instant>,
    active_confirmed: bool,
}

impl WindowGlassState {
    const ACTIVATION_RETRY_BUDGET: Duration = Duration::from_secs(3);

    fn should_attempt_activation(&self) -> bool {
        if !self.revealed || self.active_confirmed {
            return false;
        }
        match self.revealed_at.map(|at| at.elapsed()) {
            Some(elapsed) => elapsed < Self::ACTIVATION_RETRY_BUDGET,
            None => true,
        }
    }

    fn focus_shadow_visible(focused: bool, visible: bool, fullscreen: bool) -> bool {
        focused && visible && !fullscreen
    }

    fn fullscreen_background(appearance: vmux_setting::ResolvedScheme) -> [f32; 3] {
        match appearance {
            vmux_setting::ResolvedScheme::Light => {
                vmux_layout::window::WINDOW_BACKGROUND_LIGHT_SRGB
            }
            vmux_setting::ResolvedScheme::Dark => vmux_layout::window::WINDOW_BACKGROUND_DARK_SRGB,
        }
    }
}

#[derive(Default)]
struct GlassHandles(HashMap<Entity, NativeWindowGlass>);

struct NativeWindowGlass {
    glass: Option<Retained<NSGlassEffectView>>,
    backdrop_window: Option<Retained<NSPanel>>,
    parent_window: Option<Retained<objc2_app_kit::NSWindow>>,
    observers: Vec<Retained<AnyObject>>,
}

impl NativeWindowGlass {
    fn attach_observers(&mut self) {
        let (Some(backdrop), Some(parent)) = (&self.backdrop_window, &self.parent_window) else {
            return;
        };
        let follower = backdrop.clone();
        let tracked = parent.clone();
        let block = block2::RcBlock::new(move |_n: NonNull<NSNotification>| {
            let follower: &objc2_app_kit::NSWindow = follower.as_super();
            follower.setFrame_display(tracked.frame(), false);
        });
        let center = NSNotificationCenter::defaultCenter();
        let names = unsafe { [NSWindowDidResizeNotification, NSWindowDidMoveNotification] };
        for name in names {
            let token = unsafe {
                center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    Some(parent),
                    None,
                    &block,
                )
            };
            let raw = Retained::into_raw(token) as *mut AnyObject;
            if let Some(observer) = unsafe { Retained::from_raw(raw) } {
                self.observers.push(observer);
            }
        }
    }

    fn content_view_ptr(entity: Entity) -> Option<*mut core::ffi::c_void> {
        WINIT_WINDOWS.with_borrow(|windows| {
            let id = windows.entity_to_winit.get(&entity)?;
            let wrapper = windows.windows.get(id)?;
            let handle = wrapper.window_handle().ok()?;
            match handle.as_raw() {
                RawWindowHandle::AppKit(h) => Some(h.ns_view.as_ptr()),
                _ => None,
            }
        })
    }
}

impl Drop for NativeWindowGlass {
    fn drop(&mut self) {
        if self.observers.is_empty() {
            return;
        }
        let center = NSNotificationCenter::defaultCenter();
        for token in self.observers.drain(..) {
            unsafe {
                center.removeObserver(&token);
            }
        }
    }
}

fn install_window(
    mut handles: NonSendMut<GlassHandles>,
    windows: Query<(Entity, &Window), Without<WindowGlassState>>,
    mut commands: Commands,
) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    for (entity, window) in &windows {
        let Some(ns_view) = NativeWindowGlass::content_view_ptr(entity) else {
            continue;
        };
        let content: &NSView = unsafe { &*ns_view.cast::<NSView>() };
        let Some(parent_window) = content.window() else {
            continue;
        };
        if AnyClass::get(c"NSGlassEffectView").is_none() {
            warn!("glass: NSGlassEffectView unavailable (needs macOS 26+)");
            commands.entity(entity).insert(WindowGlassState {
                revealed: window.visible,
                ..default()
            });
            handles.0.insert(
                entity,
                NativeWindowGlass {
                    glass: None,
                    backdrop_window: None,
                    parent_window: None,
                    observers: Vec::new(),
                },
            );
            continue;
        }
        let frame = parent_window.frame();
        let backdrop_window = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            frame,
            NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            NSBackingStoreType::Buffered,
            false,
        );
        let clear_color = NSColor::clearColor();
        let backdrop: &objc2_app_kit::NSWindow = backdrop_window.as_super();
        backdrop.setOpaque(false);
        backdrop.setBackgroundColor(Some(&clear_color));
        backdrop.setHasShadow(false);
        backdrop.setIgnoresMouseEvents(true);
        backdrop.setCanHide(false);
        backdrop.setHidesOnDeactivate(false);
        backdrop_window.setFloatingPanel(false);
        backdrop_window.setBecomesKeyOnlyIfNeeded(true);
        backdrop.setCollectionBehavior(
            NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        let glass: Retained<NSGlassEffectView> = NSGlassEffectView::new(mtm);
        glass.setStyle(NSGlassEffectViewStyle::Clear);
        glass.setTintColor(Some(&NSColor::clearColor()));
        let glass_view: &NSView = &glass;
        glass_view.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(frame.size.width, frame.size.height),
        ));
        glass_view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        backdrop.setContentView(Some(glass_view));
        unsafe {
            parent_window.addChildWindow_ordered(backdrop, NSWindowOrderingMode::Below);
        }
        parent_window.setHasShadow(false);
        let mut native = NativeWindowGlass {
            glass: Some(glass),
            backdrop_window: Some(backdrop_window),
            parent_window: Some(parent_window),
            observers: Vec::new(),
        };
        native.attach_observers();
        handles.0.insert(entity, native);
        commands.entity(entity).insert(WindowGlassState {
            visible: true,
            shadowed: false,
            revealed: window.visible,
            revealed_at: window.visible.then(Instant::now),
            active_confirmed: window.visible,
        });
        info!(
            ?entity,
            "glass: NSGlassEffectView installed in nonactivating child-window backdrop"
        );
    }
}

fn reveal_window(
    mut windows: Query<(&mut Window, &mut WindowGlassState)>,
    status: Single<&crate::boot_status::SplashStatus>,
) {
    if !status.reveal_ready {
        return;
    }
    for (mut window, mut state) in &mut windows {
        if state.revealed {
            continue;
        }
        window.visible = true;
        state.revealed = true;
        state.revealed_at = Some(Instant::now());
    }
}

fn restore_fullscreen_after_reveal(
    handles: NonSend<GlassHandles>,
    primary_window: Query<
        (
            Entity,
            &WindowGlassState,
            &crate::window::PendingFullscreenRestore,
        ),
        With<bevy::window::PrimaryWindow>,
    >,
    mut commands: Commands,
) {
    let Ok((window, state, pending)) = primary_window.single() else {
        return;
    };
    if !state.revealed {
        return;
    }
    if pending.0
        && let Some(parent_window) = handles
            .0
            .get(&window)
            .and_then(|native| native.parent_window.as_ref())
        && !parent_window
            .styleMask()
            .contains(NSWindowStyleMask::FullScreen)
    {
        parent_window.toggleFullScreen(None);
    }
    commands
        .entity(window)
        .remove::<crate::window::PendingFullscreenRestore>()
        .insert(crate::window::WindowRestoreComplete);
}

fn activate_revealed_window(
    mut windows: Query<(Entity, &mut WindowGlassState), With<Window>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    for (entity, mut state) in &mut windows {
        if !state.should_attempt_activation() {
            continue;
        }
        if crate::macos::MacWindow::ensure_key(entity) {
            state.active_confirmed = true;
        } else if let Some(proxy) = proxy.as_ref() {
            let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
    }
}

fn toggle_fullscreen(
    handles: NonSend<GlassHandles>,
    focused_window: vmux_layout::window::FocusedWindow,
    mut reader: MessageReader<ToggleFullscreenRequest>,
) {
    let toggle = reader.read().count() > 0;
    if toggle
        && let Some(parent_window) = focused_window
            .entity()
            .and_then(|window| handles.0.get(&window))
            .and_then(|native| native.parent_window.as_ref())
    {
        parent_window.toggleFullScreen(None);
    }
}

fn sync_window_visibility(
    mut handles: NonSendMut<GlassHandles>,
    mut clear_color: ResMut<vmux_layout::window::WindowBackground>,
    appearance: Single<&vmux_setting::ResolvedColorScheme>,
    mut window_q: Query<(
        Entity,
        &mut bevy::window::Window,
        &mut vmux_ecs::WindowFullscreen,
        &mut WindowGlassState,
    )>,
    focused_window: vmux_layout::window::FocusedWindow,
    mut exit_fullscreen: MessageReader<crate::window::ExitFullscreenRequest>,
    mut shortcut: MessageReader<vmux_input::ExitFullscreenShortcut>,
) {
    let mut focused_fullscreen = false;
    let exit_requested = exit_fullscreen.read().count() > 0;
    let shortcut_requested = shortcut.read().count() > 0;
    let exit_fullscreen = exit_requested || shortcut_requested;
    handles.0.retain(|entity, _| window_q.contains(*entity));
    for (entity, mut window, mut window_fullscreen, mut state) in &mut window_q {
        let Some(native) = handles.0.get(&entity) else {
            continue;
        };
        let bevy_fullscreen = matches!(
            window.mode,
            bevy::window::WindowMode::BorderlessFullscreen(_)
                | bevy::window::WindowMode::Fullscreen(..)
        );
        let native_fullscreen = native
            .parent_window
            .as_ref()
            .is_some_and(|window| window.styleMask().contains(NSWindowStyleMask::FullScreen));
        let fullscreen = bevy_fullscreen || native_fullscreen;
        if focused_window.entity() == Some(entity) {
            focused_fullscreen = fullscreen;
            if exit_fullscreen {
                if native_fullscreen {
                    if let Some(parent_window) = &native.parent_window {
                        parent_window.toggleFullScreen(None);
                    }
                } else {
                    window.mode = bevy::window::WindowMode::Windowed;
                }
            }
        }
        if window_fullscreen.0 != fullscreen {
            window_fullscreen.0 = fullscreen;
        }

        let visible = !fullscreen;
        if let (Some(backdrop_window), Some(parent_window)) =
            (&native.backdrop_window, &native.parent_window)
        {
            let backdrop_window: &objc2_app_kit::NSWindow = backdrop_window.as_super();
            backdrop_window.setFrame_display(parent_window.frame(), false);
            let shadowed = WindowGlassState::focus_shadow_visible(
                focused_window.entity() == Some(entity),
                window.visible,
                fullscreen,
            );
            if state.shadowed != shadowed {
                backdrop_window.setHasShadow(shadowed);
                backdrop_window.invalidateShadow();
                state.shadowed = shadowed;
            }
        }
        if state.visible == visible {
            continue;
        }
        if let Some(effect) = &native.glass {
            let glass_view: &NSView = effect;
            glass_view.setHidden(!visible);
        }
        state.visible = visible;
    }

    let [r, g, b] = WindowGlassState::fullscreen_background(appearance.0);
    let want_clear = if focused_fullscreen {
        Color::srgb(r, g, b)
    } else {
        Color::NONE
    };
    if clear_color.0 != want_clear {
        clear_color.0 = want_clear;
    }
}

fn keep_surface_background(
    windows: Query<(Entity, &vmux_ecs::WindowFullscreen), With<Window>>,
    appearance: Single<&vmux_setting::ResolvedColorScheme>,
) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    for (entity, fullscreen) in &windows {
        let Some(ns_view) = NativeWindowGlass::content_view_ptr(entity) else {
            continue;
        };
        let content: &NSView = unsafe { &*ns_view.cast::<NSView>() };
        content.setWantsLayer(true);
        let Some(layer) = content.layer() else {
            continue;
        };
        let color = if fullscreen.0 {
            let [red, green, blue] = WindowGlassState::fullscreen_background(appearance.0);
            NSColor::colorWithSRGBRed_green_blue_alpha(red.into(), green.into(), blue.into(), 1.0)
        } else {
            NSColor::clearColor()
        };
        layer.setOpaque(fullscreen.0);
        layer.setBackgroundColor(Some(&color.CGColor()));
        if let Some(window) = content.window() {
            window.setOpaque(fullscreen.0);
            window.setBackgroundColor(Some(&color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl WindowGlassState {
        fn reveal_test_app(reveal_ready: bool) -> App {
            let mut app = App::new();
            app.add_systems(Update, reveal_window);
            app.world_mut().spawn((
                Window {
                    visible: false,
                    ..default()
                },
                WindowGlassState::default(),
                bevy::window::PrimaryWindow,
            ));
            app.world_mut().spawn(crate::boot_status::SplashStatus {
                phase: crate::boot_status::BootPhase::Starting,
                reveal_ready,
            });
            app
        }
    }

    #[test]
    fn startup_window_stays_hidden_until_reveal_ready() {
        let mut app = WindowGlassState::reveal_test_app(false);

        app.update();

        let window = app
            .world_mut()
            .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
            .single(app.world())
            .expect("primary window");
        assert!(!window.visible);
    }

    #[test]
    fn startup_window_reveals_after_reveal_ready() {
        let mut app = WindowGlassState::reveal_test_app(true);

        app.update();

        let window = app
            .world_mut()
            .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
            .single(app.world())
            .expect("primary window");
        assert!(window.visible);
    }

    #[test]
    fn no_activation_before_reveal() {
        let state = WindowGlassState::default();
        assert!(!state.should_attempt_activation());
    }

    #[test]
    fn activates_immediately_after_reveal() {
        let immediate = WindowGlassState {
            revealed: true,
            ..default()
        };
        let fresh = WindowGlassState {
            revealed: true,
            revealed_at: Some(Instant::now()),
            ..default()
        };
        assert!(immediate.should_attempt_activation());
        assert!(fresh.should_attempt_activation());
    }

    #[test]
    fn stops_once_confirmed() {
        let state = WindowGlassState {
            revealed: true,
            revealed_at: Some(Instant::now()),
            active_confirmed: true,
            ..default()
        };
        assert!(!state.should_attempt_activation());
    }

    #[test]
    fn retries_within_budget_then_gives_up() {
        let within = WindowGlassState {
            revealed: true,
            revealed_at: Some(
                Instant::now()
                    .checked_sub(
                        WindowGlassState::ACTIVATION_RETRY_BUDGET - Duration::from_millis(50),
                    )
                    .unwrap(),
            ),
            ..default()
        };
        let expired = WindowGlassState {
            revealed: true,
            revealed_at: Some(
                Instant::now()
                    .checked_sub(
                        WindowGlassState::ACTIVATION_RETRY_BUDGET + Duration::from_millis(10),
                    )
                    .unwrap(),
            ),
            ..default()
        };
        assert!(within.should_attempt_activation());
        assert!(!expired.should_attempt_activation());
    }

    #[test]
    fn only_the_focused_visible_window_gets_the_emphasis_shadow() {
        assert!(WindowGlassState::focus_shadow_visible(true, true, false));
        assert!(!WindowGlassState::focus_shadow_visible(false, true, false));
        assert!(!WindowGlassState::focus_shadow_visible(true, false, false));
        assert!(!WindowGlassState::focus_shadow_visible(true, true, true));
    }
}
