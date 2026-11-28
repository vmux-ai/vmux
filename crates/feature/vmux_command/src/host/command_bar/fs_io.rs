use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use vmux_api::command_bar::{CommandBarWorkDir, PathCompleteResponse, PathEntry};

pub(super) struct CommandBarFsIo;

impl CommandBarFsIo {
    pub(super) fn read_work_entries(cwd: &str) -> Vec<CommandBarWorkDir> {
        let Ok(read) = std::fs::read_dir(cwd) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().is_ok_and(|file_type| file_type.is_dir());
            let path = entry.path().to_string_lossy().to_string();
            rows.push((name, is_dir, path));
        }
        rows.sort_by(|a, b| {
            let a_hidden = a.0.starts_with('.');
            let b_hidden = b.0.starts_with('.');
            b.1.cmp(&a.1)
                .then(a_hidden.cmp(&b_hidden))
                .then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });
        rows.into_iter()
            .map(|(_, is_dir, path)| CommandBarWorkDir { path, is_dir })
            .collect()
    }

    pub(super) fn walk_root(root: &Path, ceiling: usize) -> (Vec<(String, bool)>, bool) {
        let mut entries = Vec::new();
        let mut partial = false;
        let walk = WalkBuilder::new(root)
            .hidden(true)
            .parents(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .follow_links(false)
            .build();
        for entry in walk {
            if entries.len() >= ceiling {
                partial = true;
                break;
            }
            let Ok(entry) = entry else {
                continue;
            };
            if entry.depth() == 0 {
                continue;
            }
            let Some(kind) = entry.file_type() else {
                continue;
            };
            let Ok(relative) = entry.path().strip_prefix(root) else {
                continue;
            };
            entries.push((relative.to_string_lossy().into_owned(), kind.is_dir()));
        }
        (entries, partial)
    }

    pub(super) fn list_directory(
        query: &str,
        request_id: u64,
        max_results: usize,
    ) -> PathCompleteResponse {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let (parent, prefix) = if let Some(position) = query.rfind('/') {
            (&query[..=position], &query[position + 1..])
        } else {
            ("", query)
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
            return PathCompleteResponse {
                request_id,
                completions: Vec::new(),
                truncated: false,
                total: 0,
            };
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
        let total = u32::try_from(results.len()).unwrap_or(u32::MAX);
        results.truncate(max_results);
        PathCompleteResponse {
            request_id,
            completions: results,
            truncated: false,
            total,
        }
    }
}
