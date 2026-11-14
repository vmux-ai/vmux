use bevy::ecs::system::Commands;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::browser::{AgentOpenBrowser, OpenExternalBrowserRequest, Url};
use vmux_api::open_target::PaneDirection;
use vmux_api::protocol::{AgentCommandResult, AgentQueryResult, AgentRequestId, ClientMessage};
use vmux_ecs::agent::{
    AgentCommandResponse, AgentReply, AgentRequestAppExt, AgentRequestMessage,
    AgentRequestRouteSet, CommandOrigin,
};
use vmux_ecs::browser::{
    BrowserNavigationSnapshotResponse, BrowserScrollRequest, BrowserScrollResponse,
    BrowserSnapshotRequest, BrowserSnapshotResponse,
};
use vmux_ecs::profile::Projects;
use vmux_ecs::service::ServiceRequest;
use vmux_ecs::{PageOpenRequest, PageOpenTarget};
use vmux_flex::prelude::ComputedNode;
use vmux_history::{HistoryOpenIntent, LastActivatedAt};
use vmux_layout::active_pane::ActivatePane;
use vmux_layout::stack::{FocusedStack, Stack};
use vmux_layout::{
    BrowserGoBackRequest, BrowserGoForwardRequest, BrowserNavigateRequest, OpenBesideRequest,
    OpenInNewStackRequest,
};
use vmux_tool::AgentWorkingDirectory;
use vmux_tool::{ToolQueryAppExt, ToolQueryMessage, ToolQueryRouteSet};

use super::agent_pane::AgentBrowserResolve;

#[vmux_api::agent]
pub(crate) struct AgentBrowserNavigate {
    pub url: String,
    pub pane: Option<String>,
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
    pub anchor: Option<vmux_ecs::ProcessId>,
}

#[vmux_api::agent(Eq)]
pub(crate) struct AgentBrowserScroll {
    pub pane: Option<String>,
    pub to: Option<String>,
    pub delta: Option<i32>,
    pub anchor: Option<vmux_ecs::ProcessId>,
}

#[vmux_api::agent]
pub(crate) struct AgentOpenInNewStack {
    pub url: String,
}

pub(crate) struct AgentBrowserPlugin;

impl Plugin for AgentBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_agent_request::<AgentBrowserNavigate>()
            .add_agent_request::<AgentOpenBrowser>()
            .add_message::<PageOpenRequest>()
            .add_message::<OpenBesideRequest>()
            .add_plugins(UiEventPlugin::<(OpenExternalBrowserRequest,)>::default())
            .add_observer(open_external)
            .add_agent_request::<AgentBrowserGoBack>()
            .add_agent_request::<AgentBrowserGoForward>()
            .add_agent_request::<AgentBrowserHistorySearch>()
            .add_agent_request::<AgentOpenInNewStack>()
            .add_tool_query::<AgentBrowserSnapshot>()
            .add_tool_query::<AgentBrowserScroll>()
            .add_tool_query::<AgentWorkingDirectory>()
            .add_message::<ServiceRequest>()
            .add_message::<BrowserSnapshotRequest>()
            .add_message::<BrowserSnapshotResponse>()
            .add_message::<BrowserScrollRequest>()
            .add_message::<BrowserScrollResponse>()
            .add_message::<BrowserNavigationSnapshotResponse>()
            .add_message::<ActivatePane>()
            .add_systems(Update, open_history)
            .add_systems(
                Update,
                (
                    (
                        route_snapshot_queries,
                        route_scroll_queries,
                        answer_working_directory,
                    )
                        .after(ToolQueryRouteSet),
                    forward_snapshot_responses,
                    forward_scroll_responses,
                    forward_snapshots,
                ),
            )
            .add_systems(
                Update,
                ((
                    navigate,
                    go_back,
                    go_forward,
                    search_history,
                    open_in_new_stack,
                    open,
                )
                    .after(AgentRequestRouteSet),)
                    .chain(),
            );
    }
}

fn route_snapshot_queries(
    mut queries: MessageReader<ToolQueryMessage<AgentBrowserSnapshot>>,
    mut snapshots: MessageWriter<BrowserSnapshotRequest>,
    mut activate: MessageWriter<ActivatePane>,
    browse: AgentBrowserResolve,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in queries.read() {
        let Some(resolved) = browse.resolve_pane(&request.payload.pane, &request.payload.anchor)
        else {
            service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
                AgentQueryResult::text(
                    request.request_id,
                    Err("browser snapshot target is unavailable".to_string()),
                ),
            )));
            continue;
        };
        if let Some(request) = resolved.activation {
            activate.write(request);
        }
        snapshots.write(BrowserSnapshotRequest {
            request_id: request.request_id.0,
            pane: resolved.pane,
            webview: None,
        });
    }
}

fn route_scroll_queries(
    mut queries: MessageReader<ToolQueryMessage<AgentBrowserScroll>>,
    mut scrolls: MessageWriter<BrowserScrollRequest>,
    mut activate: MessageWriter<ActivatePane>,
    browse: AgentBrowserResolve,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in queries.read() {
        let Some(resolved) = browse.resolve_pane(&request.payload.pane, &request.payload.anchor)
        else {
            service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
                AgentQueryResult::text(
                    request.request_id,
                    Err("browser scroll target is unavailable".to_string()),
                ),
            )));
            continue;
        };
        if let Some(request) = resolved.activation {
            activate.write(request);
        }
        scrolls.write(BrowserScrollRequest {
            request_id: request.request_id.0,
            pane: resolved.pane,
            to: request.payload.to.clone(),
            delta: request.payload.delta,
        });
    }
}

fn answer_working_directory(
    mut requests: MessageReader<ToolQueryMessage<AgentWorkingDirectory>>,
    browse: AgentBrowserResolve,
    projects: Projects,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let result = if browse.agent_pane(request.payload.anchor).is_none() {
            Err("agent pane not found".to_string())
        } else if let Some(path) = browse.working_directory(request.payload.anchor) {
            Ok(path.to_string_lossy().into_owned())
        } else {
            projects
                .path()
                .map(|path| path.to_string_lossy().into_owned())
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::text(request.request_id, result),
        )));
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

fn forward_snapshots(
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

fn go_back(
    mut requests: MessageReader<AgentRequestMessage<AgentBrowserGoBack>>,
    mut go_back: MessageWriter<BrowserGoBackRequest>,
    mut activate: MessageWriter<ActivatePane>,
    browse: AgentBrowserResolve,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let Some(resolved) = browse.command_pane(&request.payload.pane, &request.origin) else {
            responses.write(request.reply.response(AgentCommandResult::Error(
                "browser_go_back: target pane is unavailable".to_string(),
            )));
            continue;
        };
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
        let Some(resolved) = browse.command_pane(&request.payload.pane, &request.origin) else {
            responses.write(request.reply.response(AgentCommandResult::Error(
                "browser_go_forward: target pane is unavailable".to_string(),
            )));
            continue;
        };
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

#[derive(bevy::ecs::system::SystemParam)]
struct OpenResolver<'w, 's> {
    focused: FocusedStack<'w, 's>,
    stacks: Query<
        'w,
        's,
        (
            Entity,
            &'static vmux_ecs::PageMetadata,
            Option<&'static vmux_ecs::EntityTarget<vmux_session::Session>>,
        ),
        With<Stack>,
    >,
    sessions: Query<'w, 's, &'static vmux_session::SessionId>,
    parents: Query<'w, 's, &'static ChildOf>,
    nodes: Query<'w, 's, &'static ComputedNode>,
}

impl OpenResolver<'_, '_> {
    fn resolve(&self, source: Option<&str>) -> Result<(Entity, bool), String> {
        let stack = if let Some(source) = source {
            let mut route =
                url::Url::parse("vmux://sessions/").expect("static Session route is valid");
            route
                .path_segments_mut()
                .expect("Session route supports path segments")
                .push(source);
            let url = route.as_str();
            self.stacks.iter().find_map(|(stack, metadata, target)| {
                let matches = match target {
                    Some(target) => self
                        .sessions
                        .get(target.entity())
                        .is_ok_and(|id| id.0 == source),
                    None => metadata.url == url,
                };
                matches.then_some(stack)
            })
        } else {
            self.focused.stack
        }
        .ok_or("browser source conversation is unavailable")?;
        let pane = self
            .parents
            .get(stack)
            .map_err(|_| "browser source pane is unavailable")?
            .parent();
        let width = self
            .nodes
            .get(pane)
            .map(|node| node.size.x * node.inverse_scale_factor)
            .unwrap_or_default();
        Ok((pane, width.is_finite() && width >= 1120.0))
    }
}

fn open(
    mut requests: MessageReader<AgentRequestMessage<AgentOpenBrowser>>,
    target: OpenResolver,
    mut responses: MessageWriter<AgentCommandResponse>,
    mut beside: MessageWriter<OpenBesideRequest>,
    mut pages: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if let Err(error) = Url::parse(&request.payload.url) {
            responses.write(request.reply.response(AgentCommandResult::Error(error)));
            continue;
        }
        match target.resolve(request.payload.source.as_deref()) {
            Ok((pane, wide)) => {
                if wide {
                    beside.write(OpenBesideRequest {
                        pane,
                        direction: Some(PaneDirection::Right),
                        url: request.payload.url.clone(),
                        request_id: request.reply.request_id.0,
                        focus: true,
                    });
                } else {
                    let stack = commands
                        .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id();
                    pages.write(PageOpenRequest {
                        target: PageOpenTarget::Stack(stack),
                        url: request.payload.url.clone(),
                        request_id: None,
                    });
                    commands.trigger(vmux_ecs::ActivateRequest { entity: stack });
                }
                responses.write(request.reply.ok());
            }
            Err(error) => {
                responses.write(request.reply.response(AgentCommandResult::Error(error)));
            }
        }
    }
}

struct External(Url);

impl External {
    fn open(self) -> std::io::Result<()> {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let mut child = std::process::Command::new(program)
            .arg(self.0.as_str())
            .spawn()?;
        std::thread::Builder::new()
            .name("external-browser".into())
            .spawn(move || {
                let _ = child.wait();
            })?;
        Ok(())
    }
}

fn open_external(trigger: On<UiInput<OpenExternalBrowserRequest>>) {
    let Ok(url) = Url::parse(&trigger.event().payload.url) else {
        return;
    };
    if let Err(error) = External(url).open() {
        bevy::log::warn!("external browser could not be opened: {error}");
    }
}

#[cfg(test)]
mod opening_tests {
    use super::*;
    use vmux_layout::active_pane::ActiveStack;

    #[test]
    fn account_pages_preserve_the_source_and_choose_layout_from_logical_width() {
        for (physical_width, inverse_scale, wide) in [
            (2400.0, 0.5, true),
            (1200.0, 0.5, false),
            (600.0, 1.0, false),
        ] {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_ecs::PrimitivesPlugin))
                .add_message::<AgentRequestMessage<AgentOpenBrowser>>()
                .add_message::<AgentCommandResponse>()
                .add_message::<OpenBesideRequest>()
                .add_message::<PageOpenRequest>()
                .add_systems(Update, open);
            let pane = app
                .world_mut()
                .spawn(ComputedNode {
                    size: Vec2::new(physical_width, 900.0),
                    inverse_scale_factor: inverse_scale,
                    ..Default::default()
                })
                .id();
            let source = "sign-in/source?draft#1";
            let source_url = "vmux://start/onboarding/agent";
            let session = app
                .world_mut()
                .spawn((
                    vmux_session::Session,
                    vmux_session::SessionId(source.into()),
                ))
                .id();
            let stack = app.world_mut().spawn((Stack::bundle(), ChildOf(pane))).id();
            app.world_mut().entity_mut(stack).insert((
                vmux_ecs::EntityTarget::<vmux_session::Session>::new(session),
                vmux_ecs::PageMetadata {
                    url: source_url.into(),
                    ..Default::default()
                },
            ));
            let other_pane = app.world_mut().spawn_empty().id();
            let other_stack = app
                .world_mut()
                .spawn((Stack::bundle(), ChildOf(other_pane)))
                .id();
            app.world_mut().spawn(
                ActiveStack {
                    pane: Some(other_pane),
                    stack: Some(other_stack),
                    ..Default::default()
                }
                .local_bundle(),
            );
            let url = "https://example.test/auth?state=%2B%2F&challenge=a+b";
            app.world_mut().write_message(AgentRequestMessage {
                reply: AgentReply::new(AgentRequestId::new()),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: None,
                },
                payload: AgentOpenBrowser {
                    url: url.into(),
                    source: Some(source.into()),
                },
            });
            app.update();
            assert_eq!(
                app.world()
                    .get::<vmux_ecs::PageMetadata>(stack)
                    .unwrap()
                    .url,
                source_url
            );
            let beside = app
                .world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .drain()
                .collect::<Vec<_>>();
            let pages = app
                .world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .collect::<Vec<_>>();
            if wide {
                assert_eq!(beside.len(), 1);
                assert_eq!(beside[0].pane, pane);
                assert_eq!(beside[0].direction, Some(PaneDirection::Right));
                assert_eq!(beside[0].url, url);
                assert!(pages.is_empty());
            } else {
                assert!(beside.is_empty());
                assert_eq!(pages.len(), 1);
                assert_eq!(pages[0].url, url);
                let PageOpenTarget::Stack(opened) = pages[0].target else {
                    panic!("a narrow pane must open a new Stack");
                };
                assert_ne!(opened, stack);
                assert_eq!(app.world().get::<ChildOf>(opened).unwrap().parent(), pane);
                assert!(app.world().get::<LastActivatedAt>(pane).is_some());
            }
        }
    }
}
