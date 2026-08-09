use super::*;
use crate::{AssistantBlock, Message};

fn user(text: &str) -> Message {
    Message::user(text)
}

fn assistant(text: &str) -> Message {
    Message::Assistant {
        blocks: vec![AssistantBlock::Text(text.to_string())],
    }
}

#[test]
fn context_budget_keeps_newest_complete_messages() {
    let messages = vec![
        user("old message that should not fit"),
        assistant("middle message"),
        user("new message"),
    ];

    let built = build_context(&messages, 100);

    assert!(built.truncated);
    assert!(built.text.contains(OMITTED_MARKER));
    assert!(built.text.contains("new message"));
    assert!(!built.text.contains("old message"));
}

#[test]
fn context_budget_preserves_chronological_order() {
    let messages = vec![user("first"), assistant("second"), user("third")];

    let built = build_context(&messages, 1_000);

    let first = built.text.find("first").unwrap();
    let second = built.text.find("second").unwrap();
    let third = built.text.find("third").unwrap();
    assert!(first < second && second < third);
    assert!(!built.truncated);
}

#[test]
fn context_budget_keeps_a_contiguous_newest_suffix() {
    let messages = vec![
        user("old-small"),
        assistant(&"middle-large".repeat(20)),
        user("new-small"),
    ];

    let built = build_context(&messages, 120);

    assert!(built.text.contains("new-small"));
    assert!(!built.text.contains("middle-large"));
    assert!(!built.text.contains("old-small"));
}

#[test]
fn context_ignores_non_text_assistant_blocks_and_tool_results() {
    let messages = vec![
        Message::Assistant {
            blocks: vec![
                AssistantBlock::Thinking("secret".into()),
                AssistantBlock::Text("visible".into()),
                AssistantBlock::ToolUse {
                    call_id: "c".into(),
                    name: "run".into(),
                    args: "{}".into(),
                    parent_call_id: None,
                },
            ],
        },
        Message::ToolResult {
            call_id: "c".into(),
            content: "tool output".into(),
            is_error: false,
        },
    ];

    let built = build_context(&messages, 1_000);

    assert!(built.text.contains("visible"));
    assert!(!built.text.contains("secret"));
    assert!(!built.text.contains("tool output"));
}

#[test]
fn private_wire_prompt_keeps_display_prompt_separate() {
    let prompt = wire_prompt("prior conversation", "continue here");

    assert!(prompt.starts_with(HANDOFF_PROMPT_PREFIX));
    assert!(prompt.contains("prior conversation"));
    assert!(prompt.ends_with("continue here"));
}

#[test]
fn replay_private_prompt_is_replaced_with_display_prompt() {
    let mut messages = vec![
        user(&wire_prompt("prior conversation", "continue here")),
        assistant("done"),
    ];

    sanitize_replayed_messages(&mut messages, Some("continue here"));

    assert_eq!(messages[0], user("continue here"));
    assert_eq!(messages[1], assistant("done"));
}

#[test]
fn replay_sanitizes_every_retried_private_prompt_from_its_own_payload() {
    let mut messages = vec![
        user(&wire_prompt("prior conversation", "first try")),
        user(&wire_prompt("prior conversation", "second try")),
    ];

    sanitize_replayed_messages(&mut messages, Some("stale sidecar text"));

    assert_eq!(messages, vec![user("first try"), user("second try")]);
}

#[test]
fn replay_preserves_plain_prompt_starting_with_private_prefix() {
    let text = format!("{HANDOFF_PROMPT_PREFIX} ordinary user text");
    let mut messages = vec![user(&text)];

    sanitize_replayed_messages(&mut messages, Some("fallback"));

    assert_eq!(messages, vec![user(&text)]);
}

#[test]
fn pending_context_sends_once_and_can_retry_after_error() {
    let mut pending = PendingHandoff {
        context: "prior conversation".into(),
        sent: false,
    };

    assert_eq!(
        pending.context_for_send().as_deref(),
        Some("prior conversation")
    );
    assert!(pending.context_for_send().is_none());
    pending.retry();
    assert_eq!(
        pending.context_for_send().as_deref(),
        Some("prior conversation")
    );
}

#[test]
fn imported_conversation_sidecar_round_trips() {
    let root = std::env::temp_dir().join(format!(
        "vmux-handoff-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let imported = ImportedConversation {
        source_agent: "Codex".into(),
        source_kind: AgentKind::Codex,
        source_sid: "cx/1".into(),
        messages: vec![user("fix auth"), assistant("working")],
        truncated: true,
        first_prompt: Some("continue".into()),
    };

    save_in(&root, "claude/custom", "target?1", &imported).unwrap();
    let loaded = load_in(&root, "claude/custom", "target?1").unwrap();

    assert_eq!(loaded, imported);
    assert!(record_path_in(&root, "claude/custom", "target?1").starts_with(&root));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn missing_or_malformed_sidecar_is_ignored() {
    let root = std::env::temp_dir().join(format!(
        "vmux-handoff-bad-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    assert!(load_in(&root, "claude", "missing").is_none());
    let path = record_path_in(&root, "claude", "bad");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "not json").unwrap();
    assert!(load_in(&root, "claude", "bad").is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn visible_messages_prepend_imported_history() {
    let imported = ImportedConversation {
        source_agent: "Codex".into(),
        source_kind: AgentKind::Codex,
        source_sid: "cx-1".into(),
        messages: vec![user("old")],
        truncated: false,
        first_prompt: Some("new".into()),
    };

    assert_eq!(
        visible_messages(Some(&imported), &[assistant("reply")]),
        vec![user("old"), assistant("reply")]
    );
}
