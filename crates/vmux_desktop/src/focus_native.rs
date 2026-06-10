use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use vmux_browser::HostFocusIntent;

/// When the active page is a terminal (OSR) or there is none, the winit host window must own
/// macOS first-responder so Bevy delivers keys (terminal → PTY, layout shortcuts). A previously
/// focused windowed web page or the command bar leaves its native `NSView` as first-responder,
/// which blacks out the host keyboard — so while [`HostFocusIntent::WinitHost`] is active we hand
/// first-responder back to the winit content view. Re-asserted every frame (not just on the
/// transition) because a closing command bar / page can resign a frame *after* the intent flips,
/// which a one-shot reclaim would miss. `makeFirstResponder` is skipped when winit already holds it.
pub(crate) fn apply_winit_host_focus(
    _non_send: NonSendMarker,
    intent: Res<HostFocusIntent>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut announced: Local<bool>,
) {
    if *intent != HostFocusIntent::WinitHost {
        *announced = false;
        return;
    }
    let Ok(window_entity) = primary.single() else {
        return;
    };
    if reclaim_first_responder(window_entity) && !*announced {
        info!(target: "vmux::host_focus", "winit reclaim first responder (window={window_entity:?})");
        *announced = true;
    }
}

/// Make the winit content view the window's first responder. Returns `true` if it actually changed
/// (i.e. winit did not already hold it).
fn reclaim_first_responder(window_entity: Entity) -> bool {
    use bevy::winit::WINIT_WINDOWS;
    use objc2_app_kit::{NSResponder, NSView};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let view_ptr = WINIT_WINDOWS.with_borrow(|windows| {
        let id = windows.entity_to_winit.get(&window_entity)?;
        let wrapper = windows.windows.get(id)?;
        let handle = wrapper.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::AppKit(h) => Some(h.ns_view.as_ptr()),
            _ => None,
        }
    });
    let Some(view_ptr) = view_ptr else {
        return false;
    };
    let view: &NSView = unsafe { &*view_ptr.cast::<NSView>() };
    let Some(window) = view.window() else {
        return false;
    };
    let responder: &NSResponder = view;
    if let Some(current) = window.firstResponder()
        && core::ptr::eq(
            (&*current) as *const NSResponder,
            responder as *const NSResponder,
        )
    {
        return false;
    }
    window.makeFirstResponder(Some(responder))
}
