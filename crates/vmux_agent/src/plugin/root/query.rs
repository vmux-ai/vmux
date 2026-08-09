//! Questions an agent asks about the app, and the answers that come back out of band.
//!
//! A query whose answer needs another subsystem — a screenshot, a recording, a layout snapshot —
//! is forwarded as a request and matched to its response by request id, so nothing blocks a frame
//! waiting on it.

use bevy::prelude::*;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{
    AgentCommandResult, AgentQuery, AgentQueryResult, AgentRequestId, ClientMessage,
};
use vmux_setting::AppSettings;

use crate::events::{
    AgentQueryRequest, RecordStartRequest, RecordStartResponse, RecordStopRequest,
    RecordStopResponse, RecordingInfo, ScreenshotImage, ScreenshotRequest, ScreenshotResponse,
    snapshot_response_to_query_result,
};
use vmux_core::browser::{
    BrowserScrollRequest, BrowserSnapshotRequest, BrowserSnapshotResponse, NavAwaitingSnapshot,
};

use super::browser_pane::AgentBrowserResolve;

pub(crate) fn handle_agent_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    settings: Res<AppSettings>,
    spaces: Query<
        (
            &vmux_layout::space::SpaceId,
            &Name,
            Has<vmux_core::Active>,
            Option<&vmux_core::Order>,
        ),
        With<vmux_layout::space::Space>,
    >,
    bm_pins: Query<
        (
            &vmux_core::Uuid,
            &vmux_core::PageMetadata,
            &vmux_core::BookmarkOrder,
        ),
        With<vmux_core::Pin>,
    >,
    bm_folders: Query<
        (
            &vmux_core::Uuid,
            &Name,
            Option<&Children>,
            Has<vmux_core::Collapsed>,
            &vmux_core::BookmarkOrder,
        ),
        With<vmux_core::Folder>,
    >,
    bm_top: Query<
        (
            &vmux_core::Uuid,
            &vmux_core::PageMetadata,
            &vmux_core::BookmarkOrder,
        ),
        (With<vmux_core::Bookmark>, Without<ChildOf>),
    >,
    bm_children: Query<
        (
            &vmux_core::Uuid,
            &vmux_core::PageMetadata,
            &vmux_core::BookmarkOrder,
        ),
        With<vmux_core::Bookmark>,
    >,
    mut layout_snapshot_writer: MessageWriter<vmux_layout::apply::LayoutSnapshotRequest>,
    mut screenshot_writer: MessageWriter<ScreenshotRequest>,
    mut browser_snapshot_writer: MessageWriter<BrowserSnapshotRequest>,
    mut browser_scroll_writer: MessageWriter<BrowserScrollRequest>,
    mut record_start_writer: MessageWriter<RecordStartRequest>,
    mut record_stop_writer: MessageWriter<RecordStopRequest>,
    mut browse: AgentBrowserResolve,
) {
    let Some(service) = service else { return };

    for request in reader.read() {
        match request.query {
            AgentQuery::ReadLayout { anchor } => {
                layout_snapshot_writer.write(vmux_layout::apply::LayoutSnapshotRequest {
                    request_id: request.request_id.0,
                    anchor,
                });
            }
            AgentQuery::GetSettings => {
                let result =
                    AgentQueryResult::Settings(vmux_setting::serialize_settings_to_json(&settings));
                service.0.send(ClientMessage::AgentQueryResponse {
                    request_id: request.request_id,
                    result,
                });
            }
            AgentQuery::ListSpaces => {
                let mut rows: Vec<(u32, serde_json::Value)> = spaces
                    .iter()
                    .map(|(id, name, is_active, order)| {
                        (
                            order.map(|o| o.0).unwrap_or(u32::MAX),
                            serde_json::json!({
                                "id": id.0,
                                "name": name.to_string(),
                                "profile": vmux_space::model::bootstrap_profile_name(),
                                "is_active": is_active,
                            }),
                        )
                    })
                    .collect();
                rows.sort_by_key(|(order, _)| *order);
                let rows: Vec<serde_json::Value> = rows.into_iter().map(|(_, row)| row).collect();
                let json = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string());
                service.0.send(ClientMessage::AgentQueryResponse {
                    request_id: request.request_id,
                    result: AgentQueryResult::Spaces(json),
                });
            }
            AgentQuery::BookmarkList => {
                let row = |u: &vmux_core::Uuid, m: &vmux_core::PageMetadata| {
                    serde_json::json!({
                        "uuid": u.0,
                        "url": m.url,
                        "title": m.title,
                        "favicon_url": m.icon.favicon_url(),
                    })
                };
                let mut pin_rows: Vec<(u32, serde_json::Value)> =
                    bm_pins.iter().map(|(u, m, o)| (o.0, row(u, m))).collect();
                pin_rows.sort_by_key(|(order, _)| *order);
                let pins: Vec<serde_json::Value> = pin_rows.into_iter().map(|(_, v)| v).collect();
                let mut roots: Vec<(u32, serde_json::Value)> = Vec::new();
                for (uuid, name, children, collapsed, order) in bm_folders.iter() {
                    let mut kids: Vec<(u32, serde_json::Value)> = Vec::new();
                    if let Some(children) = children {
                        for child in children.iter() {
                            if let Ok((u, m, order)) = bm_children.get(child) {
                                kids.push((order.0, row(u, m)));
                            }
                        }
                    }
                    kids.sort_by_key(|(order, _)| *order);
                    let kids: Vec<serde_json::Value> =
                        kids.into_iter().map(|(_, row)| row).collect();
                    roots.push((
                        order.0,
                        serde_json::json!({
                            "kind": "folder",
                            "uuid": uuid.0,
                            "name": name.as_str(),
                            "collapsed": collapsed,
                            "children": kids,
                        }),
                    ));
                }
                for (uuid, meta, order) in bm_top.iter() {
                    let mut entry = row(uuid, meta);
                    entry["kind"] = serde_json::json!("entry");
                    roots.push((order.0, entry));
                }
                roots.sort_by_key(|(order, _)| *order);
                let roots: Vec<serde_json::Value> = roots.into_iter().map(|(_, v)| v).collect();
                let json =
                    serde_json::to_string(&serde_json::json!({"pins": pins, "roots": roots}))
                        .unwrap_or_else(|_| "{}".to_string());
                service.0.send(ClientMessage::AgentQueryResponse {
                    request_id: request.request_id,
                    result: AgentQueryResult::Spaces(json),
                });
            }
            AgentQuery::Screenshot { ref pane } => {
                screenshot_writer.write(ScreenshotRequest {
                    request_id: request.request_id.0,
                    pane: pane.clone(),
                });
            }
            AgentQuery::BrowserSnapshot {
                ref pane,
                ref anchor,
            } => {
                browser_snapshot_writer.write(BrowserSnapshotRequest {
                    request_id: request.request_id.0,
                    pane: browse.resolve_pane(pane, anchor),
                    webview: None,
                });
            }
            AgentQuery::BrowserScroll {
                ref pane,
                ref to,
                delta,
                ref anchor,
            } => {
                browser_scroll_writer.write(BrowserScrollRequest {
                    request_id: request.request_id.0,
                    pane: browse.resolve_pane(pane, anchor),
                    to: to.clone(),
                    delta,
                });
            }
            AgentQuery::RecordStart {
                gif,
                max_secs,
                ref pane,
            } => {
                record_start_writer.write(RecordStartRequest {
                    request_id: request.request_id.0,
                    gif,
                    max_secs,
                    pane: pane.clone(),
                });
            }
            AgentQuery::RecordStop { ref dir, ref name } => {
                record_stop_writer.write(RecordStopRequest {
                    request_id: request.request_id.0,
                    dir: dir.clone(),
                    name: name.clone(),
                });
            }
            // ReadTerminal/ReadTerminalFull/CommandExit/RunCompletion are
            // answered by the service directly; they never reach the GUI.
            AgentQuery::ReadTerminal { .. }
            | AgentQuery::ReadTerminalFull { .. }
            | AgentQuery::CommandExit { .. }
            | AgentQuery::RunCompletion { .. } => {}
        }
    }
}

pub(crate) fn forward_layout_apply_responses(
    mut reader: MessageReader<vmux_layout::apply::LayoutApplyResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        let result = match response.result.clone() {
            Ok(snapshot) => AgentCommandResult::Layout(snapshot),
            Err(message) => AgentCommandResult::Error(message),
        };
        service.0.send(ClientMessage::AgentCommandResponse {
            request_id: AgentRequestId(response.request_id),
            result,
        });
    }
}

pub(crate) fn forward_layout_snapshot_responses(
    mut reader: MessageReader<vmux_layout::apply::LayoutSnapshotResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: AgentQueryResult::Layout(response.snapshot.clone()),
        });
    }
}

pub(crate) fn screenshot_response_to_query_result(
    result: &Result<ScreenshotImage, String>,
) -> AgentQueryResult {
    match result {
        Ok(img) => AgentQueryResult::Image {
            path: img.path.clone(),
            png: img.png.clone(),
            width: img.width,
            height: img.height,
        },
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
}

pub(crate) fn forward_screenshot_responses(
    mut reader: MessageReader<ScreenshotResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: screenshot_response_to_query_result(&response.result),
        });
    }
}

pub(crate) fn forward_snapshot_responses(
    mut reader: MessageReader<BrowserSnapshotResponse>,
    service: Option<Res<ServiceClient>>,
    mut nav_awaiting: ResMut<NavAwaitingSnapshot>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        if nav_awaiting.0.remove(&response.request_id) {
            let result = match &response.result {
                Ok(json) => AgentCommandResult::Text(json.clone()),
                Err(message) => AgentCommandResult::Error(message.clone()),
            };
            service.0.send(ClientMessage::AgentCommandResponse {
                request_id: AgentRequestId(response.request_id),
                result,
            });
        } else {
            service.0.send(ClientMessage::AgentQueryResponse {
                request_id: AgentRequestId(response.request_id),
                result: snapshot_response_to_query_result(&response.result),
            });
        }
    }
}

pub(crate) fn record_start_response_to_query_result(
    result: &Result<u32, String>,
) -> AgentQueryResult {
    match result {
        Ok(max_secs) => AgentQueryResult::Text(format!("recording started, max {max_secs}s")),
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
}

pub(crate) fn forward_record_start_responses(
    mut reader: MessageReader<RecordStartResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: record_start_response_to_query_result(&response.result),
        });
    }
}

pub(crate) fn record_stop_response_to_query_result(
    result: &Result<RecordingInfo, String>,
) -> AgentQueryResult {
    match result {
        Ok(info) => AgentQueryResult::Recording {
            mp4_path: info.mp4_path.clone(),
            gif_path: info.gif_path.clone(),
            duration_ms: info.duration_ms,
            bytes: info.bytes,
            auto_stopped: info.auto_stopped,
        },
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
}

pub(crate) fn forward_record_stop_responses(
    mut reader: MessageReader<RecordStopResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: record_stop_response_to_query_result(&response.result),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub(crate) fn screenshot_response_maps_ok_and_err() {
        let ok = screenshot_response_to_query_result(&Ok(ScreenshotImage {
            path: "/tmp/a.png".into(),
            png: vec![9, 8, 7],
            width: 10,
            height: 20,
        }));
        assert!(matches!(
            ok,
            AgentQueryResult::Image { path, png, width, height }
                if path == "/tmp/a.png" && png == vec![9, 8, 7] && width == 10 && height == 20
        ));

        let err = screenshot_response_to_query_result(&Err("nope".to_string()));
        assert!(matches!(err, AgentQueryResult::Error(m) if m == "nope"));
    }

    #[test]
    pub(crate) fn record_start_response_maps_ok_and_err() {
        let ok = record_start_response_to_query_result(&Ok(120));
        assert!(matches!(ok, AgentQueryResult::Text(t) if t.contains("120")));
        let err = record_start_response_to_query_result(&Err("nope".to_string()));
        assert!(matches!(err, AgentQueryResult::Error(m) if m == "nope"));
    }

    #[test]
    pub(crate) fn record_stop_response_maps_ok_and_err() {
        let ok = record_stop_response_to_query_result(&Ok(RecordingInfo {
            mp4_path: "/tmp/x.mp4".into(),
            gif_path: None,
            duration_ms: 1000,
            bytes: 42,
            auto_stopped: false,
        }));
        assert!(
            matches!(ok, AgentQueryResult::Recording { mp4_path, .. } if mp4_path == "/tmp/x.mp4")
        );
        let err = record_stop_response_to_query_result(&Err("boom".to_string()));
        assert!(matches!(err, AgentQueryResult::Error(m) if m == "boom"));
    }
}
