use bevy::prelude::*;
use vmux_api::protocol::AgentCommandResult;
use vmux_core::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
    CommandOrigin,
};

use super::tool::AgentInvokeCommand;
use crate::definition::{
    CommandDefinition, CommandInvocation, CommandRuntimePlugin, DispatchCommandInvocations,
    WriteCommandRequests,
};
use crate::page_key::KeyPlugin;
use crate::snapshot::UiStatePlugin;
use crate::surface::SurfacePlugin;
use vmux_core::team::{Agent, Profile, User};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_core::host::manifest::FeatureManifestPlugin::<
            crate::Feature,
        >::new(crate::FEATURE_MANIFEST));
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_agent_request::<AgentInvokeCommand>()
            .add_plugins((
                KeyPlugin,
                UiStatePlugin,
                SurfacePlugin,
                crate::CommandToolPlugin,
            ))
            .add_systems(Update, invoke_command.after(AgentRequestRouteSet))
            .add_systems(
                Update,
                log_command_invocations
                    .after(WriteCommandRequests)
                    .before(DispatchCommandInvocations),
            )
            .add_systems(Last, keep_frames_coming);
    }
}

fn invoke_command(
    mut requests: MessageReader<AgentRequestMessage<AgentInvokeCommand>>,
    definitions: Query<&CommandDefinition>,
    mut invocations: MessageWriter<CommandInvocation>,
    agents: Query<(Entity, &Agent, Option<&vmux_core::ProcessId>)>,
    user: Query<Entity, With<User>>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let args = match vmux_core::JsonArguments::try_from(&request.payload.args) {
            Ok(args) => args.0,
            Err(message) => {
                responses.write(request.reply.response(AgentCommandResult::Error(message)));
                continue;
            }
        };
        let caller = match &request.origin {
            CommandOrigin::Agent {
                anchor: Some(pid), ..
            } => agents
                .iter()
                .find(|(_, _, process)| process.as_ref().is_some_and(|process| *process == pid))
                .map(|(entity, _, _)| entity),
            CommandOrigin::Agent { sid: Some(sid), .. } if !sid.is_empty() => agents
                .iter()
                .find(|(_, agent, _)| agent.sid.as_str() == sid.as_str())
                .map(|(entity, _, _)| entity),
            CommandOrigin::User => user.single().ok(),
            _ => None,
        }
        .unwrap_or(Entity::PLACEHOLDER);
        let Some(definition) = definitions
            .iter()
            .find(|definition| definition.matches(&request.payload.id))
        else {
            responses.write(request.reply.response(AgentCommandResult::Error(format!(
                "unknown app command: {}",
                request.payload.id
            ))));
            continue;
        };
        let result = if request.origin.is_agent() {
            definition.agent_invocation(caller, args)
        } else {
            definition.user_invocation(caller, args)
        };
        match result {
            Ok(invocation) => {
                invocations.write(invocation);
                responses.write(request.reply.ok());
            }
            Err(message) => {
                responses.write(request.reply.response(AgentCommandResult::Error(message)));
            }
        }
    }
}

const COMMAND_SETTLE_WINDOW: std::time::Duration = std::time::Duration::from_millis(150);

fn keep_frames_coming(
    mut settle: Local<Option<std::time::Instant>>,
    mut reader: MessageReader<CommandInvocation>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    if reader.read().count() > 0 {
        *settle = Some(std::time::Instant::now());
    }
    let Some(since) = *settle else {
        return;
    };
    if since.elapsed() >= COMMAND_SETTLE_WINDOW {
        *settle = None;
        return;
    }
    let Some(proxy) = proxy else {
        return;
    };
    let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
}

fn log_command_invocations(
    mut reader: MessageReader<CommandInvocation>,
    profiles: Query<(&Profile, Has<User>)>,
) {
    for invocation in reader.read() {
        let who = profiles
            .get(invocation.caller)
            .map(|(p, is_user)| format!("{} ({})", p.name, if is_user { "user" } else { "agent" }))
            .unwrap_or_else(|_| "unknown".to_string());
        info!(
            target: "vmux_command::invocation",
            caller = %who,
            id = %invocation.id,
            "CommandInvocation"
        );
    }
}
