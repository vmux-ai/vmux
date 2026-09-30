use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::AgentRequestInput;
#[cfg(test)]
use crate::host::model_selection::SavedAgentModel;
use crate::host::model_selection::{
    AcpModeRequestCounter, AcpModelRequestCounter, AgentSelectionKey, ModelSelectionPlugin,
};
use crate::host::model_selection::{AgentModeSelections, AgentModelSelections};
use crate::runtime::acp::{AcpModeState, AcpModelState};
use vmux_api::command_bar::{AgentModels, AgentModes};
use vmux_api::protocol::{
    AgentCommandResult, AgentListModels, AgentSelectModel, AgentSetEffort, ClientMessage,
};
use vmux_api::room::RemoteModelState;
use vmux_chat::event::{
    ModeState, ModelOptionEntry, ModelState, SelectMode, SelectModel, SetAgentEffort,
};
use vmux_chat::host::{ChatModeStateChanged, ChatModelStateChanged, ChatView};
use vmux_command::event::{StartSelectMode, StartSelectModel};
use vmux_command::snapshot::{AgentPromptTarget, CommandBarProjection};
use vmux_core::agent::{AgentKind, default_effort, effort_levels};
use vmux_core::page::PageReady;
use vmux_core::service::{ServiceMessageSet, ServiceRequest};
use vmux_session::AcpSession;
use vmux_setting::{AppSettings, SettingsWriteRequest};

pub(super) struct AcpModelPlugin;

impl Plugin for AcpModelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ModelSelectionPlugin)
            .add_message::<ServiceRequest>()
            .add_message::<AcpSetModelRequest>()
            .add_message::<AcpSetModeRequest>()
            .add_message::<ModeSelectRequest>()
            .add_message::<ModelSelectRequest>()
            .add_message::<EffortSetRequest>()
            .add_plugins(UiEventPlugin::<(
                SelectModel,
                SetAgentEffort,
                SelectMode,
                PageReady,
            )>::default())
            .add_plugins(UiEventPlugin::<(StartSelectModel, StartSelectMode)>::default())
            .add_observer(on_select_model)
            .add_observer(on_select_mode)
            .add_observer(on_set_agent_effort)
            .add_observer(on_start_select_model)
            .add_observer(on_start_select_mode)
            .add_observer(sync_page_model_state)
            .add_systems(
                Update,
                (
                    answer_remote_model_commands.after(ServiceMessageSet),
                    seed_cli_model_lists,
                    apply_model_selection,
                    apply_mode_selection,
                    apply_effort_setting,
                    push_acp_model_state_to_page,
                    remove_model_state,
                    push_acp_mode_state_to_page,
                    remove_mode_state,
                    apply_last_used_acp_model.after(crate::runtime::acp::AcpModelInfoSet),
                    send_acp_model_requests,
                    send_acp_mode_requests,
                    remember_acp_model_lists,
                    remember_acp_mode_lists,
                    publish_agent_models.after(remember_acp_model_lists),
                    publish_agent_modes.after(remember_acp_mode_lists),
                ),
            );
    }
}

#[derive(Message)]
struct ModelSelectRequest {
    sid: String,
    model_id: String,
}

#[derive(Message)]
struct EffortSetRequest {
    agent_key: String,
    level: String,
}

fn answer_remote_model_commands(
    mut reader: MessageReader<AgentRequestInput>,
    sessions: Query<(&AcpSession, &AcpModelState)>,
    settings: Res<AppSettings>,
    mut selects: MessageWriter<ModelSelectRequest>,
    mut efforts: MessageWriter<EffortSetRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let result = if let Ok(Some(payload)) = request.decode::<AgentListModels>() {
            match remote_model_state(&payload.sid, &sessions, &settings) {
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
                selects.write(ModelSelectRequest {
                    sid: payload.sid,
                    model_id: payload.model_id,
                });
                AgentCommandResult::Ok
            }
        } else if let Ok(Some(payload)) = request.decode::<AgentSetEffort>() {
            match sessions
                .iter()
                .find(|(session, _)| session.sid == payload.sid)
            {
                Some((session, _)) => {
                    efforts.write(EffortSetRequest {
                        agent_key: session.agent_id.clone(),
                        level: payload.level,
                    });
                    AgentCommandResult::Ok
                }
                None => AgentCommandResult::Error("no such session".to_string()),
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
    sessions: &Query<(&AcpSession, &AcpModelState)>,
    settings: &AppSettings,
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
    let mut available_effort_levels = Vec::new();
    for level in effort_levels(&session.agent_id) {
        available_effort_levels.push((*level).to_string());
    }
    Some(RemoteModelState {
        models,
        selected_id: model_state.display_model_id().to_string(),
        effort_levels: available_effort_levels,
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
    mut counter: Single<&mut AcpModelRequestCounter>,
    mut last_used: Single<&mut AgentModelSelections>,
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
        model_state.pending = Some(crate::runtime::acp::PendingAcpModelSelection {
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

struct AcpModelProjection<'a> {
    model: Option<&'a AcpModelState>,
    session: &'a AcpSession,
    settings: Option<&'a AppSettings>,
}

impl AcpModelProjection<'_> {
    fn state(&self) -> ModelState {
        let agent_key = self.session.agent_id.as_str();
        let mut state = match self.model {
            Some(model) => ModelState {
                current_model_id: model.display_model_id().to_string(),
                current_model_name: model.current_name().to_string(),
                default_model_id: model.default_model_id.clone(),
                models: Self::options(model),
                ..Default::default()
            },
            None => ModelState::default(),
        };
        state.agent_key = agent_key.to_string();
        state.effort_current = self
            .settings
            .and_then(|settings| settings.agent.effort_for(agent_key))
            .unwrap_or("")
            .to_string();
        state.effort_default = default_effort(agent_key).to_string();
        state.effort_levels = effort_levels(agent_key)
            .iter()
            .map(|level| level.to_string())
            .collect();
        state
    }

    fn cross_runtime(&self) -> bool {
        super::registry::RegistryAgent::kind(&self.session.agent_id)
            .map(AgentKind::supports_cross_runtime)
            .unwrap_or(false)
    }

    fn options(model: &AcpModelState) -> Vec<ModelOptionEntry> {
        model
            .models
            .iter()
            .map(|model| ModelOptionEntry {
                id: model.id.clone(),
                name: model.name.clone(),
                description: model.description.clone().unwrap_or_default(),
            })
            .collect()
    }
}

struct AcpModeProjection<'a>(Option<&'a AcpModeState>);

impl AcpModeProjection<'_> {
    fn state(&self) -> ModeState {
        match self.0 {
            Some(mode) => ModeState {
                current_mode_id: mode.display_mode_id().to_string(),
                modes: mode.modes.clone(),
            },
            None => ModeState::default(),
        }
    }
}

fn on_start_select_model(
    trigger: On<UiInput<StartSelectModel>>,
    mut last_used: Single<&mut AgentModelSelections>,
) {
    let request = &trigger.event().payload;
    if request.agent_key.is_empty() || request.model_id.is_empty() {
        return;
    }
    last_used.select(&request.agent_key, &request.model_id);
}

fn on_start_select_mode(
    trigger: On<UiInput<StartSelectMode>>,
    mut last_used: Single<&mut AgentModeSelections>,
) {
    let request = &trigger.event().payload;
    if request.agent_key.is_empty() || request.mode_id.is_empty() {
        return;
    }
    last_used.select(&request.agent_key, &request.mode_id);
}

fn seed_cli_model_lists(
    sources: Query<
        (&crate::CliSessionSource, &crate::host::cli::CliModelCatalog),
        Added<crate::host::cli::CliModelCatalog>,
    >,
    mut selections: Single<&mut AgentModelSelections>,
) {
    for (source, catalog) in &sources {
        if catalog.models.is_empty() {
            continue;
        }
        let kind = source.kind;
        let agent_key = format!("cli:{}", kind.as_url_segment());
        let url = AgentPromptTarget::Cli(kind).url();
        selections.remember_catalog(&agent_key, &url, &catalog.selected, &catalog.models);
    }
}

fn remember_acp_model_lists(
    sessions: Query<(&AcpSession, &AcpModelState), Changed<AcpModelState>>,
    mut last_used: Single<&mut AgentModelSelections>,
) {
    for (session, state) in &sessions {
        let listed = AcpModelProjection::options(state);
        let current = state.display_model_id().to_string();
        let url = AgentSelectionKey::acp_url(&session.agent_id);
        last_used.remember_catalog(&session.agent_id, &url, &current, &listed);
    }
}

fn remember_acp_mode_lists(
    sessions: Query<(&AcpSession, &AcpModeState), Changed<AcpModeState>>,
    mut last_used: Single<&mut AgentModeSelections>,
) {
    for (session, state) in &sessions {
        let url = AgentSelectionKey::acp_url(&session.agent_id);
        last_used.remember_catalog(
            &session.agent_id,
            &url,
            state.display_mode_id(),
            &state.modes,
        );
    }
}

fn publish_agent_models(
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

fn publish_agent_modes(
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

fn push_acp_model_state_to_page(
    sessions: Query<(Entity, &AcpSession, &AcpModelState), Changed<AcpModelState>>,
    children: Query<&Children>,
    chat_views: Query<(), With<ChatView>>,
    settings: Option<Res<AppSettings>>,
    mut commands: Commands,
) {
    for (stack, session, model_state) in &sessions {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| chat_views.contains(entity)) else {
            continue;
        };
        let projection = AcpModelProjection {
            model: Some(model_state),
            session,
            settings: settings.as_deref(),
        };
        commands.trigger(ChatModelStateChanged::new(
            webview,
            projection.state(),
            projection.cross_runtime(),
        ));
    }
}

fn remove_model_state(
    mut removed: RemovedComponents<AcpModelState>,
    sessions: Query<&AcpSession>,
    children: Query<&Children>,
    chat_views: Query<(), With<ChatView>>,
    settings: Option<Res<AppSettings>>,
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
        let projection = AcpModelProjection {
            model: None,
            session,
            settings: settings.as_deref(),
        };
        commands.trigger(ChatModelStateChanged::new(
            webview,
            projection.state(),
            projection.cross_runtime(),
        ));
    }
}

fn push_acp_mode_state_to_page(
    sessions: Query<(Entity, &AcpModeState), Changed<AcpModeState>>,
    children: Query<&Children>,
    chat_views: Query<(), With<ChatView>>,
    mut commands: Commands,
) {
    for (stack, mode_state) in &sessions {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| chat_views.contains(entity)) else {
            continue;
        };
        commands.trigger(ChatModeStateChanged::new(
            webview,
            AcpModeProjection(Some(mode_state)).state(),
        ));
    }
}

fn remove_mode_state(
    mut removed: RemovedComponents<AcpModeState>,
    children: Query<&Children>,
    chat_views: Query<(), With<ChatView>>,
    mut commands: Commands,
) {
    for stack in removed.read() {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| chat_views.contains(entity)) else {
            continue;
        };
        commands.trigger(ChatModeStateChanged::new(
            webview,
            AcpModeProjection(None).state(),
        ));
    }
}

fn sync_page_model_state(
    trigger: On<UiInput<PageReady>>,
    views: Query<&ChildOf, With<ChatView>>,
    sessions: Query<(&AcpSession, Option<&AcpModelState>, Option<&AcpModeState>)>,
    settings: Option<Res<AppSettings>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(parent) = views.get(webview) else {
        return;
    };
    let Ok((session, model, mode)) = sessions.get(parent.parent()) else {
        return;
    };
    let projection = AcpModelProjection {
        model,
        session,
        settings: settings.as_deref(),
    };
    commands.trigger(ChatModelStateChanged::new(
        webview,
        projection.state(),
        projection.cross_runtime(),
    ));
    commands.trigger(ChatModeStateChanged::new(
        webview,
        AcpModeProjection(mode).state(),
    ));
}

fn on_select_model(
    trigger: On<UiInput<SelectModel>>,
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
    trigger: On<UiInput<SelectMode>>,
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
    mut counter: Single<&mut AcpModeRequestCounter>,
    mut last_used: Single<&mut AgentModeSelections>,
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
        state.pending = Some(crate::runtime::acp::PendingAcpModeSelection {
            request_id,
            mode_id: selection.mode_id.clone(),
        });
    }
}

fn on_set_agent_effort(
    trigger: On<UiInput<SetAgentEffort>>,
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
    mut settings: ResMut<AppSettings>,
    mut writes: MessageWriter<SettingsWriteRequest>,
) {
    for request in reader.read() {
        let (agent_key, level) = (request.agent_key.as_str(), request.level.as_str());
        if agent_key.is_empty() {
            continue;
        }
        if !level.is_empty() && !effort_levels(agent_key).contains(&level) {
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
                writes.write(SettingsWriteRequest { ron_bytes });
            }
            Err(error) => bevy::log::warn!("effort: persist for {agent_key} failed: {error}"),
        }
    }
}

fn apply_last_used_acp_model(
    mut sessions: Query<(&AcpSession, &mut AcpModelState), Added<AcpModelState>>,
    last_used: Single<&AgentModelSelections>,
    mut counter: Single<&mut AcpModelRequestCounter>,
    mut requests: MessageWriter<AcpSetModelRequest>,
) {
    for (session, mut state) in &mut sessions {
        let Some(remembered) = last_used
            .by_agent
            .get(AgentSelectionKey::normalize(&session.agent_id))
        else {
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
        state.pending = Some(crate::runtime::acp::PendingAcpModelSelection {
            request_id,
            model_id: model_id.clone(),
        });
    }
}

fn send_acp_model_requests(
    mut requests: MessageReader<AcpSetModelRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        service_requests.write(ServiceRequest(ClientMessage::AcpSetModel {
            sid: request.sid.clone(),
            request_id: request.request_id,
            config_id: request.config_id.clone(),
            model_id: request.model_id.clone(),
        }));
    }
}

fn send_acp_mode_requests(
    mut requests: MessageReader<AcpSetModeRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        service_requests.write(ServiceRequest(ClientMessage::AcpSetMode {
            sid: request.sid.clone(),
            request_id: request.request_id,
            config_id: request.config_id.clone(),
            mode_id: request.mode_id.clone(),
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
                AcpModelRequestCounter::default(),
                AgentModelSelections::default(),
            ))
            .id();
        app.add_message::<AcpSetModelRequest>()
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
                        vmux_api::protocol::AcpModelOption {
                            id: "default".into(),
                            name: "Default".into(),
                            description: None,
                        },
                        vmux_api::protocol::AcpModelOption {
                            id: "fable".into(),
                            name: "Fable".into(),
                            description: None,
                        },
                    ],
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
                .resource_mut::<Messages<AcpSetModelRequest>>()
                .drain()
                .count(),
            0
        );
    }

    #[test]
    fn mode_selection_updates_cached_state_before_response() {
        let mut app = App::new();
        app.world_mut().spawn((
            AcpModeRequestCounter::default(),
            AgentModeSelections::default(),
        ));
        app.add_message::<AcpSetModeRequest>()
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
                        vmux_api::protocol::AcpModeOption {
                            id: "ask".into(),
                            name: "Ask".into(),
                            description: None,
                        },
                        vmux_api::protocol::AcpModeOption {
                            id: "auto".into(),
                            name: "Auto Allow".into(),
                            description: None,
                        },
                    ],
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
        app.world_mut().spawn(selections);
        app.world_mut()
            .spawn(vmux_command::snapshot::CommandBarProjection::default());
        app.add_systems(Update, publish_agent_models);

        app.update();

        let published = app
            .world()
            .iter_entities()
            .find_map(|entity| entity.get::<vmux_command::snapshot::CommandBarProjection>())
            .unwrap();
        let published = &published.agent_models;
        assert_eq!(published.agents.len(), 1);
        assert_eq!(published.agents[0].agent_key, "cli:codex");
        assert_eq!(published.agents[0].url, "vmux://sessions/codex/cli");
        assert_eq!(published.agents[0].selected, "gpt-next");
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
                remember_acp_mode_lists,
                publish_agent_modes.after(remember_acp_mode_lists),
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
            AcpModeState {
                config_id: "mode".into(),
                current_mode_id: "agent".into(),
                pending: None,
                modes: vec![vmux_api::protocol::AcpModeOption {
                    id: "agent".into(),
                    name: "Agent".into(),
                    description: None,
                }],
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
    fn agent_selection_keys_stay_stable_when_loaded_again() {
        for agent_id in [
            "claude",
            "claude-acp",
            "codex",
            "codex-acp",
            "vibe",
            "vibe-acp",
            "mistral-vibe",
            "custom",
            "custom-acp",
        ] {
            let once = AgentSelectionKey::normalize(agent_id);
            let twice = AgentSelectionKey::normalize(once);
            assert_eq!(once, twice, "{agent_id}");
        }
    }

    #[test]
    fn fresh_agent_session_applies_last_used_model() {
        let mut app = App::new();
        let registry = app
            .world_mut()
            .spawn((
                AcpModelRequestCounter::default(),
                AgentModelSelections::default(),
            ))
            .id();
        app.add_message::<AcpSetModelRequest>()
            .add_systems(Update, apply_last_used_acp_model);
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
                AcpModelState {
                    config_id: "model".into(),
                    current_model_id: "default".into(),
                    default_model_id: "default".into(),
                    pending: None,
                    models: vec![
                        vmux_api::protocol::AcpModelOption {
                            id: "default".into(),
                            name: "Default".into(),
                            description: None,
                        },
                        vmux_api::protocol::AcpModelOption {
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
