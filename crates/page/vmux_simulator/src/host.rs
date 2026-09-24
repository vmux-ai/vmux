#[cfg(target_os = "macos")]
mod core_simulator;
mod device;
mod hid;
mod input;
mod stream;

use crate::event::{HardwareButton, SimulatorClipboardOperation, SimulatorReady};
use crate::url::{PAGE_HOST, PAGE_URL, SimulatorRoute};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use hid::HidBroker;
use stream::StreamServer;
use vmux_api::protocol::SimulatorAction;
use vmux_core::PageMetadata;
use vmux_core::host::page::{NativelyHosted, PageReady};
use vmux_core::host::{UiState, UiStatePlugin};

pub use device::{Axe, SimulatorDevice};

pub struct SimulatorPlugin;

impl Plugin for SimulatorPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::SimulatorPage::plugin());
        app.world_mut().spawn((
            PAGE_MANIFEST,
            NativelyHosted::subtree(PAGE_URL, PAGE_MANIFEST.title),
        ));
        app.init_resource::<ActiveSimulatorView>()
            .add_plugins(UiStatePlugin::<SimulatorReady>::default())
            .configure_sets(Update, (SimulatorFocusSet, SimulatorInputSet).chain())
            .add_message::<HardwareButtonRequest>()
            .add_message::<SimulatorClipboardRequest>()
            .add_message::<SimulatorSoftwareKeyboardRequest>()
            .add_message::<SimulatorFocusRequest>()
            .add_message::<SimulatorControlRequest>()
            .add_message::<SimulatorControlResponse>()
            .add_message::<SimulatorScreenshotRequest>()
            .add_message::<SimulatorScreenshotResponse>()
            .add_systems(
                Update,
                (
                    Self::start_device_attachments,
                    Self::finish_device_attachments,
                    Self::announce,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                Self::sync_stream_activity
                    .after(Self::finish_device_attachments)
                    .after(SimulatorFocusSet),
            )
            .add_systems(
                Update,
                Self::handle_screenshot_requests.in_set(SimulatorInputSet),
            )
            .add_plugins(input::SimulatorInputPlugin);

        #[cfg(target_os = "macos")]
        app.add_plugins(core_simulator::CoreSimulatorPlugin);
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HardwareButtonRequest {
    pub view: Option<Entity>,
    pub button: HardwareButton,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimulatorClipboardRequest {
    pub view: Option<Entity>,
    pub operation: SimulatorClipboardOperation,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimulatorSoftwareKeyboardRequest {
    pub view: Option<Entity>,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimulatorFocusRequest(pub Option<Entity>);

#[derive(Message, Clone)]
pub struct SimulatorControlRequest {
    pub request_id: [u8; 16],
    pub action: SimulatorAction,
}

#[derive(Message, Clone)]
pub struct SimulatorControlResponse {
    pub request_id: [u8; 16],
    pub result: Result<String, String>,
}

#[derive(Message, Clone)]
pub struct SimulatorScreenshotRequest {
    pub request_id: [u8; 16],
}

#[derive(Clone)]
pub struct SimulatorScreenshot {
    pub path: String,
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Message, Clone)]
pub struct SimulatorScreenshotResponse {
    pub request_id: [u8; 16],
    pub result: Result<SimulatorScreenshot, String>,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SimulatorFocusSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SimulatorInputSet;

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: PAGE_HOST,
    title: "Simulator",
    title_message_id: Some("simulator-title"),
    replaces_command: None,
    keywords: &["simulator", "ios", "iphone", "device"],
    icon: Some(vmux_core::BuiltinIcon::Smartphone),
    command_bar: true,
};

#[derive(Component)]
struct DevicePoints(f32, f32);

#[derive(Component)]
struct DevicePixels(u32, u32);

type SimulatorViews<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PageMetadata,
        Option<&'static ChildOf>,
        Option<&'static UiState<SimulatorReady>>,
    ),
    With<PageReady>,
>;

type SimulatorAttachmentCandidates<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PageMetadata,
        Option<&'static AttachedRoute>,
        Option<&'static SimulatorDevice>,
        Has<DeviceAttachment>,
        Has<AttachmentFailed>,
    ),
    With<PageReady>,
>;

#[derive(Resource, Default)]
struct ActiveSimulatorView(Option<Entity>);

#[derive(Component, Clone, PartialEq, Eq)]
struct AttachedRoute(SimulatorRoute);

#[derive(Component)]
struct DeviceAttachment(Task<Result<AttachedDevice, String>>);

#[derive(Component)]
struct AttachmentFailed;

struct AttachedDevice {
    axe: Axe,
    hid: HidBroker,
    keyboard: input::SimulatorKeyboard,
    device: SimulatorDevice,
    points: Option<(f32, f32)>,
    pixels: Option<(u32, u32)>,
    server: StreamServer,
}

impl SimulatorPlugin {
    const URL_PREFIX: &'static str = "vmux://simulator/";

    #[cfg(target_os = "macos")]
    pub fn exit_helper_if_requested() {
        core_simulator::exit_if_requested();
    }

    fn start_device_attachments(
        views: SimulatorAttachmentCandidates,
        wake: Option<Res<EventLoopProxyWrapper>>,
        mut commands: Commands,
    ) {
        for (entity, metadata, attached_route, device, starting, failed) in &views {
            let Ok(route) = SimulatorRoute::try_from(metadata.url.as_str()) else {
                continue;
            };
            let matches = attached_route.is_some_and(|current| current.0 == route)
                || device.is_some_and(|device| device.matches_route(&route));
            if matches && (starting || failed || device.is_some()) {
                continue;
            }
            commands.entity(entity).remove::<(
                DeviceAttachment,
                AttachmentFailed,
                Axe,
                HidBroker,
                input::SimulatorKeyboard,
                SimulatorDevice,
                DevicePoints,
                DevicePixels,
                StreamServer,
                UiState<SimulatorReady>,
                input::DeviceTouchSession,
            )>();
            let wake = wake.as_ref().map(|wrapper| (**wrapper).clone());
            let attached_route = route.clone();
            let task = IoTaskPool::get().spawn(async move {
                let result = AttachedDevice::start(&route);
                if let Some(proxy) = wake {
                    let _ = proxy.send_event(WinitUserEvent::WakeUp);
                }
                result
            });
            commands
                .entity(entity)
                .insert((AttachedRoute(attached_route), DeviceAttachment(task)));
        }
    }

    fn finish_device_attachments(
        mut attachments: Query<(Entity, &mut DeviceAttachment)>,
        wake: Option<Res<EventLoopProxyWrapper>>,
        #[cfg(target_os = "macos")] mut keyboard: MessageWriter<
            core_simulator::HardwareKeyboardSetRequest,
        >,
        mut commands: Commands,
    ) {
        for (entity, mut attachment) in &mut attachments {
            let Some(result) = future::block_on(future::poll_once(&mut attachment.0)) else {
                continue;
            };
            commands.entity(entity).remove::<DeviceAttachment>();
            let attached = match result {
                Ok(attached) => attached,
                Err(error) => {
                    error!("could not attach an iOS Simulator: {error}");
                    commands.entity(entity).insert(AttachmentFailed);
                    continue;
                }
            };
            info!(
                "mirroring {} on loopback port {}",
                attached.device.name,
                attached.server.port()
            );
            #[cfg(target_os = "macos")]
            keyboard.write(core_simulator::HardwareKeyboardSetRequest {
                udid: attached.device.udid.clone(),
                enabled: false,
            });
            let mut entity_commands = commands.entity(entity);
            if let Some((width, height)) = attached.points {
                entity_commands.insert(DevicePoints(width, height));
            }
            if let Some((width, height)) = attached.pixels {
                entity_commands.insert(DevicePixels(width, height));
            }
            entity_commands.insert((
                attached.server,
                attached.device,
                attached.hid,
                attached.keyboard,
                attached.axe,
                input::DeviceTouchSession::default(),
            ));
            if let Some(wake) = wake.as_deref() {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
        }
    }

    fn announce(
        views: SimulatorViews,
        attachments: Query<(&StreamServer, &SimulatorDevice)>,
        mut commands: Commands,
    ) {
        for (entity, meta, child_of, announced) in views.iter() {
            if !meta.url.starts_with(Self::URL_PREFIX) {
                continue;
            }
            let payload = match attachments.get(entity) {
                Ok((server, device)) => SimulatorReady {
                    port: server.port(),
                    capability: server.capability().to_string(),
                    version: device
                        .version
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    device_name: device.name.clone(),
                    frame_width: server.frame_width(),
                    frame_height: server.frame_height(),
                    frame_stride: server.frame_stride(),
                },
                Err(_) => SimulatorReady::default(),
            };
            if let Ok((_, device)) = attachments.get(entity)
                && let Some(canonical_url) = device.canonical_url()
                && meta.url != canonical_url
            {
                let mut canonical = meta.clone();
                canonical.url = canonical_url;
                commands.entity(entity).insert(canonical.clone());
                if let Some(child_of) = child_of {
                    commands.entity(child_of.parent()).insert(canonical);
                }
            }
            if announced.is_some_and(|announced| announced.current() == Some(&payload)) {
                continue;
            }
            UiState::<SimulatorReady>::write(&mut commands, entity, &payload);
        }
    }

    fn sync_stream_activity(
        active: Res<ActiveSimulatorView>,
        streams: Query<(Entity, &StreamServer)>,
    ) {
        for (entity, stream) in &streams {
            stream.set_active(active.0 == Some(entity));
        }
    }

    fn handle_screenshot_requests(
        mut requests: MessageReader<SimulatorScreenshotRequest>,
        mut responses: MessageWriter<SimulatorScreenshotResponse>,
        active: Res<ActiveSimulatorView>,
        attachments: Query<(Entity, &SimulatorDevice, &Axe)>,
    ) {
        for request in requests.read() {
            let result = match active.select(attachments.iter().map(|(entity, _, _)| entity)) {
                Some(entity) => {
                    let (_, device, axe) = attachments.get(entity).unwrap();
                    SimulatorScreenshot::capture(request.request_id, device, axe)
                }
                None => Err("no iOS Simulator is attached".to_string()),
            };
            responses.write(SimulatorScreenshotResponse {
                request_id: request.request_id,
                result,
            });
        }
    }
}

impl ActiveSimulatorView {
    fn select(&self, candidates: impl Iterator<Item = Entity>) -> Option<Entity> {
        let mut first = None;
        for entity in candidates {
            if self.0 == Some(entity) {
                return Some(entity);
            }
            if first.is_none() {
                first = Some(entity);
            }
        }
        first
    }
}

impl SimulatorScreenshot {
    fn capture(request_id: [u8; 16], device: &SimulatorDevice, axe: &Axe) -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "vmux-simulator-{}-{:02x}{:02x}{:02x}{:02x}.png",
            std::process::id(),
            request_id[0],
            request_id[1],
            request_id[2],
            request_id[3]
        ));
        let png = device.screenshot(axe, &path)?;
        let (width, height) = SimulatorDevice::png_size(&png)
            .ok_or_else(|| "simulator screenshot is not a valid PNG".to_string())?;
        Ok(Self {
            path: path.to_string_lossy().into_owned(),
            png,
            width,
            height,
        })
    }
}

impl AttachedDevice {
    fn start(route: &SimulatorRoute) -> Result<Self, String> {
        let axe = Axe::locate().ok_or_else(|| {
            format!(
                "`{}` not found; install it with `brew install cameroncooke/axe/axe`",
                Axe::BIN
            )
        })?;
        info!(
            "axe {} at {}",
            axe.version().unwrap_or_default(),
            axe.path().display()
        );
        let device = SimulatorDevice::booted_or_boot(route.version(), route.device_name())?;
        let points = device.point_size(&axe);
        let pixels = device.pixel_size(&axe);
        let hid = HidBroker::start(&axe, &device)
            .map_err(|error| format!("could not start simulator input: {error}"))?;
        let keyboard = input::SimulatorKeyboard::start(&axe, &device)
            .map_err(|error| format!("could not start simulator keyboard: {error}"))?;
        let server = StreamServer::start(&axe, device.clone(), pixels)
            .map_err(|error| format!("could not serve the simulator stream: {error}"))?;
        Ok(Self {
            axe,
            hid,
            keyboard,
            device,
            points,
            pixels,
            server,
        })
    }
}

impl SimulatorDevice {
    pub fn canonical_url(&self) -> Option<String> {
        self.version
            .as_ref()
            .map(|version| crate::url::SimulatorRoute::url(version, Some(&self.name)))
    }
}
