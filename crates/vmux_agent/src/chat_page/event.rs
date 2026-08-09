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
    ChatBlock, ChatItem, ChatPlanStep, ChatSubagent, ChatTurn, WORKING_VERB_IDS, is_guardian_tool,
    latest_tool_location,
};

#[cfg(all(test, not(web)))]
#[path = "event.test.rs"]
mod tests;
