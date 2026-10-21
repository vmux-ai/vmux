use std::path::Path;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};

use crate::project::{ProjectEntry, ProjectRoute, ProjectUiState};

pub(super) struct ProjectPagePlugin;

impl Plugin for ProjectPagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_ecs::UiStatePlugin::<ProjectUiState>::default())
            .add_systems(Update, (queue, finish).chain());
    }
}

#[derive(Component)]
struct ProjectCatalogTask(Task<ProjectUiState>);

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
                        loaded: true,
                        error,
                        ..Default::default()
                    },
                ));
                continue;
            }
        };
        if !ProjectRoute::is_root(&page.url) {
            let state = match ProjectRoute::resolve(&page.url, &root) {
                Some(Ok(_)) => ProjectUiState {
                    detail: true,
                    loaded: true,
                    ..Default::default()
                },
                Some(Err(error)) => ProjectUiState {
                    detail: true,
                    loaded: true,
                    error,
                    ..Default::default()
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
                loading: true,
                ..Default::default()
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
            entries,
            detail: false,
            loading: false,
            loaded: true,
            error: String::new(),
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
                path: directory.to_string_lossy().into_owned(),
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
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
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

        assert_eq!(
            state
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["dashboard", "vmux-cloud"]
        );
        assert_eq!(
            state.entries[0].url,
            "vmux://projects/github.com/mistralai/dashboard"
        );
        assert!(!state.loading);
        assert!(state.loaded);
        assert!(state.error.is_empty());
    }
}
