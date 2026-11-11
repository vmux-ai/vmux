use bevy_app::{App, PostUpdate, Startup};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use vmux_ecs::profile::CurrentProfile;
use vmux_ecs::{PageOpenTarget, UiStatePlugin, UiStateWriter};

use super::{SessionManagerView, StartRequest};
use crate::onboarding::{
    AgentOption, OnboardingBack, OnboardingContinue, OnboardingDraftChanged, OnboardingStart,
    OnboardingUiState, Step,
};

pub(super) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiStatePlugin::<OnboardingUiState>::default())
            .add_plugins(UiStatePlugin::<crate::onboarding::KeyEntryUiState>::default())
            .add_plugins(UiEventPlugin::<(
                OnboardingDraftChanged,
                OnboardingContinue,
                OnboardingBack,
                OnboardingStart,
            )>::default())
            .add_message::<ProvidersChanged>()
            .add_message::<SetupPrompt>()
            .add_message::<StartRequest>()
            .add_message::<vmux_ecs::PageOpenRequest>()
            .add_systems(Startup, load)
            .add_observer(edit)
            .add_observer(advance)
            .add_observer(back)
            .add_observer(directory)
            .add_observer(start)
            .add_observer(created)
            .add_systems(
                PostUpdate,
                (begin, receive, providers, publish, save).chain(),
            );
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
    #[serde(default)]
    step: Step,
    #[serde(skip)]
    starting: bool,
    #[serde(skip)]
    dirty: bool,
}

#[derive(Component)]
struct SetupOpened;

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

fn begin(
    drafts: Query<&Draft>,
    sessions: Query<(), With<crate::Session>>,
    restore: Query<&vmux_ecs::store::WorkspaceRestore>,
    pages: Query<(&vmux_ecs::PageMetadata, &ChildOf), With<vmux_ecs::page::HostsPage>>,
    stacks: Query<&vmux_ecs::PageMetadata, (With<vmux_ecs::PageNavigation>, Without<SetupOpened>)>,
    mut commands: Commands,
) {
    let Ok(draft) = drafts.single() else {
        return;
    };
    if !draft.session_id.is_empty()
        || !sessions.is_empty()
        || restore.iter().any(|restore| !restore.complete)
    {
        return;
    }
    let mut opened = std::collections::HashSet::new();
    for (metadata, parent) in &pages {
        let Ok(stack) = stacks.get(parent.parent()) else {
            continue;
        };
        if metadata.url != "vmux://start/"
            || stack.url != "vmux://start/"
            || !opened.insert(parent.parent())
        {
            continue;
        }
        commands.entity(parent.parent()).insert(SetupOpened);
        commands.write_message(vmux_ecs::PageOpenRequest {
            target: PageOpenTarget::Stack(parent.parent()),
            url: "vmux://sessions/inbox?setup=1".into(),
            request_id: None,
        });
    }
}

fn advance(
    input: On<UiInput<OnboardingContinue>>,
    views: Query<(), With<SessionManagerView>>,
    mut drafts: Query<(&mut Draft, &Catalog)>,
) {
    if !views.contains(input.event().webview) {
        return;
    }
    let Ok((mut draft, catalog)) = drafts.single_mut() else {
        return;
    };
    if draft.starting || draft.step != input.event().payload.step {
        return;
    }
    match draft.step {
        Step::Prompt if !draft.text.trim().is_empty() => draft.step = Step::Agent,
        Step::Agent if catalog.0.iter().any(|agent| agent.id == draft.agent_id) => {
            draft.step = Step::Workspace
        }
        _ => return,
    }
    draft.dirty = true;
}

fn back(
    input: On<UiInput<OnboardingBack>>,
    views: Query<(), With<SessionManagerView>>,
    mut drafts: Query<&mut Draft>,
) {
    if !views.contains(input.event().webview) {
        return;
    }
    let Ok(mut draft) = drafts.single_mut() else {
        return;
    };
    if draft.starting || draft.step != input.event().payload.step {
        return;
    }
    draft.step = match draft.step {
        Step::Prompt => return,
        Step::Agent => Step::Prompt,
        Step::Workspace => Step::Agent,
    };
    draft.dirty = true;
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
            draft.step = Step::Agent;
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
        step: draft.step,
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
    fn first_profile_enters_setup_after_restore_without_redirecting_warm_pages_or_repeating() {
        let mut app = App::new();
        app.add_plugins(Plugin);
        let restore = app
            .world_mut()
            .spawn(vmux_ecs::store::WorkspaceRestore::default())
            .id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_ecs::PageNavigation::new(vmux_ecs::PageOpenId::new()),
                vmux_ecs::PageMetadata {
                    url: "vmux://start/".into(),
                    ..Default::default()
                },
            ))
            .id();
        let warm = app.world_mut().spawn_empty().id();
        let navigating = app
            .world_mut()
            .spawn((
                vmux_ecs::PageNavigation::new(vmux_ecs::PageOpenId::new()),
                vmux_ecs::PageMetadata {
                    url: "https://example.com/".into(),
                    ..Default::default()
                },
            ))
            .id();
        for parent in [stack, warm, navigating] {
            app.world_mut().spawn((
                vmux_ecs::PageMetadata {
                    url: "vmux://start/".into(),
                    ..Default::default()
                },
                vmux_ecs::page::HostsPage,
                ChildOf(parent),
            ));
        }
        app.update();
        assert_eq!(
            app.world()
                .resource::<Messages<vmux_ecs::PageOpenRequest>>()
                .len(),
            0
        );
        app.world_mut()
            .get_mut::<vmux_ecs::store::WorkspaceRestore>(restore)
            .unwrap()
            .complete = true;
        app.update();
        let opens: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_ecs::PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(opens.len(), 1);
        assert!(matches!(opens[0].target, PageOpenTarget::Stack(target) if target == stack));
        assert_eq!(opens[0].url, "vmux://sessions/inbox?setup=1");
        app.update();
        assert_eq!(
            app.world()
                .resource::<Messages<vmux_ecs::PageOpenRequest>>()
                .len(),
            0
        );
    }

    #[test]
    fn a_profile_with_an_existing_session_keeps_its_launcher() {
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.world_mut().spawn(crate::Session);
        let stack = app
            .world_mut()
            .spawn(vmux_ecs::PageNavigation::new(vmux_ecs::PageOpenId::new()))
            .id();
        app.world_mut().spawn((
            vmux_ecs::PageMetadata {
                url: "vmux://start/".into(),
                ..Default::default()
            },
            vmux_ecs::page::HostsPage,
            ChildOf(stack),
        ));
        app.update();
        assert_eq!(
            app.world()
                .resource::<Messages<vmux_ecs::PageOpenRequest>>()
                .len(),
            0
        );
    }

    #[test]
    fn replies_advance_once_and_preserve_the_prompt_when_choosing_a_folder_or_going_back() {
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.update();
        let manager = app.world_mut().spawn(SessionManagerView).id();
        app.world_mut()
            .write_message(ProvidersChanged(vec![AgentOption {
                id: "test-agent".into(),
                name: "Test agent".into(),
                ..Default::default()
            }]));
        app.update();
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: OnboardingDraftChanged {
                text: "Build a clock".into(),
                agent_id: String::new(),
                cwd: String::new(),
            },
        });
        for _ in 0..2 {
            app.world_mut().trigger(UiInput {
                webview: manager,
                payload: OnboardingContinue { step: Step::Prompt },
            });
        }
        app.world_mut().flush();
        let draft = app
            .world_mut()
            .query_filtered::<Entity, With<Draft>>()
            .single(app.world())
            .unwrap();
        assert_eq!(app.world().get::<Draft>(draft).unwrap().step, Step::Agent);
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: OnboardingDraftChanged {
                text: "Build a clock".into(),
                agent_id: "test-agent".into(),
                cwd: String::new(),
            },
        });
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: OnboardingContinue { step: Step::Agent },
        });
        app.world_mut().flush();
        let update = crate::state::ChatUiStatePatch {
            session_directory: Some(crate::event::SessionDirectorySelection {
                revision: 1,
                path: "/tmp/project".into(),
            }),
            ..Default::default()
        };
        app.world_mut().trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(manager, &update),
        );
        app.world_mut().flush();
        assert_eq!(
            app.world().get::<Draft>(draft).unwrap().step,
            Step::Workspace
        );
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: OnboardingBack {
                step: Step::Workspace,
            },
        });
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: OnboardingBack {
                step: Step::Workspace,
            },
        });
        app.world_mut().flush();
        let current = app.world().get::<Draft>(draft).unwrap();
        assert_eq!(current.step, Step::Agent);
        assert_eq!(current.text, "Build a clock");
        assert_eq!(current.cwd, "/tmp/project");
        assert_eq!(app.world().resource::<Messages<StartRequest>>().len(), 0);
        let saved = tempfile::tempdir().unwrap();
        current.save(&saved.path().join("onboarding.json")).unwrap();
        let restored = Draft::load(&saved.path().join("onboarding.json"));
        assert_eq!(restored.step, Step::Agent);
        assert_eq!(restored.text, "Build a clock");
    }

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
