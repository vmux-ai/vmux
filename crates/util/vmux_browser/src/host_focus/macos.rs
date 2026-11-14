use crate::host_focus::HostFocusIntent;
use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;

use bevy::winit::WINIT_WINDOWS;
use objc2_app_kit::{NSResponder, NSView};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(crate) struct HostFocusPlatformPlugin;

impl bevy::prelude::Plugin for HostFocusPlatformPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_systems(Last, (initialize, apply_winit).chain());
    }
}

fn initialize(
    windows: Query<Entity, (With<bevy::prelude::Window>, Without<KeyWindowHandoff>)>,
    mut commands: bevy::prelude::Commands,
) {
    for window in &windows {
        commands.entity(window).insert(KeyWindowHandoff::default());
    }
}

fn apply_winit(
    _non_send: NonSendMarker,
    intent: Single<&HostFocusIntent>,
    focused_window: vmux_layout::window::FocusedWindow,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut windows: Query<(Entity, &mut KeyWindowHandoff), With<bevy::prelude::Window>>,
) {
    for (window, mut handoff) in &mut windows {
        if **intent != HostFocusIntent::WinitHost || Some(window) != focused_window.entity() {
            handoff.0 = false;
            continue;
        }
        if handoff.releases(ReclaimOutcome::of(window)) {
            keys.release_all();
        }
    }
}

enum ReclaimOutcome {
    AlreadyWinit,
    Reclaimed,
    PendingKeyWindow,
    Failed,
    NoView,
}

#[derive(Component, Default)]
struct KeyWindowHandoff(bool);

impl KeyWindowHandoff {
    fn releases(&mut self, outcome: ReclaimOutcome) -> bool {
        match outcome {
            ReclaimOutcome::Reclaimed => {
                self.0 = false;
                true
            }
            ReclaimOutcome::PendingKeyWindow => {
                self.0 = true;
                false
            }
            ReclaimOutcome::AlreadyWinit if self.0 => {
                self.0 = false;
                true
            }
            ReclaimOutcome::AlreadyWinit | ReclaimOutcome::Failed | ReclaimOutcome::NoView => false,
        }
    }
}

impl ReclaimOutcome {
    fn of(window_entity: Entity) -> Self {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_key_window_releases_keys_when_winit_becomes_responder() {
        let mut handoff = KeyWindowHandoff::default();
        assert!(!handoff.releases(ReclaimOutcome::PendingKeyWindow));
        assert!(handoff.releases(ReclaimOutcome::AlreadyWinit));
        assert!(!handoff.releases(ReclaimOutcome::AlreadyWinit));
    }
}
