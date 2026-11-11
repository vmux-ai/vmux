use bevy_app::{App, PostUpdate, Startup};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use vmux_ecs::profile::{CurrentProfile, Projects};
use vmux_ecs::{PageOpenTarget, UiStatePlugin, UiStateWriter};

use super::{SessionManagerView, StartRequest};
use crate::onboarding::{AgentOption, OnboardingConnect, OnboardingUiState};

pub(super) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiStatePlugin::<OnboardingUiState>::default())
            .add_plugins(UiStatePlugin::<crate::onboarding::KeyEntryUiState>::default())
            .add_plugins(UiEventPlugin::<(OnboardingConnect,)>::default())
            .add_message::<ProvidersChanged>()
            .add_message::<StartRequest>()
            .add_message::<vmux_ecs::PageOpenRequest>()
            .add_systems(Startup, load)
            .add_observer(connect)
            .add_observer(created)
            .add_observer(cancelled)
            .add_systems(PostUpdate, (begin, providers, publish, save).chain());
    }
}

#[derive(Message)]
pub struct ProvidersChanged(pub Vec<AgentOption>);

#[derive(Component, Default)]
struct Catalog(Vec<AgentOption>);

#[derive(Component, serde::Serialize, serde::Deserialize)]
struct Draft {
    #[serde(default)]
    schema: u8,
    #[serde(default)]
    agent_id: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    paused: bool,
    #[serde(skip)]
    starting: bool,
    #[serde(skip)]
    failed: bool,
    #[serde(skip)]
    dirty: bool,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            schema: 2,
            agent_id: String::new(),
            session_id: String::new(),
            paused: false,
            starting: false,
            failed: false,
            dirty: false,
        }
    }
}

impl Draft {
    fn load(path: &std::path::Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        let Ok(mut draft) = serde_json::from_slice::<Self>(&bytes) else {
            return Self::default();
        };
        if draft.schema != 2 {
            draft.schema = 2;
            draft.session_id.clear();
            draft.dirty = true;
        }
        draft
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(path, &bytes).map_err(|error| error.to_string())
    }
}

#[derive(Component)]
struct Entered;

fn load(profile: CurrentProfile, mut commands: Commands) {
    let draft = match profile.paths() {
        Some(paths) => Draft::load(&paths.profile().join("onboarding.json")),
        None => Draft::default(),
    };
    commands.spawn((
        Name::new("Agent setup"),
        draft,
        Catalog::default(),
        vmux_command::ContributedPage {
            id: "conversation-setup".into(),
            rank: 1000,
            page: vmux_api::command_bar::CommandBarPage {
                url: "vmux://sessions/inbox?setup=1".into(),
                title: vmux_ui::i18n::translate("onboarding-setup"),
                keywords: vec![
                    "onboarding".into(),
                    "setup".into(),
                    "agent".into(),
                    "install".into(),
                ],
                ..Default::default()
            },
        },
    ));
}

type SetupSessions<'w, 's> = Query<
    'w,
    's,
    (
        &'static crate::SessionId,
        Option<&'static crate::AgentId>,
        Option<&'static crate::RunState>,
        Has<vmux_ecs::ProcessAnchor>,
        Option<&'static vmux_ecs::LastActivatedAt>,
    ),
    With<crate::Session>,
>;

fn begin(
    drafts: Query<(Entity, &Draft), Without<Entered>>,
    sessions: SetupSessions,
    restore: Query<&vmux_ecs::store::WorkspaceRestore>,
    pages: Query<(&vmux_ecs::PageMetadata, &ChildOf), With<vmux_ecs::page::HostsPage>>,
    stacks: Query<(&vmux_ecs::PageMetadata, Has<vmux_ecs::Active>), With<vmux_ecs::PageNavigation>>,
    mut commands: Commands,
) {
    let Ok((owner, draft)) = drafts.single() else {
        return;
    };
    if restore.iter().any(|restore| !restore.complete) {
        return;
    }
    let mut actual_page = false;
    let mut entry: Option<(Entity, bool)> = None;
    for (metadata, parent) in &pages {
        let Ok((stack, active)) = stacks.get(parent.parent()) else {
            continue;
        };
        actual_page = true;
        if metadata.url != "vmux://start/" || stack.url != "vmux://start/" {
            continue;
        }
        if entry.is_none_or(|(_, selected)| active && !selected) {
            entry = Some((parent.parent(), active));
        }
    }
    if !actual_page {
        return;
    }
    commands.entity(owner).insert(Entered);
    let Some((stack, _)) = entry else {
        return;
    };
    let mut recorded = None;
    let mut ready = None;
    for (id, agent, state, attached, activated) in &sessions {
        if !draft.paused
            && !draft.session_id.is_empty()
            && draft.session_id == id.0
            && agent.is_some_and(|agent| draft.agent_id.is_empty() || agent.0 == draft.agent_id)
        {
            recorded = Some(id.clone());
            break;
        }
        if !attached
            || !matches!(
                state,
                Some(
                    crate::RunState::Idle
                        | crate::RunState::Streaming
                        | crate::RunState::AwaitingApproval { .. }
                )
            )
        {
            continue;
        }
        if !draft.agent_id.is_empty() && agent.is_none_or(|agent| agent.0 != draft.agent_id) {
            continue;
        }
        let activated = activated.map(|value| value.0).unwrap_or_default();
        if ready
            .as_ref()
            .is_none_or(|(timestamp, previous): &(i64, crate::SessionId)| {
                activated > *timestamp || activated == *timestamp && id.0 < previous.0
            })
        {
            ready = Some((activated, id.clone()));
        }
    }
    let selected = recorded.or_else(|| ready.map(|(_, id)| id));
    let url = selected
        .map(|id| crate::Route::Session(id).url())
        .unwrap_or_else(|| "vmux://sessions/inbox?setup=1".to_string());
    commands.write_message(vmux_ecs::PageOpenRequest {
        target: PageOpenTarget::Stack(stack),
        url,
        request_id: None,
    });
}

#[derive(bevy_ecs::system::SystemParam)]
struct ConversationRequests<'w, 's> {
    starts: MessageWriter<'w, StartRequest>,
    opens: MessageWriter<'w, vmux_ecs::PageOpenRequest>,
    commands: Commands<'w, 's>,
}

fn connect(
    input: On<UiInput<OnboardingConnect>>,
    views: Query<&ChildOf, With<SessionManagerView>>,
    keys: Query<&crate::onboarding::KeyPending>,
    mut state: Query<(&mut Draft, &Catalog)>,
    projects: Projects,
    mut requests: ConversationRequests,
    sessions: Query<(&crate::SessionId, Option<&crate::AgentId>), With<crate::Session>>,
) {
    let Ok(parent) = views.get(input.event().webview) else {
        return;
    };
    let Ok((mut draft, catalog)) = state.single_mut() else {
        return;
    };
    if draft.starting
        || keys
            .get(input.event().webview)
            .is_ok_and(|pending| pending.0)
    {
        return;
    }
    let agent_id = &input.event().payload.agent_id;
    if !catalog.0.iter().any(|agent| agent.id == *agent_id) {
        return;
    }
    let previous = sessions.iter().find(|(id, agent)| {
        id.0 == draft.session_id && agent.is_some_and(|agent| agent.0 == *agent_id)
    });
    draft.agent_id.clone_from(agent_id);
    draft.paused = false;
    draft.failed = false;
    draft.dirty = true;
    requests
        .commands
        .entity(parent.parent())
        .remove::<(vmux_ecs::PendingPrompt, vmux_ecs::PendingPromptAttachments)>();
    if let Some((id, _)) = previous {
        requests.opens.write(vmux_ecs::PageOpenRequest {
            target: PageOpenTarget::Stack(parent.parent()),
            url: crate::Route::Session(id.clone()).url(),
            request_id: None,
        });
        return;
    }
    draft.session_id.clear();
    let cwd = match projects.path() {
        Ok(path) => path.to_path_buf(),
        Err(error) => {
            bevy::log::warn!("agent setup cannot start: {error}");
            draft.failed = true;
            return;
        }
    };
    let request = StartRequest::for_setup(
        cwd,
        crate::AgentId(agent_id.clone()),
        PageOpenTarget::Stack(parent.parent()),
    );
    draft.session_id.clone_from(&request.id().0);
    draft.starting = true;
    requests.starts.write(request);
}

fn created(event: On<crate::Created>, mut drafts: Query<&mut Draft>) {
    let Ok(mut draft) = drafts.single_mut() else {
        return;
    };
    if draft.session_id == event.event().id.0 {
        draft.starting = false;
    }
}

fn cancelled(
    input: On<UiInput<crate::onboarding::ChatCancelSetup>>,
    views: super::SessionViews,
    sessions: Query<&crate::SessionId, With<crate::Session>>,
    mut drafts: Query<&mut Draft>,
) {
    let Some(session) = views.session(input.event().webview) else {
        return;
    };
    let Ok(id) = sessions.get(session) else {
        return;
    };
    let Ok(mut draft) = drafts.single_mut() else {
        return;
    };
    if draft.session_id == id.0 {
        draft.paused = true;
        draft.dirty = true;
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
        agent_id: draft.agent_id.clone(),
        starting: draft.starting,
        session_id: draft.session_id.clone(),
        failed: draft.failed,
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
        Err(error) => bevy::log::warn!("agent setup could not be saved: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_ecs::profile::{ProjectsDirectory, ProjectsRoot};

    #[test]
    fn first_launch_enters_setup_after_restore_and_later_tabs_keep_the_launcher() {
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
        for parent in [stack, warm] {
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
        assert_eq!(opens[0].url, "vmux://sessions/inbox?setup=1");
        assert!(matches!(opens[0].target, PageOpenTarget::Stack(target) if target == stack));
        let later = app
            .world_mut()
            .spawn((
                vmux_ecs::PageNavigation::new(vmux_ecs::PageOpenId::new()),
                vmux_ecs::PageMetadata {
                    url: "vmux://start/".into(),
                    ..Default::default()
                },
            ))
            .id();
        app.world_mut().spawn((
            vmux_ecs::PageMetadata {
                url: "vmux://start/".into(),
                ..Default::default()
            },
            vmux_ecs::page::HostsPage,
            ChildOf(later),
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
    fn readiness_shortcut_requires_an_attached_runtime() {
        for attached in [false, true] {
            let mut app = App::new();
            app.add_plugins(Plugin);
            let session = app
                .world_mut()
                .spawn((
                    crate::Session,
                    crate::SessionId("existing".into()),
                    crate::AgentId("agent".into()),
                    crate::RunState::Idle,
                ))
                .id();
            if attached {
                app.world_mut()
                    .entity_mut(session)
                    .insert(vmux_ecs::ProcessAnchor(vmux_ecs::ProcessId::new()));
            }
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
            app.world_mut().spawn((
                vmux_ecs::PageMetadata {
                    url: "vmux://start/".into(),
                    ..Default::default()
                },
                vmux_ecs::page::HostsPage,
                ChildOf(stack),
            ));
            app.update();
            let opens: Vec<_> = app
                .world_mut()
                .resource_mut::<Messages<vmux_ecs::PageOpenRequest>>()
                .drain()
                .collect();
            assert_eq!(opens.len(), 1);
            assert_eq!(
                opens[0].url,
                if attached {
                    "vmux://sessions/existing"
                } else {
                    "vmux://sessions/inbox?setup=1"
                }
            );
        }
    }

    #[test]
    fn agent_selection_starts_once_without_task_or_project_input() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.world_mut().spawn(ProjectsRoot(
            ProjectsDirectory::ensure_at(directory.path().to_path_buf()).unwrap(),
        ));
        app.update();
        let stack = app
            .world_mut()
            .spawn((
                vmux_ecs::PendingPrompt("Build something new".into()),
                vmux_ecs::PendingPromptAttachments::default(),
            ))
            .id();
        let view = app
            .world_mut()
            .spawn((SessionManagerView, ChildOf(stack)))
            .id();
        let unrelated = app.world_mut().spawn_empty().id();
        app.world_mut()
            .write_message(ProvidersChanged(vec![AgentOption {
                id: "agent".into(),
                name: "Agent".into(),
                ..Default::default()
            }]));
        app.update();
        app.world_mut().trigger(UiInput {
            webview: unrelated,
            payload: OnboardingConnect {
                agent_id: "agent".into(),
            },
        });
        app.world_mut().flush();
        assert_eq!(app.world().resource::<Messages<StartRequest>>().len(), 0);
        app.world_mut()
            .get_mut::<crate::onboarding::KeyPending>(view)
            .unwrap()
            .0 = true;
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: OnboardingConnect {
                agent_id: "agent".into(),
            },
        });
        app.world_mut().flush();
        assert_eq!(app.world().resource::<Messages<StartRequest>>().len(), 0);
        app.world_mut()
            .get_mut::<crate::onboarding::KeyPending>(view)
            .unwrap()
            .0 = false;
        for _ in 0..2 {
            app.world_mut().trigger(UiInput {
                webview: view,
                payload: OnboardingConnect {
                    agent_id: "agent".into(),
                },
            });
        }
        app.world_mut().flush();
        assert_eq!(app.world().resource::<Messages<StartRequest>>().len(), 1);
        assert!(app.world().get::<vmux_ecs::PendingPrompt>(stack).is_none());
        assert!(
            app.world()
                .get::<vmux_ecs::PendingPromptAttachments>(stack)
                .is_none()
        );
    }

    #[test]
    fn saved_setup_reopens_its_session_without_creating_a_duplicate() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("onboarding.json");
        Draft {
            agent_id: "agent".into(),
            session_id: "saved".into(),
            ..Default::default()
        }
        .save(&path)
        .unwrap();
        let restored = Draft::load(&path);
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.update();
        let owner = app
            .world_mut()
            .query_filtered::<Entity, With<Draft>>()
            .single(app.world())
            .unwrap();
        app.world_mut().entity_mut(owner).insert(restored);
        app.world_mut().spawn((
            crate::Session,
            crate::SessionId("saved".into()),
            crate::AgentId("agent".into()),
        ));
        app.world_mut()
            .write_message(ProvidersChanged(vec![AgentOption {
                id: "agent".into(),
                ..Default::default()
            }]));
        app.update();
        let stack = app.world_mut().spawn_empty().id();
        let view = app
            .world_mut()
            .spawn((SessionManagerView, ChildOf(stack)))
            .id();
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: OnboardingConnect {
                agent_id: "agent".into(),
            },
        });
        app.world_mut().flush();
        assert_eq!(app.world().resource::<Messages<StartRequest>>().len(), 0);
        let opens: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_ecs::PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "vmux://sessions/saved");
    }

    #[test]
    fn task_first_records_keep_the_agent_but_do_not_resume_the_work_session() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("onboarding.json");
        std::fs::write(&path, r#"{"agent_id":"agent","session_id":"old-work","text":"Build something new","step":"Workspace"}"#).unwrap();
        let migrated = Draft::load(&path);
        assert_eq!(migrated.agent_id, "agent");
        assert!(migrated.session_id.is_empty());
        migrated.save(&path).unwrap();
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert!(stored.get("text").is_none());
        assert!(stored.get("cwd").is_none());
        assert!(stored.get("step").is_none());
    }

    #[test]
    fn cancellation_returns_to_selection_and_preserves_the_session_for_explicit_retry() {
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.update();
        let owner = app
            .world_mut()
            .query_filtered::<Entity, With<Draft>>()
            .single(app.world())
            .unwrap();
        app.world_mut().entity_mut(owner).insert(Draft {
            agent_id: "agent".into(),
            session_id: "saved".into(),
            ..Default::default()
        });
        let session = app
            .world_mut()
            .spawn((
                crate::Session,
                crate::SessionId("saved".into()),
                crate::AgentId("agent".into()),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_ecs::EntityTarget::<crate::Session>::new(session),
                vmux_ecs::PageNavigation::new(vmux_ecs::PageOpenId::new()),
                vmux_ecs::PageMetadata {
                    url: "vmux://start/".into(),
                    ..Default::default()
                },
            ))
            .id();
        let view = app
            .world_mut()
            .spawn((
                ChildOf(stack),
                vmux_ecs::page::HostsPage,
                vmux_ecs::PageMetadata {
                    url: "vmux://start/".into(),
                    ..Default::default()
                },
            ))
            .id();
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: crate::onboarding::ChatCancelSetup,
        });
        app.world_mut()
            .write_message(ProvidersChanged(vec![AgentOption {
                id: "agent".into(),
                ..Default::default()
            }]));
        app.update();
        let opens: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_ecs::PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "vmux://sessions/inbox?setup=1");
        app.world_mut().entity_mut(view).insert(SessionManagerView);
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: OnboardingConnect {
                agent_id: "agent".into(),
            },
        });
        app.world_mut().flush();
        assert_eq!(app.world().resource::<Messages<StartRequest>>().len(), 0);
        let opens: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_ecs::PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "vmux://sessions/saved");
    }
}
