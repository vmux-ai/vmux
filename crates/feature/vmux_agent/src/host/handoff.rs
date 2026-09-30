use bevy::prelude::*;
use std::path::PathBuf;
use vmux_chat::host::ImportedConversation;

use crate::Message;

pub(super) struct HandoffPlugin;

impl Plugin for HandoffPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_handoff_directory)
            .add_systems(Update, load_imported_conversation)
            .add_systems(
                Update,
                persist_imported_conversation.after(vmux_core::service::ServiceMessageSet),
            );
    }
}

#[derive(Component)]
struct HandoffDirectory(PathBuf);

impl HandoffDirectory {
    fn save(
        &self,
        imported: &ImportedConversation,
        agent_id: &str,
        session_id: &str,
    ) -> Result<(), String> {
        let path = self.record_path(agent_id, session_id);
        let parent = path
            .parent()
            .ok_or_else(|| format!("invalid handoff path {}", path.display()))?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create handoff directory {}: {error}", parent.display()))?;
        let bytes = serde_json::to_vec(imported)
            .map_err(|error| format!("serialize handoff record: {error}"))?;
        std::fs::write(&path, bytes)
            .map_err(|error| format!("write handoff record {}: {error}", path.display()))
    }

    fn load(&self, agent_id: &str, session_id: &str) -> Option<ImportedConversation> {
        let bytes = std::fs::read(self.record_path(agent_id, session_id)).ok()?;
        let mut imported = serde_json::from_slice::<ImportedConversation>(&bytes).ok()?;
        let mut fallback = imported.first_prompt.as_deref();
        for message in &mut imported.messages {
            let Message::User { text, .. } = message else {
                continue;
            };
            if let Some(display_text) =
                vmux_api::protocol::extract_display_prompt(text).map(str::to_string)
            {
                *text = display_text;
            } else if vmux_api::protocol::has_private_context_envelope(text)
                && let Some(display_text) = fallback.take()
            {
                *text = display_text.to_string();
            }
        }
        Some(imported)
    }

    fn record_path(&self, agent_id: &str, session_id: &str) -> PathBuf {
        self.0
            .join(Self::hex_component(agent_id))
            .join(format!("{}.json", Self::hex_component(session_id)))
    }

    fn hex_component(value: &str) -> String {
        value
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

fn spawn_handoff_directory(mut commands: Commands) {
    commands.spawn((
        Name::new("Agent handoff directory"),
        HandoffDirectory(
            vmux_core::profile::ProfilePaths::current()
                .profile()
                .join("handoffs"),
        ),
    ));
}

fn load_imported_conversation(
    directory: Single<&HandoffDirectory>,
    sessions: Query<
        (Entity, &vmux_session::AcpSession),
        (
            Added<vmux_session::AcpSession>,
            Without<ImportedConversation>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, session) in &sessions {
        let Some(session_id) = session.resume.as_deref() else {
            continue;
        };
        let Some(imported) = directory.load(&session.agent_id, session_id) else {
            continue;
        };
        commands.entity(entity).insert(imported);
    }
}

fn persist_imported_conversation(
    directory: Single<&HandoffDirectory>,
    mut created: MessageReader<crate::event::UiAgentSessionCreated>,
    sessions: Query<(&vmux_session::AcpSession, &ImportedConversation)>,
) {
    for event in created.read() {
        for (session, imported) in &sessions {
            if session.sid != event.sid || imported.first_prompt.is_none() {
                continue;
            }
            if let Err(error) = directory.save(imported, &session.agent_id, &event.acp_session_id) {
                bevy::log::warn!("acp: failed to persist handoff metadata: {error}");
            }
        }
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PendingHandoff {
    pub context: String,
    pub sent: bool,
}

#[cfg(test)]
fn wire_prompt(context: &str, display_text: &str) -> String {
    vmux_api::protocol::compose_agent_prompt(display_text, Some(context))
}

#[cfg(test)]
fn visible_messages(imported: Option<&ImportedConversation>, live: &[Message]) -> Vec<Message> {
    let mut messages = imported
        .map(|imported| imported.messages.clone())
        .unwrap_or_default();
    messages.extend_from_slice(live);
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssistantBlock, Message};

    struct TestHandoffDirectory {
        directory: HandoffDirectory,
    }

    impl TestHandoffDirectory {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "vmux-handoff-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            Self {
                directory: HandoffDirectory(root),
            }
        }
    }

    impl Drop for TestHandoffDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory.0);
        }
    }

    fn user(text: &str) -> Message {
        Message::user(text)
    }

    fn assistant(text: &str) -> Message {
        Message::Assistant {
            blocks: vec![AssistantBlock::Text(text.to_string())],
        }
    }

    #[test]
    fn private_wire_prompt_keeps_display_prompt_separate() {
        let prompt = wire_prompt("prior conversation", "continue here");

        assert!(prompt.starts_with(vmux_api::protocol::PRIVATE_CONTEXT_PREFIX));
        assert!(prompt.contains("prior conversation"));
        assert!(prompt.ends_with("continue here"));
    }

    #[test]
    fn replay_private_prompt_is_replaced_with_display_prompt() {
        let messages = vec![
            user(&wire_prompt("prior conversation", "continue here")),
            assistant("done"),
        ];

        let directory = TestHandoffDirectory::new("replay");
        let imported = ImportedConversation {
            source_agent: String::new(),
            source_sid: String::new(),
            messages,
            truncated: false,
            first_prompt: Some("continue here".into()),
        };
        directory
            .directory
            .save(&imported, "agent", "session")
            .unwrap();
        let messages = directory
            .directory
            .load("agent", "session")
            .unwrap()
            .messages;

        assert_eq!(messages[0], user("continue here"));
        assert_eq!(messages[1], assistant("done"));
    }

    #[test]
    fn replay_sanitizes_every_retried_private_prompt_from_its_own_payload() {
        let messages = vec![
            user(&wire_prompt("prior conversation", "first try")),
            user(&wire_prompt("prior conversation", "second try")),
        ];

        let directory = TestHandoffDirectory::new("retry");
        let imported = ImportedConversation {
            source_agent: String::new(),
            source_sid: String::new(),
            messages,
            truncated: false,
            first_prompt: Some("stale sidecar text".into()),
        };
        directory
            .directory
            .save(&imported, "agent", "retry")
            .unwrap();
        let messages = directory.directory.load("agent", "retry").unwrap().messages;

        assert_eq!(messages, vec![user("first try"), user("second try")]);
    }

    #[test]
    fn replay_preserves_plain_prompt_starting_with_private_prefix() {
        let text = format!(
            "{} ordinary user text",
            vmux_api::protocol::PRIVATE_CONTEXT_PREFIX
        );
        let messages = vec![user(&text)];

        let directory = TestHandoffDirectory::new("plain");
        let imported = ImportedConversation {
            source_agent: String::new(),
            source_sid: String::new(),
            messages,
            truncated: false,
            first_prompt: Some("fallback".into()),
        };
        directory
            .directory
            .save(&imported, "agent", "plain")
            .unwrap();
        let messages = directory.directory.load("agent", "plain").unwrap().messages;

        assert_eq!(messages, vec![user(&text)]);
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
            source_sid: "cx/1".into(),
            messages: vec![user("fix auth"), assistant("working")],
            truncated: true,
            first_prompt: Some("continue".into()),
        };

        let directory = HandoffDirectory(root.clone());
        directory
            .save(&imported, "claude/custom", "target?1")
            .unwrap();
        let loaded = directory.load("claude/custom", "target?1").unwrap();

        assert_eq!(loaded, imported);
        assert!(
            directory
                .record_path("claude/custom", "target?1")
                .starts_with(&root)
        );
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

        let directory = HandoffDirectory(root.clone());
        assert!(directory.load("claude", "missing").is_none());
        let path = directory.record_path("claude", "bad");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "not json").unwrap();
        assert!(directory.load("claude", "bad").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn visible_messages_prepend_imported_history() {
        let imported = ImportedConversation {
            source_agent: "Codex".into(),
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
