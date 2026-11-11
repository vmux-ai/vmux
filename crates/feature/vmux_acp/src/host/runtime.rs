use bevy::prelude::*;
use vmux_api::protocol::{AcpSessionConfig, ClientMessage, SharedMessage};
#[cfg(test)]
use vmux_ecs::Cwd;
#[cfg(test)]
use vmux_ecs::ProcessId;
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::service::{ServiceMessageSet, ServiceRequest};
use vmux_ecs::team::Profile;
use vmux_ecs::{EntityTarget, PageMetadata, ProcessAnchor, ProcessPresentationRequest};
use vmux_git::worktree::ValidatedLinkedWorkspace;
use vmux_layout::tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_layout::worktree::TabWorktreeReady;

use super::handoff::PendingHandoff;
#[cfg(test)]
use super::runtime_driver::AcpWorkspaceState;
use super::runtime_driver::PromptWorkspace;
use crate::AcpSessionId;
use crate::host::acp::AcpSpawnQueued;
use crate::host::event::{
    AcpAgentInfo, AcpSessionConfigSelectionResult, AcpSessionConfigSnapshot, AcpSessionCreated,
    AcpTerminalCreated, AcpWorkspaceChanged,
};
use crate::policy::AcpWorkspacePolicy;
use vmux_session::host::{ImportedConversation, SessionView};
use vmux_session::{AgentId, QueuedPrompts, Route, RunState, Session, SessionId, TitlePending};

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct AcpSessionConfigSet;

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        crate::policy::add(app);
        crate::host::acp::add_registry(app);
        crate::host::acp::add(app);
        app.add_message::<ServiceRequest>()
            .add_message::<AcpAgentInfo>()
            .add_message::<AcpWorkspaceChanged>()
            .add_message::<AcpSessionConfigSnapshot>()
            .add_message::<AcpSessionConfigSelectionResult>()
            .add_message::<AcpSessionCreated>()
            .add_message::<AcpTerminalCreated>()
            .add_systems(
                Update,
                (
                    info,
                    workspace,
                    (config.in_set(AcpSessionConfigSet), selection).chain(),
                    session,
                    present_process,
                )
                    .after(ServiceMessageSet),
            )
            .add_systems(
                Update,
                input
                    .after(ServiceMessageSet)
                    .after(crate::host::model_selection::ModelSelectionSet)
                    .before(vmux_session::TranscriptReadySet),
            )
            .add_observer(close_on_remove);
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct AcpSessionConfigState {
    pub configs: Vec<AcpSessionConfig>,
    pub(super) pending: Vec<PendingAcpSessionConfig>,
    pub(super) initial: Vec<InitialAcpSessionConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingAcpSessionConfig {
    pub request_id: u64,
    pub config_id: Option<String>,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InitialAcpSessionConfig {
    pub(super) config_id: Option<String>,
    pub(super) value: String,
}

fn info(
    mut reader: MessageReader<AcpAgentInfo>,
    mut sessions: Query<(&SessionId, &AgentId, &mut Profile), With<Session>>,
) {
    for event in reader.read() {
        let name = event.name.trim();
        if name.is_empty() {
            continue;
        }
        for (session_id, agent_id, mut profile) in &mut sessions {
            if session_id.0 == event.sid && profile.name != name {
                let icon = profile.icon.clone();
                *profile = Profile::registry(name, &agent_id.0).with_icon(icon);
            }
        }
    }
}

fn ancestor_tab(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<(), With<Tab>>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if tabs.contains(current) {
            return Some(current);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

fn workspace(
    mut reader: MessageReader<AcpWorkspaceChanged>,
    sessions: Query<(Entity, &SessionId), With<Session>>,
    session_views: Query<(Entity, &EntityTarget<Session>)>,
    child_of: Query<&ChildOf>,
    tab_entities: Query<(), With<Tab>>,
    mut tabs: Query<&mut Tab>,
    mut workspaces: Query<&mut TabWorkspace>,
    managed: Query<&TabWorktree>,
    mut commands: Commands,
) {
    for event in reader.read() {
        let Ok(validated) = ValidatedLinkedWorkspace::new(
            std::path::Path::new(&event.cwd),
            std::path::Path::new(&event.workspace_cwd),
            &event.branch,
        ) else {
            bevy::log::warn!(sid = %event.sid, "ignored invalid ACP worktree metadata");
            continue;
        };
        let cwd = validated.cwd;
        let workspace_cwd = validated.workspace_cwd;
        let checkout = validated.checkout;
        for (session_entity, session_id) in &sessions {
            if session_id.0 != event.sid {
                continue;
            }
            commands.queue(vmux_session::mutation::Request::local(
                session_id.clone(),
                vmux_session::mutation::Mutation::ChangeWorkspace(cwd.clone()),
            ));
            let Some(stack) = session_views
                .iter()
                .find(|(_, target)| target.entity() == session_entity)
                .map(|(stack, _)| stack)
            else {
                continue;
            };
            let Some(tab_entity) = ancestor_tab(stack, &child_of, &tab_entities) else {
                continue;
            };
            if let Ok(mut tab) = tabs.get_mut(tab_entity) {
                tab.startup_dir = Some(cwd.to_string_lossy().into_owned());
            }
            let workspace_project_dir = workspace_cwd.to_string_lossy().into_owned();
            if let Ok(mut workspace) = workspaces.get_mut(tab_entity) {
                workspace.project_dir.clone_from(&workspace_project_dir);
            } else {
                commands.entity(tab_entity).insert(TabWorkspace {
                    project_dir: workspace_project_dir.clone(),
                });
            }
            let keeps_managed = managed.get(tab_entity).ok().is_some_and(|metadata| {
                metadata.branch == event.branch
                    && std::path::Path::new(&metadata.checkout_dir)
                        .canonicalize()
                        .ok()
                        .as_ref()
                        == Some(&checkout.root)
            });
            let mut entity = commands.entity(tab_entity);
            entity
                .insert(TabDirDecided)
                .remove::<TabWorktreeUnavailable>();
            if !keeps_managed {
                entity.remove::<TabWorktree>().remove::<TabWorktreeReady>();
            } else if let Ok(ready) = TabWorktreeReady::new(
                &cwd,
                &workspace_project_dir,
                managed.get(tab_entity).unwrap(),
                &checkout,
            ) {
                entity.insert(ready);
            } else {
                entity.remove::<TabWorktreeReady>();
            }
        }
    }
}

fn config(
    mut reader: MessageReader<AcpSessionConfigSnapshot>,
    mut sessions: Query<(Entity, &SessionId, Option<&mut AcpSessionConfigState>), With<Session>>,
    mut commands: Commands,
) {
    for event in reader.read() {
        for (entity, session_id, current) in &mut sessions {
            if session_id.0 != event.sid {
                continue;
            }
            if event.configs.is_empty() {
                if current.is_some() {
                    commands.entity(entity).remove::<AcpSessionConfigState>();
                }
                continue;
            }
            if let Some(mut current) = current {
                current.pending.retain(|pending| {
                    event.configs.iter().any(|config| {
                        config.config_id == pending.config_id
                            && config
                                .values
                                .iter()
                                .any(|option| option.value == pending.value)
                    })
                });
                for config in &event.configs {
                    if !current
                        .initial
                        .iter()
                        .any(|initial| initial.config_id == config.config_id)
                    {
                        current.initial.push(InitialAcpSessionConfig {
                            config_id: config.config_id.clone(),
                            value: config.current_value.clone(),
                        });
                    }
                }
                current.configs.clone_from(&event.configs);
            } else {
                let initial = event
                    .configs
                    .iter()
                    .map(|config| InitialAcpSessionConfig {
                        config_id: config.config_id.clone(),
                        value: config.current_value.clone(),
                    })
                    .collect();
                commands.entity(entity).insert(AcpSessionConfigState {
                    configs: event.configs.clone(),
                    pending: Vec::new(),
                    initial,
                });
            }
        }
    }
}

fn selection(
    mut reader: MessageReader<AcpSessionConfigSelectionResult>,
    mut sessions: Query<(&SessionId, &mut AcpSessionConfigState), With<Session>>,
) {
    for event in reader.read() {
        for (session_id, mut state) in &mut sessions {
            if session_id.0 != event.sid {
                continue;
            }
            let Some(index) = state.pending.iter().position(|pending| {
                pending.request_id == event.request_id
                    && pending.config_id == event.config_id
                    && pending.value == event.value
            }) else {
                continue;
            };
            state.pending.remove(index);
            if event.succeeded
                && let Some(config) = state
                    .configs
                    .iter_mut()
                    .find(|config| config.config_id == event.config_id)
            {
                config.current_value.clone_from(&event.value);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn session(
    mut reader: MessageReader<AcpSessionCreated>,
    sessions: Query<(Entity, &SessionId), With<Session>>,
    stacks: Query<(&EntityTarget<Session>, &Children)>,
    mut page_meta: Query<
        (&mut PageMetadata, Has<vmux_session::host::SetupView>),
        With<SessionView>,
    >,
    mut commands: Commands,
) {
    for ev in reader.read() {
        for (session_entity, session_id) in &sessions {
            if session_id.0 != ev.sid {
                continue;
            }
            commands
                .entity(session_entity)
                .insert(AcpSessionId(ev.acp_session_id.clone()));
            let url = Route::Session(session_id.clone()).url();
            for (target, kids) in &stacks {
                if target.entity() != session_entity {
                    continue;
                }
                for kid in kids.iter() {
                    if let Ok((mut meta, setup)) = page_meta.get_mut(kid)
                        && !setup
                        && meta.url != url
                    {
                        meta.url = url.clone();
                    }
                }
            }
        }
    }
}

fn present_process(
    mut reader: MessageReader<AcpTerminalCreated>,
    sessions: Query<(Entity, &SessionId), With<Session>>,
    mut commands: Commands,
) {
    for ev in reader.read() {
        let Some(session_entity) = sessions
            .iter()
            .find(|(_, session_id)| session_id.0 == ev.sid)
            .map(|(entity, _)| entity)
        else {
            continue;
        };
        commands.trigger(ProcessPresentationRequest {
            owner: session_entity,
            process_id: ev.process_id,
        });
    }
}

#[derive(Component)]
pub(super) struct PreferencesBlocked {
    file: Entity,
    error: String,
}

fn input(
    mut q: Query<(
        Entity,
        &SessionId,
        &AgentId,
        &mut RunState,
        Has<AcpSpawnQueued>,
        Has<TitlePending>,
        Option<&mut PendingHandoff>,
        Option<&mut ImportedConversation>,
        Option<&PreferencesBlocked>,
    )>,
    workspace: PromptWorkspace,
    policy: Single<&AcpWorkspacePolicy>,
    modes: crate::host::model_selection::AgentPreferences,
    mut service_requests: MessageWriter<ServiceRequest>,
    mut queues: QueuedPrompts,
    mut commands: Commands,
) {
    for (
        entity,
        session_id,
        agent_id,
        mut state,
        spawn_queued,
        title_pending,
        mut pending,
        mut imported,
        blocked,
    ) in &mut q
    {
        let kind = crate::host::model_selection::Kind::Modes;
        if let Some(blocked) = blocked {
            if modes.file(kind) != Some(blocked.file) {
                continue;
            }
            if modes.ready(kind) {
                if matches!(&*state, RunState::Errored(error) if *error == blocked.error) {
                    *state = RunState::Idle;
                }
                commands.entity(entity).remove::<PreferencesBlocked>();
            }
        }
        if !modes.ready(kind) {
            if spawn_queued
                && queues.ready(entity, matches!(*state, RunState::Idle))
                && let Some((file, error)) = modes.load_error(kind)
            {
                *state = RunState::Errored(error.to_string());
                commands.entity(entity).insert(PreferencesBlocked {
                    file,
                    error: error.to_string(),
                });
            }
            continue;
        }
        if !spawn_queued || !queues.ready(entity, matches!(*state, RunState::Idle)) {
            continue;
        }
        let Some((_, text, attachments)) = queues.take_next(entity) else {
            continue;
        };
        let handoff = pending.as_deref_mut().and_then(|pending| {
            if pending.sent {
                return None;
            }
            pending.sent = true;
            Some(pending.context.clone())
        });
        if handoff.is_some()
            && let Some(imported) = imported.as_deref_mut()
            && imported.first_prompt.is_none()
        {
            imported.first_prompt = Some(text.clone());
        }
        let workspace_state = workspace.state(entity);
        let context = PromptWorkspace::prompt(&policy, handoff, workspace_state, title_pending);
        let preferred_mode = modes
            .mode(&agent_id.0)
            .filter(|mode| !mode.is_empty())
            .map(str::to_string);
        service_requests.write(ServiceRequest(
            SharedMessage::AgentInput {
                sid: session_id.0.clone(),
                text,
                context,
                attachments,
                preferred_mode,
            }
            .into(),
        ));
        *state = RunState::Streaming;
    }
}

fn close_on_remove(
    trigger: On<Remove, ProcessAnchor>,
    sessions: Query<&SessionId, With<Session>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let Ok(session_id) = sessions.get(trigger.event_target()) else {
        return;
    };
    service_requests.write(ServiceRequest(ClientMessage::CloseAgentSession {
        sid: session_id.0.clone(),
    }));
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use vmux_api::protocol::AcpSessionConfigValue;

    use super::*;

    struct TestSession;

    impl TestSession {
        fn bundle(agent: &str, sid: &str, cwd: impl Into<std::path::PathBuf>) -> impl Bundle {
            (
                Session,
                SessionId(sid.into()),
                AgentId(agent.into()),
                Cwd(cwd.into()),
                ProcessAnchor(ProcessId::new()),
            )
        }
    }

    struct DispatchFixture {
        app: App,
        session: Entity,
        _directory: tempfile::TempDir,
    }

    impl DispatchFixture {
        fn new(state: RunState) -> Self {
            let mut app = App::new();
            let directory = tempfile::tempdir().unwrap();
            app.add_plugins((
                MinimalPlugins,
                vmux_session::QueuePlugin,
                vmux_session::preferences::Plugin,
                super::Plugin,
            ));
            app.world_mut()
                .write_message(vmux_session::preferences::ProfileRequested(
                    directory.path().to_path_buf(),
                ));
            let session = app
                .world_mut()
                .spawn((
                    TestSession::bundle("mock", "queue-session", "/tmp"),
                    vmux_session::PromptQueue::default(),
                    state,
                ))
                .id();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                app.update();
                if app
                    .world_mut()
                    .run_system_once(
                        |preferences: crate::host::model_selection::AgentPreferences| {
                            preferences.ready(crate::host::model_selection::Kind::Modes)
                        },
                    )
                    .unwrap()
                {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "dispatch preferences did not load"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Self {
                app,
                session,
                _directory: directory,
            }
        }

        fn enqueue(
            &mut self,
            session: Entity,
            text: &str,
            media: Vec<vmux_api::protocol::AgentAttachment>,
        ) {
            let text = text.to_string();
            self.app
                .world_mut()
                .run_system_once(move |mut queues: QueuedPrompts| {
                    queues.enqueue(session, text.clone(), media.clone(), false);
                })
                .unwrap();
        }

        fn requests(&mut self) -> Vec<ServiceRequest> {
            self.app
                .world_mut()
                .resource_mut::<Messages<ServiceRequest>>()
                .drain()
                .collect()
        }
    }

    #[test]
    fn queued_prompt_waits_for_acp_spawn_request() {
        let mut fixture = DispatchFixture::new(RunState::Idle);
        fixture.enqueue(fixture.session, "first", Vec::new());
        fixture.enqueue(fixture.session, "second", Vec::new());
        fixture.app.update();
        assert!(fixture.requests().is_empty());
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.session)
            .insert(AcpSpawnQueued);
        fixture.app.update();
        let requests = fixture.requests();
        assert!(
            matches!(requests.as_slice(), [ServiceRequest(ClientMessage::Shared(SharedMessage::AgentInput { sid, text, .. }))] if sid == "queue-session" && text == "first")
        );
        assert!(matches!(
            fixture.app.world().get::<RunState>(fixture.session),
            Some(RunState::Streaming)
        ));
        fixture.app.update();
        assert!(fixture.requests().is_empty());
        *fixture
            .app
            .world_mut()
            .get_mut::<RunState>(fixture.session)
            .unwrap() = RunState::Idle;
        fixture.app.update();
        let requests = fixture.requests();
        assert!(
            matches!(requests.as_slice(), [ServiceRequest(ClientMessage::Shared(SharedMessage::AgentInput { text, .. }))] if text == "second")
        );
    }

    #[test]
    fn refused_dispatch_retains_the_prompt_until_the_session_is_rearmed() {
        let mut fixture = DispatchFixture::new(RunState::Errored(
            "thread already has an active writer".into(),
        ));
        fixture.enqueue(fixture.session, "retained", Vec::new());
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.session)
            .insert(AcpSpawnQueued);
        fixture.app.update();
        assert!(fixture.requests().is_empty());
        *fixture
            .app
            .world_mut()
            .get_mut::<RunState>(fixture.session)
            .unwrap() = RunState::Idle;
        fixture.app.update();
        let requests = fixture.requests();
        assert!(
            matches!(requests.as_slice(), [ServiceRequest(ClientMessage::Shared(SharedMessage::AgentInput { text, .. }))] if text == "retained")
        );
    }

    #[test]
    fn flushed_dispatch_keeps_attachment_order_and_other_sessions_separate() {
        let mut fixture = DispatchFixture::new(RunState::Idle);
        let first = vmux_api::protocol::AgentAttachment {
            path: "/tmp/first.png".into(),
            name: "first".into(),
            mime_type: "image/png".into(),
            size: 1,
        };
        let second = vmux_api::protocol::AgentAttachment {
            path: "/tmp/second.png".into(),
            name: "second".into(),
            mime_type: "image/png".into(),
            size: 2,
        };
        fixture.enqueue(fixture.session, "first", vec![first.clone()]);
        fixture.enqueue(fixture.session, "second", vec![second.clone()]);
        let session = fixture.session;
        assert!(
            fixture
                .app
                .world_mut()
                .run_system_once(move |mut queues: QueuedPrompts| queues.request_flush(session))
                .unwrap()
        );
        fixture
            .app
            .world_mut()
            .entity_mut(session)
            .insert(AcpSpawnQueued);
        let other = fixture
            .app
            .world_mut()
            .spawn((
                TestSession::bundle("mock", "other", "/tmp"),
                vmux_session::PromptQueue::default(),
                RunState::Idle,
                AcpSpawnQueued,
            ))
            .id();
        fixture.enqueue(other, "third", Vec::new());
        fixture.app.update();
        let mut inputs = Vec::new();
        for request in fixture.requests() {
            let ServiceRequest(ClientMessage::Shared(SharedMessage::AgentInput {
                sid,
                text,
                attachments,
                ..
            })) = request
            else {
                panic!("unexpected provider request");
            };
            inputs.push((sid, text, attachments));
        }
        inputs.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            inputs,
            [
                ("other".into(), "third".into(), Vec::new()),
                (
                    "queue-session".into(),
                    "first\n\nsecond".into(),
                    vec![first, second]
                ),
            ]
        );
    }

    #[test]
    fn queued_prompts_wait_for_stored_modes_and_use_current_agent_preferences_once() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("agent-modes.json"),
            r#"{"one":{"selected":"auto","modes":[{"id":"auto","name":"Auto"},{"id":"ask","name":"Ask"}]},"two":{"selected":"ask","modes":[]}}"#,
        )
        .unwrap();
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.path().join(".agent-modes.json.lock"))
            .unwrap();
        lock.lock().unwrap();
        let mut app = App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            vmux_session::preferences::Plugin,
            vmux_session::QueuePlugin,
            super::Plugin,
        ));
        app.world_mut()
            .write_message(vmux_session::preferences::ProfileRequested(
                directory.path().to_path_buf(),
            ));
        let mut entities = Vec::new();
        for (agent, sid, text) in [("one", "s1", "first"), ("two", "s2", "second")] {
            let entity = app
                .world_mut()
                .spawn((
                    TestSession::bundle(agent, sid, directory.path()),
                    RunState::Idle,
                    AcpSpawnQueued,
                ))
                .id();
            let text = text.to_string();
            app.world_mut()
                .run_system_once(move |mut queues: QueuedPrompts| {
                    queues.enqueue(entity, text.clone(), Vec::new(), false)
                })
                .unwrap();
            entities.push(entity);
        }
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<ServiceRequest>>()
                .drain()
                .count(),
            0
        );
        for entity in &entities {
            assert_eq!(
                app.world_mut()
                    .run_system_once({
                        let entity = *entity;
                        move |rows: vmux_session::QueuedPromptReads| rows.visible(entity).len()
                    })
                    .unwrap(),
                1
            );
        }
        drop(lock);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut dispatched = Vec::new();
        loop {
            app.update();
            for ServiceRequest(request) in app
                .world_mut()
                .resource_mut::<Messages<ServiceRequest>>()
                .drain()
            {
                let ClientMessage::Shared(SharedMessage::AgentInput {
                    sid,
                    text,
                    preferred_mode,
                    ..
                }) = request
                else {
                    panic!("unexpected service request: {request:?}");
                };
                dispatched.push((sid, text, preferred_mode));
            }
            if dispatched.len() == 2 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "queued prompts did not dispatch"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        dispatched.sort();
        assert_eq!(
            dispatched,
            vec![
                ("s1".into(), "first".into(), Some("auto".into())),
                ("s2".into(), "second".into(), Some("ask".into())),
            ]
        );
        for entity in &entities {
            assert!(
                app.world_mut()
                    .run_system_once({
                        let entity = *entity;
                        move |rows: vmux_session::QueuedPromptReads| rows.visible(entity).is_empty()
                    })
                    .unwrap()
            );
            assert!(matches!(
                app.world().get::<RunState>(*entity),
                Some(RunState::Streaming)
            ));
        }
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<ServiceRequest>>()
                .drain()
                .count(),
            0
        );
        app.world_mut()
            .write_message(vmux_session::preferences::RememberMode {
                agent_id: "one".into(),
                mode_id: "ask".into(),
            });
        app.world_mut()
            .run_system_once({
                let entity = entities[0];
                move |mut queues: QueuedPrompts| {
                    queues.enqueue(entity, "next".into(), Vec::new(), false)
                }
            })
            .unwrap();
        app.world_mut()
            .entity_mut(entities[0])
            .insert(RunState::Idle);
        app.update();
        let requests = app
            .world_mut()
            .resource_mut::<Messages<ServiceRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert!(matches!(
            requests.as_slice(),
            [ServiceRequest(ClientMessage::Shared(SharedMessage::AgentInput {
                sid, text, preferred_mode: Some(mode), ..
            }))] if sid == "s1" && text == "next" && mode == "ask"
        ));
    }

    #[test]
    fn unbound_workspace_context_requires_project_selection_before_file_access() {
        let policy = crate::policy_driver::PolicyDriver::bundled();
        let context =
            PromptWorkspace::prompt(&policy, None, Some(AcpWorkspaceState::Unbound), false)
                .unwrap();

        assert!(context.contains("Before accessing project files"));
        assert!(context.contains("select_project"));
        assert!(context.contains("request_user_choice"));
        assert!(context.contains("explicit user approval"));
        assert!(context.contains("~/.vmux/projects/<remote-host>"));
        assert!(context.contains("~/.vmux/projects/local/<project>"));
        assert!(context.contains("create the empty directory"));
        assert!(context.contains("use the new project root directly"));
        assert!(context.contains("Do not search the user's home directory"));
        assert!(context.contains("open the picker"));
    }

    #[test]
    fn repository_context_defers_worktree_until_mutation() {
        let policy = crate::policy_driver::PolicyDriver::bundled();
        let context = PromptWorkspace::prompt(
            &policy,
            None,
            Some(AcpWorkspaceState::RepositoryNeedsWorktree),
            false,
        )
        .unwrap();

        assert!(context.contains("Reading and inspection are allowed"));
        assert!(context.contains("Never call create_worktree"));
        assert!(context.contains("Immediately before the first edit"));
        assert!(context.contains("create_worktree"));
        assert!(context.contains("request_user_choice"));
        assert!(context.contains("Never run git worktree add"));
    }

    #[test]
    fn pending_worktree_context_requires_waiting_for_activation() {
        let policy = crate::policy_driver::PolicyDriver::bundled();
        let context = PromptWorkspace::prompt(
            &policy,
            Some("prior conversation".into()),
            Some(AcpWorkspaceState::PendingWorktree),
            false,
        )
        .unwrap();

        assert!(context.starts_with("prior conversation\n\n"));
        assert!(context.contains("activation is pending"));
        assert!(context.contains("Wait for vmux"));
        assert!(context.contains("before inspecting"));
    }

    #[test]
    fn bound_workspace_keeps_only_handoff_context() {
        let policy = crate::policy_driver::PolicyDriver::bundled();
        assert_eq!(
            PromptWorkspace::prompt(
                &policy,
                Some("prior conversation".into()),
                Some(AcpWorkspaceState::Bound),
                false,
            )
            .as_deref(),
            Some("prior conversation")
        );
    }

    #[test]
    fn provisional_title_requires_model_written_summary() {
        let policy = crate::policy_driver::PolicyDriver::bundled();
        let context =
            PromptWorkspace::prompt(&policy, None, Some(AcpWorkspaceState::Bound), true).unwrap();

        assert!(context.contains("call set_conversation_title"));
        assert!(context.contains("3 to 7 word model-written title"));
        assert!(context.contains("one-sentence summary"));
        assert!(context.contains("Do not copy the user's prompt verbatim"));
    }

    #[test]
    fn ancestor_workspace_state_tracks_pending_and_bound_tab() {
        let mut app = App::new();
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "Tab 1".into(),
                startup_dir: None,
            })
            .id();
        let session = app.world_mut().spawn(Session).id();
        app.world_mut()
            .spawn((EntityTarget::<Session>::new(session), ChildOf(tab)));
        let state = |world: &mut World| {
            world
                .run_system_once(move |workspace: PromptWorkspace| workspace.state(session))
                .unwrap()
        };

        assert_eq!(state(app.world_mut()), Some(AcpWorkspaceState::Unbound));
        app.world_mut()
            .entity_mut(tab)
            .insert(vmux_space::PendingProject("/repo".into()));
        assert_eq!(
            state(app.world_mut()),
            Some(AcpWorkspaceState::PendingWorktree)
        );
        app.world_mut().entity_mut(tab).insert((
            Tab {
                name: "Tab 1".into(),
                startup_dir: Some("/repo".into()),
            },
            vmux_space::RepositoryNeedsWorktree,
        ));
        assert_eq!(
            state(app.world_mut()),
            Some(AcpWorkspaceState::RepositoryNeedsWorktree)
        );
        app.world_mut()
            .entity_mut(tab)
            .insert(TabWorkspace {
                project_dir: "/repo".into(),
            })
            .remove::<vmux_space::RepositoryNeedsWorktree>();
        assert_eq!(state(app.world_mut()), Some(AcpWorkspaceState::Bound));
    }

    #[test]
    fn acp_workspace_update_rebinds_only_matching_tab() {
        let repo = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .current_dir(repo.path())
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.path().join("seed.txt"), "seed\n").unwrap();
        git(&["add", "seed.txt"]);
        git(&["commit", "-qm", "init"]);
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree = worktree_parent.path().join("quiet-amber-wolf");
        vmux_git::worktree::CheckoutInfo::try_from(repo.path())
            .unwrap()
            .add_worktree(&worktree, "vibe/quiet-amber-wolf", "main")
            .unwrap();
        let project_dir = repo.path().canonicalize().unwrap();
        let worktree_dir = worktree.canonicalize().unwrap();
        let mut app = App::new();
        app.add_message::<crate::host::event::AcpWorkspaceChanged>()
            .add_systems(Update, workspace);
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "matching".into(),
                    startup_dir: Some(project_dir.to_string_lossy().into_owned()),
                },
                TabWorkspace {
                    project_dir: project_dir.to_string_lossy().into_owned(),
                },
            ))
            .id();
        let session = app
            .world_mut()
            .spawn(TestSession::bundle(
                "mistral-vibe",
                "matching-sid",
                project_dir.clone(),
            ))
            .id();
        app.world_mut()
            .spawn((EntityTarget::<Session>::new(session), ChildOf(tab)));
        let unrelated_tab = app
            .world_mut()
            .spawn(Tab {
                name: "unrelated".into(),
                startup_dir: Some(project_dir.to_string_lossy().into_owned()),
            })
            .id();
        app.world_mut()
            .resource_mut::<Messages<crate::host::event::AcpWorkspaceChanged>>()
            .write(crate::host::event::AcpWorkspaceChanged {
                sid: "matching-sid".into(),
                branch: "vibe/quiet-amber-wolf".into(),
                cwd: worktree_dir.to_string_lossy().into_owned(),
                workspace_cwd: project_dir.to_string_lossy().into_owned(),
            });

        app.update();

        assert_eq!(app.world().get::<Cwd>(session).unwrap().0, worktree_dir);
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(worktree_dir.to_string_lossy().as_ref())
        );
        assert_eq!(
            app.world()
                .get::<Tab>(unrelated_tab)
                .unwrap()
                .startup_dir
                .as_deref(),
            Some(project_dir.to_string_lossy().as_ref())
        );

        let background = app
            .world_mut()
            .spawn(TestSession::bundle(
                "mistral-vibe",
                "background-sid",
                project_dir.clone(),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<crate::host::event::AcpWorkspaceChanged>>()
            .write(crate::host::event::AcpWorkspaceChanged {
                sid: "background-sid".into(),
                branch: "vibe/quiet-amber-wolf".into(),
                cwd: worktree_dir.to_string_lossy().into_owned(),
                workspace_cwd: project_dir.to_string_lossy().into_owned(),
            });

        app.update();

        assert_eq!(app.world().get::<Cwd>(background).unwrap().0, worktree_dir);
    }

    #[test]
    fn live_acp_identity_updates_only_matching_profile() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_plugins(super::Plugin);
        let matching = app
            .world_mut()
            .spawn((
                TestSession::bundle("antigravity", "s1", "/tmp"),
                Profile::registry("Configured", "antigravity")
                    .with_icon(Some("https://cdn.example/antigravity.svg".into())),
            ))
            .id();
        let unrelated = app
            .world_mut()
            .spawn((
                TestSession::bundle("claude", "s2", "/tmp"),
                Profile::registry("Claude", "claude"),
            ))
            .id();

        app.world_mut().write_message(AcpAgentInfo {
            sid: "s1".into(),
            name: "Antigravity".into(),
        });
        app.update();

        assert_eq!(
            app.world().get::<Profile>(matching).unwrap().name,
            "Antigravity"
        );
        assert_eq!(
            app.world()
                .get::<Profile>(matching)
                .unwrap()
                .icon
                .as_deref(),
            Some("https://cdn.example/antigravity.svg")
        );
        assert_eq!(
            app.world().get::<Profile>(unrelated).unwrap().name,
            "Claude"
        );

        app.world_mut().write_message(AcpAgentInfo {
            sid: "s1".into(),
            name: "   ".into(),
        });
        app.update();

        assert_eq!(
            app.world().get::<Profile>(matching).unwrap().name,
            "Antigravity"
        );
    }

    #[test]
    fn live_acp_config_state_updates_only_matching_session() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_plugins(super::Plugin);
        let matching = app
            .world_mut()
            .spawn(TestSession::bundle("claude", "s1", "/tmp"))
            .id();
        let unrelated = app
            .world_mut()
            .spawn(TestSession::bundle("codex", "s2", "/tmp"))
            .id();

        app.world_mut().write_message(AcpSessionConfigSnapshot {
            sid: "s1".into(),
            configs: vec![AcpSessionConfig {
                config_id: Some("model".into()),
                name: "Model".into(),
                description: None,
                category: Some("model".into()),
                current_value: "sonnet".into(),
                values: vec![AcpSessionConfigValue {
                    value: "sonnet".into(),
                    name: "Claude Sonnet".into(),
                    description: None,
                    group: None,
                }],
            }],
        });
        app.update();

        let state = app.world().get::<AcpSessionConfigState>(matching).unwrap();
        let model = state.category("model").unwrap();
        assert_eq!(state.display_name(model), "Claude Sonnet");
        assert!(state.pending.is_empty());
        assert!(
            app.world()
                .get::<AcpSessionConfigState>(unrelated)
                .is_none()
        );
    }

    #[test]
    fn config_results_preserve_latest_pending_selection() {
        let values = ["default", "opus", "fable"]
            .into_iter()
            .map(|value| AcpSessionConfigValue {
                value: value.into(),
                name: value.into(),
                description: None,
                group: None,
            })
            .collect::<Vec<_>>();
        let mut app = App::new();
        app.add_message::<AcpSessionConfigSnapshot>()
            .add_message::<AcpSessionConfigSelectionResult>()
            .add_systems(Update, (config, selection).chain());
        let entity = app
            .world_mut()
            .spawn((
                TestSession::bundle("claude", "s1", "/tmp"),
                AcpSessionConfigState {
                    configs: vec![AcpSessionConfig {
                        config_id: Some("model".into()),
                        name: "Model".into(),
                        description: None,
                        category: Some("model".into()),
                        current_value: "default".into(),
                        values: values.clone(),
                    }],
                    pending: vec![PendingAcpSessionConfig {
                        request_id: 2,
                        config_id: Some("model".into()),
                        value: "fable".into(),
                    }],
                    initial: vec![InitialAcpSessionConfig {
                        config_id: Some("model".into()),
                        value: "default".into(),
                    }],
                },
            ))
            .id();

        app.world_mut().write_message(AcpSessionConfigSnapshot {
            sid: "s1".into(),
            configs: vec![AcpSessionConfig {
                config_id: Some("model".into()),
                name: "Model".into(),
                description: None,
                category: Some("model".into()),
                current_value: "opus".into(),
                values,
            }],
        });
        app.update();

        let state = app.world().get::<AcpSessionConfigState>(entity).unwrap();
        let model = state.category("model").unwrap();
        assert_eq!(model.current_value, "opus");
        assert_eq!(state.pending[0].request_id, 2);
        assert_eq!(state.display_name(model), "fable");

        app.world_mut()
            .write_message(AcpSessionConfigSelectionResult {
                sid: "s1".into(),
                request_id: 1,
                config_id: Some("model".into()),
                value: "fable".into(),
                succeeded: false,
            });
        app.update();
        assert_eq!(
            app.world()
                .get::<AcpSessionConfigState>(entity)
                .unwrap()
                .pending[0]
                .request_id,
            2
        );

        app.world_mut()
            .write_message(AcpSessionConfigSelectionResult {
                sid: "s1".into(),
                request_id: 2,
                config_id: Some("model".into()),
                value: "fable".into(),
                succeeded: false,
            });
        app.update();
        let state = app.world().get::<AcpSessionConfigState>(entity).unwrap();
        let model = state.category("model").unwrap();
        assert!(state.pending.is_empty());
        assert_eq!(state.display_name(model), "opus");

        {
            let mut state = app
                .world_mut()
                .get_mut::<AcpSessionConfigState>(entity)
                .unwrap();
            state.pending.push(PendingAcpSessionConfig {
                request_id: 3,
                config_id: Some("model".into()),
                value: "fable".into(),
            });
        }
        app.world_mut()
            .write_message(AcpSessionConfigSelectionResult {
                sid: "s1".into(),
                request_id: 3,
                config_id: Some("model".into()),
                value: "fable".into(),
                succeeded: true,
            });
        app.update();
        let state = app.world().get::<AcpSessionConfigState>(entity).unwrap();
        let model = state.category("model").unwrap();
        assert_eq!(model.current_value, "fable");
        assert!(state.pending.is_empty());
    }

    #[test]
    fn plugin_builds_and_runs_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_plugins(super::Plugin);
        app.world_mut()
            .spawn(TestSession::bundle("vibe-acp", "s1", "/tmp"));
        app.update();
    }

    #[test]
    fn mode_load_failure_is_visible_and_retry_dispatches_the_retained_prompt_once() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("agent-modes.json"), "{").unwrap();
        let mut app = App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            vmux_session::QueuePlugin,
            vmux_session::preferences::Plugin,
            super::Plugin,
        ));
        app.world_mut()
            .write_message(vmux_session::preferences::ProfileRequested(
                directory.path().to_path_buf(),
            ));
        let session = app
            .world_mut()
            .spawn((
                TestSession::bundle("one", "load-error", directory.path()),
                RunState::Idle,
                vmux_session::PromptQueue::default(),
                AcpSpawnQueued,
            ))
            .id();
        app.world_mut()
            .run_system_once(move |mut queues: QueuedPrompts| {
                queues.enqueue(session, "retained".into(), Vec::new(), false);
            })
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            app.update();
            if matches!(
                app.world().get::<RunState>(session),
                Some(RunState::Errored(_))
            ) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "mode loading failure did not reach session state"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            app.world_mut()
                .run_system_once(move |queues: vmux_session::QueuedPromptReads| {
                    queues.visible(session).len()
                })
                .unwrap(),
            1
        );
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<ServiceRequest>>()
                .drain()
                .count(),
            0
        );
        std::fs::write(
            directory.path().join("agent-modes.json"),
            r#"{"one":{"selected":"ask","modes":[]}}"#,
        )
        .unwrap();
        app.world_mut()
            .write_message(vmux_session::preferences::RetryPreferences {
                kind: vmux_session::preferences::Kind::Modes,
            });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let requests = loop {
            app.update();
            let requests = app
                .world_mut()
                .resource_mut::<Messages<ServiceRequest>>()
                .drain()
                .collect::<Vec<_>>();
            if !requests.is_empty() {
                break requests;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "retained prompt did not recover after preference retry"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert!(
            matches!(requests.as_slice(), [ServiceRequest(ClientMessage::Shared(SharedMessage::AgentInput { sid, text, preferred_mode: Some(mode), .. }))] if sid == "load-error" && text == "retained" && mode == "ask")
        );
        assert!(app.world().get::<PreferencesBlocked>(session).is_none());
        assert!(
            app.world_mut()
                .run_system_once(move |queues: QueuedPrompts| queues.empty(session))
                .unwrap()
        );
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<ServiceRequest>>()
                .drain()
                .count(),
            0
        );
    }
}
