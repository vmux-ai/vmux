use std::collections::{HashMap, HashSet};
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
    ArchivedAt, BlockingRuntime, BlockingTask, CreatedAt, EntityTarget, PageOpenRequest,
    PageOpenTarget,
};
use vmux_layout::{TabLayoutSpawnContent, TabLayoutSpawnRequest};
use vmux_session::history::{
    Committed as HistoryCommitted, FileFacts, Prepare as HistoryPrepare, Read as HistoryRead,
    Ready as HistoryReady, Received as HistoryReceived, Versions as HistoryVersions,
};
use vmux_session::host::{SessionManagerView, SessionView, SessionViews};
use vmux_session::provider::{
    Capability as CatalogCapability, Destination as ResumeDestination,
    Discovery as CatalogDiscovery, Entries as ProviderEntries, Generation as CatalogGeneration,
    Import, ImportRequested, Imported, Opened, Opening, Origin as CatalogOrigin,
    Prepared as CatalogPrepared, Read as CatalogRead, Ready as CatalogReady,
    Received as CatalogReceived, Source as CatalogSource, Started as CatalogReadStarted,
};
use vmux_session::{
    AgentFamily, AgentId, ArchiveRequest, ArchiveState, ConversationAppendReceived,
    ProviderArchive, ProviderKind, ProviderSession, ProviderSessionId, ProviderSessionKey,
    ProviderSessionTombstones, Route, RunState, Session, SessionDuplicate, SessionId,
    SnapshotReceived, SourceUpdatedAt, TranscriptLoading, Transcripts,
};
#[cfg(test)]
use vmux_session::{CreateRequest, Created, ProviderArchiveCompleted, ProviderArchiveWriteRequest};
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
        if !app.is_plugin_added::<vmux_session::provider::Plugin>() {
            app.add_plugins(vmux_session::provider::Plugin);
        }
        app.add_plugins((
            CatalogPlugin,
            ImportPlugin,
            archive::Plugin,
            HistoryReadPlugin,
        ))
        .init_resource::<NativeReaders>()
        .init_resource::<ActivationDirty>()
        .add_message::<ToolInventoryChanged>()
        .add_message::<ProviderConversationChanged>()
        .add_message::<vmux_session::OperationReceived>()
        .add_systems(
            Update,
            (
                mark_activation_dirty,
                activate,
                cancel_scans,
                watch,
                request_changed_conversations,
                finish,
                scan,
            )
                .chain()
                .after(CatalogPrepared)
                .before(CatalogReady),
        )
        .add_systems(
            Update,
            (
                complete_discovery,
                associate,
                request_conversation_refreshes,
            )
                .chain()
                .after(CatalogReady),
        );
    }
}

struct ImportPlugin;

impl bevy::prelude::Plugin for ImportPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_session::provider::Plugin>() {
            app.add_plugins(vmux_session::provider::Plugin);
        }
        app.init_resource::<AppSettings>()
            .add_message::<PageOpenRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_observer(open)
            .add_observer(opening)
            .add_observer(imported);
    }
}

struct CatalogPlugin;

impl bevy::prelude::Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_session::provider::Plugin>() {
            app.add_plugins(vmux_session::provider::Plugin);
        }
        app.add_message::<AcpPackageChanged>()
            .add_message::<ToolInventoryChanged>()
            .add_message::<ServiceRequest>()
            .add_service_message::<AcpCatalogReceived>()
            .add_systems(
                Update,
                (
                    register,
                    request_catalogs,
                    receive_catalogs.after(ServiceMessageSet),
                )
                    .chain()
                    .before(CatalogPrepared),
            );
        app.world_mut()
            .spawn((CatalogControl::default(), CatalogDiscovery::default()));
    }
}

struct HistoryReadPlugin;

impl bevy::prelude::Plugin for HistoryReadPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_session::history::Plugin>() {
            app.add_plugins(vmux_session::history::Plugin);
        }
        app.init_resource::<BlockingRuntime>()
            .add_message::<SnapshotReceived>()
            .add_message::<ConversationAppendReceived>()
            .add_message::<vmux_session::OperationReceived>()
            .add_systems(
                Update,
                (
                    reset_source,
                    queue_initial_conversations,
                    queue_failed_conversations,
                    queue_conversation_refreshes,
                    finish_conversations,
                )
                    .chain()
                    .after(request_conversation_refreshes)
                    .after(HistoryPrepare)
                    .before(HistoryReady),
            )
            .add_systems(Update, commit_conversations.after(HistoryReady));
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

#[derive(Resource)]
struct ActivationDirty(bool);

impl Default for ActivationDirty {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Component, Default)]
struct CatalogControl {
    active: bool,
    refresh: bool,
    generation: u64,
    next_request_id: u64,
}

#[derive(Component)]
struct CatalogQueued(vmux_api::protocol::ClientMessage);

#[derive(Component)]
struct CatalogStarted(Instant);

#[derive(Component)]
struct AcpCatalogRead;

struct CatalogCwd;

impl CatalogCwd {
    fn current() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .or_else(|| std::env::current_dir().ok().filter(|path| path.is_dir()))
    }
}

fn register(
    agents: Query<&RegistryAgent>,
    settings: Option<Res<AppSettings>>,
    sources: Query<(Entity, &AgentId, &CatalogOrigin, Option<&Name>), With<CatalogSource>>,
    mut state: Single<&mut CatalogControl>,
    mut commands: Commands,
) {
    let Some(settings) = settings else {
        return;
    };
    let mut registered = HashMap::new();
    for (entity, agent, origin, name) in &sources {
        if *origin == CatalogOrigin::Acp {
            registered.insert(agent.0.as_str(), (entity, name));
        }
    }
    let mut present = HashSet::new();
    let mut installed_families = HashSet::new();
    for agent in &agents {
        if !agent.is_installed() {
            continue;
        }
        present.insert(agent.id.as_str());
        installed_families.insert(AgentFamily::of(&agent.id));
        if let Some((source, name)) = registered.get(agent.id.as_str()) {
            if name.is_none_or(|name| name.as_str() != agent.name) {
                commands
                    .entity(*source)
                    .insert(Name::new(agent.name.clone()));
                state.refresh = true;
            }
            continue;
        }
        commands.spawn((
            CatalogSource,
            CatalogOrigin::Acp,
            CatalogCapability::Unprobed,
            CatalogGeneration(state.generation),
            AgentId(agent.id.clone()),
            Name::new(agent.name.clone()),
        ));
        state.refresh = true;
    }
    for config in &settings.agent.acp {
        if config.command.trim().is_empty()
            || present.contains(config.id.as_str())
            || installed_families.contains(&AgentFamily::of(&config.id))
        {
            continue;
        }
        present.insert(config.id.as_str());
        if let Some((source, name)) = registered.get(config.id.as_str()) {
            if name.is_none_or(|name| name.as_str() != config.name) {
                commands
                    .entity(*source)
                    .insert(Name::new(config.name.clone()));
                state.refresh = true;
            }
            continue;
        }
        commands.spawn((
            CatalogSource,
            CatalogOrigin::Acp,
            CatalogCapability::Unprobed,
            CatalogGeneration(state.generation),
            AgentId(config.id.clone()),
            Name::new(config.name.clone()),
        ));
        state.refresh = true;
    }
    for (agent, (source, _)) in registered {
        if !present.contains(agent) {
            commands.entity(source).despawn();
            state.refresh = true;
        }
    }
}

fn request_catalogs(
    activity: Activity<SessionManagerView>,
    connected: Query<Ref<ServiceConnected>>,
    unavailable: Query<(), With<vmux_ecs::service::ServiceUnavailable>>,
    agents: Query<Ref<RegistryAgent>>,
    settings: Option<Res<AppSettings>>,
    mut package_changes: MessageReader<AcpPackageChanged>,
    mut tool_changes: MessageReader<ToolInventoryChanged>,
    mut state: Single<&mut CatalogControl>,
    mut requests: MessageWriter<ServiceRequest>,
    sources: Query<
        (
            Entity,
            &AgentId,
            &CatalogOrigin,
            Option<&EntityTarget<CatalogRead>>,
            &CatalogCapability,
        ),
        With<CatalogSource>,
    >,
    operations: Query<
        (
            Entity,
            &ChildOf,
            &CatalogRead,
            &vmux_ecs::OperationState,
            Option<&CatalogStarted>,
            Option<&CatalogQueued>,
        ),
        With<AcpCatalogRead>,
    >,
    mut commands: Commands,
) {
    let mut source_entities = HashMap::new();
    for (entity, agent, origin, _, _) in &sources {
        if *origin == CatalogOrigin::Acp {
            source_entities.insert(agent.0.clone(), entity);
        }
    }
    let package_changed = package_changes.read().count() > 0;
    let mut tool_changed = false;
    for changed in tool_changes.read() {
        tool_changed |= changed.0.is("acp");
    }
    let registry_changed = agents.iter().any(|agent| agent.is_changed());
    let present = Activity::present(activity);
    let online = !connected.is_empty();
    if connected.iter().any(|connected| connected.is_added()) {
        state.refresh = true;
    }
    if present && !online && !unavailable.is_empty() {
        for source in source_entities.values() {
            if sources
                .get(*source)
                .is_ok_and(|(_, _, _, _, capability)| *capability != CatalogCapability::Failed)
            {
                commands.entity(*source).insert(CatalogCapability::Failed);
            }
        }
        state.refresh = true;
    }
    let mut canceled = HashSet::new();
    for (operation, parent, read, status, started, _) in &operations {
        if !matches!(
            status,
            vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
        ) {
            continue;
        }
        let timeout = started.is_some_and(|started| started.0.elapsed() >= CATALOG_REQUEST_TIMEOUT);
        if present && online && !timeout {
            continue;
        }
        if started.is_some() && online {
            requests.write(ServiceRequest(
                vmux_api::protocol::ClientMessage::CancelAcpSessionCatalog {
                    request_id: read.wire_id,
                },
            ));
        }
        commands
            .entity(operation)
            .insert(if timeout {
                vmux_ecs::OperationState::Failed
            } else {
                vmux_ecs::OperationState::Canceled
            })
            .remove::<CatalogQueued>();
        canceled.insert(operation);
        let source = parent.parent();
        if let Ok((_, _, _, target, _)) = sources.get(source)
            && target.is_some_and(|target| target.entity() == operation)
        {
            commands
                .entity(source)
                .insert(CatalogCapability::Failed)
                .remove::<EntityTarget<CatalogRead>>();
        }
    }
    if !present {
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
    if !online {
        if !canceled.is_empty() {
            state.refresh = true;
        }
        return;
    }
    let mut outstanding = 0;
    let mut running = 0;
    let mut queued = Vec::new();
    for (entity, _, read, status, started, wire) in &operations {
        if canceled.contains(&entity)
            || !matches!(
                status,
                vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
            )
        {
            continue;
        }
        outstanding += 1;
        if started.is_some() {
            running += 1;
        } else if wire.is_some() {
            queued.push((read.wire_id, entity));
        }
    }
    if state.refresh && outstanding == 0 {
        let Some(settings) = settings else {
            return;
        };
        let Some(cwd) = CatalogCwd::current() else {
            return;
        };
        state.refresh = false;
        let Some(generation) = state.generation.checked_add(1) else {
            for source in source_entities.values() {
                commands.entity(*source).insert(CatalogCapability::Failed);
                if let Ok(error) = vmux_ecs::OperationError::new("catalog generation exhausted") {
                    commands.entity(*source).insert(error);
                }
            }
            return;
        };
        state.generation = generation;
        let shell = settings.shell();
        let mut installed = HashSet::new();
        let mut installed_families = HashSet::new();
        let mut created = Vec::new();
        for agent in &agents {
            if !agent.is_installed() {
                continue;
            }
            installed.insert(agent.id.clone());
            installed_families.insert(AgentFamily::of(&agent.id));
            let Some(source_entity) = source_entities.get(&agent.id).copied() else {
                continue;
            };
            commands
                .entity(source_entity)
                .insert((Name::new(agent.name.clone()), CatalogGeneration(generation)));
            let launch = match AcpLaunch::installed_catalog(&agent, &shell) {
                Ok(launch) => launch,
                Err(error) => {
                    bevy::log::warn!("failed to resolve installed ACP {}: {error}", agent.id);
                    commands
                        .entity(source_entity)
                        .insert(CatalogCapability::Failed);
                    if let Ok(error) = vmux_ecs::OperationError::new(error) {
                        commands.entity(source_entity).insert(error);
                    }
                    continue;
                }
            };
            let Some(request_id) = state.next_request_id.checked_add(1) else {
                commands
                    .entity(source_entity)
                    .insert(CatalogCapability::Failed);
                if let Ok(error) = vmux_ecs::OperationError::new("catalog wire ID exhausted") {
                    commands.entity(source_entity).insert(error);
                }
                continue;
            };
            state.next_request_id = request_id;
            let operation = commands
                .spawn((
                    AcpCatalogRead,
                    CatalogRead {
                        generation,
                        wire_id: request_id,
                    },
                    vmux_ecs::Operation::bundle("provider catalog")
                        .expect("catalog operation kind"),
                    ChildOf(source_entity),
                ))
                .id();
            commands
                .entity(source_entity)
                .insert((
                    Name::new(agent.name.clone()),
                    CatalogGeneration(generation),
                    CatalogCapability::Pending,
                    EntityTarget::<CatalogRead>::new(operation),
                ))
                .remove::<moonshine_save::prelude::Save>();

            let message = launch.catalog_message(request_id, &agent.id, &agent.name, &cwd);
            if created.len() < CATALOG_CONCURRENCY {
                requests.write(ServiceRequest(message));
                commands.entity(operation).insert((
                    vmux_ecs::OperationState::Running,
                    CatalogStarted(Instant::now()),
                ));
                commands
                    .entity(source_entity)
                    .insert(CatalogCapability::Pending);
            } else {
                commands.entity(operation).insert(CatalogQueued(message));
            }
            created.push(operation);
        }
        for config in &settings.agent.acp {
            if config.command.trim().is_empty()
                || installed.contains(&config.id)
                || installed_families.contains(&AgentFamily::of(&config.id))
            {
                continue;
            }
            installed.insert(config.id.clone());
            let Some(source_entity) = source_entities.get(&config.id).copied() else {
                continue;
            };
            commands.entity(source_entity).insert((
                Name::new(config.name.clone()),
                CatalogGeneration(generation),
            ));
            let launch = match AcpLaunch::configured_catalog(config, &shell) {
                Ok(launch) => launch,
                Err(error) => {
                    bevy::log::debug!("configured ACP catalog skipped for {}: {error}", config.id);
                    commands
                        .entity(source_entity)
                        .insert(CatalogCapability::Failed);
                    if let Ok(error) = vmux_ecs::OperationError::new(error) {
                        commands.entity(source_entity).insert(error);
                    }
                    continue;
                }
            };
            let Some(request_id) = state.next_request_id.checked_add(1) else {
                commands
                    .entity(source_entity)
                    .insert(CatalogCapability::Failed);
                if let Ok(error) = vmux_ecs::OperationError::new("catalog wire ID exhausted") {
                    commands.entity(source_entity).insert(error);
                }
                continue;
            };
            state.next_request_id = request_id;
            let operation = commands
                .spawn((
                    AcpCatalogRead,
                    CatalogRead {
                        generation,
                        wire_id: request_id,
                    },
                    vmux_ecs::Operation::bundle("provider catalog")
                        .expect("catalog operation kind"),
                    ChildOf(source_entity),
                ))
                .id();
            commands
                .entity(source_entity)
                .insert((
                    Name::new(config.name.clone()),
                    CatalogGeneration(generation),
                    CatalogCapability::Pending,
                    EntityTarget::<CatalogRead>::new(operation),
                ))
                .remove::<moonshine_save::prelude::Save>();

            let configured_cwd = config
                .cwd
                .as_deref()
                .filter(|path| path.is_dir())
                .unwrap_or(&cwd);
            let message =
                launch.catalog_message(request_id, &config.id, &config.name, configured_cwd);
            if created.len() < CATALOG_CONCURRENCY {
                requests.write(ServiceRequest(message));
                commands.entity(operation).insert((
                    vmux_ecs::OperationState::Running,
                    CatalogStarted(Instant::now()),
                ));
                commands
                    .entity(source_entity)
                    .insert(CatalogCapability::Pending);
            } else {
                commands.entity(operation).insert(CatalogQueued(message));
            }
            created.push(operation);
        }

        return;
    }
    queued.sort_by_key(|(order, _)| *order);
    for (_, operation) in queued {
        if running >= CATALOG_CONCURRENCY {
            break;
        }
        let Ok((_, parent, _, _, _, Some(wire))) = operations.get(operation) else {
            continue;
        };
        if sources.get(parent.parent()).is_err() {
            continue;
        }
        requests.write(ServiceRequest(wire.0.clone()));
        commands
            .entity(operation)
            .insert((
                vmux_ecs::OperationState::Running,
                CatalogStarted(Instant::now()),
            ))
            .remove::<CatalogQueued>();
        commands
            .entity(parent.parent())
            .insert(CatalogCapability::Pending);
        running += 1;
    }
}

fn receive_catalogs(
    mut received: MessageReader<AcpCatalogReceived>,
    operations: Query<
        (Entity, &ChildOf, &CatalogRead, &vmux_ecs::OperationState),
        (Without<CatalogQueued>, With<CatalogStarted>),
    >,
    sources: Query<
        (
            &AgentId,
            &CatalogGeneration,
            &EntityTarget<CatalogRead>,
            &CatalogOrigin,
        ),
        With<CatalogSource>,
    >,
    mut canonical: MessageWriter<CatalogReceived>,
) {
    for received in received.read() {
        let Some((operation, parent, read, status)) = operations
            .iter()
            .find(|(_, _, read, _)| read.wire_id == received.request_id)
        else {
            continue;
        };
        let Ok((agent, generation, target, origin)) = sources.get(parent.parent()) else {
            continue;
        };
        if *origin != CatalogOrigin::Acp
            || agent.0 != received.agent_id
            || generation.0 != read.generation
            || target.entity() != operation
            || *status != vmux_ecs::OperationState::Running
        {
            continue;
        }
        let result = match &received.error {
            Some(error) => Err(error.clone()),
            None => Ok(AcpCatalog::sessions(received)),
        };
        canonical.write(CatalogReceived {
            operation,
            supported: received.supported,
            result,
        });
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
struct ScanRequested;

#[derive(SystemParam)]
struct NativeSources<'w, 's> {
    entries: Query<'w, 's, (Entity, &'static SourceKind), With<CatalogSource>>,
    commands: Commands<'w, 's>,
}

impl NativeSources<'_, '_> {
    fn entity(&mut self, kind: SourceKind) -> Entity {
        if let Some((entity, _)) = self.entries.iter().find(|(_, found)| **found == kind) {
            return entity;
        }
        self.commands.spawn(kind.bundle()).id()
    }

    fn queue(&mut self, kind: SourceKind) {
        let entity = self.entity(kind);
        self.commands
            .entity(entity)
            .insert((ScanRequested, CatalogCapability::Pending));
    }

    fn archives(&mut self, kind: SourceKind) {
        let entity = self.entity(kind);
        self.commands
            .entity(entity)
            .insert(archive::RefreshRequested);
    }
}

#[derive(Component)]
struct ScanTask {
    task: BlockingTask<Result<Vec<ProviderSession>, String>>,
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
struct ConversationSourceScope(ProviderSessionKey);

#[derive(Component)]
struct ConversationLoadTask {
    revision: i64,
    task: BlockingTask<Result<conversation::ReadResult, String>>,
}

#[derive(Component)]
struct ConversationReadReady {
    revision: i64,
    path: Option<PathBuf>,
    watch_paths: Vec<PathBuf>,
    cursor: u64,
    complete: bool,
}

#[derive(Resource, Clone, Default)]
struct NativeReaders(Arc<Mutex<Option<conversation::ProcessReader>>>);

#[derive(Component)]
struct ConversationWatchPaths(Vec<PathBuf>);

impl conversation::ReadResult {
    fn empty(cursor: Option<u64>) -> Self {
        Self {
            path: None,
            watch_paths: Vec::new(),
            history: HistoryReceived::messages(Vec::new(), cursor.is_none()),
            cursor: cursor.unwrap_or_default(),
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
    fn new(kind: &ProviderKind, sid: &ProviderSessionId, created_at: i64) -> Option<Self> {
        let agent = kind.0.strip_prefix("acp:")?;
        let family = AgentFamily::of(agent);
        let source = SourceKind::for_family(&family)?;
        if !conversation::Files::valid_id(&sid.0) {
            return None;
        }
        Some(Self {
            kind: format!("acp:{}", source.id()),
            sid: sid.0.clone(),
            created_at,
        })
    }

    fn for_failed(kind: &ProviderKind, sid: &ProviderSessionId, created_at: i64) -> Option<Self> {
        Self::new(kind, sid, created_at)
    }

    fn read(
        self,
        paths: &SourcePaths,
        cursor: Option<u64>,
        native: NativeReaders,
        hint: Option<PathBuf>,
        known: Option<FileFacts>,
    ) -> Result<conversation::ReadResult, String> {
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
                        known.as_ref(),
                        conversation::FileConversation::TAIL_BYTES,
                    )?
                } else {
                    codex::Reader::at(&paths.codex).conversation(
                        &self.sid,
                        self.created_at,
                        cursor,
                        known.as_ref(),
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
                    return Ok(conversation::ReadResult::empty(cursor));
                };
                conversation::FileConversation::read(
                    &path,
                    cursor,
                    known.as_ref(),
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
                    return Ok(conversation::ReadResult::empty(cursor));
                };
                conversation::FileConversation::read(
                    &path,
                    cursor,
                    known.as_ref(),
                    conversation::FileConversation::TAIL_BYTES,
                    conversation::ChatDecoder,
                )?
            }
        };
        Ok(read)
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SourceKind {
    Codex,
    Claude,
    Vibe,
}

impl SourceKind {
    const ALL: [Self; 3] = [Self::Codex, Self::Claude, Self::Vibe];

    fn bundle(
        self,
    ) -> (
        CatalogSource,
        CatalogOrigin,
        CatalogCapability,
        CatalogGeneration,
        AgentId,
        Name,
        Self,
    ) {
        (
            CatalogSource,
            CatalogOrigin::Native,
            CatalogCapability::Unprobed,
            CatalogGeneration(0),
            AgentId(self.id().into()),
            Name::new(self.family().name(self.id())),
            self,
        )
    }

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

    fn reader(&self, source: SourceKind, hints: Vec<(String, i64)>) -> Box<dyn Reader> {
        match source {
            SourceKind::Codex => Box::new(codex::Reader::at(&self.codex).with_hints(hints)),
            SourceKind::Claude => Box::new(claude::Reader::at(&self.claude)),
            SourceKind::Vibe => Box::new(vibe::Reader::at(&self.vibe)),
        }
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

struct BoundedLines<R> {
    reader: R,
    remaining: usize,
    consumed: usize,
    line_start: usize,
}

impl<R: BufRead> BoundedLines<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            remaining: PROVIDER_READ_LIMIT,
            consumed: 0,
            line_start: 0,
        }
    }

    fn consumed(&self) -> usize {
        self.consumed
    }

    fn line_start(&self) -> usize {
        self.line_start
    }
}

impl<R: BufRead> Iterator for BoundedLines<R> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        while self.remaining > 0 {
            self.line_start = self.consumed;
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
                if bytes.last() != Some(&b'\n') {
                    let skipped = self.reader.skip_until(b'\n').ok()?;
                    self.remaining = self.remaining.saturating_sub(skipped);
                    self.consumed = self.consumed.saturating_add(skipped);
                }
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
            && ConversationSource::new(
                &ProviderKind(kind.into()),
                &ProviderSessionId(sid.into()),
                0,
            )
            .is_some()
    }

    fn codex(activity: &codex::RolloutActivity) -> Self {
        Self {
            session: ProviderSessionKey::new(
                ProviderKind("acp:codex".to_string()),
                ProviderSessionId(activity.sid().to_string()),
            ),
            updated_at: activity.updated_at().0,
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
    acp: ProviderEntries,
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
    mut native_sources: NativeSources,
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
        if !viewed.contains(&entity) || ConversationSource::new(kind, sid, 0).is_none() {
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
    if let Ok((_, mut runtime, paths)) = runtime.single_mut() {
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
            for source in requested {
                native_sources.queue(source);
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
    commands.spawn((Name::new("ACP provider session catalog"), runtime, paths));
    for source in sources {
        native_sources.queue(source);
    }
}

fn mark_activation_dirty(
    mut acp: ProviderEntries,
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
    targets: Query<(), Changed<EntityTarget<Session>>>,
    mut removed_managers: RemovedComponents<SessionManagerView>,
    mut removed_views: RemovedComponents<SessionView>,
    mut removed_ready: RemovedComponents<PageReady>,
    mut removed_sessions: RemovedComponents<Session>,
    mut removed_providers: RemovedComponents<ProviderKind>,
    mut removed_targets: RemovedComponents<EntityTarget<Session>>,
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
    mut sources: NativeSources,
    mut entries: Query<
        (&ProviderSessionId, &AgentId, &mut SourceUpdatedAt),
        With<vmux_session::provider::Entry>,
    >,
    mut catalogs: Single<&mut CatalogControl>,
    mut conversations: MessageWriter<ProviderConversationChanged>,
) {
    let (_, mut runtime, paths) = runtime.into_inner();
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
        sources.archives(SourceKind::Codex);
    }
    runtime.ensure_watches(paths);
    for path in changes.codex_activity {
        let Some(activity) = codex::RolloutActivity::read(&path) else {
            continue;
        };
        conversations.write(ProviderConversationChanged::codex(&activity));
        let mut refreshed = false;
        for (id, agent, mut updated_at) in &mut entries {
            if id.0 == activity.sid() && AgentFamily::of(&agent.0) == SourceKind::Codex.family() {
                let next = updated_at.0.max(activity.updated_at().0);
                if updated_at.0 != next {
                    updated_at.0 = next;
                }
                refreshed = true;
            }
        }
        if !refreshed {
            if runtime.watch_codex_activity {
                catalogs.refresh = true;
            }
            if runtime.sources.contains(&SourceKind::Codex) {
                changes.scans.insert(SourceKind::Codex);
            }
        }
    }
    if changes.scans.is_empty() {
        return;
    }
    for source in changes.scans {
        sources.queue(source);
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

fn cancel_scans(
    runtimes: Query<&Runtime>,
    sources: Query<
        (
            Entity,
            &SourceKind,
            Option<&EntityTarget<CatalogRead>>,
            Option<&vmux_ecs::OperationError>,
        ),
        With<ScanRequested>,
    >,
    entries: Query<&ChildOf, With<vmux_session::provider::Entry>>,
    mut commands: Commands,
) {
    for (source, kind, target, error) in &sources {
        if runtimes
            .iter()
            .any(|runtime| runtime.sources.contains(kind))
        {
            continue;
        }
        commands.entity(source).remove::<ScanRequested>();
        if target.is_none() {
            let capability = if error.is_some() {
                CatalogCapability::Failed
            } else if entries.iter().any(|parent| parent.parent() == source) {
                CatalogCapability::Supported
            } else {
                CatalogCapability::Unprobed
            };
            commands.entity(source).insert(capability);
        }
    }
}

fn scan(
    runtime: Single<(Entity, &Runtime, &SourcePaths)>,
    entries: ProviderEntries,
    sources: Query<
        (Entity, &SourceKind, &CatalogGeneration),
        (With<CatalogSource>, With<ScanRequested>),
    >,
    reads: Query<&ChildOf, With<ScanTask>>,
    workers: Res<BlockingRuntime>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let (runtime_entity, runtime, paths) = runtime.into_inner();
    let mut active = HashSet::new();
    for parent in &reads {
        active.insert(parent.parent());
    }
    let mut available = CATALOG_CONCURRENCY.saturating_sub(active.len());
    for kind in SourceKind::ALL {
        if available == 0 {
            break;
        }
        let Some((source, _, generation)) = sources
            .iter()
            .find(|(_, source_kind, _)| **source_kind == kind)
        else {
            continue;
        };
        if !runtime.sources.contains(&kind) {
            commands.entity(source).remove::<ScanRequested>();
            continue;
        }
        if active.contains(&source) {
            continue;
        }
        let Some(generation) = generation.0.checked_add(1) else {
            commands.entity(source).insert(CatalogCapability::Failed);
            if let Ok(error) = vmux_ecs::OperationError::new("native catalog generation exhausted")
            {
                commands.entity(source).insert(error);
            }
            commands.entity(source).remove::<ScanRequested>();
            continue;
        };
        let mut hints = Vec::new();
        for entry in entries.from_origin(CatalogOrigin::Native) {
            if AgentFamily::of(&entry.agent.0) == kind.family() && entry.created_at.0 != 0 {
                hints.push((entry.id.0.clone(), entry.created_at.0));
            }
        }
        let reader = paths.reader(kind, hints);
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let started_at = Instant::now();
        let Some(task) = workers.spawn(vmux_ecs::wake::Wake::optional_waker(wake), move || {
            reader.read()
        }) else {
            continue;
        };
        let operation = commands
            .spawn((
                CatalogRead {
                    generation,
                    wire_id: 0,
                },
                CatalogReadStarted(started_at),
                ScanTask { task },
                vmux_ecs::Operation::bundle("provider catalog")
                    .expect("catalog operation kind")
                    .running(),
                EntityTarget::<Runtime>::new(runtime_entity),
                ChildOf(source),
            ))
            .id();
        commands.entity(source).insert((
            CatalogGeneration(generation),
            CatalogCapability::Pending,
            EntityTarget::<CatalogRead>::new(operation),
        ));
        commands.entity(source).remove::<ScanRequested>();
        available -= 1;
    }
}

fn finish(
    mut tasks: Query<(Entity, &ChildOf, &EntityTarget<Runtime>, &mut ScanTask)>,
    runtimes: Query<(), With<Runtime>>,
    sources: Query<Option<&EntityTarget<CatalogRead>>, With<CatalogSource>>,
    mut received: MessageWriter<CatalogReceived>,
    mut commands: Commands,
) {
    for (operation, parent, runtime, mut task) in &mut tasks {
        if !runtimes.contains(runtime.entity()) {
            commands
                .entity(operation)
                .insert(vmux_ecs::OperationState::Canceled)
                .remove::<ScanTask>();
            if sources
                .get(parent.parent())
                .is_ok_and(|target| target.is_some_and(|target| target.entity() == operation))
            {
                commands
                    .entity(parent.parent())
                    .insert(CatalogCapability::Failed)
                    .remove::<EntityTarget<CatalogRead>>();
            }
            continue;
        }
        let Some(result) = task.task.poll() else {
            continue;
        };
        let result = match result {
            Ok(result) => result,
            Err(_) => Err("provider catalog worker panicked".into()),
        };
        received.write(CatalogReceived {
            operation,
            supported: true,
            result,
        });
        commands.entity(operation).remove::<ScanTask>();
    }
}

fn complete_discovery(
    control: Single<&CatalogControl>,
    mut discovery: Single<&mut CatalogDiscovery>,
) {
    if control.active && !discovery.complete {
        discovery.complete = true;
    }
}

fn associate(
    entries: ProviderEntries,
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
    for (entity, agent, resume, name, cwd, created_at) in &sessions {
        let cwd = cwd.0.to_string_lossy();
        let Some(source) =
            entries.association(&agent.0, &resume.0, name.as_str(), &cwd, created_at.0)
        else {
            continue;
        };
        commands
            .entity(entity)
            .insert((source.kind.clone(), source.id.clone()));
    }
}

fn request_conversation_refreshes(
    mut acp: ProviderEntries,
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
    if !acp.is_changed() {
        return;
    }
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
        let Some(source) = acp.get(&kind.0, &provider_sid.0) else {
            continue;
        };
        let source_created_at = source.created_at.0;
        let source_updated_at = source.updated_at.0;
        let source_archive = source
            .archive_observed_at
            .map(|at| match source.archived_at {
                Some(_) => ArchiveState::Archived(at.0),
                None => ArchiveState::Active(at.0),
            });
        let archive_override =
            archive_overrides_by_session.get(&(kind.0.as_str(), provider_sid.0.as_str()));
        let archive = match archive_override {
            Some((source_updated_at, override_archived_at))
                if source_archive
                    .is_none_or(|archive| *source_updated_at >= archive.updated_at()) =>
            {
                Some(match override_archived_at {
                    Some(archived_at) => ArchiveState::Archived(*archived_at),
                    None => ArchiveState::Active(*source_updated_at),
                })
            }
            _ => source_archive,
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
        if activated_at.0 < source_updated_at {
            activated_at.0 = source_updated_at;
        }
        if ConversationSource::new(kind, provider_sid, source_created_at).is_none()
            || revision.is_some_and(|revision| revision.0 >= source_updated_at)
        {
            continue;
        }
        if let Some(mut requested) = requested {
            requested.0 = requested.0.max(source_updated_at);
        } else {
            commands
                .entity(session)
                .insert(ConversationRefreshRequested(source_updated_at));
        }
    }
}

fn reset_source(
    sessions: Query<
        (
            Entity,
            &ProviderKind,
            &ProviderSessionId,
            Option<&ConversationSourceScope>,
            Option<&RunState>,
        ),
        With<Session>,
    >,
    mut commands: Commands,
) {
    for (session, kind, provider, scope, state) in &sessions {
        if scope.is_some_and(|scope| &scope.0.kind == kind && &scope.0.id == provider) {
            continue;
        }
        commands
            .entity(session)
            .insert(ConversationSourceScope(ProviderSessionKey::new(
                kind.clone(),
                provider.clone(),
            )))
            .remove::<(
                ConversationCursor,
                ConversationFile,
                ConversationWatchPaths,
                ConversationRevision,
            )>();
        if matches!(state, Some(RunState::Errored(_))) {
            commands
                .entity(session)
                .insert(FailedConversationReadRequested);
        }
    }
}

fn queue_initial_conversations(
    paths: Single<&SourcePaths>,
    entries: ProviderEntries,
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
    versions: HistoryVersions,
    workers: Res<BlockingRuntime>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (session, _sid, kind, provider_sid, created_at, requested, cursor) in &sessions {
        if kind.0.starts_with("acp:") {
            continue;
        }
        if !transcripts.get(session).messages.is_empty() {
            commands.entity(session).remove::<TranscriptLoading>();
            continue;
        }
        if entries.get(&kind.0, &provider_sid.0).is_none() {
            commands.entity(session).remove::<TranscriptLoading>();
            continue;
        }
        let Some(source) = ConversationSource::new(kind, provider_sid, created_at.0) else {
            commands.entity(session).remove::<TranscriptLoading>();
            continue;
        };
        let revision = requested.map_or_else(
            || {
                entries
                    .get(&kind.0, &provider_sid.0)
                    .map_or(0, |source| source.updated_at.0)
            },
            |requested| requested.0,
        );
        let cursor = cursor.map(|cursor| cursor.0);
        let native = native
            .as_ref()
            .map(|native| (**native).clone())
            .unwrap_or_default();
        let hint = files.get(session).ok().map(|file| file.0.clone());
        let known = versions.file(session);
        let paths = paths.clone();
        let Some(read) = versions.capture(session, false) else {
            continue;
        };
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let Some(task) = workers
            .spawn_long(vmux_ecs::wake::Wake::optional_waker(wake), move || {
                source.read(&paths, cursor, native, hint, known)
            })
        else {
            continue;
        };
        commands
            .entity(session)
            .insert(ConversationLoadPending)
            .remove::<ConversationRefreshRequested>();
        let operation = commands.spawn((
            ChildOf(session),
            read,
            ConversationLoadTask { revision, task },
        ));
        let operation = operation.id();
        commands
            .entity(session)
            .insert(EntityTarget::<HistoryRead>::new(operation));
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
    native: Option<Res<NativeReaders>>,
    files: Query<&ConversationFile>,
    versions: HistoryVersions,
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
    for (session, _sid, kind, provider_sid, created_at, state, cursor) in &sessions {
        if !matches!(state, RunState::Errored(_)) {
            commands
                .entity(session)
                .remove::<FailedConversationReadRequested>();
            continue;
        }
        let Some(source) = ConversationSource::for_failed(kind, provider_sid, created_at.0) else {
            continue;
        };
        let native = native
            .as_ref()
            .map(|native| (**native).clone())
            .unwrap_or_default();
        let hint = files.get(session).ok().map(|file| file.0.clone());
        let known = versions.file(session);
        let paths = paths.clone();
        let cursor = cursor.map(|cursor| cursor.0);
        let Some(read) = versions.capture(session, true) else {
            continue;
        };
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let Some(task) = workers
            .spawn_long(vmux_ecs::wake::Wake::optional_waker(wake), move || {
                source.read(&paths, cursor, native, hint, known)
            })
        else {
            commands
                .entity(session)
                .insert(FailedConversationReadRequested);
            continue;
        };
        commands
            .entity(session)
            .insert(ConversationLoadPending)
            .remove::<FailedConversationReadRequested>();
        let mut operation = commands.spawn((
            ChildOf(session),
            read,
            ConversationLoadTask {
                revision: created_at.0,
                task,
            },
        ));
        operation.insert(FailedConversationRead);
        let operation = operation.id();
        commands
            .entity(session)
            .insert(EntityTarget::<HistoryRead>::new(operation));
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
    native: Option<Res<NativeReaders>>,
    files: Query<&ConversationFile>,
    versions: HistoryVersions,
    workers: Res<BlockingRuntime>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (session, _sid, kind, provider_sid, created_at, requested, cursor) in &sessions {
        let Some(source) = ConversationSource::new(kind, provider_sid, created_at.0) else {
            commands
                .entity(session)
                .remove::<ConversationRefreshRequested>();
            continue;
        };
        let revision = requested.0;
        let cursor = cursor.map(|cursor| cursor.0);
        let native = native
            .as_ref()
            .map(|native| (**native).clone())
            .unwrap_or_default();
        let hint = files.get(session).ok().map(|file| file.0.clone());
        let known = versions.file(session);
        let paths = paths.clone();
        let Some(read) = versions.capture(session, false) else {
            continue;
        };
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let Some(task) = workers
            .spawn_long(vmux_ecs::wake::Wake::optional_waker(wake), move || {
                source.read(&paths, cursor, native, hint, known)
            })
        else {
            continue;
        };
        commands
            .entity(session)
            .insert(ConversationLoadPending)
            .remove::<ConversationRefreshRequested>();
        let operation = commands.spawn((
            ChildOf(session),
            read,
            ConversationLoadTask { revision, task },
        ));
        let operation = operation.id();
        commands
            .entity(session)
            .insert(EntityTarget::<HistoryRead>::new(operation));
    }
}

fn finish_conversations(
    mut tasks: Query<(Entity, &mut ConversationLoadTask)>,
    mut received: MessageWriter<HistoryReceived>,
    mut commands: Commands,
) {
    for (operation, mut task) in &mut tasks {
        let Some(result) = task.task.poll() else {
            continue;
        };
        let result = result.unwrap_or_else(|_| Err("provider history worker panicked".into()));
        let mut read = match result {
            Ok(read) => read,
            Err(error) => {
                let mut read = conversation::ReadResult::empty(None);
                read.history.error = Some(error);
                read
            }
        };
        read.history.operation = operation;
        read.history.updated_at = task.revision;
        received.write(read.history);
        commands
            .entity(operation)
            .remove::<ConversationLoadTask>()
            .insert(ConversationReadReady {
                revision: task.revision,
                path: read.path,
                watch_paths: read.watch_paths,
                cursor: read.cursor,
                complete: read.complete,
            });
    }
}

fn commit_conversations(
    mut completed: MessageReader<HistoryCommitted>,
    reads: Query<(
        &ChildOf,
        Option<&HistoryRead>,
        Option<&ConversationReadReady>,
        Has<FailedConversationRead>,
    )>,
    pending: Query<&EntityTarget<HistoryRead>>,
    versions: HistoryVersions,
    files: Query<&ConversationFile>,
    watched: Query<&ConversationWatchPaths>,
    mut commands: Commands,
) {
    let mut handled = HashSet::new();
    for completed in completed.read() {
        let operation = completed.operation;
        if !handled.insert(operation) {
            continue;
        }
        let Ok((parent, read, ready, failed)) = reads.get(operation) else {
            continue;
        };
        let session = parent.parent();
        if !pending
            .get(session)
            .is_ok_and(|pending| pending.entity() == operation)
        {
            commands.entity(operation).despawn();
            continue;
        }
        let source_matches = read.is_some_and(|read| versions.matches(session, read));
        commands
            .entity(session)
            .remove::<(ConversationLoadPending, EntityTarget<HistoryRead>)>();
        match &completed.result {
            Ok(true) if source_matches => {
                if let Some(ready) = ready {
                    if let Some(path) = &ready.path
                        && !files.get(session).is_ok_and(|file| &file.0 == path)
                    {
                        commands
                            .entity(session)
                            .insert(ConversationFile(path.clone()));
                    }
                    if !ready.watch_paths.is_empty()
                        && !watched
                            .get(session)
                            .is_ok_and(|paths| paths.0 == ready.watch_paths)
                    {
                        commands
                            .entity(session)
                            .insert(ConversationWatchPaths(ready.watch_paths.clone()));
                    }
                    commands.entity(session).insert((
                        ConversationCursor(ready.cursor),
                        ConversationRevision(ready.revision),
                    ));
                    if !ready.complete {
                        commands
                            .entity(session)
                            .insert(ConversationRefreshRequested(ready.revision));
                    }
                }
                commands.entity(session).remove::<TranscriptLoading>();
            }
            Ok(true) => {}
            Ok(false) if source_matches => {
                if !failed && versions.has_events(session) {
                    commands.entity(session).remove::<TranscriptLoading>();
                }
            }
            Ok(false) => {}
            Err(error) if source_matches => {
                bevy::log::warn!("failed to read provider conversation: {error}");
                commands.entity(session).remove::<TranscriptLoading>();
            }
            Err(_) => {}
        }
        commands.entity(operation).despawn();
    }
}

fn open(
    trigger: On<UiInput<ResumeSession>>,
    parents: Query<&ChildOf>,
    panes: Query<(), With<vmux_layout::pane::Pane>>,
    spaces: vmux_layout::space::SpaceHierarchy,
    windows: vmux_layout::window::WindowHierarchy,
    catalog: ProviderEntries,
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
    mut imports: MessageWriter<ImportRequested>,
    mut opener: ResumeOpener,
) {
    let request = &trigger.event().payload;
    if !request.kind.starts_with("acp:") {
        return;
    }
    if tombstones.contains(&request.kind, &request.sid) {
        return;
    }
    let Ok(parent) = parents.get(trigger.event().webview) else {
        return;
    };
    let stack = parent.parent();
    let destination = ResumeDestination {
        origin: trigger.event().webview,
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
    let family = AgentFamily::of(request.kind.trim_start_matches("acp:"));
    if let Some((_, id, _, _, _, _)) =
        existing
            .iter()
            .find(|(_, _, agent, resume, kind, provider_sid)| {
                kind.is_some_and(|kind| kind.0 == request.kind)
                    && provider_sid.is_some_and(|sid| sid.0 == request.sid)
                    || AgentFamily::of(&agent.0) == family && resume.0 == request.sid
            })
    {
        opener.open(destination, Route::Session(id.clone()).url());
        return;
    }
    let Some(source) = catalog.get(&request.kind, &request.sid) else {
        return;
    };
    imports.write(ImportRequested {
        entry: source.entity,
        agent: agents.resolve(&source.agent.0),
        destination,
    });
}

fn opening(
    event: On<Opening>,
    imports: Query<(&SessionId, &ResumeDestination, &vmux_ecs::OperationState), With<Import>>,
    mut opener: ResumeOpener,
) {
    let Ok((id, destination, state)) = imports.get(event.entity) else {
        return;
    };
    if matches!(
        state,
        vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
    ) {
        opener.open(*destination, Route::Session(id.clone()).url());
    }
}

fn imported(
    event: On<Imported>,
    imports: Query<
        (
            &SessionId,
            &ResumeDestination,
            Option<&Opened>,
            &vmux_ecs::OperationState,
        ),
        With<Import>,
    >,
    sessions: Query<&ProviderSessionId, With<Session>>,
    mut opener: ResumeOpener,
    mut commands: Commands,
) {
    let Ok((id, destination, opened, state)) = imports.get(event.operation) else {
        return;
    };
    if *state != vmux_ecs::OperationState::Succeeded {
        return;
    }
    let Ok(sid) = sessions.get(event.entity) else {
        return;
    };
    commands
        .entity(event.entity)
        .insert(AcpSessionId(sid.0.clone()));
    if opened.is_none() {
        opener.open(*destination, Route::Session(id.clone()).url());
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
    use bevy_ecs::system::RunSystemOnce;
    use std::io::Write;

    struct ImportFixture {
        app: App,
        entry: Entity,
        created: Arc<Mutex<Vec<(Entity, usize)>>>,
    }

    impl ImportFixture {
        fn new(cwd: PathBuf) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_session::DomainPlugin, ImportPlugin));
            let created = Arc::new(Mutex::new(Vec::new()));
            let observed = created.clone();
            app.add_observer(move |event: On<Created>, world: &World| {
                observed.lock().unwrap().push((
                    event.entity,
                    world.resource::<Messages<PageOpenRequest>>().len(),
                ));
            });
            let source = app.world_mut().spawn(SourceKind::Codex.bundle()).id();
            let operation = app
                .world_mut()
                .spawn((
                    CatalogRead {
                        generation: 1,
                        wire_id: 0,
                    },
                    ChildOf(source),
                    vmux_ecs::Operation::bundle("provider catalog")
                        .unwrap()
                        .running(),
                ))
                .id();
            app.world_mut().entity_mut(source).insert((
                CatalogGeneration(1),
                EntityTarget::<CatalogRead>::new(operation),
            ));
            app.world_mut().write_message(CatalogReceived {
                operation,
                supported: true,
                result: Ok(vec![ProviderSession {
                    kind: "acp:codex".into(),
                    sid: "provider-session".into(),
                    agent: "codex".into(),
                    name: "Provider task".into(),
                    cwd: cwd.to_string_lossy().into_owned(),
                    created_at: 100,
                    updated_at: 200,
                    conversation_title: "Provider task".into(),
                    ..Default::default()
                }]),
            });
            app.update();
            let entry = app
                .world_mut()
                .query_filtered::<Entity, With<vmux_session::provider::Entry>>()
                .single(app.world())
                .unwrap();
            Self {
                app,
                entry,
                created,
            }
        }

        fn origin(&mut self) -> Entity {
            let stack = self.app.world_mut().spawn_empty().id();
            self.app.world_mut().spawn(ChildOf(stack)).id()
        }

        fn open(&mut self, webview: Entity, open: Option<OpenTarget>) {
            self.app.world_mut().trigger(UiInput {
                webview,
                payload: ResumeSession {
                    kind: "acp:codex".into(),
                    sid: "provider-session".into(),
                    cwd: String::new(),
                    open,
                },
            });
        }

        fn pages(&mut self) -> Vec<PageOpenRequest> {
            self.app
                .world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .collect()
        }
    }

    struct CatalogFixture {
        app: App,
    }

    impl CatalogFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<ArchiveRequest>()
                .init_resource::<ProviderSessionTombstones>()
                .add_plugins(Plugin);
            Self { app }
        }

        fn receive(&mut self, kind: SourceKind, sessions: Vec<ProviderSession>) {
            let source = self.app.world_mut().spawn(kind.bundle()).id();
            let operation = self
                .app
                .world_mut()
                .spawn((
                    CatalogRead {
                        generation: 1,
                        wire_id: 0,
                    },
                    CatalogReadStarted(Instant::now()),
                    vmux_ecs::Operation::bundle("provider catalog")
                        .unwrap()
                        .running(),
                    ChildOf(source),
                ))
                .id();
            self.app.world_mut().entity_mut(source).insert((
                CatalogGeneration(1),
                EntityTarget::<CatalogRead>::new(operation),
            ));
            self.app.world_mut().write_message(CatalogReceived {
                operation,
                supported: true,
                result: Ok(sessions),
            });
        }
    }

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
        let first = ConversationSource::new(
            &ProviderKind("acp:claude".into()),
            &ProviderSessionId("thread".into()),
            0,
        )
        .unwrap()
        .read(&paths, None, NativeReaders::default(), None, None)
        .unwrap();
        assert_eq!(
            first.messages(),
            [vmux_api::session::Message::user("first")]
        );
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"next\"}]}}\n").unwrap();
        let next = ConversationSource::new(
            &ProviderKind("acp:claude".into()),
            &ProviderSessionId("thread".into()),
            0,
        )
        .unwrap()
        .read(
            &paths,
            Some(first.cursor),
            NativeReaders::default(),
            first.path,
            first.history.file,
        )
        .unwrap();
        assert_eq!(
            next.messages(),
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
                None,
            )
            .unwrap();
        assert_eq!(
            read.messages(),
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
                .add_plugins((vmux_session::history::Plugin, HistoryReadPlugin));
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
                app.world_mut()
                    .write_message(vmux_session::OperationReceived(
                        vmux_api::session::ConversationEvent {
                            event_id: vmux_api::session::EventId::new(format!("cached-{sequence}")),
                            session_id: SessionId::new("canonical"),
                            actor_id: vmux_api::session::MemberId::local(&SessionId::new(
                                "canonical",
                            )),
                            client_op_id: None,
                            server_seq: sequence as u64,
                            created_at_ms: 1,
                            reply_to: None,
                            message,
                        },
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

        fn finish(&mut self) -> Vec<vmux_api::session::Message> {
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
            let session = self.session;
            self.app
                .world_mut()
                .run_system_once(move |transcripts: Transcripts| transcripts.get(session).messages)
                .unwrap()
        }

        fn native(&mut self, message: vmux_api::session::Message) -> Entity {
            let session = self.session;
            let read = self
                .app
                .world_mut()
                .run_system_once(move |versions: HistoryVersions| versions.capture(session, false))
                .unwrap()
                .unwrap();
            let operation = self
                .app
                .world_mut()
                .spawn((
                    ChildOf(session),
                    read,
                    ConversationLoadTask {
                        revision: 1,
                        task: vmux_ecs::BlockingPool::new("test-catalog", 1, 1)
                            .spawn(None, move || {
                                let mut history = HistoryReceived::messages(Vec::new(), false);
                                history.entries.push((Some("stable".into()), message));
                                Ok(conversation::ReadResult {
                                    path: None,
                                    watch_paths: Vec::new(),
                                    history,
                                    cursor: 1,
                                    complete: true,
                                })
                            })
                            .unwrap(),
                    },
                ))
                .id();
            self.app.world_mut().entity_mut(session).insert((
                ConversationLoadPending,
                EntityTarget::<HistoryRead>::new(operation),
            ));
            self.finish();
            let kind = self
                .app
                .world()
                .get::<ProviderKind>(session)
                .unwrap()
                .clone();
            let provider = self
                .app
                .world()
                .get::<ProviderSessionId>(session)
                .unwrap()
                .clone();
            self.app
                .world_mut()
                .query::<(&ChildOf, &Name, &ProviderKind, &ProviderSessionId)>()
                .iter(self.app.world())
                .find(|(_, name, source, id)| {
                    name.as_str() == "stable" && **source == kind && **id == provider
                })
                .unwrap()
                .0
                .parent()
        }
    }

    #[test]
    fn native_history_updates_keep_the_record_identity_and_ignore_unchanged_entries() {
        let mut fixture = FailedReadFixture::new();
        fixture.finish();
        let entity = fixture.native(vmux_api::session::Message::user("first"));
        let sequence = fixture
            .app
            .world()
            .get::<vmux_session::EventIdentity>(entity)
            .unwrap()
            .sequence;
        assert_eq!(
            fixture.native(vmux_api::session::Message::user("updated")),
            entity
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<vmux_session::EventIdentity>(entity)
                .unwrap()
                .sequence,
            sequence
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<vmux_session::MessageContent>(entity)
                .unwrap()
                .0,
            vmux_api::session::Message::user("updated")
        );
        let changed = fixture
            .app
            .world()
            .entity(entity)
            .get_ref::<vmux_session::MessageContent>()
            .unwrap()
            .last_changed();
        assert_eq!(
            fixture.native(vmux_api::session::Message::user("updated")),
            entity
        );
        assert_eq!(
            fixture
                .app
                .world()
                .entity(entity)
                .get_ref::<vmux_session::MessageContent>()
                .unwrap()
                .last_changed(),
            changed
        );
    }

    #[test]
    fn failed_codex_resume_reads_the_original_history_without_replacing_its_identity() {
        let mut fixture = FailedReadFixture::new();
        assert_eq!(
            fixture.finish(),
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
        assert_eq!(
            fixture.finish(),
            [
                cached,
                vmux_api::session::Message::user("new message after cached history")
            ]
        );
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
    fn changing_provider_identity_discards_the_previous_read_and_file_cursor() {
        let mut fixture = FailedReadFixture::new();
        fixture.app.world_mut().entity_mut(fixture.session).insert((
            ProviderSessionId("replacement".into()),
            ConversationCursor(999),
            ConversationFile(fixture._directory.path().join("old.jsonl")),
        ));
        fixture.app.update();
        assert!(fixture.finish().is_empty());
        assert!(
            fixture
                .app
                .world()
                .get::<ConversationFile>(fixture.session)
                .is_none()
        );
        assert_ne!(
            fixture
                .app
                .world()
                .get::<ConversationCursor>(fixture.session)
                .map(|cursor| cursor.0),
            Some(999)
        );
    }

    #[test]
    fn history_that_arrives_after_local_input_is_not_retried_as_a_new_snapshot() {
        let mut fixture = FailedReadFixture::new();
        fixture.app.world_mut().spawn((
            vmux_session::ConversationEvent,
            vmux_session::EventIdentity {
                event_id: vmux_api::session::EventId::new("local-event"),
                actor_id: vmux_api::session::MemberId::local(&SessionId::new("canonical")),
                client_op_id: None,
                sequence: 1,
                reply_to: None,
            },
            vmux_session::MessageContent(vmux_api::session::Message::user("local")),
            CreatedAt(1),
            ChildOf(fixture.session),
        ));
        assert_eq!(
            fixture.finish(),
            [vmux_api::session::Message::user("local")]
        );
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<FailedConversationReadRequested>(fixture.session)
                .is_none()
        );
        assert!(
            fixture
                .app
                .world_mut()
                .query::<&ConversationLoadTask>()
                .iter(fixture.app.world())
                .next()
                .is_none()
        );
    }

    #[test]
    fn configured_catalog_failure_blocks_fallback_until_the_source_is_removed() {
        let mut settings = AppSettings::default();
        settings.agent.acp = vec![vmux_setting::AcpAgentConfig {
            id: "codex".into(),
            name: "Codex".into(),
            command: "npx".into(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            version: None,
        }];
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CatalogPlugin))
            .insert_resource(settings);
        app.world_mut().spawn((SessionManagerView, PageReady));
        app.update();
        let source = app
            .world_mut()
            .query_filtered::<Entity, With<CatalogSource>>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            app.world().get::<CatalogCapability>(source),
            Some(&CatalogCapability::Unprobed)
        );
        let fallback = app
            .world_mut()
            .run_system_once(|entries: ProviderEntries| {
                entries.allows_fallback(&AgentFamily::of("codex"))
            })
            .unwrap();
        assert!(!fallback);
        app.world_mut().spawn(ServiceConnected);
        app.update();
        assert_eq!(
            app.world().get::<CatalogCapability>(source),
            Some(&CatalogCapability::Failed)
        );
        assert!(
            app.world()
                .get::<vmux_ecs::OperationError>(source)
                .is_some()
        );
        let fallback = app
            .world_mut()
            .run_system_once(|entries: ProviderEntries| {
                entries.allows_fallback(&AgentFamily::of("codex"))
            })
            .unwrap();
        assert!(!fallback);
        assert!(
            app.world_mut()
                .query::<&CatalogRead>()
                .iter(app.world())
                .next()
                .is_none()
        );
        app.world_mut()
            .resource_mut::<AppSettings>()
            .agent
            .acp
            .clear();
        app.update();
        assert!(app.world().get_entity(source).is_err());
        let fallback = app
            .world_mut()
            .run_system_once(|entries: ProviderEntries| {
                entries.allows_fallback(&AgentFamily::of("codex"))
            })
            .unwrap();
        assert!(fallback);
    }

    #[test]
    fn catalog_results_preserve_mismatched_requests_and_materialize_source_entries() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CatalogPlugin));
        app.world_mut().spawn((SessionManagerView, PageReady));
        app.world_mut().spawn(ServiceConnected);
        let source = app
            .world_mut()
            .spawn((
                CatalogSource,
                CatalogOrigin::Acp,
                CatalogCapability::Pending,
                CatalogGeneration(1),
                AgentId("codex".into()),
                Name::new("Codex"),
            ))
            .id();
        let operation = app
            .world_mut()
            .spawn((
                AcpCatalogRead,
                CatalogRead {
                    generation: 1,
                    wire_id: 7,
                },
                vmux_ecs::Operation::bundle("provider catalog")
                    .unwrap()
                    .running(),
                ChildOf(source),
            ))
            .id();
        app.world_mut()
            .entity_mut(source)
            .insert(EntityTarget::<CatalogRead>::new(operation));
        app.world_mut()
            .entity_mut(operation)
            .insert(CatalogStarted(Instant::now()));
        let mut controls = app.world_mut().query::<&mut CatalogControl>();
        let mut control = controls.single_mut(app.world_mut()).unwrap();
        control.active = true;
        control.generation = 1;
        app.world_mut().write_message(AcpCatalogReceived {
            request_id: 7,
            agent_id: "other".into(),
            agent_name: "Other".into(),
            supported: true,
            sessions: Vec::new(),
            error: None,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<EntityTarget<CatalogRead>>(source)
                .unwrap()
                .entity(),
            operation
        );
        assert_eq!(
            app.world().get::<vmux_ecs::OperationState>(operation),
            Some(&vmux_ecs::OperationState::Running)
        );
        app.world_mut().write_message(AcpCatalogReceived {
            request_id: 7,
            agent_id: "codex".into(),
            agent_name: "Codex".into(),
            supported: true,
            sessions: vec![AcpProviderSession {
                id: "native-id".into(),
                title: "Canonical catalog".into(),
                cwd: "/work".into(),
                updated_at: "2026-11-13T08:00:00Z".into(),
            }],
            error: None,
        });
        app.update();
        let name = app
            .world_mut()
            .run_system_once(|entries: vmux_session::provider::Entries| {
                entries
                    .get("acp:codex", "native-id")
                    .map(|entry| entry.name.as_str().to_string())
            })
            .unwrap();
        assert_eq!(name.as_deref(), Some("Canonical catalog"));
        assert_eq!(
            app.world().get::<CatalogCapability>(source),
            Some(&CatalogCapability::Supported)
        );
        assert_eq!(
            app.world().get::<vmux_ecs::OperationState>(operation),
            Some(&vmux_ecs::OperationState::Succeeded)
        );
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
    fn native_scan_intents_are_canceled_when_their_runtime_is_absent() {
        let mut fixture = CatalogFixture::new();
        let source = fixture
            .app
            .world_mut()
            .spawn((SourceKind::Codex.bundle(), ScanRequested))
            .id();
        fixture
            .app
            .world_mut()
            .entity_mut(source)
            .insert(CatalogCapability::Pending);
        fixture.app.update();
        assert!(fixture.app.world().get::<ScanRequested>(source).is_none());
        assert_eq!(
            fixture.app.world().get::<CatalogCapability>(source),
            Some(&CatalogCapability::Unprobed)
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&ScanTask>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn provider_resume_opens_the_pending_session_before_materialization() {
        let cwd = tempfile::tempdir().unwrap();
        let mut fixture = ImportFixture::new(cwd.path().to_path_buf());
        let origin = fixture.origin();
        let stack = fixture.app.world().get::<ChildOf>(origin).unwrap().parent();
        fixture.open(origin, None);
        fixture.open(origin, None);
        fixture.app.update();
        let created = fixture.created.lock().unwrap().clone();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].1, 1);
        let session = created[0].0;
        assert_eq!(
            fixture.app.world().get::<AcpSessionId>(session),
            Some(&AcpSessionId("provider-session".into()))
        );
        let id = fixture
            .app
            .world()
            .get::<SessionId>(session)
            .unwrap()
            .clone();
        let pages = fixture.pages();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].url, Route::Session(id).url());
        assert!(matches!(pages[0].target, PageOpenTarget::Stack(entity) if entity == stack));
    }

    #[test]
    fn provider_resume_defers_new_stack_navigation_until_materialization() {
        let cwd = tempfile::tempdir().unwrap();
        let mut fixture = ImportFixture::new(cwd.path().to_path_buf());
        let origin = fixture.origin();
        fixture.open(origin, Some(OpenTarget::InNewStack));
        fixture.app.update();
        let created = fixture.created.lock().unwrap().clone();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].1, 0);
        let id = fixture
            .app
            .world()
            .get::<SessionId>(created[0].0)
            .unwrap()
            .clone();
        let pages = fixture.pages();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].url, Route::Session(id).url());
        assert!(matches!(pages[0].target, PageOpenTarget::NewStack));
    }

    #[test]
    fn repeated_import_uses_one_session_and_the_latest_destination() {
        let cwd = tempfile::tempdir().unwrap();
        let mut fixture = ImportFixture::new(cwd.path().to_path_buf());
        let first = fixture.origin();
        let latest = fixture.origin();
        let stack = fixture.app.world().get::<ChildOf>(latest).unwrap().parent();
        fixture.open(first, Some(OpenTarget::InNewStack));
        fixture.open(latest, Some(OpenTarget::InPlace));
        fixture.app.update();
        let created = fixture.created.lock().unwrap().clone();
        assert_eq!(created.len(), 1);
        let id = fixture
            .app
            .world()
            .get::<SessionId>(created[0].0)
            .unwrap()
            .clone();
        let pages = fixture.pages();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].url, Route::Session(id).url());
        assert!(matches!(pages[0].target, PageOpenTarget::Stack(entity) if entity == stack));
    }

    #[test]
    fn a_removed_source_cannot_materialize_a_queued_import() {
        let cwd = tempfile::tempdir().unwrap();
        let mut fixture = ImportFixture::new(cwd.path().to_path_buf());
        let source = fixture
            .app
            .world()
            .get::<ChildOf>(fixture.entry)
            .unwrap()
            .parent();
        fixture
            .app
            .add_observer(move |_: On<Opening>, mut commands: Commands| {
                commands.entity(source).despawn();
            });
        let origin = fixture.origin();
        fixture.open(origin, None);
        fixture.app.update();
        assert!(fixture.created.lock().unwrap().is_empty());
        assert!(fixture.app.world().get_entity(source).is_err());
    }

    #[test]
    fn provider_association_survives_a_changed_acp_resume_identity() {
        let mut fixture = CatalogFixture::new();
        fixture.receive(
            SourceKind::Codex,
            vec![ProviderSession {
                kind: "acp:codex".into(),
                sid: "source-session".into(),
                agent: "codex".into(),
                name: "Review pull request".into(),
                cwd: "/work/dashboard".into(),
                created_at: 100,
                ..Default::default()
            }],
        );
        let app = &mut fixture.app;
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
        let mut fixture = ImportFixture::new(cwd.path().to_path_buf());
        let create = CreateRequest::new(
            "Existing",
            "",
            cwd.path().to_path_buf(),
            Some(AgentId("codex".into())),
        );
        let id = create.id().clone();
        fixture.app.world_mut().write_message(create);
        fixture.app.update();
        let session = fixture.created.lock().unwrap()[0].0;
        fixture.app.world_mut().entity_mut(session).insert((
            AcpSessionId("replacement-resume-session".into()),
            ProviderKind("acp:codex".into()),
            ProviderSessionId("provider-session".into()),
        ));
        fixture.created.lock().unwrap().clear();
        let origin = fixture.origin();
        fixture.open(origin, None);
        fixture.app.update();
        assert!(fixture.created.lock().unwrap().is_empty());
        let pages = fixture.pages();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].url, Route::Session(id).url());
    }

    #[test]
    fn provider_resume_rejects_a_dismissed_session() {
        let cwd = tempfile::tempdir().unwrap();
        let mut fixture = ImportFixture::new(cwd.path().to_path_buf());
        fixture.app.world_mut().spawn((
            vmux_session::ProviderSessionTombstone,
            ProviderKind("acp:codex".into()),
            ProviderSessionId("provider-session".into()),
        ));
        fixture.app.update();
        let origin = fixture.origin();
        fixture.open(origin, None);
        fixture.app.update();
        assert!(fixture.created.lock().unwrap().is_empty());
        assert!(fixture.pages().is_empty());
    }

    #[test]
    fn provider_resume_rejects_a_missing_working_directory() {
        let mut fixture = ImportFixture::new(PathBuf::from(
            "/vmux/provider/session/path/that/does/not/exist",
        ));
        let origin = fixture.origin();
        fixture.open(origin, None);
        fixture.app.update();
        assert!(fixture.created.lock().unwrap().is_empty());
        assert!(fixture.pages().is_empty());
    }

    #[test]
    fn newer_provider_activity_requests_an_imported_conversation_refresh() {
        let mut fixture = CatalogFixture::new();
        fixture.receive(
            SourceKind::Codex,
            vec![ProviderSession {
                kind: "acp:codex".into(),
                sid: "provider-session".into(),
                created_at: 100,
                updated_at: 300,
                ..Default::default()
            }],
        );
        let app = &mut fixture.app;
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
        let mut fixture = CatalogFixture::new();
        fixture.receive(
            SourceKind::Codex,
            vec![
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
            ],
        );
        let app = &mut fixture.app;
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
