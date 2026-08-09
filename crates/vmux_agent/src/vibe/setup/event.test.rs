use super::*;

#[test]
fn prereq_status_rkyv_roundtrip() {
    let v = AgentSetupPrereqStatus {
        needs_homebrew: true,
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
    let back = rkyv::from_bytes::<AgentSetupPrereqStatus, rkyv::rancor::Error>(&bytes).unwrap();
    assert!(back.needs_homebrew);
}

#[test]
fn result_rkyv_roundtrip() {
    let v = AgentSetupResult {
        agent: "codex".to_string(),
        ok: false,
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
    let back = rkyv::from_bytes::<AgentSetupResult, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(back.agent, "codex");
    assert!(!back.ok);
}
