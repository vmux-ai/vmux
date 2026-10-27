use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::Id;
use vmux_api::avatar::IdenticonSpec;
use vmux_api::project::ProjectRef;
use vmux_ecs::page::PrewarmPage;
use vmux_ecs::persistence::PersistenceAppExt;
use vmux_git::worktree::RepoLabel;
use vmux_git::{
    RepositoryProvisionCompleted, RepositoryProvisionOperation, RepositoryProvisionRequest,
};
use vmux_session::host::ScopeProjectionRequest;
use vmux_session::{CatalogSnapshot, SessionSummaryProjection};

use crate::project::{
    Project, ProjectArchiveRequest, ProjectBranch, ProjectBundle, ProjectCatalogView,
    ProjectCloneRequest, ProjectCollectionViewRequest, ProjectCreateRequest, ProjectDeleteRequest,
    ProjectDetailView, ProjectEntry, ProjectImportRequest, ProjectOwner, ProjectPath, ProjectRoute,
    ProjectSource, ProjectTarget, ProjectView, UiProject,
};

pub(super) struct ProjectPagePlugin;

impl Plugin for ProjectPagePlugin {
    fn build(&self, app: &mut App) {
        app.register_persisted::<Project>()
            .register_persisted::<Id<Project>>()
            .register_persisted::<ProjectPath>()
            .register_persisted::<ProjectSource>()
            .register_persisted::<ProjectRef>()
            .add_plugins(vmux_ecs::IdentityIndexPlugin::<Project>::default())
            .add_plugins(vmux_ecs::UiStatePlugin::<UiProject>::default())
            .add_plugins(UiEventPlugin::<(
                ProjectCreateRequest,
                ProjectImportRequest,
                ProjectCloneRequest,
                ProjectArchiveRequest,
                ProjectDeleteRequest,
                ProjectCollectionViewRequest,
            )>::default())
            .add_observer(create)
            .add_observer(import)
            .add_observer(clone)
            .add_observer(archive)
            .add_observer(delete)
            .add_observer(collection_view)
            .add_systems(PreStartup, prewarm)
            .add_systems(PreUpdate, rebuild_project_index)
            .add_systems(
                Update,
                (
                    resolve_project_owners,
                    queue,
                    sync_icons,
                    finish_catalogs,
                    bevy::ecs::schedule::ApplyDeferred,
                    rebuild_project_index,
                    reconcile_project_refs.in_set(vmux_session::SessionMutationReceiveSet),
                    finish_imports,
                    finish_deletes,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                (
                    resolve_project_targets,
                    finish_repository_operations,
                    sync_session_counts,
                )
                    .chain(),
            );
    }
}

fn rebuild_project_index(mut index: vmux_ecs::IdentityIndexWriter<Project>) {
    index.rebuild();
}

type ProjectIdentities<'w, 's> =
    Query<'w, 's, (Entity, &'static Id<Project>, &'static ProjectPath), With<Project>>;

struct ProjectMatch;

impl ProjectMatch {
    fn find(path: &Path, projects: &ProjectIdentities) -> Option<Id<Project>> {
        projects
            .iter()
            .filter(|(_, _, project_path)| path.starts_with(project_path.path()))
            .max_by_key(|(_, _, project_path)| project_path.path().components().count())
            .map(|(_, id, _)| id.clone())
    }
}

fn reconcile_project_refs(
    index: Res<vmux_ecs::IdentityIndex<Project>>,
    projects: ProjectIdentities,
    sessions: Query<
        (
            &Id<vmux_session::Session>,
            Ref<vmux_ecs::Cwd>,
            Option<&ProjectRef>,
        ),
        With<vmux_session::Session>,
    >,
    spaces: Query<
        (
            Entity,
            Ref<vmux_layout::space::EffectiveStartupDir>,
            Option<&ProjectRef>,
        ),
        With<vmux_layout::space::Space>,
    >,
    mut session_mutations: MessageWriter<vmux_session::SessionMutationRequest>,
    mut commands: Commands,
) {
    let projects_changed = index.is_changed();
    for (session, cwd, current) in &sessions {
        if !projects_changed && !cwd.is_changed() && current.is_some() {
            continue;
        }
        let next = ProjectMatch::find(&cwd.0, &projects);
        if current.map(ProjectRef::id) == next.as_ref() {
            continue;
        }
        session_mutations.write(vmux_session::SessionMutationRequest::local(
            session.clone(),
            vmux_session::SessionMutation::ChangeProject(next),
        ));
    }
    for (entity, startup_dir, current) in &spaces {
        if !projects_changed && !startup_dir.is_changed() && current.is_some() {
            continue;
        }
        let path = startup_dir.0.as_deref().unwrap_or_else(|| Path::new(""));
        reconcile_project_ref(entity, path, current, &projects, &mut commands);
    }
}

fn reconcile_project_ref(
    entity: Entity,
    path: &Path,
    current: Option<&ProjectRef>,
    projects: &ProjectIdentities,
    commands: &mut Commands,
) {
    let next = path
        .is_absolute()
        .then(|| ProjectMatch::find(path, projects))
        .flatten();
    match (next, current) {
        (Some(next), Some(current)) if current.id() == &next => {}
        (Some(next), _) => {
            commands.entity(entity).insert(ProjectRef::new(next));
        }
        (None, Some(_)) => {
            commands.entity(entity).remove::<ProjectRef>();
        }
        (None, None) => {}
    }
}

fn resolve_project_targets(
    index: Res<vmux_ecs::IdentityIndex<Project>>,
    references: Query<(Entity, Ref<ProjectRef>, Option<&ProjectTarget>)>,
    mut commands: Commands,
) {
    for (entity, reference, current) in &references {
        if !index.is_changed() && !reference.is_changed() {
            continue;
        }
        let target = index.entity(reference.id()).ok().flatten();
        match (target, current.map(ProjectTarget::entity)) {
            (Some(target), Some(current)) if target == current => {}
            (Some(target), _) => {
                commands.entity(entity).insert(ProjectTarget::new(target));
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<ProjectTarget>();
            }
            (None, None) => {}
        }
    }
}

fn resolve_project_owners(
    projects: Query<Entity, (With<Project>, Without<ProjectOwner>)>,
    owner: vmux_ecs::profile::Projects,
    mut commands: Commands,
) {
    let Some(owner) = owner.entity() else {
        return;
    };
    for project in &projects {
        commands.entity(project).insert(ProjectOwner::new(owner));
    }
}

fn prewarm(mut commands: Commands) {
    commands.spawn(PrewarmPage {
        host: "projects",
        url: ProjectRoute::ROOT,
        title: "Projects",
        pool_size: 1,
    });
}

#[derive(Component)]
struct ProjectCatalogTask {
    revision: u64,
    task: Task<UiProject>,
}

impl ProjectCatalogTask {
    fn spawn(
        root: PathBuf,
        revision: u64,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> Self {
        let task = IoTaskPool::get().spawn(async move {
            let state = ProjectCatalog::read(&root);
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            state
        });
        Self { revision, task }
    }
}

#[derive(Component, Clone, Default, PartialEq, Eq)]
struct ProjectPageRevision {
    url: String,
    value: u64,
}

impl ProjectPageRevision {
    fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            value: 1,
        }
    }

    fn advance_to(&mut self, url: &str) -> Option<u64> {
        if self.url == url {
            return None;
        }
        self.url = url.to_string();
        self.value = self.value.wrapping_add(1).max(1);
        Some(self.value)
    }
}

#[derive(SystemParam)]
struct ProjectCollectionViews<'w, 's> {
    hierarchy: vmux_layout::space::SpaceHierarchy<'w, 's>,
    values: Query<'w, 's, &'static vmux_ecs::CollectionView, With<vmux_layout::space::Space>>,
}

impl ProjectCollectionViews<'_, '_> {
    fn get(&self, entity: Entity) -> vmux_ecs::CollectionView {
        self.hierarchy
            .get(entity)
            .and_then(|space| self.values.get(space).ok())
            .copied()
            .unwrap_or_default()
    }
}

#[derive(SystemParam)]
struct ProjectPageQueries<'w, 's> {
    pages: Query<
        'w,
        's,
        (Entity, &'static mut vmux_ecs::PageMetadata),
        Changed<vmux_ecs::PageMetadata>,
    >,
    states: Query<'w, 's, &'static vmux_ecs::UiState<UiProject>>,
    revisions: Query<'w, 's, &'static mut ProjectPageRevision>,
    collection_views: ProjectCollectionViews<'w, 's>,
}

#[derive(Component)]
struct PendingProjectOperation {
    webview: Entity,
    root: PathBuf,
    revision: u64,
}

#[derive(SystemParam)]
struct ProjectOperationPages<'w, 's> {
    pages: Query<
        'w,
        's,
        (
            &'static vmux_ecs::PageMetadata,
            &'static ProjectPageRevision,
        ),
    >,
    pending: Query<'w, 's, &'static PendingProjectOperation>,
    states: Query<'w, 's, &'static vmux_ecs::UiState<UiProject>>,
}

impl ProjectOperationPages<'_, '_> {
    fn for_page(&self, webview: Entity, root: &Path) -> Option<PendingProjectOperation> {
        if self
            .pending
            .iter()
            .any(|operation| operation.webview == webview)
        {
            return None;
        }
        let Ok((page, revision)) = self.pages.get(webview) else {
            return None;
        };
        if !ProjectRoute::is_root(&page.url) {
            return None;
        }
        Some(PendingProjectOperation {
            webview,
            root: root.to_path_buf(),
            revision: revision.value,
        })
    }

    fn pending_state(&self, webview: Entity) -> UiProject {
        self.states
            .get(webview)
            .ok()
            .and_then(vmux_ecs::UiState::current)
            .cloned()
            .unwrap_or_default()
            .pending()
    }
}

#[derive(Component)]
struct ProjectImportTask(Task<Result<(), String>>);

#[derive(Component)]
struct ProjectDeleteTask(Task<Result<(), String>>);

struct ProjectName(String);

impl ProjectName {
    fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value.is_empty() || value == "." || value == ".." || value.contains(['/', '\\']) {
            return Err(vmux_ui::i18n::translate("project-name-invalid"));
        }
        Ok(Self(value.to_string()))
    }

    fn provision(self, root: &Path) -> Result<RepositoryProvisionOperation, String> {
        let destination = root.join(&self.0);
        if destination.exists() {
            return Err(vmux_ui::i18n::translate("project-already-exists"));
        }
        Ok(RepositoryProvisionOperation::Initialize {
            destination,
            initial_branch: "main".to_string(),
        })
    }
}

struct ProjectImport;

impl ProjectImport {
    async fn select(root: &Path) -> Result<(), String> {
        let selected = rfd::AsyncFileDialog::new()
            .set_title(vmux_ui::i18n::translate("project-import"))
            .pick_folder()
            .await;
        let Some(selected) = selected else {
            return Ok(());
        };
        let selected = selected
            .path()
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !selected.join(".git").exists() {
            return Err(vmux_ui::i18n::translate("project-import-not-repository"));
        }
        if selected.starts_with(root) {
            return Ok(());
        }
        let imports = root.join("imported");
        std::fs::create_dir_all(&imports).map_err(|error| error.to_string())?;
        let name = selected
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| vmux_ui::i18n::translate("project-name-invalid"))?;
        let link = ImportedProject::available_path(&imports, name, &selected)?;
        if link.exists() {
            return Ok(());
        }
        std::os::unix::fs::symlink(&selected, link).map_err(|error| error.to_string())
    }
}

struct ImportedProject;

impl ImportedProject {
    fn available_path(imports: &Path, name: &str, selected: &Path) -> Result<PathBuf, String> {
        for suffix in 1..=999 {
            let candidate = if suffix == 1 {
                imports.join(name)
            } else {
                imports.join(format!("{name}-{suffix}"))
            };
            if let Ok(existing) = candidate.canonicalize()
                && existing == selected
            {
                return Ok(candidate);
            }
            if std::fs::symlink_metadata(&candidate).is_err() {
                return Ok(candidate);
            }
        }
        Err(vmux_ui::i18n::translate("project-import-name-conflict"))
    }
}

struct ProjectDeletion {
    path: PathBuf,
    imported: bool,
}

impl ProjectDeletion {
    fn resolve(root: &Path, url: &str) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        let path = ProjectRoute::route_path(url, &root)
            .ok_or_else(|| vmux_ui::i18n::translate("error-page-not-found"))??;
        if path == root {
            return Err(vmux_ui::i18n::translate("error-page-not-found"));
        }
        ProjectRoute::resolve(url, &root)
            .ok_or_else(|| vmux_ui::i18n::translate("error-page-not-found"))??;
        let imported = path.parent() == Some(root.join("imported").as_path())
            && std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_symlink());
        if !imported && !path.starts_with(&root) {
            return Err(vmux_ui::i18n::translate("error-page-not-found"));
        }
        Ok(Self { path, imported })
    }

    fn remove(self) -> Result<(), String> {
        if self.imported {
            std::fs::remove_file(self.path).map_err(|error| error.to_string())
        } else {
            std::fs::remove_dir_all(self.path).map_err(|error| error.to_string())
        }
    }
}

struct GitHubRepository {
    owner: String,
    name: String,
    remote: String,
}

impl GitHubRepository {
    fn parse(value: &str) -> Result<Self, String> {
        let input = value.trim().trim_end_matches('/');
        let path = if let Some(path) = input.strip_prefix("git@github.com:") {
            path
        } else if let Some(path) = input.strip_prefix("https://github.com/") {
            path
        } else if let Some(path) = input.strip_prefix("http://github.com/") {
            path
        } else {
            input
        };
        let path = path.trim_end_matches(".git");
        let mut segments = path.split('/');
        let Some(owner) = segments.next() else {
            return Err(vmux_ui::i18n::translate("project-clone-invalid"));
        };
        let Some(name) = segments.next() else {
            return Err(vmux_ui::i18n::translate("project-clone-invalid"));
        };
        if segments.next().is_some() || !Self::valid_segment(owner) || !Self::valid_segment(name) {
            return Err(vmux_ui::i18n::translate("project-clone-invalid"));
        }
        let remote = if input.contains("://") || input.starts_with("git@") {
            input.to_string()
        } else {
            format!("https://github.com/{owner}/{name}.git")
        };
        Ok(Self {
            owner: owner.to_string(),
            name: name.to_string(),
            remote,
        })
    }

    fn valid_segment(value: &str) -> bool {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    }

    fn provision(self, root: &Path) -> Result<RepositoryProvisionOperation, String> {
        let destination = root.join("github.com").join(&self.owner).join(&self.name);
        if destination.exists() {
            return Err(vmux_ui::i18n::translate("project-already-exists"));
        }
        Ok(RepositoryProvisionOperation::Clone {
            remote: self.remote,
            destination,
        })
    }
}

fn create(
    trigger: On<UiInput<ProjectCreateRequest>>,
    projects: vmux_ecs::profile::Projects,
    pages: ProjectOperationPages,
    mut requests: MessageWriter<RepositoryProvisionRequest>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(root) = projects.path() else {
        return;
    };
    let Some(pending) = pages.for_page(webview, root) else {
        return;
    };
    let operation = match ProjectName::parse(&trigger.event().payload.name)
        .and_then(|name| name.provision(root))
    {
        Ok(operation) => operation,
        Err(error) => {
            commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
                webview,
                &UiProject {
                    view: ProjectView::Error(error),
                    ..Default::default()
                },
            ));
            return;
        }
    };
    let state = pages.pending_state(webview);
    commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
        webview, &state,
    ));
    let requester = commands.spawn(pending).id();
    requests.write(RepositoryProvisionRequest {
        requester,
        operation,
    });
}

fn import(
    trigger: On<UiInput<ProjectImportRequest>>,
    projects: vmux_ecs::profile::Projects,
    pages: ProjectOperationPages,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(root) = projects.path() else {
        return;
    };
    let Some(pending) = pages.for_page(webview, root) else {
        return;
    };
    let state = pages.pending_state(webview);
    commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
        webview, &state,
    ));
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let root = root.to_path_buf();
    let task = IoTaskPool::get().spawn(async move {
        let result = ProjectImport::select(&root).await;
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        result
    });
    commands.spawn((pending, ProjectImportTask(task)));
}

fn clone(
    trigger: On<UiInput<ProjectCloneRequest>>,
    projects: vmux_ecs::profile::Projects,
    pages: ProjectOperationPages,
    mut requests: MessageWriter<RepositoryProvisionRequest>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(root) = projects.path() else {
        return;
    };
    let Some(pending) = pages.for_page(webview, root) else {
        return;
    };
    let operation = match GitHubRepository::parse(&trigger.event().payload.repository)
        .and_then(|repository| repository.provision(root))
    {
        Ok(operation) => operation,
        Err(error) => {
            commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
                webview,
                &UiProject {
                    view: ProjectView::Error(error),
                    ..Default::default()
                },
            ));
            return;
        }
    };
    let state = pages.pending_state(webview);
    commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
        webview, &state,
    ));
    let requester = commands.spawn(pending).id();
    requests.write(RepositoryProvisionRequest {
        requester,
        operation,
    });
}

fn archive(
    trigger: On<UiInput<ProjectArchiveRequest>>,
    projects: vmux_ecs::profile::Projects,
    index: Res<vmux_ecs::IdentityIndex<Project>>,
    states: Query<(
        Entity,
        &vmux_ecs::PageMetadata,
        &vmux_ecs::UiState<UiProject>,
    )>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Ok(root) = projects.path() else {
        return;
    };
    let Some(Ok(_)) = ProjectRoute::resolve(&request.url, root) else {
        return;
    };
    let id = Id::<Project>::new(request.url.clone());
    let Ok(Some(project)) = index.entity(&id) else {
        return;
    };
    if request.archived {
        commands.entity(project).insert(vmux_ecs::ArchivedAt::now());
    } else {
        commands.entity(project).remove::<vmux_ecs::ArchivedAt>();
    }
    for (entity, page, state) in &states {
        if !ProjectRoute::is_root(&page.url) {
            continue;
        }
        let Some(mut current) = state.current().cloned() else {
            continue;
        };
        let ProjectView::Catalog(catalog) = &mut current.view else {
            continue;
        };
        let Some(entry) = catalog
            .entries
            .iter_mut()
            .find(|entry| entry.url == request.url)
        else {
            continue;
        };
        entry.archived = request.archived;
        commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
            entity, &current,
        ));
    }
}

fn delete(
    trigger: On<UiInput<ProjectDeleteRequest>>,
    projects: vmux_ecs::profile::Projects,
    pages: ProjectOperationPages,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(root) = projects.path() else {
        return;
    };
    let Some(pending) = pages.for_page(webview, root) else {
        return;
    };
    let deletion = match ProjectDeletion::resolve(root, &trigger.event().payload.url) {
        Ok(deletion) => deletion,
        Err(error) => {
            commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
                webview,
                &UiProject {
                    view: ProjectView::Error(error),
                    ..Default::default()
                },
            ));
            return;
        }
    };
    let mut state = pages.pending_state(webview);
    if let ProjectView::Catalog(catalog) = &mut state.view {
        catalog
            .entries
            .retain(|entry| entry.url != trigger.event().payload.url);
    }
    commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
        webview, &state,
    ));
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let task = IoTaskPool::get().spawn(async move {
        let result = deletion.remove();
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        result
    });
    commands.spawn((pending, ProjectDeleteTask(task)));
}

fn collection_view(
    trigger: On<UiInput<ProjectCollectionViewRequest>>,
    hierarchy: vmux_layout::space::SpaceHierarchy,
    mut spaces: Query<&mut vmux_ecs::CollectionView, With<vmux_layout::space::Space>>,
    states: Query<&vmux_ecs::UiState<UiProject>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Some(space) = hierarchy.get(webview) else {
        return;
    };
    let view = trigger.event().payload.view;
    if let Ok(mut current) = spaces.get_mut(space) {
        *current = view;
    }
    let mut state = states
        .get(webview)
        .ok()
        .and_then(vmux_ecs::UiState::current)
        .cloned()
        .unwrap_or_default();
    state.collection_view = view;
    commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
        webview, &state,
    ));
}

fn queue(
    queries: ProjectPageQueries,
    settings: Res<vmux_setting::AppSettings>,
    projects: vmux_ecs::profile::Projects,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let ProjectPageQueries {
        mut pages,
        states,
        mut revisions,
        collection_views,
    } = queries;
    for (entity, mut page) in &mut pages {
        let url = page.url.clone();
        let collection_view = collection_views.get(entity);
        if !ProjectRoute::is_page(&url) {
            let Ok(mut revision) = revisions.get_mut(entity) else {
                continue;
            };
            if revision.advance_to(&url).is_none() {
                continue;
            }
            commands.entity(entity).remove::<ProjectCatalogTask>();
            commands.trigger(ScopeProjectionRequest {
                webview: entity,
                cwd: None,
            });
            continue;
        }
        let revision = match revisions.get_mut(entity) {
            Ok(mut revision) => {
                let Some(revision) = revision.advance_to(&url) else {
                    continue;
                };
                revision
            }
            Err(_) => {
                commands
                    .entity(entity)
                    .insert(ProjectPageRevision::new(&url));
                1
            }
        };
        let root = match projects.path() {
            Ok(root) => root.to_path_buf(),
            Err(error) => {
                commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
                    entity,
                    &UiProject {
                        view: ProjectView::Error(error),
                        collection_view,
                    },
                ));
                continue;
            }
        };
        if !ProjectRoute::is_root(&url) {
            let (state, cwd) = match ProjectRoute::resolve(&url, &root) {
                Some(Ok(path)) => {
                    let name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.to_string_lossy().into_owned());
                    page.title.clone_from(&name);
                    let id = Id::<Project>::new(url.clone());
                    (
                        UiProject {
                            view: ProjectView::Detail(ProjectDetailView {
                                id: id.clone(),
                                name,
                                path: path.to_string_lossy().into_owned(),
                            }),
                            collection_view,
                        },
                        Some(path),
                    )
                }
                Some(Err(error)) => (
                    UiProject {
                        view: ProjectView::Error(error),
                        collection_view,
                    },
                    None,
                ),
                None => continue,
            };
            let seed = match &state.view {
                ProjectView::Detail(detail) => detail.id.as_str(),
                _ => &url,
            };
            page.icon = vmux_api::PageIcon::favicon(IdenticonSpec::favicon_with_variant(
                seed,
                settings.appearance.project_icon,
            ));
            commands.entity(entity).remove::<ProjectCatalogTask>();
            commands.trigger(ScopeProjectionRequest {
                webview: entity,
                cwd,
            });
            commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
                entity, &state,
            ));
            continue;
        }
        page.title = vmux_ui::i18n::translate("layout-projects");
        page.icon = vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Project);
        commands.trigger(ScopeProjectionRequest {
            webview: entity,
            cwd: None,
        });
        let has_catalog = states.get(entity).is_ok_and(|state| {
            state
                .current()
                .is_some_and(|state| matches!(state.view, ProjectView::Catalog(_)))
        });
        if !has_catalog {
            commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
                entity,
                &UiProject {
                    view: ProjectView::Loading,
                    collection_view,
                },
            ));
        }
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        commands
            .entity(entity)
            .insert(ProjectCatalogTask::spawn(root, revision, wake));
    }
}

fn sync_icons(
    settings: Res<vmux_setting::AppSettings>,
    mut pages: Query<&mut vmux_ecs::PageMetadata>,
) {
    if !settings.is_changed() {
        return;
    }
    for mut page in &mut pages {
        if !ProjectRoute::is_page(&page.url) || ProjectRoute::is_root(&page.url) {
            continue;
        }
        let icon = vmux_api::PageIcon::favicon(IdenticonSpec::favicon_with_variant(
            &page.url,
            settings.appearance.project_icon,
        ));
        if page.icon != icon {
            page.icon = icon;
        }
    }
}

fn finish_deletes(
    mut pending: Query<(Entity, &PendingProjectOperation, &mut ProjectDeleteTask)>,
    pages: Query<(&vmux_ecs::PageMetadata, &ProjectPageRevision)>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, operation, mut task) in &mut pending {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        let current = pages.get(operation.webview).is_ok_and(|(page, revision)| {
            ProjectRoute::is_root(&page.url) && revision.value == operation.revision
        });
        if current {
            match result {
                Ok(()) => {
                    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
                    commands
                        .entity(operation.webview)
                        .insert(ProjectCatalogTask::spawn(
                            operation.root.clone(),
                            operation.revision,
                            wake,
                        ));
                }
                Err(error) => {
                    commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
                        operation.webview,
                        &UiProject {
                            view: ProjectView::Error(error),
                            ..Default::default()
                        },
                    ));
                }
            }
        }
        commands.entity(entity).despawn();
    }
}

type CanonicalProjects<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Id<Project>,
        &'static Name,
        &'static ProjectPath,
        &'static ProjectSource,
        Option<&'static ProjectBranch>,
        Has<vmux_ecs::ArchivedAt>,
        Option<&'static ProjectOwner>,
    ),
    With<Project>,
>;

struct ProjectEntityState {
    entity: Entity,
    name: String,
    path: PathBuf,
    source: String,
    branch: String,
    archived: bool,
    owner: Option<Entity>,
}

struct ProjectEntities {
    by_id: HashMap<Id<Project>, ProjectEntityState>,
}

impl ProjectEntities {
    fn collect(projects: &CanonicalProjects, commands: &mut Commands) -> Self {
        let mut by_id = HashMap::<Id<Project>, ProjectEntityState>::new();
        for (entity, id, name, path, source, branch, archived, owner) in projects {
            let next = ProjectEntityState {
                entity,
                name: name.as_str().to_string(),
                path: path.path().to_path_buf(),
                source: source.as_str().to_string(),
                branch: branch
                    .map(ProjectBranch::as_str)
                    .unwrap_or_default()
                    .to_string(),
                archived,
                owner: owner.map(ProjectOwner::entity),
            };
            if let Some(existing) = by_id.get(id) {
                if existing.entity.to_bits() <= entity.to_bits() {
                    commands.entity(entity).despawn();
                    continue;
                }
                commands.entity(existing.entity).despawn();
            }
            by_id.insert(id.clone(), next);
        }
        Self { by_id }
    }

    fn reconcile(&mut self, state: &mut UiProject, owner: Option<Entity>, commands: &mut Commands) {
        let ProjectView::Catalog(catalog) = &mut state.view else {
            return;
        };
        let mut seen = HashSet::new();
        for entry in &mut catalog.entries {
            let id = entry.id.clone();
            seen.insert(id.clone());
            if let Some(current) = self.by_id.get_mut(&id) {
                entry.archived = current.archived;
                let path = PathBuf::from(&entry.path);
                if current.name != entry.name {
                    commands
                        .entity(current.entity)
                        .insert(Name::new(entry.name.clone()));
                    current.name.clone_from(&entry.name);
                }
                if current.path != path
                    && let Ok(path) = ProjectPath::new(path)
                {
                    commands.entity(current.entity).insert(path.clone());
                    current.path = path.path().to_path_buf();
                }
                if current.source != entry.source {
                    commands
                        .entity(current.entity)
                        .insert(ProjectSource::new(entry.source.clone()));
                    current.source.clone_from(&entry.source);
                }
                if current.branch != entry.branch {
                    commands
                        .entity(current.entity)
                        .insert(ProjectBranch::new(entry.branch.clone()));
                    current.branch.clone_from(&entry.branch);
                }
                if let Some(owner) = owner
                    && current.owner != Some(owner)
                {
                    commands
                        .entity(current.entity)
                        .insert(ProjectOwner::new(owner));
                    current.owner = Some(owner);
                }
                continue;
            }

            let Ok(bundle) = ProjectBundle::new(
                id.clone(),
                entry.name.clone(),
                PathBuf::from(&entry.path),
                entry.source.clone(),
                entry.branch.clone(),
            ) else {
                continue;
            };
            let mut project = commands.spawn(bundle);
            if let Some(owner) = owner {
                project.insert(ProjectOwner::new(owner));
            }
            let entity = project.id();
            self.by_id.insert(
                id,
                ProjectEntityState {
                    entity,
                    name: entry.name.clone(),
                    path: PathBuf::from(&entry.path),
                    source: entry.source.clone(),
                    branch: entry.branch.clone(),
                    archived: false,
                    owner,
                },
            );
        }

        let stale = self
            .by_id
            .keys()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        for id in stale {
            if let Some(project) = self.by_id.remove(&id) {
                commands.entity(project.entity).despawn();
            }
        }
    }
}

fn finish_catalogs(
    mut tasks: Query<(
        Entity,
        &mut ProjectCatalogTask,
        &vmux_ecs::PageMetadata,
        &ProjectPageRevision,
    )>,
    catalog: Res<CatalogSnapshot>,
    projects: CanonicalProjects,
    owner: vmux_ecs::profile::Projects,
    states: Query<&vmux_ecs::UiState<UiProject>>,
    mut commands: Commands,
) {
    let mut projects = ProjectEntities::collect(&projects, &mut commands);
    let owner = owner.entity();
    for (entity, mut task, page, revision) in &mut tasks {
        let Some(state) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).remove::<ProjectCatalogTask>();
        if !ProjectRoute::is_root(&page.url) || revision.value != task.revision {
            continue;
        }
        let mut state = ProjectCatalog::with_session_counts(state, &catalog);
        projects.reconcile(&mut state, owner, &mut commands);
        if let Ok(current) = states.get(entity)
            && let Some(current) = current.current()
        {
            state.collection_view = current.collection_view;
        }
        commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
            entity, &state,
        ));
    }
}

fn sync_session_counts(
    catalog: Res<CatalogSnapshot>,
    pages: Query<(
        Entity,
        &vmux_ecs::PageMetadata,
        &vmux_ecs::UiState<UiProject>,
    )>,
    mut commands: Commands,
) {
    if !catalog.is_changed() {
        return;
    }
    for (entity, page, state) in &pages {
        if !ProjectRoute::is_root(&page.url) {
            continue;
        }
        let Some(current) = state.current() else {
            continue;
        };
        let next = ProjectCatalog::with_session_counts(current.clone(), &catalog);
        if next == *current {
            continue;
        }
        commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
            entity, &next,
        ));
    }
}

fn finish_imports(
    mut pending: Query<(Entity, &PendingProjectOperation, &mut ProjectImportTask)>,
    pages: Query<(&vmux_ecs::PageMetadata, &ProjectPageRevision)>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, operation, mut task) in &mut pending {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        let current = pages.get(operation.webview).is_ok_and(|(page, revision)| {
            ProjectRoute::is_root(&page.url) && revision.value == operation.revision
        });
        if current {
            match result {
                Ok(()) => {
                    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
                    commands
                        .entity(operation.webview)
                        .insert(ProjectCatalogTask::spawn(
                            operation.root.clone(),
                            operation.revision,
                            wake,
                        ));
                }
                Err(error) => {
                    commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
                        operation.webview,
                        &UiProject {
                            view: ProjectView::Error(error),
                            ..Default::default()
                        },
                    ));
                }
            }
        }
        commands.entity(entity).despawn();
    }
}

fn finish_repository_operations(
    mut completed: MessageReader<RepositoryProvisionCompleted>,
    pending: Query<&PendingProjectOperation>,
    pages: Query<(&vmux_ecs::PageMetadata, &ProjectPageRevision)>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for completed in completed.read() {
        let Ok(operation) = pending.get(completed.requester) else {
            continue;
        };
        let current = pages.get(operation.webview).is_ok_and(|(page, revision)| {
            ProjectRoute::is_root(&page.url) && revision.value == operation.revision
        });
        if current {
            match &completed.result {
                Ok(()) => {
                    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
                    commands
                        .entity(operation.webview)
                        .insert(ProjectCatalogTask::spawn(
                            operation.root.clone(),
                            operation.revision,
                            wake,
                        ));
                }
                Err(error) => {
                    commands.trigger(vmux_ecs::UiStateWrite::<UiProject>::from_event(
                        operation.webview,
                        &UiProject {
                            view: ProjectView::Error(error.clone()),
                            ..Default::default()
                        },
                    ));
                }
            }
        }
        commands.entity(completed.requester).despawn();
    }
}

struct ProjectCatalog;

impl ProjectCatalog {
    const MAX_DEPTH: usize = 6;

    fn read(root: &Path) -> UiProject {
        let mut entries = Vec::new();
        Self::walk(root, root, 0, &mut entries);
        entries.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.path.cmp(&right.path))
        });
        UiProject {
            view: ProjectView::Catalog(ProjectCatalogView {
                entries,
                operation_pending: false,
            }),
            ..Default::default()
        }
    }

    fn with_session_counts(mut state: UiProject, catalog: &CatalogSnapshot) -> UiProject {
        let ProjectView::Catalog(view) = &mut state.view else {
            return state;
        };
        let now = vmux_ecs::UnixMillis::now().0;
        for entry in &mut view.entries {
            let root = Path::new(&entry.path);
            entry.ongoing_sessions = 0;
            entry.stale_sessions = 0;
            for session in &catalog.sessions {
                if !Path::new(&session.cwd).starts_with(root) {
                    continue;
                }
                let projection = SessionSummaryProjection::new(session, &catalog.stages, now);
                if projection.stale {
                    entry.stale_sessions += 1;
                } else if !projection.terminal {
                    entry.ongoing_sessions += 1;
                }
            }
        }
        state
    }

    fn walk(root: &Path, directory: &Path, depth: usize, entries: &mut Vec<ProjectEntry>) {
        if depth > 0 && directory.join(".git").exists() {
            let Some(url) = ProjectRoute::from_path(root, directory) else {
                return;
            };
            let Some(Ok(path)) = ProjectRoute::resolve(&url, root) else {
                return;
            };
            let Some(name) = directory.file_name().and_then(|name| name.to_str()) else {
                return;
            };
            let source = directory
                .strip_prefix(root)
                .ok()
                .and_then(Path::parent)
                .map(|source| source.to_string_lossy().into_owned())
                .unwrap_or_default();
            let branch = RepoLabel::read(&path)
                .map(|label| label.branch)
                .unwrap_or_default();
            let id = Id::<Project>::new(url.clone());
            entries.push(ProjectEntry {
                id,
                name: name.to_string(),
                source,
                path: path.to_string_lossy().into_owned(),
                url,
                branch,
                base_ref: String::new(),
                is_worktree: false,
                uncommitted: 0,
                ahead: 0,
                ongoing_sessions: 0,
                stale_sessions: 0,
                archived: false,
            });
            return;
        }
        if depth >= Self::MAX_DEPTH {
            return;
        }
        let Ok(read) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in read.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            if entry.path().is_dir() {
                Self::walk(root, &entry.path(), depth + 1, entries);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_session::{SessionSummary, StageSummary};

    #[test]
    fn catalog_finds_repository_roots_without_descending_into_them() {
        let root = tempfile::tempdir().unwrap();
        let vmux = root.path().join("github.com/vmux-ai/vmux-cloud");
        let dashboard = root.path().join("github.com/mistralai/dashboard");
        std::fs::create_dir_all(vmux.join(".git")).unwrap();
        std::fs::create_dir_all(vmux.join("client/.git")).unwrap();
        std::fs::create_dir_all(dashboard.join(".git")).unwrap();

        let state = ProjectCatalog::read(root.path());

        let ProjectView::Catalog(catalog) = state.view else {
            panic!("expected project catalog");
        };
        assert_eq!(
            catalog
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["dashboard", "vmux-cloud"]
        );
        assert_eq!(
            catalog.entries[0].url,
            "vmux://projects/github.com/mistralai/dashboard"
        );
        assert!(!catalog.operation_pending);
    }

    #[test]
    fn catalog_discovers_projects_without_embedding_archive_state() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("github.com/vmux-ai/vmux");
        std::fs::create_dir_all(project.join(".git")).unwrap();

        let state = ProjectCatalog::read(root.path());

        let ProjectView::Catalog(catalog) = state.view else {
            panic!("expected project catalog");
        };
        assert_eq!(catalog.entries.len(), 1);
        assert!(!catalog.entries[0].archived);
        assert!(project.exists());
    }

    #[test]
    fn project_identity_matches_its_canonical_vmux_url() {
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("local/vmux");
        std::fs::create_dir_all(local.join(".git")).unwrap();

        let initial = ProjectCatalog::read(root.path());
        let ProjectView::Catalog(initial) = initial.view else {
            panic!("expected project catalog");
        };
        assert_eq!(initial.entries[0].id.as_str(), initial.entries[0].url);

        let linked = root.path().join("github.com/vmux-ai/vmux");
        std::fs::create_dir_all(linked.parent().unwrap()).unwrap();
        std::fs::rename(local, &linked).unwrap();

        let moved = ProjectCatalog::read(root.path());
        let ProjectView::Catalog(moved) = moved.view else {
            panic!("expected project catalog");
        };
        assert_eq!(moved.entries[0].id.as_str(), moved.entries[0].url);
        assert_eq!(
            moved.entries[0].url,
            "vmux://projects/github.com/vmux-ai/vmux"
        );
    }

    #[test]
    fn project_catalog_materializes_owned_entities_and_preserves_archive_state() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("github.com/vmux-ai/vmux");
        std::fs::create_dir_all(path.join(".git")).unwrap();
        let mut app = App::new();
        let owner = app.world_mut().spawn_empty().id();
        let state = ProjectCatalog::read(root.path());
        let state = app
            .world_mut()
            .run_system_once_with(
                move |In(mut state): In<UiProject>,
                      projects: CanonicalProjects,
                      mut commands: Commands| {
                    let mut entities = ProjectEntities::collect(&projects, &mut commands);
                    entities.reconcile(&mut state, Some(owner), &mut commands);
                    state
                },
                state,
            )
            .unwrap();

        let entity = {
            let world = app.world_mut();
            let mut projects = world.query_filtered::<
                (Entity, &Id<Project>, &ProjectPath, &ProjectOwner),
                With<Project>,
            >();
            let (entity, id, project_path, project_owner) = projects.single(world).unwrap();
            assert_eq!(id.as_str(), "vmux://projects/github.com/vmux-ai/vmux");
            assert_eq!(project_path.path(), path.canonicalize().unwrap());
            assert_eq!(project_owner.entity(), owner);
            entity
        };
        let ProjectView::Catalog(catalog) = state.view else {
            panic!("expected project catalog");
        };
        assert!(!catalog.entries[0].archived);

        app.world_mut()
            .entity_mut(entity)
            .insert(vmux_ecs::ArchivedAt::new(1));
        let state = ProjectCatalog::read(root.path());
        let state = app
            .world_mut()
            .run_system_once_with(
                move |In(mut state): In<UiProject>,
                      projects: CanonicalProjects,
                      mut commands: Commands| {
                    let mut entities = ProjectEntities::collect(&projects, &mut commands);
                    entities.reconcile(&mut state, Some(owner), &mut commands);
                    state
                },
                state,
            )
            .unwrap();
        let ProjectView::Catalog(catalog) = state.view else {
            panic!("expected project catalog");
        };
        assert!(catalog.entries[0].archived);
    }

    #[test]
    fn duplicate_project_ids_are_explicit() {
        let mut app = App::new();
        app.add_plugins(vmux_ecs::IdentityIndexPlugin::<Project>::default())
            .add_systems(Update, rebuild_project_index);
        let id = Id::<Project>::new("vmux://projects/duplicate");
        let path = PathBuf::from("/tmp/duplicate");
        app.world_mut()
            .spawn(ProjectBundle::new(id.clone(), "one", path.clone(), "", "").unwrap());
        app.world_mut()
            .spawn(ProjectBundle::new(id.clone(), "two", path, "", "").unwrap());

        app.update();

        assert_eq!(
            app.world()
                .resource::<vmux_ecs::IdentityIndex<Project>>()
                .entity(&id),
            Err(vmux_ecs::IdentityLookupError::Duplicate(id))
        );
    }

    #[test]
    fn project_refs_link_sessions_and_spaces_by_stable_identity() {
        let mut app = App::new();
        app.add_plugins((
            vmux_session::SessionDomainPlugin,
            vmux_ecs::IdentityIndexPlugin::<Project>::default(),
        ))
        .add_systems(
            Update,
            (rebuild_project_index, reconcile_project_refs).chain(),
        )
        .add_systems(PostUpdate, resolve_project_targets);
        let id = Id::<Project>::new("vmux://projects/github.com/vmux-ai/vmux");
        let root = PathBuf::from("/projects/vmux");
        let project = app
            .world_mut()
            .spawn(ProjectBundle::new(id.clone(), "vmux", root.clone(), "", "").unwrap())
            .id();
        let session = app
            .world_mut()
            .spawn((
                vmux_session::Session,
                Id::<vmux_session::Session>::new("session-1"),
                vmux_ecs::Cwd(root.join(".worktrees/vmx-233")),
            ))
            .id();
        let space = app
            .world_mut()
            .spawn(vmux_layout::space::Space::bundle(Id::new("space-1")))
            .insert(vmux_layout::space::EffectiveStartupDir(Some(
                root.join("client"),
            )))
            .id();

        app.update();

        assert_eq!(app.world().get::<ProjectRef>(session).unwrap().id(), &id);
        assert_eq!(app.world().get::<ProjectRef>(space).unwrap().id(), &id);
        assert_eq!(
            app.world().get::<ProjectTarget>(session).unwrap().entity(),
            project
        );
        assert_eq!(
            app.world().get::<ProjectTarget>(space).unwrap().entity(),
            project
        );
    }

    #[test]
    fn catalog_counts_current_and_stale_sessions_separately() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("github.com/vmux-ai/vmux");
        std::fs::create_dir_all(&project).unwrap();
        let now = vmux_ecs::UnixMillis::now().0;
        let state = UiProject {
            view: ProjectView::Catalog(ProjectCatalogView {
                entries: vec![ProjectEntry {
                    id: Id::new("vmux://projects/test"),
                    name: "test".into(),
                    source: String::new(),
                    path: project.display().to_string(),
                    url: "vmux://projects/test".into(),
                    branch: String::new(),
                    base_ref: String::new(),
                    is_worktree: false,
                    uncommitted: 0,
                    ahead: 0,
                    ongoing_sessions: 0,
                    stale_sessions: 0,
                    archived: false,
                }],
                operation_pending: false,
            }),
            ..Default::default()
        };
        let catalog = CatalogSnapshot {
            stages: vec![
                StageSummary {
                    id: "active".into(),
                    terminal: false,
                    ..Default::default()
                },
                StageSummary {
                    id: "done".into(),
                    terminal: true,
                    ..Default::default()
                },
            ],
            sessions: vec![
                SessionSummary {
                    cwd: project.display().to_string(),
                    stage: "active".into(),
                    runtime: "idle".into(),
                    last_activated_at: now,
                    ..Default::default()
                },
                SessionSummary {
                    cwd: project.display().to_string(),
                    stage: "active".into(),
                    runtime: "idle".into(),
                    last_activated_at: now - (SessionSummaryProjection::STALE_AFTER as i64 * 1_000),
                    ..Default::default()
                },
                SessionSummary {
                    cwd: project.display().to_string(),
                    stage: "done".into(),
                    last_activated_at: now,
                    ..Default::default()
                },
            ],
        };

        let ProjectView::Catalog(view) = ProjectCatalog::with_session_counts(state, &catalog).view
        else {
            panic!("expected project catalog");
        };
        assert_eq!(view.entries[0].ongoing_sessions, 1);
        assert_eq!(view.entries[0].stale_sessions, 1);
    }

    #[test]
    fn catalog_opens_an_imported_repository_without_moving_it() {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        std::fs::create_dir(external.path().join(".git")).unwrap();
        let imports = root.path().join("imported");
        std::fs::create_dir(&imports).unwrap();
        std::os::unix::fs::symlink(external.path(), imports.join("external")).unwrap();

        let state = ProjectCatalog::read(root.path());
        let ProjectView::Catalog(catalog) = state.view else {
            panic!("expected project catalog");
        };
        let entry = catalog.entries.first().unwrap();
        let resolved = ProjectRoute::resolve(&entry.url, root.path())
            .unwrap()
            .unwrap();

        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(resolved, external.path().canonicalize().unwrap());
        assert_eq!(
            entry.path,
            external
                .path()
                .canonicalize()
                .unwrap()
                .display()
                .to_string()
        );
    }

    #[test]
    fn catalog_excludes_unmanaged_external_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        std::fs::create_dir(external.path().join(".git")).unwrap();
        std::os::unix::fs::symlink(external.path(), root.path().join("external")).unwrap();

        let state = ProjectCatalog::read(root.path());
        let ProjectView::Catalog(catalog) = state.view else {
            panic!("expected project catalog");
        };

        assert!(catalog.entries.is_empty());
    }

    #[test]
    fn deleting_imported_project_removes_only_the_managed_link() {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        std::fs::create_dir(external.path().join(".git")).unwrap();
        let imports = root.path().join("imported");
        std::fs::create_dir(&imports).unwrap();
        let link = imports.join("external");
        std::os::unix::fs::symlink(external.path(), &link).unwrap();

        ProjectDeletion::resolve(root.path(), "vmux://projects/imported/external")
            .unwrap()
            .remove()
            .unwrap();

        assert!(std::fs::symlink_metadata(link).is_err());
        assert!(external.path().is_dir());
    }

    #[test]
    fn deleting_managed_project_removes_its_directory() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("github.com/vmux-ai/vmux");
        std::fs::create_dir_all(project.join(".git")).unwrap();

        ProjectDeletion::resolve(root.path(), "vmux://projects/github.com/vmux-ai/vmux")
            .unwrap()
            .remove()
            .unwrap();

        assert!(!project.exists());
    }

    #[test]
    fn github_repository_accepts_shorthand_and_builds_the_managed_path() {
        let repository = GitHubRepository::parse("vmux-ai/vmux").unwrap();
        let root = Path::new("/projects");

        assert_eq!(repository.owner, "vmux-ai");
        assert_eq!(repository.name, "vmux");
        assert_eq!(repository.remote, "https://github.com/vmux-ai/vmux.git");
        assert_eq!(
            root.join("github.com")
                .join(&repository.owner)
                .join(&repository.name),
            Path::new("/projects/github.com/vmux-ai/vmux")
        );
    }
}
