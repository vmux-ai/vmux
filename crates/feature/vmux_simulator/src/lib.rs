#[cfg(host)]
pub use host::{
    Axe, HardwareButtonRequest, SimulatorButtonPressRequest, SimulatorClipboardRequest,
    SimulatorControlResponse, SimulatorDevice, SimulatorFocusSet, SimulatorInputSet,
    SimulatorKeyPressRequest, SimulatorScreenshot, SimulatorScreenshotRequest,
    SimulatorScreenshotResponse, SimulatorSoftwareKeyboardRequest, SimulatorSwipeRequest,
    SimulatorTapRequest, SimulatorToolPlugin, SimulatorTypeTextRequest,
};

pub(crate) struct Feature;

impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod event;
pub mod url;

#[vmux_page::page]
pub struct SimulatorPlugin;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
