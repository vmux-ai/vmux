use std::collections::{HashMap, HashSet, VecDeque};
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
use vmux_api::Id;
use vmux_api::chat::ResumeSession;
use vmux_api::open_target::{OpenTarget, PaneOpenMode};
use vmux_api::protocol::AcpProviderSession;
use vmux_ecs::page::{Activity, PageReady};
use vmux_connector::{
    ExternalId, ExternalLink, ExternalLinkIdentity, ExternalLinkIndex, ExternalScopeRef,
    ProviderKey, ResourceKind,
};
use vmux_ecs::service::{
    ServiceConnected, ServiceMessageAppExt, ServiceMessageSet, ServiceRequest,
};
use vmux_ecs::{
    ArchivedAt, CreatedAt, Operation, OperationKind, OperationState, PageOpenRequest,
    PageOpenTarget, RemoteUpdatedAt,
};
use vmux_layout::{TabLayoutSpawnContent, TabLayoutSpawnRequest};
use vmux_session::host::{SessionManagerView, SessionView, SessionViews};
use vmux_session::{
    AgentConversationTitle, AgentFamily, AgentId, ArchiveRequest, ArchiveState,
    ConversationAppendReceived, CreateRequest, Created, ProviderArchive, ProviderArchiveCompleted,
    ProviderArchiveWriteRequest, ProviderCatalog, ProviderSession, Route, Session,
    SessionDuplicate, SessionId, SnapshotReceived, TranscriptLoading, Transcripts,
    ProviderSessionTombstones,
};
use vmux_setting::AppSettings;
use vmux_tool::ToolInventoryChanged;

use super::acp::{AcpLaunch, AcpPackageChanged, registry::RegistryAgent};
use crate::session::{AcpSessionLink, AcpSessionLinks};

mod claude;
mod codex;
mod conversation;
mod vibe;

pub(super) fn add(app: &mut App) {
    if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
        app.add_plugins(vmux_ecs::OperationPlugin);
    }
    app.init_resource::<SourceSnapshot>()
        .init_resource::<AcpSnapshot>()
        .init_resource::<CatalogRequests>()
        .init_resource::<ProviderCatalog>()
        .init_resource::<ArchiveQueue>()
        .init_resource::<ArchiveOverlay>()
        .init_resource::<ActivationDirty>()
        .add_message::<ToolInventoryChanged>()
        .add_message::<ProviderConversationChanged>()
        .add_message::<ProviderImportRequest>()
        .add_service_message::<AcpCatalogReceived>()
        .add_observer(open)
        .add_observer(attach)
        .add_systems(
            Update,
            (
                request_catalogs,
                receive_catalogs.after(ServiceMessageSet),
                mark_activation_dirty,
                 import,
                activate,
                archive,
                finish_archives,
                watch,
                request_changed_conversations,
                scan,
                finish,
                project,
                associate,
                request_conversation_refreshes,
                queue_initial_conversations,
                queue_conversation_refreshes,
                finish_conversations,
            )
                .chain(),
        );
}

const SCAN_DEBOUNCE: Duration = Duration::from_millis(350);
const SCAN_MAX_LATENCY: Duration = Duration::from_secs(2);
const ACTIVITY_REFRESH_INTERVAL: Duration = Duration::from_millis(100);
const CATALOG_CONCURRENCY: usize = 2;
const CATALOG_REQUEST_TIMEOUT: Duration = Duration::from_secs(32);
const PROVIDER_READ_LIMIT: usize = 256 * 1024 * 1024;
const PROVIDER_LINE_LIMIT: usize = 2 * 1024 * 1024;

#[vmux_api::service_message(AcpSessionCatalog)]
struct AcpCatalogReceived {
    request_id: u64,
    agent_id: String,
    agent_name: String,
    supported: bool,
    sessions: Vec<AcpProviderSession>,
    error: Option<String>,
}

#[derive(Resource, Default)]
struct AcpSnapshot(HashMap<String, AcpSource>);

struct AcpSource {
    state: AcpSourceState,
    sessions: Vec<ProviderSession>,
}

#[derive(Resource)]
struct ActivationDirty(bool);

impl Default for ActivationDirty {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AcpSourceState {
    Unprobed,
    Pending,
    Supported,
    Unsupported,
    Failed,
}

impl AcpSnapshot {
    fn allows_fallback(&self, family: &AgentFamily) -> bool {
        for (agent, source) in &self.0 {
            if AgentFamily::of(agent) != *family {
                continue;
            }
            if source.state != AcpSourceState::Unsupported {
                return false;
            }
        }
        true
    }

    fn sessions(&self) -> impl Iterator<Item = &ProviderSession> {
        self.0.values().flat_map(|source| source.sessions.iter())
    }

    fn supports(&self, family: &AgentFamily) -> bool {
        self.0.iter().any(|(agent, source)| {
            AgentFamily::of(agent) == *family && source.state == AcpSourceState::Supported
        })
    }

    fn refresh_codex_activity(&mut self, activity: &codex::RolloutActivity) -> bool {
        let family = SourceKind::Codex.family();
        let mut refreshed = false;
        for (agent, source) in &mut self.0 {
            if AgentFamily::of(agent) != family || source.state != AcpSourceState::Supported {
                continue;
            }
            refreshed |= activity.refresh(&mut source.sessions);
        }
        refreshed
    }
}

#[derive(Resource, Default)]
struct CatalogRequests {
    active: bool,
    refresh: bool,
    generation: u64,
    next_request_id: u64,
    pending: HashMap<u64, CatalogRequest>,
    queued: VecDeque<CatalogQueued>,
}

struct CatalogRequest {
    generation: u64,
    agent_id: String,
    started_at: Option<Instant>,
}

struct CatalogQueued {
    request_id: u64,
    request: CatalogRequest,
    message: vmux_api::protocol::ClientMessage,
}

struct CatalogLaunch {
    agent_id: String,
    message: vmux_api::protocol::ClientMessage,
}

impl CatalogRequests {
    fn request_id(&mut self) -> u64 {
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        self.next_request_id
    }

    fn launch(&mut self) -> Vec<CatalogLaunch> {
        let available = CATALOG_CONCURRENCY.saturating_sub(self.pending.len());
        let mut launched = Vec::with_capacity(available);
        for _ in 0..available {
            let Some(queued) = self.queued.pop_front() else {
                break;
            };
            let agent_id = queued.request.agent_id.clone();
            let mut request = queued.request;
            request.started_at = Some(Instant::now());
            self.pending.insert(queued.request_id, request);
            launched.push(CatalogLaunch {
                agent_id,
                message: queued.message,
            });
        }
        launched
    }

    fn cancel(&mut self) -> (Vec<u64>, HashSet<String>) {
        let mut request_ids = Vec::with_capacity(self.pending.len());
        let mut agents = HashSet::new();
        for (request_id, request) in self.pending.drain() {
            request_ids.push(request_id);
            agents.insert(request.agent_id);
        }
        self.queued.clear();
        (request_ids, agents)
    }
}

struct CatalogCwd;

impl CatalogCwd {
    fn current() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .or_else(|| std::env::current_dir().ok().filter(|path| path.is_dir()))
    }
}

fn request_catalogs(
    activity: Activity<SessionManagerView>,
    connected: Query<(), With<ServiceConnected>>,
    agents: Query<Ref<RegistryAgent>>,
    settings: Option<Res<AppSettings>>,
    mut package_changes: MessageReader<AcpPackageChanged>,
    mut tool_changes: MessageReader<ToolInventoryChanged>,
    mut snapshot: ResMut<AcpSnapshot>,
    mut state: ResMut<CatalogRequests>,
    mut requests: MessageWriter<ServiceRequest>,
) {
    let package_changed = package_changes.read().next().is_some();
    let tool_changed = tool_changes.read().any(|changed| changed.0.is("acp"));
    let registry_changed = agents.iter().any(|agent| agent.is_changed());
    if !Activity::present(activity) {
        let (request_ids, launched) = state.cancel();
        for request_id in request_ids {
            requests.write(ServiceRequest(
                vmux_api::protocol::ClientMessage::CancelAcpSessionCatalog { request_id },
            ));
        }
        for agent in launched {
            if let Some(source) = snapshot.0.get_mut(&agent) {
                source.state = AcpSourceState::Failed;
            }
        }
        state.active = false;
        state.refresh = true;
        return;
    }
    if !state.active {
        state.active = true;
        state.refresh = true;
    }
    if package_changed || tool_changed || registry_changed {
        state.refresh = true;
    }
    if connected.is_empty() {
        if !state.pending.is_empty() || !state.queued.is_empty() {
            let (_, agents) = state.cancel();
            state.refresh = true;
            for agent in agents {
                if let Some(source) = snapshot.0.get_mut(&agent) {
                    source.state = AcpSourceState::Failed;
                }
            }
        }
        return;
    }
    let timed_out = state
        .pending
        .iter()
        .filter_map(|(request_id, request)| {
            request
                .started_at
                .is_some_and(|started| started.elapsed() >= CATALOG_REQUEST_TIMEOUT)
                .then_some(*request_id)
        })
        .collect::<Vec<_>>();
    for request_id in timed_out {
        if let Some(request) = state.pending.remove(&request_id) {
            requests.write(ServiceRequest(
                vmux_api::protocol::ClientMessage::CancelAcpSessionCatalog { request_id },
            ));
            if let Some(source) = snapshot.0.get_mut(&request.agent_id) {
                source.state = AcpSourceState::Failed;
            }
        }
    }
    if state.refresh && state.pending.is_empty() && state.queued.is_empty() {
        let Some(settings) = settings else {
            return;
        };
        let Some(cwd) = CatalogCwd::current() else {
            return;
        };
        state.refresh = false;
        state.generation = state.generation.wrapping_add(1).max(1);
        let generation = state.generation;
        let shell = settings.shell();
        let mut installed = HashSet::new();
        let mut installed_families = HashSet::new();
        for agent in &agents {
            if !agent.is_installed() {
                continue;
            }
            installed.insert(agent.id.clone());
            installed_families.insert(AgentFamily::of(&agent.id));
            let source = snapshot.0.entry(agent.id.clone()).or_insert(AcpSource {
                state: AcpSourceState::Unprobed,
                sessions: Vec::new(),
            });
            let launch = match AcpLaunch::installed_catalog(&agent, &shell) {
                Ok(launch) => launch,
                Err(error) => {
                    source.state = AcpSourceState::Failed;
                    bevy::log::warn!("failed to resolve installed ACP {}: {error}", agent.id);
                    continue;
                }
            };
            let request_id = state.request_id();
            state.queued.push_back(CatalogQueued {
                request_id,
                request: CatalogRequest {
                    generation,
                    agent_id: agent.id.clone(),
                    started_at: None,
                },
                message: launch.catalog_message(request_id, &agent.id, &agent.name, &cwd),
            });
        }
        for config in &settings.agent.acp {
            if config.command.trim().is_empty()
                || installed.contains(&config.id)
                || installed_families.contains(&AgentFamily::of(&config.id))
            {
                continue;
            }
            let launch = match AcpLaunch::configured_catalog(config, &shell) {
                Ok(launch) => launch,
                Err(error) => {
                    bevy::log::debug!("configured ACP catalog skipped for {}: {error}", config.id);
                    continue;
                }
            };
            installed.insert(config.id.clone());
            snapshot.0.entry(config.id.clone()).or_insert(AcpSource {
                state: AcpSourceState::Unprobed,
                sessions: Vec::new(),
            });
            let request_id = state.request_id();
            let configured_cwd = config
                .cwd
                .as_deref()
                .filter(|path| path.is_dir())
                .unwrap_or(&cwd);
            state.queued.push_back(CatalogQueued {
                request_id,
                request: CatalogRequest {
                    generation,
                    agent_id: config.id.clone(),
                    started_at: None,
                },
                message: launch.catalog_message(
                    request_id,
                    &config.id,
                    &config.name,
                    configured_cwd,
                ),
            });
        }
        snapshot.0.retain(|agent, _| installed.contains(agent));
    }
    for launch in state.launch() {
        if let Some(source) = snapshot.0.get_mut(&launch.agent_id) {
            source.state = AcpSourceState::Pending;
        }
        requests.write(ServiceRequest(launch.message));
    }
}

fn receive_catalogs(
    mut received: MessageReader<AcpCatalogReceived>,
    mut snapshot: ResMut<AcpSnapshot>,
    mut compatibility: ResMut<SourceSnapshot>,
    mut state: ResMut<CatalogRequests>,
) {
    for received in received.read() {
        let Some(request) = state.pending.remove(&received.request_id) else {
            continue;
        };
        if request.generation != state.generation || request.agent_id != received.agent_id {
            continue;
        }
        let source = snapshot
            .0
            .entry(received.agent_id.clone())
            .or_insert(AcpSource {
                state: AcpSourceState::Unprobed,
                sessions: Vec::new(),
            });
        if let Some(error) = &received.error {
            source.state = AcpSourceState::Failed;
            bevy::log::warn!(
                "ACP session catalog failed for {}: {error}",
                received.agent_id
            );
            continue;
        }
        if !received.supported {
            source.state = AcpSourceState::Unsupported;
            source.sessions.clear();
        } else {
            source.state = AcpSourceState::Supported;
            source.sessions = AcpCatalog::sessions(received);
        }
        let family = AgentFamily::of(&received.agent_id);
        if source.state == AcpSourceState::Supported {
            compatibility
                .0
                .retain(|source, _| source.family() != family);
        }
    }
}

struct AcpCatalog;

impl AcpCatalog {
    fn sessions(received: &AcpCatalogReceived) -> Vec<ProviderSession> {
        let mut sessions = received
            .sessions
            .iter()
            .filter(|session| !session.id.trim().is_empty())
            .map(|session| {
                let updated_at = Timestamp::parse(&session.updated_at);
                let name = if session.title.trim().is_empty() {
                    session.id.clone()
                } else {
                    session.title.clone()
                };
                ProviderSession {
                    kind: format!("acp:{}", received.agent_id),
                    sid: session.id.clone(),
                    agent: received.agent_id.clone(),
                    agent_name: received.agent_name.clone(),
                    name: name.clone(),
                    description: String::new(),
                    cwd: session.cwd.clone(),
                    created_at: updated_at,
                    updated_at,
                    conversation_title: name,
                    last_prompt: String::new(),
                    archive: None,
                }
            })
            .collect::<Vec<_>>();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
        sessions
    }
}

#[derive(Resource, Default)]
struct SourceSnapshot(HashMap<SourceKind, Vec<ProviderSession>>);

impl SourceSnapshot {
    fn get(&self, kind: &str, sid: &str) -> Option<&ProviderSession> {
        self.0
            .values()
            .flatten()
            .find(|session| session.kind == kind && session.sid == sid)
    }

    fn association(
        &self,
        agent: &str,
        resume: &str,
        name: &str,
        cwd: &str,
        created_at: i64,
    ) -> Option<&ProviderSession> {
        let family = AgentFamily::of(agent);
        if let Some(session) = self
            .0
            .values()
            .flatten()
            .find(|session| AgentFamily::of(&session.agent) == family && session.sid == resume)
        {
            return Some(session);
        }
        let mut fallback = None;
        let mut matches = 0;
        for session in self.0.values().flatten() {
            if AgentFamily::of(&session.agent) != family
                || session.cwd != cwd
                || session.created_at != created_at
            {
                continue;
            }
            if session.name == name || session.conversation_title == name {
                return Some(session);
            }
            matches += 1;
            fallback = Some(session);
        }
        if matches == 1 { fallback } else { None }
    }
}

struct ProviderSessionIndex<'a>(HashMap<(&'a str, &'a str), &'a ProviderSession>);

impl<'a> ProviderSessionIndex<'a> {
    fn from_snapshots(snapshot: &'a SourceSnapshot, acp: &'a AcpSnapshot) -> Self {
        let mut sessions = HashMap::new();
        for session in snapshot.0.values().flatten() {
            sessions.insert((session.kind.as_str(), session.sid.as_str()), session);
        }
        for session in acp.sessions() {
            sessions.insert((session.kind.as_str(), session.sid.as_str()), session);
        }
        Self(sessions)
    }

    fn get(&self, kind: &str, sid: &str) -> Option<&'a ProviderSession> {
        self.0.get(&(kind, sid)).copied()
    }
}

#[derive(Component)]
struct PendingImport {
    session_id: SessionId,
    destination: ResumeDestination,
    deferred_open: bool,
    source: ProviderSession,
    agent: AgentId,
}

#[derive(Message, Clone)]
struct ProviderImportRequest {
    destination: ResumeDestination,
    source: ProviderSession,
    agent: AgentId,
}

#[derive(Clone, Copy)]
struct ResumeDestination {
    stack: Entity,
    pane: Option<Entity>,
    space: Option<Entity>,
    window: Option<Entity>,
    open: OpenTarget,
}

#[derive(SystemParam)]
struct ResumeOpener<'w> {
    opens: MessageWriter<'w, PageOpenRequest>,
    tabs: MessageWriter<'w, TabLayoutSpawnRequest>,
}

impl ResumeOpener<'_> {
    fn open(&mut self, destination: ResumeDestination, url: String) {
        if destination.open == OpenTarget::InNewTab
            && let (Some(space), Some(window)) = (destination.space, destination.window)
        {
            self.tabs.write(TabLayoutSpawnRequest {
                space,
                primary_window: window,
                name: None,
                startup_dir: None,
                content: TabLayoutSpawnContent::Url {
                    url,
                    pending_prompt: None,
                },
                clear_pending_stack: true,
                focus: true,
            });
            return;
        }
        let target = match destination.open {
            OpenTarget::InNewStack => destination
                .pane
                .map(PageOpenTarget::NewStackInPane)
                .unwrap_or(PageOpenTarget::NewStack),
            OpenTarget::InPane {
                mode: PaneOpenMode::NewStack,
                ..
            } => destination
                .pane
                .map(PageOpenTarget::NewStackInPane)
                .unwrap_or(PageOpenTarget::NewStack),
            OpenTarget::InPlace
            | OpenTarget::InPane {
                mode: PaneOpenMode::InPlace,
                ..
            }
            | OpenTarget::InNewTab
            | OpenTarget::InNewSpace => PageOpenTarget::Stack(destination.stack),
        };
        self.opens.write(PageOpenRequest {
            target,
            url,
            request_id: None,
        });
    }
}

impl ResumeDestination {
    fn opens_in_place(self) -> bool {
        matches!(
            self.open,
            OpenTarget::InPlace
                | OpenTarget::InPane {
                    mode: PaneOpenMode::InPlace,
                    ..
                }
        )
    }
}

#[derive(Component)]
struct Runtime {
    changes: Mutex<mpsc::Receiver<SourceChanges>>,
    watcher: Option<RecommendedWatcher>,
    watched: HashMap<PathBuf, bool>,
    sources: HashSet<SourceKind>,
    watch_codex_activity: bool,
}

impl Runtime {
    fn ensure_watches(&mut self, paths: &SourcePaths) {
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        let desired = paths.watches(&self.sources, self.watch_codex_activity);
        let stale = self
            .watched
            .keys()
            .filter(|path| !path.exists() || !desired.iter().any(|(desired, _)| desired == *path))
            .cloned()
            .collect::<Vec<_>>();
        for path in stale {
            self.watched.remove(&path);
            let _ = watcher.unwatch(&path);
        }
        for (target, recursive) in desired {
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

impl ScanRequested {
    fn one(source: SourceKind) -> Self {
        Self(HashSet::from([source]))
    }
}

#[derive(Component)]
struct ScanTask {
    started_at: Instant,
    task: Task<Vec<SourceRead>>,
}

#[derive(Component)]
struct ArchiveTask {
    request: ProviderArchiveWriteRequest,
    task: Task<Result<ArchiveState, String>>,
}

impl ArchiveTask {
    fn spawn(
        request: ProviderArchiveWriteRequest,
        root: PathBuf,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> Self {
        let sid = request.identity.external_id().as_str().to_string();
        let archived = request.archived;
        let task = IoTaskPool::get().spawn(async move {
            let result = codex::Reader::at(&root).set_archive(&sid, archived);
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        Self { request, task }
    }
}

#[derive(Resource, Default)]
struct ArchiveQueue(VecDeque<ProviderArchiveWriteRequest>);

#[derive(Resource, Default)]
struct ArchiveOverlay(HashMap<SourceKind, HashMap<String, ArchiveOverlayEntry>>);

struct ArchiveOverlayEntry {
    state: ArchiveState,
    completed_at: Instant,
}

impl ArchiveOverlay {
    fn record(&mut self, source: SourceKind, id: String, state: ArchiveState) {
        self.0.entry(source).or_default().insert(
            id,
            ArchiveOverlayEntry {
                state,
                completed_at: Instant::now(),
            },
        );
    }

    fn apply(
        &mut self,
        source: SourceKind,
        scan_started_at: Instant,
        sessions: &mut [ProviderSession],
    ) {
        let Some(entries) = self.0.get_mut(&source) else {
            return;
        };
        for session in sessions {
            let Some(entry) = entries.get(&session.sid) else {
                continue;
            };
            if entry.completed_at > scan_started_at {
                session.archive = Some(entry.state);
            }
        }
        entries.retain(|_, entry| entry.completed_at > scan_started_at);
        if entries.is_empty() {
            self.0.remove(&source);
        }
    }
}

#[derive(Component)]
struct ConversationLoadPending;

#[derive(Component)]
struct ConversationRefreshRequested(i64);

#[derive(Component)]
struct ConversationRevision(i64);

#[derive(Component)]
struct ConversationCursor(u64);

#[derive(Component)]
struct ConversationLoadTask {
    session: Entity,
    sid: SessionId,
    revision: i64,
    task: Task<Result<ConversationRead, String>>,
}

struct ConversationRead {
    messages: Vec<vmux_api::session::Message>,
    cursor: u64,
    snapshot: bool,
    complete: bool,
}

struct ConversationSource {
    kind: String,
    sid: String,
    created_at: i64,
}

impl ConversationSource {
    fn new(kind: &str, sid: &str, created_at: i64) -> Option<Self> {
        (kind == "acp:codex").then(|| Self {
            kind: kind.to_string(),
            sid: sid.to_string(),
            created_at,
        })
    }

    fn read(
        self,
        paths: &SourcePaths,
        cursor: Option<u64>,
        known: Vec<vmux_api::session::Message>,
    ) -> Result<ConversationRead, String> {
        match self.kind.as_str() {
            "acp:codex" => {
                let read = codex::Reader::at(&paths.codex).conversation(
                    &self.sid,
                    self.created_at,
                    cursor,
                )?;
                let snapshot = cursor.is_none() && known.is_empty();
                let messages = if read.tail && !known.is_empty() {
                    KnownConversation(known).unseen(read.messages)
                } else {
                    read.messages
                };
                Ok(ConversationRead {
                    messages,
                    cursor: read.cursor,
                    snapshot,
                    complete: read.complete,
                })
            }
            _ => Ok(ConversationRead {
                messages: Vec::new(),
                cursor: cursor.unwrap_or_default(),
                snapshot: false,
                complete: true,
            }),
        }
    }
}

struct KnownConversation(Vec<vmux_api::session::Message>);

impl KnownConversation {
    const LIMIT: usize = 512;

    fn tail(messages: &[vmux_api::session::Message]) -> Vec<vmux_api::session::Message> {
        let start = messages.len().saturating_sub(Self::LIMIT);
        messages[start..].to_vec()
    }

    fn unseen(&self, incoming: Vec<vmux_api::session::Message>) -> Vec<vmux_api::session::Message> {
        let limit = self.0.len().min(incoming.len());
        for overlap in (1..=limit).rev() {
            if self.0[self.0.len() - overlap..] == incoming[..overlap] {
                return incoming[overlap..].to_vec();
            }
        }
        incoming
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SourceKind {
    Codex,
    Claude,
    Vibe,
}

impl SourceKind {
    const ALL: [Self; 3] = [Self::Codex, Self::Claude, Self::Vibe];

    fn family(self) -> AgentFamily {
        match self {
            Self::Codex => AgentFamily::of("codex"),
            Self::Claude => AgentFamily::of("claude"),
            Self::Vibe => AgentFamily::of("vibe"),
        }
    }

    fn for_family(family: &AgentFamily) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|source| source.family() == *family)
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

    fn watches(
        &self,
        sources: &HashSet<SourceKind>,
        watch_codex_activity: bool,
    ) -> Vec<(PathBuf, bool)> {
        let mut watches = Vec::new();
        if sources.contains(&SourceKind::Codex) || watch_codex_activity {
            watches.push((self.codex.clone(), false));
            watches.push((self.codex.join("sessions"), true));
        }
        if sources.contains(&SourceKind::Claude) {
            watches.push((self.claude.clone(), false));
        }
        if sources.contains(&SourceKind::Vibe) {
            watches.push((self.vibe.join("logs/session"), true));
        }
        watches
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
        let mut indexed = HashMap::<ExternalLinkIdentity, ProviderSession>::new();
        for session in sessions {
            let Some(key) = session.identity() else {
                continue;
            };
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
    consumed: usize,
}

impl<R: BufRead> BoundedLines<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            remaining: PROVIDER_READ_LIMIT,
            consumed: 0,
        }
    }

    fn consumed(&self) -> usize {
        self.consumed
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
            self.consumed = self.consumed.saturating_add(read);
            if bytes.len() > PROVIDER_LINE_LIMIT {
                let skipped = self.reader.skip_until(b'\n').ok()?;
                self.remaining = self.remaining.saturating_sub(skipped);
                self.consumed = self.consumed.saturating_add(skipped);
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
                    while let Some(remaining) = changes.max_latency().checked_sub(started.elapsed())
                    {
                        let timeout = changes.debounce().min(remaining);
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

#[derive(Message)]
struct ProviderConversationChanged {
    session: ExternalLinkIdentity,
    updated_at: i64,
}

impl ProviderConversationChanged {
    fn codex(activity: &codex::RolloutActivity) -> Self {
        Self {
            session: AcpSessionLink::identity("acp:codex", activity.sid()).unwrap(),
            updated_at: activity.updated_at(),
        }
    }
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

    fn debounce(&self) -> Duration {
        if self.codex_activity.is_empty() {
            SCAN_DEBOUNCE
        } else {
            ACTIVITY_REFRESH_INTERVAL
        }
    }

    fn max_latency(&self) -> Duration {
        if self.codex_activity.is_empty() {
            SCAN_MAX_LATENCY
        } else {
            ACTIVITY_REFRESH_INTERVAL
        }
    }
}

fn activate(
    manager_activity: Activity<SessionManagerView>,
    session_activity: Activity<SessionView>,
    acp: Res<AcpSnapshot>,
    view_targets: SessionViews,
    views: Query<Entity, With<SessionView>>,
    mut dirty: ResMut<ActivationDirty>,
    sessions: Query<(), With<Session>>,
    links: Query<(&ChildOf, &ProviderKey, &ExternalScopeRef, &ResourceKind), With<ExternalLink>>,
    mut runtime: Query<(Entity, &mut Runtime, &SourcePaths)>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;
    let manager = Activity::present(manager_activity);
    let session = Activity::present(session_activity);
    let available = SourceKind::ALL
        .into_iter()
        .filter(|source| acp.allows_fallback(&source.family()))
        .collect::<HashSet<_>>();
    let codex_family = SourceKind::Codex.family();
    let viewed = if session {
        views
            .iter()
            .filter_map(|view| view_targets.session(view))
            .collect::<HashSet<_>>()
    } else {
        HashSet::new()
    };
    let watch_codex_activity = manager && acp.supports(&SourceKind::Codex.family());
    let mut sources = if manager {
        available.clone()
    } else {
        HashSet::new()
    };
    let mut watch_codex_activity = watch_codex_activity;
    if !manager {
        for (owner, provider, scope, kind) in &links {
            if provider.as_str() != "acp"
                || kind.as_str() != "session"
                || !sessions.contains(owner.parent())
            {
                continue;
            }
            let Some(agent) = scope.id().as_str().strip_prefix("acp:") else {
                continue;
            };
            let entity = owner.parent();
            let family = AgentFamily::of(agent);
            if viewed.contains(&entity) && family == codex_family && acp.supports(&family) {
                watch_codex_activity = true;
            }
            let Some(source) = SourceKind::for_family(&family) else {
                continue;
            };
            if available.contains(&source) {
                sources.insert(source);
            }
        }
    }
    if sources.is_empty() && !watch_codex_activity {
        for (entity, _, _) in &mut runtime {
            commands.entity(entity).despawn();
        }
        return;
    }
    if let Ok((entity, mut runtime, paths)) = runtime.single_mut() {
        if runtime.sources != sources || runtime.watch_codex_activity != watch_codex_activity {
            let requested = sources
                .difference(&runtime.sources)
                .copied()
                .collect::<HashSet<_>>();
            runtime.sources = sources;
            runtime.watch_codex_activity = watch_codex_activity;
            runtime.ensure_watches(paths);
            if !requested.is_empty() {
                commands.entity(entity).insert(ScanRequested(requested));
            }
        }
        return;
    }
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
        sources: sources.clone(),
        watch_codex_activity,
    };
    runtime.ensure_watches(&paths);
    let mut entity = commands.spawn((Name::new("ACP provider session catalog"), runtime, paths));
    if !sources.is_empty() {
        entity.insert(ScanRequested(sources));
    }
}

fn mark_activation_dirty(
    acp: Res<AcpSnapshot>,
    managers: Query<
        (),
        (
            With<SessionManagerView>,
            Or<(Added<SessionManagerView>, Added<PageReady>)>,
        ),
    >,
    views: Query<
        (),
        (
            With<SessionView>,
            Or<(Added<SessionView>, Added<PageReady>, Changed<ChildOf>)>,
        ),
    >,
    sessions: Query<
        (),
        (With<Session>, Added<Session>),
    >,
    links: Query<
        (),
        (
            With<ExternalLink>,
            Or<(
                Added<ExternalLink>,
                Changed<ProviderKey>,
                Changed<ExternalScopeRef>,
                Changed<ResourceKind>,
                Changed<ExternalId>,
                Changed<ChildOf>,
            )>,
        ),
    >,
    targets: Query<(), Changed<vmux_ecs::EntityTarget<Session>>>,
    mut removed_managers: RemovedComponents<SessionManagerView>,
    mut removed_views: RemovedComponents<SessionView>,
    mut removed_ready: RemovedComponents<PageReady>,
    mut removed_sessions: RemovedComponents<Session>,
    mut removed_links: RemovedComponents<ExternalLink>,
    mut removed_providers: RemovedComponents<ProviderKey>,
    mut removed_scopes: RemovedComponents<ExternalScopeRef>,
    mut removed_kinds: RemovedComponents<ResourceKind>,
    mut removed_ids: RemovedComponents<ExternalId>,
    mut removed_owners: RemovedComponents<ChildOf>,
    mut removed_targets: RemovedComponents<vmux_ecs::EntityTarget<Session>>,
    mut dirty: ResMut<ActivationDirty>,
) {
    let changed = acp.is_changed()
        || !managers.is_empty()
        || !views.is_empty()
        || !sessions.is_empty()
        || !links.is_empty()
        || !targets.is_empty()
        || removed_managers.read().next().is_some()
        || removed_views.read().next().is_some()
        || removed_ready.read().next().is_some()
        || removed_sessions.read().next().is_some()
        || removed_links.read().next().is_some()
        || removed_providers.read().next().is_some()
        || removed_scopes.read().next().is_some()
        || removed_kinds.read().next().is_some()
        || removed_ids.read().next().is_some()
        || removed_owners.read().next().is_some()
        || removed_targets.read().next().is_some();
    if changed {
        dirty.0 = true;
    }
}

fn watch(
    runtime: Single<(Entity, &mut Runtime, &SourcePaths)>,
    mut requested: Query<&mut ScanRequested>,
    mut snapshot: ResMut<SourceSnapshot>,
    mut acp: ResMut<AcpSnapshot>,
    mut catalogs: ResMut<CatalogRequests>,
    mut conversations: MessageWriter<ProviderConversationChanged>,
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
    changes
        .scans
        .retain(|source| runtime.sources.contains(source));
    if !runtime.sources.contains(&SourceKind::Codex) && !runtime.watch_codex_activity {
        changes.codex_activity.clear();
    }
    runtime.ensure_watches(paths);
    for path in changes.codex_activity {
        let Some(activity) = codex::RolloutActivity::read(&path) else {
            continue;
        };
        conversations.write(ProviderConversationChanged::codex(&activity));
        if runtime.watch_codex_activity {
            if !acp.refresh_codex_activity(&activity) {
                catalogs.refresh = true;
            }
            continue;
        }
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

fn request_changed_conversations(
    mut changed: MessageReader<ProviderConversationChanged>,
    links: Query<
        (
            &ChildOf,
            &ProviderKey,
            &ExternalScopeRef,
            &ResourceKind,
            &ExternalId,
        ),
        With<ExternalLink>,
    >,
    mut sessions: Query<
        (
            &mut vmux_ecs::LastActivatedAt,
            Option<&ConversationRevision>,
            Option<&mut ConversationRefreshRequested>,
        ),
        (With<Session>, Without<SessionDuplicate>),
    >,
    mut commands: Commands,
) {
    for changed in changed.read() {
        let session = links.iter().find_map(|(owner, provider, scope, kind, id)| {
            let identity = ExternalLinkIdentity::new(
                provider.clone(),
                scope.id().clone(),
                kind.clone(),
                id.clone(),
            );
            (identity == changed.session).then_some(owner.parent())
        });
        let Some(session) = session else {
            continue;
        };
        let Ok((mut activated_at, revision, requested)) = sessions.get_mut(session) else {
            continue;
        };
        activated_at.0 = activated_at.0.max(changed.updated_at);
        if revision.is_some_and(|revision| revision.0 >= changed.updated_at) {
            continue;
        }
        if let Some(mut requested) = requested {
            requested.0 = requested.0.max(changed.updated_at);
        } else {
            commands
                .entity(session)
                .insert(ConversationRefreshRequested(changed.updated_at));
        }
    }
}

fn archive(
    paths: Single<&SourcePaths>,
    mut requests: MessageReader<ProviderArchiveWriteRequest>,
    tasks: Query<(), With<ArchiveTask>>,
    mut queue: ResMut<ArchiveQueue>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut completed: MessageWriter<ProviderArchiveCompleted>,
    mut commands: Commands,
) {
    for request in requests.read().cloned() {
        if request.identity.scope().as_str() != "acp:codex" {
            completed.write(ProviderArchiveCompleted {
                identity: request.identity,
                archived: request.archived,
                revision: request.revision,
                success: false,
            });
            continue;
        }
        queue.0.push_back(request);
    }
    if !tasks.is_empty() {
        return;
    }
    let Some(request) = queue.0.pop_front() else {
        return;
    };
    commands.spawn(ArchiveTask::spawn(
        request,
        paths.codex.clone(),
        proxy.as_deref().map(|proxy| (**proxy).clone()),
    ));
}

fn finish_archives(
    mut tasks: Query<(Entity, &mut ArchiveTask)>,
    mut snapshot: ResMut<SourceSnapshot>,
    mut overlay: ResMut<ArchiveOverlay>,
    runtime: Single<Entity, With<Runtime>>,
    mut requested: Query<&mut ScanRequested, With<Runtime>>,
    mut completed: MessageWriter<ProviderArchiveCompleted>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        let success = result.is_ok();
        match result {
            Ok(archive) => {
                let id = task.request.identity.external_id().as_str();
                overlay.record(SourceKind::Codex, id.to_string(), archive);
                if let Some(session) = snapshot
                    .0
                    .get_mut(&SourceKind::Codex)
                    .and_then(|sessions| sessions.iter_mut().find(|session| session.sid == id))
                {
                    session.archive = Some(archive);
                }
                if let Ok(mut requested) = requested.get_mut(*runtime) {
                    requested.0.insert(SourceKind::Codex);
                } else {
                    commands
                        .entity(*runtime)
                        .insert(ScanRequested::one(SourceKind::Codex));
                }
            }
            Err(error) => {
                bevy::log::warn!("failed to archive Codex session: {error}");
            }
        }
        completed.write(ProviderArchiveCompleted {
            identity: task.request.identity.clone(),
            archived: task.request.archived,
            revision: task.request.revision,
            success,
        });
        commands.entity(entity).despawn();
    }
}

fn scan(
    runtime: Single<
        (Entity, &Runtime, &SourcePaths, &ScanRequested),
        (With<Runtime>, Without<ScanTask>),
    >,
    snapshot: Res<SourceSnapshot>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let (entity, runtime, paths, requested) = *runtime;
    let requested = requested
        .0
        .intersection(&runtime.sources)
        .copied()
        .collect::<HashSet<_>>();
    if requested.is_empty() {
        commands.entity(entity).remove::<ScanRequested>();
        return;
    }
    let readers = paths.readers(&requested, &snapshot);
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let started_at = Instant::now();
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
        .insert(ScanTask { started_at, task });
}

fn finish(
    task: Single<(Entity, &mut ScanTask)>,
    mut snapshot: ResMut<SourceSnapshot>,
    mut overlay: ResMut<ArchiveOverlay>,
    mut commands: Commands,
) {
    let (entity, mut task) = task.into_inner();
    let Some(reads) = future::block_on(future::poll_once(&mut task.task)) else {
        return;
    };
    let mut next = snapshot.0.clone();
    for read in reads {
        match read.sessions {
            Ok(mut sessions) => {
                overlay.apply(read.source, task.started_at, &mut sessions);
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
    acp: Res<AcpSnapshot>,
    imported: Query<(Entity, &AgentId), (With<Session>, Without<SessionDuplicate>)>,
    changed: Query<
        (),
        (
            With<Session>,
            Or<(Changed<AgentId>, Changed<SessionDuplicate>)>,
        ),
    >,
    changed_links: Query<
        (),
        (
            With<ExternalLink>,
            Or<(
                Added<ExternalLink>,
                Changed<ProviderKey>,
                Changed<ExternalScopeRef>,
                Changed<ResourceKind>,
                Changed<ExternalId>,
                Changed<ChildOf>,
            )>,
        ),
    >,
    links: AcpSessionLinks,
    mut removed_links: RemovedComponents<ExternalLink>,
    mut removed_providers: RemovedComponents<ProviderKey>,
    mut removed_scopes: RemovedComponents<ExternalScopeRef>,
    mut removed_kinds: RemovedComponents<ResourceKind>,
    mut removed_ids: RemovedComponents<ExternalId>,
    mut removed_duplicates: RemovedComponents<SessionDuplicate>,
    mut catalog: ResMut<ProviderCatalog>,
) {
    let removed = removed_links.read().next().is_some()
        || removed_providers.read().next().is_some()
        || removed_scopes.read().next().is_some()
        || removed_kinds.read().next().is_some()
        || removed_ids.read().next().is_some()
        || removed_duplicates.read().next().is_some();
    if !snapshot.is_changed()
        && !acp.is_changed()
        && changed.is_empty()
        && changed_links.is_empty()
        && !removed
        && !catalog.is_added()
    {
        return;
    }
    let imported = imported
        .iter()
        .filter_map(|(entity, agent)| {
            let link = links.get(entity)?;
            let family = link
                .kind()
                .strip_prefix("acp:")
                .map(AgentFamily::of)
                .unwrap_or_else(|| AgentFamily::of(&agent.0));
            Some((family, link.id().as_str().to_string()))
        })
        .collect::<HashSet<_>>();
    let mut indexed = HashMap::<(AgentFamily, String), ProviderSession>::new();
    for session in acp.sessions() {
        let key = (AgentFamily::of(&session.agent), session.sid.clone());
        match indexed.get(&key) {
            Some(current) if current.updated_at > session.updated_at => {}
            _ => {
                indexed.insert(key, session.clone());
            }
        }
    }
    for source in SourceKind::ALL {
        if !acp.allows_fallback(&source.family()) {
            continue;
        }
        let Some(source_sessions) = snapshot.0.get(&source) else {
            continue;
        };
        for session in source_sessions {
            let key = (AgentFamily::of(&session.agent), session.sid.clone());
            match indexed.get(&key) {
                Some(current) if current.updated_at > session.updated_at => {}
                _ => {
                    indexed.insert(key, session.clone());
                }
            }
        }
    }
    let mut sessions = indexed
        .into_iter()
        .filter_map(|(key, session)| (!imported.contains(&key)).then_some(session))
        .collect::<Vec<_>>();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    if catalog.sessions != sessions {
        catalog.sessions = sessions;
    }
}

fn associate(
    snapshot: Res<SourceSnapshot>,
    sessions: Query<(Entity, &AgentId, &Name, &vmux_ecs::Cwd, &CreatedAt), With<Session>>,
    links: AcpSessionLinks,
    mut commands: Commands,
) {
    if snapshot.0.is_empty() {
        return;
    }
    for (entity, agent, name, cwd, created_at) in &sessions {
        let Some(link) = links.get(entity) else {
            continue;
        };
        let cwd = cwd.0.to_string_lossy();
        let Some(source) = snapshot.association(
            &agent.0,
            link.id().as_str(),
            name.as_str(),
            &cwd,
            created_at.0,
        ) else {
            continue;
        };
        links.set(
            &mut commands,
            entity,
            &source.kind,
            &source.sid,
            Some(source.updated_at),
        );
    }
}

fn request_conversation_refreshes(
    snapshot: Res<SourceSnapshot>,
    acp: Res<AcpSnapshot>,
    archive_overrides: Query<
        (
            &ProviderKey,
            &ExternalScopeRef,
            &ResourceKind,
            &ExternalId,
            &RemoteUpdatedAt,
            Option<&ArchivedAt>,
        ),
        With<ProviderArchive>,
    >,
    mut sessions: Query<
        (
            Entity,
            &SessionId,
            &mut vmux_ecs::LastActivatedAt,
            Option<&ArchivedAt>,
            Option<&ConversationRevision>,
            Option<&mut ConversationRefreshRequested>,
        ),
        (With<Session>, Without<SessionDuplicate>),
    >,
    links: AcpSessionLinks,
    mut archive_requests: MessageWriter<ArchiveRequest>,
    mut commands: Commands,
) {
    if !snapshot.is_changed() && !acp.is_changed() {
        return;
    }
    let sources = ProviderSessionIndex::from_snapshots(&snapshot, &acp);
    let mut archive_overrides_by_session = HashMap::new();
    for (provider, scope, kind, id, source_updated_at, archived_at) in &archive_overrides {
        archive_overrides_by_session.insert(
            ExternalLinkIdentity::new(
                provider.clone(),
                scope.id().clone(),
                kind.clone(),
                id.clone(),
            ),
            (
                source_updated_at.0.0,
                archived_at.map(|archived_at| archived_at.0),
            ),
        );
    }
    for (session, session_id, mut activated_at, archived_at, revision, requested) in &mut sessions {
        let Some(link) = links.get(session) else {
            continue;
        };
        let Some(identity) = AcpSessionLink::identity(link.kind(), link.id().as_str()) else {
            continue;
        };
        let Some(source) = sources.get(link.kind(), link.id().as_str()) else {
            continue;
        };
        let archive_override = archive_overrides_by_session.get(&identity);
        let archive = match archive_override {
            Some((source_updated_at, override_archived_at))
                if source
                    .archive
                    .is_none_or(|archive| *source_updated_at >= archive.updated_at()) =>
            {
                Some(match override_archived_at {
                    Some(archived_at) => ArchiveState::Archived(*archived_at),
                    None => ArchiveState::Active(*source_updated_at),
                })
            }
            _ => source.archive,
        };
        match archive {
            Some(ArchiveState::Archived(at))
                if archived_at.is_none_or(|current| at.max(1) > current.0) =>
            {
                archive_requests.write(ArchiveRequest {
                    id: session_id.clone(),
                    archived_at: Some(at),
                });
            }
            Some(ArchiveState::Active(at)) if archived_at.is_some_and(|current| at > current.0) => {
                archive_requests.write(ArchiveRequest {
                    id: session_id.clone(),
                    archived_at: None,
                });
            }
            _ => {}
        }
        if activated_at.0 < source.updated_at {
            activated_at.0 = source.updated_at;
        }
        if ConversationSource::new(link.kind(), link.id().as_str(), source.created_at).is_none()
            || revision.is_some_and(|revision| revision.0 >= source.updated_at)
        {
            continue;
        }
        if let Some(mut requested) = requested {
            requested.0 = requested.0.max(source.updated_at);
        } else {
            commands
                .entity(session)
                .insert(ConversationRefreshRequested(source.updated_at));
        }
    }
}

fn queue_initial_conversations(
    paths: Single<&SourcePaths>,
    snapshot: Res<SourceSnapshot>,
    sessions: Query<
        (
            Entity,
            &SessionId,
            &CreatedAt,
            Option<&ConversationRefreshRequested>,
            Option<&ConversationCursor>,
        ),
        (
            With<Session>,
            With<TranscriptLoading>,
            Without<ConversationLoadPending>,
            Without<SessionDuplicate>,
        ),
    >,
    links: AcpSessionLinks,
    transcripts: Transcripts,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (session, sid, created_at, requested, cursor) in &sessions {
        if !transcripts.get(session).messages.is_empty() {
            commands.entity(session).remove::<TranscriptLoading>();
            continue;
        }
        let Some(link) = links.get(session) else {
            commands.entity(session).remove::<TranscriptLoading>();
            continue;
        };
        if snapshot.get(link.kind(), link.id().as_str()).is_none() {
            commands.entity(session).remove::<TranscriptLoading>();
            continue;
        }
        let Some(source) = ConversationSource::new(link.kind(), link.id().as_str(), created_at.0)
        else {
            commands.entity(session).remove::<TranscriptLoading>();
            continue;
        };
        let revision = requested.map_or_else(
            || {
                snapshot
                    .get(link.kind(), link.id().as_str())
                    .map_or(0, |source| source.updated_at)
            },
            |requested| requested.0,
        );
        let cursor = cursor.map(|cursor| cursor.0);
        let paths = paths.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = source.read(&paths, cursor, Vec::new());
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(session)
            .insert(ConversationLoadPending)
            .remove::<ConversationRefreshRequested>();
        commands.spawn(ConversationLoadTask {
            session,
            sid: sid.clone(),
            revision,
            task,
        });
    }
}

fn queue_conversation_refreshes(
    paths: Single<&SourcePaths>,
    sessions: Query<
        (
            Entity,
            &SessionId,
            &CreatedAt,
            &ConversationRefreshRequested,
            Option<&ConversationCursor>,
        ),
        (
            With<Session>,
            Without<TranscriptLoading>,
            Without<ConversationLoadPending>,
            Without<SessionDuplicate>,
        ),
    >,
    links: AcpSessionLinks,
    transcripts: Transcripts,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (session, sid, created_at, requested, cursor) in &sessions {
        let Some(link) = links.get(session) else {
            commands
                .entity(session)
                .remove::<ConversationRefreshRequested>();
            continue;
        };
        let Some(source) = ConversationSource::new(link.kind(), link.id().as_str(), created_at.0)
        else {
            commands
                .entity(session)
                .remove::<ConversationRefreshRequested>();
            continue;
        };
        let revision = requested.0;
        let cursor = cursor.map(|cursor| cursor.0);
        let known = KnownConversation::tail(&transcripts.get(session).messages);
        let paths = paths.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = source.read(&paths, cursor, known);
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(session)
            .insert(ConversationLoadPending)
            .remove::<ConversationRefreshRequested>();
        commands.spawn(ConversationLoadTask {
            session,
            sid: sid.clone(),
            revision,
            task,
        });
    }
}

fn finish_conversations(
    mut tasks: Query<(Entity, &mut ConversationLoadTask)>,
    mut snapshots: MessageWriter<SnapshotReceived>,
    mut appended: MessageWriter<ConversationAppendReceived>,
    mut commands: Commands,
) {
    for (task_entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        match result {
            Ok(read) => {
                if !read.messages.is_empty() {
                    if read.snapshot {
                        snapshots.write(SnapshotReceived {
                            session: task.sid.clone(),
                            messages: read.messages,
                        });
                    } else {
                        appended.write(ConversationAppendReceived {
                            session: task.sid.clone(),
                            messages: read.messages,
                        });
                    }
                }
                commands
                    .entity(task.session)
                    .insert(ConversationCursor(read.cursor));
                if !read.complete {
                    commands
                        .entity(task.session)
                        .insert(ConversationRefreshRequested(task.revision));
                }
            }
            Err(error) => {
                bevy::log::warn!("failed to read provider conversation: {error}");
            }
        }
        commands
            .entity(task.session)
            .insert(ConversationRevision(task.revision))
            .remove::<(ConversationLoadPending, TranscriptLoading)>();
        commands.entity(task_entity).despawn();
    }
}

fn open(
    trigger: On<UiInput<ResumeSession>>,
    parents: Query<&ChildOf>,
    panes: Query<(), With<vmux_layout::pane::Pane>>,
    spaces: vmux_layout::space::SpaceHierarchy,
    windows: vmux_layout::window::WindowHierarchy,
    catalog: Res<ProviderCatalog>,
    agents: AgentCatalog,
    tombstones: Res<ProviderSessionTombstones>,
    external_index: Option<Res<ExternalLinkIndex>>,
    link_owners: Query<&ChildOf, With<ExternalLink>>,
    existing: Query<&SessionId, (With<Session>, Without<SessionDuplicate>)>,
    mut opener: ResumeOpener,
    mut imports: MessageWriter<ProviderImportRequest>,
) {
    let request = &trigger.event().payload;
    if !request.kind.starts_with("acp:") {
        return;
    }
    let Some(mut source) = catalog.get(&request.kind, &request.sid).cloned() else {
        return;
    };
    let Some(identity) = source.identity() else {
        return;
    };
    if tombstones.contains(&identity) {
        return;
    }
    let Ok(parent) = parents.get(trigger.event().webview) else {
        return;
    };
    let stack = parent.parent();
    let destination = ResumeDestination {
        stack,
        pane: parents
            .get(stack)
            .ok()
            .map(ChildOf::parent)
            .filter(|pane| panes.contains(*pane)),
        space: spaces.get(trigger.event().webview),
        window: windows.get(trigger.event().webview),
        open: request.open.unwrap_or_default(),
    };
    let existing_id = external_index.as_ref().and_then(|index| {
        let link = index.entity(&identity)?;
        let owner = link_owners.get(link).ok()?.parent();
        existing.get(owner).ok().cloned()
    });
    if let Some(id) = existing_id {
        opener.open(destination, Route::Session(id).url());
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
    source.name = name;
    imports.write(ProviderImportRequest {
        destination,
        source,
        agent,
    });
}

fn import(
    mut requests: MessageReader<ProviderImportRequest>,
    mut pending: Query<(&mut PendingImport, &OperationState), With<Operation>>,
    mut creates: MessageWriter<CreateRequest>,
    mut opener: ResumeOpener,
    mut commands: Commands,
) {
    let mut latest = HashMap::<(AgentFamily, String), ProviderImportRequest>::new();
    for request in requests.read().cloned() {
        latest.insert(
            (
                AgentFamily::of(&request.source.agent),
                request.source.sid.clone(),
            ),
            request,
        );
    }
    for request in latest.into_values() {
        let ProviderImportRequest {
            destination,
            source,
            agent,
        } = request;
        let deferred_open = !destination.opens_in_place();
        let mut reserved = false;
        for (mut pending, state) in &mut pending {
            if matches!(state, OperationState::Pending | OperationState::Running)
                && AgentFamily::of(&pending.source.agent) == AgentFamily::of(&source.agent)
                && pending.source.sid == source.sid
            {
                let open_now = pending.deferred_open && !deferred_open;
                pending.destination = destination;
                pending.deferred_open = deferred_open;
                if open_now {
                    opener.open(
                        destination,
                        Route::Session(pending.session_id.clone()).url(),
                    );
                }
                reserved = true;
                break;
            }
        }
        if reserved {
            continue;
        }
        let cwd = PathBuf::from(&source.cwd);
        let create = CreateRequest::imported(
            source.name.clone(),
            source.description.clone(),
            cwd,
            agent.clone(),
            source.created_at,
            source.updated_at,
        );
        commands.spawn((
            Operation,
            Id::<Operation>::new(uuid::Uuid::new_v4().to_string()),
            OperationKind::new("provider_session_import").unwrap(),
            OperationState::Running,
            PendingImport {
                session_id: create.id().clone(),
                destination,
                deferred_open,
                source,
                agent,
            },
        ));
        if !deferred_open {
            opener.open(destination, Route::Session(create.id().clone()).url());
        }
        creates.write(create);
    }
}

fn attach(
    trigger: On<Created>,
    mut opener: ResumeOpener,
    mut pending: Query<(Entity, &PendingImport, &mut OperationState), With<Operation>>,
    links: AcpSessionLinks,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Some((operation, import, mut state)) = pending.iter_mut().find(|(_, pending, state)| {
        pending.session_id == event.id
            && matches!(**state, OperationState::Pending | OperationState::Running)
    }) else {
        return;
    };
    let title = if import.source.conversation_title.trim().is_empty() {
        import.source.name.clone()
    } else {
        import.source.conversation_title.clone()
    };
    commands
        .entity(event.entity)
        .insert((import.agent.clone(), AgentConversationTitle(title)));
    links.set(
        &mut commands,
        event.entity,
        &import.source.kind,
        &import.source.sid,
        Some(import.source.updated_at),
    );
    if import.deferred_open {
        opener.open(import.destination, Route::Session(event.id.clone()).url());
    }
    *state = OperationState::Succeeded;
    commands.entity(operation).remove::<PendingImport>();
}

#[derive(SystemParam)]
struct AgentCatalog<'w, 's> {
    settings: Res<'w, AppSettings>,
    registry: Query<'w, 's, &'static RegistryAgent>,
}

impl AgentCatalog<'_, '_> {
    fn resolve(&self, source: &str) -> AgentId {
        let family = AgentFamily::of(source);
        if let Some(agent) = self
            .registry
            .iter()
            .find(|agent| agent.id == source && agent.is_installed())
        {
            return AgentId(agent.id.clone());
        }
        if let Some(config) = self
            .settings
            .agent
            .acp
            .iter()
            .find(|config| config.id == source)
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

    struct ProviderFixture;

    impl ProviderFixture {
        fn identity(kind: &str, id: &str) -> ExternalLinkIdentity {
            AcpSessionLink::identity(kind, id).unwrap()
        }

        fn components(
            kind: &str,
            id: &str,
        ) -> (ProviderKey, ExternalScopeRef, ResourceKind, ExternalId) {
            let identity = Self::identity(kind, id);
            (
                identity.provider().clone(),
                ExternalScopeRef::new(identity.scope().clone()),
                identity.kind().clone(),
                identity.external_id().clone(),
            )
        }

        fn spawn_link(world: &mut World, owner: Entity, kind: &str, id: &str) -> Entity {
            world
                .spawn((Self::components(kind, id), ExternalLink, ChildOf(owner)))
                .id()
        }

        fn link(world: &World, owner: Entity) -> Option<(String, String)> {
            let children = world.get::<Children>(owner)?;
            for child in children.iter() {
                let Some(provider) = world.get::<ProviderKey>(child) else {
                    continue;
                };
                let Some(scope) = world.get::<ExternalScopeRef>(child) else {
                    continue;
                };
                let Some(kind) = world.get::<ResourceKind>(child) else {
                    continue;
                };
                let Some(id) = world.get::<ExternalId>(child) else {
                    continue;
                };
                if provider.as_str() == "acp" && kind.as_str() == "session" {
                    return Some((scope.id().as_str().to_string(), id.as_str().to_string()));
                }
            }
            None
        }
    }

    #[test]
    fn provider_store_fallback_requires_an_explicitly_unsupported_catalog() {
        let family = AgentFamily::of("codex");
        let mut snapshot = AcpSnapshot::default();
        assert!(snapshot.allows_fallback(&family));

        snapshot.0.insert(
            "codex-acp".into(),
            AcpSource {
                state: AcpSourceState::Pending,
                sessions: Vec::new(),
            },
        );
        assert!(!snapshot.allows_fallback(&family));

        snapshot.0.get_mut("codex-acp").unwrap().state = AcpSourceState::Failed;
        assert!(!snapshot.allows_fallback(&family));

        snapshot.0.get_mut("codex-acp").unwrap().state = AcpSourceState::Unsupported;
        assert!(snapshot.allows_fallback(&family));
    }

    #[test]
    fn acp_catalog_normalizes_provider_sessions() {
        let received = AcpCatalogReceived {
            request_id: 1,
            agent_id: "example-acp".into(),
            agent_name: "Example".into(),
            supported: true,
            sessions: vec![AcpProviderSession {
                id: "provider-session".into(),
                cwd: "/work/example".into(),
                title: "Review changes".into(),
                updated_at: "1970-01-01T00:00:01Z".into(),
            }],
            error: None,
        };

        assert_eq!(
            AcpCatalog::sessions(&received),
            vec![ProviderSession {
                kind: "acp:example-acp".into(),
                sid: "provider-session".into(),
                agent: "example-acp".into(),
                agent_name: "Example".into(),
                name: "Review changes".into(),
                cwd: "/work/example".into(),
                created_at: 1_000,
                updated_at: 1_000,
                conversation_title: "Review changes".into(),
                ..Default::default()
            }]
        );
    }

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
            sources: SourceKind::ALL.into_iter().collect(),
            watch_codex_activity: false,
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
            sources: SourceKind::ALL.into_iter().collect(),
            watch_codex_activity: false,
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
    fn provider_resume_opens_the_pending_session_before_materialization() {
        let cwd = tempfile::tempdir().unwrap();
        let cwd = cwd.path().canonicalize().unwrap();
        let mut app = App::new();
        app.add_plugins((vmux_ecs::OperationPlugin, vmux_connector::ConnectorPlugin))
            .init_resource::<ProviderCatalog>()
            .init_resource::<ProviderSessionTombstones>()
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_message::<ProviderImportRequest>()
            .add_observer(open)
            .add_observer(attach)
            .add_systems(Update, import);
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
                open: None,
            },
        });
        app.world_mut().trigger(UiInput {
            webview,
            payload: ResumeSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                cwd: cwd.to_string_lossy().into_owned(),
                open: None,
            },
        });
        app.update();

        let created = app
            .world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].agent, Some(AgentId("codex".into())));
        let id = created[0].id().clone();
        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].url, Route::Session(id.clone()).url());
        assert!(matches!(
            opened[0].target,
            PageOpenTarget::Stack(entity) if entity == stack
        ));
        let session = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(Created {
            entity: session,
            id: id.clone(),
        });
        app.world_mut().flush();

        assert_eq!(
            ProviderFixture::link(app.world(), session),
            Some(("acp:codex".into(), "provider-session".into()))
        );
        assert!(app.world().get::<ProviderKey>(session).is_none());
        assert!(app.world().get::<ExternalId>(session).is_none());
        assert!(
            app.world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn provider_resume_defers_new_stack_navigation_until_materialization() {
        let cwd = tempfile::tempdir().unwrap();
        let cwd = cwd.path().canonicalize().unwrap();
        let mut app = App::new();
        app.add_plugins((vmux_ecs::OperationPlugin, vmux_connector::ConnectorPlugin))
            .init_resource::<ProviderCatalog>()
            .init_resource::<ProviderSessionTombstones>()
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_message::<ProviderImportRequest>()
            .add_observer(open)
            .add_observer(attach)
            .add_systems(Update, import);
        app.world_mut()
            .resource_mut::<ProviderCatalog>()
            .sessions
            .push(ProviderSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                agent: "codex".into(),
                name: "Provider task".into(),
                cwd: cwd.to_string_lossy().into_owned(),
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
                open: Some(OpenTarget::InNewStack),
            },
        });

        assert!(
            app.world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .next()
                .is_none()
        );
        let [created] = app
            .world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .drain()
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let id = created.id().clone();
        let session = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(Created {
            entity: session,
            id: id.clone(),
        });
        app.world_mut().flush();
        assert_eq!(
            ProviderFixture::link(app.world(), session),
            Some(("acp:codex".into(), "provider-session".into()))
        );
        assert!(app.world().get::<ProviderKey>(session).is_none());
        assert!(app.world().get::<ExternalId>(session).is_none());
        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].url, Route::Session(id).url());
        assert!(matches!(opened[0].target, PageOpenTarget::NewStack));
    }

    #[test]
    fn provider_association_survives_a_changed_acp_resume_identity() {
        let mut app = App::new();
        app.init_resource::<SourceSnapshot>()
            .add_systems(Update, associate);
        app.world_mut()
            .resource_mut::<SourceSnapshot>()
            .0
            .entry(SourceKind::Codex)
            .or_default()
            .push(ProviderSession {
                kind: "acp:codex".into(),
                sid: "source-session".into(),
                agent: "codex".into(),
                name: "Review pull request".into(),
                cwd: "/work/dashboard".into(),
                created_at: 100,
                ..Default::default()
            });
        let session = app
            .world_mut()
            .spawn((
                Session,
                AgentId("codex".into()),
                Name::new("Review pull request"),
                vmux_ecs::Cwd(PathBuf::from("/work/dashboard")),
                CreatedAt(100),
            ))
            .id();
        ProviderFixture::spawn_link(app.world_mut(), session, "acp:codex", "replacement-session");

        app.update();

        assert_eq!(
            ProviderFixture::link(app.world(), session),
            Some(("acp:codex".into(), "source-session".into()))
        );
    }

    #[test]
    fn provider_resume_reuses_the_canonical_session_after_resume_identity_changes() {
        let cwd = tempfile::tempdir().unwrap();
        let cwd = cwd.path().canonicalize().unwrap();
        let mut app = App::new();
        app.add_plugins((vmux_ecs::OperationPlugin, vmux_connector::ConnectorPlugin))
            .init_resource::<ProviderCatalog>()
            .init_resource::<ProviderSessionTombstones>()
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_message::<ProviderImportRequest>()
            .add_observer(open);
        app.world_mut()
            .resource_mut::<ProviderCatalog>()
            .sessions
            .push(ProviderSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                agent: "codex".into(),
                cwd: cwd.to_string_lossy().into_owned(),
                ..Default::default()
            });
        let id = SessionId::new("canonical-session");
        let session = app
            .world_mut()
            .spawn((Session, id.clone(), AgentId("codex".into())))
            .id();
        ProviderFixture::spawn_link(app.world_mut(), session, "acp:codex", "provider-session");
        app.update();
        let stack = app.world_mut().spawn_empty().id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ResumeSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                cwd: cwd.to_string_lossy().into_owned(),
                open: None,
            },
        });

        assert!(
            app.world_mut()
                .resource_mut::<Messages<CreateRequest>>()
                .drain()
                .next()
                .is_none()
        );
        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].url, Route::Session(id).url());
    }

    #[test]
    fn provider_resume_rejects_a_dismissed_session() {
        let cwd = tempfile::tempdir().unwrap();
        let cwd = cwd.path().canonicalize().unwrap();
        let mut app = App::new();
        app.init_resource::<ProviderCatalog>()
            .insert_resource(ProviderSessionTombstones::new([
                ProviderFixture::identity("acp:codex", "provider-session"),
            ]))
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_message::<ProviderImportRequest>()
            .add_observer(open);
        app.world_mut()
            .resource_mut::<ProviderCatalog>()
            .sessions
            .push(ProviderSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                agent: "codex".into(),
                cwd: cwd.to_string_lossy().into_owned(),
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
                open: None,
            },
        });

        assert!(
            app.world_mut()
                .resource_mut::<Messages<CreateRequest>>()
                .drain()
                .next()
                .is_none()
        );
        assert!(
            app.world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn provider_resume_rejects_a_missing_working_directory() {
        let mut app = App::new();
        app.init_resource::<ProviderCatalog>()
            .init_resource::<ProviderSessionTombstones>()
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_message::<ProviderImportRequest>()
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
                open: None,
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

    #[test]
    fn known_conversation_removes_only_the_shared_boundary() {
        let known = KnownConversation(vec![
            vmux_api::session::Message::user("first"),
            vmux_api::session::Message::Assistant {
                blocks: vec![vmux_api::session::AssistantBlock::Text("second".into())],
            },
        ]);
        let incoming = vec![
            vmux_api::session::Message::Assistant {
                blocks: vec![vmux_api::session::AssistantBlock::Text("second".into())],
            },
            vmux_api::session::Message::user("third"),
        ];

        assert_eq!(
            known.unseen(incoming),
            vec![vmux_api::session::Message::user("third")]
        );
    }

    #[test]
    fn newer_provider_activity_requests_an_imported_conversation_refresh() {
        let mut app = App::new();
        app.init_resource::<SourceSnapshot>()
            .init_resource::<AcpSnapshot>()
            .add_message::<ArchiveRequest>()
            .add_systems(Update, request_conversation_refreshes);
        app.world_mut()
            .resource_mut::<SourceSnapshot>()
            .0
            .entry(SourceKind::Codex)
            .or_default()
            .push(ProviderSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                created_at: 100,
                updated_at: 300,
                ..Default::default()
            });
        let stale = app
            .world_mut()
            .spawn((
                Session,
                SessionId::new("stale"),
                vmux_ecs::LastActivatedAt(200),
                ConversationRevision(200),
            ))
            .id();
        ProviderFixture::spawn_link(app.world_mut(), stale, "acp:codex", "provider-session");
        let current = app
            .world_mut()
            .spawn((
                Session,
                SessionId::new("current"),
                vmux_ecs::LastActivatedAt(300),
                ConversationRevision(300),
            ))
            .id();
        ProviderFixture::spawn_link(app.world_mut(), current, "acp:codex", "provider-session");

        app.update();

        assert_eq!(
            app.world()
                .get::<ConversationRefreshRequested>(stale)
                .map(|requested| requested.0),
            Some(300)
        );
        assert!(
            app.world()
                .get::<ConversationRefreshRequested>(current)
                .is_none()
        );
        assert_eq!(
            app.world()
                .get::<vmux_ecs::LastActivatedAt>(stale)
                .unwrap()
                .0,
            300
        );
    }

    #[test]
    fn rollout_activity_refreshes_an_imported_session_without_a_catalog_entry() {
        let mut app = App::new();
        app.add_message::<ProviderConversationChanged>()
            .add_systems(Update, request_changed_conversations);
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId::new("local-session"),
                vmux_ecs::LastActivatedAt(200),
                ConversationRevision(200),
            ))
            .id();
        ProviderFixture::spawn_link(app.world_mut(), session, "acp:codex", "provider-session");
        app.world_mut()
            .resource_mut::<Messages<ProviderConversationChanged>>()
            .write(ProviderConversationChanged {
                session: ProviderFixture::identity("acp:codex", "provider-session"),
                updated_at: 300,
            });

        app.update();

        assert_eq!(
            app.world()
                .get::<ConversationRefreshRequested>(session)
                .map(|requested| requested.0),
            Some(300)
        );
        assert_eq!(
            app.world()
                .get::<vmux_ecs::LastActivatedAt>(session)
                .unwrap()
                .0,
            300
        );
    }

    #[test]
    fn durable_archive_overrides_win_until_the_provider_has_newer_state() {
        let mut app = App::new();
        app.init_resource::<SourceSnapshot>()
            .init_resource::<AcpSnapshot>()
            .add_message::<ArchiveRequest>()
            .add_systems(Update, request_conversation_refreshes);
        app.world_mut()
            .resource_mut::<SourceSnapshot>()
            .0
            .entry(SourceKind::Codex)
            .or_default()
            .extend([
                ProviderSession {
                    kind: "acp:codex".into(),
                    sid: "locally-active".into(),
                    archive: Some(ArchiveState::Archived(200)),
                    ..Default::default()
                },
                ProviderSession {
                    kind: "acp:codex".into(),
                    sid: "locally-archived".into(),
                    archive: Some(ArchiveState::Active(200)),
                    ..Default::default()
                },
                ProviderSession {
                    kind: "acp:codex".into(),
                    sid: "provider-newer".into(),
                    archive: Some(ArchiveState::Active(400)),
                    ..Default::default()
                },
                ProviderSession {
                    kind: "acp:codex".into(),
                    sid: "session-newer".into(),
                    archive: Some(ArchiveState::Archived(300)),
                    ..Default::default()
                },
            ]);
        app.world_mut().spawn((
            ProviderArchive,
            ProviderFixture::components("acp:codex", "locally-active"),
            RemoteUpdatedAt(vmux_ecs::UnixMillis(300)),
        ));
        app.world_mut().spawn((
            ProviderArchive,
            ProviderFixture::components("acp:codex", "locally-archived"),
            RemoteUpdatedAt(vmux_ecs::UnixMillis(300)),
            ArchivedAt(300),
        ));
        app.world_mut().spawn((
            ProviderArchive,
            ProviderFixture::components("acp:codex", "provider-newer"),
            RemoteUpdatedAt(vmux_ecs::UnixMillis(300)),
            ArchivedAt(300),
        ));
        let active = app
            .world_mut()
            .spawn((
                Session,
                SessionId::new("active-session"),
                vmux_ecs::LastActivatedAt(0),
            ))
            .id();
        ProviderFixture::spawn_link(app.world_mut(), active, "acp:codex", "locally-active");
        let provider_newer = app
            .world_mut()
            .spawn((
                Session,
                SessionId::new("provider-newer-session"),
                vmux_ecs::LastActivatedAt(0),
                ArchivedAt(300),
            ))
            .id();
        ProviderFixture::spawn_link(
            app.world_mut(),
            provider_newer,
            "acp:codex",
            "provider-newer",
        );
        let archived = app
            .world_mut()
            .spawn((
                Session,
                SessionId::new("archived-session"),
                vmux_ecs::LastActivatedAt(0),
            ))
            .id();
        ProviderFixture::spawn_link(app.world_mut(), archived, "acp:codex", "locally-archived");
        let session_newer = app
            .world_mut()
            .spawn((
                Session,
                SessionId::new("session-newer"),
                vmux_ecs::LastActivatedAt(0),
                ArchivedAt(400),
            ))
            .id();
        ProviderFixture::spawn_link(app.world_mut(), session_newer, "acp:codex", "session-newer");

        app.update();

        let requests = app
            .world_mut()
            .resource_mut::<Messages<ArchiveRequest>>()
            .drain()
            .map(|request| (request.id.0, request.archived_at))
            .collect::<HashMap<_, _>>();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests.get("archived-session"), Some(&Some(300)));
        assert_eq!(requests.get("provider-newer-session"), Some(&None));
    }

    #[test]
    fn archive_overlay_keeps_a_write_newer_than_the_scan() {
        let scan_started_at = Instant::now();
        let mut overlay = ArchiveOverlay::default();
        overlay.record(
            SourceKind::Codex,
            "provider-session".into(),
            ArchiveState::Archived(300),
        );
        overlay
            .0
            .get_mut(&SourceKind::Codex)
            .unwrap()
            .get_mut("provider-session")
            .unwrap()
            .completed_at = scan_started_at + Duration::from_nanos(1);
        let mut scanned = vec![ProviderSession {
            sid: "provider-session".into(),
            archive: Some(ArchiveState::Active(200)),
            ..Default::default()
        }];

        overlay.apply(SourceKind::Codex, scan_started_at, &mut scanned);

        assert_eq!(scanned[0].archive, Some(ArchiveState::Archived(300)));
    }
}
