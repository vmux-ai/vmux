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
                resume: None,
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
fn allow_always_records_policy_and_preserves_decision_scope() {
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
    assert!(policy.allows("run_shell"));
    assert_eq!(
        protocol_decision(ApprovalDecision::AllowAlways),
        ProtoDecision::AllowAlways
    );
}

#[test]
fn approval_grants_persist_by_agent_repository_and_tool() {
    let directory = tempfile::tempdir().unwrap();
    vmux_git::worktree::repository_init(directory.path()).unwrap();
    let path = directory.path().join("approvals.json");
    let mut store = AgentApprovalStore::load_from(path.clone());
    store.remember("codex-acp", directory.path(), "mcp__vmux__run");

    let loaded = AgentApprovalStore::load_from(path);

    assert!(
        loaded
            .policy_for("codex", directory.path())
            .allows("mcp.vmux.run")
    );
    assert!(
        !loaded
            .policy_for("claude", directory.path())
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
    let mut store = AgentApprovalStore::load_from(path.clone());
    store.remember("codex", directory.path(), "execute command");

    let loaded = AgentApprovalStore::load_from(path);

    assert!(
        loaded
            .policy_for("codex-acp", directory.path())
            .allows("execute_command")
    );
}
