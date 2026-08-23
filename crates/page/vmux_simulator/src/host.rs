//! Everything the mirror needs a real machine for: the `axe` child, frame decode, and gestures.

mod device;
mod geometry;
mod input;
mod mirror;
mod stream;

use crate::url::{IosVersion, PAGE_HOST, SimulatorRoute};
use bevy::prelude::*;

pub use device::{Axe, SimulatorDevice};

/// Wires the simulator page: resolves the booted runtime, streams it, and forwards gestures.
pub struct SimulatorPlugin;

impl Plugin for SimulatorPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        vmux_core::register_host_spawn(app, PAGE_HOST);
        app.add_plugins((
            stream::StreamPlugin,
            mirror::MirrorPlugin,
            input::InputPlugin,
        ));
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: PAGE_HOST,
    title: "Simulator",
    keywords: &["simulator", "ios", "iphone", "device"],
    icon: Some(vmux_core::BuiltinIcon::Layers),
    command_bar: true,
};

impl SimulatorDevice {
    /// The URL a bare `vmux://simulator/ios` should settle on.
    pub fn canonical_url(&self) -> Option<String> {
        self.version.as_ref().map(SimulatorRoute::url)
    }
}

impl IosVersion {
    /// The booted runtime, or `None` when nothing is running.
    pub fn booted() -> Option<Self> {
        SimulatorDevice::booted()?.version
    }
}
