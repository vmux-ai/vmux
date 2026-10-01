use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::AgentRequestInput;
#[cfg(test)]
use crate::host::model_selection::SavedAgentModel;
use crate::host::model_selection::{AcpSessionConfigRequestCounter, ModelSelectionPlugin};
use crate::host::model_selection::{AgentModeSelections, AgentModelSelections};
use crate::runtime::{AcpSessionConfigState, PendingAcpSessionConfig};
use vmux_api::command_bar::{AgentModels, AgentModes};
use vmux_api::command_bar::{StartSelectMode, StartSelectModel};
use vmux_api::protocol::{
    AcpModeOption, AgentCommandResult, AgentListModels, AgentSelectModel, AgentSetEffort,
    ClientMessage,
};
use vmux_api::room::RemoteModelState;
use vmux_chat::event::{
    ModeState, ModelOptionEntry, ModelState, SelectMode, SelectModel, SetAgentEffort,
};
use vmux_chat::host::{ChatModeStateChanged, ChatModelStateChanged, ChatView};
use vmux_command::snapshot::{AgentPromptTarget, CommandBarProjection};
use vmux_core::page::PageReady;
use vmux_core::service::{ServiceMessageSet, ServiceRequest};
use vmux_session::AcpSession;

pub struct AcpSessionConfigPlugin;

impl Plugin for AcpSessionConfigPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ModelSelectionPlugin)
            .add_message::<ServiceRequest>()
            .add_message::<SessionConfigSelectRequest>()
            .add_message::<AcpSetSessionConfigRequest>()
            .add_plugins(UiEventPlugin::<(
                SelectModel,
                SetAgentEffort,
                SelectMode,
                PageReady,
            )>::default())
            .add_plugins(UiEventPlugin::<(StartSelectModel, StartSelectMode)>::default())
            .add_observer(select_model)
            .add_observer(select_mode)
            .add_observer(set_effort)
            .add_observer(start_select_model)
            .add_observer(start_select_mode)
            .add_observer(sync_page_model_state)
            .add_systems(
                Update,
                (
                    answer_remote_model_commands.after(ServiceMessageSet),
                    apply_session_selection,
                    push_state_to_page,
                    remove_state,
                    apply_last_used_model.after(crate::runtime::AcpSessionConfigSet),
                    send_session_requests,
                    remember_model_lists,
                    remember_mode_lists,
                    publish_models.after(remember_model_lists),
                    publish_modes.after(remember_mode_lists),
                ),
            );
    }
}

#[derive(Message)]
struct SessionConfigSelectRequest {
    sid: String,
    category: &'static str,
    value: String,
}

fn answer_remote_model_commands(
    mut reader: MessageReader<AgentRequestInput>,
    sessions: Query<(&AcpSession, &AcpSessionConfigState)>,
    mut selects: MessageWriter<SessionConfigSelectRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let result = if let Ok(Some(payload)) = request.decode::<AgentListModels>() {
            match remote_model_state(&payload.sid, &sessions) {
                Some(state) => match serde_json::to_string(&state) {
                    Ok(json) => AgentCommandResult::Text(json),
                    Err(error) => AgentCommandResult::Error(format!("list_models: {error}")),
                },
                None => AgentCommandResult::Error("no such session".to_string()),
            }
        } else if let Ok(Some(payload)) = request.decode::<AgentSelectModel>() {
            if !sessions
                .iter()
                .any(|(session, _)| session.sid == payload.sid)
            {
                AgentCommandResult::Error("no such session".to_string())
            } else {
                selects.write(SessionConfigSelectRequest {
                    sid: payload.sid,
                    category: "model",
                    value: payload.model_id,
                });
                AgentCommandResult::Ok
            }
        } else if let Ok(Some(payload)) = request.decode::<AgentSetEffort>() {
            if sessions
                .iter()
                .any(|(session, _)| session.sid == payload.sid)
            {
                selects.write(SessionConfigSelectRequest {
                    sid: payload.sid,
                    category: "thought_level",
                    value: payload.level,
                });
                AgentCommandResult::Ok
            } else {
                AgentCommandResult::Error("no such session".to_string())
            }
        } else {
            continue;
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentCommandResponse {
            request_id: request.request_id,
            result,
        }));
    }
}

fn remote_model_state(
    sid: &str,
    sessions: &Query<(&AcpSession, &AcpSessionConfigState)>,
) -> Option<RemoteModelState> {
    let (_, state) = sessions.iter().find(|(session, _)| session.sid == sid)?;
    let model = state.category("model")?;
    let mut models = Vec::new();
    for option in &model.values {
        models.push(ModelOptionEntry {
            id: option.value.clone(),
            name: option.name.clone(),
            description: option.description.clone().unwrap_or_default(),
        });
    }
    let thought = state.category("thought_level");
    Some(RemoteModelState {
        models,
        selected_id: state.display_value(model).to_string(),
        effort_levels: thought
            .map(|config| {
                config
                    .values
                    .iter()
                    .map(|option| option.value.clone())
                    .collect()
            })
            .unwrap_or_default(),
        effort: thought
            .map(|config| state.display_value(config).to_string())
            .unwrap_or_default(),
    })
}

fn apply_session_selection(
    mut reader: MessageReader<SessionConfigSelectRequest>,
    mut sessions: Query<(&AcpSession, &mut AcpSessionConfigState)>,
    mut counter: Single<&mut AcpSessionConfigRequestCounter>,
    mut last_models: Single<&mut AgentModelSelections>,
    mut last_modes: Single<&mut AgentModeSelections>,
    mut requests: MessageWriter<AcpSetSessionConfigRequest>,
) {
    for request in reader.read() {
        let Some((session, mut state)) = sessions
            .iter_mut()
            .find(|(session, _)| session.sid == request.sid)
        else {
            continue;
        };
        let Some(config) = state.category(request.category).cloned() else {
            continue;
        };
        if state.display_value(&config) == request.value
            || !config
                .values
                .iter()
                .any(|option| option.value == request.value)
        {
            continue;
        }
        let request_id = counter.next();
        if request.category == "model" {
            last_models.select(&session.agent_id, &request.value);
        } else if request.category == "mode" {
            last_modes.select(&session.agent_id, &request.value);
        }
        requests.write(AcpSetSessionConfigRequest {
            sid: session.sid.clone(),
            request_id,
            config_id: config.config_id.clone(),
            value: request.value.clone(),
        });
        state
            .pending
            .retain(|pending| pending.config_id != config.config_id);
        state.pending.push(PendingAcpSessionConfig {
            request_id,
            config_id: config.config_id,
            value: request.value.clone(),
        });
    }
}

#[derive(Message)]
struct AcpSetSessionConfigRequest {
    sid: String,
    request_id: u64,
    config_id: Option<String>,
    value: String,
}

struct AcpConfigProjection<'a> {
    configs: Option<&'a AcpSessionConfigState>,
    session: &'a AcpSession,
}

impl AcpConfigProjection<'_> {
    fn model_state(&self) -> ModelState {
        let agent_key = self.session.agent_id.as_str();
        let Some(configs) = self.configs else {
            return ModelState {
                agent_key: agent_key.to_string(),
                ..Default::default()
            };
        };
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

    fn mode_state(&self) -> ModeState {
        let Some(configs) = self.configs else {
            return ModeState::default();
        };
        let Some(mode) = configs.category("mode") else {
            return ModeState::default();
        };
        ModeState {
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
        }
    }
}

fn start_select_model(
    trigger: On<UiInput<StartSelectModel>>,
    mut last_used: Single<&mut AgentModelSelections>,
) {
    let request = &trigger.event().payload;
    if request.agent_key.is_empty() || request.model_id.is_empty() {
        return;
    }
    last_used.select(&request.agent_key, &request.model_id);
}

fn start_select_mode(
    trigger: On<UiInput<StartSelectMode>>,
    mut last_used: Single<&mut AgentModeSelections>,
) {
    let request = &trigger.event().payload;
    if request.agent_key.is_empty() || request.mode_id.is_empty() {
        return;
    }
    last_used.select(&request.agent_key, &request.mode_id);
}

fn remember_model_lists(
    sessions: Query<(&AcpSession, &AcpSessionConfigState), Changed<AcpSessionConfigState>>,
    mut last_used: Single<&mut AgentModelSelections>,
) {
    for (session, state) in &sessions {
        let Some(model) = state.category("model") else {
            continue;
        };
        let listed = model
            .values
            .iter()
            .map(|option| ModelOptionEntry {
                id: option.value.clone(),
                name: option.name.clone(),
                description: option.description.clone().unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        let current = state.display_value(model).to_string();
        let url = AgentPromptTarget::new(&session.agent_id).url();
        last_used.remember_catalog(&session.agent_id, &url, &current, &listed);
    }
}

fn remember_mode_lists(
    sessions: Query<(&AcpSession, &AcpSessionConfigState), Changed<AcpSessionConfigState>>,
    mut last_used: Single<&mut AgentModeSelections>,
) {
    for (session, state) in &sessions {
        let Some(mode) = state.category("mode") else {
            continue;
        };
        let modes = mode
            .values
            .iter()
            .map(|option| AcpModeOption {
                id: option.value.clone(),
                name: option.name.clone(),
                description: option.description.clone(),
            })
            .collect::<Vec<_>>();
        let url = AgentPromptTarget::new(&session.agent_id).url();
        last_used.remember_catalog(&session.agent_id, &url, state.display_value(mode), &modes);
    }
}

fn publish_models(
    last_used: Single<Ref<AgentModelSelections>>,
    mut state: Single<&mut CommandBarProjection>,
) {
    if !last_used.is_changed() {
        return;
    }
    let mut next = Vec::new();
    for (agent_key, memory) in &last_used.by_agent {
        if memory.url.is_empty() || memory.models.is_empty() {
            continue;
        }
        next.push(AgentModels {
            agent_key: agent_key.clone(),
            url: memory.url.clone(),
            selected: memory.selected.clone(),
            models: memory.models.clone(),
        });
    }
    if state.agent_models.agents != next {
        state.agent_models.agents = next;
    }
}

fn publish_modes(
    last_used: Single<Ref<AgentModeSelections>>,
    mut state: Single<&mut CommandBarProjection>,
) {
    if !last_used.is_changed() {
        return;
    }
    let mut next = Vec::new();
    for (agent_key, memory) in &last_used.by_agent {
        if memory.url.is_empty() || memory.modes.is_empty() {
            continue;
        }
        next.push(AgentModes {
            agent_key: agent_key.clone(),
            url: memory.url.clone(),
            selected: memory.selected.clone(),
            modes: memory.modes.clone(),
        });
    }
    if state.agent_modes.agents != next {
        state.agent_modes.agents = next;
    }
}

fn push_state_to_page(
    sessions: Query<(Entity, &AcpSession, &AcpSessionConfigState), Changed<AcpSessionConfigState>>,
    children: Query<&Children>,
    chat_views: Query<(), With<ChatView>>,
    mut commands: Commands,
) {
    for (stack, session, configs) in &sessions {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| chat_views.contains(entity)) else {
            continue;
        };
        let projection = AcpConfigProjection {
            configs: Some(configs),
            session,
        };
        commands.trigger(ChatModelStateChanged::new(
            webview,
            projection.model_state(),
        ));
        commands.trigger(ChatModeStateChanged::new(webview, projection.mode_state()));
    }
}

fn remove_state(
    mut removed: RemovedComponents<AcpSessionConfigState>,
    sessions: Query<&AcpSession>,
    children: Query<&Children>,
    chat_views: Query<(), With<ChatView>>,
    mut commands: Commands,
) {
    for stack in removed.read() {
        let Ok(session) = sessions.get(stack) else {
            continue;
        };
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| chat_views.contains(entity)) else {
            continue;
        };
        let projection = AcpConfigProjection {
            configs: None,
            session,
        };
        commands.trigger(ChatModelStateChanged::new(
            webview,
            projection.model_state(),
        ));
        commands.trigger(ChatModeStateChanged::new(webview, projection.mode_state()));
    }
}

fn sync_page_model_state(
    trigger: On<UiInput<PageReady>>,
    views: Query<&ChildOf, With<ChatView>>,
    sessions: Query<(&AcpSession, Option<&AcpSessionConfigState>)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(parent) = views.get(webview) else {
        return;
    };
    let Ok((session, configs)) = sessions.get(parent.parent()) else {
        return;
    };
    let projection = AcpConfigProjection { configs, session };
    commands.trigger(ChatModelStateChanged::new(
        webview,
        projection.model_state(),
    ));
    commands.trigger(ChatModeStateChanged::new(webview, projection.mode_state()));
}

fn select_model(
    trigger: On<UiInput<SelectModel>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut selects: MessageWriter<SessionConfigSelectRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    selects.write(SessionConfigSelectRequest {
        sid: session.sid.clone(),
        category: "model",
        value: trigger.event().payload.model_id.clone(),
    });
}

fn select_mode(
    trigger: On<UiInput<SelectMode>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut selects: MessageWriter<SessionConfigSelectRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    selects.write(SessionConfigSelectRequest {
        sid: session.sid.clone(),
        category: "mode",
        value: trigger.event().payload.mode_id.clone(),
    });
}

fn set_effort(
    trigger: On<UiInput<SetAgentEffort>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut selects: MessageWriter<SessionConfigSelectRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    selects.write(SessionConfigSelectRequest {
        sid: session.sid.clone(),
        category: "thought_level",
        value: trigger.event().payload.level.trim().to_string(),
    });
}

fn apply_last_used_model(
    mut sessions: Query<(&AcpSession, &mut AcpSessionConfigState), Added<AcpSessionConfigState>>,
    last_used: Single<&AgentModelSelections>,
    mut counter: Single<&mut AcpSessionConfigRequestCounter>,
    mut requests: MessageWriter<AcpSetSessionConfigRequest>,
) {
    for (session, mut state) in &mut sessions {
        let Some(remembered) = last_used.by_agent.get(&session.agent_id) else {
            continue;
        };
        let model_id = &remembered.selected;
        if model_id.is_empty() {
            continue;
        }
        let Some(model) = state.category("model").cloned() else {
            continue;
        };
        if state.display_value(&model) == model_id
            || !model.values.iter().any(|option| &option.value == model_id)
        {
            continue;
        }
        let request_id = counter.next();
        requests.write(AcpSetSessionConfigRequest {
            sid: session.sid.clone(),
            request_id,
            config_id: model.config_id.clone(),
            value: model_id.clone(),
        });
        state.pending.push(PendingAcpSessionConfig {
            request_id,
            config_id: model.config_id,
            value: model_id.clone(),
        });
    }
}

fn send_session_requests(
    mut requests: MessageReader<AcpSetSessionConfigRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        service_requests.write(ServiceRequest(ClientMessage::AcpSetSessionConfig {
            sid: request.sid.clone(),
            request_id: request.request_id,
            config_id: request.config_id.clone(),
            value: request.value.clone(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_selection_updates_cached_state_before_response() {
        let mut app = App::new();
        let registry = app
            .world_mut()
            .spawn((
                AcpSessionConfigRequestCounter::default(),
                AgentModelSelections::default(),
                AgentModeSelections::default(),
            ))
            .id();
        app.add_message::<AcpSetSessionConfigRequest>()
            .add_message::<SessionConfigSelectRequest>()
            .add_observer(select_model)
            .add_systems(Update, apply_session_selection);
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
                AcpSessionConfigState {
                    configs: vec![vmux_api::protocol::AcpSessionConfig {
                        config_id: Some("model".into()),
                        name: "Model".into(),
                        description: None,
                        category: Some("model".into()),
                        current_value: "default".into(),
                        values: vec![
                            vmux_api::protocol::AcpSessionConfigValue {
                                value: "default".into(),
                                name: "Default".into(),
                                description: None,
                                group: None,
                            },
                            vmux_api::protocol::AcpSessionConfigValue {
                                value: "fable".into(),
                                name: "Fable".into(),
                                description: None,
                                group: None,
                            },
                        ],
                    }],
                    pending: Vec::new(),
                    initial: vec![crate::runtime::InitialAcpSessionConfig {
                        config_id: Some("model".into()),
                        value: "default".into(),
                    }],
                },
            ))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SelectModel {
                model_id: "fable".into(),
            },
        });
        app.update();

        let state = app.world().get::<AcpSessionConfigState>(stack).unwrap();
        let model = state.category("model").unwrap();
        assert_eq!(model.current_value, "default");
        assert_eq!(state.pending[0].request_id, 1);
        assert_eq!(state.pending[0].value, "fable");
        assert_eq!(state.display_name(model), "Fable");
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AcpSetSessionConfigRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].sid, "s1");
        assert_eq!(requests[0].request_id, 1);
        assert_eq!(requests[0].config_id.as_deref(), Some("model"));
        assert_eq!(requests[0].value, "fable");
        assert_eq!(
            app.world()
                .get::<AgentModelSelections>(registry)
                .unwrap()
                .selected_for("claude"),
            "fable"
        );

        app.world_mut().trigger(UiInput {
            webview,
            payload: SelectModel {
                model_id: "fable".into(),
            },
        });
        app.world_mut().trigger(UiInput {
            webview,
            payload: SelectModel {
                model_id: "missing".into(),
            },
        });
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<AcpSetSessionConfigRequest>>()
                .drain()
                .count(),
            0
        );
    }

    #[test]
    fn mode_selection_updates_cached_state_before_response() {
        let mut app = App::new();
        app.world_mut().spawn((
            AcpSessionConfigRequestCounter::default(),
            AgentModelSelections::default(),
            AgentModeSelections::default(),
        ));
        app.add_message::<AcpSetSessionConfigRequest>()
            .add_message::<SessionConfigSelectRequest>()
            .add_observer(select_mode)
            .add_systems(Update, apply_session_selection);
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
                AcpSessionConfigState {
                    configs: vec![vmux_api::protocol::AcpSessionConfig {
                        config_id: None,
                        name: "Mode".into(),
                        description: None,
                        category: Some("mode".into()),
                        current_value: "ask".into(),
                        values: vec![
                            vmux_api::protocol::AcpSessionConfigValue {
                                value: "ask".into(),
                                name: "Ask".into(),
                                description: None,
                                group: None,
                            },
                            vmux_api::protocol::AcpSessionConfigValue {
                                value: "auto".into(),
                                name: "Auto Allow".into(),
                                description: None,
                                group: None,
                            },
                        ],
                    }],
                    pending: Vec::new(),
                    initial: vec![crate::runtime::InitialAcpSessionConfig {
                        config_id: None,
                        value: "ask".into(),
                    }],
                },
            ))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SelectMode {
                mode_id: "auto".into(),
            },
        });
        app.update();

        let state = app.world().get::<AcpSessionConfigState>(stack).unwrap();
        let mode = state.category("mode").unwrap();
        assert_eq!(mode.current_value, "ask");
        assert_eq!(state.display_value(mode), "auto");
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AcpSetSessionConfigRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].sid, "s1");
        assert_eq!(requests[0].request_id, 1);
        assert_eq!(requests[0].config_id, None);
        assert_eq!(requests[0].value, "auto");
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
    fn acp_mode_catalog_uses_the_canonical_launcher_identity() {
        let mut app = App::new();
        let registry = app.world_mut().spawn(AgentModeSelections::default()).id();
        app.world_mut()
            .spawn(vmux_command::snapshot::CommandBarProjection::default());
        app.add_systems(
            Update,
            (
                remember_mode_lists,
                publish_modes.after(remember_mode_lists),
            ),
        );
        app.world_mut().spawn((
            AcpSession {
                agent_id: "codex-acp".into(),
                sid: "s1".into(),
                cwd: "/tmp".into(),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            AcpSessionConfigState {
                configs: vec![vmux_api::protocol::AcpSessionConfig {
                    config_id: Some("mode".into()),
                    name: "Mode".into(),
                    description: None,
                    category: Some("mode".into()),
                    current_value: "agent".into(),
                    values: vec![vmux_api::protocol::AcpSessionConfigValue {
                        value: "agent".into(),
                        name: "Agent".into(),
                        description: None,
                        group: None,
                    }],
                }],
                pending: Vec::new(),
                initial: Vec::new(),
            },
        ));

        app.update();

        let published = app
            .world()
            .iter_entities()
            .find_map(|entity| entity.get::<vmux_command::snapshot::CommandBarProjection>())
            .unwrap();
        let published = &published.agent_modes;
        assert_eq!(published.agents.len(), 1);
        assert_eq!(published.agents[0].agent_key, "codex");
        assert_eq!(published.agents[0].url, "vmux://sessions/codex");
        assert_eq!(published.agents[0].selected, "agent");
        assert_eq!(
            app.world()
                .get::<AgentModeSelections>(registry)
                .unwrap()
                .selected_for("codex-acp"),
            "agent"
        );
    }

    #[test]
    fn fresh_agent_session_applies_last_used_model() {
        let mut app = App::new();
        let registry = app
            .world_mut()
            .spawn((
                AcpSessionConfigRequestCounter::default(),
                AgentModelSelections::default(),
                AgentModeSelections::default(),
            ))
            .id();
        app.add_message::<AcpSetSessionConfigRequest>()
            .add_systems(Update, apply_last_used_model);
        app.world_mut()
            .get_mut::<AgentModelSelections>(registry)
            .unwrap()
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
                AcpSessionConfigState {
                    configs: vec![vmux_api::protocol::AcpSessionConfig {
                        config_id: Some("model".into()),
                        name: "Model".into(),
                        description: None,
                        category: Some("model".into()),
                        current_value: "default".into(),
                        values: vec![
                            vmux_api::protocol::AcpSessionConfigValue {
                                value: "default".into(),
                                name: "Default".into(),
                                description: None,
                                group: None,
                            },
                            vmux_api::protocol::AcpSessionConfigValue {
                                value: "fable".into(),
                                name: "Fable".into(),
                                description: None,
                                group: None,
                            },
                        ],
                    }],
                    pending: Vec::new(),
                    initial: vec![crate::runtime::InitialAcpSessionConfig {
                        config_id: Some("model".into()),
                        value: "default".into(),
                    }],
                },
            ))
            .id();

        app.update();

        let state = app.world().get::<AcpSessionConfigState>(stack).unwrap();
        let model = state.category("model").unwrap();
        assert_eq!(state.display_value(model), "fable");
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AcpSetSessionConfigRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].sid, "s2");
        assert_eq!(requests[0].value, "fable");
    }
}
