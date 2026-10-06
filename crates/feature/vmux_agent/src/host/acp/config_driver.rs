use bevy::ecs::system::SystemParam;
use bevy::prelude::{Entity, Query};
use vmux_api::protocol::{AcpModeOption, AcpSessionConfig};
use vmux_chat::event::{ModeState, ModelOptionEntry, ModelState};
use vmux_session::{AgentId, Session};

use crate::host::runtime::AcpSessionConfigState;

#[derive(SystemParam)]
pub(super) struct AcpConfigProjection<'w, 's> {
    sessions: Query<
        'w,
        's,
        (&'static AgentId, Option<&'static AcpSessionConfigState>),
        bevy::prelude::With<Session>,
    >,
}

impl AcpSessionConfigState {
    pub fn category(&self, category: &str) -> Option<&AcpSessionConfig> {
        self.configs
            .iter()
            .find(|config| config.category.as_deref() == Some(category))
    }

    pub fn display_value<'a>(&'a self, config: &'a AcpSessionConfig) -> &'a str {
        self.pending
            .iter()
            .find(|pending| pending.config_id == config.config_id)
            .map(|pending| pending.value.as_str())
            .unwrap_or(&config.current_value)
    }

    pub fn display_name<'a>(&'a self, config: &'a AcpSessionConfig) -> &'a str {
        let value = self.display_value(config);
        config
            .values
            .iter()
            .find(|option| option.value == value)
            .map(|option| option.name.as_str())
            .unwrap_or(value)
    }

    pub fn initial_value<'a>(&'a self, config: &'a AcpSessionConfig) -> &'a str {
        self.initial
            .iter()
            .find(|initial| initial.config_id == config.config_id)
            .map(|initial| initial.value.as_str())
            .unwrap_or(&config.current_value)
    }
}

impl AcpConfigProjection<'_, '_> {
    pub(super) fn get(&self, entity: Entity) -> Option<(ModelState, ModeState)> {
        let (agent_id, configs) = self.sessions.get(entity).ok()?;
        let agent_key = agent_id.0.as_str();
        let model = match configs {
            None => ModelState {
                agent_key: agent_key.to_string(),
                ..Default::default()
            },
            Some(configs) => {
                let mut state = ModelState {
                    agent_key: agent_key.to_string(),
                    ..Default::default()
                };
                if let Some(model) = configs.category("model") {
                    state.current_model_id = configs.display_value(model).to_string();
                    state.current_model_name = configs.display_name(model).to_string();
                    state.default_model_id = configs.initial_value(model).to_string();
                    state.models = model
                        .values
                        .iter()
                        .map(|option| ModelOptionEntry {
                            id: option.value.clone(),
                            name: option.name.clone(),
                            description: option.description.clone().unwrap_or_default(),
                        })
                        .collect();
                }
                if let Some(thought) = configs.category("thought_level") {
                    state.effort_current = configs.display_value(thought).to_string();
                    state.effort_default = configs.initial_value(thought).to_string();
                    state.effort_levels = thought
                        .values
                        .iter()
                        .map(|option| option.value.clone())
                        .collect();
                }
                state
            }
        };
        let mode = match configs
            .and_then(|configs| configs.category("mode").map(|mode| (configs, mode)))
        {
            Some((configs, mode)) => ModeState {
                current_mode_id: configs.display_value(mode).to_string(),
                modes: mode
                    .values
                    .iter()
                    .map(|option| AcpModeOption {
                        id: option.value.clone(),
                        name: option.name.clone(),
                        description: option.description.clone(),
                    })
                    .collect(),
            },
            None => ModeState::default(),
        };
        Some((model, mode))
    }
}
