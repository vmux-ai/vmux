//! System tray integration for macOS.
//!
//! Tray icon for the service when the GUI is closed.
//!
//! When the GUI window closes but service-managed processes are still alive,
//! a tray icon is shown to indicate the service is running.
//!
//! NOTE: Full tray-icon integration is deferred to a follow-up.
//! tray-icon requires a running event loop and careful coordination
//! with Bevy/winit's event loop on macOS. For now, the service runs
//! headlessly and the user can relaunch the GUI to reconnect.
//!
//! Planned features:
//! - Tray icon appears when GUI closes with active processes
//! - Menu: "Show Vmux", "Processes (N active)", "Quit Service"
//! - Click to relaunch GUI and reattach

#[allow(dead_code)] // Will be registered in VmuxPlugin once implemented
pub(crate) struct TrayPlugin;

impl bevy::prelude::Plugin for TrayPlugin {
    fn build(&self, _app: &mut bevy::prelude::App) {
        // Placeholder — tray integration will be added in a follow-up PR
    }
}
