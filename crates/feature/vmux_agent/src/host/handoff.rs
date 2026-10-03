use bevy::prelude::*;
use vmux_chat::host::ImportedConversation;

#[cfg(test)]
use vmux_api::protocol::AgentPromptEnvelope;

use super::handoff_driver::HandoffDriver;

pub(super) fn add(app: &mut App) {
    app.add_systems(Startup, spawn)
        .add_systems(Update, load)
        .add_systems(Update, persist.after(vmux_ecs::service::ServiceMessageSet));
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Agent handoff directory"),
        HandoffDriver(
            vmux_ecs::profile::ProfilePaths::current()
                .profile()
                .join("handoffs"),
        ),
    ));
}

fn load(
    directory: Single<&HandoffDriver>,
    sessions: Query<
        (Entity, &vmux_session::AgentId, &vmux_session::AcpSessionId),
        (
            Added<vmux_session::AcpSessionId>,
            Without<ImportedConversation>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, agent_id, session_id) in &sessions {
        let Some(imported) = directory.load(&agent_id.0, &session_id.0) else {
            continue;
        };
        commands.entity(entity).insert(imported);
    }
}

fn persist(
    directory: Single<&HandoffDriver>,
    mut created: MessageReader<crate::host::event::AcpSessionCreated>,
    sessions: Query<(&vmux_session::AcpSession, &ImportedConversation)>,
) {
    for event in created.read() {
        for (session_id, agent_id, imported) in &sessions {
            if session_id.0 != event.sid || imported.first_prompt.is_none() {
                continue;
            }
            if let Err(error) = directory.save(imported, &agent_id.0, &event.acp_session_id) {
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
mod tests {
    use super::*;
    use vmux_api::conversation::{AssistantBlock, Message};

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
        let prompt = AgentPromptEnvelope::compose("continue here", Some("prior conversation"));

        assert!(AgentPromptEnvelope::new(&prompt).has_private_context());
        assert!(prompt.contains("prior conversation"));
        assert!(prompt.ends_with("continue here"));
    }

    #[test]
    fn replay_private_prompt_is_replaced_with_display_prompt() {
        let messages = vec![
            user(&AgentPromptEnvelope::compose(
                "continue here",
                Some("prior conversation"),
            )),
            assistant("done"),
        ];

        let root = tempfile::tempdir().unwrap();
        let directory = HandoffDriver(root.path().to_path_buf());
        let imported = ImportedConversation {
            source_agent: String::new(),
            source_sid: String::new(),
            messages,
            truncated: false,
            first_prompt: Some("continue here".into()),
        };
        directory.save(&imported, "agent", "session").unwrap();
        let messages = directory.load("agent", "session").unwrap().messages;

        assert_eq!(messages[0], user("continue here"));
        assert_eq!(messages[1], assistant("done"));
    }

    #[test]
    fn replay_sanitizes_every_retried_private_prompt_from_its_own_payload() {
        let messages = vec![
            user(&AgentPromptEnvelope::compose(
                "first try",
                Some("prior conversation"),
            )),
            user(&AgentPromptEnvelope::compose(
                "second try",
                Some("prior conversation"),
            )),
        ];

        let root = tempfile::tempdir().unwrap();
        let directory = HandoffDriver(root.path().to_path_buf());
        let imported = ImportedConversation {
            source_agent: String::new(),
            source_sid: String::new(),
            messages,
            truncated: false,
            first_prompt: Some("stale sidecar text".into()),
        };
        directory.save(&imported, "agent", "retry").unwrap();
        let messages = directory.load("agent", "retry").unwrap().messages;

        assert_eq!(messages, vec![user("first try"), user("second try")]);
    }

    #[test]
    fn replay_preserves_plain_prompt_starting_with_private_prefix() {
        let text = "<vmux_handoff_context> ordinary user text";
        let messages = vec![user(text)];

        let root = tempfile::tempdir().unwrap();
        let directory = HandoffDriver(root.path().to_path_buf());
        let imported = ImportedConversation {
            source_agent: String::new(),
            source_sid: String::new(),
            messages,
            truncated: false,
            first_prompt: Some("fallback".into()),
        };
        directory.save(&imported, "agent", "plain").unwrap();
        let messages = directory.load("agent", "plain").unwrap().messages;

        assert_eq!(messages, vec![user(text)]);
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

        let directory = HandoffDriver(root.clone());
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

        let directory = HandoffDriver(root.clone());
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

        let mut messages = imported.messages.clone();
        messages.push(assistant("reply"));
        assert_eq!(messages, vec![user("old"), assistant("reply")]);
    }
}
