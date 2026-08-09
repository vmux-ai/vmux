//! The chat page's signals, grouped by what they are for.
//!
//! `Page` held forty-nine `use_signal` calls in one scope, which said nothing about which of them
//! move together. Each hook here owns one cluster and hands it back as a `Copy` struct, so `Page`
//! destructures it and the rest of the body reads exactly as before.

use std::collections::{HashMap, HashSet};

use crate::chat_page::event::{
    ChatAttachment, ChatItem, ChatMediaEntry, ComposerContext, ModelOptionEntry,
    QueuedPromptSnapshot, ResumableSessionEntry, SlashCommandEntry,
};
use crate::chat_page::scroll;
use dioxus::prelude::*;

/// The rendered transcript, the window of it that is loaded, and where the reader is in it.
#[derive(Clone, Copy)]
pub struct Transcript {
    pub items: Signal<Vec<ChatItem>>,
    pub loaded_start: Signal<u32>,
    pub messages_total: Signal<u32>,
    pub history_loading: Signal<bool>,
    pub recent_messages_json: Signal<String>,
    pub recent_messages_start: Signal<u32>,
    pub at_bottom: Signal<bool>,
    pub last_top: Signal<i32>,
    pub scroll_container: scroll::Container,
}

pub fn use_transcript() -> Transcript {
    Transcript {
        items: use_signal(Vec::new),
        loaded_start: use_signal(|| 0),
        messages_total: use_signal(|| 0),
        history_loading: use_signal(|| false),
        recent_messages_json: use_signal(String::new),
        // No window loaded yet, so every incoming index is older than this.
        recent_messages_start: use_signal(|| u32::MAX),
        at_bottom: use_signal(|| true),
        last_top: use_signal(|| 0),
        scroll_container: use_signal(|| None),
    }
}

/// What the agent is doing, and anything it is blocked on waiting for the user.
#[derive(Clone, Copy)]
pub struct RunState {
    pub status: Signal<String>,
    pub error: Signal<String>,
    pub approval: Signal<Option<(String, String, String)>>,
    pub approval_sel: Signal<usize>,
    pub choice_question: Signal<String>,
    pub choice_options: Signal<Vec<String>>,
}

pub fn use_run_state() -> RunState {
    RunState {
        status: use_signal(|| "installing".to_string()),
        error: use_signal(String::new),
        approval: use_signal(|| None),
        approval_sel: use_signal(|| 0),
        choice_question: use_signal(String::new),
        choice_options: use_signal(Vec::new),
    }
}

/// How the agent presents itself in the header.
#[derive(Clone, Copy)]
pub struct AgentIdentity {
    pub agent_name: Signal<String>,
    pub conversation_title: Signal<String>,
    pub agent_icon: Signal<String>,
    pub accent: Signal<String>,
}

pub fn use_agent_identity() -> AgentIdentity {
    AgentIdentity {
        agent_name: use_signal(String::new),
        conversation_title: use_signal(String::new),
        agent_icon: use_signal(String::new),
        accent: use_signal(String::new),
    }
}

/// Where this conversation was picked up from, when it was handed over from another agent.
#[derive(Clone, Copy)]
pub struct Handoff {
    pub source: Signal<String>,
    pub truncated: Signal<bool>,
    pub message_count: Signal<u32>,
}

pub fn use_handoff() -> Handoff {
    Handoff {
        source: use_signal(String::new),
        truncated: use_signal(|| false),
        message_count: use_signal(|| 0),
    }
}

/// The prompt being written: its text, its attachments, and the recall of earlier prompts.
#[derive(Clone, Copy)]
pub struct ComposerDraft {
    pub draft: Signal<String>,
    pub attachments: Signal<Vec<ChatAttachment>>,
    pub attachment_previews: Signal<HashMap<String, ChatAttachment>>,
    pub attachment_preview_requests: Signal<HashSet<String>>,
    /// Position in the prompt history while arrowing back through it.
    pub history_cursor: Signal<Option<usize>>,
    /// The half-written prompt set aside when recall started, restored on arrowing past the end.
    pub history_scratch: Signal<String>,
    /// Prompt carried in from the launcher, shown until the agent is ready to take it.
    pub transition_preview: Signal<String>,
    pub transition_attachments: Signal<Vec<ChatAttachment>>,
}

pub fn use_composer_draft(
    transition_prompt: Option<String>,
    transition_attachments: Option<Vec<ChatAttachment>>,
) -> ComposerDraft {
    ComposerDraft {
        draft: use_signal(String::new),
        attachments: use_signal(Vec::new),
        attachment_previews: use_signal(HashMap::new),
        attachment_preview_requests: use_signal(HashSet::new),
        history_cursor: use_signal(|| None),
        history_scratch: use_signal(String::new),
        transition_preview: use_signal(|| transition_prompt.unwrap_or_default()),
        transition_attachments: use_signal(|| transition_attachments.unwrap_or_default()),
    }
}

/// Prompts typed while the agent was busy, and whether the queue is holding them back.
#[derive(Clone, Copy)]
pub struct PromptQueue {
    pub queued: Signal<Vec<QueuedPromptSnapshot>>,
    pub paused: Signal<bool>,
}

pub fn use_prompt_queue() -> PromptQueue {
    PromptQueue {
        queued: use_signal(Vec::new),
        paused: use_signal(|| false),
    }
}

/// The `@`-mention file picker.
#[derive(Clone, Copy)]
pub struct MediaPicker {
    pub entries: Signal<Vec<ChatMediaEntry>>,
    pub request_id: Signal<u64>,
    pub requested_query: Signal<Option<String>>,
    pub loading: Signal<bool>,
}

pub fn use_media_picker() -> MediaPicker {
    MediaPicker {
        entries: use_signal(Vec::new),
        request_id: use_signal(|| 0),
        requested_query: use_signal(|| None),
        loading: use_signal(|| false),
    }
}

/// Which model the agent is running, and what else it offers.
#[derive(Clone, Copy)]
pub struct ModelPicker {
    pub models: Signal<Vec<ModelOptionEntry>>,
    pub current_model_id: Signal<String>,
    pub current_model: Signal<String>,
}

pub fn use_model_picker() -> ModelPicker {
    ModelPicker {
        models: use_signal(Vec::new),
        current_model_id: use_signal(String::new),
        current_model: use_signal(String::new),
    }
}

/// How hard the agent is asked to think, for the agents that expose the choice.
#[derive(Clone, Copy)]
pub struct EffortPicker {
    pub levels: Signal<Vec<String>>,
    pub current: Signal<String>,
    /// Which agent the levels were fetched for, so a switch does not show the last one's.
    pub agent_key: Signal<String>,
    pub menu_open: Signal<bool>,
}

pub fn use_effort_picker() -> EffortPicker {
    EffortPicker {
        levels: use_signal(Vec::new),
        current: use_signal(String::new),
        agent_key: use_signal(String::new),
        menu_open: use_signal(|| false),
    }
}

/// The slash-command menu and the context it offers completions against.
#[derive(Clone, Copy)]
pub struct SlashCommands {
    pub commands: Signal<Vec<SlashCommandEntry>>,
    pub menu_sel: Signal<usize>,
    pub composer_context: Signal<ComposerContext>,
}

pub fn use_slash_commands() -> SlashCommands {
    SlashCommands {
        commands: use_signal(Vec::new),
        menu_sel: use_signal(|| 0),
        composer_context: use_signal(ComposerContext::default),
    }
}

/// Earlier sessions this agent can be resumed into.
#[derive(Clone, Copy)]
pub struct Resume {
    pub sessions: Signal<Vec<ResumableSessionEntry>>,
    pub requested: Signal<bool>,
    pub loading: Signal<bool>,
}

pub fn use_resume() -> Resume {
    Resume {
        sessions: use_signal(Vec::new),
        requested: use_signal(|| false),
        loading: use_signal(|| false),
    }
}
