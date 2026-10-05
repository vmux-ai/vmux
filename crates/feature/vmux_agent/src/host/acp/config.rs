use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::host::event::AgentRequestInput;
use crate::host::model_selection::{
    AcpSessionConfigRequestCounter, AgentModeSelections, AgentModelSelections, ModelSelectionSet,
    RememberMode, RememberModel, RememberModels, RememberModes,
};
use crate::host::runtime::{AcpSessionConfigState, PendingAcpSessionConfig};
use vmux_api::command_bar::{AgentModels, AgentModes};
use vmux_api::command_bar::{StartSelectMode, StartSelectModel};
use vmux_api::protocol::{
    AcpModeOption, AgentCommandResult, AgentListModels, AgentSelectModel, AgentSetEffort,
    ClientMessage,
};
use vmux_api::room::RemoteModelState;
use vmux_chat::event::{ModelOptionEntry, SelectMode, SelectModel, SetAgentEffort};
use vmux_chat::host::{ChatModeStateChanged, ChatModelStateChanged, ChatView};
use vmux_command::{ContributedAgentModels, ContributedAgentModes};
use vmux_ecs::page::PageReady;
use vmux_ecs::service::{ServiceMessageSet, ServiceRequest};
use vmux_session::AcpSession;

use super::config_driver::AcpConfigProjection;
use crate::route::AcpRoute;

pub(crate) fn add(app: &mut App) {
    crate::host::model_selection::add(app);
    app.add_message::<ServiceRequest>()
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
                apply_session_selection.before(ModelSelectionSet),
                push_state_to_page,
                remove_state,
                apply_last_used_model.after(crate::host::runtime::AcpSessionConfigSet),
                send_session_requests,
                remember_model_lists.before(ModelSelectionSet),
                remember_mode_lists.before(ModelSelectionSet),
                publish_models.after(ModelSelectionSet),
                publish_modes.after(ModelSelectionSet),
            ),
        );
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
    mut remembered_models: MessageWriter<RememberModel>,
    mut remembered_modes: MessageWriter<RememberMode>,
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
        counter.0 = counter.0.wrapping_add(1);
        let request_id = counter.0;
        if request.category == "model" {
            remembered_models.write(RememberModel {
                agent_id: session.agent_id.clone(),
                model_id: request.value.clone(),
            });
        } else if request.category == "mode" {
            remembered_modes.write(RememberMode {
                agent_id: session.agent_id.clone(),
                mode_id: request.value.clone(),
            });
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

fn start_select_model(
    trigger: On<UiInput<StartSelectModel>>,
    mut remembered: MessageWriter<RememberModel>,
) {
    let request = &trigger.event().payload;
    if request.agent_key.is_empty() || request.model_id.is_empty() {
        return;
    }
    remembered.write(RememberModel {
        agent_id: request.agent_key.clone(),
        model_id: request.model_id.clone(),
    });
}

fn start_select_mode(
    trigger: On<UiInput<StartSelectMode>>,
    mut remembered: MessageWriter<RememberMode>,
) {
    let request = &trigger.event().payload;
    if request.agent_key.is_empty() || request.mode_id.is_empty() {
        return;
    }
    remembered.write(RememberMode {
        agent_id: request.agent_key.clone(),
        mode_id: request.mode_id.clone(),
    });
}

fn remember_model_lists(
    sessions: Query<(&AcpSession, &AcpSessionConfigState), Changed<AcpSessionConfigState>>,
    mut remembered: MessageWriter<RememberModels>,
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
        let url = AcpRoute::agent(&session.agent_id).url();
        remembered.write(RememberModels {
            agent_id: session.agent_id.clone(),
            url,
            selected: current,
            models: listed,
        });
    }
}

fn remember_mode_lists(
    sessions: Query<(&AcpSession, &AcpSessionConfigState), Changed<AcpSessionConfigState>>,
    mut remembered: MessageWriter<RememberModes>,
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
        let url = AcpRoute::agent(&session.agent_id).url();
        remembered.write(RememberModes {
            agent_id: session.agent_id.clone(),
            url,
            selected: state.display_value(mode).to_string(),
            modes,
        });
    }
}

#[derive(Component)]
struct ModelContribution;

#[derive(Component)]
struct ModeContribution;

fn publish_models(
    last_used: Single<Ref<AgentModelSelections>>,
    existing: Query<Entity, With<ModelContribution>>,
    mut commands: Commands,
) {
    if !last_used.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    for (agent_key, memory) in &last_used.by_agent {
        if memory.url.is_empty() || memory.models.is_empty() {
            continue;
        }
        commands.spawn((
            Name::new(format!("Agent model catalog {agent_key}")),
            ModelContribution,
            ContributedAgentModels(AgentModels {
                agent_key: agent_key.clone(),
                url: memory.url.clone(),
                selected: memory.selected.clone(),
                models: memory.models.clone(),
            }),
        ));
    }
}

fn publish_modes(
    last_used: Single<Ref<AgentModeSelections>>,
    existing: Query<Entity, With<ModeContribution>>,
    mut commands: Commands,
) {
    if !last_used.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    for (agent_key, memory) in &last_used.by_agent {
        if memory.url.is_empty() || memory.modes.is_empty() {
            continue;
        }
        commands.spawn((
            Name::new(format!("Agent mode catalog {agent_key}")),
            ModeContribution,
            ContributedAgentModes(AgentModes {
                agent_key: agent_key.clone(),
                url: memory.url.clone(),
                selected: memory.selected.clone(),
                modes: memory.modes.clone(),
            }),
        ));
    }
}

fn push_state_to_page(
    sessions: Query<Entity, (With<AcpSession>, Changed<AcpSessionConfigState>)>,
    children: Query<&Children>,
    chat_views: Query<(), With<ChatView>>,
    projection: AcpConfigProjection,
    mut commands: Commands,
) {
    for stack in &sessions {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| chat_views.contains(entity)) else {
            continue;
        };
        let Some((model, mode)) = projection.get(stack) else {
            continue;
        };
        commands.trigger(ChatModelStateChanged::new(webview, model));
        commands.trigger(ChatModeStateChanged::new(webview, mode));
    }
}

fn remove_state(
    mut removed: RemovedComponents<AcpSessionConfigState>,
    children: Query<&Children>,
    chat_views: Query<(), With<ChatView>>,
    projection: AcpConfigProjection,
    mut commands: Commands,
) {
    for stack in removed.read() {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| chat_views.contains(entity)) else {
            continue;
        };
        let Some((model, mode)) = projection.get(stack) else {
            continue;
        };
        commands.trigger(ChatModelStateChanged::new(webview, model));
        commands.trigger(ChatModeStateChanged::new(webview, mode));
    }
}

fn sync_page_model_state(
    trigger: On<UiInput<PageReady>>,
    views: Query<&ChildOf, With<ChatView>>,
    projection: AcpConfigProjection,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(parent) = views.get(webview) else {
        return;
    };
    let Some((model, mode)) = projection.get(parent.parent()) else {
        return;
    };
    commands.trigger(ChatModelStateChanged::new(webview, model));
    commands.trigger(ChatModeStateChanged::new(webview, mode));
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
        counter.0 = counter.0.wrapping_add(1);
        let request_id = counter.0;
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
    use crate::host::model_selection::AgentModelMemory;

    #[test]
    fn model_selection_emits_remembered_state_before_response() {
        let mut app = App::new();
        app.world_mut()
            .spawn(AcpSessionConfigRequestCounter::default());
        app.add_message::<AcpSetSessionConfigRequest>()
            .add_message::<SessionConfigSelectRequest>()
            .add_message::<RememberModel>()
            .add_message::<RememberMode>()
            .add_observer(select_model)
            .add_systems(Update, apply_session_selection);
        let stack = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s1".into(),
                    cwd: "/tmp".into(),
                    anchor: vmux_ecs::ProcessId::new(),
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
                    initial: vec![crate::host::runtime::InitialAcpSessionConfig {
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
        let remembered = app
            .world_mut()
            .resource_mut::<Messages<RememberModel>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(remembered.len(), 1);
        assert_eq!(remembered[0].agent_id, "claude");
        assert_eq!(remembered[0].model_id, "fable");

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
            .add_message::<RememberModel>()
            .add_message::<RememberMode>()
            .add_observer(select_mode)
            .add_systems(Update, apply_session_selection);
        let stack = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s1".into(),
                    cwd: "/tmp".into(),
                    anchor: vmux_ecs::ProcessId::new(),
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
                    initial: vec![crate::host::runtime::InitialAcpSessionConfig {
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
    fn acp_mode_catalog_remembers_the_session_agent_identity() {
        let mut app = App::new();
        app.add_message::<RememberModes>()
            .add_systems(Update, remember_mode_lists);
        app.world_mut().spawn((
            AcpSession {
                agent_id: "codex-acp".into(),
                sid: "s1".into(),
                cwd: "/tmp".into(),
                anchor: vmux_ecs::ProcessId::new(),
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

        let remembered = app
            .world_mut()
            .resource_mut::<Messages<RememberModes>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(remembered.len(), 1);
        assert_eq!(remembered[0].agent_id, "codex-acp");
        assert_eq!(remembered[0].url, "vmux://sessions/codex-acp");
        assert_eq!(remembered[0].selected, "agent");
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
            .by_agent
            .insert(
                "claude".to_string(),
                AgentModelMemory {
                    url: "vmux://sessions/claude".to_string(),
                    selected: "fable".to_string(),
                    models: vec![ModelOptionEntry {
                        id: "fable".into(),
                        name: "Fable".into(),
                        description: String::new(),
                    }],
                },
            );
        let stack = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s2".into(),
                    cwd: "/tmp".into(),
                    anchor: vmux_ecs::ProcessId::new(),
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
                    initial: vec![crate::host::runtime::InitialAcpSessionConfig {
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
