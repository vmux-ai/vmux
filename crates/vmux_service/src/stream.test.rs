use super::*;

#[test]
fn serde_roundtrip_text_delta() {
    let e = StreamEvent::TextDelta("hi".into());
    let json = serde_json::to_string(&e).unwrap();
    let back: StreamEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(e, back);
}

#[test]
fn stop_reason_serializes_as_variant_name() {
    let json = serde_json::to_string(&StopReason::EndTurn).unwrap();
    assert_eq!(json, "\"EndTurn\"");
}
