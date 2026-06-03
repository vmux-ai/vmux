use bevy::prelude::*;

pub(crate) struct GlassPlugin;

impl Plugin for GlassPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<GlassState>()
            .init_non_send::<CommandBarOverlay>()
            .add_systems(Update, install_window_glass)
            // Run after PostUpdate's `sync_windowed_frames` (which raises each page to front every
            // frame) so the overlay stays on top of the pages.
            .add_systems(Last, sync_command_bar_overlay);
    }
}

fn glass_enabled() -> bool {
    std::env::var_os("VMUX_GLASS").is_some()
}

#[derive(Default)]
struct GlassState {
    installed: bool,
    _glass: Option<objc2::rc::Retained<objc2_app_kit::NSGlassEffectView>>,
}

fn install_window_glass(
    mut state: NonSendMut<GlassState>,
    window: Query<Entity, With<bevy::window::PrimaryWindow>>,
) {
    use bevy::winit::WINIT_WINDOWS;
    use objc2::{MainThreadMarker, rc::Retained, runtime::AnyClass};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSGlassEffectView, NSGlassEffectViewStyle, NSView,
        NSWindowOrderingMode,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    if state.installed {
        return;
    }
    if !glass_enabled() {
        state.installed = true;
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
    // Insert glass as a sibling *behind* the winit content view (its NSWindow frame view), so the
    // transparent Bevy/OSR surface composites over it. A content-view subview would render in front
    // of the OSR layer and hide the chrome.
    let Some(parent) = (unsafe { content.superview() }) else {
        return;
    };
    if AnyClass::get(c"NSGlassEffectView").is_none() {
        warn!("VMUX_GLASS: NSGlassEffectView unavailable (needs macOS 26+)");
        state.installed = true;
        return;
    }
    let glass: Retained<NSGlassEffectView> = NSGlassEffectView::new(mtm);
    glass.setStyle(NSGlassEffectViewStyle::Regular);
    let glass_view: &NSView = &glass;
    glass_view.setFrame(parent.bounds());
    glass_view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    parent.addSubview_positioned_relativeTo(glass_view, NSWindowOrderingMode::Below, Some(content));
    state._glass = Some(glass);
    state.installed = true;
    info!("VMUX_GLASS: NSGlassEffectView installed as window backdrop (behind content view)");
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

    if !glass_enabled() {
        return;
    }
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

    let next = overlay_frames
        .0
        .lock()
        .ok()
        .and_then(|mut map| map.remove(&modal_e));
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
