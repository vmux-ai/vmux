use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use vmux_browser::HostFocusIntent;

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
