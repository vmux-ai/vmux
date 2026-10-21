use std::path::{Path, PathBuf};
use std::process::Command;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{UiEventPlugin, UiInput};

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
            .add_systems(Update, (queue, finish, finish_operations).chain());
    }
}

#[derive(Component)]
struct ProjectCatalogTask(Task<ProjectUiState>);

#[derive(Component)]
struct PendingProjectOperation {
    webview: Entity,
    root: PathBuf,
    task: Task<Result<(), String>>,
}

enum ProjectOperation {
    Create(String),
    Import,
    Clone(String),
}

impl PendingProjectOperation {
    fn spawn(
        webview: Entity,
        root: PathBuf,
        operation: ProjectOperation,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> Self {
        let operation_root = root.clone();
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.run(&operation_root).await;
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        Self {
            webview,
            root,
            task,
        }
    }
}

impl ProjectOperation {
    async fn run(self, root: &Path) -> Result<(), String> {
        match self {
            Self::Create(name) => ProjectName::parse(&name)?.create(root),
            Self::Import => ProjectImport::select(root).await,
            Self::Clone(repository) => GitHubRepository::parse(&repository)?.clone_into(root),
        }
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

    fn create(self, root: &Path) -> Result<(), String> {
        let destination = root.join(&self.0);
        if destination.exists() {
            return Err(vmux_ui::i18n::translate("project-already-exists"));
        }
        std::fs::create_dir(&destination).map_err(|error| error.to_string())?;
        let output = Command::new("git")
            .args(["init", "--quiet", "--initial-branch", "main"])
            .current_dir(&destination)
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
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

    fn clone_into(self, root: &Path) -> Result<(), String> {
        let destination = root.join("github.com").join(&self.owner).join(&self.name);
        if destination.exists() {
            return Err(vmux_ui::i18n::translate("project-already-exists"));
        }
        let Some(parent) = destination.parent() else {
            return Err(vmux_ui::i18n::translate("project-clone-invalid"));
        };
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let output = Command::new("git")
            .arg("clone")
            .arg("--")
            .arg(&self.remote)
            .arg(&destination)
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn create(
    trigger: On<UiInput<ProjectCreateRequest>>,
    projects: vmux_ecs::profile::Projects,
    pending: Query<&PendingProjectOperation>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if pending.iter().any(|pending| pending.webview == webview) {
        return;
    }
    let Ok(root) = projects.path() else {
        return;
    };
    let state = ProjectCatalog::read(root).pending();
    commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
        webview, &state,
    ));
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    commands.spawn(PendingProjectOperation::spawn(
        webview,
        root.to_path_buf(),
        ProjectOperation::Create(trigger.event().payload.name.clone()),
        wake,
    ));
}

fn import(
    trigger: On<UiInput<ProjectImportRequest>>,
    projects: vmux_ecs::profile::Projects,
    pending: Query<&PendingProjectOperation>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if pending.iter().any(|pending| pending.webview == webview) {
        return;
    }
    let Ok(root) = projects.path() else {
        return;
    };
    let state = ProjectCatalog::read(root).pending();
    commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
        webview, &state,
    ));
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    commands.spawn(PendingProjectOperation::spawn(
        webview,
        root.to_path_buf(),
        ProjectOperation::Import,
        wake,
    ));
}

fn clone(
    trigger: On<UiInput<ProjectCloneRequest>>,
    projects: vmux_ecs::profile::Projects,
    pending: Query<&PendingProjectOperation>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if pending.iter().any(|pending| pending.webview == webview) {
        return;
    }
    let Ok(root) = projects.path() else {
        return;
    };
    let state = ProjectCatalog::read(root).pending();
    commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
        webview, &state,
    ));
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    commands.spawn(PendingProjectOperation::spawn(
        webview,
        root.to_path_buf(),
        ProjectOperation::Clone(trigger.event().payload.repository.clone()),
        wake,
    ));
}

fn queue(
    pages: Query<(Entity, &vmux_ecs::PageMetadata), Changed<vmux_ecs::PageMetadata>>,
    projects: vmux_ecs::profile::Projects,
    mut commands: Commands,
) {
    for (entity, page) in &pages {
        if !ProjectRoute::is_page(&page.url) {
            continue;
        }
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
            let state = match ProjectRoute::resolve(&page.url, &root) {
                Some(Ok(_)) => ProjectUiState {
                    view: ProjectView::Detail,
                },
                Some(Err(error)) => ProjectUiState {
                    view: ProjectView::Error(error),
                },
                None => continue,
            };
            commands.entity(entity).remove::<ProjectCatalogTask>();
            commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
                entity, &state,
            ));
            continue;
        }
        commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
            entity,
            &ProjectUiState {
                view: ProjectView::Loading,
            },
        ));
        let task = IoTaskPool::get().spawn(async move { ProjectCatalog::read(&root) });
        commands.entity(entity).insert(ProjectCatalogTask(task));
    }
}

fn finish(
    mut tasks: Query<(Entity, &mut ProjectCatalogTask, &vmux_ecs::PageMetadata)>,
    mut commands: Commands,
) {
    for (entity, mut task, page) in &mut tasks {
        let Some(state) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<ProjectCatalogTask>();
        if !ProjectRoute::is_root(&page.url) {
            continue;
        }
        commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
            entity, &state,
        ));
    }
}

fn finish_operations(
    mut pending: Query<(Entity, &mut PendingProjectOperation)>,
    mut commands: Commands,
) {
    for (entity, mut operation) in &mut pending {
        let Some(result) = future::block_on(future::poll_once(&mut operation.task)) else {
            continue;
        };
        let state = match result {
            Ok(()) => ProjectCatalog::read(&operation.root),
            Err(error) => ProjectUiState {
                view: ProjectView::Error(error),
            },
        };
        commands.trigger(vmux_ecs::UiStateWrite::<ProjectUiState>::from_event(
            operation.webview,
            &state,
        ));
        commands.entity(entity).despawn();
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
            let Some(name) = directory.file_name().and_then(|name| name.to_str()) else {
                return;
            };
            entries.push(ProjectEntry {
                name: name.to_string(),
                path: directory
                    .canonicalize()
                    .unwrap_or_else(|_| directory.to_path_buf())
                    .to_string_lossy()
                    .into_owned(),
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
