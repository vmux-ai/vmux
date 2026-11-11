#[vmux_api::contract(Default, Eq)]
pub struct AgentOption {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub installed: bool,
    pub environment_key: bool,
    pub key_variable: String,
}

#[vmux_api::ui_state(Default, Eq)]
pub struct OnboardingUiState {
    pub agents: Vec<AgentOption>,
    pub agent_id: String,
    pub starting: bool,
    pub session_id: String,
    pub failed: bool,
}

#[vmux_api::ui_event]
pub struct OnboardingConnect {
    pub agent_id: String,
}

#[vmux_api::ui_event]
pub struct OnboardingChooseAgent;

#[vmux_api::ui_event]
pub struct ChatAuthenticate {
    pub method_id: String,
}

#[vmux_api::ui_event]
pub struct ChatCancelSetup;

#[vmux_api::ui_event]
pub struct OnboardingSaveKey {
    pub agent_id: String,
    pub key: vmux_api::Secret,
}

#[vmux_api::ui_event]
pub struct ChatSaveKey {
    pub key: vmux_api::Secret,
}

#[vmux_api::ui_state(Default, Eq)]
pub struct KeyEntryUiState {
    pub agent_id: String,
    pub key_variable: String,
    pub pending: bool,
    pub failed: bool,
}

#[derive(bevy_ecs::prelude::Component, Default)]
pub struct KeyPending(pub bool);
