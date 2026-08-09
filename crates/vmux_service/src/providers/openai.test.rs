use super::*;

const TEXT: &str = include_str!("../../tests/fixtures/openai/text.sse");
const TOOLS: &str = include_str!("../../tests/fixtures/openai/tools.sse");

fn frames(raw: &str) -> Vec<&str> {
    raw.split("\n\n").filter(|s| !s.trim().is_empty()).collect()
}

#[test]
fn parses_text_then_completed_end_turn() {
    let events: Vec<StreamEvent> = frames(TEXT)
        .into_iter()
        .filter_map(parse_responses_sse)
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
fn parses_tool_call_start_args_end_completed_tool_use() {
    let events: Vec<StreamEvent> = frames(TOOLS)
        .into_iter()
        .filter_map(parse_responses_sse)
        .collect();
    let has_start = events.iter().any(|e| {
            matches!(e, StreamEvent::ToolUseStart{call_id, name} if call_id == "call_1" && name == "list_spaces")
        });
    let has_args = events.iter().any(|e| {
            matches!(e, StreamEvent::ToolUseArgsDelta{json_chunk, ..} if json_chunk == "{\"filter\":\"all\"}")
        });
    let has_end = events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolUseEnd { call_id } if call_id == "call_1"));
    let has_stop = events.iter().any(|e| {
        matches!(
            e,
            StreamEvent::StopTurn {
                reason: StopReason::ToolUse
            }
        )
    });
    assert!(has_start && has_args && has_end && has_stop, "{events:?}");
}

#[test]
fn build_request_uses_responses_endpoint_and_bearer_auth() {
    let msgs = vec![Message::user("hi")];
    let req = build_request("gpt-5", &msgs, &[], "test-key");
    assert_eq!(req.url().as_str(), ENDPOINT);
    assert_eq!(
        req.headers().get("authorization").unwrap(),
        "Bearer test-key"
    );
    let body: serde_json::Value =
        serde_json::from_slice(req.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(body["model"], "gpt-5");
    assert_eq!(body["stream"], true);
    assert_eq!(body["input"][0]["type"], "message");
}
