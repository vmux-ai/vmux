use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::acp_registry::RegistryAgent;
use crate::handoff::{BuiltContext, DEFAULT_CONTEXT_LIMIT};
use crate::session::CliSessionSources;
use vmux_api::chat::{PromptHistory, PromptHistoryRequest};
use vmux_chat::event::{
    ChatResumeQueryRequest, ResumableSessionEntry, ResumableSessions, ResumeListRequest,
    ResumeSession,
};
use vmux_chat::host::{ChatResumeProjection, ChatView};
use vmux_core::agent::{AgentKind, StackSessionHandoff, SwapStackSession};
use vmux_core::team::Profile;
use vmux_session::AcpSession;
use vmux_session::AgentRunState;
use vmux_session::AgentSession;

pub(super) struct ChatResumePlugin;

impl Plugin for ChatResumePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            ResumeListRequest,
            ChatResumeQueryRequest,
            ResumeSession,
            PromptHistoryRequest,
        )>::default())
            .add_systems(Startup, spawn_resumable_scan)
            .add_observer(on_resume_list_request)
            .add_observer(on_chat_resume_query_request)
            .add_observer(on_chat_resume_query)
            .add_observer(on_resume_session)
            .add_observer(on_prompt_history_request)
            .add_systems(
                Update,
                (
                    drain_resume_list_tasks,
                    drain_resume_handoff_tasks,
                    drain_prompt_history_tasks,
                ),
            );
    }
}

#[derive(Component)]
struct ResumeListTask {
    webview: Entity,
    task: Task<ResumeListAnswer>,
}

struct ResumeListAnswer {
    sessions: ResumableSessions,
    scanned: Option<Vec<crate::host::cli::ResumableSession>>,
    labels: RepoLabels,
}

impl ResumeListAnswer {
    const PAGE_SIZE: usize = 50;

    fn matches(session: &ResumableSessionEntry, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || session.sid.to_lowercase().contains(&query)
            || session.title.to_lowercase().contains(&query)
            || session.cwd.to_lowercase().contains(&query)
    }
}

#[derive(EntityEvent)]
struct ChatResumeQuery {
    #[event_target]
    webview: Entity,
    active: bool,
    query: String,
}

impl ChatResumeQuery {
    fn new(webview: Entity, active: bool, query: String) -> Self {
        Self {
            webview,
            active,
            query,
        }
    }
}

#[derive(Component)]
struct ResumeHandoffTask {
    stack: Entity,
    target_url: String,
    cwd: std::path::PathBuf,
    task: Task<Result<StackSessionHandoff, String>>,
}

fn relative_time_seconds(mtime: std::time::SystemTime) -> u64 {
    std::time::SystemTime::now()
        .duration_since(mtime)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone, Default)]
struct RepoLabels {
    by_dir: std::collections::HashMap<std::path::PathBuf, (String, String)>,
}

impl RepoLabels {
    fn resolve(&mut self, cwd: &std::path::Path, fallback: &str) -> (String, String) {
        if let Some(held) = self.by_dir.get(cwd) {
            return held.clone();
        }
        let read = match vmux_git::worktree::RepoLabel::read(cwd) {
            Some(label) => (label.project, label.branch),
            None => (fallback.to_string(), String::new()),
        };
        self.by_dir.insert(cwd.to_path_buf(), read.clone());
        read
    }
}

fn resume_entries(
    sessions: Vec<crate::host::cli::ResumableSession>,
    active_kind: Option<AgentKind>,
    active_name: &str,
    labels: &mut RepoLabels,
) -> Vec<ResumableSessionEntry> {
    let mut entries = Vec::new();
    for session in sessions {
        let dir = session
            .cwd
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| session.cwd.to_string_lossy().to_string());
        let agent_name = if Some(session.kind) == active_kind && !active_name.is_empty() {
            active_name.to_string()
        } else {
            session.kind.display_name().to_string()
        };
        let url = crate::AgentUrl::Cli {
            kind: session.kind,
            sid: session.sid.clone(),
        }
        .format();
        let (project, branch) = labels.resolve(&session.cwd, &dir);
        entries.push(ResumableSessionEntry {
            kind: session.kind.as_url_segment().to_string(),
            sid: session.sid,
            cwd: session.cwd.to_string_lossy().to_string(),
            url,
            title: session.title,
            latest: session.latest,
            subtitle: dir,
            age_seconds: relative_time_seconds(session.mtime),
            updated_at: chrono::DateTime::<chrono::Local>::from(session.mtime)
                .format("%Y-%m-%d")
                .to_string(),
            agent_name,
            project,
            branch,
            cross_runtime: session.cross_runtime,
        });
    }
    entries
}

fn foreign_handoff_target(
    active_agent_id: &str,
    active_kind: Option<AgentKind>,
    source_kind: AgentKind,
) -> Option<String> {
    (active_kind != Some(source_kind)).then(|| {
        crate::AgentUrl::Acp {
            id: active_agent_id.to_string(),
            sid: None,
        }
        .format()
    })
}

fn resume_agent_name(
    profile: Option<&Profile>,
    kind: Option<AgentKind>,
    acp_id: Option<&str>,
) -> String {
    profile
        .map(|profile| profile.name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| kind.map(|kind| kind.display_name().to_string()))
        .or_else(|| acp_id.map(str::to_string))
        .unwrap_or_default()
}

#[derive(Component, Default)]
struct ResumableScan {
    sessions: Vec<crate::host::cli::ResumableSession>,
    labels: RepoLabels,
    read_at: Option<std::time::Instant>,
}

fn spawn_resumable_scan(mut commands: Commands) {
    commands.spawn((Name::new("Resumable agent scan"), ResumableScan::default()));
}

impl ResumableScan {
    const FRESH_FOR: std::time::Duration = std::time::Duration::from_secs(20);

    fn is_fresh(&self) -> bool {
        self.read_at
            .is_some_and(|at| at.elapsed() < Self::FRESH_FOR)
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct ResumeAsk<'w, 's> {
    child_of: Query<'w, 's, &'static ChildOf>,
    acp_sessions: Query<'w, 's, &'static AcpSession>,
    agent_sessions: Query<'w, 's, &'static AgentSession>,
    profiles: Query<'w, 's, &'static Profile>,
    space_hierarchy: vmux_layout::space::SpaceHierarchy<'w, 's>,
    settings: Option<Res<'w, vmux_setting::AppSettings>>,
}

impl ResumeAsk<'_, '_> {
    fn agent_of(&self, webview: Entity) -> (Option<AgentKind>, String) {
        let stack = self.child_of.get(webview).ok().map(ChildOf::parent);
        let acp = stack.and_then(|stack| self.acp_sessions.get(stack).ok());
        let kind = acp
            .and_then(|acp| RegistryAgent::kind(&acp.agent_id))
            .or_else(|| {
                stack.and_then(|stack| {
                    self.agent_sessions
                        .get(stack)
                        .ok()
                        .map(|session| session.kind)
                })
            });
        let name = resume_agent_name(
            stack.and_then(|stack| self.profiles.get(stack).ok()),
            kind,
            acp.map(|acp| acp.agent_id.as_str()),
        );
        (kind, name)
    }

    fn project_of(&self, webview: Entity) -> Option<std::path::PathBuf> {
        let settings = self.settings.as_deref()?;
        let space_id = self.space_hierarchy.id(webview)?;
        let dir = settings.space(&space_id)?.active_dir()?;
        (!dir.is_empty()).then(|| std::path::PathBuf::from(dir))
    }
}

struct Preferred;

impl Preferred {
    fn first(
        sessions: &[crate::host::cli::ResumableSession],
        kind: Option<AgentKind>,
        project: Option<&std::path::Path>,
    ) -> Vec<crate::host::cli::ResumableSession> {
        if kind.is_none() && project.is_none() {
            return sessions.to_vec();
        }
        let mut ranked: Vec<_> = sessions.to_vec();
        ranked.sort_by_key(|session| {
            let in_project = project.is_some_and(|dir| session.cwd.starts_with(dir));
            let same_agent = kind.is_some_and(|kind| session.kind == kind);
            (!in_project, !same_agent)
        });
        ranked
    }
}

#[derive(Component)]
struct PromptHistoryTask {
    webview: Entity,
    task: Task<PromptHistory>,
}

fn on_prompt_history_request(
    trigger: On<UiInput<PromptHistoryRequest>>,
    sources: CliSessionSources,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let wake = vmux_core::host::wake::Wake::from_resource(proxy);
    let asked = trigger.event().payload.clone();
    let Some(kind) = AgentKind::from_url_segment(&asked.agent) else {
        return;
    };
    let history = sources.prompt_history(kind);
    let task = IoTaskPool::get().spawn(async move {
        let _wake = wake;
        let cwd = std::path::PathBuf::from(&asked.cwd);
        PromptHistory {
            prompts: history.map(|history| (history.0)(&cwd)).unwrap_or_default(),
        }
    });
    commands.spawn(PromptHistoryTask { webview, task });
}

fn drain_prompt_history_tasks(
    mut tasks: Query<(Entity, &mut PromptHistoryTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(history) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        commands.trigger(vmux_core::host::UiStateWrite::<
            vmux_api::command_bar::CommandBarUiState,
        >::from_event(task.webview, &history));
    }
}

fn on_resume_list_request(
    trigger: On<UiInput<ResumeListRequest>>,
    sources: CliSessionSources,
    ask: ResumeAsk,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    scan: Single<&ResumableScan>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let wake = vmux_core::host::wake::Wake::from_resource(proxy);
    let sources = sources.all();
    let (kind, agent_name) = ask.agent_of(webview);
    let project = ask.project_of(webview);
    let request_id = trigger.event().payload.request_id;
    let query = trigger.event().payload.query.clone();
    let offset = trigger.event().payload.offset;
    let mut labels = scan.labels.clone();
    let held = scan
        .is_fresh()
        .then(|| Preferred::first(&scan.sessions, kind, project.as_deref()));
    let task = IoTaskPool::get().spawn(async move {
        let _wake = wake;
        let (ranked, scanned) = match held {
            Some(ranked) => (ranked, None),
            None => {
                let pool = IoTaskPool::get();
                let mut scanning = Vec::new();
                for source in sources.iter().copied() {
                    scanning.push(pool.spawn(async move { (source.list_sessions)() }));
                }
                let mut all = Vec::new();
                for scan in scanning {
                    all.extend(scan.await);
                }
                let all = crate::host::cli::ResumableSession::newest_unique(all);
                let ranked = Preferred::first(&all, kind, project.as_deref());
                (ranked, Some(all))
            }
        };
        let mut built = resume_entries(ranked, kind, &agent_name, &mut labels);
        built.retain(|session| ResumeListAnswer::matches(session, &query));
        let total = built.len() as u32;
        let sessions = built
            .into_iter()
            .skip(offset as usize)
            .take(ResumeListAnswer::PAGE_SIZE)
            .collect();
        ResumeListAnswer {
            sessions: ResumableSessions {
                request_id,
                query,
                sessions,
                offset,
                total,
            },
            scanned,
            labels,
        }
    });
    commands.spawn(ResumeListTask { webview, task });
}

fn on_chat_resume_query_request(
    trigger: On<UiInput<ChatResumeQueryRequest>>,
    mut commands: Commands,
) {
    commands.trigger(ChatResumeQuery::new(
        trigger.event().webview,
        trigger.event().payload.active,
        trigger.event().payload.query.clone(),
    ));
}

fn on_chat_resume_query(
    trigger: On<ChatResumeQuery>,
    mut projections: Query<&mut ChatResumeProjection, With<ChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event_target();
    let request = trigger.event();
    let Ok(mut projection) = projections.get_mut(webview) else {
        return;
    };
    let Some(request_id) = projection.start(request.active, request.query.clone()) else {
        return;
    };
    commands.trigger(
        vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
            webview,
            &projection.0,
        ),
    );
    if !request.active {
        return;
    }
    commands.trigger(UiInput {
        webview,
        payload: ResumeListRequest {
            request_id,
            query: request.query.clone(),
            offset: 0,
        },
    });
}

fn drain_resume_list_tasks(
    mut tasks: Query<(Entity, &mut ResumeListTask)>,
    mut projections: Query<&mut ChatResumeProjection, With<ChatView>>,
    mut scan: Single<&mut ResumableScan>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(answer) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        scan.labels = answer.labels;
        if let Some(scanned) = answer.scanned {
            scan.sessions = scanned;
            scan.read_at = Some(std::time::Instant::now());
        }
        if let Ok(mut projection) = projections.get_mut(task.webview) {
            if !projection.finish(&answer.sessions) {
                continue;
            }
            commands.trigger(
                vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                    task.webview,
                    &projection.0,
                ),
            );
        } else {
            commands.trigger(vmux_core::host::UiStateWrite::<
                vmux_api::command_bar::CommandBarUiState,
            >::from_event(task.webview, &answer.sessions));
        }
    }
}

fn drain_resume_handoff_tasks(
    mut tasks: Query<(Entity, &mut ResumeHandoffTask)>,
    mut states: Query<&mut AgentRunState>,
    mut swap: MessageWriter<SwapStackSession>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok(handoff) => {
                swap.write(SwapStackSession {
                    stack: pending.stack,
                    target_url: pending.target_url.clone(),
                    cwd: pending.cwd.clone(),
                    handoff: Some(handoff),
                });
            }
            Err(message) => {
                if let Ok(mut state) = states.get_mut(pending.stack) {
                    *state = AgentRunState::Errored(message);
                }
            }
        }
    }
}

fn on_resume_session(
    trigger: On<UiInput<ResumeSession>>,
    child_of: Query<&ChildOf>,
    acp_sessions: Query<&AcpSession>,
    settings: Res<vmux_setting::AppSettings>,
    sources: CliSessionSources,
    mut commands: Commands,
    mut swap: MessageWriter<SwapStackSession>,
) {
    let payload = &trigger.event().payload;
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let stack = parent.parent();
    let Some(kind) = AgentKind::from_url_segment(&payload.kind) else {
        return;
    };
    if let Ok(acp) = acp_sessions.get(stack)
        && let Some(target_url) =
            foreign_handoff_target(&acp.agent_id, RegistryAgent::kind(&acp.agent_id), kind)
    {
        let source = sources.get(kind);
        let source_sid = payload.sid.clone();
        let source_agent = kind.display_name().to_string();
        let cwd = std::path::PathBuf::from(&payload.cwd);
        let task = IoTaskPool::get().spawn(async move {
            let source = source.ok_or_else(|| {
                format!(
                    "no CLI session source registered for {}",
                    kind.display_name()
                )
            })?;
            let messages = (source.load_transcript)(&source_sid)?;
            let built = BuiltContext::from_messages(&messages, DEFAULT_CONTEXT_LIMIT);
            Ok(StackSessionHandoff {
                source_agent,
                source_kind: kind,
                source_sid,
                messages,
                context: built.text,
                truncated: built.truncated,
            })
        });
        commands.spawn(ResumeHandoffTask {
            stack,
            target_url,
            cwd,
            task,
        });
        return;
    }
    let prefer_acp = acp_sessions.get(stack).is_ok();
    let acp_ids: Vec<String> = settings.agent.acp.iter().map(|c| c.id.clone()).collect();
    let target = crate::AgentUrl::for_session(kind, &payload.sid, prefer_acp, &acp_ids);
    swap.write(SwapStackSession {
        stack,
        target_url: target.format(),
        cwd: std::path::PathBuf::from(&payload.cwd),
        handoff: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_query_matches_sid_title_and_cwd_case_insensitively() {
        let sessions = [
            ResumableSessionEntry {
                sid: "SID-ABC".into(),
                title: "Fix auth".into(),
                cwd: "/work/api".into(),
                ..Default::default()
            },
            ResumableSessionEntry {
                sid: "sid-def".into(),
                title: "Docs".into(),
                cwd: "/work/site".into(),
                ..Default::default()
            },
        ];
        assert!(ResumeListAnswer::matches(&sessions[0], "abc"));
        assert!(ResumeListAnswer::matches(&sessions[0], "AUTH"));
        assert!(ResumeListAnswer::matches(&sessions[1], "SITE"));
        assert!(!ResumeListAnswer::matches(&sessions[0], "missing"));
    }

    #[test]
    fn resume_projection_rejects_stale_results() {
        let mut projection = ChatResumeProjection::default();
        let stale = projection.start(true, "old".into()).unwrap();
        let current = projection.start(true, "new".into()).unwrap();
        assert!(!projection.finish(&ResumableSessions {
            request_id: stale,
            query: "old".into(),
            ..Default::default()
        }));
        assert!(projection.0.loading);
        assert!(projection.finish(&ResumableSessions {
            request_id: current,
            query: "new".into(),
            total: 1,
            ..Default::default()
        }));
        assert_eq!(projection.0.total, 1);
        assert!(!projection.0.loading);
    }

    #[test]
    fn resume_results_include_all_agent_kinds_with_source_labels() {
        use crate::host::cli::ResumableSession;
        use std::time::SystemTime;

        let session = |kind, sid: &str| ResumableSession {
            kind,
            sid: sid.into(),
            cwd: "/work".into(),
            transcript: "/work/none.jsonl".into(),
            mtime: SystemTime::UNIX_EPOCH,
            title: sid.into(),
            latest: String::new(),
            cross_runtime: kind.supports_cross_runtime(),
        };
        let entries = resume_entries(
            vec![
                session(AgentKind::Claude, "claude-1"),
                session(AgentKind::Codex, "codex-1"),
            ],
            Some(AgentKind::Claude),
            "Antigravity",
            &mut RepoLabels::default(),
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].agent_name, "Antigravity");
        assert_eq!(entries[1].agent_name, "Codex");
    }

    #[test]
    fn foreign_resume_keeps_active_acp_agent_fresh() {
        assert_eq!(
            foreign_handoff_target("claude", Some(AgentKind::Claude), AgentKind::Codex,),
            Some("vmux://sessions/claude".to_string())
        );
        assert_eq!(
            foreign_handoff_target("claude", Some(AgentKind::Claude), AgentKind::Claude,),
            None
        );
        assert_eq!(
            foreign_handoff_target("custom-acp", None, AgentKind::Codex),
            Some("vmux://sessions/custom-acp".to_string())
        );
    }

    #[test]
    fn resume_agent_name_prefers_profile_then_kind_then_id() {
        let profile = Profile::registry("Antigravity", "antigravity");
        assert_eq!(
            resume_agent_name(Some(&profile), Some(AgentKind::Claude), Some("claude")),
            "Antigravity"
        );
        assert_eq!(
            resume_agent_name(None, Some(AgentKind::Claude), Some("claude")),
            "Claude"
        );
        assert_eq!(
            resume_agent_name(None, None, Some("custom-acp")),
            "custom-acp"
        );
    }
}
