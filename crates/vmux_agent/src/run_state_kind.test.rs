use super::*;

#[test]
fn from_state_idle() {
    let s = AgentRunState::Idle;
    assert_eq!(AgentRunStateKind::from(&s), AgentRunStateKind::Idle);
}

#[test]
fn from_state_errored() {
    let s = AgentRunState::Errored("oops".into());
    assert_eq!(AgentRunStateKind::from(&s), AgentRunStateKind::Errored);
}

#[test]
fn last_run_state_kind_default_is_idle() {
    assert_eq!(LastRunStateKind::default().0, AgentRunStateKind::Idle);
}
