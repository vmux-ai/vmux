use super::*;

#[test]
fn user_roundtrip() {
    let message = Message::user("hi");
    let json = serde_json::to_string(&message).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(message, back);
    assert!(!json.contains("attachments"));
}

#[test]
fn user_deserializes_legacy_message_without_attachments() {
    let message: Message = serde_json::from_str(r#"{"User":{"text":"hi"}}"#).unwrap();
    assert_eq!(message, Message::user("hi"));
}

#[test]
fn assistant_blocks_roundtrip() {
    let message = Message::Assistant {
        blocks: vec![
            AssistantBlock::Text("hello".into()),
            AssistantBlock::ToolUse {
                call_id: "abc".into(),
                name: "list_spaces".into(),
                args: "{}".to_string(),
                parent_call_id: None,
            },
        ],
    };
    let json = serde_json::to_string(&message).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(message, back);
}

#[test]
fn tool_use_deserializes_without_parent_call_id() {
    let block: AssistantBlock =
        serde_json::from_str(r#"{"ToolUse":{"call_id":"abc","name":"run","args":"{}"}}"#).unwrap();
    assert!(matches!(
        block,
        AssistantBlock::ToolUse {
            parent_call_id: None,
            ..
        }
    ));
}

#[test]
fn new_chat_request_roundtrips() {
    let request = NewChatRequest {
        client_op_id: ClientOpId::new("op-1"),
        text: "start here".to_string(),
        agent_url: Some("vmux://agent/claude".to_string()),
    };
    let json = serde_json::to_string(&request).unwrap();
    let back: NewChatRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back.text, request.text);
    assert_eq!(back.agent_url, request.agent_url);
}

#[test]
fn prompt_request_deserializes_without_attachments() {
    let request: PromptRequest =
        serde_json::from_str(r#"{"client_op_id":"op-1","text":"hello"}"#).unwrap();
    assert_eq!(request.text, "hello");
    assert!(request.attachments.is_empty());
}

#[test]
fn message_projection_has_stable_order_and_reply_links() {
    let events = RoomEvent::from_messages(
        "session-1",
        100,
        &[
            Message::user("hello"),
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("hi".to_string())],
            },
        ],
    );

    assert_eq!(
        events[0].event_id,
        EventId::new("session:session-1:event:1")
    );
    assert_eq!(events[1].server_seq, 2);
    assert_eq!(events[1].reply_to, Some(events[0].event_id.clone()));
    assert_eq!(events[1].created_at_ms, 101);
}

#[test]
fn inline_media_query_requires_an_open_token() {
    assert_eq!(
        inline_media_query("inspect @Pictures/scr"),
        Some(InlineMediaQuery {
            start: 8,
            query: "Pictures/scr",
        })
    );
    assert_eq!(inline_media_query("mail@example.com"), None);
    assert_eq!(inline_media_query("inspect @image.png next"), None);
}

#[test]
fn conversation_title_uses_first_user_prompt() {
    let messages = vec![
        Message::user("  Show me something fun.\n in terminal  "),
        Message::Assistant { blocks: Vec::new() },
        Message::user("later"),
    ];
    assert_eq!(
        Message::conversation_title(&messages, "Codex"),
        "Show me something fun. in terminal"
    );
}

#[test]
fn conversation_title_falls_back_and_sanitizes() {
    assert_eq!(Message::conversation_title(&[], "Codex"), "Codex");
    assert_eq!(
        Message::conversation_title(&[Message::user("Fix \u{202e}\x1b title")], "Codex"),
        "Fix title"
    );
}
