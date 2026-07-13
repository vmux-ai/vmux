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
        Message::User { text } if !text.trim().is_empty() => Some(format!("User:\n{text}")),
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
        let Message::User { text } = message else {
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
mod tests {
    use super::*;
    use crate::{AssistantBlock, Message};

    fn user(text: &str) -> Message {
        Message::User {
            text: text.to_string(),
        }
    }

    fn assistant(text: &str) -> Message {
        Message::Assistant {
            blocks: vec![AssistantBlock::Text(text.to_string())],
        }
    }

    #[test]
    fn context_budget_keeps_newest_complete_messages() {
        let messages = vec![
            user("old message that should not fit"),
            assistant("middle message"),
            user("new message"),
        ];

        let built = build_context(&messages, 100);

        assert!(built.truncated);
        assert!(built.text.contains(OMITTED_MARKER));
        assert!(built.text.contains("new message"));
        assert!(!built.text.contains("old message"));
    }

    #[test]
    fn context_budget_preserves_chronological_order() {
        let messages = vec![user("first"), assistant("second"), user("third")];

        let built = build_context(&messages, 1_000);

        let first = built.text.find("first").unwrap();
        let second = built.text.find("second").unwrap();
        let third = built.text.find("third").unwrap();
        assert!(first < second && second < third);
        assert!(!built.truncated);
    }

    #[test]
    fn context_budget_keeps_a_contiguous_newest_suffix() {
        let messages = vec![
            user("old-small"),
            assistant(&"middle-large".repeat(20)),
            user("new-small"),
        ];

        let built = build_context(&messages, 120);

        assert!(built.text.contains("new-small"));
        assert!(!built.text.contains("middle-large"));
        assert!(!built.text.contains("old-small"));
    }

    #[test]
    fn context_ignores_non_text_assistant_blocks_and_tool_results() {
        let messages = vec![
            Message::Assistant {
                blocks: vec![
                    AssistantBlock::Thinking("secret".into()),
                    AssistantBlock::Text("visible".into()),
                    AssistantBlock::ToolUse {
                        call_id: "c".into(),
                        name: "run".into(),
                        args: "{}".into(),
                    },
                ],
            },
            Message::ToolResult {
                call_id: "c".into(),
                content: "tool output".into(),
                is_error: false,
            },
        ];

        let built = build_context(&messages, 1_000);

        assert!(built.text.contains("visible"));
        assert!(!built.text.contains("secret"));
        assert!(!built.text.contains("tool output"));
    }

    #[test]
    fn private_wire_prompt_keeps_display_prompt_separate() {
        let prompt = wire_prompt("prior conversation", "continue here");

        assert!(prompt.starts_with(HANDOFF_PROMPT_PREFIX));
        assert!(prompt.contains("prior conversation"));
        assert!(prompt.ends_with("continue here"));
    }

    #[test]
    fn replay_private_prompt_is_replaced_with_display_prompt() {
        let mut messages = vec![
            user(&wire_prompt("prior conversation", "continue here")),
            assistant("done"),
        ];

        sanitize_replayed_messages(&mut messages, Some("continue here"));

        assert_eq!(messages[0], user("continue here"));
        assert_eq!(messages[1], assistant("done"));
    }

    #[test]
    fn replay_sanitizes_every_retried_private_prompt_from_its_own_payload() {
        let mut messages = vec![
            user(&wire_prompt("prior conversation", "first try")),
            user(&wire_prompt("prior conversation", "second try")),
        ];

        sanitize_replayed_messages(&mut messages, Some("stale sidecar text"));

        assert_eq!(messages, vec![user("first try"), user("second try")]);
    }

    #[test]
    fn replay_preserves_plain_prompt_starting_with_private_prefix() {
        let text = format!("{HANDOFF_PROMPT_PREFIX} ordinary user text");
        let mut messages = vec![user(&text)];

        sanitize_replayed_messages(&mut messages, Some("fallback"));

        assert_eq!(messages, vec![user(&text)]);
    }

    #[test]
    fn pending_context_sends_once_and_can_retry_after_error() {
        let mut pending = PendingHandoff {
            context: "prior conversation".into(),
            sent: false,
        };

        assert_eq!(
            pending.context_for_send().as_deref(),
            Some("prior conversation")
        );
        assert!(pending.context_for_send().is_none());
        pending.retry();
        assert_eq!(
            pending.context_for_send().as_deref(),
            Some("prior conversation")
        );
    }

    #[test]
    fn imported_conversation_sidecar_round_trips() {
        let root = std::env::temp_dir().join(format!(
            "vmux-handoff-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let imported = ImportedConversation {
            source_agent: "Codex".into(),
            source_kind: AgentKind::Codex,
            source_sid: "cx/1".into(),
            messages: vec![user("fix auth"), assistant("working")],
            truncated: true,
            first_prompt: Some("continue".into()),
        };

        save_in(&root, "claude/custom", "target?1", &imported).unwrap();
        let loaded = load_in(&root, "claude/custom", "target?1").unwrap();

        assert_eq!(loaded, imported);
        assert!(record_path_in(&root, "claude/custom", "target?1").starts_with(&root));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_or_malformed_sidecar_is_ignored() {
        let root = std::env::temp_dir().join(format!(
            "vmux-handoff-bad-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        assert!(load_in(&root, "claude", "missing").is_none());
        let path = record_path_in(&root, "claude", "bad");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "not json").unwrap();
        assert!(load_in(&root, "claude", "bad").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn visible_messages_prepend_imported_history() {
        let imported = ImportedConversation {
            source_agent: "Codex".into(),
            source_kind: AgentKind::Codex,
            source_sid: "cx-1".into(),
            messages: vec![user("old")],
            truncated: false,
            first_prompt: Some("new".into()),
        };

        assert_eq!(
            visible_messages(Some(&imported), &[assistant("reply")]),
            vec![user("old"), assistant("reply")]
        );
    }
}
