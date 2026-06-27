//! Pure, host-testable builders for the Explorer panel view-models: file-tree
//! flattening, open-editors list ops, markdown outline, and LSP symbol
//! flattening. State lives in the native plugin; these functions turn it into
//! the render-ready rows pushed to the dumb Dioxus page.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use vmux_core::event::{FileDirEntry, OutlineRow, TreeRow};

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

/// Append `path` to the session open-editors list if not already present,
/// preserving open order (matches VS Code's behaviour).
pub fn note_open(list: &mut Vec<PathBuf>, path: &PathBuf) {
    if !list.contains(path) {
        list.push(path.clone());
    }
}

/// Remove `path` from the open-editors list; a no-op if absent.
pub fn close(list: &mut Vec<PathBuf>, path: &PathBuf) {
    list.retain(|p| p != path);
}

/// Whether `path` is a markdown file (outline comes from the heading scanner
/// rather than LSP `documentSymbol`).
pub fn is_markdown(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md") | Some("markdown")
    )
}

/// Parse markdown ATX headings (`#`..`######`) into outline rows, ignoring
/// headings inside fenced code blocks. `kind = 15` is the LSP `String` symbol
/// kind (the `abc` glyph); `depth = heading level - 1`.
pub fn markdown_outline(text: &str) -> Vec<OutlineRow> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let hashes = t.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            out.push(OutlineRow {
                name: t[hashes..].trim().to_string(),
                kind: 15,
                line: i as u32,
                depth: (hashes - 1) as u16,
            });
        }
    }
    out
}

/// Flatten an LSP `textDocument/documentSymbol` response into outline rows.
/// Handles both the hierarchical `DocumentSymbol[]` shape (recursing
/// `children`) and the flat `SymbolInformation[]` shape (`location`).
pub fn flatten_symbols(value: &serde_json::Value) -> Vec<OutlineRow> {
    let mut out = Vec::new();
    if let Some(arr) = value.as_array() {
        for item in arr {
            push_symbol(item, 0, &mut out);
        }
    }
    out
}

fn push_symbol(item: &serde_json::Value, depth: u16, out: &mut Vec<OutlineRow>) {
    let name = item
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return;
    }
    let kind = item.get("kind").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
    out.push(OutlineRow {
        name,
        kind,
        line: symbol_line(item),
        depth,
    });
    if let Some(children) = item.get("children").and_then(|v| v.as_array()) {
        for c in children {
            push_symbol(c, depth + 1, out);
        }
    }
}

fn symbol_line(item: &serde_json::Value) -> u32 {
    if let Some(line) = item
        .get("selectionRange")
        .or_else(|| item.get("range"))
        .and_then(|r| r.pointer("/start/line"))
        .and_then(|v| v.as_u64())
    {
        return line as u32;
    }
    item.pointer("/location/range/start/line")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32
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

    #[test]
    fn note_open_dedups_and_preserves_order() {
        let mut list = Vec::new();
        note_open(&mut list, &PathBuf::from("/a"));
        note_open(&mut list, &PathBuf::from("/b"));
        note_open(&mut list, &PathBuf::from("/a"));
        assert_eq!(list, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    #[test]
    fn close_removes_and_absent_is_noop() {
        let mut list = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        close(&mut list, &PathBuf::from("/a"));
        assert_eq!(list, vec![PathBuf::from("/b")]);
        close(&mut list, &PathBuf::from("/zzz"));
        assert_eq!(list, vec![PathBuf::from("/b")]);
    }

    #[test]
    fn markdown_outline_levels_and_lines() {
        let md = "# Title\nintro\n## Install\n### Step\n#nospace\n";
        let rows = markdown_outline(md);
        let got: Vec<_> = rows
            .iter()
            .map(|r| (r.name.as_str(), r.depth, r.line))
            .collect();
        assert_eq!(
            got,
            vec![("Title", 0, 0), ("Install", 1, 2), ("Step", 2, 3)]
        );
        assert!(rows.iter().all(|r| r.kind == 15));
    }

    #[test]
    fn markdown_outline_ignores_headings_in_fences() {
        let md = "# Real\n```\n# Fake\n```\n## After\n";
        let names: Vec<_> = markdown_outline(md).into_iter().map(|r| r.name).collect();
        assert_eq!(names, vec!["Real".to_string(), "After".to_string()]);
    }

    #[test]
    fn flatten_symbols_hierarchical() {
        let v = serde_json::json!([
            {
                "name": "Foo",
                "kind": 5,
                "range": { "start": { "line": 2, "character": 0 }, "end": { "line": 9, "character": 0 } },
                "selectionRange": { "start": { "line": 2, "character": 6 }, "end": { "line": 2, "character": 9 } },
                "children": [
                    { "name": "bar", "kind": 6, "selectionRange": { "start": { "line": 4, "character": 4 } } }
                ]
            }
        ]);
        let rows = flatten_symbols(&v);
        let got: Vec<_> = rows
            .iter()
            .map(|r| (r.name.as_str(), r.kind, r.line, r.depth))
            .collect();
        assert_eq!(got, vec![("Foo", 5, 2, 0), ("bar", 6, 4, 1)]);
    }

    #[test]
    fn flatten_symbols_flat() {
        let v = serde_json::json!([
            { "name": "main", "kind": 12, "location": { "uri": "file:///x", "range": { "start": { "line": 7, "character": 0 } } } }
        ]);
        let rows = flatten_symbols(&v);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "main");
        assert_eq!(rows[0].kind, 12);
        assert_eq!(rows[0].line, 7);
        assert_eq!(rows[0].depth, 0);
    }
}
