use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use vmux_browser::HostFocusIntent;

/// In [`HostFocusIntent::WinitHost`] (a terminal/OSR page is active) the winit content view must own
/// macOS first-responder so Bevy delivers keys to the focused terminal. A windowed CEF page in the
/// stack — e.g. a terminal webview that autofocuses on load, or the command bar modal — can steal it
/// a frame or more later, so re-assert every frame. [`reclaim_first_responder`] is a no-op once the
/// winit view already holds first-responder, so this only acts when it was stolen.
///
/// On the frame first-responder is reclaimed *from* a CEF subview, release all tracked keys: while a
/// CEF subview held first-responder it consumed key-up events winit never saw, so modifiers (Cmd in
/// particular) can be stuck "pressed" in [`ButtonInput`] and would swallow subsequent typing.
pub(crate) fn apply_winit_host_focus(
    _non_send: NonSendMarker,
    intent: Res<HostFocusIntent>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
) {
    if *intent != HostFocusIntent::WinitHost {
        return;
    }
    let Ok(window_entity) = primary.single() else {
        return;
    };
    if matches!(
        reclaim_first_responder(window_entity),
        ReclaimOutcome::Reclaimed
    ) {
        keys.release_all();
    }
}

enum ReclaimOutcome {
    AlreadyWinit,
    Reclaimed,
    Failed,
    NoView,
}

/// Make the winit content view the window's first responder.
fn reclaim_first_responder(window_entity: Entity) -> ReclaimOutcome {
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
        return ReclaimOutcome::NoView;
    };
    let view: &NSView = unsafe { &*view_ptr.cast::<NSView>() };
    let Some(window) = view.window() else {
        return ReclaimOutcome::NoView;
    };
    let responder: &NSResponder = view;
    let already_winit = window.firstResponder().is_some_and(|current| {
        core::ptr::eq(
            (&*current) as *const NSResponder,
            responder as *const NSResponder,
        )
    });
    if !already_winit && !window.makeFirstResponder(Some(responder)) {
        return ReclaimOutcome::Failed;
    }
    if !window.isKeyWindow() {
        return ReclaimOutcome::Failed;
    }
    if already_winit {
        ReclaimOutcome::AlreadyWinit
    } else {
        ReclaimOutcome::Reclaimed
    }
}
