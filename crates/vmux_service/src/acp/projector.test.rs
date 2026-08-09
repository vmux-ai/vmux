use super::*;
use agent_client_protocol::schema::v1::{
    ContentChunk, Diff, SessionInfoUpdate, SessionUpdate, Terminal, TextContent, ToolCall,
    ToolCallContent, ToolCallUpdateFields,
};

fn chunk(text: &str) -> SessionUpdate {
    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
        text,
    ))))
}

fn meta(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let serde_json::Value::Object(meta) = value else {
        panic!("expected metadata object")
    };
    meta
}

#[test]
fn session_info_worktree_metadata_emits_workspace_change() {
    let mut meta = serde_json::Map::new();
    meta.insert(
        "worktree".to_string(),
        serde_json::json!({
            "name": "quiet-amber-wolf",
            "branch": "vibe/quiet-amber-wolf",
            "cwd": "/worktrees/quiet-amber-wolf/subdir",
            "workspaceCwd": "/repo/subdir"
        }),
    );
    let mut projector = AcpProjector::new();

    let intents = projector.apply(SessionUpdate::SessionInfoUpdate(
        SessionInfoUpdate::new().meta(meta),
    ));

    assert_eq!(
        intents,
        vec![Intent::WorkspaceChanged {
            name: "quiet-amber-wolf".to_string(),
            branch: "vibe/quiet-amber-wolf".to_string(),
            cwd: "/worktrees/quiet-amber-wolf/subdir".to_string(),
            workspace_cwd: "/repo/subdir".to_string(),
        }]
    );
}

#[test]
fn session_info_rejects_relative_worktree_paths() {
    let mut meta = serde_json::Map::new();
    meta.insert(
        "worktree".to_string(),
        serde_json::json!({
            "name": "quiet-amber-wolf",
            "branch": "vibe/quiet-amber-wolf",
            "cwd": "worktrees/quiet-amber-wolf",
            "workspaceCwd": "/repo"
        }),
    );
    let mut projector = AcpProjector::new();

    let intents = projector.apply(SessionUpdate::SessionInfoUpdate(
        SessionInfoUpdate::new().meta(meta),
    ));

    assert!(intents.is_empty());
}

#[test]
fn message_chunks_accumulate_into_one_assistant_message() {
    let mut p = AcpProjector::new();
    let first = p.apply(chunk("Hel"));
    let second = p.apply(chunk("lo"));
    assert_eq!(
        first,
        vec![Intent::Delta("Hel".to_string()), Intent::Snapshot]
    );
    assert_eq!(
        second,
        vec![Intent::Delta("lo".to_string()), Intent::Snapshot]
    );
    assert_eq!(p.messages().len(), 1);
    assert_eq!(
        p.messages()[0],
        Message::Assistant {
            blocks: vec![AssistantBlock::Text("Hello".to_string())],
        }
    );
}

#[test]
fn push_user_records_a_turn_before_following_assistant_text() {
    let mut p = AcpProjector::new();
    let attachment = AgentAttachment {
        path: "/tmp/image.png".into(),
        name: "image.png".into(),
        mime_type: "image/png".into(),
        size: 3,
    };
    p.push_user("hi".to_string(), vec![attachment.clone()]);
    p.apply(chunk("hello"));
    assert_eq!(p.messages().len(), 2);
    assert_eq!(
        p.messages()[0],
        Message::User {
            text: "hi".to_string(),
            attachments: vec![attachment],
        }
    );
    assert_eq!(
        p.messages()[1],
        Message::Assistant {
            blocks: vec![AssistantBlock::Text("hello".to_string())],
        }
    );
}

#[test]
fn tool_call_with_diff_emits_proposed_diff_and_records_block() {
    let mut p = AcpProjector::new();
    let tc = ToolCall::new("c1", "Edit file").content(vec![ToolCallContent::Diff(
        Diff::new("/tmp/a.rs", "b").old_text("a"),
    )]);
    let intents = p.apply(SessionUpdate::ToolCall(tc));
    assert!(intents.contains(&Intent::Snapshot));
    assert!(intents.iter().any(|i| matches!(
        i,
        Intent::ProposedDiff { call_id, path, old_text, new_text }
            if call_id == "c1"
                && path == "/tmp/a.rs"
                && old_text.as_deref() == Some("a")
                && new_text == "b"
    )));
    assert_eq!(p.messages().len(), 1);
    match &p.messages()[0] {
        Message::Assistant { blocks } => assert!(matches!(
            blocks.first(),
            Some(AssistantBlock::ToolUse { call_id, .. }) if call_id == "c1"
        )),
        other => panic!("expected assistant message, got {other:?}"),
    }
}

#[test]
fn codex_subagent_metadata_projects_first_class_block() {
    let mut p = AcpProjector::new();
    let tc = ToolCall::new("sub-1", "Start subagent explorer")
        .status(ToolCallStatus::InProgress)
        .raw_input(serde_json::json!({
            "agentThreadId": "thread-child",
            "agentPath": ".codex/agents/explorer.toml",
            "activityKind": "started",
            "prompt": "Inspect ACP projection",
            "model": "gpt-5.4",
            "reasoningEffort": "high"
        }))
        .meta(meta(serde_json::json!({
            "codex": {
                "subagent": {
                    "threadId": "thread-child",
                    "path": ".codex/agents/explorer.toml",
                    "activity": "started"
                }
            }
        })));

    p.apply(SessionUpdate::ToolCall(tc));

    let Message::Assistant { blocks } = &p.messages()[0] else {
        panic!("expected assistant message")
    };
    let AssistantBlock::Subagent(subagent) = &blocks[0] else {
        panic!("expected subagent block")
    };
    assert_eq!(subagent.provider, "Codex");
    assert_eq!(subagent.status, "in_progress");
    assert_eq!(subagent.action, "started");
    assert_eq!(subagent.agent_name.as_deref(), Some("explorer"));
    assert_eq!(subagent.thread_id.as_deref(), Some("thread-child"));
    assert_eq!(subagent.prompt.as_deref(), Some("Inspect ACP projection"));
    assert_eq!(subagent.model.as_deref(), Some("gpt-5.4"));
    assert_eq!(subagent.reasoning_effort.as_deref(), Some("high"));
}

#[test]
fn codex_collaboration_metadata_projects_child_threads() {
    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("spawn-1", "spawn_agent")
            .status(ToolCallStatus::InProgress)
            .raw_input(serde_json::json!({
                "prompt": "Inspect two subsystems",
                "receiverThreadIds": ["thread-a", "thread-b"],
                "model": "gpt-5.4",
                "reasoningEffort": "medium"
            }))
            .meta(meta(serde_json::json!({
                "codex": {
                    "collaboration": {
                        "tool": "spawn_agent",
                        "senderThreadId": "thread-root",
                        "receiverThreadIds": ["thread-a", "thread-b"]
                    }
                }
            }))),
    ));

    let Message::Assistant { blocks } = &p.messages()[0] else {
        panic!("expected assistant message")
    };
    let AssistantBlock::Subagent(subagent) = &blocks[0] else {
        panic!("expected subagent block")
    };
    assert_eq!(subagent.action, "spawn_agent");
    assert_eq!(subagent.parent_thread_id.as_deref(), Some("thread-root"));
    assert_eq!(subagent.child_thread_ids, ["thread-a", "thread-b"]);
    assert_eq!(subagent.prompt.as_deref(), Some("Inspect two subsystems"));
}

#[test]
fn subagent_update_without_metadata_preserves_identity_and_records_output() {
    use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("sub-1", "Start subagent explorer")
            .status(ToolCallStatus::InProgress)
            .meta(meta(serde_json::json!({
                "codex": {
                    "subagent": {
                        "threadId": "thread-child",
                        "path": "explorer",
                        "activity": "started"
                    }
                }
            }))),
    ));

    p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "sub-1",
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .raw_output(serde_json::json!({"summary": "inspection complete"})),
    )));

    let Message::Assistant { blocks } = &p.messages()[0] else {
        panic!("expected assistant message")
    };
    assert!(matches!(
        &blocks[0],
        AssistantBlock::Subagent(subagent) if subagent.status == "completed"
    ));
    assert!(p.messages().iter().any(|message| matches!(
        message,
        Message::ToolResult { call_id, content, is_error: false }
            if call_id == "sub-1" && content.contains("inspection complete")
    )));
}

#[test]
fn claude_agent_and_child_tool_preserve_parent_relationship() {
    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("agent-1", "Inspect ACP support")
            .status(ToolCallStatus::InProgress)
            .raw_input(serde_json::json!({
                "description": "Inspect ACP support",
                "prompt": "Trace subagent metadata",
                "subagent_type": "Explore",
                "model": "sonnet"
            }))
            .meta(meta(serde_json::json!({
                "claudeCode": {"toolName": "Agent"}
            }))),
    ));
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("read-1", "Read files").meta(meta(serde_json::json!({
            "claudeCode": {
                "toolName": "Read",
                "parentToolUseId": "agent-1"
            }
        }))),
    ));

    let Message::Assistant { blocks } = &p.messages()[0] else {
        panic!("expected assistant message")
    };
    assert!(matches!(
        &blocks[0],
        AssistantBlock::Subagent(subagent)
            if subagent.provider == "Claude"
                && subagent.agent_name.as_deref() == Some("Explore")
                && subagent.prompt.as_deref() == Some("Trace subagent metadata")
    ));
    assert!(matches!(
        &blocks[1],
        AssistantBlock::ToolUse { parent_call_id, .. }
            if parent_call_id.as_deref() == Some("agent-1")
    ));
}

#[test]
fn edit_diff_without_locations_emits_and_retries_file_touch() {
    use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

    let mut p = AcpProjector::new();
    let started = p.apply(SessionUpdate::ToolCall(
        ToolCall::new("c1", "Editing files")
            .kind(ToolKind::Edit)
            .content(vec![ToolCallContent::Diff(Diff::new(
                "/repo/src/main.rs",
                "new",
            ))]),
    ));
    let completed = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1",
        ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
    )));

    for intents in [started, completed] {
        assert!(intents.iter().any(|intent| matches!(
            intent,
            Intent::FileTouched { path, line: None, kind }
                if path == "/repo/src/main.rs"
                    && *kind == crate::protocol::FileTouchKind::Edit
        )));
    }
}

#[test]
fn tool_call_details_returns_projected_title_and_input() {
    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("c1", "vmux.run")
            .raw_input(serde_json::json!({"command": "echo hi", "focus": true})),
    ));

    assert_eq!(
        p.tool_call_details("c1"),
        Some((
            "vmux.run".to_string(),
            r#"{"command":"echo hi","focus":true}"#.to_string(),
        ))
    );
}

#[test]
fn conversation_title_tool_stays_out_of_transcript() {
    let mut p = AcpProjector::new();
    let started = p.apply(SessionUpdate::ToolCall(
        ToolCall::new("title-1", "mcp__vmux__set_conversation_title")
            .raw_input(serde_json::json!({"title": "Paris Izakaya Website"})),
    ));
    assert_eq!(
        p.tool_call_details("title-1"),
        Some((
            "mcp__vmux__set_conversation_title".to_string(),
            r#"{"title":"Paris Izakaya Website"}"#.to_string(),
        ))
    );
    let completed = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "title-1",
        ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
    )));

    assert!(started.is_empty());
    assert!(completed.is_empty());
    assert!(p.messages().is_empty());
    assert!(p.tool_call_details("title-1").is_none());
}

#[test]
fn conversation_title_tool_recognizes_agent_identifier_variants() {
    for title in [
        "mcp__vmux__set_conversation_title",
        "mcp.vmux.set_conversation_title",
        "mcp vmux set conversation title",
        "mcp-vmux-set-conversation-title",
        "mcp:vmux:set:conversation:title",
        "mcp__vmux.set-conversation title",
    ] {
        assert!(is_conversation_title_tool(title));
    }
    assert!(!is_conversation_title_tool(
        "mcp__other__set_conversation_title"
    ));
    assert!(!is_conversation_title_tool("set_conversation_title"));
}

#[test]
fn read_tool_call_locations_emit_file_touched() {
    let mut p = AcpProjector::new();
    let tc = ToolCall::new("c1", "Read file")
        .kind(ToolKind::Read)
        .locations(vec![ToolCallLocation::new("/repo/src/main.rs")]);
    let intents = p.apply(SessionUpdate::ToolCall(tc));
    assert!(intents.iter().any(|i| matches!(
        i,
        Intent::FileTouched { path, line: None, kind }
            if path == "/repo/src/main.rs" && *kind == crate::protocol::FileTouchKind::Read
    )));
}

#[test]
fn completed_edit_retries_file_touch_from_initial_tool_call() {
    use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

    let mut p = AcpProjector::new();
    let tc = ToolCall::new("c1", "Write file")
        .kind(ToolKind::Edit)
        .locations(vec![ToolCallLocation::new("/repo/new.rs")]);
    p.apply(SessionUpdate::ToolCall(tc));

    let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1",
        ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
    )));

    assert!(intents.iter().any(|intent| matches!(
        intent,
        Intent::FileTouched { path, line: None, kind }
            if path == "/repo/new.rs" && *kind == crate::protocol::FileTouchKind::Edit
    )));
}

#[test]
fn failed_edit_clears_pending_and_suppresses_future_touches() {
    use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("c1", "Write file")
            .kind(ToolKind::Edit)
            .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
    ));

    let failed = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1",
        ToolCallUpdateFields::new().status(ToolCallStatus::Failed),
    )));
    let completed = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1",
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .kind(ToolKind::Edit)
            .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
    )));

    assert!(
        failed
            .iter()
            .chain(&completed)
            .all(|intent| !matches!(intent, Intent::FileTouched { .. }))
    );
    assert!(!p.file_touches.contains_key("c1"));
    assert!(!p.file_touch_order.iter().any(|call_id| call_id == "c1"));
}

#[test]
fn locations_only_update_uses_initial_edit_kind() {
    use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("c1", "Write file").kind(ToolKind::Edit),
    ));

    let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1",
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::InProgress)
            .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
    )));

    assert!(intents.iter().any(|intent| matches!(
        intent,
        Intent::FileTouched { path, line: None, kind }
            if path == "/repo/new.rs" && *kind == crate::protocol::FileTouchKind::Edit
    )));
}

#[test]
fn kind_only_update_uses_initial_locations() {
    use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("c1", "Write file").locations(vec![ToolCallLocation::new("/repo/new.rs")]),
    ));

    let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1",
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::InProgress)
            .kind(ToolKind::Edit),
    )));

    assert!(intents.iter().any(|intent| matches!(
        intent,
        Intent::FileTouched { path, line: None, kind }
            if path == "/repo/new.rs" && *kind == crate::protocol::FileTouchKind::Edit
    )));
}

#[test]
fn completion_with_explicit_locations_uses_replacement() {
    use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("c1", "Write files")
            .kind(ToolKind::Edit)
            .locations(vec![
                ToolCallLocation::new("/repo/a.rs"),
                ToolCallLocation::new("/repo/b.rs"),
            ]),
    ));

    let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1",
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .kind(ToolKind::Edit)
            .locations(vec![ToolCallLocation::new("/repo/b.rs")]),
    )));
    let touches: Vec<_> = intents
        .iter()
        .filter_map(|intent| match intent {
            Intent::FileTouched { path, .. } => Some(path.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(touches, vec!["/repo/b.rs"]);
}

#[test]
fn completion_reclassification_does_not_replay_initial_edit() {
    use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("c1", "Write file")
            .kind(ToolKind::Edit)
            .locations(vec![ToolCallLocation::new("/repo/old.rs")]),
    ));

    let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1",
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .kind(ToolKind::Read)
            .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
    )));

    assert_eq!(
        intents
            .iter()
            .filter(|intent| matches!(intent, Intent::FileTouched { .. }))
            .collect::<Vec<_>>(),
        vec![&Intent::FileTouched {
            path: "/repo/new.rs".to_string(),
            line: None,
            kind: crate::protocol::FileTouchKind::Read,
        }]
    );
}

#[test]
fn repeated_completion_emits_no_duplicate_file_touch() {
    use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("c1", "Write file")
            .kind(ToolKind::Edit)
            .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
    ));
    p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1",
        ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
    )));

    let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1",
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .kind(ToolKind::Edit)
            .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
    )));

    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, Intent::FileTouched { .. }))
    );
    assert!(!p.file_touches.contains_key("c1"));
}

#[test]
fn read_completion_with_unchanged_identity_emits_no_duplicate_touch() {
    use agent_client_protocol::schema::v1::{ToolCallUpdate, ToolCallUpdateFields};

    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(
        ToolCall::new("c1", "Read file")
            .kind(ToolKind::Read)
            .locations(vec![ToolCallLocation::new("/repo/file.rs")]),
    ));

    let intents = p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1",
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .kind(ToolKind::Read)
            .locations(vec![ToolCallLocation::new("/repo/file.rs")]),
    )));

    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, Intent::FileTouched { .. }))
    );
}

#[test]
fn finalized_file_touch_tombstones_are_bounded() {
    let mut p = AcpProjector::new();
    for index in 0..1025 {
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new(format!("c{index}"), "Read file")
                .kind(ToolKind::Read)
                .status(ToolCallStatus::Completed)
                .locations(vec![ToolCallLocation::new(format!("/repo/{index}.rs"))]),
        ));
    }

    assert!(p.finalized_file_touches.len() <= 1024);
}

#[test]
fn in_progress_file_touches_are_bounded() {
    let mut p = AcpProjector::new();
    for index in 0..1025 {
        p.apply(SessionUpdate::ToolCall(
            ToolCall::new(format!("c{index}"), "Read file")
                .kind(ToolKind::Read)
                .locations(vec![ToolCallLocation::new(format!("/repo/{index}.rs"))]),
        ));
    }

    assert!(p.file_touches.len() <= 1024);
    assert!(!p.file_touches.contains_key("c0"));
    assert!(p.file_touches.contains_key("c1024"));
}

#[test]
fn non_file_tool_call_emits_no_file_touched() {
    let mut p = AcpProjector::new();
    let tc = ToolCall::new("c1", "run a command")
        .kind(ToolKind::Execute)
        .locations(vec![ToolCallLocation::new("/repo/x")]);
    let intents = p.apply(SessionUpdate::ToolCall(tc));
    assert!(
        !intents
            .iter()
            .any(|i| matches!(i, Intent::FileTouched { .. }))
    );
}

fn thought(text: &str) -> SessionUpdate {
    SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
        text,
    ))))
}

#[test]
fn thought_chunks_accumulate_into_a_thinking_block() {
    let mut p = AcpProjector::new();
    p.apply(thought("plan"));
    p.apply(thought("ning"));
    assert_eq!(p.messages().len(), 1);
    assert_eq!(
        p.messages()[0],
        Message::Assistant {
            blocks: vec![AssistantBlock::Thinking("planning".to_string())],
        }
    );
}

#[test]
fn plan_update_replaces_the_single_plan_block() {
    use agent_client_protocol::schema::v1::{Plan, PlanEntry, PlanEntryPriority};
    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::Plan(Plan::new(vec![PlanEntry::new(
        "step one",
        PlanEntryPriority::High,
        PlanEntryStatus::Pending,
    )])));
    p.apply(SessionUpdate::Plan(Plan::new(vec![PlanEntry::new(
        "step one",
        PlanEntryPriority::High,
        PlanEntryStatus::Completed,
    )])));
    let blocks = match &p.messages()[0] {
        Message::Assistant { blocks } => blocks,
        other => panic!("expected assistant, got {other:?}"),
    };
    assert_eq!(blocks.len(), 1);
    match &blocks[0] {
        AssistantBlock::Plan { steps } => {
            assert_eq!(steps.len(), 1);
            assert_eq!(steps[0].content, "step one");
            assert_eq!(steps[0].status, "completed");
        }
        other => panic!("expected plan, got {other:?}"),
    }
}

#[test]
fn tool_call_update_content_becomes_a_tool_result() {
    use agent_client_protocol::schema::v1::{Content, ToolCallUpdate, ToolCallUpdateFields};
    let mut p = AcpProjector::new();
    p.apply(SessionUpdate::ToolCall(ToolCall::new("c1", "run")));
    let fields = ToolCallUpdateFields::new()
        .status(ToolCallStatus::Completed)
        .content(vec![ToolCallContent::Content(Content::new(
            ContentBlock::Text(TextContent::new("hello output")),
        ))]);
    p.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "c1", fields,
    )));
    assert!(p.messages().iter().any(|m| matches!(
        m,
        Message::ToolResult { call_id, content, is_error: false }
            if call_id == "c1" && content == "hello output"
    )));
}

#[test]
fn tool_call_with_terminal_folds_to_pane_pointer_result() {
    let mut p = AcpProjector::new();
    let tc =
        ToolCall::new("c1", "Run").content(vec![ToolCallContent::Terminal(Terminal::new("t1"))]);
    p.apply(SessionUpdate::ToolCall(tc));
    assert!(p.messages().iter().any(|m| matches!(
        m,
        Message::ToolResult { call_id, content, .. }
            if call_id == "c1" && content.contains("pane")
    )));
}

#[test]
fn tool_call_with_terminal_and_text_prefers_text_output() {
    use agent_client_protocol::schema::v1::Content;
    let mut p = AcpProjector::new();
    let tc = ToolCall::new("c1", "Run").content(vec![
        ToolCallContent::Terminal(Terminal::new("t1")),
        ToolCallContent::Content(Content::new(ContentBlock::Text(TextContent::new(
            "real output",
        )))),
    ]);
    p.apply(SessionUpdate::ToolCall(tc));
    assert!(p.messages().iter().any(|m| matches!(
        m,
        Message::ToolResult { call_id, content, .. }
            if call_id == "c1" && content == "real output"
    )));
}
