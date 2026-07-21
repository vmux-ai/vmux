//! Shared bin-ipc payloads for the `vmux://agent` chat page. Compiled for both native
//! (emit/receive in the Bevy host) and wasm (the Dioxus page). rkyv for the bin-ipc wire;
//! serde for the JSON-encoded message list.

/// Bin-event id: native → page conversation/run-state snapshot.
pub const CHAT_SNAPSHOT_EVENT: &str = "chat_snapshot";
pub const CHAT_HISTORY_PAGE_EVENT: &str = "chat_history_page";
pub const CHAT_INITIAL_ITEM_LIMIT: u32 = 48;
pub const CHAT_HISTORY_PAGE_SIZE: u32 = 40;
pub const CHAT_HISTORY_MAX_PAGE_SIZE: u32 = 80;
pub use vmux_command::prompt_media::{
    CHAT_ATTACHMENT_PREVIEWS_EVENT, CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT,
    ChatAttachPaths, ChatAttachment, ChatAttachmentPreviewRequest, ChatAttachments,
    ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest, ChatPasteMedia, ChatPickFiles,
    ChatSubmitAttachment,
};

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
    /// Native-formatted "2h ago · proj".
    pub subtitle: String,
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

/// The page's block type inside a [`ChatTurn`]. Mirrors `vmux_service::message::AssistantBlock`
/// plus folded tool results and reconnect progress.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ChatBlock {
    Text(String),
    Thinking(String),
    ToolUse {
        call_id: String,
        name: String,
        args: String,
        parent_call_id: Option<String>,
    },
    Subagent(Box<ChatSubagent>),
    Diff {
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    Plan {
        steps: Vec<ChatPlanStep>,
    },
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
    Reconnect {
        attempt: u32,
        total: u32,
    },
}

/// Page representation of `vmux_service::message::SubagentBlock`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChatSubagent {
    pub call_id: String,
    pub provider: String,
    pub title: String,
    pub status: String,
    pub action: String,
    pub agent_name: Option<String>,
    pub thread_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub child_thread_ids: Vec<String>,
    pub parent_call_id: Option<String>,
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub raw_input: String,
}

/// Mirror of `vmux_service::message::PlanStep`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChatPlanStep {
    pub content: String,
    pub status: String,
}

/// A rendered conversation entry: a user bubble or a grouped assistant turn. Built backend by
/// `group_turns`, carried as JSON in [`ChatSnapshot::messages_json`].
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ChatItem {
    User {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<ChatSubmitAttachment>,
    },
    Turn(ChatTurn),
}

impl ChatItem {
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            text: text.into(),
            context: None,
            attachments: Vec::new(),
        }
    }
}

/// One assistant turn: its ordered prose/activity timeline and run-state.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ChatTurn {
    /// Prose, thinking, tools, reconnects, plans, and diffs in transcript order.
    pub blocks: Vec<ChatBlock>,
    /// True only for the live (tail) turn while the run is active.
    pub running: bool,
    /// Final wall-clock seconds for a turn that finished this process; `None` otherwise.
    pub duration_secs: Option<u32>,
    /// Number of non-prose activity blocks.
    pub step_count: u32,
}

pub(crate) fn is_guardian_tool(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("guardian")
        || lower.contains("approval")
        || lower == "review"
        || lower.ends_with("_review")
        || lower.ends_with(".review")
        || lower.ends_with(":review")
}

impl ChatTurn {
    #[cfg(any(test, target_arch = "wasm32"))]
    pub(crate) fn latest_top_level_tool_index(&self) -> Option<usize> {
        self.blocks
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, block)| match block {
                ChatBlock::ToolUse { .. } if self.parent_tool_index(index).is_none() => Some(index),
                _ => None,
            })
    }

    pub(crate) fn parent_tool_index(&self, index: usize) -> Option<usize> {
        let mut parent = self.direct_parent_index(index)?;
        for _ in 0..self.blocks.len() {
            let Some(next) = self.direct_parent_index(parent) else {
                break;
            };
            if next == parent {
                break;
            }
            parent = next;
        }
        Some(parent)
    }

    fn direct_parent_index(&self, index: usize) -> Option<usize> {
        match self.blocks.get(index)? {
            ChatBlock::ToolUse {
                parent_call_id: Some(parent_call_id),
                ..
            } => self.call_index(parent_call_id),
            ChatBlock::Subagent(subagent) => subagent
                .parent_call_id
                .as_deref()
                .and_then(|parent_call_id| self.call_index(parent_call_id)),
            ChatBlock::ToolUse { name, .. } if is_guardian_tool(name) => {
                self.guardian_parent_index(index)
            }
            ChatBlock::ToolResult { call_id, .. } if !call_id.is_empty() => {
                self.call_index(call_id)
            }
            _ => None,
        }
    }

    fn call_index(&self, call_id: &str) -> Option<usize> {
        self.blocks.iter().position(|block| match block {
            ChatBlock::ToolUse {
                call_id: block_call_id,
                ..
            } => block_call_id == call_id,
            ChatBlock::Subagent(subagent) => subagent.call_id == call_id,
            _ => false,
        })
    }

    fn guardian_parent_index(&self, index: usize) -> Option<usize> {
        for (candidate, block) in self.blocks[..index].iter().enumerate().rev() {
            match block {
                ChatBlock::ToolUse { name, .. } if is_guardian_tool(name) => {}
                ChatBlock::ToolUse { .. } | ChatBlock::Subagent(_) => return Some(candidate),
                _ => return None,
            }
        }
        None
    }
}

#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) fn latest_tool_location(items: &[ChatItem]) -> Option<(usize, usize)> {
    items
        .iter()
        .enumerate()
        .rev()
        .find_map(|(item_index, item)| match item {
            ChatItem::Turn(turn) => turn
                .latest_top_level_tool_index()
                .map(|block_index| (item_index, block_index)),
            ChatItem::User { .. } => None,
        })
}

/// The curated verbs the running-turn header cycles through (owned by the shared contract, not
/// the view). The page picks one at random every few seconds while streaming.
pub const WORKING_VERBS: &[&str] = &[
    "Working",
    "Thinking",
    "Pondering",
    "Noodling",
    "Percolating",
    "Conjuring",
    "Cooking",
    "Brewing",
    "Musing",
    "Ruminating",
    "Scheming",
    "Synthesizing",
    "Tinkering",
    "Churning",
    "Vibing",
    "Simmering",
    "Crafting",
    "Divining",
    "Mulling",
    "Spelunking",
];

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn chat_snapshot_rkyv_roundtrip() {
        let v = ChatSnapshot {
            messages_json: "[]".to_string(),
            messages_start: 12,
            messages_total: 60,
            status: "streaming".to_string(),
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
                context: Some("workspace policy".into()),
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
                if context.as_deref() == Some("workspace policy")
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
        assert!(!WORKING_VERBS.is_empty());
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
                subtitle: "2h ago · w".into(),
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
