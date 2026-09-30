use bevy::prelude::*;
use vmux_api::protocol::AcpModeOption;
use vmux_chat::event::ModelOptionEntry;
use vmux_command::snapshot::AgentPromptTarget;
use vmux_core::profile::ProfilePaths;
use vmux_path::AtomicFile;

use crate::acp_registry::RegistryAgent;

pub(super) struct ModelSelectionPlugin;

impl Plugin for ModelSelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                spawn_agent_model_registry,
                ApplyDeferred,
                load_agent_model_selections,
                load_agent_mode_selections,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            (save_agent_model_selections, save_agent_mode_selections),
        );
    }
}

#[derive(Component, Default)]
pub(crate) struct AgentModelSelections {
    pub(super) by_agent: std::collections::BTreeMap<String, AgentModelMemory>,
    pub(super) dirty: bool,
}

#[derive(Component, Default)]
pub(crate) struct AgentModeSelections {
    pub(super) by_agent: std::collections::BTreeMap<String, AgentModeMemory>,
    pub(super) dirty: bool,
}

#[derive(Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct AgentModelMemory {
    #[serde(default)]
    pub(super) url: String,
    pub(super) selected: String,
    #[serde(default)]
    pub(super) models: Vec<ModelOptionEntry>,
}

#[derive(Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct AgentModeMemory {
    #[serde(default)]
    pub(super) url: String,
    pub(super) selected: String,
    #[serde(default)]
    pub(super) modes: Vec<AcpModeOption>,
}

pub(super) struct AgentSelectionKey;

impl AgentSelectionKey {
    pub(super) fn normalize(agent_id: &str) -> &str {
        RegistryAgent::url_id(agent_id)
    }

    pub(super) fn acp_url(agent_id: &str) -> String {
        AgentPromptTarget::new(Self::normalize(agent_id)).url()
    }
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
pub(super) enum SavedAgentModel {
    Remembered(AgentModelMemory),
    Selected(String),
}

impl SavedAgentModel {
    pub(super) fn memory(self) -> AgentModelMemory {
        match self {
            Self::Remembered(memory) => memory,
            Self::Selected(selected) => AgentModelMemory {
                url: String::new(),
                selected,
                models: Vec::new(),
            },
        }
    }
}

fn agent_model_selections_path() -> std::path::PathBuf {
    ProfilePaths::current().profile().join("agent-models.json")
}

fn agent_mode_selections_path() -> std::path::PathBuf {
    ProfilePaths::current().profile().join("agent-modes.json")
}

fn spawn_agent_model_registry(mut commands: Commands) {
    commands.spawn((
        Name::new("Agent model registry"),
        AcpSessionConfigRequestCounter::default(),
        AgentModelSelections::default(),
        AgentModeSelections::default(),
    ));
}

fn load_agent_model_selections(mut models: Single<&mut AgentModelSelections>) {
    let Ok(bytes) = std::fs::read(agent_model_selections_path()) else {
        return;
    };
    let Ok(saved) =
        serde_json::from_slice::<std::collections::BTreeMap<String, SavedAgentModel>>(&bytes)
    else {
        return;
    };
    for (agent, entry) in saved {
        let key = AgentSelectionKey::normalize(&agent).to_string();
        let mut memory = entry.memory();
        if !memory.url.is_empty() {
            memory.url = AgentSelectionKey::acp_url(&agent);
        }
        models.by_agent.insert(key, memory);
    }
    models.dirty = false;
}

fn load_agent_mode_selections(mut modes: Single<&mut AgentModeSelections>) {
    let Ok(bytes) = std::fs::read(agent_mode_selections_path()) else {
        return;
    };
    let Ok(saved) =
        serde_json::from_slice::<std::collections::BTreeMap<String, AgentModeMemory>>(&bytes)
    else {
        return;
    };
    for (agent, mut memory) in saved {
        let key = AgentSelectionKey::normalize(&agent).to_string();
        memory.url = AgentSelectionKey::acp_url(&agent);
        modes.by_agent.insert(key, memory);
    }
    modes.dirty = false;
}

fn save_agent_model_selections(mut models: Single<&mut AgentModelSelections>) {
    if !models.dirty {
        return;
    }
    let path = agent_model_selections_path();
    let Ok(bytes) = serde_json::to_vec_pretty(&models.by_agent) else {
        return;
    };
    if AtomicFile::write(&path, &bytes).is_ok() {
        models.dirty = false;
    }
}

fn save_agent_mode_selections(mut modes: Single<&mut AgentModeSelections>) {
    if !modes.dirty {
        return;
    }
    let path = agent_mode_selections_path();
    let Ok(bytes) = serde_json::to_vec_pretty(&modes.by_agent) else {
        return;
    };
    if AtomicFile::write(&path, &bytes).is_ok() {
        modes.dirty = false;
    }
}

#[derive(Component, Default)]
pub(super) struct AcpSessionConfigRequestCounter(u64);

impl AgentModelSelections {
    pub(super) fn select(&mut self, agent_id: &str, model_id: &str) {
        let key = AgentSelectionKey::normalize(agent_id).to_string();
        let entry = self.by_agent.entry(key).or_default();
        if !entry.models.is_empty() && !entry.models.iter().any(|model| model.id == model_id) {
            return;
        }
        if entry.selected == model_id {
            return;
        }
        entry.selected = model_id.to_string();
        self.dirty = true;
    }

    pub(crate) fn selected_for(&self, agent_id: &str) -> &str {
        match self.by_agent.get(AgentSelectionKey::normalize(agent_id)) {
            Some(memory) => &memory.selected,
            None => "",
        }
    }

    pub(super) fn remember_catalog(
        &mut self,
        agent_id: &str,
        url: &str,
        selected: &str,
        models: &[ModelOptionEntry],
    ) {
        if models.is_empty() {
            return;
        }
        let key = AgentSelectionKey::normalize(agent_id).to_string();
        let entry = self.by_agent.entry(key).or_default();
        let mut changed = false;
        if entry.url != url {
            entry.url = url.to_string();
            changed = true;
        }
        if entry.models != models {
            entry.models = models.to_vec();
            changed = true;
        }
        if !entry.models.iter().any(|model| model.id == entry.selected) {
            let next = if entry.models.iter().any(|model| model.id == selected) {
                selected
            } else {
                &entry.models[0].id
            };
            if entry.selected != next {
                entry.selected = next.to_string();
                changed = true;
            }
        }
        if changed {
            self.dirty = true;
        }
    }
}

impl AgentModeSelections {
    pub(super) fn select(&mut self, agent_id: &str, mode_id: &str) {
        let key = AgentSelectionKey::normalize(agent_id).to_string();
        let entry = self.by_agent.entry(key).or_default();
        if !entry.modes.is_empty() && !entry.modes.iter().any(|mode| mode.id == mode_id) {
            return;
        }
        if entry.selected == mode_id {
            return;
        }
        entry.selected = mode_id.to_string();
        self.dirty = true;
    }

    pub(crate) fn selected_for(&self, agent_id: &str) -> &str {
        match self.by_agent.get(AgentSelectionKey::normalize(agent_id)) {
            Some(memory) => &memory.selected,
            None => "",
        }
    }

    pub(super) fn remember_catalog(
        &mut self,
        agent_id: &str,
        url: &str,
        selected: &str,
        modes: &[AcpModeOption],
    ) {
        if modes.is_empty() {
            return;
        }
        let key = AgentSelectionKey::normalize(agent_id).to_string();
        let entry = self.by_agent.entry(key).or_default();
        let mut changed = false;
        if entry.url != url {
            entry.url = url.to_string();
            changed = true;
        }
        if entry.modes != modes {
            entry.modes = modes.to_vec();
            changed = true;
        }
        if !entry.modes.iter().any(|mode| mode.id == entry.selected) {
            let next = if entry.modes.iter().any(|mode| mode.id == selected) {
                selected
            } else {
                &entry.modes[0].id
            };
            if entry.selected != next {
                entry.selected = next.to_string();
                changed = true;
            }
        }
        if changed {
            self.dirty = true;
        }
    }
}

impl AcpSessionConfigRequestCounter {
    pub(super) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }
}
