use super::*;

fn test_broker() -> AgentBroker {
    let (agent_tx, _) = broadcast::channel::<ServiceMessage>(16);
    AgentBroker::new(
        agent_tx,
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(Mutex::new(HashMap::new())),
    )
}

#[test]
fn resolve_provider_known_and_unknown() {
    assert!(resolve_provider("anthropic").is_some());
    assert!(resolve_provider("openai").is_some());
    assert!(resolve_provider("mistral").is_some());
    assert!(resolve_provider("nope").is_none());
}

#[tokio::test]
async fn spawn_then_snapshot_empty_then_close() {
    let mut mgr = AgentSessionManager::default();
    mgr.spawn(
        "s".to_string(),
        "anthropic",
        "m".to_string(),
        "/tmp/project".to_string(),
        Vec::new(),
        HashSet::new(),
        test_broker(),
    )
    .unwrap();
    match mgr.snapshot("s").await {
        Some(ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot {
            messages_json, ..
        })) => {
            assert_eq!(messages_json, "[]");
        }
        other => panic!("expected snapshot, got {other:?}"),
    }
    mgr.close("s");
    assert!(mgr.snapshot("s").await.is_none());
}

#[tokio::test]
async fn spawn_is_idempotent_per_sid() {
    let mut mgr = AgentSessionManager::default();
    mgr.spawn(
        "s".into(),
        "openai",
        "m".into(),
        "/tmp/project".into(),
        Vec::new(),
        HashSet::new(),
        test_broker(),
    )
    .unwrap();
    mgr.spawn(
        "s".into(),
        "openai",
        "m".into(),
        "/tmp/project".into(),
        Vec::new(),
        HashSet::new(),
        test_broker(),
    )
    .unwrap();
    assert!(mgr.snapshot("s").await.is_some());
    mgr.close("s");
}

#[tokio::test]
async fn unknown_provider_is_rejected() {
    let mut mgr = AgentSessionManager::default();
    let err = mgr
        .spawn(
            "s".into(),
            "bogus",
            "m".into(),
            "/tmp/project".into(),
            Vec::new(),
            HashSet::new(),
            test_broker(),
        )
        .unwrap_err();
    assert!(err.contains("bogus"));
}

#[tokio::test]
async fn remote_summary_exposes_active_session() {
    let mut mgr = AgentSessionManager::default();
    mgr.spawn(
        "s".into(),
        "openai",
        "gpt-test".into(),
        "/tmp/project".into(),
        Vec::new(),
        HashSet::new(),
        test_broker(),
    )
    .unwrap();

    let session = mgr.remote_session("s").unwrap();

    assert_eq!(session.name, "openai");
    assert_eq!(session.model.as_deref(), Some("gpt-test"));
    assert_eq!(session.cwd, "/tmp/project");
    mgr.close("s");
}

#[tokio::test]
async fn approval_resolution_is_broadcast_immediately() {
    let mut mgr = AgentSessionManager::default();
    mgr.spawn(
        "s".into(),
        "openai",
        "gpt-test".into(),
        "/tmp/project".into(),
        Vec::new(),
        HashSet::new(),
        test_broker(),
    )
    .unwrap();
    let handle = mgr.sessions.get("s").unwrap();
    *handle.approval.lock().unwrap() = Some(RemoteApproval {
        call_id: "call-1".into(),
        name: "run".into(),
        args_json: "{}".into(),
    });
    let mut receiver = mgr.subscribe("s").unwrap();

    mgr.input(
        "s",
        SessionInput::Approve {
            call_id: "call-1".into(),
            decision: ApprovalDecision::Allow,
        },
    );

    assert!(matches!(
        receiver.try_recv(),
        Ok(ServiceMessage::Shared(SharedEvent::AgentApprovalResolved { sid, call_id }))
            if sid == "s" && call_id == "call-1"
    ));
    assert!(mgr.remote_session("s").unwrap().approval.is_none());
    mgr.close("s");
}
