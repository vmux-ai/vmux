use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
#[cfg(test)]
use std::path::PathBuf;

use crate::host::event::{AgentApprovalReply, ApprovalDecision};
use vmux_api::protocol::{ClientMessage, SharedMessage};
use vmux_ecs::service::ServiceRequest;
use vmux_ecs::{Cwd, EntityTarget};
use vmux_session::event::ChatApproval;
use vmux_session::{AgentId, ApprovalPolicy, RunState, Session, SessionId};

use super::approval_driver::ApprovalDriver;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ApprovalSyncSet;

pub(super) fn add(app: &mut App) {
    app.add_message::<ServiceRequest>()
        .add_systems(Startup, spawn)
        .add_observer(receive)
        .add_observer(reply)
        .add_systems(Update, policy.in_set(ApprovalSyncSet));
}

fn receive(
    trigger: On<UiInput<ChatApproval>>,
    child_of: Query<&ChildOf>,
    targets: Query<&EntityTarget<Session>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let payload = &trigger.event().payload;
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    let Ok(target) = targets.get(parent.parent()) else {
        return;
    };
    commands.trigger(AgentApprovalReply {
        session: target.entity(),
        call_id: payload.call_id.clone(),
        decision: payload.decision,
    });
}

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("Agent approval store"), ApprovalDriver::load()));
}

fn policy(
    store: Single<&ApprovalDriver>,
    mut sessions: Query<
        (&AgentId, &Cwd, &mut ApprovalPolicy),
        (With<Session>, Or<(Changed<AgentId>, Changed<Cwd>)>),
    >,
) {
    for (agent_id, cwd, mut policy) in &mut sessions {
        *policy = store.policy_for(&agent_id.0, &cwd.0);
    }
}

#[allow(clippy::type_complexity)]
fn reply(
    trigger: On<AgentApprovalReply>,
    mut q: Query<
        (
            &mut RunState,
            &mut ApprovalPolicy,
            &SessionId,
            &AgentId,
            &Cwd,
        ),
        With<Session>,
    >,
    mut service_requests: MessageWriter<ServiceRequest>,
    mut store: Option<Single<&mut ApprovalDriver>>,
) {
    let reply = trigger.event();
    let Ok((mut state, mut policy, session_id, agent_id, cwd)) = q.get_mut(reply.session) else {
        return;
    };
    let matches_call = matches!(
        &*state,
        RunState::AwaitingApproval { call_id, .. } if call_id == &reply.call_id
    );
    if !matches_call {
        return;
    }
    if reply.decision == ApprovalDecision::AllowAlways
        && let RunState::AwaitingApproval { name, .. } = &*state
    {
        policy.allow(name);
        if let Some(store) = store.as_deref_mut() {
            store.remember(&agent_id.0, &cwd.0, name);
        }
    }
    service_requests.write(ServiceRequest(ClientMessage::Shared(
        SharedMessage::AgentApprove {
            sid: session_id.0.clone(),
            call_id: reply.call_id.clone(),
            decision: reply.decision,
        },
    )));
    *state = RunState::Streaming;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vmux_api::protocol::ProcessId;
    use vmux_ecs::ProcessAnchor;

    struct TestSession;

    impl TestSession {
        fn bundle(agent: &str) -> impl Bundle {
            (
                Session,
                SessionId("s".into()),
                AgentId(agent.into()),
                Cwd(PathBuf::from("/tmp")),
                ProcessAnchor(ProcessId::new()),
            )
        }
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        add(&mut app);
        app.update();
        let path =
            std::env::temp_dir().join(format!("vmux-agent-approval-{}.json", uuid::Uuid::new_v4()));
        let store = ApprovalDriver::load_from(path);
        let entity = {
            let world = app.world_mut();
            let mut stores = world.query_filtered::<Entity, With<ApprovalDriver>>();
            stores.single(world).unwrap()
        };
        app.world_mut().entity_mut(entity).insert(store);
        app
    }

    #[test]
    fn deny_sets_streaming() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                TestSession::bundle("anthropic"),
                ApprovalPolicy::default(),
                RunState::AwaitingApproval {
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
            app.world().get::<RunState>(entity),
            Some(RunState::Streaming)
        ));
    }

    #[test]
    fn acp_session_reply_sets_streaming() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                TestSession::bundle("vibe-acp"),
                ApprovalPolicy::default(),
                RunState::AwaitingApproval {
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
            app.world().get::<RunState>(entity),
            Some(RunState::Streaming)
        ));
    }

    #[test]
    fn allow_always_records_policy_and_preserves_decision_scope() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                TestSession::bundle("anthropic"),
                ApprovalPolicy::default(),
                RunState::AwaitingApproval {
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
        let policy = app.world().get::<ApprovalPolicy>(entity).unwrap();
        assert!(policy.allows("run_shell"));
    }

    #[test]
    fn approval_grants_persist_by_agent_repository_and_tool() {
        let directory = tempfile::tempdir().unwrap();
        vmux_git::worktree::CheckoutInfo::initialize(directory.path()).unwrap();
        let path = directory.path().join("approvals.json");
        let mut store = ApprovalDriver::load_from(path.clone());
        store.remember("codex-acp", directory.path(), "mcp__vmux__run");

        let loaded = ApprovalDriver::load_from(path);

        assert!(
            loaded
                .policy_for("codex-acp", directory.path())
                .allows("mcp.vmux.run")
        );
        assert!(
            !loaded
                .policy_for("codex", directory.path())
                .allows("mcp.vmux.run")
        );
        assert!(
            !loaded
                .policy_for("codex", directory.path())
                .allows("mcp.vmux.open_file")
        );
    }

    #[test]
    fn approval_grants_persist_by_working_directory_outside_repository() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("approvals.json");
        let mut store = ApprovalDriver::load_from(path.clone());
        store.remember("codex", directory.path(), "execute command");

        let loaded = ApprovalDriver::load_from(path);

        assert!(
            loaded
                .policy_for("codex", directory.path())
                .allows("execute_command")
        );
        assert!(
            !loaded
                .policy_for("codex-acp", directory.path())
                .allows("execute_command")
        );
    }
}
