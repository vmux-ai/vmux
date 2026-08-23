//! Everything the mirror needs a real machine for: the `axe` children and the loopback stream.

mod device;
mod input;
mod stream;

use crate::event::{SIMULATOR_READY_EVENT, SimulatorGesture, SimulatorReady};
use crate::url::PAGE_HOST;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use input::DeviceGesture;
use stream::StreamServer;
use vmux_core::page::PageReady;
use vmux_core::{
    CefPageAttachRequest, PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask,
};

pub use device::{Axe, SimulatorDevice};

/// Wires the simulator page: serves the booted device's stream and replays gestures onto it.
pub struct SimulatorPlugin;

impl Plugin for SimulatorPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        vmux_core::register_host_spawn(app, PAGE_HOST);
        app.init_resource::<Announced>()
            .add_systems(Startup, Self::attach_device)
            .add_systems(
                Update,
                Self::claim_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_systems(Update, Self::announce)
            .add_plugins(BinEventEmitterPlugin::<(SimulatorGesture,)>::for_hosts(&[
                PAGE_HOST,
            ]))
            .add_observer(Self::on_gesture)
            .add_observer(Self::forget_on_reload);
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

/// What each view has already been told, so the announcement is not re-sent every tick.
#[derive(Resource, Default)]
struct Announced(HashMap<Entity, SimulatorReady>);

/// A page-open task nobody has claimed or failed yet.
type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

impl SimulatorPlugin {
    /// URL prefix identifying a browser showing this page, used in place of a marker component.
    const URL_PREFIX: &'static str = "vmux://simulator/";

    /// Claims every simulator URL, pinned or not.
    ///
    /// A `PrewarmPage` would only match one exact string, which leaves
    /// `vmux://simulator/ios/27.0` unroutable from a bookmark or the command bar even though the
    /// page reaches that URL itself. Prewarming is also wrong here: a hidden warm copy connects
    /// to the stream and holds an `axe` child at full frame rate for a page nobody is looking at.
    fn claim_page_open(
        tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
        mut attach: MessageWriter<CefPageAttachRequest>,
        mut commands: Commands,
    ) {
        for (entity, task) in &tasks {
            if crate::url::SimulatorRoute::of_url(&task.url).is_none() {
                continue;
            }
            attach.write(CefPageAttachRequest {
                stack: task.stack,
                url: task.url.clone(),
                title: PAGE_MANIFEST.title.to_string(),
                bg_color: None,
            });
            commands.entity(entity).insert(PageOpenHandled);
        }
    }

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
        mut told: ResMut<Announced>,
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
        // Per view, not one global latch: a second view opening later must still be told, and a
        // reload resets the page's copy without changing the payload.
        told.0.retain(|entity, _| views.contains(*entity));
        for (entity, meta) in views.iter() {
            if !meta.url.starts_with(Self::URL_PREFIX) {
                continue;
            }
            if told.0.get(&entity) == Some(&payload) {
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
            told.0.insert(entity, payload.clone());
        }
    }

    /// A reload clears the page's copy, so the view must be told again.
    fn forget_on_reload(trigger: On<BinReceive<PageReady>>, mut told: ResMut<Announced>) {
        told.0.remove(&trigger.event().webview);
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
