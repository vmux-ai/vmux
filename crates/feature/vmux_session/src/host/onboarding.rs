use bevy_app::{App, PostUpdate, Startup, Update};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use vmux_ecs::profile::{CurrentProfile, Projects};
use vmux_ecs::{PageOpenTarget, UiStatePlugin, UiStateWriter};

use super::StartRequest;
use crate::onboarding::{AgentOption, OnboardingChooseAgent, OnboardingConnect, OnboardingUiState};

pub(super) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiStatePlugin::<OnboardingUiState>::default())
            .add_plugins(UiStatePlugin::<crate::onboarding::KeyEntryUiState>::default())
            .add_plugins(UiEventPlugin::<(OnboardingConnect, OnboardingChooseAgent)>::default())
            .add_message::<ProvidersChanged>()
            .add_message::<StartRequest>()
            .add_message::<vmux_ecs::PageOpenRequest>()
            .add_systems(Startup, load)
            .add_systems(
                Update,
                sync_views.after(vmux_ecs::PageOpenSet::HandleKnownPages),
            )
            .add_observer(connect)
            .add_observer(choose_agent)
            .add_observer(created)
            .add_observer(cancelled)
            .add_systems(PostUpdate, (begin, providers, publish, save).chain());
    }
}

#[derive(Component)]
#[require(crate::onboarding::KeyPending)]
pub struct SetupView;

#[derive(Component)]
pub struct SetupTarget;

fn sync_views(
    mut views: Query<
        (
            Entity,
            &mut vmux_ecs::PageMetadata,
            &ChildOf,
            Has<SetupView>,
            Has<super::SessionView>,
        ),
        With<vmux_ecs::page::HostsPage>,
    >,
    drafts: Query<&Draft>,
    sessions: Query<
        (
            Entity,
            &crate::SessionId,
            Option<&crate::AgentId>,
            Has<crate::SetupPending>,
            Option<&crate::RunState>,
            Has<vmux_ecs::ProcessAnchor>,
        ),
        With<crate::Session>,
    >,
    targets: Query<(&vmux_ecs::EntityTarget<crate::Session>, Has<SetupTarget>)>,
    mut commands: Commands,
) {
    let draft = drafts.single().ok();
    let desired = draft.and_then(|draft| {
        if !draft.setup_started || draft.session_id.is_empty() {
            return None;
        }
        sessions.iter().find(|(_, id, agent, ..)| {
            id.0 == draft.session_id && agent.is_some_and(|agent| agent.0 == draft.agent_id)
        })
    });
    for (view, mut metadata, parent, setup, session_view) in &mut views {
        if crate::Route::parse(&metadata.url) != Some(crate::Route::Setup) {
            if setup {
                commands.entity(view).remove::<SetupView>();
            }
            continue;
        }
        if !setup {
            commands.entity(view).insert(SetupView);
        }
        let stack = parent.parent();
        let desired = desired.filter(|(session, ..)| {
            draft.is_some_and(|draft| !draft.paused)
                || (session_view
                    && targets
                        .get(stack)
                        .is_ok_and(|(target, setup)| setup && target.entity() == *session))
        });
        if let Some((session, id, _, pending, state, attached)) = desired {
            let bound = targets
                .get(stack)
                .is_ok_and(|(target, setup)| setup && target.entity() == session);
            if bound
                && session_view
                && !pending
                && attached
                && matches!(
                    state,
                    Some(
                        crate::RunState::Idle
                            | crate::RunState::Streaming
                            | crate::RunState::AwaitingApproval { .. }
                    )
                )
            {
                metadata.url = crate::Route::Session(id.clone()).url();
                commands.entity(view).remove::<SetupView>();
                commands.entity(stack).remove::<SetupTarget>();
                commands.trigger(UiInput {
                    webview: view,
                    payload: vmux_api::address::PageAddressPublish {
                        address: vmux_api::address::PageAddress {
                            url: metadata.url.clone(),
                            current: vmux_api::address::AddressOption {
                                label_key: "sessions-title".into(),
                                icon: Some(vmux_api::BuiltinIcon::MessagesRound),
                                intent: Some(vmux_api::address::AddressIntent::Navigate {
                                    url: metadata.url.clone(),
                                }),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    },
                });
                continue;
            }
            if !bound {
                commands
                    .entity(view)
                    .remove::<(super::SessionView, super::ChatSynced)>();
                commands.trigger(
                    vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                        view,
                        &crate::state::ChatUiStatePatch {
                            snapshot: Some(Box::new(crate::event::ChatSnapshot {
                                setup_pending: true,
                                status: "preparing".into(),
                                ..Default::default()
                            })),
                            transcript: Some(
                                Box::new(crate::event::ChatTranscriptState::default()),
                            ),
                            composer_draft: Some(crate::event::ChatComposerDraft::default()),
                            ..Default::default()
                        },
                    ),
                );
                commands.entity(stack).insert((
                    vmux_ecs::EntityTarget::<crate::Session>::new(session),
                    SetupTarget,
                ));
                commands.write_message(vmux_ecs::PageOpenRequest {
                    target: PageOpenTarget::Stack(stack),
                    url: crate::Route::Setup.url(),
                    request_id: None,
                });
            }
        } else {
            commands
                .entity(stack)
                .remove::<(vmux_ecs::EntityTarget<crate::Session>, SetupTarget)>();
            if session_view {
                commands.entity(view).remove::<super::SessionView>();
            }
        }
        let title = vmux_ui::i18n::translate("onboarding-setup");
        if metadata.title != title {
            metadata.title = title;
        }
        if setup && !metadata.is_changed() {
            continue;
        }
        commands.trigger(UiInput {
            webview: view,
            payload: vmux_api::address::PageAddressPublish {
                address: vmux_api::address::PageAddress {
                    url: metadata.url.clone(),
                    current: vmux_api::address::AddressOption {
                        label_key: "onboarding-setup".into(),
                        icon: Some(vmux_api::BuiltinIcon::MessagesRound),
                        intent: Some(vmux_api::address::AddressIntent::Navigate {
                            url: crate::Route::Setup.url(),
                        }),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
        });
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
    #[serde(default)]
    setup_started: bool,
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
            setup_started: false,
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
                url: "vmux://start/onboarding/agent".into(),
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
        Entity,
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
    pending: Query<(), With<crate::SetupPending>>,
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
    let mut recorded_setup = None;
    for (session, id, agent, state, attached, activated) in &sessions {
        if !draft.paused
            && !draft.session_id.is_empty()
            && draft.session_id == id.0
            && agent.is_some_and(|agent| draft.agent_id.is_empty() || agent.0 == draft.agent_id)
        {
            recorded = Some(id.clone());
            if pending.contains(session)
                || !attached
                || !matches!(
                    state,
                    Some(
                        crate::RunState::Idle
                            | crate::RunState::Streaming
                            | crate::RunState::AwaitingApproval { .. }
                    )
                )
            {
                recorded_setup = Some(session);
            }
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
    let Some(selected) = selected else {
        return;
    };
    let url = if let Some(session) = recorded_setup {
        commands.entity(stack).insert((
            vmux_ecs::EntityTarget::<crate::Session>::new(session),
            SetupTarget,
        ));
        commands.entity(session).insert(crate::SetupPending);
        crate::Route::Setup.url()
    } else {
        crate::Route::Session(selected).url()
    };
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
    views: Query<
        (
            &ChildOf,
            Has<super::SessionView>,
            Option<&crate::onboarding::KeyPending>,
        ),
        With<SetupView>,
    >,
    targets: Query<(), With<SetupTarget>>,
    mut state: Query<(&mut Draft, &Catalog)>,
    projects: Projects,
    mut requests: ConversationRequests,
    sessions: Query<(Entity, &crate::SessionId, Option<&crate::AgentId>), With<crate::Session>>,
) {
    let Ok((parent, session_view, key_pending)) = views.get(input.event().webview) else {
        return;
    };
    if session_view || targets.contains(parent.parent()) {
        return;
    }
    let Ok((mut draft, catalog)) = state.single_mut() else {
        return;
    };
    if draft.starting || key_pending.is_some_and(|pending| pending.0) {
        return;
    }
    let agent_id = &input.event().payload.agent_id;
    if !catalog.0.iter().any(|agent| agent.id == *agent_id) {
        return;
    }
    let previous = sessions.iter().find(|(_, id, agent)| {
        id.0 == draft.session_id && agent.is_some_and(|agent| agent.0 == *agent_id)
    });
    draft.setup_started = true;
    draft.agent_id.clone_from(agent_id);
    draft.paused = false;
    draft.failed = false;
    draft.dirty = true;
    requests
        .commands
        .entity(parent.parent())
        .remove::<(vmux_ecs::PendingPrompt, vmux_ecs::PendingPromptAttachments)>();
    if let Some((session, _, _)) = previous {
        requests.commands.entity(parent.parent()).insert((
            vmux_ecs::EntityTarget::<crate::Session>::new(session),
            SetupTarget,
        ));
        requests.opens.write(vmux_ecs::PageOpenRequest {
            target: PageOpenTarget::Stack(parent.parent()),
            url: crate::Route::Setup.url(),
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

fn choose_agent(
    input: On<UiInput<OnboardingChooseAgent>>,
    parents: Query<&ChildOf, (With<SetupView>, With<super::SessionView>)>,
    targets: Query<&vmux_ecs::EntityTarget<crate::Session>, With<SetupTarget>>,
    sessions: Query<(&crate::SessionId, &crate::AgentId), With<crate::Session>>,
    mut drafts: Query<&mut Draft>,
    mut commands: Commands,
) {
    let view = input.event().webview;
    let Ok(parent) = parents.get(view) else {
        return;
    };
    let Ok(target) = targets.get(parent.parent()) else {
        return;
    };
    let Ok((id, agent)) = sessions.get(target.entity()) else {
        return;
    };
    let Ok(mut draft) = drafts.single_mut() else {
        return;
    };
    if draft.session_id != id.0 || draft.agent_id != agent.0 {
        return;
    }
    draft.paused = true;
    draft.dirty = true;
    commands.trigger(UiInput {
        webview: view,
        payload: crate::onboarding::ChatCancelSetup,
    });
    commands
        .entity(parent.parent())
        .remove::<(SetupTarget, vmux_ecs::EntityTarget<crate::Session>)>();
    commands
        .entity(view)
        .remove::<(super::SessionView, super::ChatSynced)>();
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
    views: Query<
        (
            Entity,
            &vmux_ecs::PageMetadata,
            &ChildOf,
            Has<super::SessionView>,
        ),
        With<SetupView>,
    >,
    targets: Query<(&vmux_ecs::EntityTarget<crate::Session>, Has<SetupTarget>)>,
    sessions: Query<&crate::SessionId, With<crate::Session>>,
    mut writer: UiStateWriter<OnboardingUiState>,
    mut published: Local<std::collections::HashMap<Entity, (String, OnboardingUiState)>>,
) {
    let Ok((draft, catalog)) = state.single() else {
        return;
    };
    let mut next = OnboardingUiState {
        agents: catalog.0.clone(),
        agent_id: draft.agent_id.clone(),
        starting: draft.starting,
        session_id: draft.session_id.clone(),
        failed: draft.failed,
    };
    published.retain(|view, _| views.contains(*view));
    for (view, metadata, parent, session_view) in &views {
        next.starting = draft.starting
            || (!session_view && targets.get(parent.parent()).is_ok_and(|(_, setup)| setup));
        next.session_id = if session_view {
            targets
                .get(parent.parent())
                .ok()
                .and_then(|(target, setup)| {
                    if !setup {
                        return None;
                    }
                    let id = sessions.get(target.entity()).ok()?;
                    (id.0 == draft.session_id).then(|| id.0.clone())
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
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

    struct Setup {
        app: App,
        owner: Entity,
        stack: Entity,
        view: Entity,
        session: Entity,
    }

    impl Setup {
        fn delivered(event: On<bevy_cef::prelude::BinHostEmitEvent>) {
            event.event().mark_delivered();
        }

        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(Plugin)
                .add_observer(Self::delivered)
                .insert_non_send(bevy_cef::prelude::Browsers::default());
            app.world_mut().spawn(vmux_api::UiEventPermissions {
                url: crate::Route::SETUP_URL,
                owns_subtree: false,
                permissions: &["OnboardingUiState"],
            });
            app.update();
            let owner = app
                .world_mut()
                .query_filtered::<Entity, With<Draft>>()
                .single(app.world())
                .unwrap();
            app.world_mut().entity_mut(owner).insert(Draft {
                agent_id: "chosen".into(),
                session_id: "setup-session".into(),
                setup_started: true,
                ..Default::default()
            });
            let session = app
                .world_mut()
                .spawn((
                    crate::Session,
                    crate::SessionId("setup-session".into()),
                    crate::AgentId("chosen".into()),
                    crate::SetupPending,
                    crate::RunState::Errored("cancelled".into()),
                ))
                .id();
            let stack = app
                .world_mut()
                .spawn((
                    vmux_ecs::EntityTarget::<crate::Session>::new(session),
                    SetupTarget,
                ))
                .id();
            let view = app
                .world_mut()
                .spawn((
                    vmux_ecs::page::HostsPage,
                    SetupView,
                    super::super::SessionView,
                    ChildOf(stack),
                    vmux_ecs::PageMetadata {
                        url: crate::Route::Setup.url(),
                        ..Default::default()
                    },
                ))
                .id();
            app.world_mut().entity_mut(view).insert((
                bevy_cef::prelude::BinIpcSurfaceGeneration(1),
                bevy_cef::prelude::BinIpcNavigationRevision(1),
                bevy_cef::prelude::BinIpcEpoch(1),
            ));
            app.world_mut()
                .non_send_mut::<bevy_cef::prelude::Browsers>()
                .set_externally_hosted(view);
            app.update();
            Self {
                app,
                owner,
                stack,
                view,
                session,
            }
        }
    }

    #[test]
    fn stopping_setup_keeps_its_explicit_conversation_and_retry_target() {
        let mut setup = Setup::new();
        setup.app.world_mut().trigger(UiInput {
            webview: setup.view,
            payload: crate::onboarding::ChatCancelSetup,
        });
        setup.app.update();
        assert!(setup.app.world().get::<Draft>(setup.owner).unwrap().paused);
        assert_eq!(
            setup
                .app
                .world()
                .get::<vmux_ecs::EntityTarget<crate::Session>>(setup.stack)
                .unwrap()
                .entity(),
            setup.session
        );
        assert!(
            setup
                .app
                .world()
                .get::<super::super::SessionView>(setup.view)
                .is_some()
        );
        let state = setup
            .app
            .world()
            .get::<vmux_ecs::UiState<OnboardingUiState>>(setup.view)
            .unwrap()
            .current()
            .unwrap();
        assert_eq!(state.session_id, "setup-session");
    }

    #[test]
    fn choosing_another_agent_returns_to_selection_without_rebinding_the_paused_session() {
        let mut setup = Setup::new();
        setup.app.world_mut().trigger(UiInput {
            webview: setup.view,
            payload: OnboardingChooseAgent,
        });
        setup.app.update();
        assert!(setup.app.world().get::<Draft>(setup.owner).unwrap().paused);
        assert!(
            setup
                .app
                .world()
                .get::<vmux_ecs::EntityTarget<crate::Session>>(setup.stack)
                .is_none()
        );
        assert!(
            setup
                .app
                .world()
                .get::<super::super::SessionView>(setup.view)
                .is_none()
        );
        setup.app.update();
        let state = setup
            .app
            .world()
            .get::<vmux_ecs::UiState<OnboardingUiState>>(setup.view)
            .unwrap()
            .current()
            .unwrap();
        assert!(state.session_id.is_empty());
        assert!(!state.starting);
        setup.app.update();
        assert!(
            setup
                .app
                .world()
                .get::<vmux_ecs::EntityTarget<crate::Session>>(setup.stack)
                .is_none()
        );
    }

    #[test]
    fn a_ready_saved_setup_is_bound_before_its_url_becomes_a_work_session() {
        let mut setup = Setup::new();
        let other = setup
            .app
            .world_mut()
            .spawn((
                crate::Session,
                crate::SessionId("other".into()),
                crate::AgentId("other".into()),
            ))
            .id();
        setup
            .app
            .world_mut()
            .entity_mut(setup.stack)
            .insert(vmux_ecs::EntityTarget::<crate::Session>::new(other))
            .remove::<SetupTarget>();
        setup
            .app
            .world_mut()
            .entity_mut(setup.session)
            .remove::<crate::SetupPending>()
            .insert((
                crate::RunState::Idle,
                vmux_ecs::ProcessAnchor(vmux_ecs::ProcessId::new()),
            ));
        setup.app.update();
        assert_eq!(
            setup
                .app
                .world()
                .get::<vmux_ecs::PageMetadata>(setup.view)
                .unwrap()
                .url,
            crate::Route::Setup.url()
        );
        assert_eq!(
            setup
                .app
                .world()
                .get::<vmux_ecs::EntityTarget<crate::Session>>(setup.stack)
                .unwrap()
                .entity(),
            setup.session
        );
        assert!(
            setup
                .app
                .world()
                .get::<super::super::SessionView>(setup.view)
                .is_none()
        );
        setup
            .app
            .world_mut()
            .entity_mut(setup.view)
            .insert(super::super::SessionView);
        setup.app.update();
        assert_eq!(
            setup
                .app
                .world()
                .get::<vmux_ecs::PageMetadata>(setup.view)
                .unwrap()
                .url,
            "vmux://sessions/setup-session"
        );
        setup.app.update();
        assert!(setup.app.world().get::<SetupView>(setup.view).is_none());
    }

    #[test]
    fn setup_route_claims_reused_views_without_manager_authority() {
        let mut app = App::new();
        app.add_plugins(Plugin);
        let stack = app.world_mut().spawn_empty().id();
        let view = app
            .world_mut()
            .spawn((
                vmux_ecs::page::HostsPage,
                ChildOf(stack),
                vmux_ecs::PageMetadata {
                    url: crate::Route::Setup.url(),
                    title: "Sessions".into(),
                    ..Default::default()
                },
            ))
            .id();
        app.update();

        assert!(app.world().get::<SetupView>(view).is_some());
        assert!(
            app.world()
                .get::<super::super::SessionManagerView>(view)
                .is_none()
        );
        assert_eq!(
            app.world()
                .get::<vmux_ecs::PageMetadata>(view)
                .unwrap()
                .title,
            vmux_ui::i18n::translate("onboarding-setup")
        );

        app.world_mut()
            .entity_mut(view)
            .insert(vmux_ecs::PageMetadata {
                url: crate::Route::Manager.url(),
                ..Default::default()
            });
        app.update();
        assert!(app.world().get::<SetupView>(view).is_none());
    }

    #[test]
    fn existing_workspace_preserves_launcher_and_unfinished_setup_resumes_after_restart() {
        for (store_present, resume, expected) in
            [(true, false, 0), (true, true, 0), (false, false, 0)]
        {
            let mut app = App::new();
            app.add_plugins(Plugin);
            app.update();
            if resume {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("onboarding.json");
                Draft {
                    setup_started: true,
                    ..Default::default()
                }
                .save(&path)
                .unwrap();
                let owner = app
                    .world_mut()
                    .query_filtered::<Entity, With<Draft>>()
                    .single(app.world())
                    .unwrap();
                app.world_mut().entity_mut(owner).insert(Draft::load(&path));
            }
            app.world_mut().spawn(vmux_ecs::store::WorkspaceRestore {
                complete: true,
                store_present,
            });
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
                vmux_ecs::page::HostsPage,
                vmux_ecs::PageMetadata {
                    url: "vmux://start/".into(),
                    ..Default::default()
                },
                ChildOf(stack),
            ));
            app.update();
            let opens: Vec<_> = app
                .world_mut()
                .resource_mut::<Messages<vmux_ecs::PageOpenRequest>>()
                .drain()
                .collect();
            assert_eq!(opens.len(), expected);
            if let Some(open) = opens.first() {
                assert_eq!(open.url, "vmux://start/onboarding/agent");
            }
        }
    }

    #[test]
    fn first_launch_stays_on_start_until_explicit_setup_and_later_tabs_keep_the_launcher() {
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
        assert!(opens.is_empty());
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
            if attached {
                assert_eq!(opens.len(), 1);
                assert_eq!(opens[0].url, "vmux://sessions/existing");
            } else {
                assert!(opens.is_empty());
            }
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
        let view = app.world_mut().spawn((SetupView, ChildOf(stack))).id();
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
        let view = app.world_mut().spawn((SetupView, ChildOf(stack))).id();
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
        assert_eq!(opens[0].url, crate::Route::Setup.url());
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
    fn cancellation_keeps_start_and_preserves_the_session_for_explicit_retry() {
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
        assert!(opens.is_empty());
        app.world_mut().entity_mut(view).insert(SetupView);
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
        assert_eq!(opens[0].url, crate::Route::Setup.url());
    }
}
