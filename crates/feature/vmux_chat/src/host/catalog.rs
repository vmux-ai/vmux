use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::page::PageReady;
use vmux_ecs::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_ecs::{UiState, UiStateWrite};
use vmux_git::RepoInfoCache;
use vmux_session::{
    AgentId, CatalogSnapshot, CleanupRequest, CreateRequest, Created, DescriptionUpdateRequest,
    RenameRequest, Route, SessionId, StageChangeRequest, StageId,
};

use crate::event::{
    SessionDirectorySelection, SessionRepositories, SessionRepository, SessionsChooseDirectory,
    SessionsCleanup, SessionsCreate, SessionsDescriptionUpdate, SessionsRename,
    SessionsStageChange,
};
use crate::state::ChatUiState;

pub(super) struct CatalogPlugin;

impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingOpens>()
            .init_resource::<DirectoryRevision>()
            .init_resource::<CatalogSnapshot>()
            .init_resource::<SessionRepositories>()
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
            .add_observer(ready)
            .add_systems(Update, finish_directory_pickers)
            .add_systems(PostUpdate, (sync_repositories, publish).chain());
    }
}

#[derive(Component)]
#[require(SessionManagerUiState)]
pub struct SessionManagerView;

type SessionManagerUiState = UiState<ChatUiState>;

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

#[derive(SystemParam)]
struct ManagerViews<'w, 's> {
    views: Query<'w, 's, Option<&'static PageMetadata>, With<SessionManagerView>>,
}

impl ManagerViews<'_, '_> {
    fn contains(&self, entity: Entity) -> bool {
        self.views.contains(entity)
    }

    fn agent(&self, entity: Entity) -> Option<AgentId> {
        self.views
            .get(entity)
            .ok()
            .flatten()
            .and_then(|metadata| Route::requested_agent(&metadata.url))
    }
}

fn create(
    trigger: On<UiInput<SessionsCreate>>,
    parents: Query<&ChildOf>,
    managers: ManagerViews,
    mut pending: ResMut<PendingOpens>,
    mut requests: MessageWriter<CreateRequest>,
) {
    if !managers.contains(trigger.event().webview) {
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
        managers.agent(trigger.event().webview),
    );
    pending.0.insert(request.id().clone(), parent.parent());
    requests.write(request);
}

fn choose_directory(
    trigger: On<UiInput<SessionsChooseDirectory>>,
    managers: ManagerViews,
    pending: Query<&PendingDirectoryPicker>,
    projects: vmux_ecs::profile::Projects,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if !managers.contains(webview) || pending.iter().any(|picker| picker.webview == webview) {
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
    managers: ManagerViews,
    mut requests: MessageWriter<RenameRequest>,
) {
    if !managers.contains(trigger.event().webview) {
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
    managers: ManagerViews,
    mut requests: MessageWriter<DescriptionUpdateRequest>,
) {
    if !managers.contains(trigger.event().webview) {
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
    managers: ManagerViews,
    mut requests: MessageWriter<StageChangeRequest>,
) {
    if !managers.contains(trigger.event().webview) {
        return;
    }
    let request = &trigger.event().payload;
    requests.write(StageChangeRequest {
        id: SessionId(request.id.clone()),
        stage: StageId(request.stage.clone()),
    });
}

fn cleanup(
    trigger: On<UiInput<SessionsCleanup>>,
    managers: ManagerViews,
    mut requests: MessageWriter<CleanupRequest>,
) {
    if !managers.contains(trigger.event().webview) {
        return;
    }
    requests.write(CleanupRequest {
        id: SessionId(trigger.event().payload.id.clone()),
    });
}

fn publish(
    catalog: Res<CatalogSnapshot>,
    repositories: Res<SessionRepositories>,
    targets: Query<Entity, (With<SessionManagerView>, With<PageReady>)>,
    mut commands: Commands,
) {
    if !catalog.is_changed() && !repositories.is_changed() {
        return;
    }
    for target in &targets {
        commands.trigger(UiStateWrite::<ChatUiState>::from_event(target, &*catalog));
        commands.trigger(UiStateWrite::<ChatUiState>::from_event(
            target,
            &*repositories,
        ));
    }
}

fn sync_repositories(
    catalog: Res<CatalogSnapshot>,
    mut cache: Option<Single<&mut RepoInfoCache>>,
    mut repositories: ResMut<SessionRepositories>,
) {
    let mut sessions = Vec::with_capacity(catalog.sessions.len());
    for session in &catalog.sessions {
        let path = Path::new(&session.cwd);
        let info = cache.as_mut().and_then(|cache| cache.lookup(path));
        let repository = match info {
            Some(info) => SessionRepository {
                id: session.id.clone(),
                project: info.project_name(),
                branch: info.branch,
                url: vmux_git::GitUrl::from_path(&info.repo_root).unwrap_or_default(),
            },
            None => SessionRepository {
                id: session.id.clone(),
                project: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                branch: String::new(),
                url: String::new(),
            },
        };
        sessions.push(repository);
    }
    let next = SessionRepositories { sessions };
    if *repositories != next {
        *repositories = next;
    }
}

fn ready(
    trigger: On<UiInput<PageReady>>,
    managers: ManagerViews,
    catalog: Res<CatalogSnapshot>,
    repositories: Res<SessionRepositories>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    if managers.contains(target) {
        commands.trigger(UiStateWrite::<ChatUiState>::from_event(target, &*catalog));
        commands.trigger(UiStateWrite::<ChatUiState>::from_event(
            target,
            &*repositories,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Published(Vec<Entity>);

    impl Published {
        fn record(trigger: On<UiStateWrite<ChatUiState>>, mut published: ResMut<Published>) {
            if trigger.event().update().sessions.is_some() {
                published.0.push(trigger.event().webview());
            }
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
    fn page_ready_republishes_catalog_only_to_manager_views() {
        let mut app = App::new();
        app.init_resource::<CatalogSnapshot>()
            .init_resource::<SessionRepositories>()
            .init_resource::<Published>()
            .add_observer(Published::record)
            .add_observer(ready);
        let manager = app.world_mut().spawn(SessionManagerView).id();
        let session = app.world_mut().spawn(super::super::session::ChatView).id();

        app.world_mut().trigger(UiInput {
            webview: session,
            payload: PageReady {},
        });
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: PageReady {},
        });
        app.world_mut().flush();

        assert_eq!(app.world().resource::<Published>().0, vec![manager]);
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
}
