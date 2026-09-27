use bevy::prelude::*;
use vmux_api::protocol::{
    AgentBrowserGoBack, AgentBrowserGoForward, AgentBrowserHistorySearch,
    AgentBrowserInstallExtension, AgentBrowserNavigate, AgentCommandResult, AgentOpenInNewStack,
    AgentRequestId,
};
use vmux_core::agent::{AgentCommandResponse, AgentReply, AgentRequestInput, CommandOrigin};

use super::agent_pane::AgentBrowserResolve;

pub(crate) struct AgentBrowserPlugin;

impl Plugin for AgentBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentRequestInput>()
            .add_message::<AgentCommandResponse>()
            .add_message::<AgentBrowserNavigateRequest>()
            .add_message::<AgentBrowserInstallExtensionRequest>()
            .add_message::<AgentBrowserGoBackRequest>()
            .add_message::<AgentBrowserGoForwardRequest>()
            .add_message::<AgentBrowserHistorySearchRequest>()
            .add_message::<AgentOpenInNewStackRequest>()
            .add_message::<vmux_extension::ExtensionInstallRequest>()
            .add_systems(Update, open_history)
            .add_systems(
                Update,
                (
                    route_browser_commands,
                    (
                        navigate,
                        install_extension,
                        go_back,
                        go_forward,
                        search_history,
                        open_in_new_stack,
                    ),
                )
                    .chain(),
            );
    }
}

#[derive(Message, Clone)]
struct AgentBrowserNavigateRequest {
    reply: AgentReply,
    origin: CommandOrigin,
    payload: AgentBrowserNavigate,
}

#[derive(Message, Clone)]
struct AgentBrowserInstallExtensionRequest {
    reply: AgentReply,
    payload: AgentBrowserInstallExtension,
}

#[derive(Message, Clone)]
struct AgentBrowserGoBackRequest {
    reply: AgentReply,
    origin: CommandOrigin,
    payload: AgentBrowserGoBack,
}

#[derive(Message, Clone)]
struct AgentBrowserGoForwardRequest {
    reply: AgentReply,
    origin: CommandOrigin,
    payload: AgentBrowserGoForward,
}

#[derive(Message, Clone)]
struct AgentBrowserHistorySearchRequest {
    reply: AgentReply,
    payload: AgentBrowserHistorySearch,
}

#[derive(Message, Clone)]
struct AgentOpenInNewStackRequest {
    reply: AgentReply,
    payload: AgentOpenInNewStack,
}

fn route_browser_commands(
    mut commands: MessageReader<AgentRequestInput>,
    mut navigate: MessageWriter<AgentBrowserNavigateRequest>,
    mut install_extension: MessageWriter<AgentBrowserInstallExtensionRequest>,
    mut go_back: MessageWriter<AgentBrowserGoBackRequest>,
    mut go_forward: MessageWriter<AgentBrowserGoForwardRequest>,
    mut search_history: MessageWriter<AgentBrowserHistorySearchRequest>,
    mut open_in_new_stack: MessageWriter<AgentOpenInNewStackRequest>,
) {
    for request in commands.read() {
        let reply = AgentReply::new(request.request_id);
        if let Ok(Some(payload)) = request.decode::<AgentBrowserNavigate>() {
            navigate.write(AgentBrowserNavigateRequest {
                reply,
                origin: request.origin.clone(),
                payload,
            });
        } else if let Ok(Some(payload)) = request.decode::<AgentBrowserInstallExtension>() {
            install_extension.write(AgentBrowserInstallExtensionRequest { reply, payload });
        } else if let Ok(Some(payload)) = request.decode::<AgentBrowserGoBack>() {
            go_back.write(AgentBrowserGoBackRequest {
                reply,
                origin: request.origin.clone(),
                payload,
            });
        } else if let Ok(Some(payload)) = request.decode::<AgentBrowserGoForward>() {
            go_forward.write(AgentBrowserGoForwardRequest {
                reply,
                origin: request.origin.clone(),
                payload,
            });
        } else if let Ok(Some(payload)) = request.decode::<AgentBrowserHistorySearch>() {
            search_history.write(AgentBrowserHistorySearchRequest { reply, payload });
        } else if let Ok(Some(payload)) = request.decode::<AgentOpenInNewStack>() {
            open_in_new_stack.write(AgentOpenInNewStackRequest { reply, payload });
        }
    }
}

fn open_history(
    mut intents: MessageReader<vmux_history::query::HistoryOpenIntent>,
    mut navigate: MessageWriter<AgentBrowserNavigateRequest>,
    mut open_in_new_stack: MessageWriter<AgentOpenInNewStackRequest>,
) {
    for intent in intents.read() {
        let reply = AgentReply::new(AgentRequestId::new());
        if intent.in_new_stack {
            open_in_new_stack.write(AgentOpenInNewStackRequest {
                reply,
                payload: AgentOpenInNewStack {
                    url: intent.url.clone(),
                },
            });
        } else {
            navigate.write(AgentBrowserNavigateRequest {
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
    mut requests: MessageReader<AgentBrowserNavigateRequest>,
    mut navigate: MessageWriter<vmux_layout::BrowserNavigateRequest>,
    mut open_beside: MessageWriter<vmux_layout::OpenBesideRequest>,
    mut activate: MessageWriter<vmux_layout::active_pane::ActivatePane>,
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
                open_beside.write(vmux_layout::OpenBesideRequest {
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
        navigate.write(vmux_layout::BrowserNavigateRequest {
            url: request.payload.url.clone(),
            pane,
            request_id: Some(request.reply.request_id.0),
            new_stack,
            profile,
        });
    }
}

fn install_extension(
    mut requests: MessageReader<AgentBrowserInstallExtensionRequest>,
    mut install: MessageWriter<vmux_extension::ExtensionInstallRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        install.write(vmux_extension::ExtensionInstallRequest {
            source: request.payload.source.clone(),
            requester: None,
        });
        responses.write(request.reply.ok());
    }
}

fn go_back(
    mut requests: MessageReader<AgentBrowserGoBackRequest>,
    mut go_back: MessageWriter<vmux_layout::BrowserGoBackRequest>,
    mut activate: MessageWriter<vmux_layout::active_pane::ActivatePane>,
    browse: AgentBrowserResolve,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let resolved = browse.command_pane(&request.payload.pane, &request.origin);
        if let Some(activation) = resolved.activation {
            activate.write(activation);
        }
        go_back.write(vmux_layout::BrowserGoBackRequest {
            pane: resolved.pane,
        });
        responses.write(request.reply.ok());
    }
}

fn go_forward(
    mut requests: MessageReader<AgentBrowserGoForwardRequest>,
    mut go_forward: MessageWriter<vmux_layout::BrowserGoForwardRequest>,
    mut activate: MessageWriter<vmux_layout::active_pane::ActivatePane>,
    browse: AgentBrowserResolve,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let resolved = browse.command_pane(&request.payload.pane, &request.origin);
        if let Some(activation) = resolved.activation {
            activate.write(activation);
        }
        go_forward.write(vmux_layout::BrowserGoForwardRequest {
            pane: resolved.pane,
        });
        responses.write(request.reply.ok());
    }
}

fn search_history(
    mut requests: MessageReader<AgentBrowserHistorySearchRequest>,
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
    mut requests: MessageReader<AgentOpenInNewStackRequest>,
    mut open: MessageWriter<vmux_layout::OpenInNewStackRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        open.write(vmux_layout::OpenInNewStackRequest {
            url: request.payload.url.clone(),
        });
        responses.write(request.reply.ok());
    }
}
