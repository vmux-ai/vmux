use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, block_on, futures_lite::future};
use bevy_cef::prelude::{BinHostEmitEvent, BinReceive, Browsers, UiEventPlugin};

use crate::command_bar::project_files::{MAX_RESULTS, ProjectCompletions, ProjectIndex, RankBias};
use crate::event::{PathCompleteRequest, PathEntry};
use crate::snapshot::{
    CommandBarProjectRoots, CommandBarWorkSnapshot, CommandBarWorkspaceSnapshot,
    WriteCommandBarSnapshots,
};

pub(super) struct CommandBarCompletionPlugin;

impl Plugin for CommandBarCompletionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(PathCompleteRequest,)>::default())
            .init_resource::<ProjectIndex>()
            .init_resource::<PathCompletions>()
            .add_observer(on_path_complete_request)
            .add_systems(
                Update,
                (
                    warm_project_index.after(WriteCommandBarSnapshots),
                    answer_settled_project_index.after(warm_project_index),
                    answer_path_completions.after(answer_settled_project_index),
                ),
            );
    }
}

fn on_path_complete_request(
    trigger: On<BinReceive<PathCompleteRequest>>,
    workspace: Res<CommandBarWorkspaceSnapshot>,
    projects: Res<CommandBarProjectRoots>,
    work: Res<CommandBarWorkSnapshot>,
    browsers: NonSend<Browsers>,
    mut index: ResMut<ProjectIndex>,
    mut paths: ResMut<PathCompletions>,
    mut commands: Commands,
) {
    let asking = trigger.event().webview;
    if !browsers.can_emit_to(&asking) {
        return;
    }
    let query = &trigger.event().payload.query;
    let roots = ProjectQuery::roots_for(query, workspace.project_root.as_deref(), &projects.roots);
    if roots.is_empty() {
        index.forget(asking);
        paths.start(asking, query);
        return;
    }
    let bias = RankBias::new(
        ProjectQuery::favoured(
            projects.active.as_deref(),
            workspace.project_root.as_deref(),
        ),
        &work.recent_files,
    );
    let Some(completions) = index.matches(&roots, &bias, query, asking) else {
        paths.start(asking, query);
        return;
    };
    paths.cancel(asking);
    commands.trigger(BinHostEmitEvent::from_event(
        asking,
        &completions.response(),
    ));
}

fn warm_project_index(
    workspace: Res<CommandBarWorkspaceSnapshot>,
    projects: Res<CommandBarProjectRoots>,
    mut index: ResMut<ProjectIndex>,
) {
    if !workspace.is_changed() && !projects.is_changed() {
        return;
    }
    let roots = ProjectQuery::all(workspace.project_root.as_deref(), &projects.roots);
    if roots.is_empty() {
        return;
    }
    index.warm(&roots);
}

fn answer_settled_project_index(
    workspace: Res<CommandBarWorkspaceSnapshot>,
    projects: Res<CommandBarProjectRoots>,
    work: Res<CommandBarWorkSnapshot>,
    browsers: NonSend<Browsers>,
    mut index: ResMut<ProjectIndex>,
    mut paths: ResMut<PathCompletions>,
    mut commands: Commands,
) {
    let pending = index.pending();
    if pending.is_empty() {
        return;
    }
    let bias = RankBias::new(
        ProjectQuery::favoured(
            projects.active.as_deref(),
            workspace.project_root.as_deref(),
        ),
        &work.recent_files,
    );
    for asked in pending {
        if !browsers.can_emit_to(&asked.webview) {
            index.forget(asked.webview);
            paths.cancel(asked.webview);
            continue;
        }
        let roots = ProjectQuery::roots_for(
            &asked.query,
            workspace.project_root.as_deref(),
            &projects.roots,
        );
        if roots.is_empty() {
            continue;
        }
        let Some(completions) = index.settled_for(asked.webview, &roots, &bias) else {
            continue;
        };
        paths.cancel(asked.webview);
        commands.trigger(BinHostEmitEvent::from_event(
            asked.webview,
            &completions.response(),
        ));
    }
}

fn answer_path_completions(
    browsers: NonSend<Browsers>,
    mut paths: ResMut<PathCompletions>,
    mut commands: Commands,
) {
    for (webview, completions) in paths.settled() {
        if browsers.can_emit_to(&webview) {
            commands.trigger(BinHostEmitEvent::from_event(
                webview,
                &completions.response(),
            ));
        }
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

    fn names_a_location(query: &str) -> bool {
        query.starts_with('/') || query.starts_with('~') || query.starts_with('.')
    }
}

#[derive(Resource, Default)]
struct PathCompletions(Vec<PendingPathCompletion>);

impl PathCompletions {
    fn start(&mut self, webview: Entity, query: &str) {
        self.cancel(webview);
        let query = PathQuery(query.to_string());
        let task = IoTaskPool::get().spawn(async move { query.complete() });
        self.0.push(PendingPathCompletion { webview, task });
    }

    fn cancel(&mut self, webview: Entity) {
        self.0.retain(|pending| pending.webview != webview);
    }

    fn settled(&mut self) -> Vec<(Entity, ProjectCompletions)> {
        let mut settled = Vec::new();
        let mut at = 0;
        while at < self.0.len() {
            let completion = block_on(future::poll_once(&mut self.0[at].task));
            let Some(completion) = completion else {
                at += 1;
                continue;
            };
            let pending = self.0.swap_remove(at);
            settled.push((pending.webview, completion));
        }
        settled
    }
}

struct PendingPathCompletion {
    webview: Entity,
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
