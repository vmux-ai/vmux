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
        q.enqueue("a".into());
        assert!(q.ready(true));
        assert!(!q.ready(false));
        q.paused = true;
        assert!(!q.ready(true));
    }

    #[test]
    fn take_next_preserves_fifo_without_flush() {
        let mut q = PromptQueue::default();
        q.enqueue("first".into());
        q.enqueue("second".into());
        assert_eq!(
            q.take_next().map(|prompt| prompt.text),
            Some("first".to_string())
        );
        assert_eq!(
            q.items.front().map(|item| item.text.as_str()),
            Some("second")
        );
    }

    #[test]
    fn take_next_merges_all_items_for_flush() {
        let mut q = PromptQueue::default();
        q.enqueue("first".into());
        q.enqueue("second".into());

        assert!(q.request_flush());
        assert_eq!(
            q.take_next().map(|prompt| prompt.text),
            Some("first\n\nsecond".to_string())
        );
        assert!(q.items.is_empty());
        assert!(!q.flush_pending);
    }

    #[test]
    fn take_next_merges_attachments_for_flush() {
        let mut q = PromptQueue::default();
        q.enqueue_with_attachments(
            String::new(),
            vec![AgentAttachment {
                path: "/tmp/a.png".into(),
                name: "a.png".into(),
                mime_type: "image/png".into(),
                size: 3,
            }],
        );
        q.enqueue_with_attachments(
            "describe both".into(),
            vec![AgentAttachment {
                path: "/tmp/b.png".into(),
                name: "b.png".into(),
                mime_type: "image/png".into(),
                size: 4,
            }],
        );

        assert!(q.request_flush());
        let prompt = q.take_next().unwrap();
        assert_eq!(prompt.text, "describe both");
        assert_eq!(prompt.attachments.len(), 2);
    }

    #[test]
    fn enqueue_preserves_pending_flush() {
        let mut q = PromptQueue::default();
        q.enqueue("first".into());
        assert!(q.request_flush());

        q.enqueue("second".into());

        assert!(q.flush_pending);
        assert_eq!(
            q.take_next().map(|prompt| prompt.text),
            Some("first\n\nsecond".to_string())
        );
    }

    #[test]
    fn cancel_flush_clears_pending_flush() {
        let mut q = PromptQueue::default();
        q.enqueue("first".into());
        assert!(q.request_flush());

        q.cancel_flush();

        assert!(!q.flush_pending);
    }

    #[test]
    fn clear_resets_queue_state() {
        let mut q = PromptQueue::default();
        q.enqueue("first".into());
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
        q.enqueue("first".into());
        assert!(q.request_flush());
        q.paused = true;

        q.resume();

        assert!(!q.paused);
        assert!(!q.flush_pending);
    }

    #[test]
    fn remove_targets_stable_id() {
        let mut q = PromptQueue::default();
        q.enqueue("first".into());
        q.enqueue("second".into());
        let second_id = q.items[1].id;

        assert!(q.remove(second_id));
        assert_eq!(q.items.len(), 1);
        assert_eq!(q.items[0].text, "first");
        assert!(!q.remove(second_id));
    }

    #[test]
    fn removing_last_item_resets_queue_state() {
        let mut q = PromptQueue::default();
        q.enqueue("first".into());
        let id = q.items[0].id;
        assert!(q.request_flush());
        q.paused = true;

        assert!(q.remove(id));
        assert!(q.items.is_empty());
        assert!(!q.paused);
        assert!(!q.flush_pending);
    }
}
