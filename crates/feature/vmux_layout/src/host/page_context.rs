use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::PageMetadata;
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ecs::page::PageReady;
use vmux_git::state::{GitPageContext, GitWorkspaceChanged};

use crate::space::FocusedSpace;
use crate::tab::{
    Tab, TabDirDecided, TabHierarchy, TabWorkspace, TabWorktree, TabWorktreeUnavailable,
};
use crate::worktree::{ManagedWorktreeRoot, TabWorktreeReady};

pub struct PageContextPlugin;

impl Plugin for PageContextPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(ProjectActivateRequest,)>::default())
            .add_observer(git_page_ready)
            .add_observer(project_activate);
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
            let checkout = std::path::Path::new(checkout);
            if checkout.is_dir() {
                return Self::from_checkout(
                    &project_dir,
                    checkout,
                    (!branch.trim().is_empty()).then_some(branch.trim()),
                );
            }
        }
        if !branch.trim().is_empty() {
            let activation = crate::worktree::TabWorktreeActivation::checkout_branch(
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
        Self::from_checkout(&project_dir, &project_dir, None)
    }

    fn from_checkout(
        project_dir: &std::path::Path,
        checkout_dir: &std::path::Path,
        expected_branch: Option<&str>,
    ) -> Result<Self, String> {
        let project_dir = project_dir
            .canonicalize()
            .map_err(|error| format!("invalid project directory: {error}"))?;
        let checkout_dir = checkout_dir
            .canonicalize()
            .map_err(|error| format!("invalid checkout directory: {error}"))?;
        let Some(info) = vmux_git::worktree::RepoInfo::read(&checkout_dir) else {
            if expected_branch.is_some() {
                return Err("branch checkout is no longer available".to_string());
            }
            return Ok(Self {
                startup_dir: checkout_dir.to_string_lossy().into_owned(),
                project_dir: checkout_dir.to_string_lossy().into_owned(),
                worktree: None,
            });
        };
        if let Some(expected_branch) = expected_branch
            && info.branch != expected_branch
        {
            return Err(format!(
                "branch {expected_branch} is no longer checked out at {}",
                checkout_dir.display()
            ));
        }
        let source = vmux_git::worktree::CheckoutInfo::try_from(project_dir.as_path())
            .map_err(|error| error.0)?;
        let relative_dir = project_dir
            .strip_prefix(&source.root)
            .map_err(|_| "project directory is outside its checkout".to_string())?;
        let project_dir = info
            .project_root()
            .join(relative_dir)
            .canonicalize()
            .map_err(|error| format!("invalid project directory: {error}"))?;
        let checkout = vmux_git::worktree::CheckoutInfo::try_from(checkout_dir.as_path())
            .map_err(|error| error.0)?;
        if source.common_dir != checkout.common_dir {
            return Err("worktree belongs to a different repository".to_string());
        }
        let execution_dir = checkout
            .root
            .join(relative_dir)
            .canonicalize()
            .map_err(|error| format!("project directory is missing from checkout: {error}"))?;
        if !execution_dir.is_dir() || !execution_dir.starts_with(&checkout.root) {
            return Err("project directory escapes checkout".to_string());
        }
        let project_dir_text = project_dir.to_string_lossy().into_owned();
        if !info.is_worktree {
            return Ok(Self {
                startup_dir: execution_dir.to_string_lossy().into_owned(),
                project_dir: project_dir_text,
                worktree: None,
            });
        }
        let metadata = TabWorktree {
            repo_root: info.project_root().to_string_lossy().into_owned(),
            checkout_dir: checkout.root.to_string_lossy().into_owned(),
            branch: info.branch,
            base_ref: info.base_ref,
        };
        let ready = TabWorktreeReady::new(&execution_dir, &project_dir_text, &metadata, &checkout)?;
        Ok(Self {
            startup_dir: execution_dir.to_string_lossy().into_owned(),
            project_dir: project_dir_text,
            worktree: Some((metadata, ready)),
        })
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct WorkspaceWriter<'w, 's> {
    tabs: Query<'w, 's, &'static mut Tab>,
    commands: Commands<'w, 's>,
}

impl WorkspaceWriter<'_, '_> {
    fn apply(
        &mut self,
        selection: TabWorkspaceSelection,
        tab_entity: Entity,
    ) -> Result<String, String> {
        let mut tab = self
            .tabs
            .get_mut(tab_entity)
            .map_err(|error| error.to_string())?;
        tab.startup_dir = Some(selection.startup_dir.clone());
        if crate::worktree::WorktreeName::is_generated(&tab.name)
            && let Some(name) = std::path::Path::new(&selection.project_dir)
                .file_name()
                .and_then(|name| name.to_str())
            && !name.is_empty()
        {
            tab.name = name.to_string();
        }
        let mut entity = self.commands.entity(tab_entity);
        entity.insert((
            TabWorkspace {
                project_dir: selection.project_dir,
            },
            TabDirDecided,
        ));
        match selection.worktree {
            Some((metadata, ready)) => {
                entity
                    .insert((metadata, ready))
                    .remove::<TabWorktreeUnavailable>();
            }
            None => {
                entity.remove::<(TabWorktree, TabWorktreeReady, TabWorktreeUnavailable)>();
            }
        }
        Ok(selection.startup_dir)
    }
}

fn git_page_ready(
    trigger: On<UiInput<PageReady>>,
    tabs: TabHierarchy,
    pages: Query<&PageMetadata>,
    focused_space: FocusedSpace,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(page) = pages.get(webview) else {
        return;
    };
    if !page.url.starts_with(vmux_git::GIT_PAGE_URL)
        && page.url != vmux_git::GitPlugin::MANIFEST.url
    {
        return;
    }
    let path = tabs
        .startup_dir(webview)
        .map(PathBuf::from)
        .or_else(|| focused_space.startup_dir().map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    commands.trigger(
        vmux_ecs::UiStateWrite::<vmux_git::state::GitUiState>::from_event(
            webview,
            &GitPageContext {
                working_directory: path,
                page_url: page.url.clone(),
            },
        ),
    );
}

fn project_activate(
    trigger: On<UiInput<ProjectActivateRequest>>,
    child_of: Query<&ChildOf>,
    tab_entities: Query<(), With<Tab>>,
    pane_entities: Query<Entity, With<crate::pane::Pane>>,
    pages: Query<(Entity, &PageMetadata)>,
    managed_root: Res<ManagedWorktreeRoot>,
    mut writer: WorkspaceWriter,
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
        Some(tab_entity) => TabWorkspaceSelection::resolve(path, branch, checkout, &managed_root.0)
            .and_then(|selection| writer.apply(selection, tab_entity)),
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
        writer.commands.trigger(
            vmux_ecs::UiStateWrite::<vmux_git::state::GitUiState>::from_event(webview, &event),
        );
        return;
    };
    for (page, metadata) in &pages {
        if !vmux_git::GitUrl::is_page(&metadata.url) {
            continue;
        }
        let mut current = page;
        loop {
            if current == tab_entity {
                writer.commands.trigger(
                    vmux_ecs::UiStateWrite::<vmux_git::state::GitUiState>::from_event(page, &event),
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

    #[derive(Resource, Default)]
    struct ContextWrites(Vec<GitPageContext>);

    #[derive(Resource, Default)]
    struct WorkspaceWrites(Vec<Entity>);

    fn capture_context(
        trigger: On<vmux_ecs::UiStateWrite<vmux_git::state::GitUiState>>,
        mut writes: ResMut<ContextWrites>,
    ) {
        if let Some(context) = &trigger.event().update().context {
            writes.0.push(context.clone());
        }
    }

    fn capture_workspace(
        trigger: On<vmux_ecs::UiStateWrite<vmux_git::state::GitUiState>>,
        mut writes: ResMut<WorkspaceWrites>,
    ) {
        if trigger.event().update().workspace.is_some() {
            writes.0.push(trigger.event().webview());
        }
    }

    #[test]
    fn git_page_ready_publishes_page_context() {
        let mut app = App::new();
        app.init_resource::<ContextWrites>()
            .add_observer(git_page_ready)
            .add_observer(capture_context);
        let webview = app
            .world_mut()
            .spawn(PageMetadata {
                url: "git://tmp/repo".to_string(),
                ..Default::default()
            })
            .id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: PageReady {},
        });
        app.update();

        let writes = &app.world().resource::<ContextWrites>().0;
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].page_url, "git://tmp/repo");
    }

    #[test]
    fn project_activation_updates_the_owning_tab_workspace() {
        let project = tempfile::tempdir().unwrap();
        let managed_root = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()))
            .add_observer(project_activate);
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

    #[test]
    fn project_activation_notifies_only_git_pages_in_the_tab() {
        let project = tempfile::tempdir().unwrap();
        let managed_root = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()))
            .init_resource::<WorkspaceWrites>()
            .add_observer(project_activate)
            .add_observer(capture_workspace);
        let tab = app.world_mut().spawn(Tab::default()).id();
        let requester = app
            .world_mut()
            .spawn((
                ChildOf(tab),
                PageMetadata {
                    url: "vmux://projects/".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        let git = app
            .world_mut()
            .spawn((
                ChildOf(tab),
                PageMetadata {
                    url: "git://tmp/repo".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        app.world_mut().spawn((
            ChildOf(tab),
            PageMetadata {
                url: "vmux://sessions/".to_string(),
                ..Default::default()
            },
        ));

        app.world_mut().trigger(UiInput {
            webview: requester,
            payload: ProjectActivateRequest {
                path: project.path().to_string_lossy().into_owned(),
                branch: String::new(),
                checkout: String::new(),
                pane_id: None,
            },
        });
        app.update();

        assert_eq!(app.world().resource::<WorkspaceWrites>().0, vec![git]);
    }

    #[test]
    fn branch_checkout_preserves_nested_project_directory() {
        let repository = tempfile::tempdir().unwrap();
        let nested = repository.path().join("crates/app");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("main.rs"), "fn main() {}\n").unwrap();
        let source = vmux_git::worktree::CheckoutInfo::initialize(repository.path()).unwrap();
        source.ensure_initial_snapshot("Initialize").unwrap();
        let worktrees = tempfile::tempdir().unwrap();
        let checkout = worktrees.path().join("feature");
        source.add_worktree(&checkout, "feature", "HEAD").unwrap();
        let managed_root = tempfile::tempdir().unwrap();

        let selection = TabWorkspaceSelection::resolve(
            nested.to_string_lossy().as_ref(),
            "feature",
            checkout.join("crates/app").to_string_lossy().as_ref(),
            managed_root.path(),
        )
        .unwrap();

        let expected_project = nested.canonicalize().unwrap();
        let expected_checkout = checkout.join("crates/app").canonicalize().unwrap();
        let (metadata, _) = selection.worktree.unwrap();
        assert_eq!(selection.project_dir, expected_project.to_string_lossy());
        assert_eq!(selection.startup_dir, expected_checkout.to_string_lossy());
        assert_eq!(
            metadata.repo_root,
            repository.path().canonicalize().unwrap().to_string_lossy()
        );
        assert_eq!(
            metadata.checkout_dir,
            checkout.canonicalize().unwrap().to_string_lossy()
        );
        assert_eq!(metadata.branch, "feature");
    }

    #[test]
    fn branch_checkout_rejects_stale_checkout_ownership() {
        let repository = tempfile::tempdir().unwrap();
        std::fs::write(repository.path().join("README.md"), "vmux\n").unwrap();
        let source = vmux_git::worktree::CheckoutInfo::initialize(repository.path()).unwrap();
        source.ensure_initial_snapshot("Initialize").unwrap();
        let worktrees = tempfile::tempdir().unwrap();
        let checkout = worktrees.path().join("feature");
        source.add_worktree(&checkout, "feature", "HEAD").unwrap();
        let managed_root = tempfile::tempdir().unwrap();

        let error = TabWorkspaceSelection::resolve(
            repository.path().to_string_lossy().as_ref(),
            "other",
            checkout.to_string_lossy().as_ref(),
            managed_root.path(),
        )
        .err()
        .unwrap();

        assert!(error.contains("no longer checked out"));
    }
}
