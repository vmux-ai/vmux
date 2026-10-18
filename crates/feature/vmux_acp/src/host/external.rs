use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::UiInput;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vmux_api::chat::ResumeSession;
use vmux_ecs::{PageOpenRequest, PageOpenTarget};
use vmux_session::{
    AgentConversationTitle, AgentFamily, AgentId, CreateRequest, Created, ProviderCatalog,
    ProviderSession, Route, Session, SessionId,
};
use vmux_setting::AppSettings;

use super::acp::registry::RegistryAgent;
use crate::AcpSessionId;

mod claude;
mod codex;
mod vibe;

pub(super) fn add(app: &mut App) {
    app.init_resource::<SourceSnapshot>()
        .init_resource::<PendingImports>()
        .init_resource::<ProviderCatalog>()
        .add_observer(open)
        .add_observer(attach)
        .add_systems(Startup, start)
        .add_systems(Update, (watch, delay, scan, finish, project).chain());
}

const SCAN_DEBOUNCE: Duration = Duration::from_millis(350);

#[derive(Resource, Default)]
struct SourceSnapshot(Vec<ProviderSession>);

#[derive(Resource, Default)]
struct PendingImports(HashMap<SessionId, PendingImport>);

struct PendingImport {
    stack: Entity,
    source: ProviderSession,
    agent: AgentId,
}

#[derive(Component)]
struct Runtime {
    changes: Mutex<mpsc::Receiver<()>>,
    _watcher: Option<RecommendedWatcher>,
}

#[derive(Component)]
struct ScanRequested;

#[derive(Component)]
struct ScanDelay(Task<()>);

#[derive(Component)]
struct ScanTask(Task<Vec<ProviderSession>>);

#[derive(Component, Clone)]
struct SourcePaths {
    codex: PathBuf,
    claude: PathBuf,
    vibe: PathBuf,
}

impl SourcePaths {
    fn current() -> Option<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        Some(Self {
            codex: std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex")),
            claude: std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".claude")),
            vibe: std::env::var_os("VIBE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".vibe")),
        })
    }

    fn readers(&self) -> Readers {
        Readers(vec![
            Box::new(codex::Reader::at(&self.codex)),
            Box::new(claude::Reader::at(&self.claude)),
            Box::new(vibe::Reader::at(&self.vibe)),
        ])
    }

    fn watches(&self) -> Vec<(PathBuf, RecursiveMode)> {
        vec![
            (self.codex.clone(), RecursiveMode::NonRecursive),
            (self.claude.clone(), RecursiveMode::NonRecursive),
            (self.vibe.join("logs/session"), RecursiveMode::Recursive),
        ]
    }

    fn contains(&self, paths: &[PathBuf]) -> bool {
        let codex = self.codex.join("session_index.jsonl");
        let claude = self.claude.join("history.jsonl");
        let vibe = self.vibe.join("logs/session");
        paths.iter().any(|path| {
            path == &codex
                || path == &claude
                || path.starts_with(&vibe)
                    && path.file_name().and_then(|name| name.to_str()) == Some("meta.json")
        })
    }
}

trait Reader: Send + Sync {
    fn read(&self) -> Result<Vec<ProviderSession>, String>;
}

struct Readers(Vec<Box<dyn Reader>>);

impl Readers {
    fn read(&self) -> Vec<ProviderSession> {
        let mut sessions = Vec::new();
        for reader in &self.0 {
            match reader.read() {
                Ok(mut read) => sessions.append(&mut read),
                Err(error) => bevy::log::warn!("failed to read provider sessions: {error}"),
            }
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
        sessions
    }
}

fn start(proxy: Option<Res<EventLoopProxyWrapper>>, mut commands: Commands) {
    let Some(paths) = SourcePaths::current() else {
        return;
    };
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let (sender, receiver) = mpsc::channel();
    let watched_paths = paths.clone();
    let watcher =
        notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
            let Ok(event) = result else {
                return;
            };
            if (!event.kind.is_create() && !event.kind.is_modify() && !event.kind.is_remove())
                || !watched_paths.contains(&event.paths)
            {
                return;
            }
            let _ = sender.send(());
            if let Some(wake) = wake.as_ref() {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
        });
    let mut watcher = watcher.ok();
    for (path, mode) in paths.watches() {
        if !path.exists() {
            continue;
        }
        let Some(active) = watcher.as_mut() else {
            break;
        };
        let _ = active.watch(&path, mode);
    }
    commands.spawn((
        Name::new("ACP provider session catalog"),
        Runtime {
            changes: Mutex::new(receiver),
            _watcher: watcher,
        },
        paths,
        ScanRequested,
    ));
}

fn watch(
    runtime: Single<(Entity, &Runtime)>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let (entity, runtime) = *runtime;
    let Ok(changes) = runtime.changes.lock() else {
        return;
    };
    let mut changed = false;
    while changes.try_recv().is_ok() {
        changed = true;
    }
    if changed {
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            std::thread::sleep(SCAN_DEBOUNCE);
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
        });
        commands.entity(entity).insert(ScanDelay(task));
    }
}

fn delay(mut delays: Query<(Entity, &mut ScanDelay)>, mut commands: Commands) {
    for (entity, mut delay) in &mut delays {
        if future::block_on(future::poll_once(&mut delay.0)).is_none() {
            continue;
        }
        commands
            .entity(entity)
            .remove::<ScanDelay>()
            .insert(ScanRequested);
    }
}

fn scan(
    runtime: Single<(Entity, &SourcePaths), (With<ScanRequested>, Without<ScanTask>)>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let (entity, paths) = *runtime;
    let readers = paths.readers();
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let task = IoTaskPool::get().spawn(async move {
        let sessions = readers.read();
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        sessions
    });
    commands
        .entity(entity)
        .remove::<ScanRequested>()
        .insert(ScanTask(task));
}

fn finish(
    task: Single<(Entity, &mut ScanTask)>,
    mut snapshot: ResMut<SourceSnapshot>,
    mut commands: Commands,
) {
    let (entity, mut task) = task.into_inner();
    let Some(sessions) = future::block_on(future::poll_once(&mut task.0)) else {
        return;
    };
    if snapshot.0 != sessions {
        snapshot.0 = sessions;
    }
    commands.entity(entity).remove::<ScanTask>();
}

fn project(
    snapshot: Res<SourceSnapshot>,
    imported: Query<(&AgentId, &AcpSessionId), With<Session>>,
    changed: Query<(), (With<Session>, Or<(Changed<AgentId>, Changed<AcpSessionId>)>)>,
    mut removed: RemovedComponents<AcpSessionId>,
    mut catalog: ResMut<ProviderCatalog>,
) {
    let removed = removed.read().next().is_some();
    if !snapshot.is_changed() && changed.is_empty() && !removed && !catalog.is_added() {
        return;
    }
    let imported = imported
        .iter()
        .map(|(agent, sid)| (AgentFamily::of(&agent.0), sid.0.clone()))
        .collect::<HashSet<_>>();
    let mut sessions = Vec::new();
    for session in &snapshot.0 {
        let key = (AgentFamily::of(&session.agent), session.sid.clone());
        if !imported.contains(&key) {
            sessions.push(session.clone());
        }
    }
    if catalog.sessions != sessions {
        catalog.sessions = sessions;
    }
}

fn open(
    trigger: On<UiInput<ResumeSession>>,
    parents: Query<&ChildOf>,
    catalog: Res<ProviderCatalog>,
    agents: AgentCatalog,
    existing: Query<(Entity, &SessionId, &AgentId, &AcpSessionId), With<Session>>,
    mut pending: ResMut<PendingImports>,
    mut creates: MessageWriter<CreateRequest>,
    mut opens: MessageWriter<PageOpenRequest>,
) {
    let request = &trigger.event().payload;
    if !request.kind.starts_with("acp:") {
        return;
    }
    let Some(source) = catalog.get(&request.kind, &request.sid).cloned() else {
        return;
    };
    let Ok(parent) = parents.get(trigger.event().webview) else {
        return;
    };
    let stack = parent.parent();
    let family = AgentFamily::of(&source.agent);
    if let Some((_, id, _, _)) = existing
        .iter()
        .find(|(_, _, agent, sid)| AgentFamily::of(&agent.0) == family && sid.0 == source.sid)
    {
        opens.write(PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url: Route::Session(id.clone()).url(),
            request_id: None,
        });
        return;
    }
    let agent = agents.resolve(&source.agent);
    let name = if source.name.trim().is_empty() {
        source.sid.clone()
    } else {
        source.name.clone()
    };
    let create = CreateRequest::imported(
        name,
        source.description.clone(),
        PathBuf::from(&source.cwd),
        agent.clone(),
        source.created_at,
        source.updated_at,
    );
    pending.0.insert(
        create.id().clone(),
        PendingImport {
            stack,
            source,
            agent,
        },
    );
    creates.write(create);
}

fn attach(
    trigger: On<Created>,
    mut pending: ResMut<PendingImports>,
    mut opens: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Some(import) = pending.0.remove(&event.id) else {
        return;
    };
    let title = if import.source.conversation_title.trim().is_empty() {
        import.source.name.clone()
    } else {
        import.source.conversation_title.clone()
    };
    commands.entity(event.entity).insert((
        import.agent,
        AcpSessionId(import.source.sid),
        AgentConversationTitle(title),
    ));
    opens.write(PageOpenRequest {
        target: PageOpenTarget::Stack(import.stack),
        url: Route::Session(event.id.clone()).url(),
        request_id: None,
    });
}

#[derive(SystemParam)]
struct AgentCatalog<'w, 's> {
    settings: Res<'w, AppSettings>,
    registry: Query<'w, 's, &'static RegistryAgent>,
}

impl AgentCatalog<'_, '_> {
    fn resolve(&self, source: &str) -> AgentId {
        let family = AgentFamily::of(source);
        if let Some(config) = self
            .settings
            .agent
            .acp
            .iter()
            .find(|config| AgentFamily::of(&config.id) == family)
        {
            return AgentId(config.id.clone());
        }
        if let Some(agent) = self
            .registry
            .iter()
            .find(|agent| AgentFamily::of(&agent.id) == family && agent.is_installed())
        {
            return AgentId(agent.id.clone());
        }
        if let Some(agent) = self
            .registry
            .iter()
            .find(|agent| AgentFamily::of(&agent.id) == family)
        {
            return AgentId(agent.id.clone());
        }
        AgentId(source.to_string())
    }
}

struct Timestamp;

impl Timestamp {
    fn parse(value: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(value)
            .map(|timestamp| timestamp.timestamp_millis())
            .unwrap_or_default()
    }
}

struct PromptText;

impl PromptText {
    fn compact(value: &str) -> String {
        let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.chars().count() <= 160 {
            return normalized;
        }
        let mut compact = normalized.chars().take(159).collect::<String>();
        compact.push('…');
        compact
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_resume_materializes_a_vmux_session_before_navigation() {
        let mut app = App::new();
        app.init_resource::<ProviderCatalog>()
            .init_resource::<PendingImports>()
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_observer(open)
            .add_observer(attach);
        app.world_mut()
            .resource_mut::<ProviderCatalog>()
            .sessions
            .push(ProviderSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                agent: "codex".into(),
                agent_name: "Codex".into(),
                name: "Provider task".into(),
                cwd: "/work/vmux".into(),
                created_at: 100,
                updated_at: 200,
                conversation_title: "Provider task".into(),
                ..Default::default()
            });
        let stack = app.world_mut().spawn_empty().id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ResumeSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                cwd: "/work/vmux".into(),
            },
        });

        let created = app
            .world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].agent, Some(AgentId("codex".into())));
        let id = created[0].id().clone();
        let session = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(Created {
            entity: session,
            id: id.clone(),
        });
        app.world_mut().flush();

        assert_eq!(
            app.world().get::<AcpSessionId>(session),
            Some(&AcpSessionId("provider-session".into()))
        );
        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].url, Route::Session(id).url());
        assert!(matches!(
            opened[0].target,
            PageOpenTarget::Stack(entity) if entity == stack
        ));
    }
}
