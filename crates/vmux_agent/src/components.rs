use std::collections::{HashSet, VecDeque};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

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

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct AgentApprovalPolicy {
    pub auto: HashSet<String>,
}

/// FIFO of prompts waiting to be dispatched to this session's agent. Normal dispatch takes one
/// prompt per idle turn. `paused` holds the queue after an interrupt until the user resumes,
/// clears, or submits again; `flush_pending` combines all queued prompts for an Esc flush.
#[derive(Component, Clone, Debug, Default)]
pub struct PromptQueue {
    pub items: VecDeque<String>,
    pub paused: bool,
    flush_pending: bool,
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
        self.items.push_back(text);
        self.paused = false;
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
    pub fn take_next(&mut self) -> Option<String> {
        if !self.flush_pending {
            return self.items.pop_front();
        }
        self.flush_pending = false;
        let mut text = self.items.pop_front()?;
        text.reserve(self.items.iter().map(|item| item.len() + 2).sum());
        for item in self.items.drain(..) {
            text.push_str("\n\n");
            text.push_str(&item);
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_components_default_constructible() {
        let _ = AgentMessages::default();
        let _ = AgentApprovalPolicy::default();
        let _ = PromptQueue::default();
    }

    #[test]
    fn prompt_queue_ready_gate() {
        let mut q = PromptQueue::default();
        assert!(!q.ready(true));
        q.items.push_back("a".into());
        assert!(q.ready(true));
        assert!(!q.ready(false));
        q.paused = true;
        assert!(!q.ready(true));
    }

    #[test]
    fn take_next_preserves_fifo_without_flush() {
        let mut q = PromptQueue::default();
        q.items.push_back("first".into());
        q.items.push_back("second".into());
        assert_eq!(q.take_next(), Some("first".to_string()));
        assert_eq!(q.items.front().map(String::as_str), Some("second"));
    }

    #[test]
    fn take_next_merges_all_items_for_flush() {
        let mut q = PromptQueue::default();
        q.items.push_back("first".into());
        q.items.push_back("second".into());

        assert!(q.request_flush());
        assert_eq!(q.take_next(), Some("first\n\nsecond".to_string()));
        assert!(q.items.is_empty());
        assert!(!q.flush_pending);
    }

    #[test]
    fn enqueue_preserves_pending_flush() {
        let mut q = PromptQueue::default();
        q.items.push_back("first".into());
        assert!(q.request_flush());

        q.enqueue("second".into());

        assert!(q.flush_pending);
        assert_eq!(q.take_next(), Some("first\n\nsecond".to_string()));
    }

    #[test]
    fn cancel_flush_clears_pending_flush() {
        let mut q = PromptQueue::default();
        q.items.push_back("first".into());
        assert!(q.request_flush());

        q.cancel_flush();

        assert!(!q.flush_pending);
    }

    #[test]
    fn clear_resets_queue_state() {
        let mut q = PromptQueue::default();
        q.items.push_back("first".into());
        assert!(q.request_flush());
        q.paused = true;

        q.clear();

        assert!(q.items.is_empty());
        assert!(!q.paused);
        assert!(!q.flush_pending);
    }

    #[test]
    fn resume_resets_pause_and_flush() {
        let mut q = PromptQueue::default();
        q.items.push_back("first".into());
        assert!(q.request_flush());
        q.paused = true;

        q.resume();

        assert!(!q.paused);
        assert!(!q.flush_pending);
    }
}
