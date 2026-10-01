use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, block_on, futures_lite::future};
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use vmux_ecs::host::UiStateWrite;

use crate::host::command_bar::project_files::{
    MAX_RESULTS, PendingProjectCompletion, ProjectCompletions, ProjectIndex, RankBias,
};
use crate::host::snapshot::{
    CommandBarProjectRoots, CommandBarWorkSnapshot, CommandBarWorkspaceSnapshot,
    WriteCommandBarSnapshots,
};
use vmux_api::command_bar::{CommandBarUiState, PathCompleteRequest, PathEntry};

pub(super) struct CompletionPlugin;

impl Plugin for CompletionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_plugins(UiEventPlugin::<(PathCompleteRequest,)>::default())
            .add_observer(request)
            .add_systems(
                Update,
                (
                    warm.after(WriteCommandBarSnapshots),
                    answer_index.after(warm),
                    start_paths.after(answer_index),
                    answer_paths.after(start_paths),
                ),
            );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct Sources<'w, 's> {
    workspace: Single<'w, 's, Ref<'static, CommandBarWorkspaceSnapshot>>,
    projects: Single<'w, 's, Ref<'static, CommandBarProjectRoots>>,
    work: Single<'w, 's, Ref<'static, CommandBarWorkSnapshot>>,
}

impl Sources<'_, '_> {
    fn roots(&self, query: &str) -> Vec<PathBuf> {
        ProjectQuery::roots_for(
            query,
            self.workspace.project_root.as_deref(),
            &self.projects.roots,
        )
    }

    fn all(&self) -> Vec<PathBuf> {
        ProjectQuery::all(self.workspace.project_root.as_deref(), &self.projects.roots)
    }

    fn bias(&self) -> RankBias {
        RankBias::new(
            ProjectQuery::favoured(
                self.projects.active.as_deref(),
                self.workspace.project_root.as_deref(),
            ),
            &self.work.recent_files,
        )
    }

    fn changed(&self) -> bool {
        self.workspace.is_changed() || self.projects.is_changed() || self.work.is_changed()
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("Project file index"), ProjectIndex::default()));
}

fn request(
    trigger: On<UiInput<PathCompleteRequest>>,
    sources: Sources,
    browsers: NonSend<Browsers>,
    pending: Query<&PendingProjectCompletion>,
    mut index: Single<&mut ProjectIndex>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let asking = trigger.event().webview;
    if !browsers.can_emit_to(&asking) {
        return;
    }
    let query = &trigger.event().payload.query;
    let request_id = trigger.event().payload.request_id;
    let roots = sources.roots(query);
    if roots.is_empty() {
        commands
            .entity(asking)
            .remove::<PendingProjectCompletion>()
            .insert(PathCompletionRequest {
                request_id,
                query: query.to_string(),
            });
        return;
    }
    let mut wanted = sources.all();
    for request in &pending {
        ProjectQuery::include(&mut wanted, &request.roots);
    }
    ProjectQuery::include(&mut wanted, &roots);
    index.sync(&wanted, proxy.as_deref());
    let bias = sources.bias();
    if index.walking(&roots) {
        commands.entity(asking).insert(PendingProjectCompletion {
            request_id,
            query: query.to_string(),
            roots: roots.clone(),
            answered_with: index.generation(),
        });
    } else {
        commands.entity(asking).remove::<PendingProjectCompletion>();
    }
    let Some(completions) = index.matches(&roots, &bias, query) else {
        commands.entity(asking).insert(PathCompletionRequest {
            request_id,
            query: query.to_string(),
        });
        return;
    };
    commands
        .entity(asking)
        .remove::<PathCompletionRequest>()
        .remove::<PathCompletionOperation>();
    commands.trigger(UiStateWrite::<CommandBarUiState>::from_event(
        asking,
        &completions.response(request_id),
    ));
}

fn warm(
    sources: Sources,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    pending: Query<&PendingProjectCompletion>,
    mut index: Single<&mut ProjectIndex>,
) {
    if !sources.changed() {
        return;
    }
    let mut roots = sources.all();
    for request in &pending {
        ProjectQuery::include(&mut roots, &request.roots);
    }
    if roots.is_empty() {
        return;
    }
    index.sync(&roots, proxy.as_deref());
}

fn answer_index(
    sources: Sources,
    browsers: NonSend<Browsers>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut index: Single<&mut ProjectIndex>,
    mut pending: Query<(Entity, &mut PendingProjectCompletion)>,
    mut commands: Commands,
) {
    if pending.is_empty() {
        return;
    }
    let mut wanted = sources.all();
    for (_, request) in pending.iter() {
        ProjectQuery::include(&mut wanted, &request.roots);
    }
    index.sync(&wanted, proxy.as_deref());
    let bias = sources.bias();
    for (webview, mut request) in &mut pending {
        if !browsers.can_emit_to(&webview) {
            commands
                .entity(webview)
                .remove::<PendingProjectCompletion>()
                .remove::<PathCompletionRequest>()
                .remove::<PathCompletionOperation>();
            continue;
        }
        let roots = sources.roots(&request.query);
        if roots.is_empty() {
            commands
                .entity(webview)
                .remove::<PendingProjectCompletion>();
            continue;
        }
        request.roots.clone_from(&roots);
        let completions = index.settled_for(&mut request, &roots, &bias);
        if !index.walking(&roots) {
            commands
                .entity(webview)
                .remove::<PendingProjectCompletion>();
        }
        let Some(completions) = completions else {
            continue;
        };
        commands
            .entity(webview)
            .remove::<PathCompletionRequest>()
            .remove::<PathCompletionOperation>();
        commands.trigger(UiStateWrite::<CommandBarUiState>::from_event(
            webview,
            &completions.response(request.request_id),
        ));
    }
}

fn start_paths(
    requests: Query<(Entity, &PathCompletionRequest), Changed<PathCompletionRequest>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (webview, request) in &requests {
        let query = PathQuery(request.query.clone());
        let wake = vmux_ecs::host::wake::Wake::beside(proxy.as_deref());
        let task = IoTaskPool::get().spawn(async move {
            let _wake = wake;
            query.complete()
        });
        commands
            .entity(webview)
            .remove::<PathCompletionRequest>()
            .insert(PathCompletionOperation {
                request_id: request.request_id,
                task,
            });
    }
}

fn answer_paths(
    browsers: NonSend<Browsers>,
    mut paths: Query<(Entity, &mut PathCompletionOperation)>,
    mut commands: Commands,
) {
    for (webview, mut pending) in &mut paths {
        let Some(completions) = block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        if browsers.can_emit_to(&webview) {
            commands.trigger(UiStateWrite::<CommandBarUiState>::from_event(
                webview,
                &completions.response(pending.request_id),
            ));
        }
        commands.entity(webview).remove::<PathCompletionOperation>();
    }
}

struct ProjectQuery;

impl ProjectQuery {
    fn roots_for(query: &str, project_root: Option<&str>, registered: &[String]) -> Vec<PathBuf> {
        let query = query.trim();
        if query.is_empty() || Self::names_a_location(query) {
            return Vec::new();
        }
        Self::all(project_root, registered)
    }

    fn favoured<'a>(active: Option<&'a str>, project_root: Option<&'a str>) -> Option<&'a str> {
        for candidate in [active, project_root] {
            let Some(candidate) = candidate else {
                continue;
            };
            let candidate = candidate.trim();
            if !candidate.is_empty() {
                return Some(candidate);
            }
        }
        None
    }

    fn all(project_root: Option<&str>, registered: &[String]) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        for candidate in project_root
            .into_iter()
            .chain(registered.iter().map(String::as_str))
        {
            let root = PathBuf::from(candidate.trim());
            if roots.contains(&root) || !root.is_dir() {
                continue;
            }
            roots.push(root);
        }
        roots
    }

    fn include(roots: &mut Vec<PathBuf>, additional: &[PathBuf]) {
        for root in additional {
            if !roots.contains(root) {
                roots.push(root.clone());
            }
        }
    }

    fn names_a_location(query: &str) -> bool {
        query.starts_with('/') || query.starts_with('~') || query.starts_with('.')
    }
}

#[derive(Component)]
struct PathCompletionRequest {
    request_id: u64,
    query: String,
}

#[derive(Component)]
struct PathCompletionOperation {
    request_id: u64,
    task: Task<ProjectCompletions>,
}

struct PathQuery(String);

impl PathQuery {
    fn complete(self) -> ProjectCompletions {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let query = self.0;
        let (parent, prefix) = if let Some(position) = query.rfind('/') {
            (&query[..=position], &query[position + 1..])
        } else {
            ("", query.as_str())
        };
        let resolved_parent = if parent.starts_with("~/") || parent == "~/" {
            PathBuf::from(&home).join(&parent[2..])
        } else if parent.starts_with('/') {
            PathBuf::from(parent)
        } else if parent.is_empty() {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(&home))
        } else {
            PathBuf::from(&home).join(parent)
        };
        let Ok(entries) = std::fs::read_dir(&resolved_parent) else {
            return ProjectCompletions::listed(Vec::new(), 0);
        };
        let prefix_lower = prefix.to_lowercase();
        let mut results = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
            if !prefix.is_empty() && !name.to_lowercase().starts_with(&prefix_lower) {
                continue;
            }
            let display_name = if is_dir {
                format!("{name}/")
            } else {
                name.clone()
            };
            let child = resolved_parent.join(&name);
            let full_path = if is_dir {
                format!("{}/", child.display())
            } else {
                child.display().to_string()
            };
            results.push(PathEntry {
                name: display_name,
                is_dir,
                full_path,
                project: String::new(),
            });
        }
        results.sort_by(|a, b| {
            let a_hidden = a.name.starts_with('.');
            let b_hidden = b.name.starts_with('.');
            b.is_dir
                .cmp(&a.is_dir)
                .then(a_hidden.cmp(&b_hidden))
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let total = results.len();
        results.truncate(MAX_RESULTS);
        ProjectCompletions::listed(results, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_query_lists_directories_then_visible_files_then_hidden_files() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("folder")).unwrap();
        std::fs::write(root.path().join("file"), "").unwrap();
        std::fs::write(root.path().join(".hidden"), "").unwrap();

        let completions = PathQuery(format!("{}/", root.path().display())).complete();
        let names = completions
            .entries
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();

        assert_eq!(names, ["folder/", "file", ".hidden"]);
    }
}
