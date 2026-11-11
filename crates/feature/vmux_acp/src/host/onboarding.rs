use bevy::prelude::*;
use vmux_session::host::ProvidersChanged;
use vmux_session::onboarding::AgentOption;
use vmux_setting::AppSettings;

use super::acp::registry::RegistryAgent;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::protocol::ClientMessage;
use vmux_ecs::service::ServiceRequest;
use vmux_session::onboarding::{ChatAuthenticate, ChatCancelSetup};
use vmux_session::{AgentId, RunState, Session, SessionId};

pub(super) fn add(app: &mut App) {
    app.add_message::<ProvidersChanged>()
        .add_message::<ServiceRequest>()
        .add_plugins(UiEventPlugin::<(ChatAuthenticate, ChatCancelSetup)>::default())
        .add_observer(authenticate)
        .add_observer(cancel)
        .add_systems(Startup, capture.after(vmux_setting::SettingsLoadSet))
        .add_systems(
            Update,
            (
                receive.in_set(AvailabilitySet),
                (support, publish).chain().in_set(ProjectionSet),
            )
                .chain()
                .after(vmux_command::WriteCommandBarSnapshots),
        );
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct AvailabilitySet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ProjectionSet;

fn support(
    views: Query<Entity, With<vmux_session::host::SessionView>>,
    targets: vmux_session::host::SessionViews,
    sessions: Query<(&SessionId, &AgentId), With<Session>>,
    mut published: Local<std::collections::HashMap<Entity, (SessionId, AgentId)>>,
    mut writer: vmux_ecs::UiStateWriter<vmux_session::onboarding::KeyEntryUiState>,
) {
    published.retain(|view, _| views.contains(*view));
    for view in &views {
        let Some(entity) = targets.session(view) else {
            continue;
        };
        let Ok((session, agent)) = sessions.get(entity) else {
            continue;
        };
        if published
            .get(&view)
            .is_some_and(|(id, current)| id == session && current == agent)
        {
            continue;
        }
        published.insert(view, (session.clone(), agent.clone()));
        writer.push(
            view,
            vmux_session::onboarding::KeyEntryUiState {
                agent_id: agent.0.clone(),
                key_variable: AgentCredential::input_variable(&agent.0),
                ..Default::default()
            },
        );
    }
}

fn authenticate(
    input: On<UiInput<ChatAuthenticate>>,
    views: vmux_session::host::SessionViews,
    mut sessions: Query<(&SessionId, &mut RunState), With<Session>>,
    mut requests: MessageWriter<ServiceRequest>,
) {
    let Some(session) = views.session(input.event().webview) else {
        return;
    };
    let Ok((id, mut state)) = sessions.get_mut(session) else {
        return;
    };
    let RunState::AuthenticationRequired {
        methods, pending, ..
    } = &mut *state
    else {
        return;
    };
    let method_id = &input.event().payload.method_id;
    if *pending || !methods.iter().any(|method| &method.id == method_id) {
        return;
    }
    *pending = true;
    requests.write(ServiceRequest(ClientMessage::AuthenticateAcpAgent {
        sid: id.0.clone(),
        method_id: method_id.clone(),
    }));
}

fn cancel(
    input: On<UiInput<ChatCancelSetup>>,
    views: vmux_session::host::SessionViews,
    mut sessions: Query<&mut RunState, With<Session>>,
    mut commands: Commands,
) {
    let Some(session) = views.session(input.event().webview) else {
        return;
    };
    let Ok(mut state) = sessions.get_mut(session) else {
        return;
    };
    if matches!(
        *state,
        RunState::Preparing | RunState::Installing { .. } | RunState::AuthenticationRequired { .. }
    ) {
        *state = RunState::Errored("ACP setup cancelled".into());
        commands
            .entity(session)
            .insert(super::navigation_driver::RetryRequested)
            .remove::<vmux_ecs::ProcessAnchor>();
    }
}

fn publish(
    agents: Query<(&RegistryAgent, Has<super::command_bar::InstalledAgent>)>,
    settings: Option<Res<AppSettings>>,
    mut published: Local<Vec<AgentOption>>,
    mut updates: MessageWriter<ProvidersChanged>,
    keys: Query<&AgentCredential>,
) {
    let Some(settings) = settings else {
        return;
    };
    let keys = keys.single().copied().unwrap_or_default();
    let mut options = Vec::new();
    for (agent, installed) in &agents {
        if agent.binary_for_host().is_none()
            && agent.distribution.npx.is_none()
            && agent.distribution.uvx.is_none()
        {
            continue;
        }
        options.push(AgentOption {
            id: agent.id.clone(),
            name: agent.name.clone(),
            icon: agent.icon.clone().unwrap_or_default(),
            installed,
            environment_key: keys.available(&agent.id),
            key_variable: AgentCredential::input_variable(&agent.id),
        });
    }
    for config in &settings.agent.acp {
        if config.command.is_empty() || options.iter().any(|agent| agent.id == config.id) {
            continue;
        }
        options.push(AgentOption {
            id: config.id.clone(),
            name: config.name.clone(),
            installed: true,
            environment_key: keys.available(&config.id),
            key_variable: AgentCredential::input_variable(&config.id),
            ..Default::default()
        });
    }
    options.sort_by(|left, right| {
        (right.installed, right.environment_key)
            .cmp(&(left.installed, left.environment_key))
            .then_with(|| left.name.cmp(&right.name))
    });
    if *published != options {
        *published = options.clone();
        updates.write(ProvidersChanged(options));
    }
}

#[derive(Component, Default, Clone, Copy)]
pub(super) struct AgentCredential {
    anthropic: bool,
    openai: bool,
    mistral: bool,
}

#[derive(Component)]
struct CredentialScan(bevy::tasks::Task<AgentCredential>);

fn capture(
    settings: Res<AppSettings>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let shell = settings.shell();
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let task = bevy::tasks::IoTaskPool::get().spawn(async move {
        let result = AgentCredential::capture(&shell);
        if let Some(wake) = wake {
            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
        result
    });
    commands.spawn((
        Name::new("Agent credential availability"),
        CredentialScan(task),
    ));
}

fn receive(mut scans: Query<(Entity, &mut CredentialScan)>, mut commands: Commands) {
    for (entity, mut scan) in &mut scans {
        if let Some(keys) = bevy::tasks::futures_lite::future::block_on(
            bevy::tasks::futures_lite::future::poll_once(&mut scan.0),
        ) {
            commands
                .entity(entity)
                .remove::<CredentialScan>()
                .insert(keys);
        }
    }
}

impl AgentCredential {
    pub(super) fn environment(
        _agent: &str,
        inherited: &[(String, String)],
        _application_data: &std::path::Path,
    ) -> Vec<(String, String)> {
        let mut environment = Vec::new();
        environment.extend(inherited.iter().cloned());
        #[cfg(target_os = "macos")]
        if let Some(variable) = Self::variable(_agent) {
            let store = vmux_ecs::profile::CredentialStore::at(
                vmux_ecs::profile::Profile::current(),
                _application_data.to_path_buf(),
                "acp",
            );
            match store.read(variable) {
                Ok(Some(value)) => {
                    if let Ok(value) = std::str::from_utf8(&value) {
                        environment.push((variable.into(), value.into()));
                    }
                }
                Ok(None) => {}
                Err(error) => bevy::log::warn!("ACP credential could not be read: {error}"),
            }
        }
        environment
    }

    pub(super) fn variable(agent: &str) -> Option<&'static str> {
        match agent {
            "claude" | "claude-acp" => Some("ANTHROPIC_API_KEY"),
            "codex" | "codex-acp" => Some("OPENAI_API_KEY"),
            "vibe" | "vibe-acp" | "mistral-vibe" => Some("MISTRAL_API_KEY"),
            _ => None,
        }
    }

    pub(super) fn input_variable(agent: &str) -> String {
        if cfg!(target_os = "macos") {
            Self::variable(agent).unwrap_or_default().into()
        } else {
            String::new()
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn saved(&mut self, agent: &str) {
        match Self::variable(agent) {
            Some("ANTHROPIC_API_KEY") => self.anthropic = true,
            Some("OPENAI_API_KEY") => self.openai = true,
            Some("MISTRAL_API_KEY") => self.mistral = true,
            _ => {}
        }
    }
    fn capture(shell: &str) -> Self {
        let environment = vmux_process::LoginShellEnvironment::get(shell);
        let mut keys = Self::default();
        for (name, value) in environment {
            if value.trim().is_empty() {
                continue;
            }
            match name.as_str() {
                "ANTHROPIC_API_KEY" => keys.anthropic = true,
                "OPENAI_API_KEY" => keys.openai = true,
                "MISTRAL_API_KEY" => keys.mistral = true,
                _ => {}
            }
        }
        #[cfg(target_os = "macos")]
        {
            let store = vmux_ecs::profile::CredentialStore::current("acp");
            for agent in ["claude-acp", "codex-acp", "mistral-vibe"] {
                if let Ok(Some(key)) = store.read(Self::variable(agent).unwrap_or_default())
                    && !key.is_empty()
                {
                    keys.saved(agent);
                }
            }
        }
        keys
    }

    fn available(self, agent: &str) -> bool {
        match agent {
            "claude" | "claude-acp" => self.anthropic,
            "codex" | "codex-acp" => self.openai,
            "vibe" | "vibe-acp" | "mistral-vibe" => self.mistral,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DeferredSettings;

    impl DeferredSettings {
        fn load(mut commands: Commands) {
            commands.insert_resource(AppSettings::default());
        }
    }

    #[test]
    fn credential_discovery_waits_for_startup_settings() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(vmux_ecs::UiStatePlugin::<
                vmux_session::onboarding::KeyEntryUiState,
            >::default())
            .add_systems(
                Startup,
                DeferredSettings::load.in_set(vmux_setting::SettingsLoadSet),
            );
        add(&mut app);

        app.update();

        assert!(app.world().contains_resource::<AppSettings>());
        let mut discovery = app
            .world_mut()
            .query_filtered::<Entity, Or<(With<CredentialScan>, With<AgentCredential>)>>();
        assert_eq!(discovery.iter(app.world()).count(), 1);
    }
}
