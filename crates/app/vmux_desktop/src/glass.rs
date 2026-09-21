use std::collections::HashMap;
use std::time::{Duration, Instant};

use bevy::prelude::*;

pub(crate) struct GlassPlugin;

impl Plugin for GlassPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<GlassState>()
            .add_systems(PreUpdate, install_window_glass)
            .add_systems(
                Update,
                (
                    sync_window_glass_visibility,
                    keep_window_surface_layer_transparent,
                ),
            )
            .add_systems(
                Update,
                handle_toggle_fullscreen_command.in_set(vmux_command::ReadAppCommands),
            )
            .add_systems(
                Last,
                (
                    reveal_window_after_layout_ready,
                    restore_fullscreen_after_reveal,
                    ensure_window_active_after_reveal,
                )
                    .chain(),
            );
    }
}

const ACTIVATION_RETRY_BUDGET: Duration = Duration::from_secs(3);

#[derive(Default)]
struct GlassState(HashMap<Entity, WindowGlass>);

#[derive(Default)]
struct WindowGlass {
    visible: bool,
    shadowed: bool,
    revealed: bool,
    revealed_at: Option<Instant>,
    active_confirmed: bool,
    _glass: Option<objc2::rc::Retained<objc2_app_kit::NSGlassEffectView>>,
    _backdrop_window: Option<objc2::rc::Retained<objc2_app_kit::NSPanel>>,
    _parent_window: Option<objc2::rc::Retained<objc2_app_kit::NSWindow>>,
}

impl WindowGlass {
    fn track_parent_frame(&self) {
        use objc2::ClassType;
        use objc2_app_kit::{NSWindowDidMoveNotification, NSWindowDidResizeNotification};
        use objc2_foundation::{NSNotification, NSNotificationCenter};
        use std::ptr::NonNull;

        let (Some(backdrop), Some(parent)) = (&self._backdrop_window, &self._parent_window) else {
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
            std::mem::forget(token);
        }
    }
}

fn install_window_glass(mut state: NonSendMut<GlassState>, windows: Query<(Entity, &Window)>) {
    use bevy::winit::WINIT_WINDOWS;
    use objc2::{ClassType, MainThreadMarker, MainThreadOnly, rc::Retained, runtime::AnyClass};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSGlassEffectView,
        NSGlassEffectViewStyle, NSPanel, NSView, NSWindowCollectionBehavior, NSWindowOrderingMode,
        NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    for (entity, window) in &windows {
        if state.0.contains_key(&entity) {
            continue;
        }
        let ns_view = WINIT_WINDOWS.with_borrow(|windows| {
            let id = windows.entity_to_winit.get(&entity)?;
            let wrapper = windows.windows.get(id)?;
            let handle = wrapper.window_handle().ok()?;
            match handle.as_raw() {
                RawWindowHandle::AppKit(h) => Some(h.ns_view),
                _ => None,
            }
        });
        let Some(ns_view) = ns_view else {
            continue;
        };
        let content: &NSView = unsafe { &*ns_view.as_ptr().cast::<NSView>() };
        let Some(parent_window) = content.window() else {
            continue;
        };
        if AnyClass::get(c"NSGlassEffectView").is_none() {
            warn!("glass: NSGlassEffectView unavailable (needs macOS 26+)");
            state.0.insert(
                entity,
                WindowGlass {
                    revealed: window.visible,
                    ..default()
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
        let installed = WindowGlass {
            visible: true,
            shadowed: false,
            revealed: window.visible,
            revealed_at: window.visible.then(Instant::now),
            active_confirmed: window.visible,
            _glass: Some(glass),
            _backdrop_window: Some(backdrop_window),
            _parent_window: Some(parent_window),
        };
        installed.track_parent_frame();
        state.0.insert(entity, installed);
        info!(
            ?entity,
            "glass: NSGlassEffectView installed in nonactivating child-window backdrop"
        );
    }
}

fn reveal_window_after_layout_ready(
    mut state: NonSendMut<GlassState>,
    mut windows: Query<(Entity, &mut Window)>,
    status: Res<crate::boot_status::SplashStatus>,
) {
    if !status.reveal_ready {
        return;
    }
    for (entity, mut window) in &mut windows {
        let Some(glass) = state.0.get_mut(&entity) else {
            continue;
        };
        if glass.revealed {
            continue;
        }
        window.visible = true;
        glass.revealed = true;
        glass.revealed_at = Some(Instant::now());
    }
}

fn restore_fullscreen_after_reveal(
    state: NonSend<GlassState>,
    primary_window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    pending: Option<Res<crate::window_state::PendingFullscreenRestore>>,
    mut commands: Commands,
) {
    use objc2_app_kit::NSWindowStyleMask;

    let Some(pending) = pending else {
        return;
    };
    let Some(glass) = primary_window
        .single()
        .ok()
        .and_then(|window| state.0.get(&window))
    else {
        return;
    };
    if !glass.revealed {
        return;
    }
    if pending.0
        && let Some(parent_window) = &glass._parent_window
        && !parent_window
            .styleMask()
            .contains(NSWindowStyleMask::FullScreen)
    {
        parent_window.toggleFullScreen(None);
    }
    commands.remove_resource::<crate::window_state::PendingFullscreenRestore>();
    commands.insert_resource(crate::window_state::WindowRestoreComplete);
}

fn should_attempt_activation(
    revealed: bool,
    active_confirmed: bool,
    elapsed_since_reveal: Option<Duration>,
) -> bool {
    if !revealed || active_confirmed {
        return false;
    }
    match elapsed_since_reveal {
        Some(elapsed) => elapsed < ACTIVATION_RETRY_BUDGET,
        None => true,
    }
}

fn ensure_window_active_after_reveal(
    mut state: NonSendMut<GlassState>,
    windows: Query<Entity, With<Window>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    for entity in &windows {
        let Some(glass) = state.0.get_mut(&entity) else {
            continue;
        };
        let elapsed = glass.revealed_at.map(|at| at.elapsed());
        if !should_attempt_activation(glass.revealed, glass.active_confirmed, elapsed) {
            continue;
        }
        if crate::runtime::ensure_native_window_active(entity) {
            glass.active_confirmed = true;
        } else if let Some(proxy) = proxy.as_ref() {
            let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
    }
}

fn handle_toggle_fullscreen_command(
    state: NonSend<GlassState>,
    focused_window: Res<vmux_layout::window::FocusedWindow>,
    mut reader: MessageReader<vmux_command::AppCommand>,
) {
    use vmux_command::{AppCommand, LayoutCommand, WindowCommand};

    let toggle = reader.read().any(|cmd| {
        matches!(
            cmd,
            AppCommand::Layout(LayoutCommand::Window(WindowCommand::ToggleFullscreen))
        )
    });
    if toggle
        && let Some(parent_window) = focused_window
            .0
            .and_then(|window| state.0.get(&window))
            .and_then(|glass| glass._parent_window.as_ref())
    {
        parent_window.toggleFullScreen(None);
    }
}

fn sync_window_glass_visibility(
    mut state: NonSendMut<GlassState>,
    mut clear_color: ResMut<vmux_layout::window::WindowBackground>,
    mut window_q: Query<(Entity, &mut bevy::window::Window)>,
    focused_window: Res<vmux_layout::window::FocusedWindow>,
    mut window_fullscreen: ResMut<crate::window_state::WindowFullscreen>,
) {
    use objc2::ClassType;
    use objc2_app_kit::NSWindowStyleMask;

    let mut focused_fullscreen = false;
    let exit_fullscreen = crate::native_keyboard::take_exit_fullscreen_request();
    state.0.retain(|entity, _| window_q.contains(*entity));
    for (entity, mut window) in &mut window_q {
        let Some(glass) = state.0.get_mut(&entity) else {
            continue;
        };
        let bevy_fullscreen = matches!(
            window.mode,
            bevy::window::WindowMode::BorderlessFullscreen(_)
                | bevy::window::WindowMode::Fullscreen(..)
        );
        let native_fullscreen = glass
            ._parent_window
            .as_ref()
            .is_some_and(|window| window.styleMask().contains(NSWindowStyleMask::FullScreen));
        let fullscreen = bevy_fullscreen || native_fullscreen;
        if focused_window.0 == Some(entity) {
            focused_fullscreen = fullscreen;
            if exit_fullscreen {
                if native_fullscreen {
                    if let Some(parent_window) = &glass._parent_window {
                        parent_window.toggleFullScreen(None);
                    }
                } else {
                    window.mode = bevy::window::WindowMode::Windowed;
                }
            }
        }

        let visible = !fullscreen;
        if let (Some(backdrop_window), Some(parent_window)) =
            (&glass._backdrop_window, &glass._parent_window)
        {
            let backdrop_window: &objc2_app_kit::NSWindow = backdrop_window.as_super();
            backdrop_window.setFrame_display(parent_window.frame(), false);
            let shadowed =
                focus_shadow_visible(focused_window.0 == Some(entity), window.visible, fullscreen);
            if glass.shadowed != shadowed {
                backdrop_window.setHasShadow(shadowed);
                backdrop_window.invalidateShadow();
                glass.shadowed = shadowed;
            }
        }
        if glass.visible == visible {
            continue;
        }
        if let Some(effect) = &glass._glass {
            let glass_view: &objc2_app_kit::NSView = effect;
            glass_view.setHidden(!visible);
        }
        glass.visible = visible;
    }

    if window_fullscreen.0 != focused_fullscreen {
        window_fullscreen.0 = focused_fullscreen;
    }

    let [r, g, b] = vmux_layout::window::WINDOW_BACKGROUND_SRGB;
    let want_clear = if focused_fullscreen {
        Color::srgb(r, g, b)
    } else {
        Color::NONE
    };
    if clear_color.0 != want_clear {
        clear_color.0 = want_clear;
    }

    crate::native_keyboard::set_window_fullscreen(focused_fullscreen);
}

fn focus_shadow_visible(focused: bool, visible: bool, fullscreen: bool) -> bool {
    focused && visible && !fullscreen
}

fn content_view_ptr(entity: Entity) -> Option<*mut core::ffi::c_void> {
    use bevy::winit::WINIT_WINDOWS;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
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

fn keep_window_surface_layer_transparent(windows: Query<Entity, With<Window>>) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSColor, NSView};

    if MainThreadMarker::new().is_none() {
        return;
    }
    for entity in &windows {
        let Some(ns_view) = content_view_ptr(entity) else {
            continue;
        };
        let content: &NSView = unsafe { &*ns_view.cast::<NSView>() };
        content.setWantsLayer(true);
        let Some(layer) = content.layer() else {
            continue;
        };
        let clear_color = NSColor::clearColor();
        layer.setOpaque(false);
        layer.setBackgroundColor(Some(&clear_color.CGColor()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reveal_test_app(reveal_ready: bool) -> App {
        let mut app = App::new();
        app.add_systems(Update, reveal_window_after_layout_ready);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    visible: false,
                    ..default()
                },
                bevy::window::PrimaryWindow,
            ))
            .id();
        let mut state = GlassState::default();
        state.0.insert(window, WindowGlass::default());
        app.world_mut().insert_non_send(state);
        app.insert_resource(crate::boot_status::SplashStatus {
            phase: crate::boot_status::BootPhase::Starting,
            reveal_ready,
        });
        app
    }

    #[test]
    fn startup_window_stays_hidden_until_reveal_ready() {
        let mut app = reveal_test_app(false);

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
        let mut app = reveal_test_app(true);

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
        assert!(!should_attempt_activation(false, false, None));
    }

    #[test]
    fn activates_immediately_after_reveal() {
        assert!(should_attempt_activation(true, false, None));
        assert!(should_attempt_activation(true, false, Some(Duration::ZERO)));
    }

    #[test]
    fn stops_once_confirmed() {
        assert!(!should_attempt_activation(
            true,
            true,
            Some(Duration::from_millis(10))
        ));
    }

    #[test]
    fn retries_within_budget_then_gives_up() {
        assert!(should_attempt_activation(
            true,
            false,
            Some(ACTIVATION_RETRY_BUDGET - Duration::from_millis(1))
        ));
        assert!(!should_attempt_activation(
            true,
            false,
            Some(ACTIVATION_RETRY_BUDGET)
        ));
    }

    #[test]
    fn only_the_focused_visible_window_gets_the_emphasis_shadow() {
        assert!(focus_shadow_visible(true, true, false));
        assert!(!focus_shadow_visible(false, true, false));
        assert!(!focus_shadow_visible(true, false, false));
        assert!(!focus_shadow_visible(true, true, true));
    }
}
