use bevy_app::{App, PostUpdate, Startup};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use vmux_ecs::profile::CurrentProfile;
use vmux_ecs::{PageOpenTarget, UiStatePlugin, UiStateWriter};

use super::{SessionManagerView, StartRequest};
use crate::onboarding::{AgentOption, OnboardingDraftChanged, OnboardingStart, OnboardingUiState};

pub(super) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiStatePlugin::<OnboardingUiState>::default())
            .add_plugins(UiStatePlugin::<crate::onboarding::KeyEntryUiState>::default())
            .add_plugins(UiEventPlugin::<(OnboardingDraftChanged, OnboardingStart)>::default())
            .add_message::<ProvidersChanged>()
            .add_message::<SetupPrompt>()
            .add_message::<StartRequest>()
            .add_message::<vmux_ecs::PageOpenRequest>()
            .add_systems(Startup, load)
            .add_observer(edit)
            .add_observer(directory)
            .add_observer(start)
            .add_observer(created)
            .add_systems(PostUpdate, (receive, providers, publish, save).chain());
    }
}

#[derive(Message)]
pub struct ProvidersChanged(pub Vec<AgentOption>);

#[derive(Message)]
pub struct SetupPrompt {
    pub text: String,
    pub attachments: Vec<vmux_api::protocol::AgentAttachment>,
}

#[derive(Component, Default)]
struct Catalog(Vec<AgentOption>);

#[derive(Component, Default, serde::Serialize, serde::Deserialize)]
struct Draft {
    text: String,
    agent_id: String,
    cwd: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    attachments: Vec<vmux_api::protocol::AgentAttachment>,
    #[serde(skip)]
    starting: bool,
    #[serde(skip)]
    dirty: bool,
}

impl Draft {
    fn load(path: &std::path::Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(path, &bytes).map_err(|error| error.to_string())
    }
}

fn load(profile: CurrentProfile, mut commands: Commands) {
    let draft = match profile.paths() {
        Some(paths) => Draft::load(&paths.profile().join("onboarding.json")),
        None => Draft::default(),
    };
    commands.spawn((
        Name::new("Conversation setup"),
        draft,
        Catalog::default(),
        vmux_command::ContributedPage {
            id: "conversation-setup".into(),
            rank: 1000,
            page: vmux_api::command_bar::CommandBarPage {
                url: "vmux://sessions/inbox?setup=1".into(),
                title: vmux_ui::i18n::translate("onboarding-start"),
                keywords: vec![
                    "onboarding".into(),
                    "setup".into(),
                    "agent".into(),
                    "install".into(),
                ],
                prompt_target: true,
                ..Default::default()
            },
        },
    ));
}

fn edit(
    input: On<UiInput<OnboardingDraftChanged>>,
    views: Query<(), With<SessionManagerView>>,
    mut drafts: Query<&mut Draft>,
) {
    if !views.contains(input.event().webview) {
        return;
    }
    let Ok(mut draft) = drafts.single_mut() else {
        return;
    };
    if draft.starting {
        return;
    }
    let next = &input.event().payload;
    if draft.text != next.text || draft.agent_id != next.agent_id || draft.cwd != next.cwd {
        draft.session_id.clear();
    }
    draft.text.clone_from(&next.text);
    draft.agent_id.clone_from(&next.agent_id);
    draft.cwd.clone_from(&next.cwd);
    draft.dirty = true;
}

fn directory(
    update: On<vmux_ecs::UiStateWrite<crate::state::ChatUiState>>,
    views: Query<(), With<SessionManagerView>>,
    mut drafts: Query<&mut Draft>,
) {
    if !views.contains(update.event().webview()) {
        return;
    }
    let Some(directory) = &update.event().update().session_directory else {
        return;
    };
    let Ok(mut draft) = drafts.single_mut() else {
        return;
    };
    if !draft.starting {
        if draft.cwd != directory.path {
            draft.session_id.clear();
        }
        draft.cwd.clone_from(&directory.path);
        draft.dirty = true;
    }
}

#[derive(bevy_ecs::system::SystemParam)]
struct ConversationRequests<'w> {
    starts: MessageWriter<'w, StartRequest>,
    opens: MessageWriter<'w, vmux_ecs::PageOpenRequest>,
}

fn start(
    input: On<UiInput<OnboardingStart>>,
    views: Query<&ChildOf, With<SessionManagerView>>,
    keys: Query<&crate::onboarding::KeyPending>,
    mut state: Query<(&mut Draft, &Catalog)>,
    profile: CurrentProfile,
    mut requests: ConversationRequests,
    sessions: Query<&crate::SessionId, With<crate::Session>>,
) {
    let Ok(parent) = views.get(input.event().webview) else {
        return;
    };
    let Ok((mut draft, catalog)) = state.single_mut() else {
        return;
    };
    if keys
        .get(input.event().webview)
        .is_ok_and(|pending| pending.0)
        || draft.starting
        || draft.text.trim().is_empty()
    {
        return;
    }
    if !draft.session_id.is_empty() && sessions.iter().any(|id| id.0 == draft.session_id) {
        requests.opens.write(vmux_ecs::PageOpenRequest {
            target: PageOpenTarget::Stack(parent.parent()),
            url: crate::Route::Session(crate::SessionId(draft.session_id.clone())).url(),
            request_id: None,
        });
        return;
    }
    let Some(agent) = catalog.0.iter().find(|agent| agent.id == draft.agent_id) else {
        return;
    };
    let cwd = if draft.cwd.is_empty() {
        let Some(paths) = profile.paths() else {
            return;
        };
        paths.projects()
    } else {
        std::path::PathBuf::from(&draft.cwd)
    };
    let Ok(cwd) = cwd.canonicalize() else {
        return;
    };
    if !cwd.is_dir() {
        return;
    }
    let request = StartRequest::from_prompt(
        draft.text.clone(),
        cwd,
        crate::AgentId(agent.id.clone()),
        PageOpenTarget::Stack(parent.parent()),
        draft.attachments.clone(),
    );
    draft.session_id.clone_from(&request.id().0);
    requests.starts.write(request);
    draft.starting = true;
    draft.dirty = true;
}

fn created(event: On<crate::Created>, mut drafts: Query<&mut Draft>) {
    let Ok(mut draft) = drafts.single_mut() else {
        return;
    };
    if draft.session_id == event.event().id.0 {
        draft.starting = false;
    }
}

fn receive(mut requests: MessageReader<SetupPrompt>, mut drafts: Query<&mut Draft>) {
    let Ok(mut draft) = drafts.single_mut() else {
        return;
    };
    for request in requests.read() {
        if !draft.starting {
            draft.text.clone_from(&request.text);
            draft.attachments.clone_from(&request.attachments);
            draft.session_id.clear();
            draft.dirty = true;
        }
    }
}

fn providers(mut updates: MessageReader<ProvidersChanged>, mut catalogs: Query<&mut Catalog>) {
    let Ok(mut catalog) = catalogs.single_mut() else {
        return;
    };
    for update in updates.read() {
        if catalog.0 != update.0 {
            catalog.0.clone_from(&update.0);
        }
    }
}

fn publish(
    state: Query<(&Draft, &Catalog)>,
    views: Query<(Entity, &vmux_ecs::PageMetadata), With<SessionManagerView>>,
    mut writer: UiStateWriter<OnboardingUiState>,
    mut published: Local<std::collections::HashMap<Entity, (String, OnboardingUiState)>>,
) {
    let Ok((draft, catalog)) = state.single() else {
        return;
    };
    let next = OnboardingUiState {
        agents: catalog.0.clone(),
        text: draft.text.clone(),
        agent_id: draft.agent_id.clone(),
        cwd: draft.cwd.clone(),
        starting: draft.starting,
        session_id: draft.session_id.clone(),
    };
    published.retain(|view, _| views.contains(*view));
    for (view, metadata) in &views {
        if published
            .get(&view)
            .is_some_and(|(url, state)| url == &metadata.url && state == &next)
        {
            continue;
        }
        published.insert(view, (metadata.url.clone(), next.clone()));
        writer.push(view, next.clone());
    }
}

fn save(profile: CurrentProfile, mut drafts: Query<&mut Draft>) {
    let Ok(mut draft) = drafts.single_mut() else {
        return;
    };
    if !draft.dirty {
        return;
    }
    let Some(paths) = profile.paths() else {
        return;
    };
    match draft.save(&paths.profile().join("onboarding.json")) {
        Ok(()) => draft.dirty = false,
        Err(error) => bevy::log::warn!("conversation draft could not be saved: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_manager_views_can_start_and_repeated_input_creates_one_session() {
        let workspace = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.update();
        let stack = app.world_mut().spawn_empty().id();
        let manager = app
            .world_mut()
            .spawn((SessionManagerView, ChildOf(stack)))
            .id();
        let unrelated = app.world_mut().spawn_empty().id();
        app.world_mut()
            .write_message(ProvidersChanged(vec![AgentOption {
                id: "test-agent".into(),
                name: "Test agent".into(),
                installed: false,
                ..Default::default()
            }]));
        app.update();
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: OnboardingDraftChanged {
                text: "Build a small clock".into(),
                agent_id: "test-agent".into(),
                cwd: workspace.path().to_string_lossy().into_owned(),
            },
        });
        app.world_mut().trigger(UiInput {
            webview: unrelated,
            payload: OnboardingStart,
        });
        app.world_mut().flush();
        assert_eq!(app.world().resource::<Messages<StartRequest>>().len(), 0);
        app.world_mut()
            .get_mut::<crate::onboarding::KeyPending>(manager)
            .unwrap()
            .0 = true;
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: OnboardingStart,
        });
        app.world_mut().flush();
        assert_eq!(app.world().resource::<Messages<StartRequest>>().len(), 0);
        app.world_mut()
            .get_mut::<crate::onboarding::KeyPending>(manager)
            .unwrap()
            .0 = false;
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: OnboardingStart,
        });
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: OnboardingStart,
        });
        app.world_mut().flush();
        assert_eq!(app.world().resource::<Messages<StartRequest>>().len(), 1);
    }

    #[test]
    fn persisted_setup_reopens_its_existing_session_instead_of_creating_another() {
        let workspace = tempfile::tempdir().unwrap();
        let draft_path = workspace.path().join("onboarding.json");
        Draft {
            text: "Explain the build failure".into(),
            agent_id: "test-agent".into(),
            session_id: "existing-session".into(),
            ..Default::default()
        }
        .save(&draft_path)
        .unwrap();
        let restored = Draft::load(&draft_path);
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.update();
        let state = app
            .world_mut()
            .query_filtered::<Entity, With<Draft>>()
            .single(app.world())
            .unwrap();
        app.world_mut().entity_mut(state).insert(restored);
        app.world_mut()
            .spawn((crate::Session, crate::SessionId("existing-session".into())));
        let stack = app.world_mut().spawn_empty().id();
        let manager = app
            .world_mut()
            .spawn((SessionManagerView, ChildOf(stack)))
            .id();
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: OnboardingStart,
        });
        app.world_mut().flush();
        assert_eq!(app.world().resource::<Messages<StartRequest>>().len(), 0);
        let opens = app
            .world_mut()
            .resource_mut::<Messages<vmux_ecs::PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "vmux://sessions/existing-session");
    }
}
