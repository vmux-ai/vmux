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

/// How long to keep re-asserting activation after reveal before giving up, so a degenerate case
/// (activation never confirms) cannot wake the loop forever.
const ACTIVATION_RETRY_BUDGET: Duration = Duration::from_secs(3);

#[derive(Default)]
struct GlassState {
    installed: bool,
    visible: bool,
    revealed: bool,
    revealed_at: Option<Instant>,
    active_confirmed: bool,
    _glass: Option<objc2::rc::Retained<objc2_app_kit::NSGlassEffectView>>,
    _backdrop_window: Option<objc2::rc::Retained<objc2_app_kit::NSPanel>>,
    _parent_window: Option<objc2::rc::Retained<objc2_app_kit::NSWindow>>,
}

fn install_window_glass(
    mut state: NonSendMut<GlassState>,
    window: Query<Entity, With<bevy::window::PrimaryWindow>>,
) {
    use bevy::winit::WINIT_WINDOWS;
    use objc2::{ClassType, MainThreadMarker, MainThreadOnly, rc::Retained, runtime::AnyClass};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSGlassEffectView,
        NSGlassEffectViewStyle, NSPanel, NSView, NSWindowCollectionBehavior, NSWindowOrderingMode,
        NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    if state.installed {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Ok(entity) = window.single() else {
        return;
    };
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
        return;
    };
    let content: &NSView = unsafe { &*ns_view.as_ptr().cast::<NSView>() };
    let Some(parent_window) = content.window() else {
        return;
    };
    if AnyClass::get(c"NSGlassEffectView").is_none() {
        warn!("glass: NSGlassEffectView unavailable (needs macOS 26+)");
        state.installed = true;
        return;
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
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
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
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    backdrop.setContentView(Some(glass_view));
    unsafe {
        parent_window.addChildWindow_ordered(backdrop, NSWindowOrderingMode::Below);
    }
    state.visible = true;
    state._glass = Some(glass);
    state._backdrop_window = Some(backdrop_window);
    state._parent_window = Some(parent_window);
    state.installed = true;
    info!("glass: NSGlassEffectView installed in nonactivating child-window backdrop");
}

fn reveal_window_after_layout_ready(
    mut state: NonSendMut<GlassState>,
    mut window: Query<(Entity, &mut Window), With<bevy::window::PrimaryWindow>>,
    status: Res<crate::boot_status::SplashStatus>,
) {
    if state.revealed || !state.installed || !status.reveal_ready {
        return;
    }
    let Ok((_, mut window)) = window.single_mut() else {
        return;
    };
    window.visible = true;
    state.revealed = true;
    state.revealed_at = Some(Instant::now());
}

/// After reveal, apply the persisted fullscreen intent once: enter native
/// fullscreen if it was saved, then mark restore complete so geometry capture
/// can begin. Consumes [`crate::window_state::PendingFullscreenRestore`].
fn restore_fullscreen_after_reveal(
    state: NonSend<GlassState>,
    pending: Option<Res<crate::window_state::PendingFullscreenRestore>>,
    mut commands: Commands,
) {
    use objc2_app_kit::NSWindowStyleMask;

    let Some(pending) = pending else {
        return;
    };
    if !state.revealed {
        return;
    }
    if pending.0
        && let Some(parent_window) = &state._parent_window
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

/// The reveal frame shows the window, but the app is still in the background (the splash is a
/// nonactivating panel). Activation is async, so retry it each frame until the app is active and
/// the window is key, waking the loop in between so the retry actually runs.
fn ensure_window_active_after_reveal(
    mut state: NonSendMut<GlassState>,
    window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    let elapsed = state.revealed_at.map(|at| at.elapsed());
    if !should_attempt_activation(state.revealed, state.active_confirmed, elapsed) {
        return;
    }
    let Ok(entity) = window.single() else {
        return;
    };
    if crate::runtime::ensure_native_window_active(entity) {
        state.active_confirmed = true;
    } else if let Some(proxy) = proxy {
        let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
    }
}

/// Toggle native macOS fullscreen when the `ToggleFullscreen` command fires (Ctrl+Cmd+F).
/// vmux hides the native window buttons, so this is the entry point into/out of fullscreen.
fn handle_toggle_fullscreen_command(
    state: NonSend<GlassState>,
    mut reader: MessageReader<vmux_command::AppCommand>,
) {
    use vmux_command::{AppCommand, LayoutCommand, WindowCommand};

    let toggle = reader.read().any(|cmd| {
        matches!(
            cmd,
            AppCommand::Layout(LayoutCommand::Window(WindowCommand::ToggleFullscreen))
        )
    });
    if toggle && let Some(parent_window) = &state._parent_window {
        parent_window.toggleFullScreen(None);
    }
}

fn sync_window_glass_visibility(
    mut state: NonSendMut<GlassState>,
    mut clear_color: ResMut<vmux_layout::window::WindowBackground>,
    mut window_q: Query<&mut bevy::window::Window, With<bevy::window::PrimaryWindow>>,
    mut window_fullscreen: ResMut<crate::window_state::WindowFullscreen>,
) {
    use objc2::ClassType;
    use objc2_app_kit::NSWindowStyleMask;

    let bevy_fullscreen = window_q
        .single()
        .map(|w| {
            matches!(
                w.mode,
                bevy::window::WindowMode::BorderlessFullscreen(_)
                    | bevy::window::WindowMode::Fullscreen(..)
            )
        })
        .unwrap_or(false);
    let native_fullscreen = state
        ._parent_window
        .as_ref()
        .is_some_and(|w| w.styleMask().contains(NSWindowStyleMask::FullScreen));
    let fullscreen = bevy_fullscreen || native_fullscreen;

    if window_fullscreen.0 != fullscreen {
        window_fullscreen.0 = fullscreen;
    }

    let [r, g, b] = vmux_layout::window::WINDOW_BACKGROUND_SRGB;
    let want_clear = if fullscreen {
        Color::srgb(r, g, b)
    } else {
        Color::NONE
    };
    if clear_color.0 != want_clear {
        clear_color.0 = want_clear;
    }

    crate::native_keyboard::set_window_fullscreen(fullscreen);

    if crate::native_keyboard::take_exit_fullscreen_request() {
        if native_fullscreen {
            if let Some(parent_window) = &state._parent_window {
                parent_window.toggleFullScreen(None);
            }
        } else if let Ok(mut window) = window_q.single_mut() {
            window.mode = bevy::window::WindowMode::Windowed;
        }
        return;
    }

    let visible = !fullscreen;
    if let (Some(backdrop_window), Some(parent_window)) =
        (&state._backdrop_window, &state._parent_window)
    {
        let backdrop_window: &objc2_app_kit::NSWindow = backdrop_window.as_super();
        backdrop_window.setFrame_display(parent_window.frame(), false);
    }
    if state.visible == visible {
        return;
    }
    if let Some(glass) = &state._glass {
        let glass_view: &objc2_app_kit::NSView = glass;
        glass_view.setHidden(!visible);
    }
    state.visible = visible;
}

fn primary_content_view_ptr(entity: Entity) -> Option<*mut core::ffi::c_void> {
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

fn keep_window_surface_layer_transparent(window: Query<Entity, With<bevy::window::PrimaryWindow>>) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSColor, NSView};

    if MainThreadMarker::new().is_none() {
        return;
    }
    let Ok(entity) = window.single() else {
        return;
    };
    let Some(ns_view) = primary_content_view_ptr(entity) else {
        return;
    };
    let content: &NSView = unsafe { &*ns_view.cast::<NSView>() };
    content.setWantsLayer(true);
    let Some(layer) = content.layer() else {
        return;
    };
    let clear_color = NSColor::clearColor();
    layer.setOpaque(false);
    layer.setBackgroundColor(Some(&clear_color.CGColor()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_install_does_not_reveal_window() {
        let source = include_str!("glass.rs");
        let install = source
            .split("fn install_window_glass")
            .nth(1)
            .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
            .unwrap_or_default();

        assert!(!install.contains("window.visible = true"));
        assert!(!install.contains("activate_native_window"));
    }

    #[test]
    fn window_backdrop_uses_clear_glass_style() {
        let source = include_str!("glass.rs");
        let install = source
            .split("fn install_window_glass")
            .nth(1)
            .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
            .unwrap_or_default();

        assert!(install.contains("NSGlassEffectViewStyle::Clear"));
        assert!(!install.contains("NSGlassEffectViewStyle::Regular"));
    }

    #[test]
    fn window_backdrop_uses_clear_glass_tint() {
        let source = include_str!("glass.rs");
        let install = source
            .split("fn install_window_glass")
            .nth(1)
            .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
            .unwrap_or_default();

        assert!(install.contains("glass.setTintColor(Some(&NSColor::clearColor()))"));
    }

    #[test]
    fn window_backdrop_lives_in_nonactivating_child_window() {
        let source = include_str!("glass.rs");
        let install = source
            .split("fn install_window_glass")
            .nth(1)
            .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
            .unwrap_or_default();

        assert!(install.contains("NSPanel"));
        assert!(install.contains("NSWindowStyleMask::NonactivatingPanel"));
        assert!(install.contains("setIgnoresMouseEvents(true)"));
        assert!(install.contains("addChildWindow_ordered"));
        assert!(install.contains("NSWindowOrderingMode::Below"));
    }

    #[test]
    fn window_backdrop_tracks_parent_window_frame() {
        let source = include_str!("glass.rs");
        let sync = source
            .split("fn sync_window_glass_visibility")
            .nth(1)
            .and_then(|tail| tail.split("fn primary_content_view_ptr").next())
            .unwrap_or_default();

        assert!(sync.contains("backdrop_window.setFrame_display(parent_window.frame(), false)"));
    }

    #[test]
    fn desktop_enables_nspanel_binding_for_glass_backdrop() {
        let manifest = include_str!("../Cargo.toml");

        assert!(manifest.contains("\"objc2-app-kit/NSPanel\""));
    }

    fn reveal_test_app(reveal_ready: bool) -> App {
        let mut app = App::new();
        app.add_systems(Update, reveal_window_after_layout_ready);
        app.world_mut().insert_non_send(GlassState {
            installed: true,
            ..default()
        });
        app.world_mut().spawn((
            Window {
                visible: false,
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));
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
    fn reveal_does_not_activate_inline() {
        let source = include_str!("glass.rs");
        let reveal = source
            .split("fn reveal_window_after_layout_ready")
            .nth(1)
            .and_then(|tail| tail.split("fn should_attempt_activation").next())
            .unwrap_or_default();

        assert!(!reveal.contains("activate_native_window"));
        assert!(reveal.contains("state.revealed_at = Some(Instant::now())"));
    }

    #[test]
    fn activation_retry_system_is_registered() {
        let source = include_str!("glass.rs");
        let build = source
            .split("fn build(&self, app: &mut App)")
            .nth(1)
            .and_then(|tail| tail.split("#[derive(Default)]").next())
            .unwrap_or_default();

        assert!(build.contains("ensure_window_active_after_reveal"));
    }

    #[test]
    fn surface_transparency_system_is_registered() {
        let source = include_str!("glass.rs");
        let build = source
            .split("fn build(&self, app: &mut App)")
            .nth(1)
            .and_then(|tail| tail.split("#[derive(Default)]").next())
            .unwrap_or_default();

        assert!(build.contains("keep_window_surface_layer_transparent"));
    }

    #[test]
    fn surface_layer_kept_non_opaque_and_clear() {
        let source = include_str!("glass.rs");
        let func = source
            .split("fn keep_window_surface_layer_transparent")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(test)]").next())
            .unwrap_or_default();

        assert!(func.contains("layer.setOpaque(false)"));
        assert!(func.contains("layer.setBackgroundColor(Some(&clear_color.CGColor()))"));
    }
}
