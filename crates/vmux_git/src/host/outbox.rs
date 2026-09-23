use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy_cef::prelude::BinHostEmitEvent;
use vmux_core::host::FileUiStateUpdates;

use crate::event::GitErrorEvent;

use super::GitUpdateSet;
use super::job::{Emit, JobKind, run_job};
use super::status::GitStatusJobs;

pub(super) struct OutboxPlugin;

impl Plugin for OutboxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitOutbox>()
            .add_systems(Update, drain_git_outbox.in_set(GitUpdateSet::Outbox));
    }
}

enum GitOutboxItem {
    Events {
        webview: Entity,
        emits: Vec<Emit>,
    },
    StatusBatch {
        repo_root: PathBuf,
        results: Vec<(Entity, Vec<Emit>)>,
    },
}

#[derive(Resource, Clone, Default)]
pub(super) struct GitOutbox(Arc<Mutex<Vec<GitOutboxItem>>>);

impl GitOutbox {
    pub(super) fn spawn(&self, webview: Entity, job: JobKind) {
        let outbox = self.clone();
        std::thread::spawn(move || {
            outbox.events(webview, run_job(job));
        });
    }

    pub(super) fn events(&self, webview: Entity, emits: Vec<Emit>) {
        self.push(GitOutboxItem::Events { webview, emits });
    }

    pub(super) fn error(&self, webview: Entity, message: String) {
        self.events(webview, vec![Emit::Error(GitErrorEvent { message })]);
    }

    pub(super) fn status_batch(&self, repo_root: PathBuf, results: Vec<(Entity, Vec<Emit>)>) {
        self.push(GitOutboxItem::StatusBatch { repo_root, results });
    }

    fn push(&self, item: GitOutboxItem) {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(item);
    }

    fn drain(&self) -> Vec<GitOutboxItem> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .drain(..)
            .collect()
    }
}

impl Emit {
    fn deliver(
        self,
        commands: &mut Commands,
        pages: &mut Query<&mut vmux_core::PageMetadata>,
        file_pages: &Query<(), With<FileUiStateUpdates>>,
        webview: Entity,
    ) {
        match self {
            Self::Repository(event) => {
                if let Ok(mut page) = pages.get_mut(webview) {
                    if let Some(url) = crate::GitUrl::from_path(Path::new(&event.repo_root)) {
                        page.url = url;
                    }
                    page.title = match event.branch.is_empty() {
                        true => event.repo_name.clone(),
                        false => format!("{} · {}", event.repo_name, event.branch),
                    };
                }
                commands.trigger(BinHostEmitEvent::from_event(webview, &event));
            }
            Self::BranchLog(event) => {
                commands.trigger(BinHostEmitEvent::from_event(webview, &event));
            }
            Self::Status(event) => {
                FileUiStateUpdates::deliver(file_pages, commands, webview, &event);
            }
            Self::DiffMeta(event) => {
                FileUiStateUpdates::deliver(file_pages, commands, webview, &event);
            }
            Self::DiffViewport(event) => {
                FileUiStateUpdates::deliver(file_pages, commands, webview, &event);
            }
            Self::Result(event) => {
                FileUiStateUpdates::deliver(file_pages, commands, webview, &event);
            }
            Self::Error(event) => {
                FileUiStateUpdates::deliver(file_pages, commands, webview, &event);
            }
        }
    }
}

fn drain_git_outbox(
    outbox: Res<GitOutbox>,
    mut jobs: ResMut<GitStatusJobs>,
    mut pages: Query<&mut vmux_core::PageMetadata>,
    file_pages: Query<(), With<FileUiStateUpdates>>,
    mut commands: Commands,
) {
    for item in outbox.drain() {
        match item {
            GitOutboxItem::Events { webview, emits } => {
                for emit in emits {
                    emit.deliver(&mut commands, &mut pages, &file_pages, webview);
                }
            }
            GitOutboxItem::StatusBatch { repo_root, results } => {
                jobs.complete(&repo_root);
                for (webview, emits) in results {
                    for emit in emits {
                        emit.deliver(&mut commands, &mut pages, &file_pages, webview);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_empties_outbox() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<GitStatusJobs>()
            .add_plugins(OutboxPlugin);

        let webview = app.world_mut().spawn_empty().id();
        app.world()
            .resource::<GitOutbox>()
            .error(webview, "boom".into());

        app.update();

        assert!(app.world().resource::<GitOutbox>().drain().is_empty());
    }
}
