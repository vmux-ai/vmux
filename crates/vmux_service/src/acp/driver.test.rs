use super::*;
use agent_client_protocol::schema::v1::{
    ContentChunk, Implementation, PermissionOptionKind, SessionConfigSelectGroup,
    SessionConfigSelectOption, ToolCall, ToolCallUpdateFields,
};

#[test]
fn stderr_detail_from_shows_last_lines_and_skips_blanks() {
    let tail: VecDeque<String> = [
        "npm warn old",
        "",
        "npm error 403 Forbidden",
        "   ",
        "Blocked by Security Policy",
    ]
    .iter()
    .map(|line| line.to_string())
    .collect();
    assert_eq!(
        stderr_detail_from(&tail, 2),
        "\n\nnpm error 403 Forbidden\nBlocked by Security Policy"
    );
}

#[test]
fn stderr_detail_from_is_empty_without_output() {
    let blanks: VecDeque<String> = ["", "   "].iter().map(|line| line.to_string()).collect();
    assert!(stderr_detail_from(&blanks, 8).is_empty());
    assert!(stderr_detail_from(&VecDeque::new(), 8).is_empty());
}

fn opt(id: &str, kind: PermissionOptionKind) -> PermissionOption {
    PermissionOption::new(id.to_string(), id.to_string(), kind)
}

#[tokio::test]
async fn prompt_content_embeds_supported_images() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("image.png");
    std::fs::write(&path, b"png").unwrap();
    let attachment = AgentAttachment {
        path: path.to_string_lossy().into_owned(),
        name: "image.png".into(),
        mime_type: "image/png".into(),
        size: 3,
    };
    let mut capabilities = PromptCapabilities::default();
    capabilities.image = true;

    let blocks = prompt_content_blocks("inspect", None, &[attachment], &capabilities).await;

    assert!(matches!(&blocks[0], ContentBlock::Text(text) if text.text == "inspect"));
    assert!(matches!(
        &blocks[1],
        ContentBlock::Image(image)
            if image.data == base64::engine::general_purpose::STANDARD.encode(b"png")
                && image.mime_type == "image/png"
    ));
}

#[tokio::test]
async fn prompt_content_links_files_without_media_capability() {
    let attachment = AgentAttachment {
        path: "/tmp/report.txt".into(),
        name: "report.txt".into(),
        mime_type: "text/plain".into(),
        size: 12,
    };

    let blocks =
        prompt_content_blocks("", None, &[attachment], &PromptCapabilities::default()).await;

    assert!(matches!(
        &blocks[0],
        ContentBlock::ResourceLink(link)
            if link.name == "report.txt" && link.uri == "file:///tmp/report.txt"
    ));
}

#[tokio::test]
async fn prompt_content_links_supported_media_above_embed_limit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("large.png");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(PROMPT_MEDIA_FILE_LIMIT + 1)
        .unwrap();
    let attachment = AgentAttachment {
        path: path.to_string_lossy().into_owned(),
        name: "large.png".into(),
        mime_type: "image/png".into(),
        size: PROMPT_MEDIA_FILE_LIMIT + 1,
    };
    let mut capabilities = PromptCapabilities::default();
    capabilities.image = true;

    let blocks = prompt_content_blocks("", None, &[attachment], &capabilities).await;

    assert!(matches!(
        &blocks[0],
        ContentBlock::ResourceLink(link)
            if link.name == "large.png"
                && link.size == i64::try_from(PROMPT_MEDIA_FILE_LIMIT + 1).ok()
    ));
}

#[test]
fn acp_display_name_prefers_title_then_name() {
    let titled = Implementation::new("antigravity", "1.0").title("Antigravity");
    assert_eq!(
        acp_display_name(Some(&titled)).as_deref(),
        Some("Antigravity")
    );

    let named = Implementation::new("claude-code-acp", "1.0");
    assert_eq!(
        acp_display_name(Some(&named)).as_deref(),
        Some("claude-code-acp")
    );
}

#[test]
fn vibe_acp_injects_session_mcp_servers_into_vibe_config() {
    let server = McpServer::Stdio(
        agent_client_protocol::schema::v1::McpServerStdio::new("vmux", "/tmp/vmux")
            .args(vec!["mcp".to_string(), "--profile".to_string()])
            .env(vec![agent_client_protocol::schema::v1::EnvVariable::new(
                "VMUX_PROFILE",
                "dev",
            )]),
    );
    let env = apply_vibe_mcp_env(
        "mistral-vibe",
        vec![(
            "VIBE_MCP_SERVERS".to_string(),
            r#"[{"name":"other","transport":"stdio","command":"other"}]"#.to_string(),
        )],
        &[server],
    );
    let value = env
        .iter()
        .find(|(key, _)| key == "VIBE_MCP_SERVERS")
        .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
        .unwrap();

    assert_eq!(value[0]["name"], "other");
    assert_eq!(value[1]["name"], "vmux");
    assert_eq!(value[1]["command"], "/tmp/vmux");
    assert_eq!(value[1]["args"], serde_json::json!(["mcp", "--profile"]));
    assert_eq!(value[1]["env"]["VMUX_PROFILE"], "dev");
}

#[test]
fn vibe_temp_root_overrides_child_tmpdir() {
    let root = VibeTempRoot::create("mistral-vibe").unwrap().unwrap();
    let env = root.apply_env(vec![
        ("TMPDIR".to_string(), "/old".to_string()),
        ("HOME".to_string(), "/home/test".to_string()),
    ]);
    let expected = root.path().to_string_lossy().into_owned();

    assert_eq!(
        env.iter()
            .filter(|(key, _)| key == "TMPDIR")
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>(),
        vec![expected.as_str()]
    );
    assert!(
        env.iter()
            .any(|(key, value)| key == "HOME" && value == "/home/test")
    );
    assert!(VibeTempRoot::create("codex").unwrap().is_none());
}

#[test]
fn acp_display_name_ignores_blank_metadata() {
    let blank_title = Implementation::new("codex-acp", "1.0").title("   ");
    assert_eq!(
        acp_display_name(Some(&blank_title)).as_deref(),
        Some("codex-acp")
    );

    let blank = Implementation::new("   ", "1.0");
    assert_eq!(acp_display_name(Some(&blank)), None);
    assert_eq!(acp_display_name(None), None);
}

#[test]
fn acp_agent_info_is_replayable_without_a_subscriber() {
    let (stream_tx, stream_rx) = broadcast::channel(1);
    drop(stream_rx);
    let shared = AcpShared::new(
        "s1".into(),
        PathBuf::from("/tmp"),
        ProcessId::new(),
        stream_tx,
        Arc::new(tokio::sync::Mutex::new(ProcessManager::default())),
        Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    );

    shared.publish_agent_info("Antigravity".into());

    match shared.agent_info_message() {
        Some(ServiceMessage::Shared(SharedEvent::AcpAgentInfo { sid, name })) => {
            assert_eq!(sid, "s1");
            assert_eq!(name, "Antigravity");
        }
        other => panic!("expected replayable ACP agent info, got {other:?}"),
    }
}

#[test]
fn model_info_reads_categorized_grouped_selector() {
    let config = SessionConfigOption::select(
        "llm",
        "Language model",
        "opus",
        vec![SessionConfigSelectGroup::new(
            "anthropic",
            "Anthropic",
            vec![
                SessionConfigSelectOption::new("sonnet", "Claude Sonnet"),
                SessionConfigSelectOption::new("opus", "Claude Opus").description("Most capable"),
            ],
        )],
    )
    .category(SessionConfigOptionCategory::Model);

    let info = model_info(&[config]).expect("model selector");

    assert_eq!(info.config_id, "llm");
    assert_eq!(info.current_model_id, "opus");
    assert_eq!(info.models.len(), 2);
    assert_eq!(info.models[1].name, "Claude Opus");
    assert_eq!(info.models[1].description.as_deref(), Some("Most capable"));
}

#[test]
fn model_info_falls_back_to_model_id_without_category() {
    let config = SessionConfigOption::select(
        "model",
        "Runtime",
        "gpt-5",
        vec![SessionConfigSelectOption::new("gpt-5", "GPT-5")],
    );

    let info = model_info(&[config]).expect("model selector");

    assert_eq!(info.config_id, "model");
    assert_eq!(info.current_model_id, "gpt-5");
}

#[test]
fn acp_model_info_is_replayable_without_a_subscriber() {
    let (stream_tx, stream_rx) = broadcast::channel(1);
    drop(stream_rx);
    let shared = AcpShared::new(
        "s1".into(),
        PathBuf::from("/tmp"),
        ProcessId::new(),
        stream_tx,
        Arc::new(tokio::sync::Mutex::new(ProcessManager::default())),
        Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    );
    let config = SessionConfigOption::select(
        "model",
        "Model",
        "sonnet",
        vec![SessionConfigSelectOption::new("sonnet", "Claude Sonnet")],
    )
    .category(SessionConfigOptionCategory::Model);

    shared.publish_model_info(&[config]);

    match shared.model_info_message() {
        Some(ServiceMessage::Shared(SharedEvent::AcpModelInfo {
            sid,
            current_model_id,
            models,
            ..
        })) => {
            assert_eq!(sid, "s1");
            assert_eq!(current_model_id, "sonnet");
            assert_eq!(models[0].name, "Claude Sonnet");
        }
        other => panic!("expected replayable ACP model info, got {other:?}"),
    }
}

#[test]
fn model_selection_result_publishes_request_identity() {
    let (stream_tx, mut stream_rx) = broadcast::channel(2);
    let shared = AcpShared::new(
        "s1".into(),
        PathBuf::from("/tmp"),
        ProcessId::new(),
        stream_tx,
        Arc::new(tokio::sync::Mutex::new(ProcessManager::default())),
        Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    );
    publish_model_selection_result(&shared, 7, "fable", false);

    match stream_rx.try_recv().expect("selection result") {
        ServiceMessage::AcpModelSelectionResult {
            sid,
            request_id,
            model_id,
            succeeded,
        } => {
            assert_eq!(sid, "s1");
            assert_eq!(request_id, 7);
            assert_eq!(model_id, "fable");
            assert!(!succeeded);
        }
        other => panic!("expected ACP model selection result, got {other:?}"),
    }
}

#[test]
fn selected_model_wins_over_stale_set_response() {
    let (stream_tx, mut stream_rx) = broadcast::channel(4);
    let shared = AcpShared::new(
        "s1".into(),
        PathBuf::from("/tmp"),
        ProcessId::new(),
        stream_tx,
        Arc::new(tokio::sync::Mutex::new(ProcessManager::default())),
        Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    );
    let stale = SessionConfigOption::select(
        "model",
        "Model",
        "fable",
        vec![
            SessionConfigSelectOption::new("default", "Default"),
            SessionConfigSelectOption::new("fable", "Fable"),
        ],
    )
    .category(SessionConfigOptionCategory::Model);
    shared.publish_model_info(std::slice::from_ref(&stale));
    let _ = stream_rx.try_recv();

    shared.publish_selected_model("model", "default", &[stale]);

    match stream_rx.try_recv().expect("selected model update") {
        ServiceMessage::Shared(SharedEvent::AcpModelInfo {
            current_model_id, ..
        }) => assert_eq!(current_model_id, "default"),
        other => panic!("expected ACP model info, got {other:?}"),
    }
}

#[test]
fn selected_model_uses_cached_options_when_set_response_is_empty() {
    let (stream_tx, mut stream_rx) = broadcast::channel(4);
    let shared = AcpShared::new(
        "s1".into(),
        PathBuf::from("/tmp"),
        ProcessId::new(),
        stream_tx,
        Arc::new(tokio::sync::Mutex::new(ProcessManager::default())),
        Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    );
    let config = SessionConfigOption::select(
        "model",
        "Model",
        "fable",
        vec![
            SessionConfigSelectOption::new("default", "Default"),
            SessionConfigSelectOption::new("fable", "Fable"),
        ],
    )
    .category(SessionConfigOptionCategory::Model);
    shared.publish_model_info(&[config]);
    let _ = stream_rx.try_recv();

    shared.publish_selected_model("model", "default", &[]);

    match stream_rx.try_recv().expect("selected model update") {
        ServiceMessage::Shared(SharedEvent::AcpModelInfo {
            current_model_id,
            models,
            ..
        }) => {
            assert_eq!(current_model_id, "default");
            assert_eq!(models.len(), 2);
        }
        other => panic!("expected ACP model info, got {other:?}"),
    }
}

#[test]
fn history_replay_emits_progressive_and_final_snapshots() {
    let (stream_tx, mut stream_rx) = broadcast::channel(64);
    let shared = Arc::new(AcpShared::new(
        "s1".into(),
        PathBuf::from("/tmp"),
        ProcessId::new(),
        stream_tx,
        Arc::new(tokio::sync::Mutex::new(ProcessManager::default())),
        Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    ));
    shared.begin_history_replay();

    project_session_update(
        &shared,
        SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            "hello",
        )))),
    );
    let ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { messages_json, .. }) =
        stream_rx.try_recv().expect("first progressive snapshot")
    else {
        panic!("expected snapshot");
    };
    let messages: Vec<crate::message::Message> = serde_json::from_str(&messages_json).unwrap();
    assert_eq!(messages.len(), 1);
    for _ in 0..300 {
        project_session_update(
            &shared,
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new("x"),
            ))),
        );
    }

    let snapshots: Vec<ServiceMessage> = std::iter::from_fn(|| stream_rx.try_recv().ok()).collect();
    assert!(snapshots.len() > 1);
    assert!(snapshots.len() < 64);
    shared.finish_history_replay(true);

    let ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { messages_json, .. }) =
        stream_rx.try_recv().expect("final snapshot")
    else {
        panic!("expected snapshot");
    };
    let messages: Vec<crate::message::Message> = serde_json::from_str(&messages_json).unwrap();
    assert_eq!(messages.len(), 2);
    assert!(matches!(
        &messages[1],
        crate::message::Message::Assistant { blocks }
            if matches!(blocks.as_slice(), [crate::message::AssistantBlock::Text(text)] if text.len() == 300)
    ));
    assert!(matches!(
        stream_rx.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
}

#[test]
fn failed_history_replay_discards_partial_transcript() {
    let (stream_tx, mut stream_rx) = broadcast::channel(64);
    let shared = Arc::new(AcpShared::new(
        "s1".into(),
        PathBuf::from("/tmp"),
        ProcessId::new(),
        stream_tx,
        Arc::new(tokio::sync::Mutex::new(ProcessManager::default())),
        Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    ));
    shared.begin_history_replay();
    project_session_update(
        &shared,
        SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            "partial",
        )))),
    );

    let ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { messages_json, .. }) =
        stream_rx.try_recv().expect("progressive snapshot")
    else {
        panic!("expected snapshot");
    };
    let messages: Vec<crate::message::Message> = serde_json::from_str(&messages_json).unwrap();
    assert_eq!(messages.len(), 1);

    shared.finish_history_replay(false);

    assert!(shared.projector.lock().unwrap().messages().is_empty());
    let ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { messages_json, .. }) =
        stream_rx.try_recv().expect("clearing snapshot")
    else {
        panic!("expected snapshot");
    };
    let messages: Vec<crate::message::Message> = serde_json::from_str(&messages_json).unwrap();
    assert!(messages.is_empty());
    assert!(matches!(
        stream_rx.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
}

#[test]
fn approval_details_fall_back_to_projected_tool_call() {
    let mut projector = AcpProjector::new();
    projector.apply(agent_client_protocol::schema::v1::SessionUpdate::ToolCall(
        ToolCall::new("call-1", "vmux.run")
            .raw_input(serde_json::json!({"command": "echo hi", "focus": true})),
    ));
    let request = RequestPermissionRequest::new(
        "session-1",
        agent_client_protocol::schema::v1::ToolCallUpdate::new(
            "call-1",
            ToolCallUpdateFields::new(),
        ),
        Vec::new(),
    );

    assert_eq!(
        approval_details(&request, &projector),
        Some((
            "vmux.run".to_string(),
            r#"{"command":"echo hi","focus":true}"#.to_string(),
        ))
    );
}

#[test]
fn approval_details_prefer_permission_request_fields() {
    let mut projector = AcpProjector::new();
    projector.apply(agent_client_protocol::schema::v1::SessionUpdate::ToolCall(
        ToolCall::new("call-1", "old").raw_input(serde_json::json!({"command": "old"})),
    ));
    let request = RequestPermissionRequest::new(
        "session-1",
        agent_client_protocol::schema::v1::ToolCallUpdate::new(
            "call-1",
            ToolCallUpdateFields::new()
                .title("new")
                .raw_input(serde_json::json!({"command": "new"})),
        ),
        Vec::new(),
    );

    assert_eq!(
        approval_details(&request, &projector),
        Some(("new".to_string(), r#"{"command":"new"}"#.to_string(),))
    );
}

#[test]
fn approval_details_reject_missing_tool_identity() {
    let request = RequestPermissionRequest::new(
        "session-1",
        agent_client_protocol::schema::v1::ToolCallUpdate::new(
            "call-1",
            ToolCallUpdateFields::new(),
        ),
        Vec::new(),
    );

    assert_eq!(approval_details(&request, &AcpProjector::new()), None);
}

#[test]
fn approval_details_use_kind_when_request_has_arguments_but_no_title() {
    let request = RequestPermissionRequest::new(
        "session-1",
        agent_client_protocol::schema::v1::ToolCallUpdate::new(
            "call-1",
            ToolCallUpdateFields::new()
                .kind(ToolKind::Execute)
                .raw_input(serde_json::json!({"command": "echo hi"})),
        ),
        Vec::new(),
    );

    assert_eq!(
        approval_details_from_kind(&request),
        Some((
            "Execute command".to_string(),
            r#"{"command":"echo hi"}"#.to_string(),
        ))
    );
}

#[tokio::test]
async fn approval_details_wait_for_preceding_tool_call_projection() {
    let (stream_tx, _) = broadcast::channel(2);
    let shared = Arc::new(AcpShared::new(
        "s1".into(),
        PathBuf::from("/tmp"),
        ProcessId::new(),
        stream_tx,
        Arc::new(tokio::sync::Mutex::new(ProcessManager::default())),
        Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    ));
    let request = RequestPermissionRequest::new(
        "session-1",
        agent_client_protocol::schema::v1::ToolCallUpdate::new(
            "call-1",
            ToolCallUpdateFields::new()
                .kind(ToolKind::Execute)
                .raw_input(serde_json::json!({"command": "echo hi"})),
        ),
        Vec::new(),
    );
    let waiting = {
        let shared = Arc::clone(&shared);
        tokio::spawn(async move { resolve_approval_details(&request, &shared).await })
    };

    tokio::task::yield_now().await;
    project_session_update(
        &shared,
        SessionUpdate::ToolCall(
            ToolCall::new("call-1", "vmux.run")
                .raw_input(serde_json::json!({"command": "echo hi"})),
        ),
    );

    assert_eq!(
        waiting.await.unwrap(),
        Some((
            "vmux.run".to_string(),
            r#"{"command":"echo hi"}"#.to_string(),
        ))
    );
}

#[tokio::test]
async fn conversation_title_permission_resolves_as_host_owned_tool() {
    let (stream_tx, _) = broadcast::channel(2);
    let shared = AcpShared::new(
        "s1".into(),
        PathBuf::from("/tmp"),
        ProcessId::new(),
        stream_tx,
        Arc::new(tokio::sync::Mutex::new(ProcessManager::default())),
        Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    );
    project_session_update(
        &shared,
        SessionUpdate::ToolCall(
            ToolCall::new("title-1", "mcp__vmux__set_conversation_title")
                .raw_input(serde_json::json!({"title": "Paris Izakaya Website"})),
        ),
    );
    let request = RequestPermissionRequest::new(
        "session-1",
        agent_client_protocol::schema::v1::ToolCallUpdate::new(
            "title-1",
            ToolCallUpdateFields::new()
                .kind(ToolKind::Execute)
                .raw_input(serde_json::json!({"title": "Paris Izakaya Website"})),
        ),
        Vec::new(),
    );

    let (name, _) = resolve_approval_details(&request, &shared).await.unwrap();
    assert!(is_conversation_title_tool(&name));
}

#[test]
fn native_choice_tool_is_always_permissionless() {
    for name in [
        "mcp__vmux__request_user_choice",
        "mcp.vmux.request_user_choice",
        "vmux:request-user-choice",
        "request_user_choice",
    ] {
        assert!(is_permissionless_host_tool(name), "{name}");
    }
    assert!(!is_permissionless_host_tool("other_request_user_choice"));
}

#[test]
fn knowledge_read_tools_are_always_permissionless() {
    for name in [
        "mcp__vmux__search_knowledge",
        "mcp.vmux.read_knowledge",
        "vmux:search-knowledge",
        "read_knowledge",
    ] {
        assert!(is_permissionless_host_tool(name), "{name}");
    }
    assert!(!is_permissionless_host_tool("write_knowledge"));
    assert!(!is_permissionless_host_tool("other_search_knowledge"));
}

#[tokio::test]
async fn requested_resume_loads_only_when_supported() {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let loaded = load_requested_session(Some("resume-1".into()), true, |sid| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async move {
            assert_eq!(sid.to_string(), "resume-1");
            Ok::<(), ()>(())
        }
    })
    .await;
    assert_eq!(loaded.unwrap().to_string(), "resume-1");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

    let skipped = load_requested_session(Some("resume-2".into()), false, |_| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async { Ok::<(), ()>(()) }
    })
    .await;
    assert!(skipped.is_none());
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_requested_resume_stays_unassigned() {
    let loaded = load_requested_session(Some("stale".into()), true, |_| async {
        Err::<(), &'static str>("missing")
    })
    .await;
    assert!(loaded.is_none());
}

#[tokio::test]
async fn ensure_session_creates_once_then_reuses_id() {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let mut session_id = None;
    let (created_id, created) = ensure_session(&mut session_id, || {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async { Ok::<SessionId, ()>(SessionId::new("created")) }
    })
    .await
    .unwrap();
    assert!(created);
    assert_eq!(created_id.to_string(), "created");

    let (reused_id, created) = ensure_session(&mut session_id, || {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async { Ok::<SessionId, ()>(SessionId::new("unexpected")) }
    })
    .await
    .unwrap();
    assert!(!created);
    assert_eq!(reused_id.to_string(), "created");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_session_creation_remains_retryable() {
    let mut session_id = None;
    let result = ensure_session(&mut session_id, || async {
        Err::<SessionId, &'static str>("create failed")
    })
    .await;
    assert_eq!(result.unwrap_err(), "create failed");
    assert!(session_id.is_none());
}

#[test]
fn status_after_prompt_cancel_wins() {
    assert_eq!(status_after_prompt(false, None), AgentRunStatus::Idle);
    assert_eq!(
        status_after_prompt(false, Some("boom".into())),
        AgentRunStatus::Errored("boom".into())
    );
    assert_eq!(status_after_prompt(true, None), AgentRunStatus::Interrupted);
    assert_eq!(
        status_after_prompt(true, Some("boom".into())),
        AgentRunStatus::Interrupted
    );
}

#[test]
fn private_context_wraps_wire_prompt_without_changing_display_text() {
    let wire = compose_agent_prompt("continue here", Some("prior conversation"));

    assert!(wire.starts_with(crate::protocol::PRIVATE_CONTEXT_PREFIX));
    assert!(wire.contains("prior conversation"));
    assert!(wire.ends_with("continue here"));
    assert_eq!(compose_agent_prompt("plain", None), "plain");
}

#[test]
fn claude_acp_disables_native_shell_and_steers_skill_continuation() {
    let meta = session_meta_for_agent_with_knowledge("claude", "memory context", Some("high"))
        .expect("Claude ACP metadata");
    let options = &meta["claudeCode"]["options"];

    assert_eq!(options["effort"], "high");
    assert_eq!(
        options["disallowedTools"],
        serde_json::json!(["Bash", "Monitor", "WebSearch", "WebFetch"])
    );
    assert!(
        options["allowedTools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool == "mcp__vmux__run")
    );
    assert!(
        options["allowedTools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool == "mcp__vmux__set_conversation_title")
    );
    let prompt = meta["systemPrompt"]["append"].as_str().unwrap();
    assert!(prompt.contains("mcp__vmux__run"));
    assert!(prompt.contains("continue the original user request"));
    assert!(prompt.contains("memory context"));
    assert!(prompt.contains("mcp__vmux__set_conversation_title"));
    let unset = session_meta_for_agent_with_knowledge("claude", "memory context", None)
        .expect("Claude ACP metadata");
    assert!(unset["claudeCode"]["options"].get("effort").is_none());
    let bogus = session_meta_for_agent_with_knowledge("claude", "memory context", Some("turbo"))
        .expect("Claude ACP metadata");
    assert!(bogus["claudeCode"]["options"].get("effort").is_none());
    let generic = session_meta_for_agent_with_knowledge("vibe-acp", "skill context", None).unwrap()
        ["systemPrompt"]["append"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(generic.starts_with("skill context\n\n"));
    assert!(generic.contains("mcp__vmux__set_conversation_title"));
    assert!(generic.contains("first tool of the turn"));
    assert!(generic.contains("raw first prompt as a provisional title"));
    assert!(generic.contains("topic materially changes"));
    assert!(generic.contains("same-topic follow-ups"));
    assert!(generic.contains("corrected spelling and grammar"));
    assert!(generic.contains("Never copy the user's prompt verbatim"));
    assert!(generic.contains("never needs user permission"));
}

#[test]
fn pick_permission_option_preserves_decision_scope() {
    let opts = vec![
        opt("once", PermissionOptionKind::AllowOnce),
        opt("always", PermissionOptionKind::AllowAlways),
        opt("rej", PermissionOptionKind::RejectOnce),
    ];
    assert_eq!(
        pick_permission_option(&opts, ApprovalDecision::Allow)
            .unwrap()
            .to_string(),
        "once"
    );
    assert_eq!(
        pick_permission_option(&opts, ApprovalDecision::AllowAlways)
            .unwrap()
            .to_string(),
        "always"
    );
    assert_eq!(
        pick_permission_option(&opts, ApprovalDecision::Deny)
            .unwrap()
            .to_string(),
        "rej"
    );

    let always_only = vec![
        opt("aa", PermissionOptionKind::AllowAlways),
        opt("ra", PermissionOptionKind::RejectAlways),
    ];
    assert_eq!(
        pick_permission_option(&always_only, ApprovalDecision::Allow)
            .unwrap()
            .to_string(),
        "aa"
    );
    assert_eq!(
        pick_permission_option(
            &[opt("once", PermissionOptionKind::AllowOnce)],
            ApprovalDecision::AllowAlways,
        ),
        None
    );
}

#[test]
fn resolve_in_cwd_rejects_escape() {
    let cwd = std::path::Path::new("/work");
    assert_eq!(
        resolve_in_cwd(cwd, std::path::Path::new("/work/a.rs")),
        Some(PathBuf::from("/work/a.rs"))
    );
    assert!(resolve_in_cwd(cwd, std::path::Path::new("/etc/passwd")).is_none());
    assert!(resolve_in_cwd(cwd, std::path::Path::new("/work/../etc/passwd")).is_none());
}

#[test]
fn vibe_fs_scope_allows_only_process_temp_root() {
    let temp_root = tempfile::tempdir().unwrap();
    let scratchpad = temp_root.path().join("vibe-scratchpad-cafebabe-runtime");
    std::fs::create_dir_all(&scratchpad).unwrap();
    let scope = AcpFsScope {
        cwd: PathBuf::from("/work"),
        vibe_temp_root: Some(temp_root.path().to_path_buf()),
    };

    assert!(resolve_acp_fs_path(&scope, &scratchpad.join("test.nu")).is_some());
    assert!(
        resolve_acp_fs_path(
            &scope,
            &scratchpad.canonicalize().unwrap().join("canonical.nu")
        )
        .is_some()
    );
    assert!(
        resolve_acp_fs_path(
            &scope,
            &std::env::temp_dir().join("vibe-scratchpad-deadbeef-foreign/test.nu")
        )
        .is_none()
    );
    assert!(resolve_acp_fs_path(&scope, std::path::Path::new("/etc/passwd")).is_none());
}

#[cfg(unix)]
#[test]
fn vibe_fs_scope_rejects_scratchpad_symlink_outside_process_root() {
    use std::os::unix::fs::symlink;

    let temp_root = tempfile::tempdir().unwrap();
    let foreign_root = tempfile::tempdir().unwrap();
    let target = foreign_root.path().join("vibe-scratchpad-cafebabe-target");
    let alias = temp_root.path().join("vibe-scratchpad-deadbeef-alias");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("secret.nu"), "secret").unwrap();
    symlink(&target, &alias).unwrap();

    assert!(resolve_vibe_scratchpad(temp_root.path(), &alias.join("secret.nu")).is_none());

    std::fs::remove_file(alias).unwrap();
}

#[test]
fn slice_lines_honors_line_and_limit() {
    let text = "a\nb\nc\nd";
    assert_eq!(slice_lines(text, None, None), "a\nb\nc\nd");
    assert_eq!(slice_lines(text, Some(2), None), "b\nc\nd");
    assert_eq!(slice_lines(text, Some(2), Some(2)), "b\nc");
    assert_eq!(slice_lines(text, Some(10), Some(2)), "");
}

fn test_shared(
    manager: Arc<tokio::sync::Mutex<ProcessManager>>,
) -> (Arc<AcpShared>, broadcast::Receiver<ServiceMessage>) {
    let (stream_tx, stream_rx) = broadcast::channel(64);
    let shared = Arc::new(AcpShared::new(
        "s1".to_string(),
        std::env::temp_dir(),
        ProcessId::new(),
        stream_tx,
        manager,
        Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    ));
    (shared, stream_rx)
}

#[test]
fn explicit_workspace_rebind_updates_host_file_scope() {
    let target = tempfile::tempdir().unwrap();
    let (shared, _) = test_shared(Arc::new(tokio::sync::Mutex::new(ProcessManager::default())));

    shared.rebind_cwd(target.path().to_path_buf()).unwrap();

    assert_eq!(shared.cwd(), target.path().canonicalize().unwrap());
}

#[test]
fn approval_resolution_is_broadcast_immediately() {
    let (shared, mut receiver) =
        test_shared(Arc::new(tokio::sync::Mutex::new(ProcessManager::default())));
    *shared.approval.lock().unwrap() = Some(RemoteApproval {
        call_id: "call-1".into(),
        name: "run".into(),
        args_json: "{}".into(),
    });

    assert!(shared.resolve_approval("call-1"));
    assert!(matches!(
        receiver.try_recv(),
        Ok(ServiceMessage::Shared(SharedEvent::AgentApprovalResolved { sid, call_id }))
            if sid == "s1" && call_id == "call-1"
    ));
    assert!(shared.approval.lock().unwrap().is_none());
}

#[test]
fn workspace_change_rebinds_runtime_file_operations() {
    let original = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .current_dir(original.path())
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "t@example.com"]);
    git(&["config", "user.name", "Test"]);
    git(&["config", "commit.gpgsign", "false"]);
    let original_file = original.path().join("original.txt");
    std::fs::write(&original_file, "original").unwrap();
    git(&["add", "original.txt"]);
    git(&["commit", "-qm", "init"]);
    let worktree_parent = tempfile::tempdir().unwrap();
    let worktree = worktree_parent.path().join("quiet-amber-wolf");
    git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "vibe/quiet-amber-wolf",
        worktree.to_str().unwrap(),
        "main",
    ]);
    let worktree_file = worktree.join("original.txt");
    let original_file = original_file.canonicalize().unwrap();
    let worktree_file = worktree_file.canonicalize().unwrap();
    let (stream_tx, _stream_rx) = broadcast::channel(4);
    let shared = AcpShared::new(
        "s1".into(),
        original.path().canonicalize().unwrap(),
        ProcessId::new(),
        stream_tx,
        Arc::new(tokio::sync::Mutex::new(ProcessManager::default())),
        Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    );

    shared.publish_workspace_change(
        "quiet-amber-wolf",
        "vibe/quiet-amber-wolf",
        worktree.to_str().unwrap(),
        original.path().to_str().unwrap(),
    );

    let worktree = worktree.canonicalize().unwrap();
    assert_eq!(shared.cwd(), worktree);
    let scope = AcpFsScope {
        cwd: shared.cwd(),
        vibe_temp_root: None,
    };
    assert_eq!(
        read_text_file(&scope, &ReadTextFileRequest::new("s1", &worktree_file)),
        Ok("original".into())
    );
    assert_eq!(
        read_text_file(&scope, &ReadTextFileRequest::new("s1", &original_file)),
        Err("path outside session cwd".into())
    );

    let arbitrary = tempfile::tempdir().unwrap();
    shared.publish_workspace_change(
        "malicious",
        "vibe/quiet-amber-wolf",
        arbitrary.path().to_str().unwrap(),
        original.path().to_str().unwrap(),
    );

    assert_eq!(shared.cwd(), worktree);
}

/// A lagged broadcast drops messages, and ProcessExited is only ever sent once. Treating a lag
/// as "keep waiting" leaves terminal/wait_for_exit blocked forever on a command that already
/// finished, so the manager's recorded code has to win.
#[test]
fn a_dropped_exit_is_recovered_from_the_manager() {
    assert!(matches!(
        exit_after_lag(Some(Some(7))),
        Some(AcpTerminalExit::Exited(Some(7)))
    ));
    assert!(matches!(
        exit_after_lag(None),
        Some(AcpTerminalExit::Removed)
    ));

    assert!(
        exit_after_lag(Some(None)).is_none(),
        "a child that is still running has an exit still to come, so waiting is correct"
    );
}

/// End-to-end of the daemon terminal path: `terminal/create` spawns a real PTY + emits
/// `AcpTerminalCreated`; `wait_for_exit` resolves with the child's code; `output` reads the
/// completed command's text after exit (kept alive); `release` stops tracking it.
#[tokio::test]
async fn acp_terminal_create_wait_output_release() {
    let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
    let (shared, mut stream_rx) = test_shared(manager.clone());

    // Drive PTY output + exit detection like the server poll loop (which keeps ACP terminals).
    let poll_mgr = manager.clone();
    let poll = tokio::spawn(async move {
        loop {
            poll_mgr.lock().await.poll_all();
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    });

    let req = CreateTerminalRequest::new("s1", "/bin/sh").args(vec![
        "-c".to_string(),
        "printf hi; sleep 0.1; exit 7".to_string(),
    ]);
    let created = create_terminal(&shared, req).await.expect("create");
    let tid = created.terminal_id.0.to_string();
    assert!(shared.terminals.lock().unwrap().contains_key(&tid));

    let (emitted_id, emitted_pid) = loop {
        match stream_rx.recv().await.expect("stream open") {
            ServiceMessage::AcpTerminalCreated {
                terminal_id,
                process_id,
                ..
            } => break (terminal_id, process_id),
            _ => continue,
        }
    };
    assert_eq!(emitted_id, tid);
    assert_eq!(emitted_pid.to_string(), tid);

    let wait = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        wait_for_terminal_exit(
            &shared,
            WaitForTerminalExitRequest::new("s1", TerminalId::new(tid.clone())),
        ),
    )
    .await
    .expect("wait_for_exit timed out")
    .expect("wait_for_exit");
    assert_eq!(wait.exit_status.exit_code, Some(7));

    let out = terminal_output(
        &shared,
        TerminalOutputRequest::new("s1", TerminalId::new(tid.clone())),
    )
    .await
    .expect("output");
    assert!(out.output.contains("hi"), "output was {:?}", out.output);
    assert_eq!(out.exit_status.and_then(|status| status.exit_code), Some(7));

    release_terminal(
        &shared,
        ReleaseTerminalRequest::new("s1", TerminalId::new(tid.clone())),
    )
    .await
    .expect("release");
    assert!(!shared.terminals.lock().unwrap().contains_key(&tid));

    poll.abort();
}

#[tokio::test]
async fn terminal_output_unknown_terminal_errors() {
    let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
    let (shared, _rx) = test_shared(manager);
    let result = terminal_output(&shared, TerminalOutputRequest::new("s1", "does-not-exist")).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_terminal_rejects_nonexistent_cwd() {
    let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
    let (shared, _rx) = test_shared(manager.clone());
    let cwd = std::env::temp_dir().join(format!(
        "vmux-acp-missing-cwd-{}-{}",
        std::process::id(),
        ProcessId::new()
    ));

    let result = create_terminal(
        &shared,
        CreateTerminalRequest::new("s1", "/bin/sh").cwd(cwd),
    )
    .await;

    assert!(result.is_err());
    assert!(manager.lock().await.processes.is_empty());
}

#[tokio::test]
async fn create_terminal_rejects_relative_cwd() {
    let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
    let (shared, _rx) = test_shared(manager.clone());

    let result = create_terminal(
        &shared,
        CreateTerminalRequest::new("s1", "/bin/sh").cwd("."),
    )
    .await;

    assert!(result.is_err());
    assert!(manager.lock().await.processes.is_empty());
}

#[tokio::test]
async fn removed_terminal_errors_for_wait_and_output() {
    let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
    let (shared, _rx) = test_shared(manager.clone());
    let created = create_terminal(
        &shared,
        CreateTerminalRequest::new("s1", "/bin/sh")
            .args(vec!["-c".to_string(), "sleep 30".to_string()]),
    )
    .await
    .expect("create");
    let terminal_id = created.terminal_id.0.to_string();
    let process_id = terminal_id.parse().expect("process id");
    manager.lock().await.remove_process(&process_id);

    let wait = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        wait_for_terminal_exit(
            &shared,
            WaitForTerminalExitRequest::new("s1", TerminalId::new(terminal_id.clone())),
        ),
    )
    .await
    .expect("wait timeout");
    assert!(wait.is_err());

    let output = terminal_output(
        &shared,
        TerminalOutputRequest::new("s1", TerminalId::new(terminal_id.clone())),
    )
    .await;
    assert!(output.is_err());
    shared.terminals.lock().unwrap().remove(&terminal_id);
}

#[tokio::test]
async fn release_terminal_kills_running_command() {
    let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
    let (shared, _rx) = test_shared(manager.clone());
    let created = create_terminal(
        &shared,
        CreateTerminalRequest::new("s1", "/bin/sh")
            .args(vec!["-c".to_string(), "sleep 30".to_string()]),
    )
    .await
    .expect("create");
    let terminal_id = created.terminal_id.0.to_string();
    let process_id = terminal_id.parse().expect("process id");

    release_terminal(
        &shared,
        ReleaseTerminalRequest::new("s1", TerminalId::new(terminal_id)),
    )
    .await
    .expect("release");

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    let exited = loop {
        let exited = {
            let mut manager = manager.lock().await;
            manager.poll_all();
            manager
                .processes
                .get(&process_id)
                .and_then(|process| process.process_exit())
                .is_some()
        };
        if exited || std::time::Instant::now() >= deadline {
            break exited;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    if !exited {
        manager.lock().await.remove_process(&process_id);
    }

    assert!(exited, "release must kill a running terminal command");
}

#[tokio::test]
async fn terminal_output_respects_byte_limit_at_char_boundary() {
    let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
    let (shared, _rx) = test_shared(manager.clone());
    let poll_manager = manager.clone();
    let poll = tokio::spawn(async move {
        loop {
            poll_manager.lock().await.poll_all();
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    });
    let created = create_terminal(
        &shared,
        CreateTerminalRequest::new("s1", "/bin/sh")
            .args(vec!["-c".to_string(), "printf 'abécd'".to_string()])
            .output_byte_limit(3),
    )
    .await
    .expect("create");
    let terminal_id = created.terminal_id.0.to_string();

    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        wait_for_terminal_exit(
            &shared,
            WaitForTerminalExitRequest::new("s1", TerminalId::new(terminal_id.clone())),
        ),
    )
    .await
    .expect("wait timeout")
    .expect("wait");
    let output = terminal_output(
        &shared,
        TerminalOutputRequest::new("s1", TerminalId::new(terminal_id.clone())),
    )
    .await
    .expect("output");

    assert_eq!(output.output, "cd");
    assert!(output.truncated);

    release_terminal(
        &shared,
        ReleaseTerminalRequest::new("s1", TerminalId::new(terminal_id)),
    )
    .await
    .expect("release");
    poll.abort();
}
