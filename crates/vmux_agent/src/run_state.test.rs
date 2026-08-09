use super::*;

#[test]
fn default_is_idle() {
    assert!(matches!(AgentRunState::default(), AgentRunState::Idle));
}

#[test]
fn errored_holds_message() {
    let s = AgentRunState::Errored("oops".into());
    match s {
        AgentRunState::Errored(m) => assert_eq!(m, "oops"),
        _ => panic!("wrong variant"),
    }
}
