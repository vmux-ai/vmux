use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{AgentKind, AssistantBlock, Message};

pub const HANDOFF_PROMPT_PREFIX: &str = vmux_service::protocol::PRIVATE_CONTEXT_PREFIX;
pub const OMITTED_MARKER: &str = "[Older source turns omitted]";
pub const DEFAULT_CONTEXT_LIMIT: usize = 64 * 1024;

const CONTEXT_INTRO: &str = "Conversation imported from another agent:\n";

#[derive(Component, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImportedConversation {
    pub source_agent: String,
    pub source_kind: AgentKind,
    pub source_sid: String,
    pub messages: Vec<Message>,
    pub truncated: bool,
    pub first_prompt: Option<String>,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PendingHandoff {
    pub context: String,
    pub sent: bool,
}

impl PendingHandoff {
    pub fn context_for_send(&mut self) -> Option<String> {
        if self.sent {
            return None;
        }
        self.sent = true;
        Some(self.context.clone())
    }

    pub fn retry(&mut self) {
        self.sent = false;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltContext {
    pub text: String,
    pub truncated: bool,
}

pub fn build_context(messages: &[Message], limit: usize) -> BuiltContext {
    let segments: Vec<String> = messages.iter().filter_map(context_segment).collect();
    let full = format!("{CONTEXT_INTRO}{}", segments.join("\n"));
    if full.chars().count() <= limit {
        return BuiltContext {
            text: full,
            truncated: false,
        };
    }

    let reserved = CONTEXT_INTRO.chars().count() + OMITTED_MARKER.chars().count() + 2;
    let mut remaining = limit.saturating_sub(reserved);
    let mut kept = Vec::new();
    for segment in segments.iter().rev() {
        let len = segment.chars().count() + usize::from(!kept.is_empty());
        if len > remaining {
            break;
        }
        remaining -= len;
        kept.push(segment.clone());
    }
    kept.reverse();

    BuiltContext {
        text: format!("{CONTEXT_INTRO}{OMITTED_MARKER}\n\n{}", kept.join("\n")),
        truncated: true,
    }
}

fn context_segment(message: &Message) -> Option<String> {
    match message {
        Message::User { text, .. } if !text.trim().is_empty() => Some(format!("User:\n{text}")),
        Message::Assistant { blocks } => {
            let text = blocks
                .iter()
                .filter_map(|block| match block {
                    AssistantBlock::Text(text) if !text.trim().is_empty() => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            (!text.is_empty()).then(|| format!("Assistant:\n{text}"))
        }
        _ => None,
    }
}

pub fn wire_prompt(context: &str, display_text: &str) -> String {
    vmux_service::protocol::compose_agent_prompt(display_text, Some(context))
}

pub fn sanitize_replayed_messages(messages: &mut [Message], first_prompt: Option<&str>) {
    let mut fallback = first_prompt;
    for message in messages {
        let Message::User { text, .. } = message else {
            continue;
        };
        if let Some(display_text) =
            vmux_service::protocol::extract_display_prompt(text).map(str::to_string)
        {
            *text = display_text;
        } else if vmux_service::protocol::has_private_context_envelope(text)
            && let Some(display_text) = fallback.take()
        {
            *text = display_text.to_string();
        }
    }
}

pub fn visible_messages(imported: Option<&ImportedConversation>, live: &[Message]) -> Vec<Message> {
    let mut messages = imported
        .map(|imported| imported.messages.clone())
        .unwrap_or_default();
    messages.extend_from_slice(live);
    messages
}

pub fn save(
    agent_id: &str,
    session_id: &str,
    imported: &ImportedConversation,
) -> Result<(), String> {
    save_in(
        &vmux_core::profile::profile_dir().join("handoffs"),
        agent_id,
        session_id,
        imported,
    )
}

pub fn load(agent_id: &str, session_id: &str) -> Option<ImportedConversation> {
    load_in(
        &vmux_core::profile::profile_dir().join("handoffs"),
        agent_id,
        session_id,
    )
}

fn hex_component(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn record_path_in(root: &Path, agent_id: &str, session_id: &str) -> PathBuf {
    root.join(hex_component(agent_id))
        .join(format!("{}.json", hex_component(session_id)))
}

fn save_in(
    root: &Path,
    agent_id: &str,
    session_id: &str,
    imported: &ImportedConversation,
) -> Result<(), String> {
    let path = record_path_in(root, agent_id, session_id);
    let parent = path
        .parent()
        .ok_or_else(|| format!("invalid handoff path {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|err| format!("create handoff directory {}: {err}", parent.display()))?;
    let bytes =
        serde_json::to_vec(imported).map_err(|err| format!("serialize handoff record: {err}"))?;
    std::fs::write(&path, bytes)
        .map_err(|err| format!("write handoff record {}: {err}", path.display()))
}

fn load_in(root: &Path, agent_id: &str, session_id: &str) -> Option<ImportedConversation> {
    let bytes = std::fs::read(record_path_in(root, agent_id, session_id)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
#[path = "handoff.test.rs"]
mod tests;
