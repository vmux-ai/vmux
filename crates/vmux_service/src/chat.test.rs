use super::*;

fn assistant(blocks: Vec<AssistantBlock>) -> Message {
    Message::Assistant { blocks }
}
fn tool(id: &str) -> AssistantBlock {
    AssistantBlock::ToolUse {
        call_id: id.into(),
        name: "run".into(),
        args: "{}".into(),
        parent_call_id: None,
    }
}

fn subagent(id: &str) -> AssistantBlock {
    AssistantBlock::Subagent(Box::new(SubagentBlock {
        call_id: id.into(),
        provider: "Claude".into(),
        title: "Inspect ACP support".into(),
        status: "in_progress".into(),
        action: "delegate".into(),
        agent_name: Some("Explore".into()),
        thread_id: None,
        parent_thread_id: None,
        child_thread_ids: Vec::new(),
        parent_call_id: None,
        prompt: Some("Trace metadata".into()),
        model: Some("sonnet".into()),
        reasoning_effort: None,
        raw_input: "{}".into(),
    }))
}

#[test]
fn splits_steps_and_answer_folds_tool_result() {
    let msgs = vec![
        Message::user("hi"),
        assistant(vec![AssistantBlock::Thinking("t".into()), tool("c1")]),
        Message::ToolResult {
            call_id: "c1".into(),
            content: "ok".into(),
            is_error: false,
        },
        assistant(vec![AssistantBlock::Text("done".into())]),
    ];
    let items = group_turns(&msgs, &[], false);
    assert_eq!(items.len(), 2);
    assert!(matches!(&items[0], ChatItem::User { text, .. } if text == "hi"));
    let ChatItem::Turn(t) = &items[1] else {
        panic!()
    };
    assert_eq!(t.step_count, 2);
    assert_eq!(t.blocks.len(), 4);
    assert!(matches!(t.blocks[2], ChatBlock::ToolResult { .. }));
    assert!(matches!(&t.blocks[3], ChatBlock::Text(text) if text == "done"));
    assert!(!t.running);
}

#[test]
fn user_attachments_are_projected_into_chat_items() {
    let messages = vec![Message::user_with_attachments(
        "inspect",
        vec![crate::protocol::AgentAttachment {
            path: "/tmp/image.png".into(),
            name: "image.png".into(),
            mime_type: "image/png".into(),
            size: 3,
        }],
    )];

    let items = group_turns(&messages, &[], false);

    assert!(matches!(
        &items[0],
        ChatItem::User {
            text, attachments, ..
        }
            if text == "inspect"
                && attachments.len() == 1
                && attachments[0].path == "/tmp/image.png"
    ));
}

#[test]
fn one_turn_per_user_durations_by_ordinal() {
    let msgs = vec![
        Message::user("a"),
        assistant(vec![AssistantBlock::Text("1".into())]),
        Message::user("b"),
        assistant(vec![AssistantBlock::Text("2".into())]),
    ];
    let items = group_turns(&msgs, &[5, 9], false);
    assert_eq!(items.len(), 4);
    let ChatItem::Turn(t0) = &items[1] else {
        panic!()
    };
    let ChatItem::Turn(t1) = &items[3] else {
        panic!()
    };
    assert_eq!(t0.duration_secs, Some(5));
    assert_eq!(t1.duration_secs, Some(9));
}

#[test]
fn tail_page_only_clones_recent_items() {
    let messages = vec![
        Message::user("a"),
        assistant(vec![AssistantBlock::Text("one".into())]),
        Message::user("b"),
        assistant(vec![AssistantBlock::Text("two".into())]),
        Message::user("c"),
        assistant(vec![AssistantBlock::Text("three".into())]),
    ];

    let page = group_turns_tail(&[], &messages, &[1, 2, 3], false, 3);

    assert_eq!((page.start, page.end, page.total), (3, 6, 6));
    assert_eq!(page.items.len(), 3);
    assert!(matches!(&page.items[0], ChatItem::Turn(turn) if turn.duration_secs == Some(2)));
    assert!(matches!(&page.items[1], ChatItem::User { text, .. } if text == "c"));
}

#[test]
fn older_page_ends_at_requested_cursor() {
    let messages = vec![
        Message::user("a"),
        assistant(vec![AssistantBlock::Text("one".into())]),
        Message::user("b"),
        assistant(vec![AssistantBlock::Text("two".into())]),
    ];

    let page = group_turns_before(&[], &messages, &[1, 2], false, 3, 2);

    assert_eq!((page.start, page.end, page.total), (1, 3, 4));
    assert!(matches!(&page.items[0], ChatItem::Turn(turn) if turn.duration_secs == Some(1)));
    assert!(matches!(&page.items[1], ChatItem::User { text, .. } if text == "b"));
}

#[test]
fn private_continuation_starts_hidden_turn() {
    let private = crate::protocol::compose_agent_prompt(
        "",
        Some("Project selected. Continue the original request."),
    );
    let messages = vec![
        Message::user("fix it"),
        assistant(vec![AssistantBlock::Text("Choose a project.".into())]),
        Message::user(private),
        assistant(vec![AssistantBlock::Text("Which branch?".into())]),
    ];

    let items = group_turns(&messages, &[], false);

    assert_eq!(items.len(), 3);
    assert!(matches!(&items[0], ChatItem::User { text, .. } if text == "fix it"));
    assert!(matches!(
        &items[1],
        ChatItem::Turn(turn)
            if matches!(&turn.blocks[0], ChatBlock::Text(text) if text == "Choose a project.")
    ));
    assert!(matches!(
        &items[2],
        ChatItem::Turn(turn)
            if matches!(&turn.blocks[0], ChatBlock::Text(text) if text == "Which branch?")
    ));
}

#[test]
fn private_context_is_collapsed_separately_from_display_prompt() {
    let private =
        crate::protocol::compose_agent_prompt("show me something fun", Some("project policy"));
    let messages = vec![Message::user(format!("show me something fun{private}"))];

    let items = group_turns(&messages, &[], false);

    assert!(matches!(
        &items[0],
        ChatItem::User { text, context, .. }
            if text == "show me something fun"
                && context.as_deref() == Some("project policy")
    ));
}

#[test]
fn missing_duration_is_none() {
    let msgs = vec![
        Message::user("a"),
        assistant(vec![AssistantBlock::Text("1".into())]),
        Message::user("b"),
        assistant(vec![AssistantBlock::Text("2".into())]),
    ];
    let items = group_turns(&msgs, &[5], false);
    let ChatItem::Turn(t1) = &items[3] else {
        panic!()
    };
    assert_eq!(t1.duration_secs, None);
}

#[test]
fn running_marks_and_nulls_last_turn() {
    let msgs = vec![
        Message::user("a"),
        assistant(vec![AssistantBlock::Text("1".into())]),
    ];
    let items = group_turns(&msgs, &[5], true);
    let ChatItem::Turn(t) = &items[1] else {
        panic!()
    };
    assert!(t.running);
    assert_eq!(t.duration_secs, None);
}

#[test]
fn running_emits_empty_tail_turn_after_user() {
    let msgs = vec![Message::user("a")];
    let items = group_turns(&msgs, &[], true);
    assert_eq!(items.len(), 2);
    let ChatItem::Turn(t) = &items[1] else {
        panic!()
    };
    assert!(t.running);
    assert_eq!(t.step_count, 0);
    assert!(t.blocks.is_empty());
}

#[test]
fn preserves_step_and_prose_order() {
    let msgs = vec![
        Message::user("a"),
        assistant(vec![
            AssistantBlock::Text("before".into()),
            tool("c1"),
            AssistantBlock::Text("after".into()),
        ]),
    ];
    let items = group_turns(&msgs, &[], false);
    let ChatItem::Turn(turn) = &items[1] else {
        panic!()
    };
    assert!(matches!(&turn.blocks[0], ChatBlock::Text(text) if text == "before"));
    assert!(matches!(&turn.blocks[1], ChatBlock::ToolUse { .. }));
    assert!(matches!(&turn.blocks[2], ChatBlock::Text(text) if text == "after"));
}

#[test]
fn unmatched_tool_result_remains_a_step() {
    let msgs = vec![
        Message::user("a"),
        Message::ToolResult {
            call_id: "missing".into(),
            content: "output".into(),
            is_error: false,
        },
    ];
    let items = group_turns(&msgs, &[], false);
    let ChatItem::Turn(turn) = &items[1] else {
        panic!()
    };
    assert_eq!(turn.step_count, 1);
}

#[test]
fn guardian_and_results_count_as_one_tool_step() {
    let msgs = vec![
        Message::user("a"),
        assistant(vec![
            AssistantBlock::ToolUse {
                call_id: "read-1".into(),
                name: "read_file".into(),
                args: "{}".into(),
                parent_call_id: None,
            },
            AssistantBlock::ToolUse {
                call_id: "review-1".into(),
                name: "guardian_review".into(),
                args: "{}".into(),
                parent_call_id: None,
            },
        ]),
        Message::ToolResult {
            call_id: "read-1".into(),
            content: "output".into(),
            is_error: false,
        },
    ];
    let items = group_turns(&msgs, &[], false);
    let ChatItem::Turn(turn) = &items[1] else {
        panic!()
    };
    assert_eq!(turn.step_count, 1);
}

#[test]
fn subagent_children_and_results_fold_into_one_visible_step() {
    let msgs = vec![
        Message::user("a"),
        assistant(vec![
            subagent("agent-1"),
            AssistantBlock::ToolUse {
                call_id: "read-1".into(),
                name: "read_file".into(),
                args: "{}".into(),
                parent_call_id: Some("agent-1".into()),
            },
        ]),
        Message::ToolResult {
            call_id: "read-1".into(),
            content: "file contents".into(),
            is_error: false,
        },
        Message::ToolResult {
            call_id: "agent-1".into(),
            content: "done".into(),
            is_error: false,
        },
    ];

    let items = group_turns(&msgs, &[], false);
    let ChatItem::Turn(turn) = &items[1] else {
        panic!()
    };
    assert_eq!(turn.step_count, 1);
    assert!(matches!(&turn.blocks[0], ChatBlock::Subagent(_)));
    assert_eq!(turn.parent_tool_index(1), Some(0));
    assert_eq!(turn.parent_tool_index(2), Some(0));
    assert_eq!(turn.parent_tool_index(3), Some(0));
}

#[test]
fn collapses_consecutive_reconnect_updates() {
    let msgs = vec![
        Message::user("a"),
        assistant(vec![AssistantBlock::Text(
            "Reconnecting... 1/5\n\nReconnecting… 2/5\nReconnecting 3/5".into(),
        )]),
    ];
    let items = group_turns(&msgs, &[], true);
    let ChatItem::Turn(turn) = &items[1] else {
        panic!()
    };
    assert_eq!(turn.blocks.len(), 1);
    assert!(matches!(
        turn.blocks[0],
        ChatBlock::Reconnect {
            attempt: 3,
            total: 5
        }
    ));
    assert_eq!(turn.step_count, 1);
}

#[test]
fn reconnect_updates_do_not_swallow_prose() {
    let msgs = vec![
        Message::user("a"),
        assistant(vec![AssistantBlock::Text(
            "before\nReconnecting... 2/5\nafter".into(),
        )]),
    ];
    let items = group_turns(&msgs, &[], false);
    let ChatItem::Turn(turn) = &items[1] else {
        panic!()
    };
    assert!(matches!(&turn.blocks[0], ChatBlock::Text(text) if text == "before"));
    assert!(matches!(
        turn.blocks[1],
        ChatBlock::Reconnect {
            attempt: 2,
            total: 5
        }
    ));
    assert!(matches!(&turn.blocks[2], ChatBlock::Text(text) if text == "after"));
}
