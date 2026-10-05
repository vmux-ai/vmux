use agent_client_protocol::schema::v1::SessionUpdate;
use bevy::prelude::{App, Query, Update};
use vmux_api::protocol::{
    AgentFileTouched, AgentRequest, AgentRequestId, ServiceMessage, SharedEvent,
};

use super::driver::AcpTranscriptInput;
use super::projection_driver::{AcpProjector, Intent};
use super::{AcpHistoryReplay, AcpSessionConfigs, AcpSessionShared, AcpTranscriptInbox};

pub(super) fn add(app: &mut App) {
    app.add_systems(Update, project);
}

fn project(
    mut sessions: Query<(
        &super::SessionId,
        &AcpSessionShared,
        &mut AcpTranscriptInbox,
        &mut AcpProjector,
        &mut AcpHistoryReplay,
        &mut AcpSessionConfigs,
    )>,
) {
    for (sid, shared, mut inbox, mut projector, mut replay, mut configs) in &mut sessions {
        while let Ok(input) = inbox.0.try_recv() {
            match input {
                AcpTranscriptInput::BeginHistoryReplay => {
                    *projector = AcpProjector::default();
                    replay.active = true;
                    replay.updates = 0;
                }
                AcpTranscriptInput::Update(update) => {
                    match update.as_ref() {
                        SessionUpdate::ConfigOptionUpdate(config) => {
                            let legacy = configs
                                .0
                                .iter()
                                .find(|config| config.config_id.is_none())
                                .cloned();
                            let mut next =
                                AcpSessionConfigs::from_acp(&config.config_options, None);
                            if !next
                                .0
                                .iter()
                                .any(|config| config.category.as_deref() == Some("mode"))
                                && let Some(legacy) = legacy
                            {
                                next.0.push(legacy);
                            }
                            if *configs != next {
                                *configs = next;
                                shared.0.emit(ServiceMessage::AcpSessionConfigState {
                                    sid: sid.0.clone(),
                                    configs: configs.0.clone(),
                                });
                            }
                        }
                        SessionUpdate::CurrentModeUpdate(current) => {
                            let value = current.current_mode_id.to_string();
                            if let Some(config) = configs
                                .0
                                .iter_mut()
                                .find(|config| config.config_id.is_none())
                                && config.current_value != value
                                && config.values.iter().any(|option| option.value == value)
                            {
                                config.current_value = value;
                                shared.0.emit(ServiceMessage::AcpSessionConfigState {
                                    sid: sid.0.clone(),
                                    configs: configs.0.clone(),
                                });
                            }
                        }
                        _ => {}
                    }
                    let intents = projector.apply(*update);
                    shared
                        .0
                        .projector_updates
                        .send_modify(|revision| *revision += 1);
                    if replay.active {
                        for intent in &intents {
                            if let Intent::WorkspaceChanged(workspace) = intent {
                                shared.0.publish_workspace_change(workspace);
                            }
                        }
                        replay.updates += 1;
                        if replay.updates == 1
                            || replay
                                .updates
                                .is_multiple_of(super::driver::HISTORY_REPLAY_SNAPSHOT_INTERVAL)
                        {
                            shared
                                .0
                                .emit(shared.0.snapshot_message(projector.messages()));
                        }
                        continue;
                    }
                    for intent in intents {
                        match intent {
                            Intent::Delta(text) => {
                                shared
                                    .0
                                    .emit(ServiceMessage::Shared(SharedEvent::AgentDelta {
                                        sid: sid.0.clone(),
                                        text,
                                    }))
                            }
                            Intent::Snapshot => shared
                                .0
                                .emit(shared.0.snapshot_message(projector.messages())),
                            Intent::ProposedDiff {
                                call_id,
                                path,
                                old_text,
                                new_text,
                            } => shared.0.emit(ServiceMessage::AcpProposedDiff {
                                sid: sid.0.clone(),
                                call_id,
                                path,
                                old_text,
                                new_text,
                            }),
                            Intent::FileTouched { path, line, kind } => {
                                let Ok(request) = AgentRequest::encode(&AgentFileTouched {
                                    anchor: shared.0.anchor,
                                    path,
                                    line,
                                    col: None,
                                    end_col: None,
                                    kind,
                                }) else {
                                    continue;
                                };
                                shared.0.emit(ServiceMessage::AgentRequest {
                                    request_id: AgentRequestId::new(),
                                    anchor: Some(shared.0.anchor),
                                    request,
                                });
                            }
                            Intent::WorkspaceChanged(workspace) => {
                                shared.0.publish_workspace_change(&workspace)
                            }
                        }
                    }
                }
                AcpTranscriptInput::FinishHistoryReplay(loaded) => {
                    if !loaded {
                        *projector = AcpProjector::default();
                    }
                    replay.active = false;
                    replay.updates = 0;
                    shared
                        .0
                        .emit(shared.0.snapshot_message(projector.messages()));
                }
                AcpTranscriptInput::PushUser { text, attachments } => {
                    projector.push_user(text, attachments);
                    shared
                        .0
                        .emit(shared.0.snapshot_message(projector.messages()));
                }
                AcpTranscriptInput::Snapshot => {
                    shared
                        .0
                        .emit(shared.0.snapshot_message(projector.messages()));
                }
                AcpTranscriptInput::ApprovalDetails { query, response } => {
                    let _ = response.send(projector.approval_details(&query));
                }
            }
        }
    }
}
