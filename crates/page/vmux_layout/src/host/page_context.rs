use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_core::event::PageContextRequest;
use vmux_core::event::space::ProjectActivateRequest;
use vmux_git::state::{GitPageContext, GitWorkspaceChanged};

use crate::settings::EffectiveStartupDir;
use crate::tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use crate::worktree::{ManagedWorktreeRoot, TabWorktreeReady};

pub struct PageContextPlugin;

impl Plugin for PageContextPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(PageContextRequest, ProjectActivateRequest)>::default())
            .add_observer(on_page_context_request)
            .add_observer(on_project_activate);
    }
}

struct TabWorkspaceSelection {
    startup_dir: String,
    project_dir: String,
    worktree: Option<(TabWorktree, TabWorktreeReady)>,
}

impl TabWorkspaceSelection {
    fn resolve(
        path: &str,
        branch: &str,
        checkout: &str,
        managed_root: &std::path::Path,
    ) -> Result<Self, String> {
        let project_dir = std::path::Path::new(path.trim())
            .canonicalize()
            .map_err(|error| format!("invalid project directory: {error}"))?;
        if !project_dir.is_dir() {
            return Err("project path is not a directory".to_string());
        }
        if !checkout.trim().is_empty() {
            return Self::from_checkout(&project_dir, std::path::Path::new(checkout));
        }
        if !branch.trim().is_empty() {
            let activation = crate::worktree::create_worktree_for_existing_branch_blocking(
                &project_dir,
                branch.trim(),
                managed_root,
            )?;
            return Ok(Self {
                startup_dir: activation.execution_dir.to_string_lossy().into_owned(),
                project_dir: project_dir.to_string_lossy().into_owned(),
                worktree: Some((activation.metadata, activation.ready)),
            });
        }
        Self::from_checkout(&project_dir, &project_dir)
    }

    fn from_checkout(
        project_dir: &std::path::Path,
        checkout_dir: &std::path::Path,
    ) -> Result<Self, String> {
        let project_dir = project_dir
            .canonicalize()
            .map_err(|error| format!("invalid project directory: {error}"))?;
        let checkout_dir = checkout_dir
            .canonicalize()
            .map_err(|error| format!("invalid checkout directory: {error}"))?;
        let Some(info) = vmux_git::worktree::repo_info(&checkout_dir) else {
            return Ok(Self {
                startup_dir: checkout_dir.to_string_lossy().into_owned(),
                project_dir: checkout_dir.to_string_lossy().into_owned(),
                worktree: None,
            });
        };
        if !info.is_worktree {
            return Ok(Self {
                startup_dir: checkout_dir.to_string_lossy().into_owned(),
                project_dir: checkout_dir.to_string_lossy().into_owned(),
                worktree: None,
            });
        }
        let source = vmux_git::worktree::checkout_info(&project_dir).map_err(|error| error.0)?;
        let project_dir = info
            .project_root()
            .canonicalize()
            .map_err(|error| format!("invalid project directory: {error}"))?;
        let checkout = vmux_git::worktree::checkout_info(&checkout_dir).map_err(|error| error.0)?;
        if source.common_dir != checkout.common_dir {
            return Err("worktree belongs to a different repository".to_string());
        }
        let metadata = TabWorktree {
            repo_root: source.root.to_string_lossy().into_owned(),
            checkout_dir: checkout.root.to_string_lossy().into_owned(),
            branch: info.branch,
            base_ref: info.base_ref,
        };
        let project_dir_text = project_dir.to_string_lossy().into_owned();
        let ready = TabWorktreeReady::new(&checkout_dir, &project_dir_text, &metadata, &checkout)?;
        Ok(Self {
            startup_dir: checkout_dir.to_string_lossy().into_owned(),
            project_dir: project_dir_text,
            worktree: Some((metadata, ready)),
        })
    }

    fn apply(self, tab_entity: Entity, tab: &mut Tab, commands: &mut Commands) -> String {
        tab.startup_dir = Some(self.startup_dir.clone());
        if crate::worktree::is_generated_tab_name(&tab.name)
            && let Some(name) = std::path::Path::new(&self.project_dir)
                .file_name()
                .and_then(|name| name.to_str())
            && !name.is_empty()
        {
            tab.name = name.to_string();
        }
        let mut entity = commands.entity(tab_entity);
        entity.insert((
            TabWorkspace {
                project_dir: self.project_dir,
            },
            TabDirDecided,
        ));
        match self.worktree {
            Some((metadata, ready)) => {
                entity
                    .insert((metadata, ready))
                    .remove::<TabWorktreeUnavailable>();
            }
            None => {
                entity.remove::<(TabWorktree, TabWorktreeReady, TabWorktreeUnavailable)>();
            }
        }
        self.startup_dir
    }
}

fn on_page_context_request(
    trigger: On<UiInput<PageContextRequest>>,
    child_of: Query<&ChildOf>,
    tabs: Query<&Tab>,
    pages: Query<&vmux_core::PageMetadata>,
    effective_dir: Option<Res<EffectiveStartupDir>>,
    mut commands: Commands,
) {
    let path = crate::tab::ancestor_tab_startup_dir(trigger.event().webview, &child_of, &tabs)
        .map(PathBuf::from)
        .or_else(|| {
            effective_dir
                .as_ref()
                .and_then(|effective| effective.0.as_ref())
                .and_then(|(_, path)| path.clone())
        })
        .or_else(|| std::env::current_dir().ok())
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    let page_url = pages
        .get(trigger.event().webview)
        .map(|page| page.url.clone())
        .unwrap_or_default();
    commands.trigger(
        vmux_core::host::UiStateWrite::<vmux_git::state::GitUiState>::from_event(
            trigger.event().webview,
            &GitPageContext {
                working_directory: path,
                page_url,
            },
        ),
    );
}

fn on_project_activate(
    trigger: On<UiInput<ProjectActivateRequest>>,
    child_of: Query<&ChildOf>,
    tab_entities: Query<(), With<Tab>>,
    pane_entities: Query<Entity, With<crate::pane::Pane>>,
    pages: Query<Entity, With<vmux_core::PageMetadata>>,
    mut tabs: Query<&mut Tab>,
    managed_root: Res<ManagedWorktreeRoot>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let ProjectActivateRequest {
        path,
        branch,
        checkout,
        pane_id,
    } = &trigger.event().payload;
    let mut current = pane_id
        .and_then(|pane_id| {
            pane_entities
                .iter()
                .find(|entity| entity.to_bits() == pane_id)
        })
        .unwrap_or(webview);
    let tab_entity = loop {
        if tab_entities.contains(current) {
            break Some(current);
        }
        let Ok(parent) = child_of.get(current) else {
            break None;
        };
        current = parent.parent();
    };
    let result = match tab_entity {
        Some(tab_entity) => match tabs.get_mut(tab_entity) {
            Ok(mut tab) => TabWorkspaceSelection::resolve(path, branch, checkout, &managed_root.0)
                .map(|selection| selection.apply(tab_entity, &mut tab, &mut commands)),
            Err(error) => Err(error.to_string()),
        },
        None => Err("tab workspace is unavailable".to_string()),
    };
    let (path, error) = match result {
        Ok(path) => (path, String::new()),
        Err(error) => (String::new(), error),
    };
    let event = GitWorkspaceChanged {
        path,
        branch: branch.clone(),
        error,
    };
    let Some(tab_entity) = tab_entity else {
        commands.trigger(
            vmux_core::host::UiStateWrite::<vmux_git::state::GitUiState>::from_event(
                webview, &event,
            ),
        );
        return;
    };
    for page in &pages {
        let mut current = page;
        loop {
            if current == tab_entity {
                commands.trigger(
                    vmux_core::host::UiStateWrite::<vmux_git::state::GitUiState>::from_event(
                        page, &event,
                    ),
                );
                break;
            }
            let Ok(parent) = child_of.get(current) else {
                break;
            };
            current = parent.parent();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_activation_updates_the_owning_tab_workspace() {
        let project = tempfile::tempdir().unwrap();
        let managed_root = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()))
            .add_observer(on_project_activate);
        let tab = app.world_mut().spawn(Tab::default()).id();
        let webview = app.world_mut().spawn(ChildOf(tab)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ProjectActivateRequest {
                path: project.path().to_string_lossy().into_owned(),
                branch: String::new(),
                checkout: String::new(),
                pane_id: None,
            },
        });
        app.update();

        let expected = project.path().canonicalize().unwrap();
        let tab_state = app.world().get::<Tab>(tab).unwrap();
        let workspace = app.world().get::<TabWorkspace>(tab).unwrap();
        assert_eq!(
            tab_state.startup_dir.as_deref(),
            Some(expected.to_string_lossy().as_ref())
        );
        assert_eq!(
            workspace.project_dir,
            expected.to_string_lossy().into_owned()
        );
        assert!(app.world().get::<TabDirDecided>(tab).is_some());
    }
}
