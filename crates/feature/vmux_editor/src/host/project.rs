use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use serde::{Deserialize, Serialize};
use vmux_api::avatar::IdenticonSpec;
use vmux_ecs::page::PrewarmPage;
use vmux_git::worktree::RepoLabel;
use vmux_git::{
    RepositoryProvisionCompleted, RepositoryProvisionOperation, RepositoryProvisionRequest,
};
use vmux_session::host::ScopeProjectionRequest;
use vmux_session::{CatalogSnapshot, SessionSummaryProjection};

use crate::project::{
    ProjectArchiveRequest, ProjectCatalogView, ProjectCloneRequest, ProjectCollectionViewRequest,
    ProjectCreateRequest, ProjectDeleteRequest, ProjectDetailView, ProjectEntry, ProjectId,
    ProjectImportRequest, ProjectRoute, ProjectUiState, ProjectView,
};

pub(super) struct ProjectPagePlugin;

impl Plugin for ProjectPagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_ecs::UiStatePlugin::<ProjectUiState>::default())
            .add_message::<vmux_layout::NewTabRequest>()
            .add_message::<vmux_layout::stack::OpenRequest>()
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
            .add_observer(open_context)
            .add_systems(PreStartup, prewarm)
            .add_systems(
                Update,
                (
                    queue,
                    sync_icons,
                    finish_catalogs,
                    finish_imports,
                    finish_deletes,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                (finish_repository_operations, sync_session_counts).chain(),
            );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct ProjectSources<'w, 's> {
    info: Query<'w, 's, &'static mut vmux_git::RepoInfoCache>,
    projects: vmux_ecs::profile::Projects<'w, 's>,
    states: Query<'w, 's, &'static vmux_ecs::UiState<ProjectUiState>>,
}

fn open_context(
    request: On<vmux_ecs::workspace::ProjectContextOpenRequest>,
    mut source: ProjectSources,
    active: vmux_layout::stack::ActiveTabParam,
    workspace: vmux_layout::page_context::Context,
    space: vmux_layout::space::FocusedSpace,
    mut opens: MessageWriter<vmux_layout::stack::OpenRequest>,
    mut tabs_open: MessageWriter<vmux_layout::NewTabRequest>,
) {
    let mut directory = space.startup_dir().map(PathBuf::from);
    let mut project = None;
    if let Some(tab) = active.get() {
        directory = workspace.directory(tab).or(directory);
        project = workspace.project_directory(tab);
    }
    if project.is_none()
        && let Some(directory) = directory.as_ref()
        && let Ok(mut repository_info) = source.info.single_mut()
        && let Some(repository) = repository_info.cached(directory)
    {
        project = Some(repository.project_root().to_path_buf());
    }
    let selected = project.as_ref().or(directory.as_ref());
    let mut url = ProjectRoute::ROOT.to_string();
    if let (Ok(root), Some(directory)) = (source.projects.path(), selected) {
        for state in &source.states {
            let Some(state) = state.current() else {
                continue;
            };
            let ProjectView::Catalog(catalog) = &state.view else {
                continue;
            };
            if let Some(entry) = catalog
                .entries
                .iter()
                .filter(|entry| directory.starts_with(&entry.path))
                .max_by_key(|entry| entry.path.len())
            {
                url.clone_from(&entry.url);
                break;
            }
        }
        if ProjectRoute::is_root(&url)
            && let Some(current) = ProjectCatalog::url_for_directory(root, directory)
        {
            url = current;
        }
    }
    if request.open == Some(vmux_api::open_target::OpenTarget::InNewTab) {
        tabs_open.write(vmux_layout::NewTabRequest {
            url,
            pending_prompt: None,
        });
        return;
    }
    opens.write(vmux_layout::stack::OpenRequest {
        url: Some(url),
        policy: if request.force_new {
            vmux_layout::stack::OpenPolicy::NewStack
        } else {
            vmux_layout::stack::OpenPolicy::FocusReusable
        },
    });
}

fn prewarm(mut commands: Commands) {
    commands.spawn(PrewarmPage::host(
        "projects",
        ProjectRoute::ROOT,
        "Projects",
        1,
    ));
}

#[derive(Component)]
struct ProjectCatalogTask {
    revision: u64,
    task: Task<ProjectUiState>,
}

impl ProjectCatalogTask {
    fn spawn(
        root: PathBuf,
        revision: u64,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> Self {
        let task = IoTaskPool::get().spawn(async move {
            let state = ProjectCatalog::read(&root, &ProjectArchive::load(&root));
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

type ProjectPageRows<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut vmux_ecs::PageMetadata),
    (
        Changed<vmux_ecs::PageMetadata>,
        Without<vmux_ecs::page::PrewarmedPage>,
    ),
>;

#[derive(SystemParam)]
struct ProjectPageQueries<'w, 's> {
    pages: ProjectPageRows<'w, 's>,
    states: Query<'w, 's, &'static vmux_ecs::UiState<ProjectUiState>>,
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
    states: Query<'w, 's, &'static vmux_ecs::UiState<ProjectUiState>>,
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

    fn pending_state(&self, webview: Entity) -> ProjectUiState {
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

#[derive(Default, Serialize, Deserialize)]
struct ProjectArchive {
    projects: BTreeSet<String>,
}

impl ProjectArchive {
    fn path(root: &Path) -> PathBuf {
        root.join(".archive.ron")
    }

    fn load(root: &Path) -> Self {
        let Ok(source) = std::fs::read_to_string(Self::path(root)) else {
            return Self::default();
        };
        ron::from_str(&source).unwrap_or_default()
    }

    fn contains(&self, url: &str) -> bool {
        self.projects.contains(url)
    }

    fn set(&mut self, url: &str, archived: bool) {
        if archived {
            self.projects.insert(url.to_string());
        } else {
            self.projects.remove(url);
        }
    }

    fn save(&self, root: &Path) -> Result<(), String> {
        let source = ron::ser::to_string(self).map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(Self::path(root), source.as_bytes())
            .map_err(|error| error.to_string())
    }
}

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
        if input.starts_with("http://") {
            return Err(vmux_ui::i18n::translate("project-clone-invalid"));
        }
        let path = if let Some(path) = input.strip_prefix("git@github.com:") {
            path
        } else if let Some(path) = input.strip_prefix("https://github.com/") {
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
        let remote = format!("https://github.com/{owner}/{name}.git");
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
            commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                webview,
                &ProjectUiState {
                    view: ProjectView::Error(error),
                    ..Default::default()
                },
            ));
            return;
        }
    };
    let state = pages.pending_state(webview);
    commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
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
    commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
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
            commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                webview,
                &ProjectUiState {
                    view: ProjectView::Error(error),
                    ..Default::default()
                },
            ));
            return;
        }
    };
    let state = pages.pending_state(webview);
    commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
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
    states: Query<(
        Entity,
        &vmux_ecs::PageMetadata,
        &vmux_ecs::UiState<ProjectUiState>,
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
    let mut archive = ProjectArchive::load(root);
    archive.set(&request.url, request.archived);
    if let Err(error) = archive.save(root) {
        commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
            trigger.event().webview,
            &ProjectUiState {
                view: ProjectView::Error(error),
                ..Default::default()
            },
        ));
        return;
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
        commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
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
            commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                webview,
                &ProjectUiState {
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
    commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
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
    states: Query<&vmux_ecs::UiState<ProjectUiState>>,
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
    commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
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
                commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                    entity,
                    &ProjectUiState {
                        view: ProjectView::Error(error),
                        collection_view,
                    },
                ));
                continue;
            }
        };
        if !ProjectRoute::is_root(&url) {
            let choices = if let Ok(current) = states.get(entity)
                && let Some(current) = current.current()
            {
                current.choices()
            } else {
                Vec::new()
            };
            let (state, cwd) = match ProjectRoute::resolve(&url, &root) {
                Some(Ok(path)) => {
                    let name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.to_string_lossy().into_owned());
                    page.title.clone_from(&name);
                    let id = ProjectId(url.clone());
                    (
                        ProjectUiState {
                            view: ProjectView::Detail(ProjectDetailView {
                                id: id.clone(),
                                name,
                                path: path.to_string_lossy().into_owned(),
                                choices_pending: choices.is_empty(),
                                choices,
                            }),
                            collection_view,
                        },
                        Some(path),
                    )
                }
                Some(Err(error)) => (
                    ProjectUiState {
                        view: ProjectView::Error(error),
                        collection_view,
                    },
                    None,
                ),
                None => continue,
            };
            let seed = match &state.view {
                ProjectView::Detail(detail) => detail.path.as_str(),
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
            commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                entity, &state,
            ));
            if matches!(state.view, ProjectView::Detail(_)) {
                let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
                commands
                    .entity(entity)
                    .insert(ProjectCatalogTask::spawn(root, revision, wake));
            }
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
            commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                entity,
                &ProjectUiState {
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
                    commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                        operation.webview,
                        &ProjectUiState {
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

fn finish_catalogs(
    mut tasks: Query<(
        Entity,
        &mut ProjectCatalogTask,
        &vmux_ecs::PageMetadata,
        &ProjectPageRevision,
    )>,
    catalog: Res<CatalogSnapshot>,
    states: Query<&vmux_ecs::UiState<ProjectUiState>>,
    mut commands: Commands,
) {
    for (entity, mut task, page, revision) in &mut tasks {
        if !ProjectRoute::is_page(&page.url) || revision.value != task.revision {
            commands.entity(entity).remove::<ProjectCatalogTask>();
            continue;
        }
        if !ProjectRoute::is_root(&page.url) {
            let Ok(current) = states.get(entity) else {
                continue;
            };
            let Some(current) = current.current() else {
                continue;
            };
            if !matches!(&current.view, ProjectView::Detail(detail) if detail.id.0 == page.url) {
                continue;
            }
        }
        let Some(state) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).remove::<ProjectCatalogTask>();
        let mut state = if ProjectRoute::is_root(&page.url) {
            ProjectCatalog::with_session_counts(state, &catalog)
        } else {
            let Ok(current) = states.get(entity) else {
                continue;
            };
            let Some(mut current) = current.current().cloned() else {
                continue;
            };
            let ProjectView::Detail(detail) = &mut current.view else {
                continue;
            };
            detail.choices = state.choices();
            detail.choices_pending = false;
            current
        };
        if let Ok(current) = states.get(entity)
            && let Some(current) = current.current()
        {
            state.collection_view = current.collection_view;
        }
        commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
            entity, &state,
        ));
    }
}

fn sync_session_counts(
    catalog: Res<CatalogSnapshot>,
    pages: Query<(
        Entity,
        &vmux_ecs::PageMetadata,
        &vmux_ecs::UiState<ProjectUiState>,
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
        commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
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
                    commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                        operation.webview,
                        &ProjectUiState {
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
                    commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                        operation.webview,
                        &ProjectUiState {
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

    fn url_for_directory(root: &Path, directory: &Path) -> Option<String> {
        let root = vmux_path::PathIdentity::resolve(root).into_path_buf();
        let directory = vmux_path::PathIdentity::resolve(directory).into_path_buf();
        for path in directory.ancestors() {
            if path == root.as_path() {
                break;
            }
            if !path.join(".git").is_dir() {
                continue;
            }
            if let Some(url) = ProjectRoute::from_path(&root, path)
                && ProjectRoute::resolve(&url, &root).is_some_and(|result| result.is_ok())
            {
                return Some(url);
            }
            let entries = std::fs::read_dir(root.join("imported")).ok()?;
            for entry in entries.flatten() {
                if entry.path().canonicalize().ok().as_deref() != Some(path) {
                    continue;
                }
                let url = ProjectRoute::from_path(&root, &entry.path())?;
                if ProjectRoute::resolve(&url, &root).is_some_and(|result| result.is_ok()) {
                    return Some(url);
                }
            }
            return None;
        }
        None
    }

    fn read(root: &Path, archive: &ProjectArchive) -> ProjectUiState {
        let mut entries = Vec::new();
        Self::walk(root, root, 0, archive, &mut entries);
        entries.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.path.cmp(&right.path))
        });
        ProjectUiState {
            view: ProjectView::Catalog(ProjectCatalogView {
                entries,
                operation_pending: false,
            }),
            ..Default::default()
        }
    }

    fn with_session_counts(mut state: ProjectUiState, catalog: &CatalogSnapshot) -> ProjectUiState {
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
                if projection.archived {
                    continue;
                }
                if projection.stale {
                    entry.stale_sessions += 1;
                } else if !projection.terminal {
                    entry.ongoing_sessions += 1;
                }
            }
        }
        state
    }

    fn walk(
        root: &Path,
        directory: &Path,
        depth: usize,
        archive: &ProjectArchive,
        entries: &mut Vec<ProjectEntry>,
    ) {
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
            let archived = archive.contains(&url);
            let id = ProjectId(url.clone());
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
                archived,
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
                Self::walk(root, &entry.path(), depth + 1, archive, entries);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_session::{SessionSummary, StageSummary};

    #[test]
    fn contextual_project_open_uses_the_owning_repository_and_preserves_new_stack_policy() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("repo");
        let cwd = project.join(".worktrees/task/src");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::create_dir(project.join(".git")).unwrap();
        std::fs::write(
            project.join(".worktrees/task/.git"),
            "gitdir: ../../../.git/worktrees/task\n",
        )
        .unwrap();
        let mut page = ProjectPage::at(root.path(), "vmux://projects/");
        page.app.world_mut().spawn((
            vmux_layout::tab::Tab {
                name: "Repo".to_string(),
                startup_dir: Some(cwd.to_string_lossy().into_owned()),
            },
            vmux_ecs::Active,
        ));

        page.app
            .world_mut()
            .trigger(vmux_ecs::workspace::ProjectContextOpenRequest {
                force_new: true,
                open: None,
            });

        let requests = page
            .app
            .world_mut()
            .resource_mut::<Messages<vmux_layout::stack::OpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.as_deref(), Some("vmux://projects/repo"));
        assert_eq!(requests[0].policy, vmux_layout::stack::OpenPolicy::NewStack);
    }

    #[test]
    fn contextual_project_urls_keep_imported_aliases_and_ignore_unrelated_directories() {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        std::fs::create_dir(external.path().join(".git")).unwrap();
        std::fs::create_dir(external.path().join("src")).unwrap();
        std::fs::create_dir(root.path().join("imported")).unwrap();
        std::os::unix::fs::symlink(external.path(), root.path().join("imported/external")).unwrap();
        assert_eq!(
            ProjectCatalog::url_for_directory(root.path(), &external.path().join("src")),
            Some("vmux://projects/imported/external".to_string())
        );
        assert_eq!(
            ProjectCatalog::url_for_directory(root.path(), root.path()),
            None
        );
        let unrelated = tempfile::tempdir().unwrap();
        assert_eq!(
            ProjectCatalog::url_for_directory(root.path(), unrelated.path()),
            None
        );
    }

    fn deliver_frame(event: On<bevy_cef::prelude::BinHostEmitEvent>) {
        event.mark_delivered();
    }

    struct ProjectPage {
        app: App,
        view: Entity,
    }

    impl ProjectPage {
        fn at(root: &Path, url: &str) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ProjectPagePlugin))
                .init_resource::<vmux_setting::AppSettings>()
                .init_resource::<CatalogSnapshot>()
                .add_message::<RepositoryProvisionCompleted>()
                .insert_non_send(bevy_cef::prelude::Browsers::default())
                .add_observer(deliver_frame);
            app.world_mut().spawn(vmux_ecs::profile::ProjectsRoot(
                vmux_ecs::profile::ProjectsDirectory::ensure_at(root.to_path_buf()).unwrap(),
            ));
            let view = app
                .world_mut()
                .spawn((
                    vmux_ecs::PageMetadata {
                        url: url.to_string(),
                        ..Default::default()
                    },
                    bevy_cef::prelude::BinIpcSurfaceGeneration(1),
                    bevy_cef::prelude::BinIpcNavigationRevision(1),
                    bevy_cef::prelude::BinIpcEpoch(1),
                ))
                .id();
            app.world_mut()
                .non_send_mut::<bevy_cef::prelude::Browsers>()
                .set_externally_hosted(view);
            app.world_mut().spawn(vmux_api::UiEventPermissions {
                url: ProjectRoute::ROOT,
                owns_subtree: true,
                permissions: &["ProjectUiState"],
            });
            Self { app, view }
        }

        fn wait_for_choices(&mut self) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                self.app.update();
                let ready = self
                    .app
                    .world()
                    .get::<vmux_ecs::UiState<ProjectUiState>>(self.view)
                    .and_then(vmux_ecs::UiState::current)
                    .is_some_and(|state| match &state.view {
                        ProjectView::Detail(detail) => !detail.choices_pending,
                        _ => false,
                    });
                if ready {
                    return;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
        }
    }

    #[test]
    fn project_detail_loads_switch_choices_without_replacing_its_target() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("one/.git")).unwrap();
        std::fs::create_dir_all(root.path().join("two/.git")).unwrap();
        let mut page = ProjectPage::at(root.path(), "vmux://projects/one");
        page.wait_for_choices();
        let state = page
            .app
            .world()
            .get::<vmux_ecs::UiState<ProjectUiState>>(page.view)
            .and_then(vmux_ecs::UiState::current)
            .unwrap();
        let ProjectView::Detail(detail) = &state.view else {
            panic!("catalog completion must retain the selected project");
        };
        assert_eq!(detail.id.0, "vmux://projects/one");
        assert_eq!(
            detail
                .choices
                .iter()
                .map(|entry| entry.url.as_str())
                .collect::<Vec<_>>(),
            ["vmux://projects/one", "vmux://projects/two"]
        );
    }

    #[test]
    fn catalog_completion_before_the_initial_detail_state_is_retained() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("one/.git")).unwrap();
        std::fs::create_dir_all(root.path().join("two/.git")).unwrap();
        let mut page = ProjectPage::at(root.path(), "vmux://projects/one");
        page.app
            .world_mut()
            .get_mut::<bevy_cef::prelude::BinIpcEpoch>(page.view)
            .unwrap()
            .0 = 0;
        page.app.update();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let task = page
                .app
                .world()
                .get::<ProjectCatalogTask>(page.view)
                .expect("catalog result must survive the initial state publication");
            if task.task.is_finished() {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        page.app
            .world_mut()
            .get_mut::<bevy_cef::prelude::BinIpcEpoch>(page.view)
            .unwrap()
            .0 = 1;
        page.app.world_mut().trigger(UiInput {
            webview: page.view,
            payload: vmux_api::PageReady,
        });
        page.wait_for_choices();

        let state = page
            .app
            .world()
            .get::<vmux_ecs::UiState<ProjectUiState>>(page.view)
            .and_then(vmux_ecs::UiState::current)
            .unwrap();
        let ProjectView::Detail(detail) = &state.view else {
            panic!("catalog completion must retain the selected project");
        };
        assert_eq!(detail.id.0, "vmux://projects/one");
        assert_eq!(
            detail
                .choices
                .iter()
                .map(|entry| entry.url.as_str())
                .collect::<Vec<_>>(),
            ["vmux://projects/one", "vmux://projects/two"]
        );
    }

    #[test]
    fn project_detail_resolves_identity_without_opening_an_editor() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("empty");
        std::fs::create_dir(&project).unwrap();
        let mut page = ProjectPage::at(root.path(), "vmux://projects/empty");
        page.app.update();
        let state = page
            .app
            .world()
            .get::<vmux_ecs::UiState<ProjectUiState>>(page.view)
            .and_then(vmux_ecs::UiState::current)
            .expect("project detail must be published");
        let ProjectView::Detail(detail) = &state.view else {
            panic!("expected project detail");
        };
        assert_eq!(detail.id.0, "vmux://projects/empty");
        assert_eq!(detail.name, "empty");
        assert_eq!(
            detail.path,
            project.canonicalize().unwrap().display().to_string()
        );
        assert!(
            page.app
                .world()
                .get::<super::super::editor::FileView>(page.view)
                .is_none()
        );
    }

    #[test]
    fn catalog_finds_repository_roots_without_descending_into_them() {
        let root = tempfile::tempdir().unwrap();
        let vmux = root.path().join("github.com/vmux-ai/vmux-cloud");
        let dashboard = root.path().join("github.com/mistralai/dashboard");
        std::fs::create_dir_all(vmux.join(".git")).unwrap();
        std::fs::create_dir_all(vmux.join("client/.git")).unwrap();
        std::fs::create_dir_all(dashboard.join(".git")).unwrap();

        let state = ProjectCatalog::read(root.path(), &ProjectArchive::default());

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
    fn catalog_marks_archived_projects_without_removing_their_files() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("github.com/vmux-ai/vmux");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        let url = "vmux://projects/github.com/vmux-ai/vmux";
        let mut archive = ProjectArchive::default();
        archive.set(url, true);

        let state = ProjectCatalog::read(root.path(), &archive);

        let ProjectView::Catalog(catalog) = state.view else {
            panic!("expected project catalog");
        };
        assert_eq!(catalog.entries.len(), 1);
        assert!(catalog.entries[0].archived);
        assert!(project.exists());
    }

    #[test]
    fn project_identity_matches_its_canonical_vmux_url() {
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("local/vmux");
        std::fs::create_dir_all(local.join(".git")).unwrap();

        let initial = ProjectCatalog::read(root.path(), &ProjectArchive::default());
        let ProjectView::Catalog(initial) = initial.view else {
            panic!("expected project catalog");
        };
        assert_eq!(initial.entries[0].id.0, initial.entries[0].url);

        let linked = root.path().join("github.com/vmux-ai/vmux");
        std::fs::create_dir_all(linked.parent().unwrap()).unwrap();
        std::fs::rename(local, &linked).unwrap();

        let moved = ProjectCatalog::read(root.path(), &ProjectArchive::default());
        let ProjectView::Catalog(moved) = moved.view else {
            panic!("expected project catalog");
        };
        assert_eq!(moved.entries[0].id.0, moved.entries[0].url);
        assert_eq!(
            moved.entries[0].url,
            "vmux://projects/github.com/vmux-ai/vmux"
        );
    }

    #[test]
    fn catalog_counts_current_and_stale_sessions_separately() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("github.com/vmux-ai/vmux");
        std::fs::create_dir_all(&project).unwrap();
        let now = vmux_ecs::UnixMillis::now().0;
        let state = ProjectUiState {
            view: ProjectView::Catalog(ProjectCatalogView {
                entries: vec![ProjectEntry {
                    path: project.display().to_string(),
                    ..Default::default()
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

        let state = ProjectCatalog::read(root.path(), &ProjectArchive::default());
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

        let state = ProjectCatalog::read(root.path(), &ProjectArchive::default());
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

    #[test]
    fn github_repository_normalizes_supported_urls_to_https() {
        for input in [
            "https://github.com/vmux-ai/vmux.git",
            "git@github.com:vmux-ai/vmux.git",
        ] {
            let repository = GitHubRepository::parse(input).unwrap();
            assert_eq!(repository.remote, "https://github.com/vmux-ai/vmux.git");
        }
    }

    #[test]
    fn github_repository_rejects_insecure_http() {
        assert!(GitHubRepository::parse("http://github.com/vmux-ai/vmux.git").is_err());
    }
}
