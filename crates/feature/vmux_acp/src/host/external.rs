use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::UiInput;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vmux_api::chat::ResumeSession;
use vmux_api::open_target::{OpenTarget, PaneOpenMode};
use vmux_api::protocol::AcpProviderSession;
use vmux_ecs::page::{Activity, PageReady};
use vmux_ecs::service::{
    ServiceConnected, ServiceMessageAppExt, ServiceMessageSet, ServiceRequest,
};
use vmux_ecs::{
    ArchivedAt, BlockingRuntime, BlockingTask, CreatedAt, PageOpenRequest, PageOpenTarget,
};
use vmux_layout::{TabLayoutSpawnContent, TabLayoutSpawnRequest};
use vmux_session::host::{SessionManagerView, SessionView, SessionViews};
use vmux_session::{
    AgentConversationTitle, AgentFamily, AgentId, ArchiveRequest, ArchiveState,
    ConversationAppendReceived, CreateRequest, Created, ProviderArchive, ProviderCatalog,
    ProviderKind, ProviderSession, ProviderSessionId, ProviderSessionKey,
    ProviderSessionTombstones, Route, RunState, Session, SessionDuplicate, SessionId,
    SnapshotReceived, SourceUpdatedAt, TranscriptLoading, Transcripts,
};
#[cfg(test)]
use vmux_session::{ProviderArchiveCompleted, ProviderArchiveWriteRequest};
use vmux_setting::AppSettings;
use vmux_tool::ToolInventoryChanged;

use super::acp::{AcpLaunch, AcpPackageChanged, registry::RegistryAgent};
use crate::AcpSessionId;

mod archive;
mod claude;
mod codex;
mod conversation;
mod vibe;

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((archive::Plugin, HistoryReadPlugin))
            .init_resource::<SourceSnapshot>()
            .init_resource::<NativeReaders>()
            .init_resource::<AcpSnapshot>()
            .init_resource::<CatalogRequests>()
            .init_resource::<PendingImports>()
            .init_resource::<ProviderCatalog>()
            .init_resource::<ActivationDirty>()
            .add_message::<ToolInventoryChanged>()
            .add_message::<ProviderConversationChanged>()
            .add_message::<vmux_session::OperationReceived>()
            .add_service_message::<AcpCatalogReceived>()
            .add_observer(open)
            .add_observer(attach)
            .add_systems(
                Update,
                (
                    request_catalogs,
                    receive_catalogs.after(ServiceMessageSet),
                    mark_activation_dirty,
                    activate,
                    watch,
                    request_changed_conversations,
                    scan,
                    finish,
                    project,
                    associate,
                    request_conversation_refreshes,
                )
                    .chain(),
            );
    }
}

struct HistoryReadPlugin;

impl bevy::prelude::Plugin for HistoryReadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockingRuntime>()
            .init_resource::<SourceSnapshot>()
            .add_message::<SnapshotReceived>()
            .add_message::<ConversationAppendReceived>()
            .add_message::<vmux_session::OperationReceived>()
            .add_systems(
                Update,
                (
                    queue_initial_conversations,
                    queue_failed_conversations,
                    queue_conversation_refreshes,
                    finish_conversations,
                )
                    .chain()
                    .after(request_conversation_refreshes),
            );
    }
}

const SCAN_DEBOUNCE: Duration = Duration::from_millis(350);
const SCAN_MAX_LATENCY: Duration = Duration::from_secs(2);
const ACTIVITY_REFRESH_INTERVAL: Duration = Duration::from_millis(100);
const CONVERSATION_POLL_INTERVAL: Duration = Duration::from_millis(250);
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
    let package_changed = package_changes.read().count() > 0;
    let mut tool_changed = false;
    for changed in tool_changes.read() {
        tool_changed |= changed.0.is("acp");
    }
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

#[derive(Resource, Default)]
struct PendingImports {
    by_id: HashMap<SessionId, PendingImport>,
    by_source: HashMap<(AgentFamily, String), SessionId>,
}

impl PendingImports {
    fn reserve(&mut self, id: SessionId, import: PendingImport) -> PendingImportReservation {
        let key = (
            AgentFamily::of(&import.source.agent),
            import.source.sid.clone(),
        );
        if let Some(existing) = self.by_source.get(&key)
            && let Some(pending) = self.by_id.get_mut(existing)
        {
            pending.destination = import.destination;
            let open_now = pending.deferred_open && !import.deferred_open;
            pending.deferred_open = import.deferred_open;
            return PendingImportReservation {
                id: existing.clone(),
                created: false,
                open_now,
            };
        }
        self.by_source.insert(key, id.clone());
        let open_now = !import.deferred_open;
        self.by_id.insert(id.clone(), import);
        PendingImportReservation {
            id,
            created: true,
            open_now,
        }
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
    destination: ResumeDestination,
    deferred_open: bool,
    source: ProviderSession,
    agent: AgentId,
}

struct PendingImportReservation {
    id: SessionId,
    created: bool,
    open_now: bool,
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
    manager: bool,
    conversation_watch: Option<ConversationWatch>,
}

impl Runtime {
    fn ensure_watches(&mut self, paths: &SourcePaths) {
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        let desired = paths.watches(&self.sources, self.watch_codex_activity, self.manager);
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

#[derive(Component)]
struct ScanTask {
    started_at: Instant,
    task: BlockingTask<Vec<SourceRead>>,
}

#[derive(Component)]
struct ConversationLoadPending;

#[derive(Component)]
struct FailedConversationRead;

#[derive(Component)]
struct FailedConversationReadRequested;

#[derive(Component)]
struct ConversationRefreshRequested(i64);

#[derive(Component)]
struct ConversationRevision(i64);

#[derive(Component)]
struct ConversationCursor(u64);

#[derive(Component)]
struct ConversationFile(PathBuf);

#[derive(Component)]
struct ConversationLoadTask {
    session: Entity,
    sid: SessionId,
    revision: i64,
    task: BlockingTask<Result<ConversationRead, String>>,
}

struct ConversationRead {
    path: Option<PathBuf>,
    watch_paths: Vec<PathBuf>,
    records: Vec<conversation::Record>,
    messages: Vec<vmux_api::session::Message>,
    cursor: u64,
    snapshot: bool,
    complete: bool,
}

#[derive(Resource, Clone, Default)]
struct NativeReaders(Arc<Mutex<Option<conversation::ProcessReader>>>);

#[derive(Component)]
struct ConversationWatchPaths(Vec<PathBuf>);

impl ConversationRead {
    fn empty(cursor: Option<u64>) -> Self {
        Self {
            path: None,
            watch_paths: Vec::new(),
            records: Vec::new(),
            messages: Vec::new(),
            cursor: cursor.unwrap_or_default(),
            snapshot: false,
            complete: true,
        }
    }
}

struct ConversationSource {
    kind: String,
    sid: String,
    created_at: i64,
}

impl ConversationSource {
    fn new(kind: &str, sid: &str, created_at: i64) -> Option<Self> {
        let agent = kind.strip_prefix("acp:")?;
        let family = AgentFamily::of(agent);
        let source = SourceKind::for_family(&family)?;
        if !conversation::Files::valid_id(sid) {
            return None;
        }
        Some(Self {
            kind: format!("acp:{}", source.id()),
            sid: sid.to_string(),
            created_at,
        })
    }

    fn for_failed(kind: &str, sid: &str, created_at: i64) -> Option<Self> {
        Self::new(kind, sid, created_at)
    }

    fn read(
        self,
        paths: &SourcePaths,
        cursor: Option<u64>,
        known: Vec<vmux_api::session::Message>,
        native: NativeReaders,
        hint: Option<PathBuf>,
    ) -> Result<ConversationRead, String> {
        let source = SourceKind::for_family(&AgentFamily::of(
            self.kind.strip_prefix("acp:").unwrap_or_default(),
        ))
        .ok_or("unsupported history source")?;
        let read = match source {
            SourceKind::Codex => {
                if let Some(path) = hint {
                    codex::Reader::conversation_file(
                        &path,
                        cursor,
                        conversation::FileConversation::TAIL_BYTES,
                    )?
                } else {
                    codex::Reader::at(&paths.codex).conversation(
                        &self.sid,
                        self.created_at,
                        cursor,
                    )?
                }
            }
            SourceKind::Claude => {
                let path = hint.or(conversation::Files::locate(
                    &paths.claude.join("projects"),
                    &self.sid,
                    false,
                )?);
                let Some(path) = path else {
                    return Ok(ConversationRead::empty(cursor));
                };
                conversation::FileConversation::read(
                    &path,
                    cursor,
                    conversation::FileConversation::TAIL_BYTES,
                    conversation::ChatDecoder,
                )?
            }
            SourceKind::Vibe => {
                let root = std::env::var_os("MISTRAL_VIBE_SESSION_ROOT")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| paths.vibe.join("logs/session"));
                if root.join("unified").join(&self.sid).is_dir() {
                    return vibe::Reader::conversation(&root, &self.sid, &native);
                }
                let path = hint.or(conversation::Files::locate(&root, &self.sid, true)?);
                let Some(path) = path else {
                    return Ok(ConversationRead::empty(cursor));
                };
                conversation::FileConversation::read(
                    &path,
                    cursor,
                    conversation::FileConversation::TAIL_BYTES,
                    conversation::ChatDecoder,
                )?
            }
        };
        let snapshot = cursor.is_none() && known.is_empty();
        let messages = if read.tail && !known.is_empty() {
            KnownConversation(known).unseen(read.messages)
        } else {
            read.messages
        };
        Ok(ConversationRead {
            path: read.path,
            watch_paths: Vec::new(),
            records: Vec::new(),
            messages,
            cursor: read.cursor,
            snapshot,
            complete: read.complete,
        })
    }
}

struct KnownConversation(Vec<vmux_api::session::Message>);

impl KnownConversation {
    const LIMIT: usize = 512;

    fn tail(messages: &[vmux_api::session::Message]) -> Vec<vmux_api::session::Message> {
        let start = messages.len().saturating_sub(Self::LIMIT);
        messages[start..].to_vec()
    }

    fn units(messages: &[vmux_api::session::Message]) -> Vec<vmux_api::session::Message> {
        let mut units = Vec::new();
        for message in messages {
            match message {
                vmux_api::session::Message::Assistant { blocks } => {
                    for block in blocks {
                        units.push(vmux_api::session::Message::Assistant {
                            blocks: vec![block.clone()],
                        });
                    }
                }
                _ => units.push(message.clone()),
            }
        }
        units
    }

    fn same(left: &vmux_api::session::Message, right: &vmux_api::session::Message) -> bool {
        match (left, right) {
            (
                vmux_api::session::Message::User {
                    text: left,
                    attachments: a,
                },
                vmux_api::session::Message::User {
                    text: right,
                    attachments: b,
                },
            ) => {
                vmux_api::protocol::AgentPromptEnvelope::new(left)
                    .display()
                    .unwrap_or(left)
                    == vmux_api::protocol::AgentPromptEnvelope::new(right)
                        .display()
                        .unwrap_or(right)
                    && a == b
            }
            (
                vmux_api::session::Message::Assistant { blocks: a },
                vmux_api::session::Message::Assistant { blocks: b },
            ) => match (a.as_slice(), b.as_slice()) {
                (
                    [
                        vmux_api::session::AssistantBlock::ToolUse {
                            call_id: a,
                            args: x,
                            ..
                        },
                    ],
                    [
                        vmux_api::session::AssistantBlock::ToolUse {
                            call_id: b,
                            args: y,
                            ..
                        },
                    ],
                ) => {
                    a == b
                        && (x == y
                            || serde_json::from_str::<serde_json::Value>(x)
                                .ok()
                                .zip(serde_json::from_str::<serde_json::Value>(y).ok())
                                .is_some_and(|(x, y)| x == y))
                }
                _ => a == b,
            },
            _ => left == right,
        }
    }

    fn unseen(&self, incoming: Vec<vmux_api::session::Message>) -> Vec<vmux_api::session::Message> {
        let known = Self::units(&self.0);
        let incoming = Self::units(&incoming);
        let limit = known.len().min(incoming.len());
        for overlap in (1..=limit).rev() {
            if known[known.len() - overlap..]
                .iter()
                .zip(&incoming[..overlap])
                .all(|(left, right)| Self::same(left, right))
            {
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

    fn id(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Vibe => "mistral-vibe",
        }
    }

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
        manager: bool,
    ) -> Vec<(PathBuf, bool)> {
        let mut watches = Vec::new();
        if manager && sources.contains(&SourceKind::Codex) {
            watches.push((self.codex.clone(), false));
        }
        if sources.contains(&SourceKind::Codex) || watch_codex_activity {
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

    fn change(&self, kind: &notify::EventKind, path: &Path) -> Option<SourceChange> {
        let codex_index = self.codex.join("session_index.jsonl");
        let codex_sessions = self.codex.join("sessions");
        let claude_history = self.claude.join("history.jsonl");
        let vibe = self.vibe.join("logs/session");
        if path == self.codex || path == codex_index {
            return Some(SourceChange::Scan(SourceKind::Codex));
        }
        if path == codex_sessions {
            return Some(SourceChange::Scan(SourceKind::Codex));
        }
        if path.starts_with(&codex_sessions) {
            if kind.is_modify()
                && path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
            {
                return Some(SourceChange::CodexActivity(path.to_path_buf()));
            }
            if kind.is_create() || kind.is_remove() {
                return Some(SourceChange::Scan(SourceKind::Codex));
            }
            return None;
        }
        if Self::codex_state(path, &self.codex) {
            return Some(SourceChange::CodexArchives);
        }
        if path == self.claude || path == claude_history {
            return Some(SourceChange::Scan(SourceKind::Claude));
        }
        if path == vibe || path.starts_with(&vibe) || vibe.starts_with(path) {
            return Some(SourceChange::Scan(SourceKind::Vibe));
        }
        None
    }

    fn changes(&self, event: &notify::Event) -> Vec<SourceChange> {
        event
            .paths
            .iter()
            .filter_map(|path| self.change(&event.kind, path))
            .collect()
    }

    fn codex_state(path: &Path, root: &Path) -> bool {
        if path.parent() != Some(root) {
            return false;
        }
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.starts_with("state_")
                    && (name.ends_with(".sqlite") || name.ends_with(".sqlite-wal"))
            })
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
        let mut indexed = HashMap::<ProviderSessionKey, ProviderSession>::new();
        for session in sessions {
            if session.kind.trim().is_empty() || session.sid.trim().is_empty() {
                continue;
            }
            let key = ProviderSessionKey::new(
                ProviderKind(session.kind.clone()),
                ProviderSessionId(session.sid.clone()),
            );
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ConversationTarget {
    session: ProviderSessionKey,
    path: PathBuf,
}

impl ConversationTarget {
    fn change(&self) -> SourceChange {
        let updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(i64::MAX as u128) as i64;
        SourceChange::ConversationActivity(self.session.clone(), updated_at)
    }
}

struct ConversationWatch {
    sender: mpsc::Sender<HashSet<ConversationTarget>>,
    paths: HashSet<ConversationTarget>,
}

impl ConversationWatch {
    fn start(changes: mpsc::Sender<SourceChange>) -> Self {
        let (sender, receiver) = mpsc::channel::<HashSet<ConversationTarget>>();
        let worker = std::thread::Builder::new()
            .name("vmux-acp-conversation-watch".to_string())
            .spawn(move || {
                let mut files = HashMap::<ConversationTarget, Option<ConversationStamp>>::new();
                loop {
                    let paths = if files.is_empty() {
                        match receiver.recv() {
                            Ok(paths) => paths,
                            Err(_) => break,
                        }
                    } else {
                        match receiver.recv_timeout(CONVERSATION_POLL_INTERVAL) {
                            Ok(paths) => paths,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                for (path, previous) in &mut files {
                                    let current = ConversationStamp::read(&path.path);
                                    if *previous != current {
                                        *previous = current;
                                        let _ = changes.send(path.change());
                                    }
                                }
                                continue;
                            }
                        }
                    };
                    files.retain(|path, _| paths.contains(path));
                    for path in paths {
                        if files.contains_key(&path) {
                            continue;
                        }
                        let stamp = ConversationStamp::read(&path.path);
                        files.insert(path.clone(), stamp);
                        let _ = changes.send(path.change());
                    }
                }
            });
        if let Err(error) = worker {
            bevy::log::warn!("ACP conversation watch init failed: {error}");
        }
        Self {
            sender,
            paths: HashSet::new(),
        }
    }

    fn set_paths(&mut self, paths: HashSet<ConversationTarget>) {
        if self.paths == paths {
            return;
        }
        let _ = self.sender.send(paths.clone());
        self.paths = paths;
    }
}

#[derive(PartialEq, Eq)]
struct ConversationStamp {
    length: u64,
    modified: std::time::SystemTime,
}

impl ConversationStamp {
    fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        if !metadata.file_type().is_file() {
            return None;
        }
        Some(Self {
            length: metadata.len(),
            modified: metadata.modified().ok()?,
        })
    }
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum SourceChange {
    Scan(SourceKind),
    CodexActivity(PathBuf),
    ConversationActivity(ProviderSessionKey, i64),
    CodexArchives,
}

#[derive(Message)]
struct ProviderConversationChanged {
    session: ProviderSessionKey,
    updated_at: i64,
}

impl ProviderConversationChanged {
    fn matches(&self, kind: &str, sid: &str) -> bool {
        if self.session.matches(kind, sid) {
            return true;
        }
        self.session.id.0 == sid
            && AgentFamily::of(self.session.kind.0.strip_prefix("acp:").unwrap_or_default())
                == AgentFamily::of(kind.strip_prefix("acp:").unwrap_or_default())
            && ConversationSource::new(kind, sid, 0).is_some()
    }

    fn codex(activity: &codex::RolloutActivity) -> Self {
        Self {
            session: ProviderSessionKey::new(
                ProviderKind("acp:codex".to_string()),
                ProviderSessionId(activity.sid().to_string()),
            ),
            updated_at: activity.updated_at(),
        }
    }
}

#[derive(Default)]
struct SourceChanges {
    conversations: HashMap<ProviderSessionKey, i64>,
    scans: HashSet<SourceKind>,
    codex_activity: HashSet<PathBuf>,
    codex_archives: bool,
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
            SourceChange::ConversationActivity(session, revision) => {
                self.conversations
                    .entry(session)
                    .and_modify(|current| *current = (*current).max(revision))
                    .or_insert(revision);
            }
            SourceChange::CodexArchives => self.codex_archives = true,
        }
    }

    fn extend(&mut self, changes: Self) {
        for (session, revision) in changes.conversations {
            self.insert(SourceChange::ConversationActivity(session, revision));
        }
        self.scans.extend(changes.scans);
        self.codex_activity.extend(changes.codex_activity);
        self.codex_archives |= changes.codex_archives;
    }

    fn is_empty(&self) -> bool {
        self.scans.is_empty()
            && self.codex_activity.is_empty()
            && self.conversations.is_empty()
            && !self.codex_archives
    }

    fn debounce(&self) -> Duration {
        if self.scans.is_empty()
            && !self.codex_archives
            && (!self.codex_activity.is_empty() || !self.conversations.is_empty())
        {
            ACTIVITY_REFRESH_INTERVAL
        } else {
            SCAN_DEBOUNCE
        }
    }

    fn max_latency(&self) -> Duration {
        if self.scans.is_empty()
            && !self.codex_archives
            && (!self.codex_activity.is_empty() || !self.conversations.is_empty())
        {
            ACTIVITY_REFRESH_INTERVAL
        } else {
            SCAN_MAX_LATENCY
        }
    }
}

fn activate(
    manager_activity: Activity<SessionManagerView>,
    session_activity: Activity<SessionView>,
    acp: Res<AcpSnapshot>,
    view_targets: SessionViews,
    views: Query<Entity, With<SessionView>>,
    sessions: Query<
        (
            Entity,
            &ProviderKind,
            &ProviderSessionId,
            Option<&ConversationFile>,
            Option<&RunState>,
            Option<&ConversationWatchPaths>,
        ),
        With<Session>,
    >,
    mut dirty: ResMut<ActivationDirty>,
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
    let watch_codex_activity = manager;
    let mut sources = if manager {
        available.clone()
    } else {
        HashSet::new()
    };
    let mut watch_codex_activity = watch_codex_activity;
    if !manager {
        for (entity, kind, _, _, _, _) in &sessions {
            let Some(agent) = kind.0.strip_prefix("acp:") else {
                continue;
            };
            let family = AgentFamily::of(agent);
            if viewed.contains(&entity) && family == codex_family {
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
    let mut conversation_paths = HashSet::new();
    for (entity, kind, sid, file, state, watched) in &sessions {
        if !viewed.contains(&entity) || ConversationSource::new(&kind.0, &sid.0, 0).is_none() {
            continue;
        }
        if let (Some(file), Some(RunState::Errored(_))) = (file, state) {
            let paths = watched
                .map(|watched| watched.0.clone())
                .unwrap_or_else(|| vec![file.0.clone()]);
            for path in paths {
                conversation_paths.insert(ConversationTarget {
                    session: ProviderSessionKey::new(kind.clone(), sid.clone()),
                    path,
                });
            }
        }
    }
    if sources.is_empty() && !watch_codex_activity && conversation_paths.is_empty() {
        for (entity, _, _) in &mut runtime {
            commands.entity(entity).despawn();
        }
        return;
    }
    if let Ok((entity, mut runtime, paths)) = runtime.single_mut() {
        if let Some(watch) = &mut runtime.conversation_watch {
            watch.set_paths(conversation_paths);
        }
        if runtime.sources != sources
            || runtime.watch_codex_activity != watch_codex_activity
            || runtime.manager != manager
        {
            let requested = sources
                .difference(&runtime.sources)
                .copied()
                .collect::<HashSet<_>>();
            runtime.sources = sources;
            runtime.watch_codex_activity = watch_codex_activity;
            runtime.manager = manager;
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
    let mut conversation_watch = ConversationWatch::start(events.sender.clone());
    conversation_watch.set_paths(conversation_paths);
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
            for change in watched_paths.changes(&event) {
                let _ = sender.send(change);
            }
        });
    let mut runtime = Runtime {
        changes: Mutex::new(events.receiver),
        watcher: watcher.ok(),
        watched: HashMap::new(),
        sources: sources.clone(),
        watch_codex_activity,
        manager,
        conversation_watch: Some(conversation_watch),
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
        (
            With<Session>,
            Or<(
                Added<Session>,
                Added<ProviderKind>,
                Changed<ProviderKind>,
                Changed<RunState>,
                Changed<ConversationFile>,
                Changed<ConversationWatchPaths>,
            )>,
        ),
    >,
    targets: Query<(), Changed<vmux_ecs::EntityTarget<Session>>>,
    mut removed_managers: RemovedComponents<SessionManagerView>,
    mut removed_views: RemovedComponents<SessionView>,
    mut removed_ready: RemovedComponents<PageReady>,
    mut removed_sessions: RemovedComponents<Session>,
    mut removed_providers: RemovedComponents<ProviderKind>,
    mut removed_targets: RemovedComponents<vmux_ecs::EntityTarget<Session>>,
    mut dirty: ResMut<ActivationDirty>,
) {
    let manager_removed = removed_managers.read().count() > 0;
    let view_removed = removed_views.read().count() > 0;
    let ready_removed = removed_ready.read().count() > 0;
    let session_removed = removed_sessions.read().count() > 0;
    let provider_removed = removed_providers.read().count() > 0;
    let target_removed = removed_targets.read().count() > 0;
    let changed = acp.is_changed()
        || !managers.is_empty()
        || !views.is_empty()
        || !sessions.is_empty()
        || !targets.is_empty()
        || manager_removed
        || view_removed
        || ready_removed
        || session_removed
        || provider_removed
        || target_removed;
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
    for (session, updated_at) in changes.conversations.drain() {
        conversations.write(ProviderConversationChanged {
            session,
            updated_at,
        });
    }
    changes
        .scans
        .retain(|source| runtime.sources.contains(source));
    if !runtime.sources.contains(&SourceKind::Codex) && !runtime.watch_codex_activity {
        changes.codex_activity.clear();
    }
    if changes.scans.contains(&SourceKind::Codex) {
        changes.codex_archives = false;
    }
    if changes.codex_archives && runtime.manager && runtime.sources.contains(&SourceKind::Codex) {
        commands.entity(entity).insert(archive::RefreshRequested);
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
    mut sessions: Query<
        (
            Entity,
            &ProviderKind,
            &ProviderSessionId,
            &mut vmux_ecs::LastActivatedAt,
            Option<&ConversationRevision>,
            Option<&mut ConversationRefreshRequested>,
        ),
        (With<Session>, Without<SessionDuplicate>),
    >,
    mut commands: Commands,
) {
    for changed in changed.read() {
        for (session, kind, sid, mut activated_at, revision, requested) in &mut sessions {
            if !changed.matches(&kind.0, &sid.0) {
                continue;
            }
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
}

fn scan(
    runtime: Single<
        (Entity, &Runtime, &SourcePaths, &ScanRequested),
        (With<Runtime>, Without<ScanTask>),
    >,
    snapshot: Res<SourceSnapshot>,
    workers: Res<BlockingRuntime>,
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
    let Some(task) = workers.spawn(wake, move || readers.read()) else {
        return;
    };
    commands
        .entity(entity)
        .remove::<ScanRequested>()
        .insert(ScanTask { started_at, task });
}

fn finish(
    task: Single<(Entity, &mut ScanTask)>,
    mut snapshot: ResMut<SourceSnapshot>,
    mut overlay: ResMut<archive::Overlay>,
    mut commands: Commands,
) {
    let (entity, mut task) = task.into_inner();
    let Some(result) = task.task.poll() else {
        return;
    };
    let reads = match result {
        Ok(reads) => reads,
        Err(_) => {
            bevy::log::warn!("provider catalog worker panicked");
            commands.entity(entity).remove::<ScanTask>();
            return;
        }
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
    requests: Res<CatalogRequests>,
    scans: Query<(), Or<(With<ScanRequested>, With<ScanTask>)>>,
    unavailable: Query<(), With<vmux_ecs::service::ServiceUnavailable>>,
    imported: Query<
        (
            &AgentId,
            Option<&ProviderKind>,
            Option<&ProviderSessionId>,
            Option<&AcpSessionId>,
        ),
        (With<Session>, Without<SessionDuplicate>),
    >,
    changed: Query<
        (),
        (
            With<Session>,
            Or<(
                Changed<AgentId>,
                Changed<ProviderKind>,
                Changed<ProviderSessionId>,
                Changed<AcpSessionId>,
                Changed<SessionDuplicate>,
            )>,
        ),
    >,
    mut removed: RemovedComponents<AcpSessionId>,
    mut removed_duplicates: RemovedComponents<SessionDuplicate>,
    mut catalog: ResMut<ProviderCatalog>,
) {
    let mut loading_changed = false;
    if requests.active {
        let loading = unavailable.is_empty()
            && (requests.generation == 0
                || requests.refresh
                || !requests.pending.is_empty()
                || !requests.queued.is_empty()
                || !scans.is_empty());
        if catalog.loading != loading {
            catalog.loading = loading;
            loading_changed = true;
        }
    }
    let session_removed = removed.read().count() > 0;
    let duplicate_removed = removed_duplicates.read().count() > 0;
    let removed = session_removed || duplicate_removed;
    if !loading_changed
        && !snapshot.is_changed()
        && !acp.is_changed()
        && changed.is_empty()
        && !removed
        && !catalog.is_added()
    {
        return;
    }
    let imported = imported
        .iter()
        .filter_map(|(agent, kind, provider_sid, resume)| {
            let sid = provider_sid
                .map(|sid| sid.0.clone())
                .or_else(|| resume.map(|sid| sid.0.clone()))?;
            let family = kind
                .and_then(|kind| kind.0.strip_prefix("acp:"))
                .map(AgentFamily::of)
                .unwrap_or_else(|| AgentFamily::of(&agent.0));
            Some((family, sid))
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
    sessions: Query<
        (
            Entity,
            &AgentId,
            &AcpSessionId,
            &Name,
            &vmux_ecs::Cwd,
            &CreatedAt,
        ),
        (With<Session>, Without<ProviderSessionId>),
    >,
    mut commands: Commands,
) {
    if snapshot.0.is_empty() {
        return;
    }
    for (entity, agent, resume, name, cwd, created_at) in &sessions {
        let cwd = cwd.0.to_string_lossy();
        let Some(source) =
            snapshot.association(&agent.0, &resume.0, name.as_str(), &cwd, created_at.0)
        else {
            continue;
        };
        commands.entity(entity).insert((
            ProviderKind(source.kind.clone()),
            ProviderSessionId(source.sid.clone()),
        ));
    }
}

fn request_conversation_refreshes(
    snapshot: Res<SourceSnapshot>,
    acp: Res<AcpSnapshot>,
    archive_overrides: Query<
        (
            &ProviderKind,
            &ProviderSessionId,
            &SourceUpdatedAt,
            Option<&ArchivedAt>,
        ),
        With<ProviderArchive>,
    >,
    mut sessions: Query<
        (
            Entity,
            &SessionId,
            &ProviderKind,
            &ProviderSessionId,
            &mut vmux_ecs::LastActivatedAt,
            Option<&ArchivedAt>,
            Option<&ConversationRevision>,
            Option<&mut ConversationRefreshRequested>,
        ),
        (With<Session>, Without<SessionDuplicate>),
    >,
    mut archive_requests: MessageWriter<ArchiveRequest>,
    mut commands: Commands,
) {
    if !snapshot.is_changed() && !acp.is_changed() {
        return;
    }
    let sources = ProviderSessionIndex::from_snapshots(&snapshot, &acp);
    let mut archive_overrides_by_session = HashMap::new();
    for (kind, sid, source_updated_at, archived_at) in &archive_overrides {
        archive_overrides_by_session.insert(
            (kind.0.as_str(), sid.0.as_str()),
            (
                source_updated_at.0,
                archived_at.map(|archived_at| archived_at.0),
            ),
        );
    }
    for (
        session,
        session_id,
        kind,
        provider_sid,
        mut activated_at,
        archived_at,
        revision,
        requested,
    ) in &mut sessions
    {
        let Some(source) = sources.get(&kind.0, &provider_sid.0) else {
            continue;
        };
        let archive_override =
            archive_overrides_by_session.get(&(kind.0.as_str(), provider_sid.0.as_str()));
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
        if ConversationSource::new(&kind.0, &provider_sid.0, source.created_at).is_none()
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
            &ProviderKind,
            &ProviderSessionId,
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
    transcripts: Transcripts,
    native: Option<Res<NativeReaders>>,
    files: Query<&ConversationFile>,
    workers: Res<BlockingRuntime>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (session, sid, kind, provider_sid, created_at, requested, cursor) in &sessions {
        if kind.0.starts_with("acp:") {
            continue;
        }
        if !transcripts.get(session).messages.is_empty() {
            commands.entity(session).remove::<TranscriptLoading>();
            continue;
        }
        if snapshot.get(&kind.0, &provider_sid.0).is_none() {
            commands.entity(session).remove::<TranscriptLoading>();
            continue;
        }
        let Some(source) = ConversationSource::new(&kind.0, &provider_sid.0, created_at.0) else {
            commands.entity(session).remove::<TranscriptLoading>();
            continue;
        };
        let revision = requested.map_or_else(
            || {
                snapshot
                    .get(&kind.0, &provider_sid.0)
                    .map_or(0, |source| source.updated_at)
            },
            |requested| requested.0,
        );
        let cursor = cursor.map(|cursor| cursor.0);
        let native = native
            .as_ref()
            .map(|native| (**native).clone())
            .unwrap_or_default();
        let hint = files.get(session).ok().map(|file| file.0.clone());
        let paths = paths.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let Some(task) = workers.spawn_long(wake, move || {
            source.read(&paths, cursor, Vec::new(), native, hint)
        }) else {
            continue;
        };
        commands
            .entity(session)
            .insert(ConversationLoadPending)
            .remove::<ConversationRefreshRequested>();
        commands.spawn((
            ChildOf(session),
            ConversationLoadTask {
                session,
                sid: sid.clone(),
                revision,
                task,
            },
        ));
    }
}

fn queue_failed_conversations(
    paths: Option<Single<&SourcePaths>>,
    sessions: Query<
        (
            Entity,
            &SessionId,
            &ProviderKind,
            &ProviderSessionId,
            &CreatedAt,
            &RunState,
            Option<&ConversationCursor>,
        ),
        (
            With<Session>,
            Or<(Changed<RunState>, With<FailedConversationReadRequested>)>,
            Without<ConversationLoadPending>,
            Without<SessionDuplicate>,
        ),
    >,
    transcripts: Transcripts,
    native: Option<Res<NativeReaders>>,
    files: Query<&ConversationFile>,
    workers: Res<BlockingRuntime>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    if sessions.is_empty() {
        return;
    }
    let paths = match paths {
        Some(paths) => paths.into_inner().clone(),
        None => {
            let Some(paths) = SourcePaths::current() else {
                return;
            };
            paths
        }
    };
    for (session, sid, kind, provider_sid, created_at, state, cursor) in &sessions {
        if !matches!(state, RunState::Errored(_)) {
            commands
                .entity(session)
                .remove::<FailedConversationReadRequested>();
            continue;
        }
        let Some(source) = ConversationSource::for_failed(&kind.0, &provider_sid.0, created_at.0)
        else {
            continue;
        };
        let native = native
            .as_ref()
            .map(|native| (**native).clone())
            .unwrap_or_default();
        let hint = files.get(session).ok().map(|file| file.0.clone());
        let paths = paths.clone();
        let cursor = cursor.map(|cursor| cursor.0);
        let known = KnownConversation::tail(&transcripts.get(session).messages);
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let Some(task) = workers.spawn_long(wake, move || {
            source.read(&paths, cursor, known, native, hint)
        }) else {
            commands
                .entity(session)
                .insert(FailedConversationReadRequested);
            continue;
        };
        commands
            .entity(session)
            .insert(ConversationLoadPending)
            .remove::<FailedConversationReadRequested>();
        commands.spawn((
            FailedConversationRead,
            ChildOf(session),
            ConversationLoadTask {
                session,
                sid: sid.clone(),
                revision: created_at.0,
                task,
            },
        ));
    }
}

fn queue_conversation_refreshes(
    paths: Single<&SourcePaths>,
    sessions: Query<
        (
            Entity,
            &SessionId,
            &ProviderKind,
            &ProviderSessionId,
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
    transcripts: Transcripts,
    native: Option<Res<NativeReaders>>,
    files: Query<&ConversationFile>,
    workers: Res<BlockingRuntime>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (session, sid, kind, provider_sid, created_at, requested, cursor) in &sessions {
        let Some(source) = ConversationSource::new(&kind.0, &provider_sid.0, created_at.0) else {
            commands
                .entity(session)
                .remove::<ConversationRefreshRequested>();
            continue;
        };
        let revision = requested.0;
        let cursor = cursor.map(|cursor| cursor.0);
        let known = KnownConversation::tail(&transcripts.get(session).messages);
        let native = native
            .as_ref()
            .map(|native| (**native).clone())
            .unwrap_or_default();
        let hint = files.get(session).ok().map(|file| file.0.clone());
        let paths = paths.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let Some(task) = workers.spawn_long(wake, move || {
            source.read(&paths, cursor, known, native, hint)
        }) else {
            continue;
        };
        commands
            .entity(session)
            .insert(ConversationLoadPending)
            .remove::<ConversationRefreshRequested>();
        commands.spawn((
            ChildOf(session),
            ConversationLoadTask {
                session,
                sid: sid.clone(),
                revision,
                task,
            },
        ));
    }
}

fn finish_conversations(
    mut tasks: Query<(
        Entity,
        &mut ConversationLoadTask,
        Has<FailedConversationRead>,
    )>,
    states: Query<&RunState, With<Session>>,
    files: Query<&ConversationFile>,
    watched: Query<&ConversationWatchPaths>,
    children: Query<&Children, With<Session>>,
    existing: Query<
        (
            &vmux_session::EventIdentity,
            &vmux_session::MessageContent,
            &ChildOf,
        ),
        With<vmux_session::ConversationEvent>,
    >,
    transcripts: Transcripts,
    mut snapshots: MessageWriter<SnapshotReceived>,
    mut appended: MessageWriter<ConversationAppendReceived>,
    mut operations: MessageWriter<vmux_session::OperationReceived>,
    mut commands: Commands,
) {
    for (task_entity, mut task, failed_read) in &mut tasks {
        let Some(result) = task.task.poll() else {
            continue;
        };
        let result = result.unwrap_or_else(|_| Err("provider history worker panicked".into()));
        if failed_read
            && (!states
                .get(task.session)
                .is_ok_and(|state| matches!(state, RunState::Errored(_)))
                || result.as_ref().is_ok_and(|read| read.snapshot)
                    && !transcripts.get(task.session).messages.is_empty())
        {
            commands
                .entity(task.session)
                .try_remove::<ConversationLoadPending>();
            commands.entity(task_entity).despawn();
            continue;
        }
        match result {
            Ok(read) => {
                if !read.watch_paths.is_empty()
                    && !watched
                        .get(task.session)
                        .is_ok_and(|paths| paths.0 == read.watch_paths)
                {
                    commands
                        .entity(task.session)
                        .insert(ConversationWatchPaths(read.watch_paths));
                }
                if !read.records.is_empty() {
                    let mut indexed = HashMap::new();
                    let mut sequence = 0u64;
                    for child in children
                        .get(task.session)
                        .into_iter()
                        .flat_map(|children| children.iter())
                    {
                        let Ok((identity, content, _)) = existing.get(child) else {
                            continue;
                        };
                        sequence = sequence.max(identity.sequence);
                        indexed.insert(identity.event_id.clone(), (identity.sequence, &content.0));
                    }
                    for record in read.records {
                        let id = vmux_api::session::EventId::new(format!(
                            "session:{}:external:{}",
                            task.sid.0, record.id
                        ));
                        let event_sequence = if let Some((sequence, content)) = indexed.get(&id) {
                            if **content == record.message {
                                continue;
                            }
                            *sequence
                        } else {
                            sequence = sequence.saturating_add(1);
                            sequence
                        };
                        let actor =
                            if matches!(&record.message, vmux_api::session::Message::User { .. }) {
                                vmux_api::session::MemberId::local(&task.sid)
                            } else {
                                vmux_api::session::MemberId::agent(&task.sid)
                            };
                        operations.write(vmux_session::OperationReceived(
                            vmux_api::session::ConversationEvent {
                                event_id: id,
                                session_id: task.sid.clone(),
                                actor_id: actor,
                                client_op_id: None,
                                server_seq: event_sequence,
                                created_at_ms: task.revision.max(0) as u64,
                                reply_to: None,
                                message: record.message,
                            },
                        ));
                    }
                }
                if let Some(path) = read.path
                    && !files.get(task.session).is_ok_and(|file| file.0 == path)
                {
                    commands.entity(task.session).insert(ConversationFile(path));
                }
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
    existing: Query<
        (
            Entity,
            &SessionId,
            &AgentId,
            &AcpSessionId,
            Option<&ProviderKind>,
            Option<&ProviderSessionId>,
        ),
        (With<Session>, Without<SessionDuplicate>),
    >,
    tombstones: Res<ProviderSessionTombstones>,
    mut pending: ResMut<PendingImports>,
    mut creates: MessageWriter<CreateRequest>,
    mut opener: ResumeOpener,
) {
    let request = &trigger.event().payload;
    if !request.kind.starts_with("acp:") {
        return;
    }
    if tombstones.contains(&request.kind, &request.sid) {
        return;
    }
    let Some(mut source) = catalog.get(&request.kind, &request.sid).cloned() else {
        return;
    };
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
    let family = AgentFamily::of(&source.agent);
    if let Some((_, id, _, _, _, _)) =
        existing
            .iter()
            .find(|(_, _, agent, resume, kind, provider_sid)| {
                kind.is_some_and(|kind| kind.0 == source.kind)
                    && provider_sid.is_some_and(|sid| sid.0 == source.sid)
                    || AgentFamily::of(&agent.0) == family && resume.0 == source.sid
            })
    {
        opener.open(destination, Route::Session(id.clone()).url());
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
    let deferred_open = !destination.opens_in_place();
    let reservation = pending.reserve(
        create.id().clone(),
        PendingImport {
            destination,
            deferred_open,
            source,
            agent,
        },
    );
    if reservation.open_now {
        opener.open(destination, Route::Session(reservation.id.clone()).url());
    }
    if reservation.created {
        creates.write(create);
    }
}

fn attach(
    trigger: On<Created>,
    mut pending: ResMut<PendingImports>,
    mut opener: ResumeOpener,
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
        AcpSessionId(import.source.sid.clone()),
        ProviderKind(import.source.kind),
        ProviderSessionId(import.source.sid),
        AgentConversationTitle(title),
    ));
    if import.deferred_open {
        opener.open(import.destination, Route::Session(event.id.clone()).url());
    }
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
    use std::io::Write;

    #[test]
    fn matching_conversations_requires_the_same_family_even_with_equal_provider_ids() {
        let changed = ProviderConversationChanged {
            session: ProviderSessionKey::new(
                ProviderKind("acp:codex".into()),
                ProviderSessionId("same".into()),
            ),
            updated_at: 1,
        };
        assert!(changed.matches("acp:codex-acp", "same"));
        assert!(!changed.matches("acp:claude", "same"));
        assert!(!changed.matches("acp:mistral-vibe", "same"));
    }

    #[test]
    fn replay_matching_ignores_display_metadata_and_assistant_message_grouping() {
        let tool = vmux_api::session::AssistantBlock::ToolUse {
            call_id: "call".into(),
            name: "Bash".into(),
            args: "{}".into(),
            parent_call_id: None,
            kind: Some(vmux_api::session::ToolKind::Execute),
        };
        let known = KnownConversation(vec![vmux_api::session::Message::Assistant {
            blocks: vec![
                vmux_api::session::AssistantBlock::Text("context".into()),
                tool.clone(),
            ],
        }]);
        let mut replay = tool;
        if let vmux_api::session::AssistantBlock::ToolUse { kind, name, .. } = &mut replay {
            *kind = None;
            *name = "Execute command".into();
        }
        let unseen = known.unseen(vec![
            vmux_api::session::Message::Assistant {
                blocks: vec![vmux_api::session::AssistantBlock::Text("context".into())],
            },
            vmux_api::session::Message::Assistant {
                blocks: vec![replay],
            },
            vmux_api::session::Message::user("new"),
        ]);
        assert_eq!(unseen, [vmux_api::session::Message::user("new")]);
    }

    #[test]
    fn shared_jsonl_reader_follows_a_wrapped_chat_log_with_a_cached_path() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("projects/work");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("thread.jsonl");
        std::fs::write(
            &path,
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"first\"}}\n",
        )
        .unwrap();
        let paths = SourcePaths {
            codex: directory.path().into(),
            claude: directory.path().into(),
            vibe: directory.path().into(),
        };
        let first = ConversationSource::new("acp:claude", "thread", 0)
            .unwrap()
            .read(&paths, None, Vec::new(), NativeReaders::default(), None)
            .unwrap();
        assert_eq!(first.messages, [vmux_api::session::Message::user("first")]);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"next\"}]}}\n").unwrap();
        let next = ConversationSource::new("acp:claude", "thread", 0)
            .unwrap()
            .read(
                &paths,
                Some(first.cursor),
                first.messages,
                NativeReaders::default(),
                first.path,
            )
            .unwrap();
        assert_eq!(
            next.messages,
            [vmux_api::session::Message::Assistant {
                blocks: vec![vmux_api::session::AssistantBlock::Text("next".into())]
            }]
        );
    }

    #[test]
    fn conversation_watch_reads_appends_before_the_writer_closes_and_releases_unused_files() {
        let mut fixture = FailedReadFixture::new();
        fixture.finish();
        let path = fixture
            .app
            .world()
            .get::<ConversationFile>(fixture.session)
            .unwrap()
            .0
            .clone();
        let cursor = fixture
            .app
            .world()
            .get::<ConversationCursor>(fixture.session)
            .unwrap()
            .0;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        let events = SourceWatchEvents::start(None);
        let mut watch = ConversationWatch::start(events.sender.clone());
        let key = ProviderSessionKey::new(
            ProviderKind("acp:codex".into()),
            ProviderSessionId("01a14fcc-f46a-7c12-b000-779be98d72ae".into()),
        );
        watch.set_paths(
            [ConversationTarget {
                session: key.clone(),
                path: path.clone(),
            }]
            .into_iter()
            .collect(),
        );
        events
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        file.write_all(b"{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"live update\"}]}}\n").unwrap();
        file.flush().unwrap();
        let changed = events
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(changed.conversations.contains_key(&key));
        let read = codex::Reader::at(fixture._directory.path())
            .conversation(
                "01a14fcc-f46a-7c12-b000-779be98d72ae",
                Timestamp::parse("2026-10-18T16:16:34.454Z"),
                Some(cursor),
            )
            .unwrap();
        assert_eq!(
            read.messages,
            [vmux_api::session::Message::user("live update")]
        );
        watch.set_paths(HashSet::new());
        std::thread::sleep(CONVERSATION_POLL_INTERVAL * 2);
        while events.receiver.try_recv().is_ok() {}
        file.write_all(b"after detach\n").unwrap();
        file.flush().unwrap();
        assert!(matches!(
            events.receiver.recv_timeout(CONVERSATION_POLL_INTERVAL * 3),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
    }

    struct FailedReadFixture {
        app: App,
        session: Entity,
        _directory: tempfile::TempDir,
        release: Vec<mpsc::SyncSender<()>>,
        _blockers: Vec<BlockingTask<()>>,
    }

    impl FailedReadFixture {
        fn new() -> Self {
            Self::with_cached(Vec::new())
        }

        fn with_cached(cached: Vec<vmux_api::session::Message>) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let rollout = directory.path().join("sessions/2026/10/18/rollout-2026-10-18T18-16-34-01a14fcc-f46a-7c12-b000-779be98d72ae.jsonl");
            std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
            std::fs::write(&rollout, concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"session_id\":\"01a14fcc-f46a-7c12-b000-779be98d72ae\",\"timestamp\":\"2026-10-18T16:16:34.454Z\",\"cwd\":\"/work/vmux\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"original conversation\"}]}}\n"
            )).unwrap();
            if !cached.is_empty() {
                let mut body = std::fs::read_to_string(&rollout).unwrap();
                body.push_str("{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"new message after cached history\"}]}}\n");
                std::fs::write(&rollout, body).unwrap();
            }
            let workers = BlockingRuntime::default();
            let mut release = Vec::new();
            let mut blockers = Vec::new();
            for _ in 0..2 {
                let (started_tx, started_rx) = mpsc::sync_channel(1);
                let (release_tx, release_rx) = mpsc::sync_channel(1);
                blockers.push(
                    workers
                        .spawn_long(None, move || {
                            started_tx.send(()).unwrap();
                            let _ = release_rx.recv_timeout(Duration::from_secs(5));
                        })
                        .unwrap(),
                );
                started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                release.push(release_tx);
            }
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(workers)
                .add_plugins(HistoryReadPlugin);
            app.world_mut().spawn(SourcePaths {
                codex: directory.path().to_path_buf(),
                claude: directory.path().join("claude"),
                vibe: directory.path().join("vibe"),
            });
            let session = app
                .world_mut()
                .spawn((
                    Session,
                    SessionId("canonical".into()),
                    ProviderKind("acp:codex-acp".into()),
                    ProviderSessionId("01a14fcc-f46a-7c12-b000-779be98d72ae".into()),
                    AcpSessionId("01a14fcc-f46a-7c12-b000-779be98d72ae".into()),
                    CreatedAt(Timestamp::parse("2026-10-18T16:16:34.454Z")),
                    RunState::Errored("thread_active_writer".into()),
                ))
                .id();
            for (sequence, message) in cached.into_iter().enumerate() {
                app.world_mut().spawn((
                    vmux_session::ConversationEvent,
                    vmux_session::EventIdentity {
                        event_id: vmux_api::session::EventId::new(format!("cached-{sequence}")),
                        actor_id: vmux_api::session::MemberId::local(&SessionId::new("canonical")),
                        client_op_id: None,
                        sequence: sequence as u64,
                        reply_to: None,
                    },
                    vmux_session::MessageContent(message),
                    CreatedAt(1),
                    ChildOf(session),
                ));
            }
            app.update();
            Self {
                app,
                session,
                _directory: directory,
                release,
                _blockers: blockers,
            }
        }

        fn finish(&mut self) -> Vec<SnapshotReceived> {
            for release in self.release.drain(..) {
                release.send(()).unwrap();
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            while self
                .app
                .world()
                .get::<ConversationLoadPending>(self.session)
                .is_some()
            {
                assert!(
                    Instant::now() < deadline,
                    "provider history read did not finish"
                );
                self.app.update();
                std::thread::sleep(Duration::from_millis(2));
            }
            self.app
                .world_mut()
                .resource_mut::<Messages<SnapshotReceived>>()
                .drain()
                .collect()
        }

        fn native(
            &mut self,
            message: vmux_api::session::Message,
        ) -> Vec<vmux_api::session::ConversationEvent> {
            let session = self.session;
            self.app
                .world_mut()
                .entity_mut(session)
                .insert(ConversationLoadPending);
            self.app.world_mut().spawn(ConversationLoadTask {
                session,
                sid: SessionId::new("canonical"),
                revision: 1,
                task: vmux_ecs::BlockingPool::new("test-catalog", 1, 1)
                    .spawn(None, move || {
                        Ok(ConversationRead {
                            path: None,
                            watch_paths: Vec::new(),
                            records: vec![conversation::Record {
                                id: "stable".into(),
                                message,
                            }],
                            messages: Vec::new(),
                            cursor: 1,
                            snapshot: false,
                            complete: true,
                        })
                    })
                    .unwrap(),
            });
            let deadline = Instant::now() + Duration::from_secs(3);
            while self
                .app
                .world()
                .get::<ConversationLoadPending>(session)
                .is_some()
            {
                assert!(Instant::now() < deadline, "native history update");
                self.app.update();
                std::thread::sleep(Duration::from_millis(2));
            }
            self.app
                .world_mut()
                .resource_mut::<Messages<vmux_session::OperationReceived>>()
                .drain()
                .map(|event| event.0)
                .collect()
        }
    }

    #[test]
    fn native_history_updates_keep_the_record_identity_and_ignore_unchanged_entries() {
        let mut fixture = FailedReadFixture::new();
        fixture.finish();
        let first = fixture.native(vmux_api::session::Message::user("first"));
        assert_eq!(first.len(), 1);
        let event = &first[0];
        let entity = fixture
            .app
            .world_mut()
            .spawn((
                vmux_session::ConversationEvent,
                vmux_session::EventIdentity {
                    event_id: event.event_id.clone(),
                    actor_id: event.actor_id.clone(),
                    client_op_id: None,
                    sequence: event.server_seq,
                    reply_to: None,
                },
                vmux_session::MessageContent(event.message.clone()),
                CreatedAt(1),
                ChildOf(fixture.session),
            ))
            .id();
        let updated = fixture.native(vmux_api::session::Message::user("updated"));
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].event_id, event.event_id);
        assert_eq!(updated[0].server_seq, event.server_seq);
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(vmux_session::MessageContent(updated[0].message.clone()));
        assert!(
            fixture
                .native(vmux_api::session::Message::user("updated"))
                .is_empty()
        );
    }

    #[test]
    fn failed_codex_resume_reads_the_original_history_without_replacing_its_identity() {
        let mut fixture = FailedReadFixture::new();
        let snapshots = fixture.finish();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].session.0, "canonical");
        assert_eq!(
            snapshots[0].messages,
            [vmux_api::session::Message::user("original conversation")]
        );
        assert!(
            matches!(fixture.app.world().get::<RunState>(fixture.session), Some(RunState::Errored(message)) if message == "thread_active_writer")
        );
        assert_eq!(
            fixture.app.world().get::<AcpSessionId>(fixture.session),
            Some(&AcpSessionId("01a14fcc-f46a-7c12-b000-779be98d72ae".into()))
        );
    }

    #[test]
    fn failed_codex_resume_keeps_cached_history_and_appends_new_provider_messages() {
        let cached = vmux_api::session::Message::user("original conversation");
        let mut fixture = FailedReadFixture::with_cached(vec![cached.clone()]);
        assert!(fixture.finish().is_empty());
        let appended = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<ConversationAppendReceived>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(appended.len(), 1);
        assert_eq!(appended[0].session, SessionId::new("canonical"));
        assert_eq!(
            appended[0].messages,
            [vmux_api::session::Message::user(
                "new message after cached history"
            )]
        );
        let mut contents = fixture
            .app
            .world_mut()
            .query::<&vmux_session::MessageContent>();
        assert_eq!(contents.single(fixture.app.world()).unwrap().0, cached);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ProviderKind>(fixture.session)
                .unwrap()
                .0,
            "acp:codex-acp"
        );
    }

    #[test]
    fn local_failure_history_cannot_overwrite_a_successful_retry() {
        let mut fixture = FailedReadFixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.session)
            .insert(RunState::Idle);
        assert!(fixture.finish().is_empty());
    }

    #[test]
    fn a_pending_provider_does_not_hide_completed_catalogs() {
        let mut app = App::new();
        app.init_resource::<SourceSnapshot>()
            .init_resource::<AcpSnapshot>()
            .init_resource::<ProviderCatalog>()
            .insert_resource(CatalogRequests {
                active: true,
                generation: 1,
                refresh: true,
                ..Default::default()
            })
            .add_systems(Update, project);
        let mut snapshot = app.world_mut().resource_mut::<AcpSnapshot>();
        snapshot.0.insert(
            "codex".into(),
            AcpSource {
                state: AcpSourceState::Supported,
                sessions: vec![ProviderSession {
                    kind: "acp:codex".into(),
                    sid: "ready".into(),
                    agent: "codex".into(),
                    ..Default::default()
                }],
            },
        );
        snapshot.0.insert(
            "mistral-vibe".into(),
            AcpSource {
                state: AcpSourceState::Pending,
                sessions: Vec::new(),
            },
        );
        app.update();
        let catalog = app.world().resource::<ProviderCatalog>();
        assert!(catalog.loading);
        assert_eq!(catalog.sessions.len(), 1);
        assert_eq!(catalog.sessions[0].sid, "ready");
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
            manager: true,
            conversation_watch: None,
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
            manager: true,
            conversation_watch: None,
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
    fn codex_watch_classifies_only_catalog_inputs() {
        let home = tempfile::tempdir().unwrap();
        let paths = SourcePaths {
            codex: home.path().join(".codex"),
            claude: home.path().join(".claude"),
            vibe: home.path().join(".vibe"),
        };
        let modified = notify::EventKind::Modify(notify::event::ModifyKind::Any);
        let rollout = paths
            .codex
            .join("sessions/2026/11/01/rollout-session-id.jsonl");

        assert_eq!(
            paths.change(&modified, &paths.codex.join("logs_2.sqlite-wal")),
            None
        );
        assert_eq!(
            paths.change(&modified, &paths.codex.join("state_5.sqlite-wal")),
            Some(SourceChange::CodexArchives)
        );
        assert_eq!(
            paths.change(&modified, &paths.codex.join("session_index.jsonl")),
            Some(SourceChange::Scan(SourceKind::Codex))
        );
        assert_eq!(
            paths.change(&modified, &rollout),
            Some(SourceChange::CodexActivity(rollout))
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
        app.init_resource::<ProviderCatalog>()
            .init_resource::<PendingImports>()
            .init_resource::<ProviderSessionTombstones>()
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
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
            app.world().get::<AcpSessionId>(session),
            Some(&AcpSessionId("provider-session".into()))
        );
        assert_eq!(
            app.world().get::<ProviderKind>(session),
            Some(&ProviderKind("acp:codex".into()))
        );
        assert_eq!(
            app.world().get::<ProviderSessionId>(session),
            Some(&ProviderSessionId("provider-session".into()))
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
    fn provider_resume_defers_new_stack_navigation_until_materialization() {
        let cwd = tempfile::tempdir().unwrap();
        let cwd = cwd.path().canonicalize().unwrap();
        let mut app = App::new();
        app.init_resource::<ProviderCatalog>()
            .init_resource::<PendingImports>()
            .init_resource::<ProviderSessionTombstones>()
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_observer(open)
            .add_observer(attach);
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
                AcpSessionId("replacement-session".into()),
                Name::new("Review pull request"),
                vmux_ecs::Cwd(PathBuf::from("/work/dashboard")),
                CreatedAt(100),
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<ProviderKind>(session),
            Some(&ProviderKind("acp:codex".into()))
        );
        assert_eq!(
            app.world().get::<ProviderSessionId>(session),
            Some(&ProviderSessionId("source-session".into()))
        );
    }

    #[test]
    fn acp_history_loading_waits_for_protocol_replay() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<ServiceRequest>()
            .add_message::<SnapshotReceived>()
            .add_message::<ConversationAppendReceived>()
            .add_message::<ProviderArchiveWriteRequest>()
            .add_message::<ProviderArchiveCompleted>()
            .add_message::<ArchiveRequest>()
            .add_message::<AcpPackageChanged>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_message::<CreateRequest>()
            .add_plugins(Plugin);
        let (_send, changes) = mpsc::channel();
        app.world_mut().spawn((
            SourcePaths {
                codex: dir.path().join("codex"),
                claude: dir.path().join("claude"),
                vibe: dir.path().join("vibe"),
            },
            Runtime {
                changes: Mutex::new(changes),
                watcher: None,
                watched: HashMap::new(),
                sources: HashSet::new(),
                watch_codex_activity: false,
                manager: false,
                conversation_watch: None,
            },
        ));
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("canonical".into()),
                ProviderKind("acp:codex-acp".into()),
                ProviderSessionId("provider".into()),
                CreatedAt(0),
                TranscriptLoading,
            ))
            .id();
        app.update();
        assert!(app.world().entity(session).contains::<TranscriptLoading>());
        assert!(
            app.world_mut()
                .query::<&ConversationLoadTask>()
                .iter(app.world())
                .next()
                .is_none()
        );
    }

    #[test]
    fn provider_resume_reuses_the_canonical_session_after_resume_identity_changes() {
        let cwd = tempfile::tempdir().unwrap();
        let cwd = cwd.path().canonicalize().unwrap();
        let mut app = App::new();
        app.init_resource::<ProviderCatalog>()
            .init_resource::<PendingImports>()
            .init_resource::<ProviderSessionTombstones>()
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
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
        app.world_mut().spawn((
            Session,
            id.clone(),
            AgentId("codex".into()),
            AcpSessionId("replacement-resume-session".into()),
            ProviderKind("acp:codex".into()),
            ProviderSessionId("provider-session".into()),
        ));
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
            .init_resource::<PendingImports>()
            .insert_resource(ProviderSessionTombstones::new([ProviderSessionKey::new(
                ProviderKind("acp:codex".into()),
                ProviderSessionId("provider-session".into()),
            )]))
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
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
            .init_resource::<PendingImports>()
            .init_resource::<ProviderSessionTombstones>()
            .init_resource::<AppSettings>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
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
                ProviderKind("acp:codex".into()),
                ProviderSessionId("provider-session".into()),
                vmux_ecs::LastActivatedAt(200),
                ConversationRevision(200),
            ))
            .id();
        let current = app
            .world_mut()
            .spawn((
                Session,
                SessionId::new("current"),
                ProviderKind("acp:codex".into()),
                ProviderSessionId("provider-session".into()),
                vmux_ecs::LastActivatedAt(300),
                ConversationRevision(300),
            ))
            .id();

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
                ProviderKind("acp:codex-acp".into()),
                ProviderSessionId("provider-session".into()),
                vmux_ecs::LastActivatedAt(200),
                ConversationRevision(200),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<ProviderConversationChanged>>()
            .write(ProviderConversationChanged {
                session: ProviderSessionKey::new(
                    ProviderKind("acp:codex".into()),
                    ProviderSessionId("provider-session".into()),
                ),
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
            ProviderKind("acp:codex".into()),
            ProviderSessionId("locally-active".into()),
            SourceUpdatedAt(300),
        ));
        app.world_mut().spawn((
            ProviderArchive,
            ProviderKind("acp:codex".into()),
            ProviderSessionId("locally-archived".into()),
            SourceUpdatedAt(300),
            ArchivedAt(300),
        ));
        app.world_mut().spawn((
            ProviderArchive,
            ProviderKind("acp:codex".into()),
            ProviderSessionId("provider-newer".into()),
            SourceUpdatedAt(300),
            ArchivedAt(300),
        ));
        app.world_mut().spawn((
            Session,
            SessionId::new("active-session"),
            ProviderKind("acp:codex".into()),
            ProviderSessionId("locally-active".into()),
            vmux_ecs::LastActivatedAt(0),
        ));
        app.world_mut().spawn((
            Session,
            SessionId::new("provider-newer-session"),
            ProviderKind("acp:codex".into()),
            ProviderSessionId("provider-newer".into()),
            vmux_ecs::LastActivatedAt(0),
            ArchivedAt(300),
        ));
        app.world_mut().spawn((
            Session,
            SessionId::new("archived-session"),
            ProviderKind("acp:codex".into()),
            ProviderSessionId("locally-archived".into()),
            vmux_ecs::LastActivatedAt(0),
        ));
        app.world_mut().spawn((
            Session,
            SessionId::new("session-newer"),
            ProviderKind("acp:codex".into()),
            ProviderSessionId("session-newer".into()),
            vmux_ecs::LastActivatedAt(0),
            ArchivedAt(400),
        ));

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
}
