use super::*;

const MISTRAL_TEXT: &str = include_str!("../../tests/fixtures/mistral/text.sse");
const MISTRAL_TOOLS: &str = include_str!("../../tests/fixtures/mistral/tools.sse");

fn frames(raw: &str) -> Vec<&str> {
    raw.split("\n\n").filter(|s| !s.trim().is_empty()).collect()
}

#[test]
fn parses_text_then_stop() {
    let events: Vec<StreamEvent> = frames(MISTRAL_TEXT)
        .into_iter()
        .filter_map(parse_chat_completions_sse)
        .collect();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0], StreamEvent::TextDelta("hello".into()));
    assert_eq!(events[1], StreamEvent::TextDelta(" world".into()));
    assert!(matches!(
        events[2],
        StreamEvent::StopTurn {
            reason: StopReason::EndTurn
        }
    ));
}

#[test]
fn parses_tool_call_sequence() {
    let events: Vec<StreamEvent> = frames(MISTRAL_TOOLS)
        .into_iter()
        .filter_map(parse_chat_completions_sse)
        .collect();
    match &events[0] {
        StreamEvent::ToolUseStart { call_id, name } => {
            assert_eq!(call_id, "call_1");
            assert_eq!(name, "list_spaces");
        }
        other => panic!("expected ToolUseStart, got {other:?}"),
    }
    match &events[1] {
        StreamEvent::ToolUseArgsDelta { json_chunk, .. } => {
            assert_eq!(json_chunk, "{\"filter\":\"all\"}");
        }
        other => panic!("expected ToolUseArgsDelta, got {other:?}"),
    }
    assert!(matches!(
        events[2],
        StreamEvent::StopTurn {
            reason: StopReason::ToolUse
        }
    ));
}

#[test]
fn messages_to_chat_completions_roundtrip() {
    let msgs = vec![
        Message::user("hi"),
        Message::Assistant {
            blocks: vec![AssistantBlock::Text("hello".into())],
        },
        Message::ToolResult {
            call_id: "c1".into(),
            content: "ok".into(),
            is_error: false,
        },
    ];
    let out = messages_to_chat_completions(&msgs);
    assert_eq!(out[0]["role"], "user");
    assert_eq!(out[1]["role"], "assistant");
    assert_eq!(out[2]["role"], "tool");
    assert_eq!(out[2]["tool_call_id"], "c1");
}

#[test]
fn tools_to_function_specs_shape() {
    let tools = vec![ToolDef {
        name: "list_spaces".into(),
        description: "desc".into(),
        input_schema: json!({"type":"object"}),
        read_only: true,
    }];
    let out = tools_to_function_specs(&tools);
    assert_eq!(out[0]["type"], "function");
    assert_eq!(out[0]["function"]["name"], "list_spaces");
}
