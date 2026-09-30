pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");

pub mod event;
pub mod url;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::{
    Axe, HardwareButtonRequest, SimulatorButtonPressRequest, SimulatorClipboardRequest,
    SimulatorControlResponse, SimulatorDevice, SimulatorFocusSet, SimulatorInputSet,
    SimulatorKeyPressRequest, SimulatorPlugin, SimulatorScreenshot, SimulatorScreenshotRequest,
    SimulatorScreenshotResponse, SimulatorSoftwareKeyboardRequest, SimulatorSwipeRequest,
    SimulatorTapRequest, SimulatorToolPlugin, SimulatorTypeTextRequest,
};
