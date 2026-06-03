use bevy::prelude::*;

pub(crate) struct GlassPlugin;

impl Plugin for GlassPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<GlassState>()
            .add_systems(Update, install_window_glass);
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
