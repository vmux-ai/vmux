use bevy::prelude::*;
use vmux_api::BinEvent;
#[cfg(test)]
use vmux_api::protocol::AgentRequest;
use vmux_api::protocol::{
    AgentBookmark, AgentBookmarkList, AgentBookmarkNode, AgentBookmarks, AgentImage,
    AgentRecordStart, AgentRecordStop, AgentRecording, AgentRequestId, AgentScreenshot,
    AgentVaultStatus, AgentWorkingDirectory, ClientMessage, ProcessId,
};
use vmux_command::WriteCommandRequests;
use vmux_core::service::ServiceRequest;
use vmux_core::{Bookmark, BookmarkOrder, Collapsed, Folder, PageMetadata, Pin, Uuid};
use vmux_layout::tab::Tab;
use vmux_tool::{ToolQueryHandled, ToolQueryRequest, ToolQueryRouteSet};

use vmux_browser::AgentBrowserResolve;

use crate::event::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
    ScreenshotImage, ScreenshotRequest, ScreenshotResponse,
};

pub(crate) struct AgentQueryPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AgentQuerySet;

#[derive(Message)]
struct WorkingDirectoryRequest {
    request_id: AgentRequestId,
    anchor: ProcessId,
}

#[derive(Message)]
struct VaultStatusRequest {
    request_id: AgentRequestId,
}

#[derive(Message)]
struct BookmarkListRequest {
    request_id: AgentRequestId,
}

#[derive(bevy::ecs::system::SystemParam)]
struct AgentQueryWriters<'w> {
    working_directory: MessageWriter<'w, WorkingDirectoryRequest>,
    vault: MessageWriter<'w, VaultStatusRequest>,
    bookmarks: MessageWriter<'w, BookmarkListRequest>,
    screenshot: MessageWriter<'w, ScreenshotRequest>,
    record_start: MessageWriter<'w, RecordStartRequest>,
    record_stop: MessageWriter<'w, RecordStopRequest>,
}

fn route_agent_queries(
    mut requests: MessageReader<ToolQueryRequest>,
    mut handled: MessageWriter<ToolQueryHandled>,
    mut writers: AgentQueryWriters,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let recognized = matches!(
            request.query.id.as_str(),
            AgentScreenshot::ID
                | AgentRecordStart::ID
                | AgentRecordStop::ID
                | AgentBookmarkList::ID
                | AgentWorkingDirectory::ID
                | AgentVaultStatus::ID
        );
        if !recognized {
            continue;
        }
        handled.write(ToolQueryHandled(request.request_id));
        let result = match request.query.id.as_str() {
            AgentScreenshot::ID => serde_json::from_slice::<AgentScreenshot>(&request.query.body)
                .map_err(|error| error.to_string())
                .map(|query| {
                    writers.screenshot.write(ScreenshotRequest {
                        request_id: request.request_id.0,
                        pane: query.pane,
                    });
                }),
            AgentRecordStart::ID => serde_json::from_slice::<AgentRecordStart>(&request.query.body)
                .map_err(|error| error.to_string())
                .map(|query| {
                    writers.record_start.write(RecordStartRequest {
                        request_id: request.request_id.0,
                        gif: query.gif,
                        max_secs: query.max_secs,
                        pane: query.pane,
                    });
                }),
            AgentRecordStop::ID => serde_json::from_slice::<AgentRecordStop>(&request.query.body)
                .map_err(|error| error.to_string())
                .map(|query| {
                    writers.record_stop.write(RecordStopRequest {
                        request_id: request.request_id.0,
                        dir: query.dir,
                        name: query.name,
                    });
                }),
            AgentBookmarkList::ID => {
                serde_json::from_slice::<AgentBookmarkList>(&request.query.body)
                    .map_err(|error| error.to_string())
                    .map(|_| {
                        writers.bookmarks.write(BookmarkListRequest {
                            request_id: request.request_id,
                        });
                    })
            }
            AgentWorkingDirectory::ID => {
                serde_json::from_slice::<AgentWorkingDirectory>(&request.query.body)
                    .map_err(|error| error.to_string())
                    .map(|query| {
                        writers.working_directory.write(WorkingDirectoryRequest {
                            request_id: request.request_id,
                            anchor: query.anchor,
                        });
                    })
            }
            AgentVaultStatus::ID => serde_json::from_slice::<AgentVaultStatus>(&request.query.body)
                .map_err(|error| error.to_string())
                .map(|_| {
                    writers.vault.write(VaultStatusRequest {
                        request_id: request.request_id,
                    });
                }),
            _ => unreachable!(),
        };
        if let Err(message) = result {
            service_requests.write(ServiceRequest(ClientMessage::AgentQueryError {
                request_id: request.request_id,
                message,
            }));
        }
    }
}

impl Plugin for AgentQueryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ToolQueryRequest>()
            .add_message::<WorkingDirectoryRequest>()
            .add_message::<VaultStatusRequest>()
            .add_message::<BookmarkListRequest>()
            .add_systems(
                Update,
                route_agent_queries
                    .in_set(ToolQueryRouteSet)
                    .after(super::AgentContinuationSet),
            )
            .add_systems(
                Update,
                (
                    answer_working_directory_queries,
                    answer_vault_queries,
                    answer_bookmark_queries,
                )
                    .in_set(AgentQuerySet)
                    .in_set(WriteCommandRequests)
                    .after(ToolQueryRouteSet),
            )
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

fn answer_working_directory_queries(
    mut reader: MessageReader<WorkingDirectoryRequest>,
    browse: AgentBrowserResolve,
    tabs: Query<&Tab>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let result = if browse.agent_pane(request.anchor).is_none() {
            Err("agent pane not found".to_string())
        } else if let Some(path) = browse.working_directory(request.anchor, &tabs) {
            Ok(path.to_string_lossy().into_owned())
        } else {
            match super::run_terminal::AgentCwd::projects() {
                Ok(path) => Ok(path.to_string_lossy().into_owned()),
                Err(message) => Err(message),
            }
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentWorkingDirectoryResult {
            request_id: request.request_id,
            result,
        }));
    }
}

fn answer_vault_queries(
    mut reader: MessageReader<VaultStatusRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        service_requests.write(ServiceRequest(ClientMessage::AgentVaultStatusResult {
            request_id: request.request_id,
            result: Ok(vmux_core::profile::vault::VaultStatus::current().snapshot()),
        }));
    }
}

fn answer_bookmark_queries(
    mut reader: MessageReader<BookmarkListRequest>,
    pins: Query<(&Uuid, &PageMetadata, &BookmarkOrder), With<Pin>>,
    folders: Query<
        (
            &Uuid,
            &Name,
            Option<&Children>,
            Has<Collapsed>,
            &BookmarkOrder,
        ),
        With<Folder>,
    >,
    top_level: Query<(&Uuid, &PageMetadata, &BookmarkOrder), (With<Bookmark>, Without<ChildOf>)>,
    bookmarks: Query<(&Uuid, &PageMetadata, &BookmarkOrder), With<Bookmark>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let mut pin_rows: Vec<(u32, AgentBookmark)> = pins
            .iter()
            .map(|(uuid, metadata, order)| {
                (
                    order.0,
                    AgentBookmark::new(
                        uuid.0.clone(),
                        metadata.url.clone(),
                        metadata.title.clone(),
                        metadata.icon.favicon_url(),
                    ),
                )
            })
            .collect();
        pin_rows.sort_by_key(|(order, _)| *order);
        let pins = pin_rows.into_iter().map(|(_, bookmark)| bookmark).collect();
        let mut roots: Vec<(u32, AgentBookmarkNode)> = Vec::new();
        for (uuid, name, children, collapsed, order) in &folders {
            let mut child_rows: Vec<(u32, AgentBookmark)> = Vec::new();
            if let Some(children) = children {
                for child in children.iter() {
                    if let Ok((child_uuid, metadata, child_order)) = bookmarks.get(child) {
                        child_rows.push((
                            child_order.0,
                            AgentBookmark::new(
                                child_uuid.0.clone(),
                                metadata.url.clone(),
                                metadata.title.clone(),
                                metadata.icon.favicon_url(),
                            ),
                        ));
                    }
                }
            }
            child_rows.sort_by_key(|(order, _)| *order);
            let children = child_rows
                .into_iter()
                .map(|(_, bookmark)| bookmark)
                .collect();
            roots.push((
                order.0,
                AgentBookmarkNode::Folder {
                    uuid: uuid.0.clone(),
                    name: name.to_string(),
                    collapsed,
                    children,
                },
            ));
        }
        for (uuid, metadata, order) in &top_level {
            roots.push((
                order.0,
                AgentBookmarkNode::Entry {
                    bookmark: AgentBookmark::new(
                        uuid.0.clone(),
                        metadata.url.clone(),
                        metadata.title.clone(),
                        metadata.icon.favicon_url(),
                    ),
                },
            ));
        }
        roots.sort_by_key(|(order, _)| *order);
        let roots = roots.into_iter().map(|(_, node)| node).collect();
        service_requests.write(ServiceRequest(ClientMessage::AgentBookmarksResult {
            request_id: request.request_id,
            result: Ok(AgentBookmarks { pins, roots }),
        }));
    }
}

fn screenshot_result(result: &Result<ScreenshotImage, String>) -> Result<AgentImage, String> {
    match result {
        Ok(img) => Ok(AgentImage {
            path: img.path.clone(),
            png: img.png.clone(),
            width: img.width,
            height: img.height,
        }),
        Err(message) => Err(message.clone()),
    }
}

fn forward_screenshot_responses(
    mut reader: MessageReader<ScreenshotResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in reader.read() {
        service_requests.write(ServiceRequest(ClientMessage::AgentScreenshotResult {
            request_id: AgentRequestId(response.request_id),
            result: screenshot_result(&response.result),
        }));
    }
}

fn forward_record_start_responses(
    mut reader: MessageReader<RecordStartResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in reader.read() {
        service_requests.write(ServiceRequest(ClientMessage::AgentRecordStartResult {
            request_id: AgentRequestId(response.request_id),
            result: response.result.clone(),
        }));
    }
}

fn recording_result(result: &Result<RecordingInfo, String>) -> Result<AgentRecording, String> {
    match result {
        Ok(info) => Ok(AgentRecording {
            mp4_path: info.mp4_path.clone(),
            gif_path: info.gif_path.clone(),
            duration_ms: info.duration_ms,
            bytes: info.bytes,
            auto_stopped: info.auto_stopped,
        }),
        Err(message) => Err(message.clone()),
    }
}

fn forward_record_stop_responses(
    mut reader: MessageReader<RecordStopResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in reader.read() {
        service_requests.write(ServiceRequest(ClientMessage::AgentRecordStopResult {
            request_id: AgentRequestId(response.request_id),
            result: recording_result(&response.result),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query_routing_app() -> App {
        let mut app = App::new();
        app.add_message::<ToolQueryRequest>()
            .add_message::<ToolQueryHandled>()
            .add_message::<ServiceRequest>()
            .add_message::<WorkingDirectoryRequest>()
            .add_message::<VaultStatusRequest>()
            .add_message::<BookmarkListRequest>()
            .add_message::<ScreenshotRequest>()
            .add_message::<RecordStartRequest>()
            .add_message::<RecordStopRequest>()
            .add_systems(Update, route_agent_queries);
        app
    }

    #[test]
    fn agent_queries_are_routed_into_typed_requests() {
        let mut app = query_routing_app();
        let screenshot_request_id = AgentRequestId([1; 16]);
        let vault_request_id = AgentRequestId([2; 16]);
        let mut screenshots = app
            .world()
            .resource::<Messages<ScreenshotRequest>>()
            .get_cursor();
        let mut vault = app
            .world()
            .resource::<Messages<VaultStatusRequest>>()
            .get_cursor();

        app.world_mut().write_message(ToolQueryRequest {
            request_id: screenshot_request_id,
            query: AgentRequest::encode(&AgentScreenshot {
                pane: Some("pane:7".to_string()),
            })
            .unwrap(),
        });
        app.world_mut().write_message(ToolQueryRequest {
            request_id: vault_request_id,
            query: AgentRequest::encode(&AgentVaultStatus).unwrap(),
        });
        app.update();

        let screenshot = screenshots
            .read(app.world().resource::<Messages<ScreenshotRequest>>())
            .next()
            .expect("expected screenshot request");
        assert_eq!(screenshot.request_id, screenshot_request_id.0);
        assert_eq!(screenshot.pane.as_deref(), Some("pane:7"));
        let vault = vault
            .read(app.world().resource::<Messages<VaultStatusRequest>>())
            .next()
            .expect("expected vault request");
        assert_eq!(vault.request_id, vault_request_id);
    }

    #[test]
    fn screenshot_response_maps_ok_and_err() {
        let ok = screenshot_result(&Ok(ScreenshotImage {
            path: "/tmp/a.png".into(),
            png: vec![9, 8, 7],
            width: 10,
            height: 20,
        }))
        .unwrap();
        assert_eq!(ok.path, "/tmp/a.png");
        assert_eq!(ok.png, vec![9, 8, 7]);
        assert_eq!(ok.width, 10);
        assert_eq!(ok.height, 20);

        assert_eq!(
            screenshot_result(&Err("nope".to_string())),
            Err("nope".to_string())
        );
    }

    #[test]
    fn record_stop_response_maps_ok_and_err() {
        let ok = recording_result(&Ok(RecordingInfo {
            mp4_path: "/tmp/x.mp4".into(),
            gif_path: None,
            duration_ms: 1000,
            bytes: 42,
            auto_stopped: false,
        }))
        .unwrap();
        assert_eq!(ok.mp4_path, "/tmp/x.mp4");
        assert_eq!(
            recording_result(&Err("boom".to_string())),
            Err("boom".to_string())
        );
    }
}
