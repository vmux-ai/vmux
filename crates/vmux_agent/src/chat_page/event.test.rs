use super::*;

#[test]
fn chat_snapshot_rkyv_roundtrip() {
    let v = ChatSnapshot {
        messages_json: "[]".to_string(),
        messages_start: 12,
        messages_total: 60,
        status: "streaming".to_string(),
        conversation_title: "Refine generated summaries".to_string(),
        handoff_source: "Codex".to_string(),
        handoff_truncated: true,
        handoff_message_count: 2,
        choice_question: "Repository?".into(),
        choice_options: vec!["Local".into(), "Remote".into(), "Create".into()],
        queued: vec![
            QueuedPromptSnapshot {
                id: 4,
                text: "a".into(),
                attachment_names: vec!["image.png".into()],
            },
            QueuedPromptSnapshot {
                id: 9,
                text: "b".into(),
                attachment_names: Vec::new(),
            },
        ],
        paused: true,
        ..Default::default()
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
    let back = rkyv::from_bytes::<ChatSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(back.status, "streaming");
    assert_eq!(back.conversation_title, "Refine generated summaries");
    assert_eq!(back.messages_start, 12);
    assert_eq!(back.messages_total, 60);
    assert_eq!(back.queued.len(), 2);
    assert_eq!(back.queued[0].id, 4);
    assert_eq!(back.queued[0].text, "a");
    assert_eq!(back.queued[1].id, 9);
    assert_eq!(back.queued[1].text, "b");
    assert!(back.paused);
    assert_eq!(back.handoff_source, "Codex");
    assert!(back.handoff_truncated);
    assert_eq!(back.handoff_message_count, 2);
    assert_eq!(back.choice_question, "Repository?");
    assert_eq!(back.choice_options.len(), 3);
}

#[test]
fn chat_history_page_rkyv_roundtrip() {
    let value = ChatHistoryPage {
        items_json: "[]".into(),
        start: 4,
        end: 44,
        total: 92,
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&value).unwrap();
    let back = rkyv::from_bytes::<ChatHistoryPage, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!((back.start, back.end, back.total), (4, 44, 92));
}

#[test]
fn chat_media_entries_rkyv_roundtrip() {
    let value = ChatMediaEntries {
        request_id: 7,
        query: "Pictures/scr".into(),
        entries: vec![ChatMediaEntry {
            path: "/Users/me/Pictures/screenshot.png".into(),
            name: "screenshot.png".into(),
            parent: "~/Pictures".into(),
            mime_type: "image/png".into(),
            is_dir: false,
            preview_data_url: "data:image/png;base64,cG5n".into(),
        }],
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&value).unwrap();
    let back = rkyv::from_bytes::<ChatMediaEntries, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(back.request_id, 7);
    assert_eq!(back.entries[0].name, "screenshot.png");
    assert!(
        back.entries[0]
            .preview_data_url
            .starts_with("data:image/png")
    );
}

#[test]
fn chat_choice_selected_rkyv_roundtrip() {
    let value = ChatChoiceSelected { index: 2 };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&value).unwrap();
    let back = rkyv::from_bytes::<ChatChoiceSelected, rkyv::rancor::Error>(&bytes).unwrap();

    assert_eq!(back.index, 2);
}

#[test]
fn chat_item_turn_roundtrip() {
    let items = vec![
        ChatItem::User {
            text: "hi".into(),
            context: Some("project policy".into()),
            attachments: vec![ChatSubmitAttachment {
                path: "/tmp/image.png".into(),
                name: "image.png".into(),
                mime_type: "image/png".into(),
                size: 3,
            }],
        },
        ChatItem::Turn(ChatTurn {
            blocks: vec![
                ChatBlock::Thinking("hmm".into()),
                ChatBlock::ToolResult {
                    call_id: "call-1".into(),
                    content: "ok".into(),
                    is_error: false,
                },
                ChatBlock::Text("done".into()),
            ],
            running: false,
            duration_secs: Some(12),
            step_count: 2,
        }),
    ];
    let json = serde_json::to_string(&items).unwrap();
    let back: Vec<ChatItem> = serde_json::from_str(&json).unwrap();
    assert_eq!(back.len(), 2);
    assert!(matches!(
        &back[0],
        ChatItem::User { context, attachments, .. }
            if context.as_deref() == Some("project policy")
                && attachments.first().is_some_and(|attachment| attachment.name == "image.png")
    ));
    let ChatItem::Turn(turn) = &back[1] else {
        panic!("expected turn")
    };
    assert_eq!(turn.step_count, 2);
    assert_eq!(turn.duration_secs, Some(12));
    assert_eq!(turn.blocks.len(), 3);
    assert!(matches!(
        turn.blocks[1],
        ChatBlock::ToolResult {
            is_error: false,
            ..
        }
    ));
}

#[test]
fn working_verbs_nonempty() {
    assert!(!WORKING_VERB_IDS.is_empty());
}

#[test]
fn tool_children_associate_with_their_parent_call() {
    let turn = ChatTurn {
        blocks: vec![
            ChatBlock::ToolUse {
                call_id: "read-1".into(),
                name: "read_file".into(),
                args: "{}".into(),
                parent_call_id: None,
            },
            ChatBlock::ToolUse {
                call_id: "review-1".into(),
                name: "guardian_review".into(),
                args: "{}".into(),
                parent_call_id: None,
            },
            ChatBlock::ToolResult {
                call_id: "read-1".into(),
                content: "file contents".into(),
                is_error: false,
            },
            ChatBlock::ToolResult {
                call_id: "review-1".into(),
                content: "review complete".into(),
                is_error: false,
            },
        ],
        ..Default::default()
    };

    assert_eq!(turn.parent_tool_index(0), None);
    assert_eq!(turn.parent_tool_index(1), Some(0));
    assert_eq!(turn.parent_tool_index(2), Some(0));
    assert_eq!(turn.parent_tool_index(3), Some(0));
}

#[test]
fn latest_top_level_tool_ignores_results_and_nested_tools() {
    let turn = ChatTurn {
        blocks: vec![
            ChatBlock::ToolUse {
                call_id: "first".into(),
                name: "read_file".into(),
                args: "{}".into(),
                parent_call_id: None,
            },
            ChatBlock::ToolResult {
                call_id: "first".into(),
                content: "done".into(),
                is_error: false,
            },
            ChatBlock::ToolUse {
                call_id: "nested".into(),
                name: "guardian_review".into(),
                args: "{}".into(),
                parent_call_id: Some("first".into()),
            },
            ChatBlock::ToolUse {
                call_id: "second".into(),
                name: "run".into(),
                args: "{}".into(),
                parent_call_id: None,
            },
        ],
        ..Default::default()
    };

    assert_eq!(turn.latest_top_level_tool_index(), Some(3));
}

#[test]
fn latest_tool_location_selects_only_the_newest_turn_tool() {
    let tool = |call_id: &str| ChatBlock::ToolUse {
        call_id: call_id.into(),
        name: "run".into(),
        args: "{}".into(),
        parent_call_id: None,
    };
    let items = vec![
        ChatItem::Turn(ChatTurn {
            blocks: vec![tool("old")],
            ..Default::default()
        }),
        ChatItem::User {
            text: "next".into(),
            context: None,
            attachments: Vec::new(),
        },
        ChatItem::Turn(ChatTurn {
            blocks: vec![ChatBlock::Text("working".into()), tool("new")],
            ..Default::default()
        }),
    ];

    assert_eq!(latest_tool_location(&items), Some((2, 1)));
}

#[test]
fn empty_call_ids_do_not_associate() {
    let turn = ChatTurn {
        blocks: vec![
            ChatBlock::ToolUse {
                call_id: String::new(),
                name: "read_file".into(),
                args: "{}".into(),
                parent_call_id: None,
            },
            ChatBlock::ToolResult {
                call_id: String::new(),
                content: "file contents".into(),
                is_error: false,
            },
        ],
        ..Default::default()
    };

    assert_eq!(turn.parent_tool_index(0), None);
    assert_eq!(turn.parent_tool_index(1), None);
}

#[test]
fn standalone_guardian_owns_its_result() {
    let turn = ChatTurn {
        blocks: vec![
            ChatBlock::ToolUse {
                call_id: "review-1".into(),
                name: "guardian_review".into(),
                args: "{}".into(),
                parent_call_id: None,
            },
            ChatBlock::ToolResult {
                call_id: "review-1".into(),
                content: "review complete".into(),
                is_error: false,
            },
        ],
        ..Default::default()
    };

    assert_eq!(turn.parent_tool_index(0), None);
    assert_eq!(turn.parent_tool_index(1), Some(0));
}

#[test]
fn resumable_sessions_rkyv_roundtrip() {
    let v = ResumableSessions {
        sessions: vec![ResumableSessionEntry {
            kind: "claude".into(),
            sid: "sid-9".into(),
            cwd: "/w".into(),
            title: "fix bug".into(),
            subtitle: "w".into(),
            age_seconds: 7200,
            agent_name: "Claude".into(),
            cross_runtime: true,
        }],
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
    let back = rkyv::from_bytes::<ResumableSessions, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(back.sessions.len(), 1);
    assert_eq!(back.sessions[0].sid, "sid-9");
    assert_eq!(back.sessions[0].agent_name, "Claude");
    assert!(back.sessions[0].cross_runtime);
}
