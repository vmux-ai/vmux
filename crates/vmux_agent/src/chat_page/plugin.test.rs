use super::*;
use std::path::Path;

#[test]
fn streaming_snapshots_wait_for_frame_interval() {
    assert!(!chat_snapshot_due(
        true,
        false,
        Some(CHAT_STREAM_PUSH_INTERVAL - std::time::Duration::from_millis(1)),
    ));
    assert!(chat_snapshot_due(
        true,
        false,
        Some(CHAT_STREAM_PUSH_INTERVAL),
    ));
}

#[test]
fn state_changes_and_completed_turns_push_immediately() {
    assert!(chat_snapshot_due(
        true,
        true,
        Some(std::time::Duration::ZERO)
    ));
    assert!(chat_snapshot_due(
        false,
        false,
        Some(std::time::Duration::ZERO),
    ));
}

#[test]
fn media_query_paths_decode_percent_escapes() {
    assert_eq!(
        decode_media_query_path("Pictures/My%20Image%25.png"),
        std::path::PathBuf::from("Pictures/My Image%.png")
    );
}

#[test]
fn media_thumbnail_is_small_png_data_url() {
    let path =
        std::env::temp_dir().join(format!("vmux-media-thumbnail-{}.png", uuid::Uuid::new_v4()));
    let image = image::RgbaImage::from_pixel(240, 120, image::Rgba([20, 40, 60, 255]));
    image.save(&path).unwrap();
    let source_size = std::fs::metadata(&path).unwrap().len();

    let data_url = media_thumbnail_data_url(&path, source_size);

    std::fs::remove_file(path).unwrap();
    let encoded = data_url.strip_prefix("data:image/png;base64,").unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    let thumbnail = image::load_from_memory(&bytes).unwrap();
    assert_eq!(thumbnail.width().max(thumbnail.height()), 96);
}

#[test]
fn clipboard_tiff_is_converted_to_png() {
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        4,
        3,
        image::Rgba([20, 40, 60, 255]),
    ));
    let mut tiff = std::io::Cursor::new(Vec::new());
    image.write_to(&mut tiff, image::ImageFormat::Tiff).unwrap();

    let png = tiff_to_png(&tiff.into_inner()).unwrap();
    let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap();

    assert_eq!((decoded.width(), decoded.height()), (4, 3));
}

#[test]
fn runtime_switch_builtin_acp_agents_to_cli() {
    let cases = [
        ("claude", "claude"),
        ("claude-acp", "claude"),
        ("codex", "codex"),
        ("codex-acp", "codex"),
        ("vibe", "vibe"),
        ("mistral-vibe", "vibe"),
    ];
    let ids = cases
        .iter()
        .map(|(id, _)| (*id).to_string())
        .collect::<Vec<_>>();
    for (agent_id, cli_segment) in cases {
        let got = runtime_switch_target(agent_id, Some("sid-9"), Path::new("/w"), "cli", &ids);
        assert_eq!(
            got,
            Some((
                format!("vmux://agent/{cli_segment}/cli/sid-9"),
                std::path::PathBuf::from("/w")
            ))
        );
    }
}

#[test]
fn runtime_switch_requires_session_id() {
    let ids = vec!["claude".to_string()];
    assert_eq!(
        runtime_switch_target("claude", None, Path::new("/w"), "cli", &ids),
        None
    );
}

#[test]
fn runtime_switch_gated_for_unknown_agent() {
    let ids = vec!["claude".to_string()];
    assert_eq!(
        runtime_switch_target("custom", Some("s"), Path::new("/w"), "cli", &ids),
        None
    );
}

#[test]
fn slash_commands_include_cli_only_when_cross_runtime() {
    let base = slash_commands_for(false, false);
    assert_eq!(base.len(), 2);
    assert_eq!(base[0].name, "upload");
    let with_model = slash_commands_for(false, true);
    assert_eq!(with_model.len(), 3);
    assert_eq!(with_model[2].name, "model");
    let with_cli = slash_commands_for(true, false);
    assert_eq!(with_cli.len(), 3);
    assert_eq!(with_cli[2].name, "cli");
}

#[test]
fn model_selection_updates_cached_state_before_response() {
    let mut app = App::new();
    app.init_resource::<AcpModelRequestCounter>()
        .init_resource::<LastUsedAcpModels>()
        .add_message::<AcpSetModelRequest>()
        .add_observer(on_select_model);
    let stack = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "claude".into(),
                sid: "s1".into(),
                cwd: "/tmp".into(),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            AcpModelState {
                config_id: "model".into(),
                current_model_id: "default".into(),
                pending: None,
                models: vec![
                    vmux_service::protocol::AcpModelOption {
                        id: "default".into(),
                        name: "Default".into(),
                        description: None,
                    },
                    vmux_service::protocol::AcpModelOption {
                        id: "fable".into(),
                        name: "Fable".into(),
                        description: None,
                    },
                ],
            },
        ))
        .id();
    let webview = app.world_mut().spawn(ChildOf(stack)).id();

    app.world_mut().trigger(BinReceive {
        webview,
        payload: SelectModel {
            model_id: "fable".into(),
        },
    });

    let state = app.world().get::<AcpModelState>(stack).unwrap();
    assert_eq!(state.current_model_id, "default");
    assert_eq!(
        state.pending.as_ref().map(|pending| pending.request_id),
        Some(1)
    );
    assert_eq!(
        state
            .pending
            .as_ref()
            .map(|pending| pending.model_id.as_str()),
        Some("fable")
    );
    assert_eq!(state.current_name(), "Fable");
    let requests: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<AcpSetModelRequest>>()
        .drain()
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].sid, "s1");
    assert_eq!(requests[0].request_id, 1);
    assert_eq!(requests[0].config_id, "model");
    assert_eq!(requests[0].model_id, "fable");
    assert_eq!(
        app.world()
            .resource::<LastUsedAcpModels>()
            .by_agent
            .get("claude")
            .map(String::as_str),
        Some("fable")
    );

    app.world_mut().trigger(BinReceive {
        webview,
        payload: SelectModel {
            model_id: "fable".into(),
        },
    });
    app.world_mut().trigger(BinReceive {
        webview,
        payload: SelectModel {
            model_id: "missing".into(),
        },
    });
    assert_eq!(
        app.world_mut()
            .resource_mut::<Messages<AcpSetModelRequest>>()
            .drain()
            .count(),
        0
    );
}

#[test]
fn fresh_agent_session_applies_last_used_model() {
    let mut app = App::new();
    app.init_resource::<AcpModelRequestCounter>()
        .init_resource::<LastUsedAcpModels>()
        .add_message::<AcpSetModelRequest>()
        .add_systems(Update, apply_last_used_acp_model);
    app.world_mut()
        .resource_mut::<LastUsedAcpModels>()
        .by_agent
        .insert("claude".into(), "fable".into());
    let stack = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "claude".into(),
                sid: "s2".into(),
                cwd: "/tmp".into(),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            AcpModelState {
                config_id: "model".into(),
                current_model_id: "default".into(),
                pending: None,
                models: vec![
                    vmux_service::protocol::AcpModelOption {
                        id: "default".into(),
                        name: "Default".into(),
                        description: None,
                    },
                    vmux_service::protocol::AcpModelOption {
                        id: "fable".into(),
                        name: "Fable".into(),
                        description: None,
                    },
                ],
            },
        ))
        .id();

    app.update();

    let state = app.world().get::<AcpModelState>(stack).unwrap();
    assert_eq!(state.display_model_id(), "fable");
    let requests: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<AcpSetModelRequest>>()
        .drain()
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].sid, "s2");
    assert_eq!(requests[0].model_id, "fable");
}

#[test]
fn composer_workspace_controls_dispatch_for_current_session() {
    let mut app = App::new();
    app.add_message::<AgentCommandRequest>()
        .add_observer(on_chat_select_workspace)
        .add_observer(on_chat_create_worktree);
    let anchor = vmux_core::ProcessId::new();
    let stack = app
        .world_mut()
        .spawn(AcpSession {
            agent_id: "claude".into(),
            sid: "s1".into(),
            cwd: "/tmp".into(),
            anchor,
            resume: None,
        })
        .id();
    let webview = app.world_mut().spawn(ChildOf(stack)).id();

    app.world_mut().trigger(BinReceive {
        webview,
        payload: ChatSelectWorkspace,
    });
    app.world_mut().trigger(BinReceive {
        webview,
        payload: ChatCreateWorktree,
    });

    let requests = app
        .world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 2);
    assert!(matches!(requests[0].origin, CommandOrigin::User));
    assert!(matches!(
        requests[0].command,
        ServiceAgentCommand::ChooseWorkspace { anchor: got } if got == anchor
    ));
    assert!(matches!(
        requests[1].command,
        ServiceAgentCommand::CreateWorktree { anchor: got } if got == anchor
    ));
}

#[test]
fn resume_results_include_all_agent_kinds_with_source_labels() {
    use crate::client::cli::strategy::ResumableSession;
    use std::time::SystemTime;

    let session = |kind, sid: &str| ResumableSession {
        kind,
        sid: sid.into(),
        cwd: "/work".into(),
        mtime: SystemTime::UNIX_EPOCH,
        title: sid.into(),
        cross_runtime: kind_supports_cross_runtime(kind),
    };
    let entries = resume_entries(
        vec![
            session(AgentKind::Claude, "claude-1"),
            session(AgentKind::Codex, "codex-1"),
        ],
        Some(AgentKind::Claude),
        "Antigravity",
    );
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].agent_name, "Antigravity");
    assert_eq!(entries[1].agent_name, "Codex");
}

#[test]
fn foreign_resume_keeps_active_acp_agent_fresh() {
    assert_eq!(
        foreign_handoff_target("claude", Some(AgentKind::Claude), AgentKind::Codex,),
        Some("vmux://agent/claude".to_string())
    );
    assert_eq!(
        foreign_handoff_target("claude", Some(AgentKind::Claude), AgentKind::Claude,),
        None
    );
    assert_eq!(
        foreign_handoff_target("custom-acp", None, AgentKind::Codex),
        Some("vmux://agent/custom-acp".to_string())
    );
}

#[test]
fn snapshot_reports_grouped_imported_item_boundary() {
    let imported = ImportedConversation {
        source_agent: "Codex".into(),
        source_kind: AgentKind::Codex,
        source_sid: "codex-1".into(),
        messages: vec![
            crate::Message::user("one"),
            crate::Message::Assistant {
                blocks: vec![crate::AssistantBlock::ToolUse {
                    call_id: "call-1".into(),
                    name: "run".into(),
                    args: "{}".into(),
                    parent_call_id: None,
                }],
            },
            crate::Message::ToolResult {
                call_id: "call-1".into(),
                content: "two".into(),
                is_error: false,
            },
        ],
        truncated: false,
        first_prompt: None,
    };
    let snapshot = snapshot_of(
        &AgentMessages::default(),
        &AgentRunState::Idle,
        None,
        None,
        None,
        &PromptQueue::default(),
        Some(&imported),
        None,
        None,
    );

    assert_eq!(snapshot.handoff_message_count, 2);
}

#[test]
fn snapshot_includes_approval_tool_and_input() {
    let snapshot = snapshot_of(
        &AgentMessages::default(),
        &AgentRunState::AwaitingApproval {
            call_id: "call-1".into(),
            name: "vmux.run".into(),
            args: serde_json::json!({"command": "echo hi", "focus": true}),
        },
        None,
        None,
        None,
        &PromptQueue::default(),
        None,
        None,
        None,
    );

    assert_eq!(snapshot.approval_name, "vmux.run");
    assert_eq!(
        snapshot.approval_args_json,
        r#"{"command":"echo hi","focus":true}"#
    );
}

#[test]
fn snapshot_includes_model_written_conversation_title() {
    let title = AgentConversationTitle("Refine generated chat summaries".into());
    let snapshot = snapshot_of(
        &AgentMessages::default(),
        &AgentRunState::Idle,
        None,
        None,
        None,
        &PromptQueue::default(),
        None,
        Some(&title),
        None,
    );

    assert_eq!(
        snapshot.conversation_title,
        "Refine generated chat summaries"
    );
}

#[test]
fn first_prompt_updates_conversation_title_immediately() {
    use bevy_cef::prelude::BinReceive;

    let mut app = App::new();
    app.add_observer(on_chat_submit);
    let session = app
        .world_mut()
        .spawn((PromptQueue::default(), AgentRunState::Idle))
        .id();
    let webview = app.world_mut().spawn(ChildOf(session)).id();

    app.world_mut().trigger(BinReceive {
        webview,
        payload: ChatSubmit {
            text: "  make me a new\nJapanese restaurant website  ".into(),
            attachments: Vec::new(),
        },
    });
    app.world_mut().flush();

    assert_eq!(
        app.world().get::<AgentConversationTitle>(session),
        Some(&AgentConversationTitle(
            "make me a new Japanese restaurant website".into()
        ))
    );
    assert_eq!(
        app.world()
            .get::<PromptQueue>(session)
            .and_then(|queue| queue.items.front())
            .map(|prompt| prompt.text.as_str()),
        Some("  make me a new\nJapanese restaurant website  ")
    );

    app.world_mut().trigger(BinReceive {
        webview,
        payload: ChatSubmit {
            text: "make it darker".into(),
            attachments: Vec::new(),
        },
    });
    app.world_mut().flush();

    assert_eq!(
        app.world().get::<AgentConversationTitle>(session),
        Some(&AgentConversationTitle(
            "make me a new Japanese restaurant website".into()
        ))
    );
}

#[test]
fn submitting_after_error_rearms_prompt_dispatch() {
    let mut queue = PromptQueue::default();
    let mut state = AgentRunState::Errored("failed".into());

    enqueue_prompt(&mut queue, &mut state, "retry".into(), Vec::new());

    assert!(matches!(state, AgentRunState::Idle));
    assert_eq!(
        queue.items.front().map(|item| item.text.as_str()),
        Some("retry")
    );
    assert!(!queue.paused);
}

#[test]
fn normal_cancel_overrides_pending_flush() {
    use bevy_cef::prelude::BinReceive;

    let mut app = App::new();
    app.add_observer(on_chat_cancel);
    let mut queue = PromptQueue::default();
    queue.enqueue("queued".into());
    assert!(queue.request_flush());
    let stack = app.world_mut().spawn(queue).id();
    let webview = app.world_mut().spawn(ChildOf(stack)).id();

    app.world_mut().trigger(BinReceive::<ChatCancel> {
        webview,
        payload: ChatCancel,
    });
    app.world_mut().flush();

    assert!(
        !app.world()
            .get::<PromptQueue>(stack)
            .unwrap()
            .flush_pending()
    );
}

#[test]
fn escape_flush_rearms_errored_queue() {
    use bevy_cef::prelude::BinReceive;

    let mut app = App::new();
    app.add_observer(on_chat_escape);
    let mut queue = PromptQueue::default();
    queue.enqueue("retry".into());
    queue.paused = true;
    let stack = app
        .world_mut()
        .spawn((queue, AgentRunState::Errored("failed".into())))
        .id();
    let webview = app.world_mut().spawn(ChildOf(stack)).id();

    app.world_mut().trigger(BinReceive::<ChatEscape> {
        webview,
        payload: ChatEscape,
    });
    app.world_mut().flush();

    assert!(matches!(
        app.world().get::<AgentRunState>(stack),
        Some(AgentRunState::Idle)
    ));
    let queue = app.world().get::<PromptQueue>(stack).unwrap();
    assert!(queue.flush_pending());
    assert!(!queue.paused);
}

#[test]
fn escape_without_queue_clears_stale_flush() {
    use bevy_cef::prelude::BinReceive;

    let mut app = App::new();
    app.add_observer(on_chat_escape);
    let mut queue = PromptQueue::default();
    queue.enqueue("queued".into());
    assert!(queue.request_flush());
    queue.items.clear();
    let stack = app
        .world_mut()
        .spawn((queue, AgentRunState::Streaming))
        .id();
    let webview = app.world_mut().spawn(ChildOf(stack)).id();

    app.world_mut().trigger(BinReceive::<ChatEscape> {
        webview,
        payload: ChatEscape,
    });
    app.world_mut().flush();

    assert!(
        !app.world()
            .get::<PromptQueue>(stack)
            .unwrap()
            .flush_pending()
    );
}

#[test]
fn cancel_queued_prompt_removes_only_target() {
    use bevy_cef::prelude::BinReceive;

    let mut app = App::new();
    app.add_observer(on_chat_cancel_queued_prompt);
    let mut queue = PromptQueue::default();
    queue.enqueue("first".into());
    queue.enqueue("second".into());
    let second_id = queue.items[1].id;
    let stack = app.world_mut().spawn(queue).id();
    let webview = app.world_mut().spawn(ChildOf(stack)).id();

    app.world_mut()
        .trigger(BinReceive::<ChatCancelQueuedPrompt> {
            webview,
            payload: ChatCancelQueuedPrompt { id: second_id },
        });
    app.world_mut().flush();

    let queue = app.world().get::<PromptQueue>(stack).unwrap();
    assert_eq!(queue.items.len(), 1);
    assert_eq!(queue.items[0].text, "first");
}

#[test]
fn resume_agent_name_prefers_profile_then_kind_then_id() {
    let profile = Profile::registry("Antigravity", "antigravity");
    assert_eq!(
        resume_agent_name(Some(&profile), Some(AgentKind::Claude), Some("claude")),
        "Antigravity"
    );
    assert_eq!(
        resume_agent_name(None, Some(AgentKind::Claude), Some("claude")),
        "Claude"
    );
    assert_eq!(
        resume_agent_name(None, None, Some("custom-acp")),
        "custom-acp"
    );
}

#[test]
fn page_ready_clears_chat_synced_only_for_chat_views() {
    use bevy::prelude::*;
    use bevy_cef::prelude::BinReceive;
    use vmux_core::page::PageReady;

    let mut app = App::new();
    app.add_observer(reset_chat_synced_on_page_ready);

    let chat = app.world_mut().spawn((AgentChatView, ChatSynced)).id();
    let other = app.world_mut().spawn(ChatSynced).id();

    app.world_mut().trigger(BinReceive::<PageReady> {
        webview: chat,
        payload: PageReady {},
    });
    app.world_mut().trigger(BinReceive::<PageReady> {
        webview: other,
        payload: PageReady {},
    });
    app.world_mut().flush();

    assert!(
        app.world().get::<ChatSynced>(chat).is_none(),
        "a chat view must re-sync (ChatSynced cleared) when the page reloads"
    );
    assert!(
        app.world().get::<ChatSynced>(other).is_some(),
        "a non-chat view must be left untouched"
    );
}

fn duration_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, track_turn_duration);
    app
}

#[test]
fn streaming_then_idle_records_one_duration() {
    let mut app = duration_app();
    let e = app.world_mut().spawn(AgentRunState::Streaming).id();
    app.update();
    assert!(
        app.world()
            .get::<AgentTurnMeta>(e)
            .unwrap()
            .turn_start
            .is_some()
    );
    *app.world_mut().get_mut::<AgentRunState>(e).unwrap() = AgentRunState::Idle;
    app.update();
    let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
    assert_eq!(meta.durations.len(), 1);
    assert!(meta.turn_start.is_none());
}

#[test]
fn awaiting_approval_does_not_finalize() {
    let mut app = duration_app();
    let e = app.world_mut().spawn(AgentRunState::Streaming).id();
    app.update();
    *app.world_mut().get_mut::<AgentRunState>(e).unwrap() = AgentRunState::AwaitingApproval {
        call_id: "c".into(),
        name: "n".into(),
        args: serde_json::Value::Null,
    };
    app.update();
    let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
    assert!(meta.durations.is_empty());
    assert!(meta.turn_start.is_some());
}
