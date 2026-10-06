use std::path::PathBuf;

use bevy::prelude::Component;
use vmux_api::conversation::Message;
use vmux_api::protocol::AgentPromptEnvelope;
use vmux_chat::host::ImportedConversation;

#[derive(Component)]
pub(super) struct HandoffDriver(pub(super) PathBuf);

impl HandoffDriver {
    pub(super) fn save(
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

    pub(super) fn load(&self, agent_id: &str, session_id: &str) -> Option<ImportedConversation> {
        let bytes = std::fs::read(self.record_path(agent_id, session_id)).ok()?;
        let mut imported = serde_json::from_slice::<ImportedConversation>(&bytes).ok()?;
        let mut fallback = imported.first_prompt.as_deref();
        for message in &mut imported.messages {
            let Message::User { text, .. } = message else {
                continue;
            };
            let prompt = AgentPromptEnvelope::new(text);
            if let Some(display_text) = prompt.display().map(str::to_string) {
                *text = display_text;
            } else if prompt.has_private_context()
                && let Some(display_text) = fallback.take()
            {
                *text = display_text.to_string();
            }
        }
        Some(imported)
    }

    pub(super) fn record_path(&self, agent_id: &str, session_id: &str) -> PathBuf {
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
