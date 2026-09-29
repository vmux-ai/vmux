use bevy::prelude::*;
use vmux_api::BinEvent;
use vmux_api::protocol::{
    AgentCommandResult, AgentOpenInNewStack, AgentQueryResult, AgentRequestId,
    AgentWorkingDirectory, ClientMessage,
};
use vmux_core::agent::{
    AgentCommandResponse, AgentReply, AgentRequestAppExt, AgentRequestMessage,
    AgentRequestRouteSet, CommandOrigin,
};
use vmux_core::browser::{
    BrowserNavigationSnapshotResponse, BrowserScrollRequest, BrowserScrollResponse,
    BrowserSnapshotRequest, BrowserSnapshotResponse,
};
use vmux_core::profile::ProjectsDirectory;
use vmux_core::service::ServiceRequest;
use vmux_extension::ExtensionInstallRequest;
use vmux_history::HistoryOpenIntent;
use vmux_layout::active_pane::ActivatePane;
use vmux_layout::{
    BrowserGoBackRequest, BrowserGoForwardRequest, BrowserNavigateRequest, OpenBesideRequest,
    OpenInNewStackRequest,
};
use vmux_tool::{ToolQueryHandled, ToolQueryRequest, ToolQueryRouteSet};

use super::agent_pane::AgentBrowserResolve;

#[vmux_api::agent]
pub(crate) struct AgentBrowserNavigate {
    pub url: String,
    pub pane: Option<String>,
}

#[vmux_api::agent]
pub(crate) struct AgentBrowserInstallExtension {
    pub source: String,
}

#[vmux_api::agent]
pub(crate) struct AgentBrowserGoBack {
    pub pane: Option<String>,
}

#[vmux_api::agent]
pub(crate) struct AgentBrowserGoForward {
    pub pane: Option<String>,
}

#[vmux_api::agent]
pub(crate) struct AgentBrowserHistorySearch {
    pub query: String,
    pub limit: u32,
}

#[vmux_api::agent(Eq)]
pub(crate) struct AgentBrowserSnapshot {
    pub pane: Option<String>,
    pub anchor: Option<vmux_core::ProcessId>,
}

#[vmux_api::agent(Eq)]
pub(crate) struct AgentBrowserScroll {
    pub pane: Option<String>,
    pub to: Option<String>,
    pub delta: Option<i32>,
    pub anchor: Option<vmux_core::ProcessId>,
}

pub(crate) struct AgentBrowserPlugin;

impl Plugin for AgentBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_agent_request::<AgentBrowserNavigate>()
            .add_agent_request::<AgentBrowserInstallExtension>()
            .add_agent_request::<AgentBrowserGoBack>()
            .add_agent_request::<AgentBrowserGoForward>()
            .add_agent_request::<AgentBrowserHistorySearch>()
            .add_agent_request::<AgentOpenInNewStack>()
            .add_message::<ToolQueryRequest>()
            .add_message::<ToolQueryHandled>()
            .add_message::<ServiceRequest>()
            .add_message::<BrowserSnapshotRequest>()
            .add_message::<BrowserSnapshotResponse>()
            .add_message::<BrowserScrollRequest>()
            .add_message::<BrowserScrollResponse>()
            .add_message::<BrowserNavigationSnapshotResponse>()
            .add_message::<ActivatePane>()
            .add_message::<WorkingDirectoryRequest>()
            .add_message::<ExtensionInstallRequest>()
            .add_systems(Update, open_history)
            .add_systems(
                Update,
                (
                    route_browser_queries.in_set(ToolQueryRouteSet),
                    answer_working_directory_queries.after(ToolQueryRouteSet),
                    forward_snapshot_responses,
                    forward_scroll_responses,
                    forward_navigation_snapshot_responses,
                ),
            )
            .add_systems(
                Update,
                ((
                    navigate,
                    install_extension,
                    go_back,
                    go_forward,
                    search_history,
                    open_in_new_stack,
                )
                    .after(AgentRequestRouteSet),)
                    .chain(),
            );
    }
}

#[derive(Message)]
struct WorkingDirectoryRequest {
    request_id: AgentRequestId,
    anchor: vmux_core::ProcessId,
}

fn route_browser_queries(
    mut queries: MessageReader<ToolQueryRequest>,
    mut handled: MessageWriter<ToolQueryHandled>,
    mut snapshots: MessageWriter<BrowserSnapshotRequest>,
    mut scrolls: MessageWriter<BrowserScrollRequest>,
    mut working_directories: MessageWriter<WorkingDirectoryRequest>,
    mut activate: MessageWriter<ActivatePane>,
    mut service_requests: MessageWriter<ServiceRequest>,
    browse: AgentBrowserResolve,
) {
    for request in queries.read() {
        if request.query.id != AgentBrowserSnapshot::id()
            && request.query.id != AgentBrowserScroll::id()
            && request.query.id != AgentWorkingDirectory::id()
        {
            continue;
        }
        handled.write(ToolQueryHandled(request.request_id));
        match request.query.decode::<AgentBrowserSnapshot>() {
            Ok(Some(query)) => {
                let resolved = browse.resolve_pane(&query.pane, &query.anchor);
                if let Some(request) = resolved.activation {
                    activate.write(request);
                }
                snapshots.write(BrowserSnapshotRequest {
                    request_id: request.request_id.0,
                    pane: resolved.pane,
                    webview: None,
                });
                continue;
            }
            Err(message) => {
                service_requests.write(ServiceRequest(ClientMessage::AgentQueryError {
                    request_id: request.request_id,
                    message,
                }));
                continue;
            }
            Ok(None) => {}
        }
        match request.query.decode::<AgentBrowserScroll>() {
            Ok(Some(query)) => {
                let resolved = browse.resolve_pane(&query.pane, &query.anchor);
                if let Some(request) = resolved.activation {
                    activate.write(request);
                }
                scrolls.write(BrowserScrollRequest {
                    request_id: request.request_id.0,
                    pane: resolved.pane,
                    to: query.to,
                    delta: query.delta,
                });
            }
            Err(message) => {
                service_requests.write(ServiceRequest(ClientMessage::AgentQueryError {
                    request_id: request.request_id,
                    message,
                }));
            }
            Ok(None) => {}
        }
        match request.query.decode::<AgentWorkingDirectory>() {
            Ok(Some(query)) => {
                working_directories.write(WorkingDirectoryRequest {
                    request_id: request.request_id,
                    anchor: query.anchor,
                });
            }
            Err(message) => {
                service_requests.write(ServiceRequest(ClientMessage::AgentQueryError {
                    request_id: request.request_id,
                    message,
                }));
            }
            Ok(None) => {}
        }
    }
}

fn answer_working_directory_queries(
    mut requests: MessageReader<WorkingDirectoryRequest>,
    browse: AgentBrowserResolve,
    tabs: Query<&vmux_layout::tab::Tab>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let result = if browse.agent_pane(request.anchor).is_none() {
            Err("agent pane not found".to_string())
        } else if let Some(path) = browse.working_directory(request.anchor, &tabs) {
            Ok(path.to_string_lossy().into_owned())
        } else {
            ProjectsDirectory::ensure()
                .map(ProjectsDirectory::into_path)
                .map(|path| path.to_string_lossy().into_owned())
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentWorkingDirectoryResult {
            request_id: request.request_id,
            result,
        }));
    }
}

fn forward_snapshot_responses(
    mut responses: MessageReader<BrowserSnapshotResponse>,
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

fn forward_scroll_responses(
    mut responses: MessageReader<BrowserScrollResponse>,
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

fn forward_navigation_snapshot_responses(
    mut responses: MessageReader<BrowserNavigationSnapshotResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in responses.read() {
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

fn open_history(
    mut intents: MessageReader<HistoryOpenIntent>,
    mut navigate: MessageWriter<AgentRequestMessage<AgentBrowserNavigate>>,
    mut open_in_new_stack: MessageWriter<AgentRequestMessage<AgentOpenInNewStack>>,
) {
    for intent in intents.read() {
        let reply = AgentReply::new(AgentRequestId::new());
        if intent.in_new_stack {
            open_in_new_stack.write(AgentRequestMessage {
                reply,
                origin: CommandOrigin::User,
                payload: AgentOpenInNewStack {
                    url: intent.url.clone(),
                },
            });
        } else {
            navigate.write(AgentRequestMessage {
                reply,
                origin: CommandOrigin::User,
                payload: AgentBrowserNavigate {
                    url: intent.url.clone(),
                    pane: None,
                },
            });
        }
    }
}

fn navigate(
    mut requests: MessageReader<AgentRequestMessage<AgentBrowserNavigate>>,
    mut navigate: MessageWriter<BrowserNavigateRequest>,
    mut open_beside: MessageWriter<OpenBesideRequest>,
    mut activate: MessageWriter<ActivatePane>,
    browse: AgentBrowserResolve,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let mut pane = request.payload.pane.clone();
        let mut new_stack = false;
        let mut profile = None;
        if pane.is_none()
            && let CommandOrigin::Agent {
                anchor: Some(anchor),
                ..
            } = &request.origin
        {
            profile = Some(format!("{anchor:?}"));
            if let Some(claim) = browse.claim_browser_pane(*anchor) {
                pane = Some(claim.pane.to_bits().to_string());
                new_stack = true;
                activate.write(claim.activation);
            } else if let Some(agent_pane) = browse.agent_pane(*anchor) {
                open_beside.write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: request.payload.url.clone(),
                    request_id: request.reply.request_id.0,
                    focus: false,
                });
                continue;
            } else {
                responses.write(request.reply.response(AgentCommandResult::Error(
                    "browser_navigate: agent has no resolvable pane".to_string(),
                )));
                continue;
            }
        }
        navigate.write(BrowserNavigateRequest {
            url: request.payload.url.clone(),
            pane,
            request_id: Some(request.reply.request_id.0),
            new_stack,
            profile,
        });
    }
}

fn install_extension(
    mut requests: MessageReader<AgentRequestMessage<AgentBrowserInstallExtension>>,
    mut install: MessageWriter<ExtensionInstallRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        install.write(ExtensionInstallRequest {
            source: request.payload.source.clone(),
            requester: None,
        });
        responses.write(request.reply.ok());
    }
}

fn go_back(
    mut requests: MessageReader<AgentRequestMessage<AgentBrowserGoBack>>,
    mut go_back: MessageWriter<BrowserGoBackRequest>,
    mut activate: MessageWriter<ActivatePane>,
    browse: AgentBrowserResolve,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let resolved = browse.command_pane(&request.payload.pane, &request.origin);
        if let Some(activation) = resolved.activation {
            activate.write(activation);
        }
        go_back.write(BrowserGoBackRequest {
            pane: resolved.pane,
        });
        responses.write(request.reply.ok());
    }
}

fn go_forward(
    mut requests: MessageReader<AgentRequestMessage<AgentBrowserGoForward>>,
    mut go_forward: MessageWriter<BrowserGoForwardRequest>,
    mut activate: MessageWriter<ActivatePane>,
    browse: AgentBrowserResolve,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let resolved = browse.command_pane(&request.payload.pane, &request.origin);
        if let Some(activation) = resolved.activation {
            activate.write(activation);
        }
        go_forward.write(BrowserGoForwardRequest {
            pane: resolved.pane,
        });
        responses.write(request.reply.ok());
    }
}

fn search_history(
    mut requests: MessageReader<AgentRequestMessage<AgentBrowserHistorySearch>>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        bevy::log::info!(
            "browser_history_search: query={:?} limit={}",
            request.payload.query,
            request.payload.limit
        );
        responses.write(request.reply.ok());
    }
}

fn open_in_new_stack(
    mut requests: MessageReader<AgentRequestMessage<AgentOpenInNewStack>>,
    mut open: MessageWriter<OpenInNewStackRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        open.write(OpenInNewStackRequest {
            url: request.payload.url.clone(),
        });
        responses.write(request.reply.ok());
    }
}
