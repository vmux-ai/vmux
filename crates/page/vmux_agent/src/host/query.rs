use bevy::prelude::*;
use bevy_cef::prelude::HostWindow;
use vmux_command::WriteCommandRequests;
use vmux_service::client::{ServiceInbound, ServiceRequest};
use vmux_service::protocol::{
    AgentBookmark, AgentBookmarkNode, AgentBookmarks, AgentCommandResult, AgentImage, AgentQuery,
    AgentRecording, AgentRequestId, AgentSpace, ClientMessage, JsonValue, ProcessId,
    ServiceMessage,
};
use vmux_setting::AppSettings;
use vmux_terminal::ServiceMessageSet;

use crate::events::{
    AgentQueryRequest, RecordStartRequest, RecordStartResponse, RecordStopRequest,
    RecordStopResponse, RecordingInfo, ScreenshotImage, ScreenshotRequest, ScreenshotResponse,
};
use vmux_core::browser::{
    BrowserNavigationSnapshotResponse, BrowserScrollRequest, BrowserScrollResponse,
    BrowserSnapshotRequest, BrowserSnapshotResponse,
};

use super::browser_pane::AgentBrowserResolve;

pub(crate) struct AgentQueryPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AgentQueryIngressSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AgentQuerySet;

#[derive(Message)]
struct WorkingDirectoryRequest {
    request_id: AgentRequestId,
    anchor: ProcessId,
}

#[derive(Message)]
struct SettingsReadRequest {
    request_id: AgentRequestId,
}

#[derive(Message)]
struct SpaceListRequest {
    request_id: AgentRequestId,
}

#[derive(Message)]
struct CommandListRequest {
    request_id: AgentRequestId,
}

#[derive(Message)]
struct VaultStatusRequest {
    request_id: AgentRequestId,
}

#[derive(Message)]
struct BookmarkListRequest {
    request_id: AgentRequestId,
}

#[derive(Message)]
struct BrowserSnapshotResolveRequest {
    request_id: AgentRequestId,
    pane: Option<String>,
    anchor: Option<ProcessId>,
}

#[derive(Message)]
struct BrowserScrollResolveRequest {
    request_id: AgentRequestId,
    pane: Option<String>,
    to: Option<String>,
    delta: Option<i32>,
    anchor: Option<ProcessId>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct AgentQueryRoutes<'w> {
    layout: MessageWriter<'w, vmux_layout::apply::LayoutSnapshotRequest>,
    working_directory: MessageWriter<'w, WorkingDirectoryRequest>,
    settings: MessageWriter<'w, SettingsReadRequest>,
    spaces: MessageWriter<'w, SpaceListRequest>,
    commands: MessageWriter<'w, CommandListRequest>,
    vault: MessageWriter<'w, VaultStatusRequest>,
    bookmarks: MessageWriter<'w, BookmarkListRequest>,
    screenshot: MessageWriter<'w, ScreenshotRequest>,
    browser_snapshot: MessageWriter<'w, BrowserSnapshotResolveRequest>,
    browser_scroll: MessageWriter<'w, BrowserScrollResolveRequest>,
    record_start: MessageWriter<'w, RecordStartRequest>,
    record_stop: MessageWriter<'w, RecordStopRequest>,
    simulator_screenshot: MessageWriter<'w, vmux_simulator::SimulatorScreenshotRequest>,
    simulator_control: MessageWriter<'w, vmux_simulator::SimulatorControlRequest>,
}

impl AgentQueryRoutes<'_> {
    fn route(&mut self, request_id: AgentRequestId, query: &AgentQuery) {
        match query {
            AgentQuery::ReadLayout { anchor } => {
                self.layout
                    .write(vmux_layout::apply::LayoutSnapshotRequest {
                        request_id: request_id.0,
                        anchor: *anchor,
                    });
            }
            AgentQuery::GetSettings => {
                self.settings.write(SettingsReadRequest { request_id });
            }
            AgentQuery::ListSpaces => {
                self.spaces.write(SpaceListRequest { request_id });
            }
            AgentQuery::Screenshot { pane } => {
                self.screenshot.write(ScreenshotRequest {
                    request_id: request_id.0,
                    pane: pane.clone(),
                });
            }
            AgentQuery::BrowserSnapshot { pane, anchor } => {
                self.browser_snapshot.write(BrowserSnapshotResolveRequest {
                    request_id,
                    pane: pane.clone(),
                    anchor: *anchor,
                });
            }
            AgentQuery::BrowserScroll {
                pane,
                to,
                delta,
                anchor,
            } => {
                self.browser_scroll.write(BrowserScrollResolveRequest {
                    request_id,
                    pane: pane.clone(),
                    to: to.clone(),
                    delta: *delta,
                    anchor: *anchor,
                });
            }
            AgentQuery::RecordStart {
                gif,
                max_secs,
                pane,
            } => {
                self.record_start.write(RecordStartRequest {
                    request_id: request_id.0,
                    gif: *gif,
                    max_secs: *max_secs,
                    pane: pane.clone(),
                });
            }
            AgentQuery::RecordStop { dir, name } => {
                self.record_stop.write(RecordStopRequest {
                    request_id: request_id.0,
                    dir: dir.clone(),
                    name: name.clone(),
                });
            }
            AgentQuery::BookmarkList => {
                self.bookmarks.write(BookmarkListRequest { request_id });
            }
            AgentQuery::SimulatorScreenshot => {
                self.simulator_screenshot
                    .write(vmux_simulator::SimulatorScreenshotRequest {
                        request_id: request_id.0,
                    });
            }
            AgentQuery::SimulatorControl { input } => {
                self.simulator_control
                    .write(vmux_simulator::SimulatorControlRequest {
                        request_id: request_id.0,
                        input: input.clone(),
                    });
            }
            AgentQuery::WorkingDirectory { anchor } => {
                self.working_directory.write(WorkingDirectoryRequest {
                    request_id,
                    anchor: *anchor,
                });
            }
            AgentQuery::VaultStatus => {
                self.vault.write(VaultStatusRequest { request_id });
            }
            AgentQuery::ListCommands => {
                self.commands.write(CommandListRequest { request_id });
            }
            AgentQuery::ReadTerminal { .. }
            | AgentQuery::ReadTerminalFull { .. }
            | AgentQuery::CommandExit { .. }
            | AgentQuery::RunCompletion { .. } => {}
        }
    }
}

impl Plugin for AgentQueryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceInbound>()
            .add_message::<WorkingDirectoryRequest>()
            .add_message::<SettingsReadRequest>()
            .add_message::<SpaceListRequest>()
            .add_message::<CommandListRequest>()
            .add_message::<VaultStatusRequest>()
            .add_message::<BookmarkListRequest>()
            .add_message::<BrowserSnapshotResolveRequest>()
            .add_message::<BrowserScrollResolveRequest>()
            .add_systems(
                Update,
                receive_agent_queries
                    .in_set(AgentQueryIngressSet)
                    .after(ServiceMessageSet)
                    .after(super::AgentContinuationSet),
            )
            .add_systems(
                Update,
                (
                    answer_working_directory_queries,
                    answer_settings_queries,
                    answer_space_queries,
                    answer_command_queries,
                    answer_vault_queries,
                    answer_bookmark_queries,
                    route_browser_queries,
                )
                    .in_set(AgentQuerySet)
                    .in_set(WriteCommandRequests)
                    .after(AgentQueryIngressSet),
            )
            .add_systems(
                Update,
                (
                    forward_layout_apply_responses,
                    forward_layout_snapshot_responses,
                    forward_screenshot_responses,
                    forward_snapshot_responses,
                    forward_browser_scroll_responses,
                    forward_navigation_snapshot_responses,
                    forward_record_start_responses,
                    forward_record_stop_responses,
                    forward_simulator_control_responses,
                    forward_simulator_screenshot_responses,
                ),
            );
    }
}

fn receive_agent_queries(
    mut inbound: MessageReader<ServiceInbound>,
    mut local: MessageReader<AgentQueryRequest>,
    mut routes: AgentQueryRoutes,
) {
    for inbound in inbound.read() {
        let ServiceMessage::AgentQuery { request_id, query } = &inbound.0 else {
            continue;
        };
        routes.route(*request_id, query);
    }
    for request in local.read() {
        routes.route(request.request_id, &request.query);
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

fn answer_working_directory_queries(
    mut reader: MessageReader<WorkingDirectoryRequest>,
    browse: AgentBrowserResolve,
    tabs: Query<&vmux_layout::tab::Tab>,
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

fn answer_settings_queries(
    mut reader: MessageReader<SettingsReadRequest>,
    settings: Res<AppSettings>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let result = match serde_json::to_value(&*settings) {
            Ok(settings) => Ok(JsonValue::from(settings)),
            Err(error) => Err(format!("failed to serialize settings: {error}")),
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentSettingsResult {
            request_id: request.request_id,
            result,
        }));
    }
}

fn answer_space_queries(
    mut reader: MessageReader<SpaceListRequest>,
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
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let rows = AgentSpaceCatalog::collect(
            &spaces,
            focused_window.as_deref(),
            &child_of,
            &host_windows,
        );
        service_requests.write(ServiceRequest(ClientMessage::AgentSpacesResult {
            request_id: request.request_id,
            result: Ok(rows.0),
        }));
    }
}

fn answer_command_queries(
    mut reader: MessageReader<CommandListRequest>,
    commands: Query<&vmux_command::CommandDefinition>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let mut tools = commands
            .iter()
            .filter_map(vmux_command::CommandDefinition::agent_tool)
            .collect::<Vec<_>>();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        service_requests.write(ServiceRequest(ClientMessage::AgentCommandsResult {
            request_id: request.request_id,
            result: Ok(tools),
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
            result: Ok(vmux_core::profile::vault::status().snapshot()),
        }));
    }
}

fn answer_bookmark_queries(
    mut reader: MessageReader<BookmarkListRequest>,
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

fn route_browser_queries(
    mut snapshots: MessageReader<BrowserSnapshotResolveRequest>,
    mut scrolls: MessageReader<BrowserScrollResolveRequest>,
    mut snapshot_writer: MessageWriter<BrowserSnapshotRequest>,
    mut scroll_writer: MessageWriter<BrowserScrollRequest>,
    mut activate: MessageWriter<vmux_layout::active_pane::ActivatePane>,
    browse: AgentBrowserResolve,
) {
    for request in snapshots.read() {
        let resolved = browse.resolve_pane(&request.pane, &request.anchor);
        if let Some(request) = resolved.activation {
            activate.write(request);
        }
        snapshot_writer.write(BrowserSnapshotRequest {
            request_id: request.request_id.0,
            pane: resolved.pane,
            webview: None,
        });
    }

    for request in scrolls.read() {
        let resolved = browse.resolve_pane(&request.pane, &request.anchor);
        if let Some(request) = resolved.activation {
            activate.write(request);
        }
        scroll_writer.write(BrowserScrollRequest {
            request_id: request.request_id.0,
            pane: resolved.pane,
            to: request.to.clone(),
            delta: request.delta,
        });
    }
}

fn forward_layout_apply_responses(
    mut reader: MessageReader<vmux_layout::apply::LayoutApplyResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in reader.read() {
        let result = match response.result.clone() {
            Ok(snapshot) => AgentCommandResult::Layout(snapshot),
            Err(message) => AgentCommandResult::Error(message),
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentCommandResponse {
            request_id: AgentRequestId(response.request_id),
            result,
        }));
    }
}

fn forward_layout_snapshot_responses(
    mut reader: MessageReader<vmux_layout::apply::LayoutSnapshotResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in reader.read() {
        service_requests.write(ServiceRequest(ClientMessage::AgentLayoutResult {
            request_id: AgentRequestId(response.request_id),
            result: Ok(response.snapshot.clone()),
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

fn forward_snapshot_responses(
    mut reader: MessageReader<BrowserSnapshotResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in reader.read() {
        service_requests.write(ServiceRequest(ClientMessage::AgentBrowserSnapshotResult {
            request_id: AgentRequestId(response.request_id),
            result: response.result.clone(),
        }));
    }
}

fn forward_browser_scroll_responses(
    mut reader: MessageReader<BrowserScrollResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in reader.read() {
        service_requests.write(ServiceRequest(ClientMessage::AgentBrowserScrollResult {
            request_id: AgentRequestId(response.request_id),
            result: response.result.clone(),
        }));
    }
}

fn forward_navigation_snapshot_responses(
    mut reader: MessageReader<BrowserNavigationSnapshotResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in reader.read() {
        let result = match &response.result {
            Ok(json) => AgentCommandResult::Text(json.clone()),
            Err(message) => AgentCommandResult::Error(message.clone()),
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentCommandResponse {
            request_id: AgentRequestId(response.request_id),
            result,
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

fn forward_simulator_control_responses(
    mut reader: MessageReader<vmux_simulator::SimulatorControlResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in reader.read() {
        service_requests.write(ServiceRequest(ClientMessage::AgentSimulatorControlResult {
            request_id: AgentRequestId(response.request_id),
            result: response.result.clone(),
        }));
    }
}

fn forward_simulator_screenshot_responses(
    mut reader: MessageReader<vmux_simulator::SimulatorScreenshotResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in reader.read() {
        let result = match &response.result {
            Ok(image) => Ok(AgentImage {
                path: image.path.clone(),
                png: image.png.clone(),
                width: image.width,
                height: image.height,
            }),
            Err(message) => Err(message.clone()),
        };
        service_requests.write(ServiceRequest(
            ClientMessage::AgentSimulatorScreenshotResult {
                request_id: AgentRequestId(response.request_id),
                result,
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn query_routing_app() -> App {
        let mut app = App::new();
        app.add_message::<ServiceInbound>()
            .add_message::<AgentQueryRequest>()
            .add_message::<vmux_layout::apply::LayoutSnapshotRequest>()
            .add_message::<WorkingDirectoryRequest>()
            .add_message::<SettingsReadRequest>()
            .add_message::<SpaceListRequest>()
            .add_message::<CommandListRequest>()
            .add_message::<VaultStatusRequest>()
            .add_message::<BookmarkListRequest>()
            .add_message::<ScreenshotRequest>()
            .add_message::<BrowserSnapshotResolveRequest>()
            .add_message::<BrowserScrollResolveRequest>()
            .add_message::<RecordStartRequest>()
            .add_message::<RecordStopRequest>()
            .add_message::<vmux_simulator::SimulatorScreenshotRequest>()
            .add_message::<vmux_simulator::SimulatorControlRequest>()
            .add_systems(Update, receive_agent_queries);
        app
    }

    #[test]
    fn service_and_local_queries_are_decoded_into_typed_requests() {
        let mut app = query_routing_app();
        let service_request_id = AgentRequestId([1; 16]);
        let local_request_id = AgentRequestId([2; 16]);
        let mut screenshots = app
            .world()
            .resource::<Messages<ScreenshotRequest>>()
            .get_cursor();
        let mut vault = app
            .world()
            .resource::<Messages<VaultStatusRequest>>()
            .get_cursor();

        app.world_mut()
            .write_message(ServiceInbound(ServiceMessage::AgentQuery {
                request_id: service_request_id,
                query: AgentQuery::Screenshot {
                    pane: Some("pane:7".to_string()),
                },
            }));
        app.world_mut().write_message(AgentQueryRequest {
            request_id: local_request_id,
            query: AgentQuery::VaultStatus,
        });
        app.update();

        let screenshot = screenshots
            .read(app.world().resource::<Messages<ScreenshotRequest>>())
            .next()
            .expect("expected screenshot request");
        assert_eq!(screenshot.request_id, service_request_id.0);
        assert_eq!(screenshot.pane.as_deref(), Some("pane:7"));
        let vault = vault
            .read(app.world().resource::<Messages<VaultStatusRequest>>())
            .next()
            .expect("expected vault request");
        assert_eq!(vault.request_id, local_request_id);
    }

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
    pub(crate) fn record_stop_response_maps_ok_and_err() {
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
