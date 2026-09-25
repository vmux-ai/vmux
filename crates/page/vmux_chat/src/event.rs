pub const CHAT_INITIAL_ITEM_LIMIT: u32 = 48;
pub const CHAT_HISTORY_PAGE_SIZE: u32 = 40;
pub const CHAT_HISTORY_MAX_PAGE_SIZE: u32 = 80;
pub use vmux_api::chat::{
    ChatKey, ResumableSessionEntry, ResumableSessions, ResumeListRequest, ResumeSession,
    SlashCommandEntry, SlashCommands,
};
use vmux_api::json::JsonValue;
pub use vmux_api::prompt_media::{
    ChatAttachPaths, ChatAttachment, ChatAttachments, ChatMediaEntries, ChatMediaEntry,
    ChatMediaListRequest, ChatPasteMedia, ChatPickFiles,
};
pub use vmux_api::protocol::ApprovalDecision;
pub use vmux_api::room::ModelOptionEntry;

enum Events {}

impl vmux_api::BinEventFamily for Events {
    const TARGET: vmux_api::BinEventTarget =
        vmux_api::BinEventTarget::Hosts(&["sessions", "agent", "start"]);
}

#[vmux_api::contract(Default, Eq)]
pub struct QueuedPromptSnapshot {
    pub id: u64,
    pub text: String,
    pub attachments: Vec<ChatAttachment>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatSnapshot {
    pub status: String,
    pub error: String,
    pub approval: Option<PendingApproval>,
    pub queued: Vec<QueuedPromptSnapshot>,
    pub paused: bool,
    pub agent_name: String,
    pub conversation_title: String,
    pub agent_icon: String,
    pub accent_color: String,
    #[serde(default)]
    pub user_name: String,
    #[serde(default)]
    pub user_initials: String,
    #[serde(default)]
    pub user_color: String,
    pub handoff_source: String,
    pub handoff_truncated: bool,
    pub handoff_message_count: u32,
    pub choice_question: String,
    pub choice_options: Vec<String>,
}

#[vmux_api::contract(Eq)]
pub struct PendingApproval {
    pub call_id: String,
    pub name: String,
    pub details: Vec<ApprovalDetail>,
}

impl PendingApproval {
    pub fn new(call_id: String, name: String, args: &JsonValue) -> Self {
        Self {
            call_id,
            name,
            details: ApprovalDetail::rows(args),
        }
    }
}

#[vmux_api::contract(Eq)]
pub struct ApprovalDetail {
    pub label: String,
    pub value: String,
}

impl ApprovalDetail {
    fn rows(value: &JsonValue) -> Vec<Self> {
        let mut details = Vec::new();
        Self::flatten("", value, &mut details);
        details
    }

    fn flatten(path: &str, value: &JsonValue, details: &mut Vec<Self>) {
        if let JsonValue::Object(fields) = value {
            for (name, value) in fields {
                let child_path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}.{name}")
                };
                Self::flatten(&child_path, value, details);
            }
            return;
        }
        let value = match value {
            JsonValue::String(value) => value.clone(),
            other => serde_json::to_string_pretty(&Self::readable(other)).unwrap_or_default(),
        };
        details.push(Self {
            label: Self::label(path),
            value,
        });
    }

    fn readable(value: &JsonValue) -> serde_json::Value {
        match value {
            JsonValue::Null => serde_json::Value::Null,
            JsonValue::Bool(value) => serde_json::Value::Bool(*value),
            JsonValue::Number(value) => serde_json::from_str(value)
                .unwrap_or_else(|_| serde_json::Value::String(value.clone())),
            JsonValue::String(value) => serde_json::Value::String(value.clone()),
            JsonValue::Array(values) => {
                serde_json::Value::Array(values.iter().map(Self::readable).collect())
            }
            JsonValue::Object(fields) => {
                let mut object = serde_json::Map::new();
                for (name, value) in fields {
                    object.insert(name.clone(), Self::readable(value));
                }
                serde_json::Value::Object(object)
            }
        }
    }

    fn label(path: &str) -> String {
        let path = path.strip_prefix("arguments.").unwrap_or(path);
        let label = if path.is_empty() { "details" } else { path };
        label
            .split('.')
            .map(|part| {
                let words = part.replace('_', " ");
                let mut chars = words.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[vmux_api::contract(Default, Eq)]
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
    pub projects: Vec<vmux_core::event::ProjectRow>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ModeState {
    pub current_mode_id: String,
    pub modes: Vec<vmux_service::protocol::AcpModeOption>,
}

#[vmux_api::ui_event(Default)]
pub struct SelectMode {
    pub mode_id: String,
}

#[vmux_api::ui_event(Default)]
pub struct ChatHistoryRequest {
    pub generation: u64,
    pub request_id: u64,
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatTranscriptState {
    pub generation: u64,
    pub request_id: u64,
    pub prepend_revision: u64,
    pub items: Vec<ChatItem>,
    pub loaded_start: u32,
    pub total: u32,
    pub loading: bool,
}

#[vmux_api::ui_event(Default)]
pub struct ChatSubmit {
    pub text: String,
}

#[vmux_api::ui_event(Default)]
pub struct ChatRemoveAttachment {
    pub path: String,
}

#[vmux_api::ui_event(Default)]
pub struct ChatChoiceSelected {
    pub index: u32,
}

#[vmux_api::ui_event(Default)]
pub struct ChatApproval {
    pub call_id: String,
    pub decision: ApprovalDecision,
}

#[vmux_api::ui_event(Default)]
pub struct ChatCancel;

#[vmux_api::ui_event(Default)]
pub struct ChatResume;

#[vmux_api::ui_event(Default)]
pub struct ChatClearQueue;

#[vmux_api::ui_event(Default)]
pub struct ChatCancelQueuedPrompt {
    pub id: u64,
}

#[vmux_api::ui_event(Default)]
pub struct ChatEscape;

#[vmux_api::ui_event(Default)]
pub struct ChatSelectWorkspace;

#[vmux_api::ui_event(Default)]
pub struct ChatBranchesRequest {
    pub project: String,
}

#[vmux_api::contract(Default)]
pub struct ChatBranchesState {
    pub request_id: u64,
    pub project: String,
    pub branches: Vec<ChatBranch>,
    pub loading: bool,
}

#[vmux_api::contract(Default)]
pub struct ChatMediaState {
    pub request_id: u64,
    pub query: String,
    pub entries: Vec<ChatMediaEntry>,
    pub loading: bool,
}

#[vmux_api::ui_event(Default)]
pub struct ChatMediaQueryRequest {
    pub query: String,
}

#[vmux_api::contract(Default)]
pub struct ChatResumeState {
    pub request_id: u64,
    pub query: String,
    pub sessions: Vec<ResumableSessionEntry>,
    pub total: u32,
    pub loading: bool,
    pub active: bool,
}

#[vmux_api::ui_event(Default)]
pub struct ChatResumeQueryRequest {
    pub active: bool,
    pub query: String,
}

pub use vmux_core::event::ProjectBranch as ChatBranch;

#[vmux_api::ui_event(Default)]
pub struct ChatGoToBranch {
    pub project: String,
    pub branch: String,
    #[serde(default)]
    pub checkout: String,
}

#[vmux_api::contract(Default)]
pub struct ModelState {
    pub current_model_id: String,
    pub current_model_name: String,
    pub default_model_id: String,
    pub models: Vec<ModelOptionEntry>,
    pub agent_key: String,
    pub effort_current: String,
    pub effort_default: String,
    pub effort_levels: Vec<String>,
}

#[vmux_api::ui_event(Default)]
pub struct SelectModel {
    pub model_id: String,
}

#[vmux_api::ui_event(Default)]
pub struct SetAgentEffort {
    pub agent_key: String,
    pub level: String,
}

#[vmux_api::ui_event(Default)]
pub struct ChatOpenPage {
    pub url: String,
}

#[vmux_api::ui_event(Default)]
pub struct RuntimeSwitchRequest {
    pub to: String,
}

pub use vmux_api::chat::{
    ChatBlock, ChatItem, ChatPlanStep, ChatSubagent, ChatTurn, ToolName, WORKING_VERB_IDS,
    latest_tool_location,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_snapshot_rkyv_roundtrip() {
        let v = ChatSnapshot {
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
                    attachments: vec![ChatAttachment {
                        path: "/tmp/image.png".into(),
                        name: "image.png".into(),
                        mime_type: "image/png".into(),
                        size: 3,
                        preview_data_url: "data:image/png;base64,cG5n".into(),
                    }],
                },
                QueuedPromptSnapshot {
                    id: 9,
                    text: "b".into(),
                    attachments: Vec::new(),
                },
            ],
            paused: true,
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
        let back = rkyv::from_bytes::<ChatSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.status, "streaming");
        assert_eq!(back.conversation_title, "Refine generated summaries");
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
    fn chat_transcript_state_rkyv_roundtrip() {
        let value = ChatTranscriptState {
            generation: 3,
            request_id: 7,
            prepend_revision: 2,
            items: vec![ChatItem::user("older")],
            loaded_start: 4,
            total: 92,
            loading: true,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&value).unwrap();
        let back = rkyv::from_bytes::<ChatTranscriptState, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!((back.generation, back.request_id), (3, 7));
        assert_eq!((back.loaded_start, back.total), (4, 92));
        assert_eq!(back.prepend_revision, 2);
        assert!(back.loading);
        assert_eq!(back.items, vec![ChatItem::user("older")]);
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
    fn pending_approval_projects_nested_details_and_recovers_invalid_numbers() {
        let args = JsonValue::Object(vec![
            (
                "arguments".into(),
                JsonValue::Object(vec![(
                    "path".into(),
                    JsonValue::String("/tmp/SKILL.md".into()),
                )]),
            ),
            ("attempt".into(), JsonValue::Number("invalid".into())),
        ]);

        let approval = PendingApproval::new("call-1".into(), "read_file".into(), &args);

        assert_eq!(
            approval.details,
            vec![
                ApprovalDetail {
                    label: "Path".into(),
                    value: "/tmp/SKILL.md".into(),
                },
                ApprovalDetail {
                    label: "Attempt".into(),
                    value: "\"invalid\"".into(),
                },
            ]
        );
    }

    #[test]
    fn chat_item_turn_roundtrip() {
        let items = vec![
            ChatItem::User {
                text: "hi".into(),
                context: Some("project policy".into()),
                attachments: vec![ChatAttachment {
                    path: "/tmp/image.png".into(),
                    name: "image.png".into(),
                    mime_type: "image/png".into(),
                    size: 3,
                    preview_data_url: "data:image/png;base64,cG5n".into(),
                }],
                created_at_ms: 100,
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
                created_at_ms: 200,
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
                created_at_ms: 0,
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
            request_id: 8,
            query: "auth".into(),
            sessions: vec![ResumableSessionEntry {
                kind: "claude".into(),
                sid: "sid-9".into(),
                cwd: "/w".into(),
                title: "fix bug".into(),
                latest: "and the tests".into(),
                subtitle: "w".into(),
                age_seconds: 7200,
                updated_at: "2026-09-07".into(),
                agent_name: "Claude".into(),
                project: "w".into(),
                branch: "main".into(),
                cross_runtime: true,
                url: "vmux://sessions/claude/sid-9".into(),
            }],
            offset: 0,
            total: 1,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
        let back = rkyv::from_bytes::<ResumableSessions, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.request_id, 8);
        assert_eq!(back.query, "auth");
        assert_eq!(back.sessions.len(), 1);
        assert_eq!(back.sessions[0].sid, "sid-9");
        assert_eq!(back.sessions[0].latest, "and the tests");
        assert_eq!(back.sessions[0].agent_name, "Claude");
        assert!(back.sessions[0].cross_runtime);
    }
}
