use std::collections::{HashSet, VecDeque};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_service::protocol::AgentAttachment;

use crate::message::Message;
use crate::{AgentKind, AgentVariant};

#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct AgentSession {
    pub kind: AgentKind,
    pub variant: AgentVariant,
    pub sid: String,
    pub provider: String,
    pub model: String,
}

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentMessages(pub Vec<Message>);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConversationTitle(pub String);

pub(crate) fn provisional_conversation_title(text: &str) -> Option<String> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    if normalized.chars().count() <= 120 {
        return Some(normalized);
    }
    let mut title = normalized.chars().take(119).collect::<String>();
    let trimmed_len = title.trim_end().len();
    title.truncate(trimmed_len);
    title.push('…');
    Some(title)
}

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct AgentApprovalPolicy {
    pub auto: HashSet<String>,
}

impl AgentApprovalPolicy {
    /// Remembers a normalized tool identifier for automatic approval.
    pub fn allow(&mut self, tool: &str) {
        self.auto.insert(approval_tool_key(tool));
    }

    /// Returns whether a normalized tool identifier is automatically approved.
    pub fn allows(&self, tool: &str) -> bool {
        self.auto.contains(&approval_tool_key(tool))
    }
}

/// Normalizes equivalent ACP, CLI, and MCP tool identifiers to one policy key.
pub fn approval_tool_key(tool: &str) -> String {
    tool.trim()
        .to_ascii_lowercase()
        .split(|character: char| {
            character.is_ascii_whitespace() || matches!(character, '-' | '.' | ':' | '_')
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// FIFO of prompts waiting to be dispatched to this session's agent. Normal dispatch takes one
/// prompt per idle turn. `paused` holds the queue after an interrupt until the user resumes,
/// clears, or submits again; `flush_pending` combines all queued prompts for an Esc flush.
#[derive(Component, Clone, Debug, Default)]
pub struct PromptQueue {
    pub items: VecDeque<QueuedPrompt>,
    pub paused: bool,
    flush_pending: bool,
    next_id: u64,
}

/// One prompt waiting in a [`PromptQueue`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedPrompt {
    pub id: u64,
    pub text: String,
    pub attachments: Vec<AgentAttachment>,
}

impl PromptQueue {
    /// The gate for dispatching the next prompt: idle, not paused, and something queued.
    pub fn ready(&self, idle: bool) -> bool {
        idle && !self.paused && !self.items.is_empty()
    }

    /// Whether the next dispatch should combine every queued prompt.
    pub fn flush_pending(&self) -> bool {
        self.flush_pending
    }

    /// Append one prompt and allow dispatch to continue.
    pub fn enqueue(&mut self, text: String) {
        self.enqueue_with_attachments(text, Vec::new());
    }

    /// Append one prompt with local file attachments and allow dispatch to continue.
    pub fn enqueue_with_attachments(&mut self, text: String, attachments: Vec<AgentAttachment>) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.items.push_back(QueuedPrompt {
            id,
            text,
            attachments,
        });
        self.paused = false;
    }

    /// Remove one queued prompt by its stable id.
    pub fn remove(&mut self, id: u64) -> bool {
        let Some(index) = self.items.iter().position(|item| item.id == id) else {
            return false;
        };
        self.items.remove(index);
        if self.items.is_empty() {
            self.paused = false;
            self.flush_pending = false;
        }
        true
    }

    /// Mark all currently queued prompts for one combined dispatch.
    pub fn request_flush(&mut self) -> bool {
        if self.items.is_empty() {
            return false;
        }
        self.paused = false;
        self.flush_pending = true;
        true
    }

    /// Cancel a pending combined dispatch without modifying queued prompts.
    pub fn cancel_flush(&mut self) {
        self.flush_pending = false;
    }

    /// Drop queued prompts and reset queue control state.
    pub fn clear(&mut self) {
        self.items.clear();
        self.paused = false;
        self.flush_pending = false;
    }

    /// Resume normal FIFO dispatch after an interrupt.
    pub fn resume(&mut self) {
        self.paused = false;
        self.flush_pending = false;
    }

    /// Take one FIFO prompt, or all prompts joined by blank lines for a pending flush.
    pub fn take_next(&mut self) -> Option<QueuedPrompt> {
        if !self.flush_pending {
            return self.items.pop_front();
        }
        self.flush_pending = false;
        let mut prompt = self.items.pop_front()?;
        prompt
            .text
            .reserve(self.items.iter().map(|item| item.text.len() + 2).sum());
        for item in self.items.drain(..) {
            if !prompt.text.is_empty() && !item.text.is_empty() {
                prompt.text.push_str("\n\n");
            }
            prompt.text.push_str(&item.text);
            prompt.attachments.extend(item.attachments);
        }
        Some(prompt)
    }
}

#[cfg(test)]
#[path = "components.test.rs"]
mod tests;
