use std::path::PathBuf;

use vmux_api::command_bar::PathEntry;

use super::project_driver::{MAX_RESULTS, ProjectCompletions};

pub(super) struct ProjectQuery;

impl ProjectQuery {
    pub(super) fn roots_for(
        query: &str,
        project_root: Option<&str>,
        registered: &[String],
    ) -> Vec<PathBuf> {
        let query = query.trim();
        if Self::names_a_location(query) {
            return Vec::new();
        }
        Self::all(project_root, registered)
    }

    pub(super) fn favoured<'a>(
        active: Option<&'a str>,
        project_root: Option<&'a str>,
    ) -> Option<&'a str> {
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

    pub(super) fn all(project_root: Option<&str>, registered: &[String]) -> Vec<PathBuf> {
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

    pub(super) fn include(roots: &mut Vec<PathBuf>, additional: &[PathBuf]) {
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

pub(super) struct PathQuery(pub(super) String);

impl PathQuery {
    pub(super) fn complete(self) -> ProjectCompletions {
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
