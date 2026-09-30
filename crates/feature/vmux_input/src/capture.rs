use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{
    AgentImage, AgentQueryResult, AgentRequest, AgentRequestId, ClientMessage,
};
use vmux_core::host::manifest::FeatureManifestPlugin;
use vmux_core::service::ServiceRequest;
use vmux_tool::{
    AddedTool, ToolAppExt, ToolDispatchSet, ToolQuery, ToolQueryHandled, ToolQueryRequest,
    ToolQueryRouteSet,
};

#[vmux_api::agent(Eq)]
struct AgentScreenshot {
    pane: Option<String>,
}

#[vmux_api::agent(Eq)]
struct AgentRecordStart {
    gif: bool,
    max_secs: u32,
    pane: Option<String>,
}

#[vmux_api::agent(Eq)]
struct AgentRecordStop {
    dir: Option<String>,
    name: Option<String>,
}

pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeatureManifestPlugin::new(crate::FEATURE_MANIFEST))
            .add_message::<ToolQueryRequest>()
            .add_message::<ToolQueryHandled>()
            .add_message::<ServiceRequest>()
            .register_tool::<ScreenshotArgs>()
            .register_tool::<RecordStartArgs>()
            .register_tool::<RecordStopArgs>()
            .add_message::<ScreenshotRequest>()
            .add_message::<ScreenshotResponse>()
            .add_message::<RecordStartRequest>()
            .add_message::<RecordStartResponse>()
            .add_message::<RecordStopRequest>()
            .add_message::<RecordStopResponse>()
            .add_systems(
                Update,
                (screenshot, record_start, record_stop).in_set(ToolDispatchSet),
            )
            .add_systems(Update, route_capture_queries.in_set(ToolQueryRouteSet))
            .add_systems(
                Update,
                (
                    forward_screenshot_responses,
                    forward_record_start_responses,
                    forward_record_stop_responses,
                ),
            );
    }
}

#[derive(Message, Clone)]
pub struct ScreenshotRequest {
    pub request_id: [u8; 16],
    pub pane: Option<String>,
}

#[derive(Clone)]
pub struct ScreenshotImage {
    pub path: String,
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Message, Clone)]
pub struct ScreenshotResponse {
    pub request_id: [u8; 16],
    pub result: Result<ScreenshotImage, String>,
}

#[derive(Message, Clone)]
pub struct RecordStartRequest {
    pub request_id: [u8; 16],
    pub gif: bool,
    pub max_secs: u32,
    pub pane: Option<String>,
}

#[derive(Message, Clone)]
pub struct RecordStartResponse {
    pub request_id: [u8; 16],
    pub result: Result<u32, String>,
}

#[derive(Message, Clone)]
pub struct RecordStopRequest {
    pub request_id: [u8; 16],
    pub dir: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone)]
pub struct RecordingInfo {
    pub mp4_path: String,
    pub gif_path: Option<String>,
    pub duration_ms: u64,
    pub bytes: u64,
    pub auto_stopped: bool,
}

#[derive(Message, Clone)]
pub struct RecordStopResponse {
    pub request_id: [u8; 16],
    pub result: Result<RecordingInfo, String>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenshotArgs {
    pane: Option<String>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordStartArgs {
    #[serde(default)]
    gif: bool,
    max_secs: Option<u32>,
    pane: Option<String>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordStopArgs {
    dir: Option<String>,
    name: Option<String>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct CaptureQueryWriters<'w> {
    screenshot: MessageWriter<'w, ScreenshotRequest>,
    record_start: MessageWriter<'w, RecordStartRequest>,
    record_stop: MessageWriter<'w, RecordStopRequest>,
}

fn screenshot(
    mut commands: Commands,
    requests: Query<(Entity, &ScreenshotArgs), AddedTool<ScreenshotArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(AgentRequest::encode(&AgentScreenshot {
                pane: OptionalText::trim(args.pane.clone()),
            })));
    }
}

fn record_start(
    mut commands: Commands,
    requests: Query<(Entity, &RecordStartArgs), AddedTool<RecordStartArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(AgentRequest::encode(&AgentRecordStart {
                gif: args.gif,
                max_secs: args.max_secs.unwrap_or(600),
                pane: OptionalText::trim(args.pane.clone()),
            })));
    }
}

fn record_stop(
    mut commands: Commands,
    requests: Query<(Entity, &RecordStopArgs), AddedTool<RecordStopArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(AgentRequest::encode(&AgentRecordStop {
                dir: OptionalText::trim(args.dir.clone()),
                name: OptionalText::trim(args.name.clone()),
            })));
    }
}

fn route_capture_queries(
    mut queries: MessageReader<ToolQueryRequest>,
    mut handled: MessageWriter<ToolQueryHandled>,
    mut writers: CaptureQueryWriters,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in queries.read() {
        let recognized = matches!(
            request.query.id.as_str(),
            AgentScreenshot::ID | AgentRecordStart::ID | AgentRecordStop::ID
        );
        if !recognized {
            continue;
        }
        handled.write(ToolQueryHandled(request.request_id));
        let result = match request.query.id.as_str() {
            AgentScreenshot::ID => request
                .query
                .decode::<AgentScreenshot>()
                .and_then(|query| query.ok_or_else(|| "screenshot query mismatch".to_string()))
                .map(|query| {
                    writers.screenshot.write(ScreenshotRequest {
                        request_id: request.request_id.0,
                        pane: query.pane,
                    });
                }),
            AgentRecordStart::ID => request
                .query
                .decode::<AgentRecordStart>()
                .and_then(|query| query.ok_or_else(|| "record start query mismatch".to_string()))
                .map(|query| {
                    writers.record_start.write(RecordStartRequest {
                        request_id: request.request_id.0,
                        gif: query.gif,
                        max_secs: query.max_secs,
                        pane: query.pane,
                    });
                }),
            AgentRecordStop::ID => request
                .query
                .decode::<AgentRecordStop>()
                .and_then(|query| query.ok_or_else(|| "record stop query mismatch".to_string()))
                .map(|query| {
                    writers.record_stop.write(RecordStopRequest {
                        request_id: request.request_id.0,
                        dir: query.dir,
                        name: query.name,
                    });
                }),
            _ => unreachable!(),
        };
        if let Err(message) = result {
            service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
                AgentQueryResult::text(request.request_id, Err(message)),
            )));
        }
    }
}

fn forward_screenshot_responses(
    mut responses: MessageReader<ScreenshotResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in responses.read() {
        let result = response.result.as_ref().map_or_else(
            |message| Err(message.clone()),
            |image| {
                Ok((
                    format!("saved {} ({}×{})", image.path, image.width, image.height),
                    AgentImage {
                        path: image.path.clone(),
                        png: image.png.clone(),
                        width: image.width,
                        height: image.height,
                    },
                ))
            },
        );
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::image(AgentRequestId(response.request_id), result),
        )));
    }
}

fn forward_record_start_responses(
    mut responses: MessageReader<RecordStartResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in responses.read() {
        let result = response
            .result
            .clone()
            .map(|max_secs| format!("recording started, max {max_secs}s"));
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::text(AgentRequestId(response.request_id), result),
        )));
    }
}

fn forward_record_stop_responses(
    mut responses: MessageReader<RecordStopResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in responses.read() {
        let result = response.result.as_ref().map_or_else(
            |message| Err(message.clone()),
            |recording| {
                let secs = recording.duration_ms as f64 / 1000.0;
                let mut text = format!(
                    "recorded {secs:.1}s → {} ({} bytes)",
                    recording.mp4_path, recording.bytes
                );
                if let Some(gif) = &recording.gif_path {
                    text.push_str(&format!(" + {gif}"));
                }
                if recording.auto_stopped {
                    text.push_str(" (auto-stopped)");
                }
                Ok(text)
            },
        );
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::text(AgentRequestId(response.request_id), result),
        )));
    }
}

struct OptionalText;

impl OptionalText {
    fn trim(value: Option<String>) -> Option<String> {
        value.and_then(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::JsonArguments;
    use vmux_tool::{ToolCatalog, ToolCatalogRequest, ToolDispatchError, ToolInvocation};

    struct CaptureFixture;

    impl CaptureFixture {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(CapturePlugin);
            app.update();
            app
        }

        fn definitions() -> Vec<String> {
            let mut app = Self::app();
            let request = app.world_mut().spawn(ToolCatalogRequest).id();
            app.update();
            app.world_mut()
                .entity_mut(request)
                .take::<ToolCatalog>()
                .unwrap()
                .0
                .into_iter()
                .map(|definition| definition.name)
                .collect()
        }

        fn dispatch(name: &str, arguments: serde_json::Value) -> Result<AgentRequest, String> {
            let mut app = Self::app();
            let request = app
                .world_mut()
                .spawn((
                    Name::new(name.to_string()),
                    JsonArguments(arguments),
                    ToolInvocation,
                ))
                .id();
            app.update();
            if let Some(query) = app.world_mut().entity_mut(request).take::<ToolQuery>() {
                return query.0;
            }
            let error = app
                .world_mut()
                .entity_mut(request)
                .take::<ToolDispatchError>()
                .unwrap();
            Err(error.message().to_string())
        }
    }

    #[test]
    fn manifest_registers_capture_tools() {
        assert_eq!(
            CaptureFixture::definitions(),
            ["screenshot", "record_start", "record_stop"]
        );
    }

    #[test]
    fn screenshot_dispatches_optional_pane() {
        let request = CaptureFixture::dispatch("screenshot", serde_json::json!({})).unwrap();
        assert_eq!(
            request.decode::<AgentScreenshot>().unwrap(),
            Some(AgentScreenshot { pane: None })
        );
        let request =
            CaptureFixture::dispatch("screenshot", serde_json::json!({"pane": "pane:7"})).unwrap();
        assert_eq!(
            request.decode::<AgentScreenshot>().unwrap(),
            Some(AgentScreenshot {
                pane: Some("pane:7".to_string()),
            })
        );
    }

    #[test]
    fn recording_dispatches_defaults_and_output() {
        let request = CaptureFixture::dispatch("record_start", serde_json::json!({})).unwrap();
        assert_eq!(
            request.decode::<AgentRecordStart>().unwrap(),
            Some(AgentRecordStart {
                gif: false,
                max_secs: 600,
                pane: None,
            })
        );
        let request = CaptureFixture::dispatch(
            "record_start",
            serde_json::json!({"gif": true, "max_secs": 30, "pane": "pane:3"}),
        )
        .unwrap();
        assert_eq!(
            request.decode::<AgentRecordStart>().unwrap(),
            Some(AgentRecordStart {
                gif: true,
                max_secs: 30,
                pane: Some("pane:3".to_string()),
            })
        );
        let request = CaptureFixture::dispatch(
            "record_stop",
            serde_json::json!({"dir": "/tmp/out", "name": "feature-x"}),
        )
        .unwrap();
        assert_eq!(
            request.decode::<AgentRecordStop>().unwrap(),
            Some(AgentRecordStop {
                dir: Some("/tmp/out".to_string()),
                name: Some("feature-x".to_string()),
            })
        );
    }
}
