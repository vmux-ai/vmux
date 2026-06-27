//! Pure, host-testable builders for the Explorer panel view-models: file-tree
//! flattening, open-editors list ops, markdown outline, and LSP symbol
//! flattening. State lives in the native plugin; these functions turn it into
//! the render-ready rows pushed to the dumb Dioxus page.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use vmux_core::event::{FileDirEntry, TreeRow};

/// Depth-first flatten of the cached directory tree into the visible rows.
/// Only directories present in `expanded` have their (cached) children inlined.
pub fn flatten_tree(
    root: &PathBuf,
    expanded: &HashSet<PathBuf>,
    children: &HashMap<PathBuf, Vec<FileDirEntry>>,
) -> Vec<TreeRow> {
    let mut out = Vec::new();
    walk(root, 0, expanded, children, &mut out);
    out
}

fn walk(
    dir: &PathBuf,
    depth: u16,
    expanded: &HashSet<PathBuf>,
    children: &HashMap<PathBuf, Vec<FileDirEntry>>,
    out: &mut Vec<TreeRow>,
) {
    let Some(entries) = children.get(dir) else {
        return;
    };
    for e in entries {
        let p = PathBuf::from(&e.path);
        let is_open = e.is_dir && expanded.contains(&p);
        out.push(TreeRow {
            name: e.name.clone(),
            path: e.path.clone(),
            depth,
            is_dir: e.is_dir,
            expanded: is_open,
        });
        if is_open {
            walk(&p, depth + 1, expanded, children, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, path: &str, is_dir: bool) -> FileDirEntry {
        FileDirEntry {
            name: name.into(),
            path: path.into(),
            is_dir,
        }
    }

    #[test]
    fn expanded_dir_inlines_children() {
        let root = PathBuf::from("/r");
        let mut children = HashMap::new();
        children.insert(
            PathBuf::from("/r"),
            vec![
                entry("src", "/r/src", true),
                entry("a.rs", "/r/a.rs", false),
            ],
        );
        children.insert(
            PathBuf::from("/r/src"),
            vec![entry("b.rs", "/r/src/b.rs", false)],
        );
        let expanded = HashSet::from([PathBuf::from("/r/src")]);
        let rows = flatten_tree(&root, &expanded, &children);
        let got: Vec<_> = rows.iter().map(|r| (r.name.as_str(), r.depth)).collect();
        assert_eq!(got, vec![("src", 0), ("b.rs", 1), ("a.rs", 0)]);
        assert!(rows[0].expanded);
    }

    #[test]
    fn collapsed_dir_hides_children() {
        let root = PathBuf::from("/r");
        let mut children = HashMap::new();
        children.insert(PathBuf::from("/r"), vec![entry("src", "/r/src", true)]);
        children.insert(
            PathBuf::from("/r/src"),
            vec![entry("b.rs", "/r/src/b.rs", false)],
        );
        let rows = flatten_tree(&root, &HashSet::new(), &children);
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].expanded);
    }

    #[test]
    fn missing_cache_yields_no_rows() {
        let rows = flatten_tree(&PathBuf::from("/r"), &HashSet::new(), &HashMap::new());
        assert!(rows.is_empty());
    }
}
