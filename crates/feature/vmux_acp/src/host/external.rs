use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Read};
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

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
        .add_systems(Update, (watch, scan, finish, project).chain());
}

const SCAN_DEBOUNCE: Duration = Duration::from_millis(350);
const SCAN_MAX_LATENCY: Duration = Duration::from_secs(2);
const PROVIDER_READ_LIMIT: usize = 256 * 1024 * 1024;
const PROVIDER_LINE_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Resource, Default)]
struct SourceSnapshot(HashMap<SourceKind, Vec<ProviderSession>>);

#[derive(Resource, Default)]
struct PendingImports {
    by_id: HashMap<SessionId, PendingImport>,
    by_source: HashMap<(AgentFamily, String), SessionId>,
}

impl PendingImports {
    fn reserve(&mut self, id: SessionId, import: PendingImport) -> bool {
        let key = (
            AgentFamily::of(&import.source.agent),
            import.source.sid.clone(),
        );
        if let Some(existing) = self.by_source.get(&key)
            && let Some(pending) = self.by_id.get_mut(existing)
        {
            pending.stack = import.stack;
            return false;
        }
        self.by_source.insert(key, id.clone());
        self.by_id.insert(id, import);
        true
    }

    fn take(&mut self, id: &SessionId) -> Option<PendingImport> {
        let import = self.by_id.remove(id)?;
        self.by_source.remove(&(
            AgentFamily::of(&import.source.agent),
            import.source.sid.clone(),
        ));
        Some(import)
    }
}

struct PendingImport {
    stack: Entity,
    source: ProviderSession,
    agent: AgentId,
}

#[derive(Component)]
struct Runtime {
    changes: Mutex<mpsc::Receiver<SourceChanges>>,
    watcher: Option<RecommendedWatcher>,
    watched: HashMap<PathBuf, bool>,
}

impl Runtime {
    fn ensure_watches(&mut self, paths: &SourcePaths) {
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        let stale = self
            .watched
            .keys()
            .filter(|path| !path.exists())
            .cloned()
            .collect::<Vec<_>>();
        for path in stale {
            self.watched.remove(&path);
            let _ = watcher.unwatch(&path);
        }
        for (target, recursive) in paths.watches() {
            let mut path = target.clone();
            while !path.exists() && path.pop() {}
            if !path.exists() {
                continue;
            }
            let recursive = recursive && path == target;
            if self
                .watched
                .get(&path)
                .is_some_and(|current| *current || !recursive)
            {
                continue;
            }
            if self.watched.remove(&path).is_some() {
                let _ = watcher.unwatch(&path);
            }
            let mode = if recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if watcher.watch(&path, mode).is_ok() {
                self.watched.insert(path, recursive);
            }
        }
    }
}

#[derive(Component)]
struct ScanRequested(HashSet<SourceKind>);

#[derive(Component)]
struct ScanTask(Task<Vec<SourceRead>>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SourceKind {
    Codex,
    Claude,
    Vibe,
}

impl SourceKind {
    const ALL: [Self; 3] = [Self::Codex, Self::Claude, Self::Vibe];

    fn all() -> HashSet<Self> {
        Self::ALL.into_iter().collect()
    }
}

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

    fn readers(&self, sources: &HashSet<SourceKind>, snapshot: &SourceSnapshot) -> Readers {
        let mut readers = Vec::<(SourceKind, Box<dyn Reader>)>::new();
        for source in SourceKind::ALL {
            if !sources.contains(&source) {
                continue;
            }
            let reader: Box<dyn Reader> = match source {
                SourceKind::Codex => Box::new(
                    codex::Reader::at(&self.codex).with_previous(
                        snapshot
                            .0
                            .get(&source)
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                    ),
                ),
                SourceKind::Claude => Box::new(claude::Reader::at(&self.claude)),
                SourceKind::Vibe => Box::new(vibe::Reader::at(&self.vibe)),
            };
            readers.push((source, reader));
        }
        Readers(readers)
    }

    fn watches(&self) -> Vec<(PathBuf, bool)> {
        vec![
            (self.codex.clone(), false),
            (self.codex.join("sessions"), true),
            (self.claude.clone(), false),
            (self.vibe.join("logs/session"), true),
        ]
    }

    fn sources(&self, paths: &[PathBuf]) -> HashSet<SourceKind> {
        let codex = self.codex.join("session_index.jsonl");
        let codex_sessions = self.codex.join("sessions");
        let claude = self.claude.join("history.jsonl");
        let vibe = self.vibe.join("logs/session");
        let mut sources = HashSet::new();
        for path in paths {
            if path == &self.codex
                || path == &codex
                || path == &codex_sessions
                || path.starts_with(&codex_sessions)
                || path.parent() == Some(self.codex.as_path())
                || self.codex.starts_with(path)
            {
                sources.insert(SourceKind::Codex);
            }
            if path == &self.claude
                || path == &claude
                || path.parent() == Some(self.claude.as_path())
                || self.claude.starts_with(path)
            {
                sources.insert(SourceKind::Claude);
            }
            if path == &vibe || path.starts_with(&vibe) || vibe.starts_with(path) {
                sources.insert(SourceKind::Vibe);
            }
        }
        sources
    }
}

trait Reader: Send + Sync {
    fn read(&self) -> Result<Vec<ProviderSession>, String>;
}

struct Readers(Vec<(SourceKind, Box<dyn Reader>)>);

impl Readers {
    fn read(self) -> Vec<SourceRead> {
        let mut reads = Vec::new();
        for (source, reader) in self.0 {
            reads.push(SourceRead::read(source, reader));
        }
        reads
    }
}

struct SourceRead {
    source: SourceKind,
    sessions: Result<Vec<ProviderSession>, String>,
}

impl SourceRead {
    fn read(source: SourceKind, reader: Box<dyn Reader>) -> Self {
        let sessions = reader.read().map(Self::deduplicated);
        Self { source, sessions }
    }

    fn deduplicated(sessions: Vec<ProviderSession>) -> Vec<ProviderSession> {
        let mut indexed = HashMap::<(String, String), ProviderSession>::new();
        for session in sessions {
            if session.kind.trim().is_empty() || session.sid.trim().is_empty() {
                continue;
            }
            let key = (session.kind.clone(), session.sid.clone());
            match indexed.get(&key) {
                Some(current) if current.updated_at > session.updated_at => {}
                _ => {
                    indexed.insert(key, session);
                }
            }
        }
        let mut sessions = indexed.into_values().collect::<Vec<_>>();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
        sessions
    }
}

struct BoundedLines<R> {
    reader: R,
    remaining: usize,
}

impl<R: BufRead> BoundedLines<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            remaining: PROVIDER_READ_LIMIT,
        }
    }
}

impl<R: BufRead> Iterator for BoundedLines<R> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        while self.remaining > 0 {
            let limit = self.remaining.min(PROVIDER_LINE_LIMIT.saturating_add(1));
            let mut bytes = Vec::new();
            let read = {
                let mut limited = Read::by_ref(&mut self.reader).take(limit as u64);
                limited.read_until(b'\n', &mut bytes).ok()?
            };
            if read == 0 {
                return None;
            }
            self.remaining = self.remaining.saturating_sub(read);
            if bytes.len() > PROVIDER_LINE_LIMIT {
                let skipped = self.reader.skip_until(b'\n').ok()?;
                self.remaining = self.remaining.saturating_sub(skipped);
                continue;
            }
            while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                bytes.pop();
            }
            if let Ok(line) = String::from_utf8(bytes) {
                return Some(line);
            }
        }
        None
    }
}

struct SourceWatchEvents {
    sender: mpsc::Sender<SourceChange>,
    receiver: mpsc::Receiver<SourceChanges>,
}

impl SourceWatchEvents {
    fn start(proxy: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>) -> Self {
        let (input_tx, input_rx) = mpsc::channel::<SourceChange>();
        let (output_tx, output_rx) = mpsc::channel::<SourceChanges>();
        let worker = std::thread::Builder::new()
            .name("vmux-acp-session-watch-debounce".to_string())
            .spawn(move || {
                while let Ok(change) = input_rx.recv() {
                    let started = Instant::now();
                    let mut changes = SourceChanges::default();
                    changes.insert(change);
                    let mut disconnected = false;
                    while let Some(remaining) = SCAN_MAX_LATENCY.checked_sub(started.elapsed()) {
                        let timeout = SCAN_DEBOUNCE.min(remaining);
                        match input_rx.recv_timeout(timeout) {
                            Ok(change) => changes.insert(change),
                            Err(mpsc::RecvTimeoutError::Timeout) => break,
                            Err(mpsc::RecvTimeoutError::Disconnected) => {
                                disconnected = true;
                                break;
                            }
                        }
                    }
                    if output_tx.send(changes).is_err() {
                        break;
                    }
                    if let Some(proxy) = proxy.as_ref() {
                        let _ = proxy.send_event(WinitUserEvent::WakeUp);
                    }
                    if disconnected {
                        break;
                    }
                }
            });
        if let Err(error) = worker {
            bevy::log::warn!("ACP provider watch debounce init failed: {error}");
        }
        Self {
            sender: input_tx,
            receiver: output_rx,
        }
    }
}

enum SourceChange {
    Scan(SourceKind),
    CodexActivity(PathBuf),
}

#[derive(Default)]
struct SourceChanges {
    scans: HashSet<SourceKind>,
    codex_activity: HashSet<PathBuf>,
}

impl SourceChanges {
    fn insert(&mut self, change: SourceChange) {
        match change {
            SourceChange::Scan(source) => {
                self.scans.insert(source);
            }
            SourceChange::CodexActivity(path) => {
                self.codex_activity.insert(path);
            }
        }
    }

    fn extend(&mut self, changes: Self) {
        self.scans.extend(changes.scans);
        self.codex_activity.extend(changes.codex_activity);
    }

    fn is_empty(&self) -> bool {
        self.scans.is_empty() && self.codex_activity.is_empty()
    }
}

fn start(proxy: Option<Res<EventLoopProxyWrapper>>, mut commands: Commands) {
    let Some(paths) = SourcePaths::current() else {
        return;
    };
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let events = SourceWatchEvents::start(wake);
    let sender = events.sender;
    let watched_paths = paths.clone();
    let watcher =
        notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
            let Ok(event) = result else {
                return;
            };
            if !event.kind.is_create() && !event.kind.is_modify() && !event.kind.is_remove() {
                return;
            }
            let mut sources = watched_paths.sources(&event.paths);
            if event.kind.is_modify()
                && event
                    .paths
                    .iter()
                    .all(|path| path.starts_with(watched_paths.codex.join("sessions")))
            {
                sources.remove(&SourceKind::Codex);
                for path in event.paths {
                    let _ = sender.send(SourceChange::CodexActivity(path));
                }
            }
            for source in sources {
                let _ = sender.send(SourceChange::Scan(source));
            }
        });
    let mut runtime = Runtime {
        changes: Mutex::new(events.receiver),
        watcher: watcher.ok(),
        watched: HashMap::new(),
    };
    runtime.ensure_watches(&paths);
    commands.spawn((
        Name::new("ACP provider session catalog"),
        runtime,
        paths,
        ScanRequested(SourceKind::all()),
    ));
}

fn watch(
    runtime: Single<(Entity, &mut Runtime, &SourcePaths)>,
    mut requested: Query<&mut ScanRequested>,
    mut snapshot: ResMut<SourceSnapshot>,
    mut commands: Commands,
) {
    let (entity, mut runtime, paths) = runtime.into_inner();
    let mut changes = SourceChanges::default();
    {
        let Ok(receiver) = runtime.changes.lock() else {
            return;
        };
        while let Ok(received) = receiver.try_recv() {
            changes.extend(received);
        }
    }
    if changes.is_empty() {
        return;
    }
    runtime.ensure_watches(paths);
    for path in changes.codex_activity {
        let Some(activity) = codex::RolloutActivity::read(&path) else {
            continue;
        };
        let Some(sessions) = snapshot.0.get_mut(&SourceKind::Codex) else {
            changes.scans.insert(SourceKind::Codex);
            continue;
        };
        if !activity.refresh(sessions) {
            changes.scans.insert(SourceKind::Codex);
        }
    }
    if changes.scans.is_empty() {
        return;
    }
    if let Ok(mut requested) = requested.get_mut(entity) {
        requested.0.extend(changes.scans);
    } else {
        commands.entity(entity).insert(ScanRequested(changes.scans));
    }
}

fn scan(
    runtime: Single<(Entity, &SourcePaths, &ScanRequested), (With<Runtime>, Without<ScanTask>)>,
    snapshot: Res<SourceSnapshot>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let (entity, paths, requested) = *runtime;
    let readers = paths.readers(&requested.0, &snapshot);
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let task = IoTaskPool::get().spawn(async move {
        let reads = readers.read();
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        reads
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
    let Some(reads) = future::block_on(future::poll_once(&mut task.0)) else {
        return;
    };
    let mut next = snapshot.0.clone();
    for read in reads {
        match read.sessions {
            Ok(sessions) => {
                next.insert(read.source, sessions);
            }
            Err(error) => {
                bevy::log::warn!("failed to read provider sessions: {error}");
            }
        }
    }
    if snapshot.0 != next {
        snapshot.0 = next;
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
    for source in SourceKind::ALL {
        let Some(source_sessions) = snapshot.0.get(&source) else {
            continue;
        };
        for session in source_sessions {
            let key = (AgentFamily::of(&session.agent), session.sid.clone());
            if !imported.contains(&key) {
                sessions.push(session.clone());
            }
        }
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
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
    let Some(mut source) = catalog.get(&request.kind, &request.sid).cloned() else {
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
    let Ok(cwd) = PathBuf::from(&source.cwd).canonicalize() else {
        return;
    };
    if !cwd.is_dir() {
        return;
    }
    source.cwd = cwd.to_string_lossy().into_owned();
    let agent = agents.resolve(&source.agent);
    let name = if source.name.trim().is_empty() {
        source.sid.clone()
    } else {
        source.name.clone()
    };
    let create = CreateRequest::imported(
        name,
        source.description.clone(),
        cwd,
        agent.clone(),
        source.created_at,
        source.updated_at,
    );
    if !pending.reserve(
        create.id().clone(),
        PendingImport {
            stack,
            source,
            agent,
        },
    ) {
        return;
    }
    creates.write(create);
}

fn attach(
    trigger: On<Created>,
    mut pending: ResMut<PendingImports>,
    mut opens: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Some(import) = pending.take(&event.id) else {
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
    fn missing_provider_directory_upgrades_from_parent_watch() {
        let home = tempfile::tempdir().unwrap();
        let paths = SourcePaths {
            codex: home.path().join(".codex"),
            claude: home.path().join(".claude"),
            vibe: home.path().join(".vibe"),
        };
        let (_, receiver) = mpsc::channel();
        let mut runtime = Runtime {
            changes: Mutex::new(receiver),
            watcher: notify::recommended_watcher(|_| {}).ok(),
            watched: HashMap::new(),
        };

        runtime.ensure_watches(&paths);
        assert_eq!(runtime.watched.get(home.path()), Some(&false));

        std::fs::create_dir_all(&paths.codex).unwrap();
        runtime.ensure_watches(&paths);
        assert_eq!(runtime.watched.get(&paths.codex), Some(&false));
    }

    #[test]
    fn recreated_provider_directory_replaces_the_stale_watch() {
        let home = tempfile::tempdir().unwrap();
        let paths = SourcePaths {
            codex: home.path().join(".codex"),
            claude: home.path().join(".claude"),
            vibe: home.path().join(".vibe"),
        };
        std::fs::create_dir_all(paths.codex.join("sessions")).unwrap();
        let (_, receiver) = mpsc::channel();
        let mut runtime = Runtime {
            changes: Mutex::new(receiver),
            watcher: notify::recommended_watcher(|_| {}).ok(),
            watched: HashMap::new(),
        };

        runtime.ensure_watches(&paths);
        assert_eq!(
            runtime.watched.get(&paths.codex.join("sessions")),
            Some(&true)
        );
        std::fs::remove_dir(paths.codex.join("sessions")).unwrap();
        runtime.ensure_watches(&paths);
        assert!(!runtime.watched.contains_key(&paths.codex.join("sessions")));
        std::fs::create_dir(paths.codex.join("sessions")).unwrap();
        runtime.ensure_watches(&paths);

        assert_eq!(
            runtime.watched.get(&paths.codex.join("sessions")),
            Some(&true)
        );
    }

    #[test]
    fn bounded_lines_skip_an_oversized_record_without_losing_the_next_one() {
        let mut input = vec![b'x'; PROVIDER_LINE_LIMIT + 1];
        input.extend_from_slice(b"\nvalid\n");

        let lines = BoundedLines::new(std::io::Cursor::new(input)).collect::<Vec<_>>();

        assert_eq!(lines, vec!["valid"]);
    }

    #[test]
    fn provider_resume_materializes_a_vmux_session_before_navigation() {
        let cwd = tempfile::tempdir().unwrap();
        let cwd = cwd.path().canonicalize().unwrap();
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
                cwd: cwd.to_string_lossy().into_owned(),
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
                cwd: cwd.to_string_lossy().into_owned(),
            },
        });
        app.world_mut().trigger(UiInput {
            webview,
            payload: ResumeSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                cwd: cwd.to_string_lossy().into_owned(),
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

    #[test]
    fn provider_resume_rejects_a_missing_working_directory() {
        let mut app = App::new();
        app.init_resource::<ProviderCatalog>()
            .init_resource::<PendingImports>()
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_observer(open);
        app.world_mut()
            .resource_mut::<ProviderCatalog>()
            .sessions
            .push(ProviderSession {
                kind: "acp:codex".into(),
                sid: "missing".into(),
                agent: "codex".into(),
                cwd: "/vmux/provider/session/path/that/does/not/exist".into(),
                ..Default::default()
            });
        let stack = app.world_mut().spawn_empty().id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ResumeSession {
                kind: "acp:codex".into(),
                sid: "missing".into(),
                cwd: String::new(),
            },
        });

        assert!(
            app.world_mut()
                .resource_mut::<Messages<CreateRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn provider_sessions_keep_only_the_latest_duplicate_identity() {
        let sessions = SourceRead::deduplicated(vec![
            ProviderSession {
                kind: "acp:mistral-vibe".into(),
                sid: "same".into(),
                name: "old".into(),
                updated_at: 1,
                ..Default::default()
            },
            ProviderSession {
                kind: "acp:mistral-vibe".into(),
                sid: "same".into(),
                name: "new".into(),
                updated_at: 2,
                ..Default::default()
            },
        ]);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].name, "new");
    }
}
