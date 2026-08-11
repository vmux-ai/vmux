use bevy::prelude::*;

/// The non-macOS half of [`crate::host_focus::HostFocusPlugin`].
///
/// Handing first-responder back to the host window is an AppKit concern; every other platform
/// leaves the keyboard where the windowing system put it, so there is nothing to schedule.
pub(crate) struct HostFocusPlatformPlugin;

impl Plugin for HostFocusPlatformPlugin {
    fn build(&self, _app: &mut App) {}
}
