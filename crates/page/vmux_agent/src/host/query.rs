use bevy::prelude::*;
use bevy_cef::prelude::HostWindow;
use vmux_command::WriteCommandRequests;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{
    AgentBookmark, AgentBookmarkNode, AgentBookmarks, AgentCommandResult, AgentQuery,
    AgentQueryResult, AgentRequestId, AgentSpace, ClientMessage, JsonValue,
};
use vmux_setting::AppSettings;
use vmux_terminal::ServiceMessageSet;

use crate::events::{
    AgentQueryRequest, RecordStartRequest, RecordStartResponse, RecordStopRequest,
    RecordStopResponse, RecordingInfo, ScreenshotImage, ScreenshotRequest, ScreenshotResponse,
    snapshot_response_to_query_result,
};
use vmux_core::browser::{
    BrowserScrollRequest, BrowserSnapshotRequest, BrowserSnapshotResponse, NavAwaitingSnapshot,
};

use super::browser_pane::AgentBrowserResolve;

pub(super) struct QueryPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct QuerySet;

impl Plugin for QueryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                route_layout_queries,
                answer_working_directory_queries,
                answer_settings_queries,
                answer_space_queries,
                answer_command_queries,
                answer_vault_queries,
                answer_bookmark_queries,
                route_capture_queries,
                route_browser_queries,
                route_simulator_queries,
            )
                .in_set(QuerySet)
                .in_set(WriteCommandRequests)
                .after(ServiceMessageSet)
                .after(super::workspace::send_pending_agent_continuations),
        )
        .add_systems(
            Update,
            (
                forward_layout_apply_responses,
                forward_layout_snapshot_responses,
                forward_screenshot_responses,
                forward_snapshot_responses,
                forward_record_start_responses,
                forward_record_stop_responses,
                forward_simulator_control_responses,
                forward_simulator_screenshot_responses,
            ),
        );
    }
}

struct AgentSpaceCatalog(Vec<AgentSpace>);

impl AgentSpaceCatalog {
    fn collect(
        spaces: &Query<
            (
                Entity,
                &vmux_layout::space::SpaceId,
                &Name,
                Has<vmux_core::Active>,
                Option<&vmux_core::Order>,
            ),
            With<vmux_layout::space::Space>,
        >,
        focused_window: Option<&vmux_layout::window::FocusedWindow>,
        child_of: &Query<&ChildOf>,
        host_windows: &Query<&HostWindow>,
    ) -> Self {
        let mut rows: Vec<(u32, AgentSpace)> = Vec::new();
        for (entity, id, name, is_active, order) in spaces {
            let local = focused_window
                .and_then(|focused| focused.0)
                .is_some_and(|focused| {
                    vmux_layout::window::host_window_of(entity, child_of, host_windows)
                        == Some(focused)
                });
            let order = order.map(|order| order.0).unwrap_or(u32::MAX);
            if let Some((existing_order, row)) =
                rows.iter_mut().find(|(_, existing)| existing.id == id.0)
            {
                *existing_order = (*existing_order).min(order);
                if local {
                    row.is_active = is_active;
                }
                continue;
            }
            rows.push((
                order,
                AgentSpace {
                    id: id.0.clone(),
                    name: name.to_string(),
                    profile: vmux_space::model::bootstrap_profile_name(),
                    is_active: local && is_active,
                },
            ));
        }
        rows.sort_by_key(|(order, _)| *order);
        Self(rows.into_iter().map(|(_, row)| row).collect())
    }
}

fn route_layout_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    mut writer: MessageWriter<vmux_layout::apply::LayoutSnapshotRequest>,
) {
    let Some(_) = service else { return };
    for request in reader.read() {
        let AgentQuery::ReadLayout { anchor } = request.query else {
            continue;
        };
        writer.write(vmux_layout::apply::LayoutSnapshotRequest {
            request_id: request.request_id.0,
            anchor,
        });
    }
}

fn answer_working_directory_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    browse: AgentBrowserResolve,
    tabs: Query<&vmux_layout::tab::Tab>,
) {
    let Some(service) = service else { return };
    for request in reader.read() {
        let AgentQuery::WorkingDirectory { anchor } = request.query else {
            continue;
        };
        let result = if browse.agent_pane(anchor).is_none() {
            AgentQueryResult::Error("agent pane not found".to_string())
        } else if let Some(path) = browse.working_directory(anchor, &tabs) {
            AgentQueryResult::Text(path.to_string_lossy().into_owned())
        } else {
            match super::run_terminal::AgentCwd::projects() {
                Ok(path) => AgentQueryResult::Text(path.to_string_lossy().into_owned()),
                Err(message) => AgentQueryResult::Error(message),
            }
        };
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: request.request_id,
            result,
        });
    }
}

fn answer_settings_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    settings: Res<AppSettings>,
) {
    let Some(service) = service else { return };
    for request in reader.read() {
        if !matches!(request.query, AgentQuery::GetSettings) {
            continue;
        }
        let result = match serde_json::to_value(&*settings) {
            Ok(settings) => AgentQueryResult::Settings(JsonValue::from(settings)),
            Err(error) => AgentQueryResult::Error(format!("failed to serialize settings: {error}")),
        };
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: request.request_id,
            result,
        });
    }
}

fn answer_space_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    spaces: Query<
        (
            Entity,
            &vmux_layout::space::SpaceId,
            &Name,
            Has<vmux_core::Active>,
            Option<&vmux_core::Order>,
        ),
        With<vmux_layout::space::Space>,
    >,
    focused_window: Option<Res<vmux_layout::window::FocusedWindow>>,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
) {
    let Some(service) = service else { return };
    for request in reader.read() {
        if !matches!(request.query, AgentQuery::ListSpaces) {
            continue;
        }
        let rows = AgentSpaceCatalog::collect(
            &spaces,
            focused_window.as_deref(),
            &child_of,
            &host_windows,
        );
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: request.request_id,
            result: AgentQueryResult::Spaces(rows.0),
        });
    }
}

fn answer_command_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    commands: Query<&vmux_command::CommandDefinition>,
) {
    let Some(service) = service else { return };
    for request in reader.read() {
        if !matches!(request.query, AgentQuery::ListCommands) {
            continue;
        }
        let mut tools = commands
            .iter()
            .filter_map(vmux_command::CommandDefinition::agent_tool)
            .collect::<Vec<_>>();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: request.request_id,
            result: AgentQueryResult::Commands(tools),
        });
    }
}

fn answer_vault_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for request in reader.read() {
        if !matches!(request.query, AgentQuery::VaultStatus) {
            continue;
        }
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: request.request_id,
            result: AgentQueryResult::VaultStatus(vmux_core::profile::vault::status().snapshot()),
        });
    }
}

fn answer_bookmark_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    pins: Query<
        (
            &vmux_core::Uuid,
            &vmux_core::PageMetadata,
            &vmux_core::BookmarkOrder,
        ),
        With<vmux_core::Pin>,
    >,
    folders: Query<
        (
            &vmux_core::Uuid,
            &Name,
            Option<&Children>,
            Has<vmux_core::Collapsed>,
            &vmux_core::BookmarkOrder,
        ),
        With<vmux_core::Folder>,
    >,
    top_level: Query<
        (
            &vmux_core::Uuid,
            &vmux_core::PageMetadata,
            &vmux_core::BookmarkOrder,
        ),
        (With<vmux_core::Bookmark>, Without<ChildOf>),
    >,
    bookmarks: Query<
        (
            &vmux_core::Uuid,
            &vmux_core::PageMetadata,
            &vmux_core::BookmarkOrder,
        ),
        With<vmux_core::Bookmark>,
    >,
) {
    let Some(service) = service else { return };
    for request in reader.read() {
        if !matches!(request.query, AgentQuery::BookmarkList) {
            continue;
        }
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
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: request.request_id,
            result: AgentQueryResult::Bookmarks(AgentBookmarks { pins, roots }),
        });
    }
}

fn route_capture_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    mut screenshot_writer: MessageWriter<ScreenshotRequest>,
    mut record_start_writer: MessageWriter<RecordStartRequest>,
    mut record_stop_writer: MessageWriter<RecordStopRequest>,
) {
    let Some(_) = service else { return };
    for request in reader.read() {
        match &request.query {
            AgentQuery::Screenshot { pane } => {
                screenshot_writer.write(ScreenshotRequest {
                    request_id: request.request_id.0,
                    pane: pane.clone(),
                });
            }
            AgentQuery::RecordStart {
                gif,
                max_secs,
                pane,
            } => {
                record_start_writer.write(RecordStartRequest {
                    request_id: request.request_id.0,
                    gif: *gif,
                    max_secs: *max_secs,
                    pane: pane.clone(),
                });
            }
            AgentQuery::RecordStop { dir, name } => {
                record_stop_writer.write(RecordStopRequest {
                    request_id: request.request_id.0,
                    dir: dir.clone(),
                    name: name.clone(),
                });
            }
            _ => {}
        }
    }
}

fn route_browser_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    mut snapshot_writer: MessageWriter<BrowserSnapshotRequest>,
    mut scroll_writer: MessageWriter<BrowserScrollRequest>,
    mut activate: MessageWriter<vmux_layout::active_panes::ActivatePane>,
    browse: AgentBrowserResolve,
) {
    let Some(_) = service else { return };
    for request in reader.read() {
        match &request.query {
            AgentQuery::BrowserSnapshot { pane, anchor } => {
                let resolved = browse.resolve_pane(pane, anchor);
                if let Some(request) = resolved.activation {
                    activate.write(request);
                }
                snapshot_writer.write(BrowserSnapshotRequest {
                    request_id: request.request_id.0,
                    pane: resolved.pane,
                    webview: None,
                });
            }
            AgentQuery::BrowserScroll {
                pane,
                to,
                delta,
                anchor,
            } => {
                let resolved = browse.resolve_pane(pane, anchor);
                if let Some(request) = resolved.activation {
                    activate.write(request);
                }
                scroll_writer.write(BrowserScrollRequest {
                    request_id: request.request_id.0,
                    pane: resolved.pane,
                    to: to.clone(),
                    delta: *delta,
                });
            }
            _ => {}
        }
    }
}

fn route_simulator_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    mut control_writer: MessageWriter<vmux_simulator::SimulatorControlRequest>,
    mut screenshot_writer: MessageWriter<vmux_simulator::SimulatorScreenshotRequest>,
) {
    let Some(_) = service else { return };
    for request in reader.read() {
        match &request.query {
            AgentQuery::SimulatorScreenshot => {
                screenshot_writer.write(vmux_simulator::SimulatorScreenshotRequest {
                    request_id: request.request_id.0,
                });
            }
            AgentQuery::SimulatorControl { input } => {
                control_writer.write(vmux_simulator::SimulatorControlRequest {
                    request_id: request.request_id.0,
                    input: input.clone(),
                });
            }
            _ => {}
        }
    }
}

fn forward_layout_apply_responses(
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

fn forward_layout_snapshot_responses(
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

fn screenshot_response_to_query_result(
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

fn forward_screenshot_responses(
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

fn forward_snapshot_responses(
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

fn record_start_response_to_query_result(result: &Result<u32, String>) -> AgentQueryResult {
    match result {
        Ok(max_secs) => AgentQueryResult::Text(format!("recording started, max {max_secs}s")),
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
}

fn forward_record_start_responses(
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

fn record_stop_response_to_query_result(
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

fn forward_record_stop_responses(
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

fn forward_simulator_control_responses(
    mut reader: MessageReader<vmux_simulator::SimulatorControlResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        let result = match &response.result {
            Ok(message) => AgentQueryResult::Text(message.clone()),
            Err(message) => AgentQueryResult::Error(message.clone()),
        };
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result,
        });
    }
}

fn forward_simulator_screenshot_responses(
    mut reader: MessageReader<vmux_simulator::SimulatorScreenshotResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        let result = match &response.result {
            Ok(image) => AgentQueryResult::Image {
                path: image.path.clone(),
                png: image.png.clone(),
                width: image.width,
                height: image.height,
            },
            Err(message) => AgentQueryResult::Error(message.clone()),
        };
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn collect_space_rows(
        spaces: Query<
            (
                Entity,
                &vmux_layout::space::SpaceId,
                &Name,
                Has<vmux_core::Active>,
                Option<&vmux_core::Order>,
            ),
            With<vmux_layout::space::Space>,
        >,
        focused_window: Option<Res<vmux_layout::window::FocusedWindow>>,
        child_of: Query<&ChildOf>,
        host_windows: Query<&HostWindow>,
    ) -> Vec<AgentSpace> {
        AgentSpaceCatalog::collect(&spaces, focused_window.as_deref(), &child_of, &host_windows).0
    }

    #[test]
    fn listed_spaces_are_global_but_active_state_is_window_local() {
        let mut app = App::new();
        let first_window = app.world_mut().spawn_empty().id();
        let second_window = app.world_mut().spawn_empty().id();
        app.insert_resource(vmux_layout::window::FocusedWindow(Some(second_window)));
        let first_root = app.world_mut().spawn(HostWindow(first_window)).id();
        let second_root = app.world_mut().spawn(HostWindow(second_window)).id();
        app.world_mut().spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("shared".to_string()),
            Name::new("shared"),
            vmux_core::Active,
            ChildOf(first_root),
        ));
        app.world_mut().spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("shared".to_string()),
            Name::new("shared"),
            ChildOf(second_root),
        ));
        app.world_mut().spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("local".to_string()),
            Name::new("local"),
            vmux_core::Active,
            ChildOf(second_root),
        ));

        let rows = app.world_mut().run_system_once(collect_space_rows).unwrap();

        assert_eq!(rows.len(), 2);
        assert!(
            !rows
                .iter()
                .find(|row| row.id == "shared")
                .unwrap()
                .is_active
        );
        assert!(rows.iter().find(|row| row.id == "local").unwrap().is_active);
    }

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
