use bevy::prelude::*;

use crate::client::acp::AcpSession;
use crate::components::{AgentApprovalPolicy, AgentSession};
use crate::events::{AgentApprovalReply, ApprovalDecision};
use crate::run_state::AgentRunState;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{ApprovalDecision as ProtoDecision, ClientMessage};

#[allow(clippy::type_complexity)]
pub fn handle_approval_reply(
    trigger: On<AgentApprovalReply>,
    mut q: Query<(
        &mut AgentRunState,
        &mut AgentApprovalPolicy,
        Option<&AgentSession>,
        Option<&AcpSession>,
    )>,
    service: Option<Res<ServiceClient>>,
) {
    let reply = trigger.event();
    let Ok((mut state, mut policy, page, acp)) = q.get_mut(reply.session) else {
        return;
    };
    let Some(sid) = page
        .map(|s| s.sid.clone())
        .or_else(|| acp.map(|s| s.sid.clone()))
    else {
        return;
    };
    let matches_call = matches!(
        &*state,
        AgentRunState::AwaitingApproval { call_id, .. } if call_id == &reply.call_id
    );
    if !matches_call {
        return;
    }
    if reply.decision == ApprovalDecision::AllowAlways
        && let AgentRunState::AwaitingApproval { name, .. } = &*state
    {
        policy.auto.insert(name.clone());
    }
    let decision = match reply.decision {
        ApprovalDecision::Allow | ApprovalDecision::AllowAlways => ProtoDecision::Allow,
        ApprovalDecision::Deny => ProtoDecision::Deny,
    };
    if let Some(service) = service.as_ref() {
        service.0.send(ClientMessage::AgentApprove {
            sid,
            call_id: reply.call_id.clone(),
            decision,
        });
    }
    *state = AgentRunState::Streaming;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentKind, AgentVariant};
    use serde_json::json;

    fn session() -> AgentSession {
        AgentSession {
            kind: AgentKind::Vibe,
            variant: AgentVariant::Page,
            sid: "s".into(),
            provider: "anthropic".into(),
            model: "m".into(),
        }
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_observer(handle_approval_reply);
        app
    }

    #[test]
    fn deny_sets_streaming() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                session(),
                AgentApprovalPolicy::default(),
                AgentRunState::AwaitingApproval {
                    call_id: "abc".into(),
                    name: "run_shell".into(),
                    args: json!({}),
                },
            ))
            .id();
        app.world_mut().trigger(AgentApprovalReply {
            session: entity,
            call_id: "abc".into(),
            decision: ApprovalDecision::Deny,
        });
        app.update();
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Streaming)
        ));
    }

    #[test]
    fn acp_session_reply_sets_streaming() {
        use crate::client::acp::AcpSession;
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "vibe-acp".into(),
                    sid: "s".into(),
                    cwd: std::path::PathBuf::from("/tmp"),
                    anchor: vmux_core::ProcessId::new(),
                },
                AgentApprovalPolicy::default(),
                AgentRunState::AwaitingApproval {
                    call_id: "abc".into(),
                    name: "edit".into(),
                    args: json!({}),
                },
            ))
            .id();
        app.world_mut().trigger(AgentApprovalReply {
            session: entity,
            call_id: "abc".into(),
            decision: ApprovalDecision::Allow,
        });
        app.update();
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Streaming)
        ));
    }

    #[test]
    fn allow_always_records_in_policy() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                session(),
                AgentApprovalPolicy::default(),
                AgentRunState::AwaitingApproval {
                    call_id: "abc".into(),
                    name: "run_shell".into(),
                    args: json!({}),
                },
            ))
            .id();
        app.world_mut().trigger(AgentApprovalReply {
            session: entity,
            call_id: "abc".into(),
            decision: ApprovalDecision::AllowAlways,
        });
        app.update();
        let policy = app.world().get::<AgentApprovalPolicy>(entity).unwrap();
        assert!(policy.auto.contains("run_shell"));
    }
}
