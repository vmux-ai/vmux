use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_git::{
    RepositoryProvisionCompleted, RepositoryProvisionOperation, RepositoryProvisionRequest,
};
use vmux_session::host::ScopeProjectionRequest;

use crate::project::{
    ProjectCatalogView, ProjectCloneRequest, ProjectCreateRequest, ProjectEntry,
    ProjectImportRequest, ProjectRoute, ProjectUiState, ProjectView,
};

pub(super) struct ProjectPagePlugin;

impl Plugin for ProjectPagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_ecs::UiStatePlugin::<ProjectUiState>::default())
            .add_plugins(UiEventPlugin::<(
                ProjectCreateRequest,
                ProjectImportRequest,
                ProjectCloneRequest,
            )>::default())
            .add_observer(create)
            .add_observer(import)
            .add_observer(clone)
            .add_systems(Update, (queue, finish_catalogs, finish_imports).chain())
            .add_systems(PostUpdate, finish_repository_operations);
    }
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
            commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                webview,
                &ProjectUiState {
                    view: ProjectView::Error(error),
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

fn queue(
    pages: Query<(Entity, &vmux_ecs::PageMetadata), Changed<vmux_ecs::PageMetadata>>,
    states: Query<&vmux_ecs::UiState<ProjectUiState>>,
    mut revisions: Query<&mut ProjectPageRevision>,
    projects: vmux_ecs::profile::Projects,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, page) in &pages {
        if !ProjectRoute::is_page(&page.url) {
            let Ok(mut revision) = revisions.get_mut(entity) else {
                continue;
            };
            if revision.advance_to(&page.url).is_none() {
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
                let Some(revision) = revision.advance_to(&page.url) else {
                    continue;
                };
                revision
            }
            Err(_) => {
                commands
                    .entity(entity)
                    .insert(ProjectPageRevision::new(&page.url));
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
                    },
                ));
                continue;
            }
        };
        if !ProjectRoute::is_root(&page.url) {
            let (state, cwd) = match ProjectRoute::resolve(&page.url, &root) {
                Some(Ok(path)) => (
                    ProjectUiState {
                        view: ProjectView::Detail,
                    },
                    Some(path),
                ),
                Some(Err(error)) => (
                    ProjectUiState {
                        view: ProjectView::Error(error),
                    },
                    None,
                ),
                None => continue,
            };
            commands.entity(entity).remove::<ProjectCatalogTask>();
            commands.trigger(ScopeProjectionRequest {
                webview: entity,
                cwd,
            });
            commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                entity, &state,
            ));
            continue;
        }
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
                },
            ));
        }
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        commands
            .entity(entity)
            .insert(ProjectCatalogTask::spawn(root, revision, wake));
    }
}

fn finish_catalogs(
    mut tasks: Query<(
        Entity,
        &mut ProjectCatalogTask,
        &vmux_ecs::PageMetadata,
        &ProjectPageRevision,
    )>,
    mut commands: Commands,
) {
    for (entity, mut task, page, revision) in &mut tasks {
        let Some(state) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).remove::<ProjectCatalogTask>();
        if !ProjectRoute::is_root(&page.url) || revision.value != task.revision {
            continue;
        }
        commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
            entity, &state,
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

    fn read(root: &Path) -> ProjectUiState {
        let mut entries = Vec::new();
        Self::walk(root, root, 0, &mut entries);
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
        }
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
            entries.push(ProjectEntry {
                name: name.to_string(),
                path: path.to_string_lossy().into_owned(),
                url,
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
