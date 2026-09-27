use bevy::prelude::*;
use vmux_api::protocol::{
    AgentBrowserHistorySearch, AgentBrowserHistoryStep, AgentBrowserInstallExtension,
    AgentBrowserNavigate, AgentCommandResult, AgentOpenInNewStack, AgentRequestId,
};
use vmux_core::agent::AgentCommandResponse;

use crate::host::browser_pane::AgentBrowserResolve;
use crate::host::event::CommandOrigin;

use super::{AgentReply, CommandSet};

pub(super) struct BrowserCommandPlugin;

impl Plugin for BrowserCommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentBrowserNavigateRequest>()
            .add_message::<AgentBrowserInstallExtensionRequest>()
            .add_message::<AgentBrowserGoBackRequest>()
            .add_message::<AgentBrowserGoForwardRequest>()
            .add_message::<AgentBrowserHistorySearchRequest>()
            .add_message::<AgentOpenInNewStackRequest>()
            .add_message::<vmux_extension::ExtensionInstallRequest>()
            .add_systems(Update, open_history.in_set(CommandSet::History))
            .add_systems(
                Update,
                (
                    navigate,
                    install_extension,
                    go_back,
                    go_forward,
                    search_history,
                    open_in_new_stack,
                )
                    .in_set(CommandSet::Commands),
            );
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

#[derive(Message, Clone)]
pub(super) struct AgentBrowserNavigateRequest {
    pub(super) reply: AgentReply,
    pub(super) origin: CommandOrigin,
    pub(super) payload: AgentBrowserNavigate,
}

#[derive(Message, Clone)]
pub(super) struct AgentBrowserInstallExtensionRequest {
    pub(super) reply: AgentReply,
    pub(super) payload: AgentBrowserInstallExtension,
}

#[derive(Message, Clone)]
pub(super) struct AgentBrowserGoBackRequest {
    pub(super) reply: AgentReply,
    pub(super) origin: CommandOrigin,
    pub(super) payload: AgentBrowserHistoryStep,
}

#[derive(Message, Clone)]
pub(super) struct AgentBrowserGoForwardRequest {
    pub(super) reply: AgentReply,
    pub(super) origin: CommandOrigin,
    pub(super) payload: AgentBrowserHistoryStep,
}

#[derive(Message, Clone)]
pub(super) struct AgentBrowserHistorySearchRequest {
    pub(super) reply: AgentReply,
    pub(super) payload: AgentBrowserHistorySearch,
}

#[derive(Message, Clone)]
pub(super) struct AgentOpenInNewStackRequest {
    pub(super) reply: AgentReply,
    pub(super) payload: AgentOpenInNewStack,
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
