use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use vmux_browser::HostFocusIntent;

/// When the active page is a terminal (OSR) or there is none, the winit host window must own
/// macOS first-responder so Bevy delivers keys (terminal → PTY, layout shortcuts). A previously
/// focused windowed web page leaves its native `NSView` as first-responder, which blacks out the
/// host keyboard — so on the transition into [`HostFocusIntent::WinitHost`] we explicitly hand
/// first-responder back to the winit content view. Only acted on transition to avoid re-stealing
/// focus from in-page text fields every frame.
pub(crate) fn apply_winit_host_focus(
    _non_send: NonSendMarker,
    intent: Res<HostFocusIntent>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut last: Local<Option<HostFocusIntent>>,
) {
    let current = *intent;
    if current != HostFocusIntent::WinitHost {
        *last = Some(current);
        return;
    }
    if *last == Some(current) {
        return;
    }
    // Retry on later frames until the window exists, so an early transition isn't dropped.
    let Ok(window_entity) = primary.single() else {
        return;
    };
    info!(target: "vmux::host_focus", "winit reclaim first responder (window={window_entity:?})");
    reclaim_first_responder(window_entity);
    *last = Some(current);
}

fn reclaim_first_responder(window_entity: Entity) {
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
        return;
    };
    let view: &NSView = unsafe { &*view_ptr.cast::<NSView>() };
    let Some(window) = view.window() else {
        return;
    };
    let responder: &NSResponder = view;
    window.makeFirstResponder(Some(responder));
}
