use crate::SimulatorPlugin;
use crate::event::{HardwareButton, SimulatorClipboardOperation, SimulatorUiState};
use crate::url::SimulatorRoute;
use attachment::{AttachmentFailed, AttachmentSet, Attempt, Phase};
use bevy::prelude::*;
use bevy::prelude::{Commands, Query};
pub use device::{Axe, SimulatorDevice};
use stream::StreamServer;
pub use tool::SimulatorToolPlugin;
use vmux_api::protocol::{AgentImage, AgentQueryResult, AgentRequestId, ClientMessage};
use vmux_ecs::page::{HostedPage, PageReady};
use vmux_ecs::service::ServiceRequest;
use vmux_ecs::{EntityTarget, PageMetadata};
use vmux_ecs::{UiState, UiStatePlugin, UiStateWrite};
use vmux_input::{NativeKeyClaimSet, NativeKeyInputSet};
use vmux_layout::stack::{ComputeFocusSet, FocusedStack};
use vmux_tool::{ToolQueryAppExt, ToolQueryMessage, ToolQueryRouteSet};

mod attachment;
#[cfg(target_os = "macos")]
mod core_simulator;
mod device;
mod device_driver;
mod hid_driver;
mod input;
mod stream;
mod tool;

#[vmux_api::contract(Copy, Eq)]
pub enum SimulatorButton {
    Home,
    Lock,
    Siri,
}

#[vmux_api::agent(Copy, Eq)]
struct AgentSimulatorTap {
    x: u32,
    y: u32,
}

#[vmux_api::agent(Copy, Eq)]
struct AgentSimulatorSwipe {
    start_x: u32,
    start_y: u32,
    end_x: u32,
    end_y: u32,
    duration_ms: u32,
}

#[vmux_api::agent(Eq)]
struct AgentSimulatorTypeText {
    text: String,
}

#[vmux_api::agent(Copy, Eq)]
struct AgentSimulatorKeyPress {
    keycode: u8,
}

#[vmux_api::agent(Copy, Eq)]
struct AgentSimulatorButtonPress {
    button: SimulatorButton,
}

#[vmux_api::agent(Copy, Eq)]
struct AgentSimulatorScreenshot;

impl Plugin for SimulatorPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::SimulatorPage::plugin());
        app.add_plugins(
            Self::manifest()
                .plugin()
                .hosted(HostedPage::subtree(Self::url(), Self::manifest().title)),
        )
        .add_plugins(UiStatePlugin::<SimulatorUiState>::default())
        .add_plugins(SimulatorToolPlugin)
        .add_plugins(attachment::AttachmentPlugin)
        .configure_sets(
            Update,
            (NativeKeyClaimSet, NativeKeyInputSet, SimulatorInputSet).chain(),
        )
        .configure_sets(Update, SimulatorInputSet.after(SimulatorFocusSet))
        .add_message::<HardwareButtonRequest>()
        .add_message::<SimulatorClipboardRequest>()
        .add_message::<SimulatorSoftwareKeyboardRequest>()
        .add_message::<SimulatorTapRequest>()
        .add_message::<SimulatorSwipeRequest>()
        .add_message::<SimulatorTypeTextRequest>()
        .add_message::<SimulatorKeyPressRequest>()
        .add_message::<SimulatorButtonPressRequest>()
        .add_message::<SimulatorControlResponse>()
        .add_message::<SimulatorScreenshotRequest>()
        .add_message::<SimulatorScreenshotResponse>()
        .add_tool_query::<AgentSimulatorScreenshot>()
        .add_tool_query::<AgentSimulatorTap>()
        .add_tool_query::<AgentSimulatorSwipe>()
        .add_tool_query::<AgentSimulatorTypeText>()
        .add_tool_query::<AgentSimulatorKeyPress>()
        .add_tool_query::<AgentSimulatorButtonPress>()
        .add_systems(
            PreUpdate,
            (sync_active_view, ApplyDeferred)
                .chain()
                .in_set(SimulatorFocusSet),
        )
        .add_message::<ServiceRequest>()
        .add_systems(Update, announce.after(AttachmentSet))
        .add_systems(
            Update,
            (sync_active_view, ApplyDeferred)
                .chain()
                .after(ComputeFocusSet),
        )
        .add_systems(
            Update,
            sync_stream_activity
                .after(AttachmentSet)
                .after(sync_active_view),
        )
        .add_systems(Update, handle_screenshot_requests.after(sync_active_view))
        .add_systems(
            Update,
            (
                route_screenshot_queries,
                route_tap_queries,
                route_swipe_queries,
                route_text_queries,
                route_key_queries,
                route_button_queries,
            )
                .after(ToolQueryRouteSet),
        )
        .add_systems(
            Update,
            (forward_control_responses, forward_screenshot_responses),
        )
        .add_plugins(input::SimulatorInputPlugin);

        #[cfg(target_os = "macos")]
        app.add_plugins(core_simulator::CoreSimulatorPlugin);
    }
}

fn route_screenshot_queries(
    mut queries: MessageReader<ToolQueryMessage<AgentSimulatorScreenshot>>,
    mut screenshots: MessageWriter<SimulatorScreenshotRequest>,
) {
    for request in queries.read() {
        screenshots.write(SimulatorScreenshotRequest {
            request_id: request.request_id.0,
        });
    }
}

fn route_tap_queries(
    mut queries: MessageReader<ToolQueryMessage<AgentSimulatorTap>>,
    mut taps: MessageWriter<SimulatorTapRequest>,
) {
    for request in queries.read() {
        taps.write(SimulatorTapRequest {
            request_id: request.request_id.0,
            x: request.payload.x,
            y: request.payload.y,
        });
    }
}

fn route_swipe_queries(
    mut queries: MessageReader<ToolQueryMessage<AgentSimulatorSwipe>>,
    mut swipes: MessageWriter<SimulatorSwipeRequest>,
) {
    for request in queries.read() {
        swipes.write(SimulatorSwipeRequest {
            request_id: request.request_id.0,
            start_x: request.payload.start_x,
            start_y: request.payload.start_y,
            end_x: request.payload.end_x,
            end_y: request.payload.end_y,
            duration_ms: request.payload.duration_ms,
        });
    }
}

fn route_text_queries(
    mut queries: MessageReader<ToolQueryMessage<AgentSimulatorTypeText>>,
    mut text: MessageWriter<SimulatorTypeTextRequest>,
) {
    for request in queries.read() {
        text.write(SimulatorTypeTextRequest {
            request_id: request.request_id.0,
            text: request.payload.text.clone(),
        });
    }
}

fn route_key_queries(
    mut queries: MessageReader<ToolQueryMessage<AgentSimulatorKeyPress>>,
    mut keys: MessageWriter<SimulatorKeyPressRequest>,
) {
    for request in queries.read() {
        keys.write(SimulatorKeyPressRequest {
            request_id: request.request_id.0,
            keycode: request.payload.keycode,
        });
    }
}

fn route_button_queries(
    mut queries: MessageReader<ToolQueryMessage<AgentSimulatorButtonPress>>,
    mut buttons: MessageWriter<SimulatorButtonPressRequest>,
) {
    for request in queries.read() {
        buttons.write(SimulatorButtonPressRequest {
            request_id: request.request_id.0,
            button: request.payload.button,
        });
    }
}

fn forward_control_responses(
    mut responses: MessageReader<SimulatorControlResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in responses.read() {
        let (content, is_error) = match &response.result {
            Ok(content) => (content.clone(), false),
            Err(message) => (message.clone(), true),
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult {
                request_id: AgentRequestId(response.request_id),
                content,
                is_error,
                image: None,
            },
        )));
    }
}

fn forward_screenshot_responses(
    mut responses: MessageReader<SimulatorScreenshotResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in responses.read() {
        let (content, is_error, image) = match &response.result {
            Ok(image) => (
                format!("saved {} ({}×{})", image.path, image.width, image.height),
                false,
                Some(AgentImage {
                    path: image.path.clone(),
                    png: image.png.clone(),
                    width: image.width,
                    height: image.height,
                }),
            ),
            Err(message) => (message.clone(), true, None),
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult {
                request_id: AgentRequestId(response.request_id),
                content,
                is_error,
                image,
            },
        )));
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

#[derive(Message, Clone, Copy)]
pub struct SimulatorTapRequest {
    pub request_id: [u8; 16],
    pub x: u32,
    pub y: u32,
}

#[derive(Message, Clone, Copy)]
pub struct SimulatorSwipeRequest {
    pub request_id: [u8; 16],
    pub start_x: u32,
    pub start_y: u32,
    pub end_x: u32,
    pub end_y: u32,
    pub duration_ms: u32,
}

#[derive(Message, Clone)]
pub struct SimulatorTypeTextRequest {
    pub request_id: [u8; 16],
    pub text: String,
}

#[derive(Message, Clone, Copy)]
pub struct SimulatorKeyPressRequest {
    pub request_id: [u8; 16],
    pub keycode: u8,
}

#[derive(Message, Clone, Copy)]
pub struct SimulatorButtonPressRequest {
    pub request_id: [u8; 16],
    pub button: SimulatorButton,
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

#[derive(Component)]
struct DevicePoints(f32, f32);

#[derive(Component)]
struct DevicePixels(u32, u32);

#[derive(Component)]
struct ActiveSimulatorView;

fn sync_active_view(
    focus: FocusedStack,
    children: Query<&Children>,
    pages: Query<&PageMetadata, With<PageReady>>,
    active: Query<Entity, With<ActiveSimulatorView>>,
    mut commands: Commands,
) {
    let target = focus
        .as_deref()
        .and_then(|focus| focus.stack)
        .and_then(|stack| children.get(stack).ok())
        .and_then(|children| {
            children.iter().find(|entity| {
                pages
                    .get(*entity)
                    .is_ok_and(|metadata| SimulatorRoute::try_from(metadata.url.as_str()).is_ok())
            })
        });
    let mut target_active = false;
    for entity in &active {
        if Some(entity) == target {
            target_active = true;
        } else {
            commands.entity(entity).remove::<ActiveSimulatorView>();
        }
    }
    if let Some(entity) = target.filter(|_| !target_active) {
        commands.entity(entity).insert(ActiveSimulatorView);
    }
}

impl SimulatorPlugin {
    #[cfg(target_os = "macos")]
    pub fn exit_helper_if_requested() {
        core_simulator::CoreSimulatorPlugin::exit_helper_if_requested();
    }
}

#[expect(clippy::type_complexity)]
fn announce(
    views: Query<
        (
            Entity,
            &PageMetadata,
            Option<&ChildOf>,
            Option<&UiState<SimulatorUiState>>,
            Option<&EntityTarget<Attempt>>,
            Option<&AttachmentFailed>,
        ),
        With<PageReady>,
    >,
    attachments: Query<(&StreamServer, &SimulatorDevice)>,
    phases: Query<(&Phase, &vmux_ecs::OperationState)>,
    mut commands: Commands,
) {
    for (entity, meta, child_of, announced, starting, failed) in views.iter() {
        if !meta.url.starts_with(SimulatorPlugin::url()) {
            continue;
        }
        let payload = match attachments.get(entity) {
            Ok((server, device)) => SimulatorUiState {
                port: server.port(),
                status: "ready".to_string(),
                error: String::new(),
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
            Err(_) => SimulatorUiState {
                status: if failed.is_some() {
                    "errored".to_string()
                } else if let Some((phase, _)) = starting
                    .and_then(|target| phases.get(target.entity()).ok())
                    .filter(|(_, state)| {
                        matches!(
                            state,
                            vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
                        )
                    })
                {
                    phase.status().to_string()
                } else {
                    "waiting".to_string()
                },
                error: failed.map(|failed| failed.0.clone()).unwrap_or_default(),
                ..Default::default()
            },
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
        commands.trigger(UiStateWrite::<SimulatorUiState>::from_event(
            entity, &payload,
        ));
    }
}

fn sync_stream_activity(streams: Query<(&StreamServer, Has<ActiveSimulatorView>)>) {
    for (stream, active) in &streams {
        stream.set_active(active);
    }
}

fn handle_screenshot_requests(
    mut requests: MessageReader<SimulatorScreenshotRequest>,
    mut responses: MessageWriter<SimulatorScreenshotResponse>,
    active: Query<Entity, With<ActiveSimulatorView>>,
    attachments: Query<(Entity, &SimulatorDevice, &Axe)>,
) {
    let active = active.iter().next();
    for request in requests.read() {
        let result = match ActiveSimulatorView::select(
            active,
            attachments.iter().map(|(entity, _, _)| entity),
        ) {
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

impl ActiveSimulatorView {
    fn select(active: Option<Entity>, candidates: impl Iterator<Item = Entity>) -> Option<Entity> {
        let mut first = None;
        for entity in candidates {
            if active == Some(entity) {
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

impl SimulatorDevice {
    pub fn canonical_url(&self) -> Option<String> {
        self.version
            .as_ref()
            .map(|version| SimulatorRoute::url(version, Some(&self.name)))
    }
}
