use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};

use crate::client::acp::{AcpModeState, AcpModelState};
use crate::events::AgentCommandRequest;
use crate::strategy::{AgentStrategies, acp_agent_kind, kind_supports_cross_runtime};
use vmux_chat::event::{
    MODE_STATE_EVENT, MODEL_STATE_EVENT, ModeState, ModelOptionEntry, ModelState,
    SLASH_COMMANDS_EVENT, SelectMode, SelectModel, SetAgentEffort, SlashCommands,
};
use vmux_command::event::{StartSelectMode, StartSelectModel};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{AgentCommand, AgentCommandResult, ClientMessage, SharedAgentCommand};
use vmux_session::AcpSession;
use vmux_wire::room::RemoteModelState;

pub(super) struct ChatModelPlugin;

impl Plugin for ChatModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AcpModelRequestCounter>()
            .init_resource::<AcpModeRequestCounter>()
            .init_resource::<AgentModelSelections>()
            .init_resource::<AgentModeSelections>()
            .init_resource::<vmux_command::snapshot::CommandBarAgentModels>()
            .init_resource::<vmux_command::snapshot::CommandBarAgentModes>()
            .add_message::<AcpSetModelRequest>()
            .add_message::<AcpSetModeRequest>()
            .add_message::<ModeSelectRequest>()
            .add_message::<ModelSelectRequest>()
            .add_message::<EffortSetRequest>()
            .add_plugins(BinEventEmitterPlugin::<(
                SelectModel,
                SetAgentEffort,
                SelectMode,
            )>::for_hosts(super::CHAT_EVENT_HOSTS))
            .add_plugins(
                BinEventEmitterPlugin::<(StartSelectModel, StartSelectMode)>::for_hosts(&["start"]),
            )
            .add_systems(
                Startup,
                (
                    load_agent_model_selections,
                    load_agent_mode_selections,
                    seed_cli_model_lists,
                )
                    .chain(),
            )
            .add_observer(on_select_model)
            .add_observer(on_select_mode)
            .add_observer(on_set_agent_effort)
            .add_observer(on_start_select_model)
            .add_observer(on_start_select_mode)
            .add_systems(
                Update,
                (
                    answer_remote_model_commands,
                    apply_model_selection,
                    apply_mode_selection,
                    apply_effort_setting,
                    push_acp_model_state_to_page,
                    push_removed_acp_model_state_to_page,
                    push_acp_mode_state_to_page,
                    push_removed_acp_mode_state_to_page,
                    apply_last_used_acp_model.after(crate::client::acp::apply_acp_model_info),
                    send_acp_model_requests,
                    send_acp_mode_requests,
                    remember_acp_model_lists,
                    remember_acp_mode_lists,
                    publish_agent_models.after(remember_acp_model_lists),
                    publish_agent_modes.after(remember_acp_mode_lists),
                    save_agent_model_selections
                        .after(apply_last_used_acp_model)
                        .after(remember_acp_model_lists),
                    save_agent_mode_selections.after(remember_acp_mode_lists),
                ),
            );
    }
}

#[derive(Message)]
pub(super) struct ModelSelectRequest {
    pub sid: String,
    pub model_id: String,
}

#[derive(Message)]
pub(super) struct EffortSetRequest {
    pub agent_key: String,
    pub level: String,
}

fn answer_remote_model_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    service: Option<Res<ServiceClient>>,
    sessions: Query<(&AcpSession, &AcpModelState)>,
    settings: Res<vmux_setting::AppSettings>,
    mut selects: MessageWriter<ModelSelectRequest>,
    mut efforts: MessageWriter<EffortSetRequest>,
) {
    for request in reader.read() {
        let AgentCommand::Shared(command) = &request.command else {
            continue;
        };
        let result = match command {
            SharedAgentCommand::ListModels { sid } => {
                match remote_model_state(sid, &sessions, &settings) {
                    Some(state) => match serde_json::to_string(&state) {
                        Ok(json) => AgentCommandResult::Text(json),
                        Err(error) => AgentCommandResult::Error(format!("list_models: {error}")),
                    },
                    None => AgentCommandResult::Error("no such session".to_string()),
                }
            }
            SharedAgentCommand::SelectModel { sid, model_id } => {
                if !sessions.iter().any(|(session, _)| session.sid == *sid) {
                    AgentCommandResult::Error("no such session".to_string())
                } else {
                    selects.write(ModelSelectRequest {
                        sid: sid.clone(),
                        model_id: model_id.clone(),
                    });
                    AgentCommandResult::Ok
                }
            }
            SharedAgentCommand::SetEffort { sid, level } => {
                match sessions.iter().find(|(session, _)| session.sid == *sid) {
                    Some((session, _)) => {
                        efforts.write(EffortSetRequest {
                            agent_key: session.agent_id.clone(),
                            level: level.clone(),
                        });
                        AgentCommandResult::Ok
                    }
                    None => AgentCommandResult::Error("no such session".to_string()),
                }
            }
            _ => continue,
        };
        if let Some(service) = service.as_ref() {
            service.0.send(ClientMessage::AgentCommandResponse {
                request_id: request.request_id,
                result,
            });
        }
    }
}

fn remote_model_state(
    sid: &str,
    sessions: &Query<(&AcpSession, &AcpModelState)>,
    settings: &vmux_setting::AppSettings,
) -> Option<RemoteModelState> {
    let (session, model_state) = sessions.iter().find(|(session, _)| session.sid == sid)?;
    let mut models = Vec::new();
    for option in &model_state.models {
        models.push(ModelOptionEntry {
            id: option.id.clone(),
            name: option.name.clone(),
            description: option.description.clone().unwrap_or_default(),
        });
    }
    let mut effort_levels = Vec::new();
    for level in vmux_core::agent::effort_levels(&session.agent_id) {
        effort_levels.push((*level).to_string());
    }
    Some(RemoteModelState {
        models,
        selected_id: model_state.display_model_id().to_string(),
        effort_levels,
        effort: settings
            .agent
            .effort
            .get(&session.agent_id)
            .cloned()
            .unwrap_or_default(),
    })
}

fn apply_model_selection(
    mut reader: MessageReader<ModelSelectRequest>,
    mut sessions: Query<(&AcpSession, &mut AcpModelState)>,
    mut counter: ResMut<AcpModelRequestCounter>,
    mut last_used: ResMut<AgentModelSelections>,
    mut requests: MessageWriter<AcpSetModelRequest>,
) {
    for request in reader.read() {
        let Some((session, mut model_state)) = sessions
            .iter_mut()
            .find(|(session, _)| session.sid == request.sid)
        else {
            continue;
        };
        let model_id = request.model_id.clone();
        if model_state.display_model_id() == model_id
            || !model_state.models.iter().any(|model| model.id == model_id)
        {
            continue;
        }
        let request_id = counter.next();
        last_used.select(&session.agent_id, &model_id);
        requests.write(AcpSetModelRequest {
            sid: session.sid.clone(),
            request_id,
            config_id: model_state.config_id.clone(),
            model_id: model_id.clone(),
        });
        model_state.pending = Some(crate::client::acp::PendingAcpModelSelection {
            request_id,
            model_id,
        });
    }
}

#[derive(Message)]
struct AcpSetModelRequest {
    sid: String,
    request_id: u64,
    config_id: String,
    model_id: String,
}

#[derive(Message)]
struct AcpSetModeRequest {
    sid: String,
    request_id: u64,
    config_id: String,
    mode_id: String,
}

#[derive(Message)]
struct ModeSelectRequest {
    sid: String,
    mode_id: String,
}

#[derive(Resource, Default)]
pub(crate) struct AgentModelSelections {
    by_agent: std::collections::BTreeMap<String, AgentModelMemory>,
    dirty: bool,
}

#[derive(Resource, Default)]
pub(crate) struct AgentModeSelections {
    by_agent: std::collections::BTreeMap<String, AgentModeMemory>,
    dirty: bool,
}

#[derive(Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct AgentModelMemory {
    #[serde(default)]
    url: String,
    selected: String,
    #[serde(default)]
    models: Vec<ModelOptionEntry>,
}

#[derive(Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct AgentModeMemory {
    #[serde(default)]
    url: String,
    selected: String,
    #[serde(default)]
    modes: Vec<vmux_wire::protocol::AcpModeOption>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum SavedAgentModel {
    Remembered(AgentModelMemory),
    Selected(String),
}

impl SavedAgentModel {
    fn memory(self) -> AgentModelMemory {
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
    vmux_core::profile::profile_dir().join("agent-models.json")
}

fn agent_mode_selections_path() -> std::path::PathBuf {
    vmux_core::profile::profile_dir().join("agent-modes.json")
}

fn load_agent_model_selections(mut models: ResMut<AgentModelSelections>) {
    let Ok(bytes) = std::fs::read(agent_model_selections_path()) else {
        return;
    };
    let Ok(saved) =
        serde_json::from_slice::<std::collections::BTreeMap<String, SavedAgentModel>>(&bytes)
    else {
        return;
    };
    for (agent, entry) in saved {
        models.by_agent.insert(agent, entry.memory());
    }
    models.dirty = false;
}

fn load_agent_mode_selections(mut modes: ResMut<AgentModeSelections>) {
    let Ok(bytes) = std::fs::read(agent_mode_selections_path()) else {
        return;
    };
    let Ok(saved) =
        serde_json::from_slice::<std::collections::BTreeMap<String, AgentModeMemory>>(&bytes)
    else {
        return;
    };
    modes.by_agent = saved;
    modes.dirty = false;
}

fn save_agent_model_selections(mut models: ResMut<AgentModelSelections>) {
    if !models.dirty {
        return;
    }
    let path = agent_model_selections_path();
    let Some(parent) = path.parent() else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec_pretty(&models.by_agent) else {
        return;
    };
    let temp = path.with_extension("json.tmp");
    if std::fs::create_dir_all(parent).is_ok()
        && std::fs::write(&temp, bytes).is_ok()
        && std::fs::rename(&temp, &path).is_ok()
    {
        models.dirty = false;
    }
}

fn save_agent_mode_selections(mut modes: ResMut<AgentModeSelections>) {
    if !modes.dirty {
        return;
    }
    let path = agent_mode_selections_path();
    let Some(parent) = path.parent() else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec_pretty(&modes.by_agent) else {
        return;
    };
    let temp = path.with_extension("json.tmp");
    if std::fs::create_dir_all(parent).is_ok()
        && std::fs::write(&temp, bytes).is_ok()
        && std::fs::rename(&temp, &path).is_ok()
    {
        modes.dirty = false;
    }
}

#[derive(Resource, Default)]
struct AcpModelRequestCounter(u64);

#[derive(Resource, Default)]
struct AcpModeRequestCounter(u64);

fn model_state_of(state: Option<&AcpModelState>) -> ModelState {
    let Some(state) = state else {
        return ModelState::default();
    };
    ModelState {
        current_model_id: state.display_model_id().to_string(),
        current_model_name: state.current_name().to_string(),
        default_model_id: state.default_model_id.clone(),
        models: state
            .models
            .iter()
            .map(|model| ModelOptionEntry {
                id: model.id.clone(),
                name: model.name.clone(),
                description: model.description.clone().unwrap_or_default(),
            })
            .collect(),
        ..Default::default()
    }
}

pub(super) fn emit_model_state(
    webview: Entity,
    model_state: Option<&AcpModelState>,
    cross_runtime: bool,
    agent_key: &str,
    effort_current: &str,
    commands: &mut Commands,
) {
    let mut state = model_state_of(model_state);
    state.agent_key = agent_key.to_string();
    state.effort_current = effort_current.to_string();
    state.effort_default = vmux_core::agent::default_effort(agent_key).to_string();
    state.effort_levels = vmux_core::agent::effort_levels(agent_key)
        .iter()
        .map(|level| level.to_string())
        .collect();
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        MODEL_STATE_EVENT,
        &state,
    ));
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        SLASH_COMMANDS_EVENT,
        &SlashCommands::for_agent(cross_runtime, model_state.is_some()),
    ));
}

pub(super) fn emit_mode_state(
    webview: Entity,
    mode_state: Option<&AcpModeState>,
    commands: &mut Commands,
) {
    let state = match mode_state {
        Some(state) => ModeState {
            current_mode_id: state.display_mode_id().to_string(),
            modes: state.modes.clone(),
        },
        None => ModeState::default(),
    };
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        MODE_STATE_EVENT,
        &state,
    ));
}

pub(super) fn effort_current_for<'a>(
    settings: Option<&'a Res<vmux_setting::AppSettings>>,
    agent_key: &str,
) -> &'a str {
    settings
        .and_then(|settings| settings.agent.effort_for(agent_key))
        .unwrap_or("")
}

fn on_start_select_model(
    trigger: On<BinReceive<StartSelectModel>>,
    mut last_used: ResMut<AgentModelSelections>,
) {
    let request = &trigger.event().payload;
    if request.agent_key.is_empty() || request.model_id.is_empty() {
        return;
    }
    last_used.select(&request.agent_key, &request.model_id);
}

fn on_start_select_mode(
    trigger: On<BinReceive<StartSelectMode>>,
    mut last_used: ResMut<AgentModeSelections>,
) {
    let request = &trigger.event().payload;
    if request.agent_key.is_empty() || request.mode_id.is_empty() {
        return;
    }
    last_used.select(&request.agent_key, &request.mode_id);
}

fn seed_cli_model_lists(
    strategies: Res<AgentStrategies>,
    mut selections: ResMut<AgentModelSelections>,
) {
    for strategy in strategies.cli_strategies() {
        let catalog = strategy.model_catalog();
        if catalog.models.is_empty() {
            continue;
        }
        let kind = strategy.kind();
        let agent_key = format!("cli:{}", kind.as_url_segment());
        let url = vmux_command::snapshot::AgentPromptTarget::Cli(kind).url();
        selections.remember_catalog(&agent_key, &url, &catalog.selected, &catalog.models);
    }
}

fn remember_acp_model_lists(
    sessions: Query<(&AcpSession, &AcpModelState), Changed<AcpModelState>>,
    mut last_used: ResMut<AgentModelSelections>,
) {
    for (session, state) in &sessions {
        let listed = model_state_of(Some(state)).models;
        let current = state.display_model_id().to_string();
        let url = vmux_command::snapshot::AgentPromptTarget::Acp {
            id: session.agent_id.clone(),
        }
        .url();
        last_used.remember_catalog(&session.agent_id, &url, &current, &listed);
    }
}

fn remember_acp_mode_lists(
    sessions: Query<(&AcpSession, &AcpModeState), Changed<AcpModeState>>,
    mut last_used: ResMut<AgentModeSelections>,
) {
    for (session, state) in &sessions {
        let url = vmux_command::snapshot::AgentPromptTarget::Acp {
            id: session.agent_id.clone(),
        }
        .url();
        last_used.remember_catalog(
            &session.agent_id,
            &url,
            state.display_mode_id(),
            &state.modes,
        );
    }
}

fn publish_agent_models(
    last_used: Res<AgentModelSelections>,
    mut published: ResMut<vmux_command::snapshot::CommandBarAgentModels>,
) {
    if !last_used.is_changed() {
        return;
    }
    let mut next = Vec::new();
    for (agent_key, memory) in &last_used.by_agent {
        if memory.url.is_empty() || memory.models.is_empty() {
            continue;
        }
        next.push(vmux_wire::command_bar::AgentModels {
            agent_key: agent_key.clone(),
            url: memory.url.clone(),
            selected: memory.selected.clone(),
            models: memory.models.clone(),
        });
    }
    if published.agents != next {
        published.agents = next;
    }
}

fn publish_agent_modes(
    last_used: Res<AgentModeSelections>,
    mut published: ResMut<vmux_command::snapshot::CommandBarAgentModes>,
) {
    if !last_used.is_changed() {
        return;
    }
    let mut next = Vec::new();
    for (agent_key, memory) in &last_used.by_agent {
        if memory.url.is_empty() || memory.modes.is_empty() {
            continue;
        }
        next.push(vmux_wire::command_bar::AgentModes {
            agent_key: agent_key.clone(),
            url: memory.url.clone(),
            selected: memory.selected.clone(),
            modes: memory.modes.clone(),
        });
    }
    if published.agents != next {
        published.agents = next;
    }
}

fn push_acp_model_state_to_page(
    sessions: Query<(Entity, &AcpSession, &AcpModelState), Changed<AcpModelState>>,
    children: Query<&Children>,
    is_browser: Query<(), With<vmux_layout::Browser>>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (stack, session, model_state) in &sessions {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| is_browser.contains(entity)) else {
            continue;
        };
        if !browsers.can_emit_to(&webview) {
            continue;
        }
        let cross = acp_agent_kind(&session.agent_id)
            .map(kind_supports_cross_runtime)
            .unwrap_or(false);
        emit_model_state(
            webview,
            Some(model_state),
            cross,
            &session.agent_id,
            effort_current_for(settings.as_ref(), &session.agent_id),
            &mut commands,
        );
    }
}

fn push_removed_acp_model_state_to_page(
    mut removed: RemovedComponents<AcpModelState>,
    sessions: Query<&AcpSession>,
    children: Query<&Children>,
    is_browser: Query<(), With<vmux_layout::Browser>>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for stack in removed.read() {
        let Ok(session) = sessions.get(stack) else {
            continue;
        };
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| is_browser.contains(entity)) else {
            continue;
        };
        if !browsers.can_emit_to(&webview) {
            continue;
        }
        let cross = acp_agent_kind(&session.agent_id)
            .map(kind_supports_cross_runtime)
            .unwrap_or(false);
        emit_model_state(
            webview,
            None,
            cross,
            &session.agent_id,
            effort_current_for(settings.as_ref(), &session.agent_id),
            &mut commands,
        );
    }
}

fn push_acp_mode_state_to_page(
    sessions: Query<(Entity, &AcpModeState), Changed<AcpModeState>>,
    children: Query<&Children>,
    is_browser: Query<(), With<vmux_layout::Browser>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (stack, mode_state) in &sessions {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| is_browser.contains(entity)) else {
            continue;
        };
        if browsers.can_emit_to(&webview) {
            emit_mode_state(webview, Some(mode_state), &mut commands);
        }
    }
}

fn push_removed_acp_mode_state_to_page(
    mut removed: RemovedComponents<AcpModeState>,
    children: Query<&Children>,
    is_browser: Query<(), With<vmux_layout::Browser>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for stack in removed.read() {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| is_browser.contains(entity)) else {
            continue;
        };
        if browsers.can_emit_to(&webview) {
            emit_mode_state(webview, None, &mut commands);
        }
    }
}

fn on_select_model(
    trigger: On<BinReceive<SelectModel>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut selects: MessageWriter<ModelSelectRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    selects.write(ModelSelectRequest {
        sid: session.sid.clone(),
        model_id: trigger.event().payload.model_id.clone(),
    });
}

fn on_select_mode(
    trigger: On<BinReceive<SelectMode>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut selects: MessageWriter<ModeSelectRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    selects.write(ModeSelectRequest {
        sid: session.sid.clone(),
        mode_id: trigger.event().payload.mode_id.clone(),
    });
}

fn apply_mode_selection(
    mut reader: MessageReader<ModeSelectRequest>,
    mut sessions: Query<(&AcpSession, &mut AcpModeState)>,
    mut counter: ResMut<AcpModeRequestCounter>,
    mut last_used: ResMut<AgentModeSelections>,
    mut requests: MessageWriter<AcpSetModeRequest>,
) {
    for selection in reader.read() {
        let Some((session, mut state)) = sessions
            .iter_mut()
            .find(|(session, _)| session.sid == selection.sid)
        else {
            continue;
        };
        if state.display_mode_id() == selection.mode_id
            || !state.modes.iter().any(|mode| mode.id == selection.mode_id)
        {
            continue;
        }
        let request_id = counter.next();
        last_used.select(&session.agent_id, &selection.mode_id);
        requests.write(AcpSetModeRequest {
            sid: session.sid.clone(),
            request_id,
            config_id: state.config_id.clone(),
            mode_id: selection.mode_id.clone(),
        });
        state.pending = Some(crate::client::acp::PendingAcpModeSelection {
            request_id,
            mode_id: selection.mode_id.clone(),
        });
    }
}

fn on_set_agent_effort(
    trigger: On<BinReceive<SetAgentEffort>>,
    mut efforts: MessageWriter<EffortSetRequest>,
) {
    let payload = &trigger.event().payload;
    efforts.write(EffortSetRequest {
        agent_key: payload.agent_key.trim().to_string(),
        level: payload.level.trim().to_string(),
    });
}

fn apply_effort_setting(
    mut reader: MessageReader<EffortSetRequest>,
    mut settings: ResMut<vmux_setting::AppSettings>,
    mut writes: MessageWriter<vmux_setting::SettingsWriteRequest>,
) {
    for request in reader.read() {
        let (agent_key, level) = (request.agent_key.as_str(), request.level.as_str());
        if agent_key.is_empty() {
            continue;
        }
        if !level.is_empty() && !vmux_core::agent::effort_levels(agent_key).contains(&level) {
            continue;
        }
        let mut effort = settings.agent.effort.clone();
        if level.is_empty() {
            if effort.remove(agent_key).is_none() {
                continue;
            }
        } else if effort.get(agent_key).map(String::as_str) == Some(level) {
            continue;
        } else {
            effort.insert(agent_key.to_string(), level.to_string());
        }
        let value = match serde_json::to_value(&effort) {
            Ok(value) => value,
            Err(error) => {
                bevy::log::warn!("effort: serialize failed: {error}");
                continue;
            }
        };
        match settings.apply_update("agent.effort", value) {
            Ok(ron_bytes) => {
                writes.write(vmux_setting::SettingsWriteRequest { ron_bytes });
            }
            Err(error) => bevy::log::warn!("effort: persist for {agent_key} failed: {error}"),
        }
    }
}

fn apply_last_used_acp_model(
    mut sessions: Query<(&AcpSession, &mut AcpModelState), Added<AcpModelState>>,
    last_used: Res<AgentModelSelections>,
    mut counter: ResMut<AcpModelRequestCounter>,
    mut requests: MessageWriter<AcpSetModelRequest>,
) {
    for (session, mut state) in &mut sessions {
        let Some(remembered) = last_used.by_agent.get(&session.agent_id) else {
            continue;
        };
        let model_id = &remembered.selected;
        if model_id.is_empty() {
            continue;
        }
        if state.display_model_id() == model_id
            || !state.models.iter().any(|model| &model.id == model_id)
        {
            continue;
        }
        let request_id = counter.next();
        requests.write(AcpSetModelRequest {
            sid: session.sid.clone(),
            request_id,
            config_id: state.config_id.clone(),
            model_id: model_id.clone(),
        });
        state.pending = Some(crate::client::acp::PendingAcpModelSelection {
            request_id,
            model_id: model_id.clone(),
        });
    }
}

fn send_acp_model_requests(
    mut requests: MessageReader<AcpSetModelRequest>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for request in requests.read() {
        service.0.send(ClientMessage::AcpSetModel {
            sid: request.sid.clone(),
            request_id: request.request_id,
            config_id: request.config_id.clone(),
            model_id: request.model_id.clone(),
        });
    }
}

fn send_acp_mode_requests(
    mut requests: MessageReader<AcpSetModeRequest>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for request in requests.read() {
        service.0.send(ClientMessage::AcpSetMode {
            sid: request.sid.clone(),
            request_id: request.request_id,
            config_id: request.config_id.clone(),
            mode_id: request.mode_id.clone(),
        });
    }
}

impl AgentModelSelections {
    fn select(&mut self, agent_id: &str, model_id: &str) {
        let entry = self.by_agent.entry(agent_id.to_string()).or_default();
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
        match self.by_agent.get(agent_id) {
            Some(memory) => &memory.selected,
            None => "",
        }
    }

    fn remember_catalog(
        &mut self,
        agent_id: &str,
        url: &str,
        selected: &str,
        models: &[ModelOptionEntry],
    ) {
        if models.is_empty() {
            return;
        }
        let entry = self.by_agent.entry(agent_id.to_string()).or_default();
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
    fn select(&mut self, agent_id: &str, mode_id: &str) {
        let entry = self.by_agent.entry(agent_id.to_string()).or_default();
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
        match self.by_agent.get(agent_id) {
            Some(memory) => &memory.selected,
            None => "",
        }
    }

    fn remember_catalog(
        &mut self,
        agent_id: &str,
        url: &str,
        selected: &str,
        modes: &[vmux_wire::protocol::AcpModeOption],
    ) {
        if modes.is_empty() {
            return;
        }
        let entry = self.by_agent.entry(agent_id.to_string()).or_default();
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

impl AcpModelRequestCounter {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }
}

impl AcpModeRequestCounter {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_commands_include_mcp_and_gate_cli_by_runtime() {
        let names = |cross, models| {
            SlashCommands::for_agent(cross, models)
                .commands
                .iter()
                .map(|command| command.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(false, false), ["upload", "resume", "mcp"]);
        assert_eq!(names(false, true), ["upload", "resume", "mcp", "model"]);
        assert_eq!(names(true, false), ["upload", "resume", "mcp", "cli"]);
        assert_eq!(
            names(true, true),
            ["upload", "resume", "mcp", "model", "cli"]
        );
    }

    #[test]
    fn model_selection_updates_cached_state_before_response() {
        let mut app = App::new();
        app.init_resource::<AcpModelRequestCounter>()
            .init_resource::<AgentModelSelections>()
            .add_message::<AcpSetModelRequest>()
            .add_message::<ModelSelectRequest>()
            .add_observer(on_select_model)
            .add_systems(Update, apply_model_selection);
        let stack = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s1".into(),
                    cwd: "/tmp".into(),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AcpModelState {
                    config_id: "model".into(),
                    current_model_id: "default".into(),
                    default_model_id: "default".into(),
                    pending: None,
                    models: vec![
                        vmux_service::protocol::AcpModelOption {
                            id: "default".into(),
                            name: "Default".into(),
                            description: None,
                        },
                        vmux_service::protocol::AcpModelOption {
                            id: "fable".into(),
                            name: "Fable".into(),
                            description: None,
                        },
                    ],
                },
            ))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive {
            webview,
            payload: SelectModel {
                model_id: "fable".into(),
            },
        });
        app.update();

        let state = app.world().get::<AcpModelState>(stack).unwrap();
        assert_eq!(state.current_model_id, "default");
        assert_eq!(
            state.pending.as_ref().map(|pending| pending.request_id),
            Some(1)
        );
        assert_eq!(
            state
                .pending
                .as_ref()
                .map(|pending| pending.model_id.as_str()),
            Some("fable")
        );
        assert_eq!(state.current_name(), "Fable");
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AcpSetModelRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].sid, "s1");
        assert_eq!(requests[0].request_id, 1);
        assert_eq!(requests[0].config_id, "model");
        assert_eq!(requests[0].model_id, "fable");
        assert_eq!(
            app.world()
                .resource::<AgentModelSelections>()
                .selected_for("claude"),
            "fable"
        );

        app.world_mut().trigger(BinReceive {
            webview,
            payload: SelectModel {
                model_id: "fable".into(),
            },
        });
        app.world_mut().trigger(BinReceive {
            webview,
            payload: SelectModel {
                model_id: "missing".into(),
            },
        });
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<AcpSetModelRequest>>()
                .drain()
                .count(),
            0
        );
    }

    #[test]
    fn mode_selection_updates_cached_state_before_response() {
        let mut app = App::new();
        app.init_resource::<AcpModeRequestCounter>()
            .init_resource::<AgentModeSelections>()
            .add_message::<AcpSetModeRequest>()
            .add_message::<ModeSelectRequest>()
            .add_observer(on_select_mode)
            .add_systems(Update, apply_mode_selection);
        let stack = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s1".into(),
                    cwd: "/tmp".into(),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AcpModeState {
                    config_id: String::new(),
                    current_mode_id: "ask".into(),
                    pending: None,
                    modes: vec![
                        vmux_service::protocol::AcpModeOption {
                            id: "ask".into(),
                            name: "Ask".into(),
                            description: None,
                        },
                        vmux_service::protocol::AcpModeOption {
                            id: "auto".into(),
                            name: "Auto Allow".into(),
                            description: None,
                        },
                    ],
                },
            ))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive {
            webview,
            payload: SelectMode {
                mode_id: "auto".into(),
            },
        });
        app.update();

        let state = app.world().get::<AcpModeState>(stack).unwrap();
        assert_eq!(state.current_mode_id, "ask");
        assert_eq!(state.display_mode_id(), "auto");
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AcpSetModeRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].sid, "s1");
        assert_eq!(requests[0].request_id, 1);
        assert!(requests[0].config_id.is_empty());
        assert_eq!(requests[0].mode_id, "auto");
    }

    #[test]
    fn a_legacy_agent_models_file_still_names_the_remembered_model() {
        let legacy = br#"{"claude":"fable","codex":{"selected":"gpt","models":[]}}"#;
        let saved: std::collections::BTreeMap<String, SavedAgentModel> =
            serde_json::from_slice(legacy).expect("parse");
        let mut models = AgentModelSelections::default();
        for (agent, entry) in saved {
            models.by_agent.insert(agent, entry.memory());
        }

        assert_eq!(models.selected_for("claude"), "fable");
        assert_eq!(models.selected_for("codex"), "gpt");
    }

    #[test]
    fn cli_catalog_is_published_for_its_launcher_url() {
        let mut selections = AgentModelSelections::default();
        selections.remember_catalog(
            "cli:codex",
            "vmux://sessions/codex/cli",
            "gpt-next",
            &[ModelOptionEntry {
                id: "gpt-next".into(),
                name: "GPT Next".into(),
                description: String::new(),
            }],
        );
        let mut app = App::new();
        app.insert_resource(selections)
            .init_resource::<vmux_command::snapshot::CommandBarAgentModels>()
            .add_systems(Update, publish_agent_models);

        app.update();

        let published = app
            .world()
            .resource::<vmux_command::snapshot::CommandBarAgentModels>();
        assert_eq!(published.agents.len(), 1);
        assert_eq!(published.agents[0].agent_key, "cli:codex");
        assert_eq!(published.agents[0].url, "vmux://sessions/codex/cli");
        assert_eq!(published.agents[0].selected, "gpt-next");
    }

    #[test]
    fn fresh_agent_session_applies_last_used_model() {
        let mut app = App::new();
        app.init_resource::<AcpModelRequestCounter>()
            .init_resource::<AgentModelSelections>()
            .add_message::<AcpSetModelRequest>()
            .add_systems(Update, apply_last_used_acp_model);
        app.world_mut()
            .resource_mut::<AgentModelSelections>()
            .remember_catalog(
                "claude",
                "vmux://sessions/claude",
                "fable",
                &[ModelOptionEntry {
                    id: "fable".into(),
                    name: "Fable".into(),
                    description: String::new(),
                }],
            );
        let stack = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s2".into(),
                    cwd: "/tmp".into(),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AcpModelState {
                    config_id: "model".into(),
                    current_model_id: "default".into(),
                    default_model_id: "default".into(),
                    pending: None,
                    models: vec![
                        vmux_service::protocol::AcpModelOption {
                            id: "default".into(),
                            name: "Default".into(),
                            description: None,
                        },
                        vmux_service::protocol::AcpModelOption {
                            id: "fable".into(),
                            name: "Fable".into(),
                            description: None,
                        },
                    ],
                },
            ))
            .id();

        app.update();

        let state = app.world().get::<AcpModelState>(stack).unwrap();
        assert_eq!(state.display_model_id(), "fable");
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AcpSetModelRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].sid, "s2");
        assert_eq!(requests[0].model_id, "fable");
    }
}
