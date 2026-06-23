use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use vmux_browser::HostFocusIntent;
use vmux_layout::stack::FocusedStack;

/// When the active page is a terminal (OSR) or there is none, the winit host window must own
/// macOS first-responder so Bevy delivers keys (terminal → PTY, layout shortcuts). A previously
/// focused windowed web page or the command bar leaves its native `NSView` as first-responder,
/// which blacks out the host keyboard — so while [`HostFocusIntent::WinitHost`] is active we hand
/// first-responder back to the winit content view.
///
/// The reclaim is retried each frame *until it sticks*, then stops: the active page/command bar can
/// resign a frame after the intent flips (so a one-shot reclaim would miss it), but re-asserting
/// every frame fights the page for first-responder and breaks input.
pub(crate) fn apply_winit_host_focus(
    _non_send: NonSendMarker,
    intent: Res<HostFocusIntent>,
    focus: Res<FocusedStack>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut reclaimed: Local<bool>,
    mut last_stack: Local<Option<Entity>>,
) {
    if focus.stack != *last_stack {
        *last_stack = focus.stack;
        *reclaimed = false;
    }
    if *intent != HostFocusIntent::WinitHost {
        *reclaimed = false;
        return;
    }
    if *reclaimed {
        return;
    }
    let Ok(window_entity) = primary.single() else {
        return;
    };
    match reclaim_first_responder(window_entity) {
        ReclaimOutcome::AlreadyWinit | ReclaimOutcome::Reclaimed => *reclaimed = true,
        // Window/view not ready or the current responder refused to resign — retry next frame.
        ReclaimOutcome::Failed | ReclaimOutcome::NoView => {}
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
    // The window boots hidden and is revealed + activated asynchronously. `makeFirstResponder`
    // on a not-yet-key window can report success without durably sticking, so a one-shot reclaim
    // marks itself done while the view never actually holds first-responder once the window
    // becomes key — keystrokes then go nowhere until a click. Wait for the window to be key so the
    // reclaim that sticks happens on a live window. This matters most on space restore, where the
    // active stack resolves while the window is still hidden.
    if !window.isKeyWindow() {
        return ReclaimOutcome::Failed;
    }
    let responder: &NSResponder = view;
    if let Some(current) = window.firstResponder()
        && core::ptr::eq(
            (&*current) as *const NSResponder,
            responder as *const NSResponder,
        )
    {
        return ReclaimOutcome::AlreadyWinit;
    }
    if window.makeFirstResponder(Some(responder)) {
        ReclaimOutcome::Reclaimed
    } else {
        ReclaimOutcome::Failed
    }
}
