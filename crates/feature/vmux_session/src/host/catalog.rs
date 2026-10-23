use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::page::{Activity, PageReady};
use vmux_ecs::{KeyboardOwner, PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_ecs::{UiState, UiStatePlugin, UiStateWrite};
use vmux_git::RepoInfoCache;
#[cfg(test)]
use vmux_session::SessionSummary;
use vmux_session::{
    AgentId, CatalogSnapshot, CleanupRequest, CreateRequest, Created, DescriptionUpdateRequest,
    ProviderKind, ProviderSessionId, ProviderStageChangeRequest, RenameRequest, Route, SessionId,
    StageChangeRequest, StageId,
};

use super::session::ChatView;
use crate::event::{
    SessionDirectorySelection, SessionRepositories, SessionRepository, SessionsChooseDirectory,
    SessionsCleanup, SessionsCreate, SessionsDescriptionUpdate, SessionsRename,
    SessionsStageChange,
};
use crate::state::{ChatUiState, SessionCatalogUiState, SessionDetailUiState, SessionScopeUiState};

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
                SessionsChooseDirectory,
                SessionsRename,
                SessionsDescriptionUpdate,
                SessionsStageChange,
                SessionsCleanup,
            )>::default())
            .add_observer(create)
            .add_observer(choose_directory)
            .add_observer(open_created)
            .add_observer(rename)
            .add_observer(update_description)
            .add_observer(change_stage)
            .add_observer(cleanup)
            .add_observer(scope_projection)
            .add_observer(ready)
            .add_systems(Update, (finish_directory_pickers, cleanup_inactive_views))
            .add_systems(
                PostUpdate,
                (
                    sync_repositories.run_if(Activity::<SessionCatalogView>::active),
                    publish_manager,
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

type SessionCatalogUiStateComponent = UiState<SessionCatalogUiState>;
type SessionDetailUiStateComponent = UiState<SessionDetailUiState>;
type SessionScopeUiStateComponent = UiState<SessionScopeUiState>;

#[derive(Component, Default)]
pub struct SessionCatalogView;

#[derive(Component, Default)]
#[require(SessionCatalogView, SessionDetailUiStateComponent)]
struct SessionDetailView;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[require(SessionCatalogView, SessionScopeUiStateComponent)]
struct SessionScopeView(PathBuf);

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
    fn fallback(path: &Path, managed: bool) -> String {
        if !managed {
            return String::new();
        }
        for ancestor in path.ancestors() {
            if ancestor
                .file_name()
                .is_some_and(|name| name == ".worktrees")
            {
                return ancestor
                    .parent()
                    .and_then(Path::file_name)
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
            }
        }
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

#[derive(SystemParam)]
struct CatalogViews<'w, 's> {
    managers: Query<'w, 's, Option<&'static PageMetadata>, With<SessionManagerView>>,
    chats: Query<'w, 's, (), With<ChatView>>,
    scopes: Query<'w, 's, (), With<SessionScopeView>>,
}

impl CatalogViews<'_, '_> {
    fn contains(&self, entity: Entity) -> bool {
        self.managers.contains(entity)
            || self.chats.contains(entity)
            || self.scopes.contains(entity)
    }

    fn manager_contains(&self, entity: Entity) -> bool {
        self.managers.contains(entity)
    }

    fn agent(&self, entity: Entity) -> Option<AgentId> {
        self.managers
            .get(entity)
            .ok()
            .flatten()
            .and_then(|metadata| Route::requested_agent(&metadata.url))
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
    mut requests: MessageWriter<RenameRequest>,
) {
    if !views.contains(trigger.event().webview) {
        return;
    }
    let request = &trigger.event().payload;
    requests.write(RenameRequest {
        id: SessionId(request.id.clone()),
        name: request.name.clone(),
    });
}

fn update_description(
    trigger: On<UiInput<SessionsDescriptionUpdate>>,
    views: CatalogViews,
    mut requests: MessageWriter<DescriptionUpdateRequest>,
) {
    if !views.contains(trigger.event().webview) {
        return;
    }
    let request = &trigger.event().payload;
    requests.write(DescriptionUpdateRequest {
        id: SessionId(request.id.clone()),
        description: request.description.clone(),
    });
}

fn change_stage(
    trigger: On<UiInput<SessionsStageChange>>,
    views: CatalogViews,
    providers: Res<vmux_session::ProviderCatalog>,
    mut requests: MessageWriter<StageChangeRequest>,
    mut provider_requests: MessageWriter<ProviderStageChangeRequest>,
) {
    if !views.contains(trigger.event().webview) {
        return;
    }
    let request = &trigger.event().payload;
    if !request.kind.is_empty() && request.kind != "session" && !request.sid.is_empty() {
        let source_updated_at = providers
            .get(&request.kind, &request.sid)
            .map(|session| session.updated_at)
            .unwrap_or_default();
        provider_requests.write(ProviderStageChangeRequest {
            kind: ProviderKind(request.kind.clone()),
            id: ProviderSessionId(request.sid.clone()),
            stage: StageId(request.stage.clone()),
            source_updated_at,
        });
        return;
    }
    requests.write(StageChangeRequest {
        id: SessionId(request.id.clone()),
        stage: StageId(request.stage.clone()),
    });
}

fn cleanup(
    trigger: On<UiInput<SessionsCleanup>>,
    views: CatalogViews,
    mut requests: MessageWriter<CleanupRequest>,
) {
    if !views.contains(trigger.event().webview) {
        return;
    }
    requests.write(CleanupRequest {
        id: SessionId(trigger.event().payload.id.clone()),
    });
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
            Some(Route::Manager) | None => return Self::default(),
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
        }
    }
}

fn publish_manager(
    catalog: Res<CatalogSnapshot>,
    repositories: Res<SessionRepositories>,
    targets: Query<
        Entity,
        (
            With<SessionManagerView>,
            With<PageReady>,
            With<KeyboardOwner>,
        ),
    >,
    claimed: Query<
        (),
        (
            With<SessionManagerView>,
            With<PageReady>,
            Added<KeyboardOwner>,
        ),
    >,
    mut commands: Commands,
) {
    if !catalog.is_changed() && !repositories.is_changed() && claimed.is_empty() {
        return;
    }
    for target in &targets {
        commands.trigger(UiStateWrite::<SessionCatalogUiState>::from_event(
            target, &*catalog,
        ));
        commands.trigger(UiStateWrite::<SessionCatalogUiState>::from_event(
            target,
            &*repositories,
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
        commands.trigger(UiStateWrite::<SessionDetailUiState>::from_event(
            target, &state,
        ));
    }
}

fn sync_repositories(
    catalog: Res<CatalogSnapshot>,
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
            Added<KeyboardOwner>,
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
        let paths = catalog
            .sessions
            .iter()
            .filter(|session| !session.cwd.is_empty())
            .map(|session| PathBuf::from(&session.cwd))
            .collect::<Vec<_>>();
        for owner in &active {
            cache.observe_active_paths(owner, paths.iter().cloned());
        }
    }
    let mut sessions = Vec::with_capacity(catalog.sessions.len());
    for session in &catalog.sessions {
        let path = Path::new(&session.cwd);
        let info = cache
            .as_mut()
            .and_then(|cache| cache.bypass_change_detection().snapshot(path));
        let repository = match info {
            Some(info) => SessionRepository {
                id: session.id.clone(),
                project: info.project_name(),
                branch: info.branch,
                url: vmux_git::GitUrl::from_path(&info.repo_root).unwrap_or_default(),
            },
            None => SessionRepository {
                id: session.id.clone(),
                project: SessionProject::fallback(path, session.managed),
                branch: String::new(),
                url: String::new(),
            },
        };
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

fn ready(
    trigger: On<UiInput<PageReady>>,
    views: CatalogViews,
    pages: Query<&PageMetadata>,
    scopes: Query<&SessionScopeView>,
    catalog: Res<CatalogSnapshot>,
    repositories: Res<SessionRepositories>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    if views.manager_contains(target) {
        commands.entity(target).insert(SessionCatalogView);
        commands.trigger(UiStateWrite::<SessionCatalogUiState>::from_event(
            target, &*catalog,
        ));
        commands.trigger(UiStateWrite::<SessionCatalogUiState>::from_event(
            target,
            &*repositories,
        ));
    }
    if views.chats.contains(target)
        && let Ok(page) = pages.get(target)
    {
        commands.entity(target).insert(SessionDetailView);
        let state = SessionDetailUiState::project(page, &catalog, &repositories);
        commands.trigger(UiStateWrite::<SessionDetailUiState>::from_event(
            target, &state,
        ));
    }
    if let Ok(scope) = scopes.get(target) {
        let state = SessionScopeUiState::project(&scope.0, &catalog);
        commands.trigger(UiStateWrite::<SessionScopeUiState>::from_event(
            target, &state,
        ));
    }
}

fn scope_projection(
    trigger: On<ScopeProjectionRequest>,
    owners: Query<(Has<SessionManagerView>, Has<ChatView>)>,
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
            Has<ChatView>,
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
            commands.entity(entity).remove::<SessionCatalogView>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let webview = app.world_mut().spawn(super::super::session::ChatView).id();

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
    fn page_ready_republishes_catalog_to_session_views() {
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
                super::super::session::ChatView,
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
    fn leaving_session_route_removes_detail_projection() {
        let mut app = App::new();
        app.add_systems(Update, cleanup_inactive_views);
        let webview = app
            .world_mut()
            .spawn((
                super::super::session::ChatView,
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
            .remove::<super::super::session::ChatView>()
            .insert(PageMetadata {
                url: "vmux://git/".into(),
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
}
