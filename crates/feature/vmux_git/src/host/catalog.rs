use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::{GitCatalog, GitCatalogNextRequest};
use crate::state::{GitPageControllerState, GitPanel};

use super::GitUpdateSet;
use super::job::CatalogJob;
use super::job_runner::GitJob;
use super::repository::GitRepository;

const CATALOG_PAGE_SIZE: usize = 40;

pub(super) struct CatalogPlugin;

impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(GitCatalogNextRequest,)>::default())
            .add_observer(load_next)
            .add_systems(Update, sync_catalogs.in_set(GitUpdateSet::Catalog));
    }
}

type ChangedCatalogPages<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut super::state::GitState,
        &'static GitPageControllerState,
    ),
    Or<(
        Changed<super::state::GitState>,
        Changed<GitPageControllerState>,
    )>,
>;

fn sync_catalogs(mut pages: ChangedCatalogPages, mut commands: Commands) {
    for (entity, mut state, controller) in &mut pages {
        let Some(repository) = state.snapshot.repository.as_ref() else {
            continue;
        };
        let current_branch = repository.branch.clone();
        let repo_root = repository.repo_root.clone();
        let mut catalogs = vec![GitCatalog::LocalBranches];
        if !controller.selected_commit.is_empty()
            || repository.files.iter().any(|entry| entry.staged)
        {
            catalogs.push(GitCatalog::Commits);
        }
        match controller.focused_panel {
            GitPanel::Branches => catalogs.push(controller.branch_collection.into()),
            GitPanel::Commits => catalogs.push(GitCatalog::Commits),
            GitPanel::Stash => catalogs.push(GitCatalog::Stashes),
            GitPanel::Files => {}
        }
        catalogs.dedup();
        for catalog in catalogs {
            let progress = state.snapshot.catalog(catalog);
            if progress.loaded || progress.loading || progress.failed {
                continue;
            }
            state.snapshot.catalog_mut(catalog).loading = true;
            commands.spawn((
                GitJob::new(entity),
                GitRepository::at(repo_root.clone()),
                CatalogJob {
                    catalog,
                    current_branch: current_branch.clone(),
                    offset: 0,
                    limit: CATALOG_PAGE_SIZE,
                    revision: state.snapshot.nonce,
                },
            ));
        }
    }
}

fn load_next(
    trigger: On<UiInput<GitCatalogNextRequest>>,
    mut pages: Query<&mut super::state::GitState>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let catalog = trigger.event().payload.catalog;
    let Ok(mut state) = pages.get_mut(webview) else {
        return;
    };
    let progress = state.snapshot.catalog(catalog);
    if !progress.loaded || progress.loading || !progress.has_more {
        return;
    }
    let Some(repository) = state.snapshot.repository.as_ref() else {
        return;
    };
    let current_branch = repository.branch.clone();
    let repo_root = repository.repo_root.clone();
    let offset = repository.catalog_len(catalog);
    state.snapshot.catalog_mut(catalog).loading = true;
    commands.spawn((
        GitJob::new(webview),
        GitRepository::at(repo_root),
        CatalogJob {
            catalog,
            current_branch,
            offset,
            limit: CATALOG_PAGE_SIZE,
            revision: state.snapshot.nonce,
        },
    ));
}
