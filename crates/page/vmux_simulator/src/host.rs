//! Everything the mirror needs a real machine for: the `axe` children and the loopback stream.

mod device;
mod input;
mod stream;

use crate::event::{SIMULATOR_READY_EVENT, SimulatorGesture, SimulatorReady};
use crate::url::PAGE_HOST;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use input::DeviceGesture;
use stream::StreamServer;
use vmux_core::PageMetadata;
use vmux_core::page::PageReady;

pub use device::{Axe, SimulatorDevice};

/// Wires the simulator page: serves the booted device's stream and replays gestures onto it.
pub struct SimulatorPlugin;

impl Plugin for SimulatorPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        vmux_core::register_host_spawn(app, PAGE_HOST);
        app.add_systems(Startup, Self::attach_device)
            .add_systems(Update, Self::announce)
            .add_plugins(BinEventEmitterPlugin::<(SimulatorGesture,)>::for_hosts(&[
                PAGE_HOST,
            ]))
            .add_observer(Self::on_gesture);
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: PAGE_HOST,
    title: "Simulator",
    keywords: &["simulator", "ios", "iphone", "device"],
    icon: Some(vmux_core::BuiltinIcon::Layers),
    command_bar: true,
};

/// The device's point size, measured once; gestures arrive as fractions and need it to land.
#[derive(Resource)]
struct DevicePoints(f32, f32);

impl SimulatorPlugin {
    /// URL prefix identifying a browser showing this page, used in place of a marker component.
    const URL_PREFIX: &'static str = "vmux://simulator/";

    fn attach_device(mut commands: Commands) {
        if Axe::version().is_none() {
            warn!(
                "`{}` not found on PATH — the simulator page needs it; \
                 install with `brew install cameroncooke/axe/axe`",
                Axe::BIN
            );
            return;
        }
        let Some(device) = SimulatorDevice::booted() else {
            info!("no booted simulator; the simulator page will be empty");
            return;
        };
        if let Some((w, h)) = device.point_size() {
            commands.insert_resource(DevicePoints(w, h));
        }
        match StreamServer::start(device.clone()) {
            Ok(server) => {
                info!(
                    "mirroring {} on loopback port {}",
                    device.name,
                    server.port()
                );
                commands.insert_resource(server);
            }
            Err(error) => error!("could not serve the simulator stream: {error}"),
        }
        commands.insert_resource(device);
    }

    /// Tells every ready simulator view where to point its `<img>`.
    fn announce(
        browsers: NonSend<Browsers>,
        views: Query<(Entity, &PageMetadata), With<PageReady>>,
        server: Option<Res<StreamServer>>,
        device: Option<Res<SimulatorDevice>>,
        mut last: Local<SimulatorReady>,
        mut commands: Commands,
    ) {
        let payload = match (server.as_deref(), device.as_deref()) {
            (Some(server), Some(device)) => SimulatorReady {
                port: server.port(),
                version: device
                    .version
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                device_name: device.name.clone(),
            },
            _ => SimulatorReady::default(),
        };
        let mut announced = false;
        for (entity, meta) in views.iter() {
            if !meta.url.starts_with(Self::URL_PREFIX) {
                continue;
            }
            if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                continue;
            }
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                SIMULATOR_READY_EVENT,
                &payload,
            ));
            announced = true;
        }
        if announced {
            *last = payload;
        }
    }

    fn on_gesture(
        trigger: On<BinReceive<SimulatorGesture>>,
        device: Option<Res<SimulatorDevice>>,
        points: Option<Res<DevicePoints>>,
    ) {
        let (Some(device), Some(points)) = (device.as_deref(), points.as_deref()) else {
            return;
        };
        let Some(gesture) =
            DeviceGesture::resolve(&trigger.event().payload, device, (points.0, points.1))
        else {
            return;
        };
        gesture.dispatch();
    }
}

impl SimulatorDevice {
    /// The URL a bare `vmux://simulator/ios` should settle on.
    pub fn canonical_url(&self) -> Option<String> {
        self.version.as_ref().map(crate::url::SimulatorRoute::url)
    }
}
