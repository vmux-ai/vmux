use super::*;

#[test]
fn agent_session_to_entity_starts_empty() {
    let map = AgentSessionToEntity::default();
    assert!(map.0.is_empty());
}

#[test]
fn pending_session_carries_cwd_and_kind() {
    let pending = PendingAgentSession {
        kind: AgentKind::Claude,
        spawn_time: SystemTime::UNIX_EPOCH,
        cwd: PathBuf::from("/tmp/x"),
    };
    assert_eq!(pending.kind, AgentKind::Claude);
    assert_eq!(pending.cwd, PathBuf::from("/tmp/x"));
}
