use bevy::prelude::*;
use vmux_api::protocol::AcpModeOption;
use vmux_chat::event::ModelOptionEntry;
use vmux_ecs::profile::CurrentProfile;
use vmux_path::AtomicFile;

use crate::route::SessionRoute;

pub(super) fn add(app: &mut App) {
    app.add_message::<RememberModel>()
        .add_message::<RememberMode>()
        .add_message::<RememberModels>()
        .add_message::<RememberModes>()
        .add_systems(
            Startup,
            (
                spawn_model_registry,
                ApplyDeferred,
                load_model_selections,
                load_mode_selections,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (model, mode, models, modes).in_set(ModelSelectionSet),
        )
        .add_systems(PostUpdate, (save_model_selections, save_mode_selections));
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ModelSelectionSet;

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

fn spawn_model_registry(mut commands: Commands) {
    commands.spawn((
        Name::new("Agent model registry"),
        AcpSessionConfigRequestCounter::default(),
        AgentModelSelections::default(),
        AgentModeSelections::default(),
    ));
}

fn load_model_selections(profile: CurrentProfile, mut models: Single<&mut AgentModelSelections>) {
    let Some(path) = profile
        .paths()
        .map(|paths| paths.profile().join("agent-models.json"))
    else {
        return;
    };
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let Ok(saved) =
        serde_json::from_slice::<std::collections::BTreeMap<String, AgentModelMemory>>(&bytes)
    else {
        return;
    };
    for (agent, mut memory) in saved {
        if !memory.url.is_empty() && SessionRoute::parse(&memory.url).is_none() {
            memory.url = SessionRoute::manager_for_agent(&vmux_session::AgentId(agent.clone()));
        }
        models.by_agent.insert(agent, memory);
    }
    models.dirty = false;
}

fn load_mode_selections(profile: CurrentProfile, mut modes: Single<&mut AgentModeSelections>) {
    let Some(path) = profile
        .paths()
        .map(|paths| paths.profile().join("agent-modes.json"))
    else {
        return;
    };
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let Ok(saved) =
        serde_json::from_slice::<std::collections::BTreeMap<String, AgentModeMemory>>(&bytes)
    else {
        return;
    };
    for (agent, mut memory) in saved {
        if !memory.url.is_empty() && SessionRoute::parse(&memory.url).is_none() {
            memory.url.clear();
        }
        modes.by_agent.insert(agent, memory);
    }
    modes.dirty = false;
}

fn save_model_selections(profile: CurrentProfile, mut models: Single<&mut AgentModelSelections>) {
    if !models.dirty {
        return;
    }
    let Some(path) = profile
        .paths()
        .map(|paths| paths.profile().join("agent-models.json"))
    else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec_pretty(&models.by_agent) else {
        return;
    };
    if AtomicFile::write(&path, &bytes).is_ok() {
        models.dirty = false;
    }
}

fn save_mode_selections(profile: CurrentProfile, mut modes: Single<&mut AgentModeSelections>) {
    if !modes.dirty {
        return;
    }
    let Some(path) = profile
        .paths()
        .map(|paths| paths.profile().join("agent-modes.json"))
    else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec_pretty(&modes.by_agent) else {
        return;
    };
    if AtomicFile::write(&path, &bytes).is_ok() {
        modes.dirty = false;
    }
}

#[derive(Component, Default)]
pub(super) struct AcpSessionConfigRequestCounter(pub(super) u64);

#[derive(Message)]
pub(super) struct RememberModel {
    pub(super) agent_id: String,
    pub(super) model_id: String,
}

#[derive(Message)]
pub(super) struct RememberMode {
    pub(super) agent_id: String,
    pub(super) mode_id: String,
}

#[derive(Message)]
pub(super) struct RememberModels {
    pub(super) agent_id: String,
    pub(super) url: String,
    pub(super) selected: String,
    pub(super) models: Vec<ModelOptionEntry>,
}

#[derive(Message)]
pub(super) struct RememberModes {
    pub(super) agent_id: String,
    pub(super) url: String,
    pub(super) selected: String,
    pub(super) modes: Vec<AcpModeOption>,
}

fn model(
    mut requests: MessageReader<RememberModel>,
    mut selections: Single<&mut AgentModelSelections>,
) {
    for request in requests.read() {
        let entry = selections
            .by_agent
            .entry(request.agent_id.clone())
            .or_default();
        if !entry.models.is_empty()
            && !entry
                .models
                .iter()
                .any(|model| model.id == request.model_id)
        {
            continue;
        }
        if entry.selected == request.model_id {
            continue;
        }
        entry.selected.clone_from(&request.model_id);
        selections.dirty = true;
    }
}

fn mode(
    mut requests: MessageReader<RememberMode>,
    mut selections: Single<&mut AgentModeSelections>,
) {
    for request in requests.read() {
        let entry = selections
            .by_agent
            .entry(request.agent_id.clone())
            .or_default();
        if !entry.modes.is_empty() && !entry.modes.iter().any(|mode| mode.id == request.mode_id) {
            continue;
        }
        if entry.selected == request.mode_id {
            continue;
        }
        entry.selected.clone_from(&request.mode_id);
        selections.dirty = true;
    }
}

fn models(
    mut requests: MessageReader<RememberModels>,
    mut selections: Single<&mut AgentModelSelections>,
) {
    for request in requests.read() {
        if request.models.is_empty() {
            continue;
        }
        let entry = selections
            .by_agent
            .entry(request.agent_id.clone())
            .or_default();
        let mut changed = false;
        if entry.url != request.url {
            entry.url.clone_from(&request.url);
            changed = true;
        }
        if entry.models != request.models {
            entry.models.clone_from(&request.models);
            changed = true;
        }
        if !entry.models.iter().any(|model| model.id == entry.selected) {
            let next = if entry
                .models
                .iter()
                .any(|model| model.id == request.selected)
            {
                &request.selected
            } else {
                &entry.models[0].id
            };
            if entry.selected != *next {
                entry.selected = next.to_string();
                changed = true;
            }
        }
        if changed {
            selections.dirty = true;
        }
    }
}

fn modes(
    mut requests: MessageReader<RememberModes>,
    mut selections: Single<&mut AgentModeSelections>,
) {
    for request in requests.read() {
        if request.modes.is_empty() {
            continue;
        }
        let entry = selections
            .by_agent
            .entry(request.agent_id.clone())
            .or_default();
        let mut changed = false;
        if entry.url != request.url {
            entry.url.clone_from(&request.url);
            changed = true;
        }
        if entry.modes != request.modes {
            entry.modes.clone_from(&request.modes);
            changed = true;
        }
        if !entry.modes.iter().any(|mode| mode.id == entry.selected) {
            let next = if entry.modes.iter().any(|mode| mode.id == request.selected) {
                &request.selected
            } else {
                &entry.modes[0].id
            };
            if entry.selected != *next {
                entry.selected = next.to_string();
                changed = true;
            }
        }
        if changed {
            selections.dirty = true;
        }
    }
}
