use bevy::prelude::*;

use crate::components::{AgentApprovalPolicy, AgentMessages};
use crate::events::{AgentApprovalReply, ApprovalDecision};
use crate::message::Message;
use crate::run_state::AgentRunState;

pub fn handle_approval_reply(
    trigger: On<AgentApprovalReply>,
    mut q: Query<(
        &mut AgentRunState,
        &mut AgentMessages,
        &mut AgentApprovalPolicy,
    )>,
) {
    let reply = trigger.event();
    let Ok((mut state, mut messages, mut policy)) = q.get_mut(reply.session) else {
        return;
    };
    let matches_call = matches!(
        &*state,
        AgentRunState::AwaitingApproval { call_id, .. } if call_id == &reply.call_id
    );
    if !matches_call {
        return;
    }
    match reply.decision {
        ApprovalDecision::Allow | ApprovalDecision::AllowAlways => {
            let AgentRunState::AwaitingApproval {
                call_id,
                name,
                args,
                ..
            } = std::mem::replace(&mut *state, AgentRunState::Idle)
            else {
                return;
            };
            if reply.decision == ApprovalDecision::AllowAlways {
                policy.auto.insert(name.clone());
            }
            let task = crate::tool_dispatch::start_tool_task(call_id.clone(), name, args);
            *state = AgentRunState::RunningTool { call_id, task };
        }
        ApprovalDecision::Deny => {
            messages.0.push(Message::ToolResult {
                call_id: reply.call_id.clone(),
                content: "denied by user".into(),
                is_error: true,
            });
            *state = AgentRunState::Idle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_observer(handle_approval_reply);
        app
    }

    #[test]
    fn deny_appends_error_result_and_idles() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                AgentMessages::default(),
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

        let msgs = app.world().get::<AgentMessages>(entity).unwrap();
        assert_eq!(msgs.0.len(), 1);
        match &msgs.0[0] {
            Message::ToolResult { is_error, .. } => assert!(is_error),
            _ => panic!("expected tool result"),
        }
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Idle)
        ));
    }

    #[test]
    fn allow_always_records_in_policy_and_runs_tool() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                AgentMessages::default(),
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
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::RunningTool { .. })
        ));
    }
}
