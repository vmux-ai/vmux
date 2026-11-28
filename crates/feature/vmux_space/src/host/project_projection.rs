use std::path::Path;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_api::space::{ProjectRow, ProjectRowKind};
use vmux_ecs::UiStateWrite;
use vmux_git::RepoInfoCache;
use vmux_git::worktree::RepoInfo;
use vmux_layout::LayoutCef;
use vmux_layout::event::{TabBoundary, TabBoundaryState, WorkspaceBranch};
use vmux_layout::page_context::PendingWorkspaceSelection;
use vmux_layout::projection::{LayoutUiProjectionSet, ProjectProjection};
use vmux_layout::stack::FocusedStack;
use vmux_layout::state::LayoutUiState;
use vmux_layout::state_source::{
    LayoutProjection, LayoutStateSourceSet, ProjectionReceipt, StateRevision,
};
use vmux_layout::tab::TabWorktree;

use super::project::{ExpandedProjectDirs, SpaceProjects};

pub struct SpaceProjectProjectionPlugin;

impl Plugin for SpaceProjectProjectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(retire_receipt)
            .add_systems(
                PostUpdate,
                mark_dirty.in_set(LayoutStateSourceSet::MarkDirty),
            )
            .add_systems(
                PostUpdate,
                push_projects.in_set(LayoutUiProjectionSet::Source),
            );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectProjectionSelection {
    tab: Entity,
    revision: u64,
}

#[derive(Component)]
struct ProjectSelectionReceipt(Option<ProjectProjectionSelection>);

fn retire_receipt(removed: On<Remove, LayoutCef>, mut commands: Commands) {
    commands
        .entity(removed.event_target())
        .try_remove::<(ProjectionReceipt<TabBoundaryState>, ProjectSelectionReceipt)>();
}

fn mark_dirty(
    changed: Query<(), Changed<ExpandedProjectDirs>>,
    mut removed: RemovedComponents<ExpandedProjectDirs>,
    mut revision: Option<Single<&mut StateRevision>>,
) {
    let dirty = !changed.is_empty() || removed.read().count() > 0;
    if dirty && let Some(revision) = revision.as_mut() {
        revision.advance();
    }
}

#[derive(SystemParam)]
struct ProjectRows<'w, 's> {
    focus: FocusedStack<'w, 's>,
    projects: SpaceProjects<'w, 's>,
    pending: Query<'w, 's, &'static PendingWorkspaceSelection>,
    worktrees: Query<'w, 's, &'static TabWorktree>,
    repos: Option<Single<'w, 's, &'static mut RepoInfoCache>>,
}

impl ProjectRows<'_, '_> {
    fn selection(&self) -> Option<ProjectProjectionSelection> {
        let tab = self.focus.tab?;
        let pending = self.pending.get(tab).ok()?;
        Some(ProjectProjectionSelection {
            tab,
            revision: pending.revision(),
        })
    }

    fn snapshot(&mut self, layout: Entity) -> TabBoundaryState {
        let mut projects = self.projects.layout_rows();
        for row in &mut projects {
            let path = if row.display_path.is_empty() {
                &row.path
            } else {
                &row.display_path
            };
            row.display_path = vmux_path::DisplayPath::project(path);
        }
        let mut boundary = projects
            .iter()
            .find(|row| row.depth == 0 && row.is_active)
            .map(|row| TabBoundary {
                effective_dir: row.path.clone(),
                project_dir: row.path.clone(),
                source: "project".to_string(),
                branch: row.branch.clone(),
                ..Default::default()
            });
        let mut branches = Vec::new();
        if let Some(cache) = self.repos.as_mut() {
            let cache = cache.bypass_change_detection();
            cache.release(layout);
            for row in &mut projects {
                if row.missing || row.kind != ProjectRowKind::Project || !row.is_active {
                    continue;
                }
                let path = Path::new(&row.path);
                let Some(info) = cache.observe_active(layout, path) else {
                    continue;
                };
                row.branch.clone_from(&info.branch);
                if let Some(project_boundary) = Self::git_boundary(&info, row, &mut branches) {
                    boundary = Some(project_boundary);
                }
            }
        }
        self.override_branch(&mut projects, &mut boundary);
        TabBoundaryState {
            boundary,
            projects,
            branches,
        }
    }

    fn git_boundary(
        info: &RepoInfo,
        row: &ProjectRow,
        branches: &mut Vec<WorkspaceBranch>,
    ) -> Option<TabBoundary> {
        let path = Path::new(&row.path);
        let relative = path.strip_prefix(&info.repo_root).ok()?;
        let project_dir = info.main_worktree_root().join(relative);
        branches.reserve(info.branches.len());
        for holder in info.branches.iter() {
            let checkout = holder
                .checkout
                .as_ref()
                .map(|checkout| checkout.join(relative).to_string_lossy().into_owned())
                .unwrap_or_default();
            branches.push(WorkspaceBranch {
                branch: holder.branch.clone(),
                checkout,
                label: holder.checkout_label(),
            });
        }
        Some(TabBoundary {
            effective_dir: row.path.clone(),
            project_dir: project_dir.to_string_lossy().into_owned(),
            source: "project".to_string(),
            repository: info.main_worktree_name(),
            is_git_repo: true,
            is_worktree: info.is_worktree,
            branch: info.branch.clone(),
            base_ref: info.base_ref.clone(),
            uncommitted: info.uncommitted,
            ahead: info.ahead,
            changed_files: info.changed_files,
            insertions: info.insertions,
            deletions: info.deletions,
            ..Default::default()
        })
    }

    fn override_branch(&self, projects: &mut [ProjectRow], boundary: &mut Option<TabBoundary>) {
        let Some(tab) = self.focus.tab else {
            return;
        };
        if let Ok(worktree) = self.worktrees.get(tab) {
            if let Some(project) = projects.iter_mut().find(|project| project.is_active) {
                project.branch.clone_from(&worktree.branch);
            }
            if let Some(boundary) = boundary.as_mut() {
                boundary.branch.clone_from(&worktree.branch);
            }
        }
        if let Ok(pending) = self.pending.get(tab)
            && !pending.branch().is_empty()
            && boundary
                .as_ref()
                .is_some_and(|boundary| pending.matches_project_dir(&boundary.project_dir))
        {
            if let Some(project) = projects.iter_mut().find(|project| project.is_active) {
                project.branch = pending.branch().to_string();
            }
            if let Some(boundary) = boundary.as_mut() {
                boundary.branch = pending.branch().to_string();
            }
        }
    }
}

fn push_projects(
    mut projection: LayoutProjection<TabBoundaryState>,
    mut rows: ProjectRows,
    selections: Query<(Ref<LayoutCef>, &ProjectSelectionReceipt)>,
) {
    let selection = rows.selection();
    let Some((layout, _)) = projection.layout.get() else {
        return;
    };
    let changed = match selections.get(layout) {
        Ok((owner, receipt)) => owner.is_added() || receipt.0 != selection,
        Err(_) => true,
    };
    let Some(target) = projection.target_for(changed) else {
        return;
    };
    let payload = rows.snapshot(target.entity);
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    projection
        .commands
        .entity(target.entity)
        .insert(ProjectSelectionReceipt(selection));
    if !projection.should_emit(target, body) {
        return;
    }
    projection
        .commands
        .entity(target.entity)
        .insert(ProjectProjection(payload.clone()));
    projection
        .commands
        .trigger(UiStateWrite::<LayoutUiState>::from_event(
            target.entity,
            &payload,
        ));
}
