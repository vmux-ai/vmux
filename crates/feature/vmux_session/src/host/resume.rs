use std::path::Path;

use bevy_app::{App, Plugin as BevyPlugin};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_api::chat::{
    PromptHistory, PromptHistoryRequest, ResumableSessionEntry, ResumableSessions,
    ResumeListRequest, ResumeSession,
};
use vmux_api::command_bar::UiCommandBar;
use vmux_ecs::team::Profile;
use vmux_ecs::{Cwd, Description, LastActivatedAt, PageOpenRequest, PageOpenTarget, UiStateWrite};
use vmux_git::RepoInfoCache;
use vmux_session::{
    AgentFamily, AgentId, ProviderCatalog, ProviderSessionTombstones, Route, Session,
    SessionDuplicate, SessionId, Transcripts,
};
#[cfg(test)]
use vmux_session::{ProviderKind, ProviderSessionId, ProviderSessionKey};

use crate::event::ChatResumeQueryRequest;
use crate::host::session::{ChatResumeProjection, SessionView};
use crate::state::UiChat;

pub(super) struct Plugin;

impl BevyPlugin for Plugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProviderCatalog>()
            .init_resource::<ProviderSessionTombstones>()
            .add_plugins(UiEventPlugin::<(
                PromptHistoryRequest,
                ResumeListRequest,
                ResumeSession,
                ChatResumeQueryRequest,
            )>::default())
            .add_observer(prompt_history)
            .add_observer(list)
            .add_observer(open)
            .add_observer(query);
    }
}

type Sessions<'w, 's> = Query<
    'w,
    's,
    (
        &'static SessionId,
        &'static Name,
        &'static Description,
        &'static Cwd,
        &'static LastActivatedAt,
        Option<&'static AgentId>,
        Option<&'static Profile>,
    ),
    (With<Session>, Without<SessionDuplicate>),
>;

#[derive(SystemParam)]
struct ResumeCatalog<'w, 's> {
    sessions: Sessions<'w, 's>,
    providers: Res<'w, ProviderCatalog>,
    tombstones: Res<'w, ProviderSessionTombstones>,
    cache: Option<Single<'w, 's, &'static mut RepoInfoCache>>,
}

impl ResumeCatalog<'_, '_> {
    fn page(&mut self, request_id: u64, query: &str, offset: u32) -> ResumableSessions {
        let query_lower = query.trim().to_ascii_lowercase();
        let now = vmux_ecs::UnixMillis::now().0;
        let mut sessions = Vec::new();
        for (id, name, description, cwd, activated, agent, profile) in &self.sessions {
            let path = cwd.0.as_path();
            let info = self.cache.as_mut().and_then(|cache| cache.snapshot(path));
            let subtitle = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| cwd.0.to_string_lossy().into_owned());
            let project = info
                .as_ref()
                .map(vmux_git::worktree::RepoInfo::project_name)
                .filter(|project| !project.is_empty())
                .unwrap_or_else(|| subtitle.clone());
            let branch = info.map(|info| info.branch).unwrap_or_default();
            let agent_name = agent
                .map(|agent| {
                    let fallback = profile
                        .map(|profile| profile.name.as_str())
                        .unwrap_or(agent.0.as_str());
                    AgentFamily::of(&agent.0).name(fallback)
                })
                .filter(|agent| !agent.is_empty())
                .unwrap_or_else(|| vmux_ui::i18n::translate("sessions-title"));
            let cwd = path.to_string_lossy().into_owned();
            if !query_lower.is_empty()
                && ![
                    name.as_str(),
                    description.0.as_str(),
                    cwd.as_str(),
                    project.as_str(),
                    branch.as_str(),
                    agent_name.as_str(),
                ]
                .iter()
                .any(|value| value.to_ascii_lowercase().contains(&query_lower))
            {
                continue;
            }
            let age_seconds = now.saturating_sub(activated.0.max(0)).max(0) as u64 / 1_000;
            let updated_at = chrono::DateTime::from_timestamp_millis(activated.0)
                .map(|timestamp| {
                    timestamp
                        .with_timezone(&chrono::Local)
                        .format("%Y-%m-%d")
                        .to_string()
                })
                .unwrap_or_default();
            sessions.push((
                activated.0,
                ResumableSessionEntry {
                    kind: "session".to_string(),
                    sid: id.0.clone(),
                    cwd,
                    url: Route::Session(id.clone()).url(),
                    title: name.as_str().to_string(),
                    latest: description.0.clone(),
                    subtitle,
                    age_seconds,
                    updated_at,
                    agent_name,
                    project,
                    branch,
                    cross_runtime: false,
                },
            ));
        }
        for session in &self.providers.sessions {
            if self.tombstones.contains(&session.kind, &session.sid) {
                continue;
            }
            let path = Path::new(&session.cwd);
            let subtitle = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| session.cwd.clone());
            let project = subtitle.clone();
            let branch = String::new();
            if !query_lower.is_empty()
                && ![
                    session.name.as_str(),
                    session.description.as_str(),
                    session.cwd.as_str(),
                    project.as_str(),
                    branch.as_str(),
                    session.agent.as_str(),
                    session.agent_name.as_str(),
                ]
                .iter()
                .any(|value| value.to_ascii_lowercase().contains(&query_lower))
            {
                continue;
            }
            let age_seconds = now.saturating_sub(session.updated_at.max(0)).max(0) as u64 / 1_000;
            let updated_at = chrono::DateTime::from_timestamp_millis(session.updated_at)
                .map(|timestamp| {
                    timestamp
                        .with_timezone(&chrono::Local)
                        .format("%Y-%m-%d")
                        .to_string()
                })
                .unwrap_or_default();
            sessions.push((
                session.updated_at,
                ResumableSessionEntry {
                    kind: session.kind.clone(),
                    sid: session.sid.clone(),
                    cwd: session.cwd.clone(),
                    url: String::new(),
                    title: session.name.clone(),
                    latest: session.description.clone(),
                    subtitle,
                    age_seconds,
                    updated_at,
                    agent_name: AgentFamily::of(&session.agent).name(&session.agent_name),
                    project,
                    branch,
                    cross_runtime: true,
                },
            ));
        }
        sessions.sort_by_key(|(activated, _)| std::cmp::Reverse(*activated));
        let total = sessions.len() as u32;
        let sessions = sessions
            .into_iter()
            .skip(offset as usize)
            .take(ResumableSessions::PAGE as usize)
            .map(|(_, session)| session)
            .collect();
        ResumableSessions {
            request_id,
            query: query.to_string(),
            sessions,
            offset,
            total,
        }
    }
}

fn prompt_history(
    trigger: On<UiInput<PromptHistoryRequest>>,
    sessions: Query<
        (Entity, &Cwd, &AgentId, &LastActivatedAt),
        (With<Session>, Without<SessionDuplicate>),
    >,
    transcripts: Transcripts,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let requested_cwd = Path::new(&request.cwd);
    let mut matching = sessions
        .iter()
        .filter(|(_, cwd, agent, _)| agent.0 == request.agent && cwd.0.as_path() == requested_cwd)
        .map(|(entity, _, _, activated)| (activated.0, entity))
        .collect::<Vec<_>>();
    matching.sort_by_key(|(activated, _)| *activated);
    let mut prompts = Vec::new();
    for (_, session) in matching {
        for message in transcripts.get(session).messages {
            let vmux_api::session::Message::User { text, .. } = message else {
                continue;
            };
            if !text.trim().is_empty() {
                prompts.push(text);
            }
        }
    }
    if prompts.len() > 100 {
        prompts.drain(..prompts.len() - 100);
    }
    commands.trigger(UiStateWrite::<UiCommandBar>::from_event(
        trigger.event().webview,
        &PromptHistory { prompts },
    ));
}

fn list(
    trigger: On<UiInput<ResumeListRequest>>,
    mut catalog: ResumeCatalog,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let response = catalog.page(request.request_id, &request.query, request.offset);
    commands.trigger(UiStateWrite::<UiCommandBar>::from_event(
        trigger.event().webview,
        &response,
    ));
}

fn open(
    trigger: On<UiInput<ResumeSession>>,
    parents: Query<&ChildOf>,
    sessions: Query<&SessionId, With<Session>>,
    mut opens: MessageWriter<PageOpenRequest>,
) {
    let request = &trigger.event().payload;
    if request.kind != "session" || !sessions.iter().any(|session| session.0 == request.sid) {
        return;
    }
    let Ok(parent) = parents.get(trigger.event().webview) else {
        return;
    };
    opens.write(PageOpenRequest {
        target: PageOpenTarget::Stack(parent.parent()),
        url: Route::Session(SessionId(request.sid.clone())).url(),
        request_id: None,
    });
}

fn query(
    trigger: On<UiInput<ChatResumeQueryRequest>>,
    mut catalog: ResumeCatalog,
    mut projections: Query<&mut ChatResumeProjection, With<SessionView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok(mut projection) = projections.get_mut(webview) else {
        return;
    };
    let Some(request_id) = projection.start(request.active, request.query.clone()) else {
        return;
    };
    if request.active {
        let response = catalog.page(request_id, &request.query, 0);
        projection.finish(&response);
    }
    commands.trigger(UiStateWrite::<UiChat>::from_event(webview, &projection.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::ChatResumeState;
    use vmux_api::command_bar::UiCommandBarPatch;

    #[derive(Resource, Default)]
    struct Responses {
        resumes: Vec<ResumableSessions>,
        prompt_histories: Vec<PromptHistory>,
        chat_resumes: Vec<ChatResumeState>,
    }

    impl Responses {
        fn command_bar(trigger: On<UiStateWrite<UiCommandBar>>, mut responses: ResMut<Responses>) {
            if let Some(resume) =
                <UiCommandBarPatch as vmux_api::UiStatePatch<ResumableSessions>>::payload(
                    trigger.event().update(),
                )
            {
                responses.resumes.push(resume.clone());
            }
            if let Some(history) =
                <UiCommandBarPatch as vmux_api::UiStatePatch<PromptHistory>>::payload(
                    trigger.event().update(),
                )
            {
                responses.prompt_histories.push(history.clone());
            }
        }

        fn chat(trigger: On<UiStateWrite<UiChat>>, mut responses: ResMut<Responses>) {
            if let Some(resume) = <crate::state::UiChatPatch as vmux_api::UiStatePatch<
                ChatResumeState,
            >>::payload(trigger.event().update())
            {
                responses.chat_resumes.push(resume.clone());
            }
        }
    }

    fn spawn_session(world: &mut World) -> Entity {
        world
            .spawn((
                Session,
                SessionId("session-1".into()),
                Name::new("Repair resume"),
                Description("Restore delegated session requests".into()),
                Cwd("/work/vmux".into()),
                LastActivatedAt(2),
                AgentId("codex".into()),
            ))
            .id()
    }

    #[test]
    fn delegated_start_requests_answer_from_the_local_session_catalog() {
        let mut app = App::new();
        app.init_resource::<Responses>()
            .add_message::<PageOpenRequest>()
            .add_plugins(Plugin)
            .add_observer(Responses::command_bar);
        let stack = app.world_mut().spawn_empty().id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();
        let session_id = SessionId("session-1".into());
        let session = spawn_session(app.world_mut());
        app.world_mut().spawn((
            vmux_session::ConversationEvent,
            vmux_session::EventIdentity {
                event_id: vmux_api::session::EventId::new("event-1"),
                actor_id: vmux_api::session::MemberId::local(&session_id),
                client_op_id: None,
                sequence: 1,
                reply_to: None,
            },
            vmux_session::MessageContent(vmux_api::session::Message::user("First prompt")),
            vmux_ecs::CreatedAt(3),
            ChildOf(session),
        ));

        app.world_mut().trigger(UiInput {
            webview,
            payload: ResumeListRequest {
                request_id: 9,
                query: "vmux".into(),
                offset: 0,
            },
        });
        app.world_mut().trigger(UiInput {
            webview,
            payload: PromptHistoryRequest {
                agent: "codex".into(),
                cwd: "/work/vmux".into(),
            },
        });
        app.world_mut().trigger(UiInput {
            webview,
            payload: ResumeSession {
                kind: "session".into(),
                sid: "session-1".into(),
                cwd: "/work/vmux".into(),
                open: None,
            },
        });
        app.world_mut().flush();

        let responses = app.world().resource::<Responses>();
        assert_eq!(responses.resumes.len(), 1);
        assert_eq!(responses.resumes[0].request_id, 9);
        assert_eq!(responses.resumes[0].total, 1);
        assert_eq!(responses.resumes[0].sessions[0].kind, "session");
        assert_eq!(responses.resumes[0].sessions[0].sid, "session-1");
        assert_eq!(responses.prompt_histories.len(), 1);
        assert_eq!(responses.prompt_histories[0].prompts, ["First prompt"]);
        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].url, "vmux://sessions/session-1");
        assert!(matches!(
            opened[0].target,
            PageOpenTarget::Stack(entity) if entity == stack
        ));
    }

    #[test]
    fn dismissed_provider_sessions_stay_out_of_resume_results() {
        let mut app = App::new();
        app.init_resource::<Responses>()
            .add_plugins(Plugin)
            .add_observer(Responses::command_bar);
        app.world_mut()
            .resource_mut::<ProviderCatalog>()
            .sessions
            .push(vmux_session::ProviderSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                agent: "codex".into(),
                name: "Removed session".into(),
                cwd: "/work/vmux".into(),
                updated_at: 2,
                ..Default::default()
            });
        app.insert_resource(ProviderSessionTombstones::new([ProviderSessionKey::new(
            ProviderKind("acp:codex".into()),
            ProviderSessionId("provider-session".into()),
        )]));
        let webview = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ResumeListRequest {
                request_id: 11,
                query: String::new(),
                offset: 0,
            },
        });
        app.world_mut().flush();

        let responses = app.world().resource::<Responses>();
        assert_eq!(responses.resumes.len(), 1);
        assert_eq!(responses.resumes[0].total, 0);
        assert!(responses.resumes[0].sessions.is_empty());
    }

    #[test]
    fn chat_resume_query_uses_the_same_local_session_catalog() {
        let mut app = App::new();
        app.init_resource::<Responses>()
            .add_plugins(Plugin)
            .add_observer(Responses::chat);
        spawn_session(app.world_mut());
        let webview = app.world_mut().spawn(SessionView).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ChatResumeQueryRequest {
                active: true,
                query: "repair".into(),
            },
        });
        app.world_mut().flush();

        let responses = app.world().resource::<Responses>();
        assert_eq!(responses.chat_resumes.len(), 1);
        assert!(responses.chat_resumes[0].active);
        assert!(!responses.chat_resumes[0].loading);
        assert_eq!(responses.chat_resumes[0].sessions.len(), 1);
        assert_eq!(responses.chat_resumes[0].sessions[0].sid, "session-1");
    }
}
