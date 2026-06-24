use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use vmux_browser::HostFocusIntent;

pub(crate) fn apply_winit_host_focus(
    _non_send: NonSendMarker,
    intent: Res<HostFocusIntent>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut pending_key_window: Local<bool>,
) {
    if *intent != HostFocusIntent::WinitHost {
        *pending_key_window = false;
        return;
    }
    let Ok(window_entity) = primary.single() else {
        return;
    };
    if should_release_keys(
        reclaim_first_responder(window_entity),
        &mut pending_key_window,
    ) {
        keys.release_all();
    }
}

enum ReclaimOutcome {
    AlreadyWinit,
    Reclaimed,
    PendingKeyWindow,
    Failed,
    NoView,
}

fn should_release_keys(outcome: ReclaimOutcome, pending_key_window: &mut bool) -> bool {
    match outcome {
        ReclaimOutcome::Reclaimed => {
            *pending_key_window = false;
            true
        }
        ReclaimOutcome::PendingKeyWindow => {
            *pending_key_window = true;
            false
        }
        ReclaimOutcome::AlreadyWinit if *pending_key_window => {
            *pending_key_window = false;
            true
        }
        ReclaimOutcome::AlreadyWinit | ReclaimOutcome::Failed | ReclaimOutcome::NoView => false,
    }
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
    let changed_responder = !already_winit;
    if changed_responder && !window.makeFirstResponder(Some(responder)) {
        return ReclaimOutcome::Failed;
    }
    if !window.isKeyWindow() {
        if changed_responder {
            return ReclaimOutcome::PendingKeyWindow;
        }
        return ReclaimOutcome::Failed;
    }
    if already_winit {
        ReclaimOutcome::AlreadyWinit
    } else {
        ReclaimOutcome::Reclaimed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_key_window_releases_keys_when_winit_becomes_responder() {
        let mut pending = false;

        assert!(!should_release_keys(
            ReclaimOutcome::PendingKeyWindow,
            &mut pending
        ));
        assert!(pending);
        assert!(should_release_keys(
            ReclaimOutcome::AlreadyWinit,
            &mut pending
        ));
        assert!(!pending);
    }
}
