use super::*;

#[test]
fn echo_stream_returns_text_then_stop() {
    let events = synthetic_echo_stream("hi");
    assert_eq!(events.len(), 2);
    match &events[0] {
        StreamEvent::TextDelta(t) => assert_eq!(t, "echo: hi"),
        _ => panic!("expected text delta"),
    }
    assert!(matches!(events[1], StreamEvent::StopTurn { .. }));
}
