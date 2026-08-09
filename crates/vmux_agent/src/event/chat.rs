//! Shared bin-ipc payloads for the `vmux://agent` chat page. Compiled for both native
//! (emit/receive in the Bevy host) and wasm (the Dioxus page). rkyv for the bin-ipc wire;
//! serde for the JSON-encoded message list.

/// Bin-event id: native → page conversation/run-state snapshot.
pub const CHAT_SNAPSHOT_EVENT: &str = "chat_snapshot";
pub const CHAT_HISTORY_PAGE_EVENT: &str = "chat_history_page";
pub const COMPOSER_CONTEXT_EVENT: &str = "composer_context";
pub const CHAT_INITIAL_ITEM_LIMIT: u32 = 48;
pub const CHAT_HISTORY_PAGE_SIZE: u32 = 40;
pub const CHAT_HISTORY_MAX_PAGE_SIZE: u32 = 80;
pub use vmux_wire::prompt_media::{
    CHAT_ATTACHMENT_PREVIEWS_EVENT, CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT,
    ChatAttachPaths, ChatAttachment, ChatAttachmentPreviewRequest, ChatAttachments,
    ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest, ChatPasteMedia, ChatPickFiles,
    ChatSubmitAttachment,
};

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct QueuedPromptSnapshot {
    pub id: u64,
    pub text: String,
    pub attachment_names: Vec<String>,
}

/// Native → page: the recent conversation page plus run-state, pushed on every change.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatSnapshot {
    /// `serde_json` of the recent `Vec<ChatItem>` page.
    pub messages_json: String,
    pub messages_start: u32,
    pub messages_total: u32,
    /// `idle` | `streaming` | `awaiting` | `errored`.
    pub status: String,
    /// Populated when `status == "errored"`.
    pub error: String,
    /// Populated when `status == "awaiting"`.
    pub approval_call_id: String,
    pub approval_name: String,
    pub approval_args_json: String,
    /// Prompts queued behind the running turn (FIFO), oldest first.
    pub queued: Vec<QueuedPromptSnapshot>,
    /// True after an interrupt: the queue is held (not auto-advancing) until resume/clear/submit.
    pub paused: bool,
    /// Agent display name (from the session `Profile`), for the header/hero.
    pub agent_name: String,
    /// Model-written summary used as the conversation header and page title.
    pub conversation_title: String,
    /// Agent favicon URL (from `PageMetadata.icon`); may be empty (page falls back per url).
    pub agent_icon: String,
    /// Agent brand accent color (hex, from the avatar), for loading/status accents.
    pub accent_color: String,
    pub handoff_source: String,
    pub handoff_truncated: bool,
    /// Number of rendered [`ChatItem`] entries originating from the imported conversation.
    pub handoff_message_count: u32,
    pub choice_question: String,
    pub choice_options: Vec<String>,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ComposerContext {
    pub cwd: String,
    pub workspace_name: String,
    pub workspace_selected: bool,
    pub is_git_repo: bool,
    pub is_worktree: bool,
    pub branch: String,
    pub base_ref: String,
    pub uncommitted: u32,
    pub ahead: u32,
    pub can_manage_workspace: bool,
    pub auto_allow_count: u32,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatHistoryRequest {
    pub before: u32,
    pub limit: u32,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatHistoryPage {
    pub items_json: String,
    pub start: u32,
    pub end: u32,
    pub total: u32,
}

/// Page → native: the user submitted a prompt.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatSubmit {
    pub text: String,
    pub attachments: Vec<ChatSubmitAttachment>,
}

/// Page → native: answer the active agent-authored multiple-choice prompt.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatChoiceSelected {
    pub index: u32,
}

/// Page → native: the user answered a permission prompt. `decision`: 0 = deny, 1 = allow,
/// 2 = allow-always.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatApproval {
    pub call_id: String,
    pub decision: u8,
}

/// Page → native: interrupt the in-flight turn from Ctrl+C or Stop.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatCancel;

/// Page → native: resume a queue paused by a prior interrupt.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatResume;

/// Page → native: drop all queued prompts.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatClearQueue;

/// Page → native: drop one queued prompt.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatCancelQueuedPrompt {
    pub id: u64,
}

/// Page → native: apply Escape to the authoritative native queue and run state.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatEscape;

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatSelectWorkspace;

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatCreateWorktree;

/// Bin-event id: native → page, the resumable-session list (answer to [`ResumeListRequest`]).
pub const RESUMABLE_SESSIONS_EVENT: &str = "resumable_sessions";
/// Bin-event id: native → page, the slash commands available for this pane.
pub const SLASH_COMMANDS_EVENT: &str = "slash_commands";
/// Bin-event id: native → page, current ACP model and selectable models.
pub const MODEL_STATE_EVENT: &str = "model_state";

/// One row in the `/resume` picker. Strings only (the page is dumb — native does the work).
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ResumableSessionEntry {
    /// `AgentKind::as_url_segment` (vibe|claude|codex).
    pub kind: String,
    pub sid: String,
    pub cwd: String,
    pub title: String,
    /// Directory basename shown beside the localized last-modified age.
    pub subtitle: String,
    pub age_seconds: u64,
    /// Human-readable active ACP agent name.
    pub agent_name: String,
    pub cross_runtime: bool,
}

/// Native → page: the resumable sessions to show in the `/resume` picker, newest-first.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ResumableSessions {
    pub sessions: Vec<ResumableSessionEntry>,
}

/// One slash command entry (native → page).
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SlashCommandEntry {
    /// Bare command name without the leading slash (e.g. `resume`, `cli`).
    pub name: String,
    pub description: String,
}

/// Native → page: the slash commands this pane offers (varies by agent kind).
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SlashCommands {
    pub commands: Vec<SlashCommandEntry>,
}

/// One row in the `/model` picker.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ModelOptionEntry {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Native → page ACP model state.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ModelState {
    pub current_model_id: String,
    pub current_model_name: String,
    pub models: Vec<ModelOptionEntry>,
    /// Agent key for the effort selector (ACP agent id, e.g. `claude`). Empty when unknown.
    pub agent_key: String,
    /// Currently selected launch-time reasoning-effort level (`""` = agent default).
    pub effort_current: String,
    /// Effort levels this agent supports, low→high. Empty hides the effort selector.
    pub effort_levels: Vec<String>,
}

/// Page → native selected ACP model.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SelectModel {
    pub model_id: String,
}

/// Page → native: set the launch-time reasoning-effort level for an agent. `level` empty clears
/// it back to the agent's default. Applied to the next session/process the agent launches.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SetAgentEffort {
    pub agent_key: String,
    pub level: String,
}

/// Page → native: open a vmux page URL in a new stack (e.g. the error card's "change version"
/// action opening `vmux://agents`).
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ChatOpenPage {
    pub url: String,
}

/// Page → native: open the `/resume` picker (native replies with [`ResumableSessions`]).
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ResumeListRequest;

/// Page → native: resume a specific past session on this stack, in the current runtime.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ResumeSession {
    pub kind: String,
    pub sid: String,
    pub cwd: String,
}

/// Page → native: hand the current session to the other runtime. `to`: `"cli"` | `"acp"`.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RuntimeSwitchRequest {
    pub to: String,
}

pub use vmux_wire::chat::{
    ChatBlock, ChatItem, ChatPlanStep, ChatSubagent, ChatTurn, ToolName, WORKING_VERB_IDS,
    latest_tool_location,
};

#[cfg(all(test, not(web)))]
mod tests {
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
}

impl SlashCommands {
    /// The commands a session offers, which depend on what its agent can do.
    pub fn for_agent(cross_runtime: bool, has_models: bool) -> Self {
        let mut commands = vec![
            SlashCommandEntry {
                name: "upload".into(),
                description: "Attach files".into(),
            },
            SlashCommandEntry {
                name: "resume".into(),
                description: "Resume a past session".into(),
            },
        ];
        if has_models {
            commands.push(SlashCommandEntry {
                name: "model".into(),
                description: "Select model".into(),
            });
        }
        if cross_runtime {
            commands.push(SlashCommandEntry {
                name: "cli".into(),
                description: "Continue this session in the CLI".into(),
            });
        }
        Self { commands }
    }
}
