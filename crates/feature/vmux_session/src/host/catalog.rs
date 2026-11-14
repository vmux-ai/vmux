use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::page::{Activity, HostsPage, PageReady};
use vmux_ecs::{
    EntityTarget, KeyboardOwner, PageMetadata, PageOpenRequest, PageOpenSet, PageOpenTarget,
    PageTransition,
};
use vmux_ecs::{UiState, UiStatePlugin, UiStateWrite};
use vmux_git::RepoInfoCache;
use vmux_session::{
    AgentId, ArchiveRequest, CatalogSnapshot, CleanupRequest, CleanupTarget, CreateRequest,
    Created, DescriptionUpdateRequest, ProviderArchiveRequest, ProviderKind, ProviderSessionId,
    ProviderSessionKey, ProviderSessionRestoreRequest, ProviderStageChangeRequest, RenameRequest,
    Route, Session, SessionId, SessionSummaryProjection, StageChangeRequest, StageId,
};

#[cfg(test)]
use crate::SessionSummary;

use super::session::SessionView;
use crate::event::{
    SessionDirectorySelection, SessionRepositories, SessionRepository, SessionsArchive,
    SessionsChooseDirectory, SessionsCleanup, SessionsCreate, SessionsDescriptionUpdate,
    SessionsOpen, SessionsRename, SessionsRestoreDeleted, SessionsStageChange,
};
use crate::state::{
    ChatUiState, Navigation, NavigationProject, NavigationSession, SessionCatalogUiState,
    SessionCatalogUiStatePatch, SessionDetailUiState, SessionScopeUiState,
};

pub(super) struct CatalogPlugin;

impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingOpens>()
            .init_resource::<DirectoryRevision>()
            .init_resource::<CatalogSnapshot>()
            .init_resource::<SessionRepositories>()
            .add_plugins(UiStatePlugin::<SessionCatalogUiState>::default())
            .add_plugins(UiStatePlugin::<SessionDetailUiState>::default())
            .add_plugins(UiStatePlugin::<SessionScopeUiState>::default())
            .add_plugins(UiEventPlugin::<(
                SessionsCreate,
                SessionsOpen,
                SessionsChooseDirectory,
                SessionsRename,
                SessionsDescriptionUpdate,
                SessionsStageChange,
                SessionsArchive,
                SessionsCleanup,
                SessionsRestoreDeleted,
            )>::default())
            .add_observer(create)
            .add_observer(open)
            .add_observer(address_request)
            .add_observer(choose_directory)
            .add_observer(open_created)
            .add_observer(rename)
            .add_observer(update_description)
            .add_observer(change_stage)
            .add_observer(archive)
            .add_observer(cleanup)
            .add_observer(restore_deleted)
            .add_observer(scope_projection)
            .add_observer(ready)
            .add_observer(unpublish)
            .add_systems(
                Update,
                (sync_manager_views, cleanup_inactive_views)
                    .chain()
                    .after(PageOpenSet::HandleKnownPages),
            )
            .add_systems(Update, finish_directory_pickers)
            .add_systems(
                PostUpdate,
                (
                    sync_repositories.run_if(Activity::<SessionCatalogView>::active),
                    publish,
                    publish_scope,
                    publish_detail,
                )
                    .chain(),
            );
    }
}

#[derive(Component)]
#[require(SessionCatalogView, SessionCatalogUiStateComponent)]
pub struct SessionManagerView;

fn unpublish(trigger: On<Remove, SessionManagerView>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .try_remove::<SessionCatalogUiStateComponent>();
}

#[derive(Component, Clone)]
struct SessionManagerFallback {
    session: Entity,
    work_dir: Option<String>,
}

impl SessionManagerFallback {
    fn capture(
        target: &EntityTarget<Session>,
        work_dir: Option<&vmux_command::CommandBarWorkDirectory>,
    ) -> Self {
        Self {
            session: target.entity(),
            work_dir: work_dir.map(|work_dir| work_dir.0.clone()),
        }
    }
}

type SessionCatalogUiStateComponent = UiState<SessionCatalogUiState>;
type SessionDetailUiStateComponent = UiState<SessionDetailUiState>;
type SessionScopeUiStateComponent = UiState<SessionScopeUiState>;

#[derive(Component, Default)]
pub struct SessionCatalogView;

fn open(
    trigger: On<UiInput<SessionsOpen>>,
    views: Query<&ChildOf, With<SessionView>>,
    entities: CatalogEntities,
    mut opens: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(parent) = views.get(webview) else {
        return;
    };
    let Some(target) = entities.get(&trigger.event().payload.id) else {
        return;
    };
    match target {
        CatalogTarget::Session { id, .. } => {
            opens.write(PageOpenRequest {
                target: PageOpenTarget::Stack(parent.parent()),
                url: Route::Session(id.clone()).url(),
                request_id: None,
            });
        }
        CatalogTarget::Provider(source) => {
            commands.trigger(UiInput {
                webview,
                payload: crate::event::ResumeSession {
                    kind: source.kind.0.clone(),
                    sid: source.id.0.clone(),
                    cwd: source.cwd.0.to_string_lossy().into_owned(),
                    open: Some(vmux_api::open_target::OpenTarget::InPlace),
                },
            });
        }
    }
}

fn address_request(
    event: On<vmux_ecs::page::AddressIntentRequest>,
    views: Query<(), With<SessionView>>,
    mut commands: Commands,
) {
    if !views.contains(event.entity) {
        return;
    }
    if let vmux_api::address::AddressIntent::Session { id } = &event.intent {
        commands.trigger(UiInput {
            webview: event.entity,
            payload: SessionsOpen { id: id.clone() },
        });
    }
}

#[derive(Component, Default)]
#[require(SessionCatalogView, SessionDetailUiStateComponent)]
struct SessionDetailView;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[require(SessionCatalogView, SessionScopeUiStateComponent)]
struct SessionScopeView(PathBuf);

#[derive(bevy::ecs::system::SystemParam)]
struct ManagerLocations<'w, 's> {
    parents: Query<'w, 's, &'static ChildOf>,
    targets: Query<'w, 's, &'static EntityTarget<Session>>,
    work_dirs: Query<'w, 's, &'static vmux_command::CommandBarWorkDirectory>,
    fallbacks: Query<'w, 's, &'static SessionManagerFallback>,
    sessions: Query<'w, 's, &'static SessionId, With<Session>>,
    page_containers: Query<'w, 's, &'static PageMetadata, Without<HostsPage>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct ManagerChanges<'w, 's> {
    changed_views: Query<'w, 's, Entity, (With<HostsPage>, Changed<PageMetadata>)>,
    changed_containers: Query<'w, 's, Entity, (Without<HostsPage>, Changed<PageMetadata>)>,
    added_transitions: Query<'w, 's, Entity, Added<PageTransition>>,
    removed_transitions: RemovedComponents<'w, 's, PageTransition>,
}

fn sync_manager_views(
    views: Query<(Entity, &PageMetadata, Has<SessionManagerView>), With<HostsPage>>,
    mut changes: ManagerChanges,
    locations: ManagerLocations,
    children: Query<&Children>,
    transitions: Query<(), With<PageTransition>>,
    mut commands: Commands,
) {
    let mut dirty = HashSet::new();
    dirty.extend(changes.changed_views.iter());
    for container in &changes.changed_containers {
        if let Ok(children) = children.get(container) {
            dirty.extend(children.iter().filter(|child| views.contains(*child)));
        }
    }
    for transition in changes
        .added_transitions
        .iter()
        .chain(changes.removed_transitions.read())
    {
        let Ok(parent) = locations.parents.get(transition) else {
            continue;
        };
        let Ok(children) = children.get(parent.parent()) else {
            continue;
        };
        dirty.extend(children.iter().filter(|child| views.contains(*child)));
    }
    for entity in dirty {
        let Ok((entity, page, manager)) = views.get(entity) else {
            continue;
        };
        let stack = locations
            .parents
            .get(entity)
            .ok()
            .map(ChildOf::parent)
            .filter(|parent| locations.page_containers.contains(*parent));
        let route = stack
            .and_then(|stack| locations.page_containers.get(stack).ok())
            .and_then(|page| Route::parse(&page.url));
        let target_manager = matches!(route.as_ref(), Some(Route::Manager));
        let transitioning = stack
            .and_then(|stack| children.get(stack).ok())
            .is_some_and(|children| children.iter().any(|child| transitions.contains(child)));
        if transitioning && !target_manager {
            if manager {
                commands.entity(entity).remove::<(
                    SessionManagerView,
                    SessionCatalogView,
                    SessionCatalogUiStateComponent,
                )>();
            }
            continue;
        }
        let manager_route =
            target_manager && matches!(Route::parse(&page.url), Some(Route::Manager));
        if manager_route && let Some(stack) = stack {
            let captured = locations.targets.get(stack).ok().map(|target| {
                SessionManagerFallback::capture(target, locations.work_dirs.get(stack).ok())
            });
            let mut entity = commands.entity(stack);
            if locations.fallbacks.get(stack).is_err()
                && let Some(captured) = captured.clone()
            {
                entity.insert(captured);
            }
            entity
                .remove::<EntityTarget<Session>>()
                .remove::<vmux_command::CommandBarWorkDirectory>();
            if !transitioning && captured.is_none() {
                entity.remove::<SessionManagerFallback>();
            }
        } else if let (Some(stack), Some(Route::Session(id))) = (stack, route.as_ref()) {
            let mut restored = false;
            if locations.targets.get(stack).is_err()
                && let Ok(fallback) = locations.fallbacks.get(stack)
                && locations
                    .sessions
                    .get(fallback.session)
                    .is_ok_and(|candidate| candidate == id)
            {
                let mut entity = commands.entity(stack);
                entity.insert(EntityTarget::<Session>::new(fallback.session));
                match fallback.work_dir.clone() {
                    Some(work_dir) => {
                        entity.insert(vmux_command::CommandBarWorkDirectory(work_dir));
                    }
                    None => {
                        entity.remove::<vmux_command::CommandBarWorkDirectory>();
                    }
                }
                entity.remove::<SessionManagerFallback>();
                restored = true;
            }
            if !transitioning && !restored && locations.targets.get(stack).is_ok() {
                commands.entity(stack).remove::<SessionManagerFallback>();
            }
        }
        match (manager_route, manager) {
            (true, false) => {
                commands.entity(entity).try_insert(SessionManagerView);
            }
            (false, true) => {
                commands.entity(entity).remove::<SessionManagerView>();
            }
            _ => {}
        }
    }
}

#[derive(EntityEvent, Clone, Debug)]
pub struct ScopeProjectionRequest {
    #[event_target]
    pub webview: Entity,
    pub cwd: Option<PathBuf>,
}

#[derive(Resource, Default)]
struct PendingOpens(HashMap<SessionId, Entity>);

#[derive(Resource, Default)]
struct DirectoryRevision(u64);

impl DirectoryRevision {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }
}

#[derive(Component)]
struct PendingDirectoryPicker {
    webview: Entity,
    task: Task<Option<PathBuf>>,
}

struct DirectoryPicker;

impl DirectoryPicker {
    fn initial_directory(requested: &str, projects: Option<&Path>) -> PathBuf {
        let mut requested = PathBuf::from(requested);
        while !requested.as_os_str().is_empty() && !requested.is_dir() {
            requested.pop();
        }
        if requested.is_dir() {
            return requested;
        }
        if let Some(projects) = projects.filter(|path| path.is_dir()) {
            return projects.to_path_buf();
        }
        std::env::current_dir()
            .ok()
            .filter(|path| path.is_dir())
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .filter(|path| path.is_dir())
            })
            .unwrap_or_else(|| PathBuf::from("/"))
    }
}

struct SessionProject;

impl SessionProject {
    fn root(path: &Path, managed: bool) -> PathBuf {
        if managed {
            for ancestor in path.ancestors() {
                if ancestor
                    .file_name()
                    .is_some_and(|name| name == ".worktrees")
                {
                    return ancestor.parent().unwrap_or(path).to_path_buf();
                }
            }
        }
        path.to_path_buf()
    }

    fn fallback(path: &Path, managed: bool) -> String {
        if !managed {
            return String::new();
        }
        Self::root(path, managed)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn url(root: Option<&Path>, path: &Path) -> String {
        let Some(root) = root else {
            return String::new();
        };
        let Ok(relative) = path.strip_prefix(root) else {
            return String::new();
        };
        let Ok(mut url) = url::Url::parse("vmux://projects/") else {
            return String::new();
        };
        let Ok(mut segments) = url.path_segments_mut() else {
            return String::new();
        };
        segments.pop_if_empty();
        for component in relative.components() {
            let std::path::Component::Normal(component) = component else {
                return String::new();
            };
            let Some(component) = component.to_str() else {
                return String::new();
            };
            segments.push(component);
        }
        drop(segments);
        url.into()
    }
}

#[derive(SystemParam)]
struct CatalogViews<'w, 's> {
    managers: Query<'w, 's, Option<&'static PageMetadata>, With<SessionManagerView>>,
    chats: Query<'w, 's, &'static PageMetadata, With<SessionView>>,
    scopes: Query<'w, 's, &'static SessionScopeView>,
}

impl CatalogViews<'_, '_> {
    fn manager_contains(&self, entity: Entity) -> bool {
        self.managers.contains(entity)
    }

    fn can_mutate(&self, entity: Entity, session: Option<&SessionId>, cwd: &Path) -> bool {
        if self.manager_contains(entity) {
            return true;
        }
        if let Ok(page) = self.chats.get(entity) {
            return matches!(Route::parse(&page.url), Some(Route::Session(id)) if session == Some(&id));
        }
        self.scopes
            .get(entity)
            .is_ok_and(|scope| !cwd.as_os_str().is_empty() && cwd.starts_with(&scope.0))
    }

    fn agent(&self, entity: Entity) -> Option<AgentId> {
        self.managers
            .get(entity)
            .ok()
            .flatten()
            .and_then(|metadata| Route::requested_agent(&metadata.url))
    }
}

#[derive(SystemParam)]
struct CatalogEntities<'w, 's> {
    sessions: Query<
        'w,
        's,
        (
            &'static SessionId,
            &'static vmux_ecs::Cwd,
            Option<&'static ProviderKind>,
            Option<&'static ProviderSessionId>,
        ),
        (With<Session>, Without<crate::SessionDuplicate>),
    >,
    providers: crate::provider::Entries<'w, 's>,
}

enum CatalogTarget<'a> {
    Session {
        id: &'a SessionId,
        cwd: &'a vmux_ecs::Cwd,
        kind: Option<&'a ProviderKind>,
        provider_id: Option<&'a ProviderSessionId>,
    },
    Provider(crate::provider::Record<'a>),
}

impl CatalogTarget<'_> {
    fn session(&self) -> Option<&SessionId> {
        match self {
            Self::Session { id, .. } => Some(id),
            Self::Provider(_) => None,
        }
    }

    fn cwd(&self) -> &Path {
        match self {
            Self::Session { cwd, .. } => cwd.0.as_path(),
            Self::Provider(entry) => entry.cwd.0.as_path(),
        }
    }
}

impl CatalogEntities<'_, '_> {
    fn get(&self, id: &str) -> Option<CatalogTarget<'_>> {
        for (session, cwd, kind, provider_id) in &self.sessions {
            if session.0 == id {
                return Some(CatalogTarget::Session {
                    id: session,
                    cwd,
                    kind,
                    provider_id,
                });
            }
        }
        self.providers.from_catalog(id).map(CatalogTarget::Provider)
    }
}

fn create(
    trigger: On<UiInput<SessionsCreate>>,
    parents: Query<&ChildOf>,
    views: CatalogViews,
    mut pending: ResMut<PendingOpens>,
    mut requests: MessageWriter<CreateRequest>,
) {
    if !views.manager_contains(trigger.event().webview) {
        return;
    }
    let Ok(parent) = parents.get(trigger.event().webview) else {
        return;
    };
    let request = &trigger.event().payload;
    let Ok(cwd) = PathBuf::from(&request.cwd).canonicalize() else {
        return;
    };
    if !cwd.is_dir() {
        return;
    }
    let request = CreateRequest::new(
        request.name.clone(),
        request.description.clone(),
        cwd,
        views.agent(trigger.event().webview),
    );
    pending.0.insert(request.id().clone(), parent.parent());
    requests.write(request);
}

fn choose_directory(
    trigger: On<UiInput<SessionsChooseDirectory>>,
    views: CatalogViews,
    pending: Query<&PendingDirectoryPicker>,
    projects: vmux_ecs::profile::Projects,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if !views.manager_contains(webview) || pending.iter().any(|picker| picker.webview == webview) {
        return;
    }
    let project_root = projects.path().ok().map(Path::to_path_buf);
    let initial = DirectoryPicker::initial_directory(
        &trigger.event().payload.current_dir,
        project_root.as_deref(),
    );
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let task = IoTaskPool::get().spawn(async move {
        let selected = rfd::AsyncFileDialog::new()
            .set_title(vmux_ui::i18n::translate("sessions-directory-choose"))
            .set_directory(initial)
            .pick_folder()
            .await
            .map(|folder| folder.path().to_path_buf());
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        selected
    });
    commands.spawn(PendingDirectoryPicker { webview, task });
}

fn finish_directory_pickers(
    mut pending: Query<(Entity, &mut PendingDirectoryPicker)>,
    mut revision: ResMut<DirectoryRevision>,
    mut commands: Commands,
) {
    for (entity, mut picker) in &mut pending {
        let Some(selected) = future::block_on(future::poll_once(&mut picker.task)) else {
            continue;
        };
        if let Some(path) = selected.and_then(|path| path.canonicalize().ok())
            && path.is_dir()
        {
            commands.trigger(UiStateWrite::<ChatUiState>::from_event(
                picker.webview,
                &SessionDirectorySelection {
                    revision: revision.next(),
                    path: path.to_string_lossy().into_owned(),
                },
            ));
        }
        commands.entity(entity).despawn();
    }
}

fn open_created(
    trigger: On<Created>,
    mut pending: ResMut<PendingOpens>,
    mut opens: MessageWriter<PageOpenRequest>,
) {
    let event = trigger.event();
    let Some(stack) = pending.0.remove(&event.id) else {
        return;
    };
    opens.write(PageOpenRequest {
        target: PageOpenTarget::Stack(stack),
        url: Route::Session(event.id.clone()).url(),
        request_id: None,
    });
}

fn rename(
    trigger: On<UiInput<SessionsRename>>,
    views: CatalogViews,
    entities: CatalogEntities,
    mut requests: MessageWriter<RenameRequest>,
) {
    let request = &trigger.event().payload;
    let Some(target) = entities.get(&request.id) else {
        return;
    };
    let Some(id) = target.session() else {
        return;
    };
    if !views.can_mutate(trigger.event().webview, Some(id), target.cwd()) {
        return;
    }
    requests.write(RenameRequest {
        id: id.clone(),
        name: request.name.clone(),
    });
}

fn update_description(
    trigger: On<UiInput<SessionsDescriptionUpdate>>,
    views: CatalogViews,
    entities: CatalogEntities,
    mut requests: MessageWriter<DescriptionUpdateRequest>,
) {
    let request = &trigger.event().payload;
    let Some(target) = entities.get(&request.id) else {
        return;
    };
    let Some(id) = target.session() else {
        return;
    };
    if !views.can_mutate(trigger.event().webview, Some(id), target.cwd()) {
        return;
    }
    requests.write(DescriptionUpdateRequest {
        id: id.clone(),
        description: request.description.clone(),
    });
}

fn change_stage(
    trigger: On<UiInput<SessionsStageChange>>,
    views: CatalogViews,
    entities: CatalogEntities,
    mut requests: MessageWriter<StageChangeRequest>,
    mut provider_requests: MessageWriter<ProviderStageChangeRequest>,
) {
    let request = &trigger.event().payload;
    let Some(target) = entities.get(&request.id) else {
        return;
    };
    if !views.can_mutate(trigger.event().webview, target.session(), target.cwd()) {
        return;
    }
    match target {
        CatalogTarget::Provider(source) => {
            provider_requests.write(ProviderStageChangeRequest {
                kind: source.kind.clone(),
                id: source.id.clone(),
                stage: StageId(request.stage.clone()),
                source_updated_at: source.source_updated_at(),
            });
        }
        CatalogTarget::Session { id, .. } => {
            requests.write(StageChangeRequest {
                id: id.clone(),
                stage: StageId(request.stage.clone()),
            });
        }
    }
}

fn archive(
    trigger: On<UiInput<SessionsArchive>>,
    views: CatalogViews,
    entities: CatalogEntities,
    mut requests: MessageWriter<ArchiveRequest>,
    mut provider_requests: MessageWriter<ProviderArchiveRequest>,
) {
    let request = &trigger.event().payload;
    let source_updated_at = vmux_ecs::UnixMillis::now().0;
    for id in &request.ids {
        let Some(target) = entities.get(id) else {
            continue;
        };
        if !views.can_mutate(trigger.event().webview, target.session(), target.cwd()) {
            continue;
        }
        match target {
            CatalogTarget::Provider(source) => {
                provider_requests.write(ProviderArchiveRequest {
                    kind: source.kind.clone(),
                    id: source.id.clone(),
                    archived: request.archived,
                    source_updated_at,
                    native: source.archive_observed_at.is_some(),
                });
            }
            CatalogTarget::Session {
                id,
                kind,
                provider_id,
                ..
            } => {
                if let (Some(kind), Some(provider_id)) = (kind, provider_id) {
                    let native = entities
                        .providers
                        .get(&kind.0, &provider_id.0)
                        .is_some_and(|source| source.archive_observed_at.is_some());
                    provider_requests.write(ProviderArchiveRequest {
                        kind: kind.clone(),
                        id: provider_id.clone(),
                        archived: request.archived,
                        source_updated_at,
                        native,
                    });
                }
                requests.write(ArchiveRequest {
                    id: id.clone(),
                    archived_at: request.archived.then_some(source_updated_at),
                });
            }
        }
    }
}

fn cleanup(
    trigger: On<UiInput<SessionsCleanup>>,
    views: CatalogViews,
    entities: CatalogEntities,
    mut requests: MessageWriter<CleanupRequest>,
) {
    let mut seen = HashSet::new();
    let mut targets = Vec::new();
    for id in &trigger.event().payload.ids {
        if !seen.insert(id.as_str()) {
            continue;
        }
        let Some(target) = entities.get(id) else {
            continue;
        };
        if !views.can_mutate(trigger.event().webview, target.session(), target.cwd()) {
            continue;
        }
        match target {
            CatalogTarget::Session { id, .. } => targets.push(CleanupTarget::Session(id.clone())),
            CatalogTarget::Provider(source) => targets.push(CleanupTarget::Provider(
                ProviderSessionKey::new(source.kind.clone(), source.id.clone()),
            )),
        }
    }
    if !targets.is_empty() {
        requests.write(CleanupRequest { targets });
    }
}

fn restore_deleted(
    trigger: On<UiInput<SessionsRestoreDeleted>>,
    views: CatalogViews,
    mut requests: MessageWriter<ProviderSessionRestoreRequest>,
) {
    if !views.manager_contains(trigger.event().webview) {
        return;
    }
    let mut keys = trigger
        .event()
        .payload
        .sessions
        .iter()
        .filter(|session| !session.kind.is_empty() && !session.sid.is_empty())
        .map(|session| {
            ProviderSessionKey::new(
                ProviderKind(session.kind.clone()),
                ProviderSessionId(session.sid.clone()),
            )
        })
        .collect::<Vec<_>>();
    keys.sort_by(|left, right| (&left.kind.0, &left.id.0).cmp(&(&right.kind.0, &right.id.0)));
    keys.dedup();
    if !keys.is_empty() {
        requests.write(ProviderSessionRestoreRequest { keys });
    }
}

impl SessionScopeUiState {
    fn project(root: &Path, catalog: &CatalogSnapshot) -> Self {
        let sessions = catalog
            .sessions
            .iter()
            .filter(|session| Path::new(&session.cwd).starts_with(root))
            .cloned()
            .collect();
        Self {
            sessions,
            stages: catalog.stages.clone(),
        }
    }
}

impl SessionDetailUiState {
    fn project(
        page: &PageMetadata,
        catalog: &CatalogSnapshot,
        repositories: &SessionRepositories,
    ) -> Self {
        let id = match Route::parse(&page.url) {
            Some(Route::Session(id)) => id.0,
            Some(Route::Manager | Route::Provider { .. }) | None => return Self::default(),
        };
        let session = catalog
            .sessions
            .iter()
            .find(|session| session.id == id)
            .cloned();
        let repository = session
            .as_ref()
            .and_then(|session| repositories.get(&session.id));
        Self {
            session,
            repository,
            stages: catalog.stages.clone(),
            navigation: Navigation::project(catalog, repositories),
            pending_provider: None,
        }
    }
}

impl Navigation {
    fn project(catalog: &CatalogSnapshot, repositories: &SessionRepositories) -> Self {
        let mut project_urls = HashSet::new();
        let mut projects = Vec::new();
        for repository in &repositories.sessions {
            if repository.project.is_empty()
                || repository.project_url.is_empty()
                || !project_urls.insert(repository.project_url.as_str())
            {
                continue;
            }
            projects.push(NavigationProject {
                identity: repository.project_identity.clone(),
                name: repository.project.clone(),
                url: repository.project_url.clone(),
            });
        }
        projects.sort_by(|left, right| left.name.cmp(&right.name));

        let now = vmux_ecs::UnixMillis::now().0;
        let mut sessions = Vec::new();
        for session in &catalog.sessions {
            if session.archived_at > 0 {
                continue;
            }
            let presentation = SessionSummaryProjection::new(session, &catalog.stages, now);
            let project_url = repositories
                .get(&session.id)
                .map(|repository| repository.project_url)
                .unwrap_or_default();
            sessions.push(NavigationSession {
                id: session.id.clone(),
                title: presentation.title,
                project_url,
            });
        }
        Self { projects, sessions }
    }
}

fn publish(
    catalog: Res<CatalogSnapshot>,
    mut providers: crate::provider::Entries,
    repositories: Res<SessionRepositories>,
    targets: Query<
        Entity,
        (
            With<SessionCatalogUiStateComponent>,
            With<PageReady>,
            With<KeyboardOwner>,
        ),
    >,
    claimed: Query<
        (),
        (
            With<SessionCatalogUiStateComponent>,
            With<PageReady>,
            With<KeyboardOwner>,
            Or<(Added<SessionCatalogUiStateComponent>, Added<KeyboardOwner>)>,
        ),
    >,
    mut commands: Commands,
) {
    if !catalog.is_changed()
        && !providers.is_changed()
        && !repositories.is_changed()
        && claimed.is_empty()
    {
        return;
    }
    for target in &targets {
        commands.trigger(UiStateWrite::<SessionCatalogUiState>::from_event(
            target,
            &SessionCatalogUiStatePatch {
                catalog: Some(Box::new(catalog.clone())),
                repositories: Some(Box::new(repositories.clone())),
                loading: Some(providers.loading()),
            },
        ));
    }
}

fn publish_scope(
    catalog: Res<CatalogSnapshot>,
    targets: Query<(Entity, &SessionScopeView), (With<PageReady>, With<KeyboardOwner>)>,
    claimed: Query<
        (),
        (
            With<SessionScopeView>,
            With<PageReady>,
            Added<KeyboardOwner>,
        ),
    >,
    changed: Query<(), Changed<SessionScopeView>>,
    mut commands: Commands,
) {
    if !catalog.is_changed() && claimed.is_empty() && changed.is_empty() {
        return;
    }
    for (target, scope) in &targets {
        let state = SessionScopeUiState::project(&scope.0, &catalog);
        commands.trigger(UiStateWrite::<SessionScopeUiState>::from_event(
            target, &state,
        ));
    }
}

fn publish_detail(
    catalog: Res<CatalogSnapshot>,
    repositories: Res<SessionRepositories>,
    targets: Query<
        (Entity, &PageMetadata),
        (
            With<SessionDetailView>,
            With<PageReady>,
            With<KeyboardOwner>,
        ),
    >,
    claimed: Query<
        (),
        (
            With<SessionDetailView>,
            With<PageReady>,
            Added<KeyboardOwner>,
        ),
    >,
    changed: Query<(), (With<SessionDetailView>, Changed<PageMetadata>)>,
    mut commands: Commands,
) {
    if !catalog.is_changed()
        && !repositories.is_changed()
        && claimed.is_empty()
        && changed.is_empty()
    {
        return;
    }
    for (target, page) in &targets {
        let state = SessionDetailUiState::project(page, &catalog, &repositories);
        commands.trigger(UiInput {
            webview: target,
            payload: vmux_api::address::PageAddressPublish {
                address: state.address(page.url.clone()),
            },
        });
        commands.trigger(UiStateWrite::<SessionDetailUiState>::from_event(
            target, &state,
        ));
    }
}

fn sync_repositories(
    catalog: Res<CatalogSnapshot>,
    projects: vmux_ecs::profile::Projects,
    mut cache: Option<Single<&mut RepoInfoCache>>,
    active: Query<
        Entity,
        (
            With<KeyboardOwner>,
            Or<(With<SessionCatalogView>, With<SessionDetailView>)>,
        ),
    >,
    claimed: Query<
        (),
        (
            With<KeyboardOwner>,
            Or<(
                Added<SessionCatalogView>,
                Added<SessionDetailView>,
                Added<KeyboardOwner>,
            )>,
            Or<(With<SessionCatalogView>, With<SessionDetailView>)>,
        ),
    >,
    mut repositories: ResMut<SessionRepositories>,
) {
    if active.is_empty() {
        return;
    }
    let cache_changed = cache.as_ref().is_some_and(|cache| cache.is_changed());
    let claimed = !claimed.is_empty();
    if !catalog.is_changed() && !cache_changed && !claimed {
        return;
    }
    if let Some(cache) = cache.as_mut() {
        let cache = cache.bypass_change_detection();
        let mut paths = Vec::new();
        let mut unique = HashSet::new();
        for session in &catalog.sessions {
            if !session.cwd.is_empty() && unique.insert(&session.cwd) {
                paths.push(PathBuf::from(&session.cwd));
            }
        }
        for owner in &active {
            cache.observe_active_paths(owner, paths.iter().cloned());
        }
    }
    let mut sessions = Vec::with_capacity(catalog.sessions.len());
    let mut directories: HashMap<(&str, bool), SessionRepository> = HashMap::new();
    let projects_root = projects.path().ok();
    for session in &catalog.sessions {
        let key = (session.cwd.as_str(), session.managed);
        if let Some(repository) = directories.get(&key) {
            let mut repository = repository.clone();
            repository.id = session.id.clone();
            sessions.push(repository);
            continue;
        }
        let path = Path::new(&session.cwd);
        let info = cache
            .as_mut()
            .and_then(|cache| cache.bypass_change_detection().cached(path));
        let repository = match info {
            Some(info) => SessionRepository {
                project_identity: info.project_root().to_string_lossy().into_owned(),
                id: session.id.clone(),
                project: info.project_name(),
                project_url: SessionProject::url(projects_root, info.project_root()),
                branch: info.branch,
                url: vmux_git::GitUrl::from_path(&info.repo_root).unwrap_or_default(),
            },
            None => SessionRepository {
                project_identity: SessionProject::root(path, session.managed)
                    .to_string_lossy()
                    .into_owned(),
                id: session.id.clone(),
                project: SessionProject::fallback(path, session.managed),
                project_url: SessionProject::url(
                    projects_root,
                    &SessionProject::root(path, session.managed),
                ),
                branch: String::new(),
                url: (!session.cwd.is_empty())
                    .then(|| vmux_git::GitUrl::from_path(path))
                    .flatten()
                    .unwrap_or_default(),
            },
        };
        directories.insert(key, repository.clone());
        sessions.push(repository);
    }
    let next = SessionRepositories {
        ready: true,
        sessions,
    };
    if *repositories != next {
        *repositories = next;
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct CatalogContent<'w, 's> {
    catalog: Res<'w, CatalogSnapshot>,
    repositories: Res<'w, SessionRepositories>,
    providers: crate::provider::Entries<'w, 's>,
}

fn ready(
    trigger: On<UiInput<PageReady>>,
    content: CatalogContent,
    spares: Query<(), With<vmux_ecs::page::PrewarmedPage>>,
    views: CatalogViews,
    pages: Query<&PageMetadata>,
    scopes: Query<&SessionScopeView>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    if spares.contains(target) {
        return;
    }
    if views.manager_contains(target) {
        commands.entity(target).insert(SessionCatalogView);
        commands.trigger(UiStateWrite::<SessionCatalogUiState>::from_event(
            target,
            &SessionCatalogUiStatePatch {
                catalog: Some(Box::new(content.catalog.clone())),
                repositories: Some(Box::new(content.repositories.clone())),
                loading: Some(content.providers.loading()),
            },
        ));
    }
    if views.chats.contains(target)
        && let Ok(page) = pages.get(target)
    {
        commands.entity(target).insert(SessionDetailView);
        let state = SessionDetailUiState::project(page, &content.catalog, &content.repositories);
        commands.trigger(UiInput {
            webview: target,
            payload: vmux_api::address::PageAddressPublish {
                address: state.address(page.url.clone()),
            },
        });
        commands.trigger(UiStateWrite::<SessionDetailUiState>::from_event(
            target, &state,
        ));
    }
    if let Ok(scope) = scopes.get(target) {
        let state = SessionScopeUiState::project(&scope.0, &content.catalog);
        commands.trigger(UiStateWrite::<SessionScopeUiState>::from_event(
            target, &state,
        ));
    }
}

fn scope_projection(
    trigger: On<ScopeProjectionRequest>,
    owners: Query<(Has<SessionManagerView>, Has<SessionView>)>,
    catalog: Res<CatalogSnapshot>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Some(cwd) = &trigger.event().cwd else {
        commands
            .entity(target)
            .remove::<(SessionScopeView, SessionScopeUiStateComponent)>();
        if owners
            .get(target)
            .is_ok_and(|(manager, chat)| !manager && !chat)
        {
            commands.entity(target).remove::<SessionCatalogView>();
        }
        return;
    };
    commands
        .entity(target)
        .insert(SessionScopeView(cwd.clone()));
    let state = SessionScopeUiState::project(cwd, &catalog);
    commands.trigger(UiStateWrite::<SessionScopeUiState>::from_event(
        target, &state,
    ));
}

fn cleanup_inactive_views(
    views: Query<
        (
            Entity,
            Has<SessionManagerView>,
            Has<SessionView>,
            Has<SessionDetailView>,
            Has<SessionScopeView>,
        ),
        (With<SessionCatalogView>, Changed<PageMetadata>),
    >,
    mut commands: Commands,
) {
    for (entity, manager, chat, detail, scope) in &views {
        if detail && !chat {
            commands
                .entity(entity)
                .remove::<(SessionDetailView, SessionDetailUiStateComponent)>();
        }
        if !manager && !chat && !scope {
            commands
                .entity(entity)
                .remove::<(SessionCatalogView, SessionCatalogUiStateComponent)>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_manager_authority_revokes_catalog_state_without_navigation() {
        let mut app = App::new();
        app.add_observer(unpublish);
        let view = app
            .world_mut()
            .spawn((
                SessionManagerView,
                SessionScopeView(PathBuf::from("/tmp/project")),
            ))
            .id();
        app.world_mut()
            .entity_mut(view)
            .remove::<SessionManagerView>();
        app.world_mut().flush();
        assert!(
            app.world()
                .get::<SessionCatalogUiStateComponent>(view)
                .is_none()
        );
        assert!(app.world().get::<SessionScopeView>(view).is_some());
    }

    #[test]
    fn selecting_another_session_requests_the_full_open_lifecycle_in_the_current_stack() {
        let mut app = App::new();
        app.add_plugins(CatalogPlugin)
            .add_message::<PageOpenRequest>();
        let old_session = app
            .world_mut()
            .spawn((Session, SessionId("old".into())))
            .id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(old_session))
            .id();
        let view = app.world_mut().spawn((SessionView, ChildOf(stack))).id();
        app.world_mut().spawn((
            Session,
            SessionId("new".into()),
            vmux_ecs::Cwd(PathBuf::from("/projects/vmux")),
        ));
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: SessionsOpen { id: "new".into() },
        });
        app.world_mut().flush();
        let opens = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, Route::Session(SessionId("new".into())).url());
        assert!(matches!(opens[0].target, PageOpenTarget::Stack(parent) if parent == stack));
    }

    #[derive(Resource, Default)]
    struct Published {
        catalogs: Vec<Entity>,
        details: Vec<Entity>,
        scopes: Vec<(Entity, Vec<String>)>,
    }

    impl Published {
        fn record(
            trigger: On<UiStateWrite<SessionCatalogUiState>>,
            mut published: ResMut<Published>,
        ) {
            if trigger.event().update().catalog.is_some() {
                published.catalogs.push(trigger.event().webview());
            }
        }

        fn record_detail(
            trigger: On<UiStateWrite<SessionDetailUiState>>,
            mut published: ResMut<Published>,
        ) {
            published.details.push(trigger.event().webview());
        }

        fn record_scope(
            trigger: On<UiStateWrite<SessionScopeUiState>>,
            mut published: ResMut<Published>,
        ) {
            published.scopes.push((
                trigger.event().webview(),
                trigger
                    .event()
                    .update()
                    .sessions
                    .iter()
                    .map(|session| session.id.clone())
                    .collect(),
            ));
        }
    }

    #[test]
    fn create_requests_a_session_and_opens_its_canonical_route() {
        let mut app = App::new();
        app.init_resource::<PendingOpens>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_observer(create)
            .add_observer(open_created);
        let stack = app.world_mut().spawn_empty().id();
        let webview = app
            .world_mut()
            .spawn((ChildOf(stack), SessionManagerView))
            .id();
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SessionsCreate {
                name: "Task".into(),
                description: "Description".into(),
                cwd: cwd.to_string_lossy().into_owned(),
            },
        });

        let created = app
            .world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].cwd, cwd);
        let id = created[0].id().clone();
        assert!(!id.0.is_empty());
        let session = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(Created {
            entity: session,
            id: id.clone(),
        });
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
    fn session_view_cannot_create_sessions() {
        let mut app = App::new();
        app.init_resource::<PendingOpens>()
            .add_message::<CreateRequest>()
            .add_observer(create);
        let webview = app
            .world_mut()
            .spawn(super::super::session::SessionView)
            .id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SessionsCreate {
                name: "Task".into(),
                description: String::new(),
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
    fn stage_changes_use_the_canonical_target_instead_of_client_identity_fields() {
        let mut app = App::new();
        app.add_plugins((crate::DomainPlugin, CatalogPlugin));
        let source = app
            .world_mut()
            .spawn((
                crate::provider::Source,
                crate::provider::Origin::Native,
                crate::provider::Capability::Pending,
                crate::provider::Generation(1),
                AgentId("codex".into()),
            ))
            .id();
        let operation = app
            .world_mut()
            .spawn((
                crate::provider::Read {
                    generation: 1,
                    wire_id: 0,
                },
                ChildOf(source),
                vmux_ecs::Operation::bundle("provider catalog")
                    .unwrap()
                    .running(),
            ))
            .id();
        app.world_mut()
            .entity_mut(source)
            .insert(EntityTarget::<crate::provider::Read>::new(operation));
        app.world_mut().write_message(crate::provider::Received {
            operation,
            supported: true,
            result: Ok(vec![crate::ProviderSession {
                kind: "acp:codex".into(),
                sid: "inside".into(),
                name: "Target".into(),
                cwd: "/projects/vmux/task".into(),
                ..Default::default()
            }]),
        });
        app.update();
        let view = app
            .world_mut()
            .spawn(SessionScopeView(PathBuf::from("/projects/vmux")))
            .id();
        app.world_mut().trigger(UiInput {
            webview: view,
            payload: SessionsStageChange {
                id: "acp:codex:inside".into(),
                kind: "acp:codex".into(),
                sid: "outside".into(),
                stage: "done".into(),
            },
        });
        let requests = app
            .world_mut()
            .resource_mut::<Messages<ProviderStageChangeRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].id.0, "inside");
    }

    #[test]
    fn cleanup_forwards_one_authorized_model_request() {
        let mut app = App::new();
        app.add_plugins(CatalogPlugin)
            .add_message::<CleanupRequest>();
        app.world_mut().spawn((
            Session,
            SessionId("canonical".into()),
            vmux_ecs::Cwd::default(),
        ));
        let webview = app.world_mut().spawn(SessionManagerView).id();
        app.world_mut().trigger(UiInput {
            webview,
            payload: SessionsCleanup {
                ids: vec!["canonical".into()],
            },
        });

        let requests = app
            .world_mut()
            .resource_mut::<Messages<CleanupRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].targets,
            vec![CleanupTarget::Session(SessionId("canonical".into()))]
        );
    }

    #[test]
    fn scoped_cleanup_cannot_remove_sessions_outside_its_project() {
        let mut app = App::new();
        app.add_plugins(CatalogPlugin)
            .add_message::<CleanupRequest>();
        app.world_mut().spawn((
            Session,
            SessionId("inside".into()),
            vmux_ecs::Cwd(PathBuf::from("/projects/vmux/session")),
        ));
        app.world_mut().spawn((
            Session,
            SessionId("outside".into()),
            vmux_ecs::Cwd(PathBuf::from("/projects/other/session")),
        ));
        let webview = app
            .world_mut()
            .spawn(SessionScopeView(PathBuf::from("/projects/vmux")))
            .id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SessionsCleanup {
                ids: vec!["inside".into(), "outside".into()],
            },
        });

        let requests = app
            .world_mut()
            .resource_mut::<Messages<CleanupRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].targets,
            vec![CleanupTarget::Session(SessionId("inside".into()))]
        );
    }

    #[test]
    fn manager_can_restore_deleted_provider_sessions() {
        let mut app = App::new();
        app.add_message::<ProviderSessionRestoreRequest>()
            .add_observer(restore_deleted);
        let webview = app.world_mut().spawn(SessionManagerView).id();
        let target = crate::event::ProviderSessionTarget {
            kind: "acp:codex".into(),
            sid: "provider-session".into(),
        };

        app.world_mut().trigger(UiInput {
            webview,
            payload: SessionsRestoreDeleted {
                sessions: vec![target.clone(), target],
            },
        });

        let requests = app
            .world_mut()
            .resource_mut::<Messages<ProviderSessionRestoreRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].keys.len(), 1);
        assert!(requests[0].keys[0].matches("acp:codex", "provider-session"));
    }

    #[test]
    fn manager_creation_uses_the_requested_agent() {
        let mut app = App::new();
        app.init_resource::<PendingOpens>()
            .add_message::<CreateRequest>()
            .add_observer(create);
        let stack = app.world_mut().spawn_empty().id();
        let webview = app
            .world_mut()
            .spawn((
                SessionManagerView,
                ChildOf(stack),
                PageMetadata {
                    url: Route::manager_for_agent(&AgentId("codex".into())),
                    title: String::new(),
                    icon: Default::default(),
                    bg_color: None,
                },
            ))
            .id();
        let cwd = std::env::current_dir().unwrap();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SessionsCreate {
                name: "Task".into(),
                description: String::new(),
                cwd: cwd.to_string_lossy().into_owned(),
            },
        });

        let created = app
            .world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(created[0].agent, Some(AgentId("codex".into())));
    }

    #[test]
    fn page_ready_limits_full_catalog_to_manager_views() {
        let mut app = App::new();
        app.init_resource::<CatalogSnapshot>()
            .init_resource::<SessionRepositories>()
            .init_resource::<Published>()
            .add_observer(Published::record)
            .add_observer(Published::record_detail)
            .add_observer(ready);
        let manager = app.world_mut().spawn(SessionManagerView).id();
        let session = app
            .world_mut()
            .spawn((
                super::super::session::SessionView,
                PageMetadata {
                    url: Route::Session(SessionId("session-1".into())).url(),
                    ..Default::default()
                },
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview: session,
            payload: PageReady {},
        });
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: PageReady {},
        });
        app.world_mut().flush();

        let published = app.world().resource::<Published>();
        assert_eq!(published.catalogs, vec![manager]);
        assert_eq!(published.details, vec![session]);
    }

    #[test]
    fn transitioning_manager_route_gains_ownership_before_activation() {
        let mut app = App::new();
        app.add_plugins(CatalogPlugin);
        let session = app.world_mut().spawn_empty().id();
        let stack = app
            .world_mut()
            .spawn((
                PageMetadata {
                    url: Route::Manager.url(),
                    ..Default::default()
                },
                EntityTarget::<Session>::new(session),
                vmux_command::CommandBarWorkDirectory("/tmp/project".into()),
            ))
            .id();
        let webview = app
            .world_mut()
            .spawn((
                HostsPage,
                PageTransition,
                PageMetadata {
                    url: Route::Manager.url(),
                    ..Default::default()
                },
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert!(app.world().entity(webview).contains::<SessionManagerView>());
        assert!(app.world().get::<EntityTarget<Session>>(stack).is_none());
        assert!(
            app.world()
                .get::<vmux_command::CommandBarWorkDirectory>(stack)
                .is_none()
        );

        app.world_mut().entity_mut(webview).insert(PageMetadata {
            url: Route::Session(SessionId("session-1".into())).url(),
            ..Default::default()
        });
        app.update();

        let entity = app.world().entity(webview);
        assert!(!entity.contains::<SessionManagerView>());
        assert!(!entity.contains::<SessionCatalogView>());
        assert!(!entity.contains::<SessionCatalogUiStateComponent>());
    }

    #[test]
    fn leaving_manager_drops_global_authority_before_the_new_page_paints() {
        let mut app = App::new();
        app.add_plugins(CatalogPlugin);
        let session = app.world_mut().spawn_empty().id();
        let stack = app
            .world_mut()
            .spawn((
                PageMetadata {
                    url: Route::Session(SessionId("session-1".into())).url(),
                    ..Default::default()
                },
                EntityTarget::<Session>::new(session),
            ))
            .id();
        let webview = app
            .world_mut()
            .spawn((
                HostsPage,
                SessionManagerView,
                PageTransition,
                PageMetadata {
                    url: Route::Manager.url(),
                    ..Default::default()
                },
                ChildOf(stack),
            ))
            .id();

        app.update();

        let entity = app.world().entity(webview);
        assert!(!entity.contains::<SessionManagerView>());
        assert!(!entity.contains::<SessionCatalogView>());
        assert!(app.world().get::<EntityTarget<Session>>(stack).is_some());
    }

    #[test]
    fn hidden_prewarm_manager_route_stays_unclaimed() {
        let mut app = App::new();
        app.add_plugins(CatalogPlugin);
        let webview = app
            .world_mut()
            .spawn((
                HostsPage,
                PageMetadata {
                    url: Route::Manager.url(),
                    ..Default::default()
                },
            ))
            .id();

        app.update();

        let entity = app.world().entity(webview);
        assert!(!entity.contains::<SessionManagerView>());
        assert!(!entity.contains::<SessionCatalogView>());
        assert!(!entity.contains::<SessionCatalogUiStateComponent>());
    }

    #[test]
    fn leaving_session_route_removes_detail_projection() {
        let mut app = App::new();
        app.add_systems(Update, cleanup_inactive_views);
        let webview = app
            .world_mut()
            .spawn((
                super::super::session::SessionView,
                SessionDetailView,
                PageMetadata {
                    url: Route::Session(SessionId("session-1".into())).url(),
                    ..Default::default()
                },
            ))
            .id();
        app.update();

        app.world_mut()
            .entity_mut(webview)
            .remove::<super::super::session::SessionView>()
            .insert(PageMetadata {
                url: "git://".into(),
                ..Default::default()
            });
        app.update();

        let entity = app.world().entity(webview);
        assert!(!entity.contains::<SessionDetailView>());
        assert!(!entity.contains::<SessionDetailUiStateComponent>());
        assert!(!entity.contains::<SessionCatalogView>());
    }

    #[test]
    fn explicit_scope_projection_excludes_unrelated_sessions() {
        let mut app = App::new();
        app.init_resource::<CatalogSnapshot>()
            .init_resource::<SessionRepositories>()
            .init_resource::<Published>()
            .add_observer(Published::record_scope)
            .add_observer(scope_projection)
            .add_observer(ready);
        app.world_mut().resource_mut::<CatalogSnapshot>().sessions = vec![
            SessionSummary {
                id: "vmux".into(),
                cwd: "/projects/vmux".into(),
                ..Default::default()
            },
            SessionSummary {
                id: "other".into(),
                cwd: "/projects/other".into(),
                ..Default::default()
            },
        ];
        let index = app.world_mut().spawn_empty().id();
        let detail = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(ScopeProjectionRequest {
            webview: detail,
            cwd: Some(PathBuf::from("/projects/vmux")),
        });

        app.world_mut().trigger(UiInput {
            webview: index,
            payload: PageReady {},
        });
        app.world_mut().trigger(UiInput {
            webview: detail,
            payload: PageReady {},
        });
        app.world_mut().flush();

        assert_eq!(
            app.world().resource::<Published>().scopes,
            vec![(detail, vec!["vmux".into()])]
        );
        assert!(!app.world().entity(index).contains::<SessionScopeView>());
    }

    #[test]
    fn directory_picker_starts_at_the_nearest_existing_directory() {
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
        let missing = cwd.join("missing/session/project");

        assert_eq!(
            DirectoryPicker::initial_directory(&missing.to_string_lossy(), None),
            cwd
        );
    }

    #[test]
    fn project_fallback_collapses_managed_worktrees_and_ignores_provider_storage() {
        assert_eq!(
            SessionProject::fallback(Path::new("/projects/vmux-cloud/.worktrees/vmx-226"), true),
            "vmux-cloud"
        );
        assert_eq!(
            SessionProject::fallback(Path::new("/Users/example/.vibe"), false),
            ""
        );
    }

    #[test]
    fn project_url_preserves_the_managed_project_identity() {
        assert_eq!(
            SessionProject::url(
                Some(Path::new("/projects")),
                Path::new("/projects/github.com/vmux-ai/vmux cloud"),
            ),
            "vmux://projects/github.com/vmux-ai/vmux%20cloud"
        );
        assert_eq!(
            SessionProject::url(
                Some(Path::new("/projects")),
                &SessionProject::root(Path::new("/projects/vmux-cloud/.worktrees/vmx-226"), true,),
            ),
            "vmux://projects/vmux-cloud"
        );
    }
}
