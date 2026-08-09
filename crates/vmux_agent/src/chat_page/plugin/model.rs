//! Which model a session runs, and how hard its agent is asked to think.
//!
//! The list comes from the agent over ACP, so a selection is optimistic: it is shown as current
//! while the request is in flight and reconciled when the agent answers. The page and a remote
//! peer both ask for the same two changes, so both write [`ModelSelectRequest`] and
//! [`EffortSetRequest`] and one system applies each.

use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};

use crate::chat_page::event::{
    MODEL_STATE_EVENT, ModelOptionEntry, ModelState, SLASH_COMMANDS_EVENT, SelectModel,
    SetAgentEffort, SlashCommands,
};
use crate::client::acp::{AcpModelState, AcpSession};
use crate::events::AgentCommandRequest;
use crate::strategy::{acp_agent_kind, kind_supports_cross_runtime};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{AgentCommand, AgentCommandResult, ClientMessage, SharedAgentCommand};
use vmux_wire::room::{RemoteModel, RemoteModelState};

/// Model selection and effort, for the page and for a remote peer.
pub(super) struct ChatModelPlugin;

impl Plugin for ChatModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AcpModelRequestCounter>()
            .init_resource::<LastUsedAcpModels>()
            .add_message::<AcpSetModelRequest>()
            .add_message::<ModelSelectRequest>()
            .add_message::<EffortSetRequest>()
            .add_plugins(
                BinEventEmitterPlugin::<(SelectModel, SetAgentEffort)>::for_hosts(&[
                    "agent", "start",
                ]),
            )
            .add_systems(Startup, load_last_used_acp_models)
            .add_observer(on_select_model)
            .add_observer(on_set_agent_effort)
            .add_systems(
                Update,
                (
                    answer_remote_model_commands,
                    apply_model_selection,
                    apply_effort_setting,
                    push_acp_model_state_to_page,
                    push_removed_acp_model_state_to_page,
                    apply_last_used_acp_model.after(crate::client::acp::apply_acp_model_info),
                    send_acp_model_requests,
                    save_last_used_acp_models.after(apply_last_used_acp_model),
                ),
            );
    }
}

/// Switch a session to one of its models.
#[derive(Message)]
pub(super) struct ModelSelectRequest {
    pub sid: String,
    pub model_id: String,
}

/// Set an agent's effort level, or clear it when the level is empty.
#[derive(Message)]
pub(super) struct EffortSetRequest {
    pub agent_key: String,
    pub level: String,
}

/// Serve the model commands a remote peer may issue.
///
/// Read here rather than in `vmux_agent`'s central command handler for the same reason the team
/// roster is: the state is local to this module, and that system is already at Bevy's parameter
/// limit.
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

/// The wire view of one session's models and effort, or `None` when no session has that id.
///
/// A free function because [`RemoteModelState`] belongs to `vmux_wire`, which cannot see a Bevy
/// `Query`; it is private to the single caller above.
fn remote_model_state(
    sid: &str,
    sessions: &Query<(&AcpSession, &AcpModelState)>,
    settings: &vmux_setting::AppSettings,
) -> Option<RemoteModelState> {
    let (session, model_state) = sessions.iter().find(|(session, _)| session.sid == sid)?;
    let mut models = Vec::new();
    for option in &model_state.models {
        models.push(RemoteModel {
            id: option.id.clone(),
            name: option.name.clone(),
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

/// Record the choice and ask the agent for it, showing it as current in the meantime.
fn apply_model_selection(
    mut reader: MessageReader<ModelSelectRequest>,
    mut sessions: Query<(&AcpSession, &mut AcpModelState)>,
    mut counter: ResMut<AcpModelRequestCounter>,
    mut last_used: ResMut<LastUsedAcpModels>,
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
        last_used.remember(&session.agent_id, &model_id);
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

#[derive(Resource, Default)]
struct LastUsedAcpModels {
    by_agent: std::collections::BTreeMap<String, String>,
    dirty: bool,
}

fn last_used_acp_models_path() -> std::path::PathBuf {
    vmux_core::profile::profile_dir().join("agent-models.json")
}

fn load_last_used_acp_models(mut models: ResMut<LastUsedAcpModels>) {
    let Ok(bytes) = std::fs::read(last_used_acp_models_path()) else {
        return;
    };
    let Ok(saved) = serde_json::from_slice::<std::collections::BTreeMap<String, String>>(&bytes)
    else {
        return;
    };
    models.by_agent = saved;
    models.dirty = false;
}

fn save_last_used_acp_models(mut models: ResMut<LastUsedAcpModels>) {
    if !models.dirty {
        return;
    }
    let path = last_used_acp_models_path();
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

#[derive(Resource, Default)]
struct AcpModelRequestCounter(u64);

fn model_state_of(state: Option<&AcpModelState>) -> ModelState {
    let Some(state) = state else {
        return ModelState::default();
    };
    ModelState {
        current_model_id: state.display_model_id().to_string(),
        current_model_name: state.current_name().to_string(),
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

/// The persisted launch-time effort for `agent_key`, or `""` (agent default).
pub(super) fn effort_current_for<'a>(
    settings: Option<&'a Res<vmux_setting::AppSettings>>,
    agent_key: &str,
) -> &'a str {
    settings
        .and_then(|settings| settings.agent.effort_for(agent_key))
        .unwrap_or("")
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
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
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
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
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

/// Persist the launch-time effort level for an agent. Blank `level` clears the override; only
/// levels valid for the agent (see [`vmux_core::agent::effort_levels`]) are stored. Takes effect
/// when the agent next launches a session/process.
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

/// Persist an effort level, or remove the override when the level is empty.
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
        match vmux_setting::apply_settings_update(settings.as_mut(), "agent.effort", value) {
            Ok(ron_bytes) => {
                writes.write(vmux_setting::SettingsWriteRequest { ron_bytes });
            }
            Err(error) => bevy::log::warn!("effort: persist for {agent_key} failed: {error}"),
        }
    }
}

fn apply_last_used_acp_model(
    mut sessions: Query<(&AcpSession, &mut AcpModelState), Added<AcpModelState>>,
    last_used: Res<LastUsedAcpModels>,
    mut counter: ResMut<AcpModelRequestCounter>,
    mut requests: MessageWriter<AcpSetModelRequest>,
) {
    for (session, mut state) in &mut sessions {
        let Some(model_id) = last_used.by_agent.get(&session.agent_id) else {
            continue;
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The list a session offers depends on what its agent can do, and the page renders it
    /// verbatim -- a command offered for an agent that cannot serve it fails on selection.
    #[test]
    fn slash_commands_include_cli_only_when_cross_runtime() {
        let names = |cross, models| {
            SlashCommands::for_agent(cross, models)
                .commands
                .iter()
                .map(|command| command.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(false, false), ["upload", "resume"]);
        assert_eq!(names(false, true), ["upload", "resume", "model"]);
        assert_eq!(names(true, false), ["upload", "resume", "cli"]);
        assert_eq!(names(true, true), ["upload", "resume", "model", "cli"]);
    }

    #[test]
    fn model_selection_updates_cached_state_before_response() {
        let mut app = App::new();
        app.init_resource::<AcpModelRequestCounter>()
            .init_resource::<LastUsedAcpModels>()
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
                .resource::<LastUsedAcpModels>()
                .by_agent
                .get("claude")
                .map(String::as_str),
            Some("fable")
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
    fn fresh_agent_session_applies_last_used_model() {
        let mut app = App::new();
        app.init_resource::<AcpModelRequestCounter>()
            .init_resource::<LastUsedAcpModels>()
            .add_message::<AcpSetModelRequest>()
            .add_systems(Update, apply_last_used_acp_model);
        app.world_mut()
            .resource_mut::<LastUsedAcpModels>()
            .by_agent
            .insert("claude".into(), "fable".into());
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

impl LastUsedAcpModels {
    fn remember(&mut self, agent_id: &str, model_id: &str) {
        if self
            .by_agent
            .get(agent_id)
            .is_some_and(|saved| saved == model_id)
        {
            return;
        }
        self.by_agent
            .insert(agent_id.to_string(), model_id.to_string());
        self.dirty = true;
    }
}

impl AcpModelRequestCounter {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }
}
