use super::*;

const TEXT: &str = include_str!("../../tests/fixtures/anthropic/text.sse");
const TOOLS: &str = include_str!("../../tests/fixtures/anthropic/tools.sse");

fn frames(raw: &str) -> Vec<&str> {
    raw.split("\n\n").filter(|s| !s.trim().is_empty()).collect()
}

#[test]
fn parses_text_block_into_deltas_then_end_turn() {
    let events: Vec<StreamEvent> = frames(TEXT)
        .into_iter()
        .filter_map(parse_messages_sse)
        .collect();
    assert!(events.contains(&StreamEvent::TextDelta("hello".into())));
    assert!(events.contains(&StreamEvent::TextDelta(" world".into())));
    assert!(events.iter().any(|e| matches!(
        e,
        StreamEvent::StopTurn {
            reason: StopReason::EndTurn
        }
    )));
}

#[test]
fn parses_tool_use_block() {
    let events: Vec<StreamEvent> = frames(TOOLS)
        .into_iter()
        .filter_map(parse_messages_sse)
        .collect();
    assert!(events.iter().any(|e| matches!(
        e,
        StreamEvent::ToolUseStart { call_id, name } if call_id == "tool_1" && name == "list_spaces"
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        StreamEvent::ToolUseArgsDelta { json_chunk, .. } if json_chunk == "{\"filter\":\"all\"}"
    )));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUseEnd { .. }))
    );
    assert!(events.iter().any(|e| matches!(
        e,
        StreamEvent::StopTurn {
            reason: StopReason::ToolUse
        }
    )));
}

#[test]
fn build_request_sets_x_api_key_and_version_header() {
    let msgs = vec![Message::user("hi")];
    let req = build_request("claude-sonnet-4-6", &msgs, &[], "test-key");
    assert_eq!(req.url().as_str(), ENDPOINT);
    assert_eq!(req.headers().get("x-api-key").unwrap(), "test-key");
    assert_eq!(
        req.headers().get("anthropic-version").unwrap(),
        "2023-06-01"
    );
}
