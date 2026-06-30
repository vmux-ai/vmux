//! GUI-side ACP agent integration: the [`AcpSession`] component identifies an ACP agent
//! pane, and [`AcpAgentPlugin`] forwards spawn/input/close to the daemon's
//! `AcpSessionManager`. The streamed updates are consumed by the shared
//! `consume_page_agent_stream` system (ACP reuses the Page stream messages).

use bevy::prelude::*;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::ClientMessage;
use vmux_setting::AppSettings;

use crate::components::{AgentApprovalPolicy, PendingUserInput};
use crate::events::{AgentApprovalReply, AgentApprovalRequest, ApprovalDecision};

/// Marks a stack entity as an ACP agent session. vmux is ACP-only, so this is the agent
/// identity (there is no `AgentVariant`/`AgentKind` for ACP).
#[derive(Component, Clone, Debug)]
pub struct AcpSession {
    pub agent_id: String,
    pub sid: String,
    pub cwd: std::path::PathBuf,
}

pub struct AcpAgentPlugin;

impl Plugin for AcpAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_acp_session_on_add, send_acp_input))
            .add_observer(close_acp_session_on_remove)
            .add_observer(auto_allow_acp_approval);
    }
}

/// ACP agents re-request permission every time, so "allow always" must be answered by the host:
/// if the tool name is already in this session's auto-policy, reply `Allow` without prompting.
fn auto_allow_acp_approval(
    trigger: On<AgentApprovalRequest>,
    policies: Query<&AgentApprovalPolicy, With<AcpSession>>,
    mut commands: Commands,
) {
    let request = trigger.event();
    let Ok(policy) = policies.get(request.session) else {
        return;
    };
    if policy.auto.contains(&request.name) {
        commands.trigger(AgentApprovalReply {
            session: request.session,
            call_id: request.call_id.clone(),
            decision: ApprovalDecision::Allow,
        });
    }
}

fn spawn_acp_session_on_add(
    q: Query<&AcpSession, Added<AcpSession>>,
    settings: Option<Res<AppSettings>>,
    service: Option<Res<ServiceClient>>,
) {
    let (Some(service), Some(settings)) = (service, settings) else {
        return;
    };
    for session in &q {
        let Some(cfg) = settings.agent.acp.iter().find(|c| c.id == session.agent_id) else {
            warn!(agent_id = %session.agent_id, "no agent.acp config for ACP session");
            continue;
        };
        service.0.send(ClientMessage::SpawnAcpAgent {
            sid: session.sid.clone(),
            agent_id: cfg.id.clone(),
            command: cfg.command.clone(),
            args: cfg.args.clone(),
            env: cfg.env.clone(),
            cwd: session.cwd.to_string_lossy().into_owned(),
        });
    }
}

fn send_acp_input(
    mut commands: Commands,
    q: Query<(Entity, &AcpSession, &PendingUserInput)>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for (entity, session, pending) in &q {
        service.0.send(ClientMessage::AgentInput {
            sid: session.sid.clone(),
            text: pending.0.clone(),
        });
        commands.entity(entity).remove::<PendingUserInput>();
    }
}

fn close_acp_session_on_remove(
    trigger: On<Remove, AcpSession>,
    sessions: Query<&AcpSession>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    let Ok(session) = sessions.get(trigger.event_target()) else {
        return;
    };
    service.0.send(ClientMessage::ClosePageAgent {
        sid: session.sid.clone(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_builds_and_runs_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(AcpAgentPlugin);
        app.world_mut().spawn(AcpSession {
            agent_id: "vibe-acp".to_string(),
            sid: "s1".to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
        });
        app.update();
    }
}
