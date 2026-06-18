use std::time::{Duration, Instant};

use bevy::prelude::*;
use vmux_layout::cef::LayoutCef;
use vmux_layout::scene::InteractionMode;

/// How long to keep re-asserting activation after reveal before giving up, so a degenerate case
/// (activation never confirms) cannot wake the loop forever.
const ACTIVATION_RETRY_BUDGET: Duration = Duration::from_secs(3);

pub(crate) struct GlassPlugin;

impl Plugin for GlassPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<GlassState>()
            .init_non_send::<LayoutOverlay>()
            .init_non_send::<CommandBarOverlay>()
            .add_systems(PreUpdate, install_window_glass)
            .add_systems(Update, sync_window_glass_visibility)
            .add_systems(
                Update,
                handle_toggle_fullscreen_command.in_set(vmux_command::ReadAppCommands),
            )
            .add_systems(
                Last,
                (
                    reveal_window_after_layout_ready,
                    ensure_window_active_after_reveal,
                    sync_layout_overlay,
                    sync_command_bar_overlay,
                )
                    .chain(),
            );
    }
}

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
    if crate::background_lifecycle::ensure_native_window_active(entity) {
        state.active_confirmed = true;
    } else if let Some(proxy) = proxy {
        let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
    }
}

fn glass_backdrop_visible(mode: InteractionMode) -> bool {
    mode == InteractionMode::User
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
    mode: Res<InteractionMode>,
    mut clear_color: ResMut<ClearColor>,
    mut window_q: Query<&mut bevy::window::Window, With<bevy::window::PrimaryWindow>>,
    terminal_focus_q: Query<
        (),
        (
            With<vmux_terminal::Terminal>,
            With<bevy_cef::prelude::CefKeyboardTarget>,
        ),
    >,
    modal_open_q: Query<
        (&Node, Has<bevy_cef::prelude::CefKeyboardTarget>),
        With<vmux_layout::window::Modal>,
    >,
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

    let [r, g, b] = vmux_layout::window::WINDOW_BACKGROUND_SRGB;
    let want_clear = if fullscreen {
        Color::srgb(r, g, b)
    } else {
        Color::NONE
    };
    if clear_color.0 != want_clear {
        clear_color.0 = want_clear;
    }

    let terminal_focused = !terminal_focus_q.is_empty();
    let command_bar_open = vmux_layout::command_bar::handler::is_command_bar_open(&modal_open_q);
    crate::native_keyboard::set_escape_exits_fullscreen(
        fullscreen && !terminal_focused && !command_bar_open,
    );

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

    let visible = glass_backdrop_visible(*mode) && !fullscreen;
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

#[derive(Default)]
struct LayoutOverlay {
    layer: Option<objc2::rc::Retained<objc2_quartz_core::CALayer>>,
    shown: bool,
    held: Option<bevy_cef_core::prelude::AcceleratedFrame>,
}

#[derive(Default)]
struct CommandBarOverlay {
    view: Option<objc2::rc::Retained<objc2_app_kit::NSView>>,
    shown: bool,
    /// Keeps the currently-displayed IOSurface alive while it's the overlay layer's contents.
    held: Option<bevy_cef_core::prelude::AcceleratedFrame>,
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

fn sync_layout_overlay(
    mut state: NonSendMut<LayoutOverlay>,
    layout_e_q: Query<Entity, With<LayoutCef>>,
    window_q: Query<Entity, With<bevy::window::PrimaryWindow>>,
    windows: Query<&bevy::window::Window>,
    mode: Res<InteractionMode>,
    overlay_frames: Res<bevy_cef::prelude::NativeOverlayFrames>,
) {
    use objc2::{MainThreadMarker, rc::Retained, runtime::AnyObject};
    use objc2_app_kit::{NSColor, NSView};
    use objc2_quartz_core::CALayer;

    if *mode != InteractionMode::User {
        if state.shown {
            if let Some(layer) = &state.layer {
                layer.setHidden(true);
            }
            state.shown = false;
        }
        return;
    }
    let Some(_mtm) = MainThreadMarker::new() else {
        return;
    };
    let (Ok(window_e), Ok(layout_e)) = (window_q.single(), layout_e_q.single()) else {
        return;
    };
    let next = overlay_frames
        .0
        .lock()
        .ok()
        .and_then(|mut map| map.remove(&layout_e));
    if next.is_none() && state.held.is_none() {
        return;
    }
    let Some(ns_view) = primary_content_view_ptr(window_e) else {
        return;
    };
    let content: &NSView = unsafe { &*ns_view.cast::<NSView>() };
    content.setWantsLayer(true);
    let Some(host_layer) = content.layer() else {
        return;
    };
    let clear_color = NSColor::clearColor();
    host_layer.setOpaque(false);
    host_layer.setBackgroundColor(Some(&clear_color.CGColor()));
    let bounds = content.bounds();

    if state.layer.is_none() {
        let layer: Retained<objc2_quartz_core::CALayer> = CALayer::new();
        layer.setOpaque(false);
        layer.setBackgroundColor(Some(&clear_color.CGColor()));
        layer.setZPosition(100.0);
        host_layer.addSublayer(&layer);
        state.layer = Some(layer);
    }
    let Some(layer) = state.layer.clone() else {
        return;
    };
    layer.setOpaque(false);
    layer.setBackgroundColor(Some(&clear_color.CGColor()));
    layer.setFrame(bounds);
    layer.setContentsScale(
        windows
            .get(window_e)
            .map(|w| w.resolution.scale_factor() as f64)
            .unwrap_or(2.0),
    );

    if let Some(frame) = next {
        let io_surface = frame.io_surface as *mut AnyObject;
        if !io_surface.is_null() {
            unsafe { layer.setContents(Some(&*io_surface)) };
            state.held = Some(frame);
        }
    }
    layer.setHidden(false);
    state.shown = true;
}

/// A2: show the command bar's OSR IOSurface in a full-window native overlay composited **above** the
/// page (so the page stays visible through the surface's transparent backdrop). The surface is
/// produced by the OSR modal and routed here via `NativeOverlayFrames`.
fn sync_command_bar_overlay(
    mut state: NonSendMut<CommandBarOverlay>,
    modal_open_q: Query<
        (&Node, Has<bevy_cef::prelude::CefKeyboardTarget>),
        With<vmux_layout::window::Modal>,
    >,
    modal_e_q: Query<Entity, With<vmux_layout::window::Modal>>,
    window_q: Query<Entity, With<bevy::window::PrimaryWindow>>,
    windows: Query<&bevy::window::Window>,
    overlay_frames: Res<bevy_cef::prelude::NativeOverlayFrames>,
) {
    use objc2::{MainThreadMarker, MainThreadOnly, runtime::AnyObject};
    use objc2_app_kit::NSView;

    let open = vmux_layout::command_bar::handler::is_command_bar_open(&modal_open_q);
    if !open {
        if state.shown {
            if let Some(view) = &state.view {
                view.setHidden(true);
            }
            state.shown = false;
            state.held = None;
        }
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let (Ok(window_e), Ok(modal_e)) = (window_q.single(), modal_e_q.single()) else {
        return;
    };
    // Pull the latest OSR frame for the modal. A *windowed* command bar produces none, so leave the
    // overlay dormant rather than covering the native command bar with an empty input-stealing layer.
    let next = overlay_frames
        .0
        .lock()
        .ok()
        .and_then(|mut map| map.remove(&modal_e));
    if next.is_none() && state.held.is_none() {
        return;
    }
    let Some(ns_view) = primary_content_view_ptr(window_e) else {
        return;
    };
    let content: &NSView = unsafe { &*ns_view.cast::<NSView>() };
    let bounds = content.bounds();

    if state.view.is_none() {
        let view = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        view.setWantsLayer(true);
        state.view = Some(view);
    }
    let Some(view) = state.view.clone() else {
        return;
    };
    view.setFrame(bounds);

    if let Some(frame) = next {
        if let Some(layer) = view.layer() {
            let io_surface = frame.io_surface as *mut AnyObject;
            if !io_surface.is_null() {
                let scale = windows
                    .get(window_e)
                    .map(|w| w.resolution.scale_factor() as f64)
                    .unwrap_or(2.0);
                layer.setOpaque(false);
                layer.setContentsScale(scale);
                unsafe { layer.setContents(Some(&*io_surface)) };
            }
        }
        state.held = Some(frame);
    }

    if !state.shown {
        view.setHidden(false);
        state.shown = true;
    }
    // Raise above the native pages (re-add reorders to front; pages re-raise each frame).
    content.addSubview(&view);
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_layout::scene::InteractionMode;

    #[test]
    fn glass_backdrop_is_hidden_in_player_mode() {
        assert!(!glass_backdrop_visible(InteractionMode::Player));
        assert!(glass_backdrop_visible(InteractionMode::User));
    }

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
            .and_then(|tail| {
                tail.split("#[derive(Default)]\nstruct LayoutOverlay")
                    .next()
            })
            .unwrap_or_default();

        assert!(sync.contains("backdrop_window.setFrame_display(parent_window.frame(), false)"));
    }

    #[test]
    fn desktop_enables_nspanel_binding_for_glass_backdrop() {
        let manifest = include_str!("../Cargo.toml");

        assert!(manifest.contains("\"NSPanel\""));
    }

    #[test]
    fn layout_overlay_uses_layer_for_hit_test_passthrough() {
        let source = include_str!("glass.rs");
        let overlay = source
            .split("fn sync_layout_overlay")
            .nth(1)
            .and_then(|tail| tail.split("fn sync_command_bar_overlay").next())
            .unwrap_or_default();

        assert!(overlay.contains("Retained<objc2_quartz_core::CALayer>"));
        assert!(overlay.contains("CALayer::new()"));
        assert!(overlay.contains("addSublayer"));
        assert!(overlay.contains("layer.setContents"));
        assert!(!overlay.contains("NSView::initWithFrame"));
    }

    #[test]
    fn layout_overlay_keeps_host_and_overlay_layers_transparent() {
        let source = include_str!("glass.rs");
        let overlay = source
            .split("fn sync_layout_overlay")
            .nth(1)
            .and_then(|tail| tail.split("fn sync_command_bar_overlay").next())
            .unwrap_or_default();

        assert!(overlay.contains("host_layer.setOpaque(false)"));
        assert!(overlay.contains("host_layer.setBackgroundColor(Some(&clear_color.CGColor()))"));
        assert!(overlay.contains("layer.setBackgroundColor(Some(&clear_color.CGColor()))"));
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
}
