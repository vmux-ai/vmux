use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::protocol::ClientMessage;
use vmux_ecs::service::ServiceRequest;
use vmux_ecs::{UiStateWriter, profile::CredentialStore};
use vmux_session::host::{SessionManagerView, SessionViews};
use vmux_session::onboarding::{ChatSaveKey, KeyEntryUiState, KeyPending, OnboardingSaveKey};
use vmux_session::{AgentId, Session, SessionId};

use super::onboarding::AgentCredential;

pub(super) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(OnboardingSaveKey, ChatSaveKey)>::default())
            .add_message::<vmux_session::host::RecoverPrompt>()
            .add_observer(save)
            .add_observer(replace)
            .add_systems(
                Update,
                finish
                    .after(super::onboarding::AvailabilitySet)
                    .before(super::onboarding::ProjectionSet),
            );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct KeyEntry<'w, 's> {
    progress: Query<'w, 's, &'static mut KeyPending>,
    writer: UiStateWriter<'w, 's, KeyEntryUiState>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct Recovery<'w, 's> {
    views: SessionViews<'w, 's>,
    sessions: Query<
        'w,
        's,
        (
            &'static SessionId,
            &'static AgentId,
            &'static vmux_session::RunState,
        ),
        With<Session>,
    >,
    transcripts: vmux_session::Transcripts<'w, 's>,
    queues: vmux_session::QueuedPrompts<'w, 's>,
    recoveries: MessageWriter<'w, vmux_session::host::RecoverPrompt>,
    requests: MessageWriter<'w, ServiceRequest>,
}

#[derive(Component)]
struct Save {
    webview: Entity,
    session: Option<SessionId>,
    task: bevy::tasks::Task<Result<(), String>>,
}

impl Save {
    fn start(
        webview: Entity,
        session: Option<SessionId>,
        agent_id: &str,
        key: vmux_api::Secret,
        wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
    ) -> Option<Self> {
        let variable = AgentCredential::variable(agent_id)?;
        if key.expose().trim().is_empty() {
            return None;
        }
        let task = bevy::tasks::IoTaskPool::get().spawn(async move {
            let result =
                CredentialStore::current("acp").write(variable, key.expose().trim().as_bytes());
            drop(key);
            if let Some(wake) = wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        Some(Self {
            webview,
            session,
            task,
        })
    }
}

fn save(
    input: On<UiInput<OnboardingSaveKey>>,
    views: Query<(), With<SessionManagerView>>,
    pending: Query<&Save>,
    mut entry: KeyEntry,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = input.event().webview;
    if !views.contains(webview) || pending.iter().any(|save| save.webview == webview) {
        return;
    }
    let request = &input.event().payload;
    let Ok(mut progress) = entry.progress.get_mut(webview) else {
        return;
    };
    if progress.0 {
        return;
    }
    let Some(save) = Save::start(
        webview,
        None,
        &request.agent_id,
        request.key.clone(),
        proxy.as_deref().map(|proxy| (**proxy).clone()),
    ) else {
        return;
    };
    progress.0 = true;
    entry.writer.push(
        webview,
        KeyEntryUiState {
            agent_id: request.agent_id.clone(),
            key_variable: AgentCredential::input_variable(&request.agent_id),
            pending: true,
            failed: false,
        },
    );
    commands.spawn((
        Name::new("Save agent key"),
        AgentId(request.agent_id.clone()),
        save,
    ));
}

fn replace(
    input: On<UiInput<ChatSaveKey>>,
    views: SessionViews,
    sessions: Query<(&SessionId, &AgentId, &vmux_session::RunState), With<Session>>,
    pending: Query<&Save>,
    mut entry: KeyEntry,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = input.event().webview;
    if pending.iter().any(|save| save.webview == webview) {
        return;
    }
    let Some(entity) = views.session(webview) else {
        return;
    };
    let Ok((session, agent, state)) = sessions.get(entity) else {
        return;
    };
    if !state.needs_credentials() {
        return;
    }
    let Ok(mut progress) = entry.progress.get_mut(webview) else {
        return;
    };
    if progress.0 {
        return;
    }
    let Some(save) = Save::start(
        webview,
        Some(session.clone()),
        &agent.0,
        input.event().payload.key.clone(),
        proxy.as_deref().map(|proxy| (**proxy).clone()),
    ) else {
        return;
    };
    progress.0 = true;
    entry.writer.push(
        webview,
        KeyEntryUiState {
            agent_id: agent.0.clone(),
            key_variable: AgentCredential::input_variable(&agent.0),
            pending: true,
            failed: false,
        },
    );
    commands.spawn((Name::new("Replace agent key"), agent.clone(), save));
}

fn finish(
    mut saves: Query<(Entity, &AgentId, &mut Save)>,
    mut keys: Query<&mut AgentCredential>,
    mut recovery: Recovery,
    managers: Query<(), With<SessionManagerView>>,
    mut entry: KeyEntry,
    mut commands: Commands,
) {
    for (entity, agent, mut save) in &mut saves {
        let Some(result) = bevy::tasks::futures_lite::future::block_on(
            bevy::tasks::futures_lite::future::poll_once(&mut save.task),
        ) else {
            continue;
        };
        let failed = result.is_err();
        if let Err(error) = result {
            bevy::log::warn!("agent key could not be saved: {error}");
        } else {
            if let Ok(mut keys) = keys.single_mut() {
                keys.saved(&agent.0);
            }
            if let Some(expected) = &save.session
                && let Some(current) = recovery.views.session(save.webview)
                && let Ok((session, current_agent, state)) = recovery.sessions.get(current)
                && session == expected
                && current_agent == agent
                && state.needs_credentials()
            {
                if matches!(state, vmux_session::RunState::Errored(_))
                    && recovery.queues.empty(current)
                {
                    for message in recovery.transcripts.get(current).messages.iter().rev() {
                        if let vmux_api::session::Message::User { text, .. } = message {
                            recovery
                                .recoveries
                                .write(vmux_session::host::RecoverPrompt {
                                    session: session.clone(),
                                    text: text.clone(),
                                });
                            break;
                        }
                    }
                }
                recovery
                    .requests
                    .write(ServiceRequest(ClientMessage::CloseAgentSession {
                        sid: session.0.clone(),
                    }));
                commands.trigger(UiInput {
                    webview: save.webview,
                    payload: vmux_session::event::ChatRetrySession,
                });
            }
        }
        let active_view = match &save.session {
            None => managers.contains(save.webview),
            Some(expected) => recovery
                .views
                .session(save.webview)
                .and_then(|current| recovery.sessions.get(current).ok())
                .is_some_and(|(session, current_agent, _)| {
                    session == expected && current_agent == agent
                }),
        };
        if active_view {
            entry.writer.push(
                save.webview,
                KeyEntryUiState {
                    agent_id: agent.0.clone(),
                    key_variable: AgentCredential::input_variable(&agent.0),
                    pending: false,
                    failed,
                },
            );
        }
        if let Ok(mut progress) = entry.progress.get_mut(save.webview) {
            progress.0 = false;
        }
        commands.entity(entity).despawn();
    }
}
