//! Shared bin-ipc payloads for the `vmux://agent` chat page. Compiled for both native
//! (emit/receive in the Bevy host) and wasm (the Dioxus page). rkyv for the bin-ipc wire;
//! serde for the JSON-encoded message list.

/// Bin-event id: native → page conversation/run-state snapshot.
pub const CHAT_SNAPSHOT_EVENT: &str = "chat_snapshot";

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
}

/// Native → page: the full conversation plus run-state, pushed on every change.
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
    /// `serde_json` of `Vec<ChatItem>` (user bubbles + grouped assistant turns).
    pub messages_json: String,
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
    },
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
    User { text: String },
    Turn(ChatTurn),
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
            status: "streaming".to_string(),
            handoff_source: "Codex".to_string(),
            handoff_truncated: true,
            handoff_message_count: 2,
            queued: vec![
                QueuedPromptSnapshot {
                    id: 4,
                    text: "a".into(),
                },
                QueuedPromptSnapshot {
                    id: 9,
                    text: "b".into(),
                },
            ],
            paused: true,
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
        let back = rkyv::from_bytes::<ChatSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.status, "streaming");
        assert_eq!(back.queued.len(), 2);
        assert_eq!(back.queued[0].id, 4);
        assert_eq!(back.queued[0].text, "a");
        assert_eq!(back.queued[1].id, 9);
        assert_eq!(back.queued[1].text, "b");
        assert!(back.paused);
        assert_eq!(back.handoff_source, "Codex");
        assert!(back.handoff_truncated);
        assert_eq!(back.handoff_message_count, 2);
    }

    #[test]
    fn chat_item_turn_roundtrip() {
        let items = vec![
            ChatItem::User { text: "hi".into() },
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
