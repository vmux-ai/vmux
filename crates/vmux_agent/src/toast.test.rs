use super::*;

#[test]
fn rkyv_roundtrip() {
    let t = AgentToast {
        session_sid: "abc".into(),
        level: ToastLevel::Error,
        message: "boom".into(),
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&t).expect("ser");
    let back: AgentToast = rkyv::from_bytes::<AgentToast, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(back.session_sid, "abc");
    assert_eq!(back.level, ToastLevel::Error);
    assert!(back.message.contains("boom"));
}
