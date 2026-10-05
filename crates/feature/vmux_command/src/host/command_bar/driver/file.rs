use vmux_api::command_bar::PathEntry;

use super::results::CommandBarResultItem;
use super::{PaletteDraft, PaletteQuery};

pub(crate) struct CompletionQuery;

impl CompletionQuery {
    pub fn parse(input: &str) -> Option<String> {
        let trimmed = input.trim();
        if let Some(rest) = trimmed.strip_prefix("file://") {
            return Some(rest.to_string());
        }
        if PaletteQuery::new(trimmed).looks_like_path() {
            return Some(trimmed.to_string());
        }
        if trimmed.is_empty() || trimmed.contains("://") || PaletteQuery::new(trimmed).is_data_uri()
        {
            return None;
        }
        Some(trimmed.to_string())
    }

    pub fn names_a_file(value: &str) -> bool {
        for term in value.split_whitespace() {
            if term.contains('/') {
                return true;
            }
            let Some((stem, extension)) = term.rsplit_once('.') else {
                continue;
            };
            if stem.is_empty() || extension.is_empty() || extension.len() > 5 {
                continue;
            }
            if extension.chars().all(|c| c.is_ascii_alphanumeric()) {
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Completions<'a> {
    pub entries: &'a [PathEntry],
    pub partial: bool,
    pub total: usize,
}

impl<'a> Completions<'a> {
    pub fn for_query(draft: &'a PaletteDraft, query: &str) -> Self {
        if CompletionQuery::parse(query).is_none() {
            return Self::default();
        }
        Self {
            entries: &draft.completions,
            partial: draft.completions_partial,
            total: draft.completions_total,
        }
    }

    fn withheld(&self) -> usize {
        self.total.saturating_sub(self.entries.len())
    }
}

pub(crate) struct FileRows;

impl FileRows {
    pub fn merge(
        query: &str,
        completions: Completions<'_>,
        matched: Vec<CommandBarResultItem>,
    ) -> Vec<CommandBarResultItem> {
        if completions.entries.is_empty() && !completions.partial {
            return matched;
        }
        let trimmed = query.trim();
        let leads =
            PaletteQuery::new(trimmed).looks_like_path() || CompletionQuery::names_a_file(trimmed);
        let mut files = Vec::with_capacity(completions.entries.len());
        let mut listed = Vec::with_capacity(completions.entries.len());
        for entry in completions.entries {
            files.push(CommandBarResultItem::File {
                path: entry.full_path.clone(),
                is_dir: entry.is_dir,
                project: entry.project.clone(),
                relative: entry.name.clone(),
            });
            listed.push(entry.full_path.as_str());
        }
        let mut rest = Vec::with_capacity(matched.len());
        for item in matched {
            if Self::already_listed(&item, &listed) {
                continue;
            }
            rest.push(item);
        }
        let mut merged = if leads {
            files.extend(rest);
            files
        } else {
            let mut ahead = Vec::with_capacity(rest.len());
            let mut fallbacks = Vec::new();
            for item in rest {
                match item {
                    CommandBarResultItem::Search { .. } => fallbacks.push(item),
                    _ => ahead.push(item),
                }
            }
            ahead.extend(files);
            ahead.extend(fallbacks);
            ahead
        };
        if completions.partial {
            merged.push(CommandBarResultItem::PartialIndex);
        }
        if completions.withheld() > 0 {
            merged.push(CommandBarResultItem::MoreMatches {
                shown: completions.entries.len(),
                total: completions.total,
            });
        }
        merged
    }

    fn already_listed(item: &CommandBarResultItem, listed: &[&str]) -> bool {
        let Some(path) = Self::local_path(item) else {
            return false;
        };
        listed.contains(&path)
    }

    fn local_path(item: &CommandBarResultItem) -> Option<&str> {
        match item {
            CommandBarResultItem::RecentFile { url, .. }
            | CommandBarResultItem::History { url, .. } => url.strip_prefix("file://"),
            _ => None,
        }
    }

    pub fn under_projects(
        items: Vec<CommandBarResultItem>,
        projects: &[String],
    ) -> Vec<CommandBarResultItem> {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            let Some(path) = Self::local_path(&item) else {
                out.push(item);
                continue;
            };
            let Some((project, relative)) = ProjectPath::split(path, projects) else {
                out.push(item);
                continue;
            };
            out.push(CommandBarResultItem::File {
                path: path.to_string(),
                is_dir: false,
                project,
                relative,
            });
        }
        out
    }
}

pub(super) struct ProjectPath;

impl ProjectPath {
    pub fn split(path: &str, projects: &[String]) -> Option<(String, String)> {
        let mut owner = "";
        for project in projects {
            let root = project.trim().trim_end_matches('/');
            if root.is_empty() || root.len() <= owner.len() {
                continue;
            }
            let Some(rest) = path.strip_prefix(root) else {
                continue;
            };
            if !rest.starts_with('/') {
                continue;
            }
            owner = root;
        }
        if owner.is_empty() {
            return None;
        }
        let label = owner.rsplit('/').next().unwrap_or(owner);
        Some((label.to_string(), path[owner.len() + 1..].to_string()))
    }
}
