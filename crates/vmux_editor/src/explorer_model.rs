//! Pure, host-testable builders for the Explorer panel view-models: file-tree
//! flattening, open-editors list ops, markdown outline, and LSP symbol
//! flattening. State lives in the native plugin; these functions turn it into
//! the render-ready rows pushed to the dumb Dioxus page.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use vmux_core::event::{FileDirEntry, OutlineRow, TreeRow};

/// Depth-first flatten of the cached directory tree into the visible rows.
/// Only directories present in `expanded` have their (cached) children inlined.
pub fn flatten_tree(
    root: &Path,
    expanded: &HashSet<PathBuf>,
    loading: &HashSet<PathBuf>,
    children: &HashMap<PathBuf, Vec<FileDirEntry>>,
) -> Vec<TreeRow> {
    let mut out = Vec::new();
    walk(root, 0, expanded, loading, children, &mut out);
    out
}

fn walk(
    dir: &Path,
    depth: u16,
    expanded: &HashSet<PathBuf>,
    loading: &HashSet<PathBuf>,
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
            loading: loading.contains(&p),
        });
        if is_open {
            walk(&p, depth + 1, expanded, loading, children, out);
        }
    }
}

/// Append `path` to the session open-editors list if not already present,
/// preserving open order (matches VS Code's behaviour).
pub fn note_open(list: &mut Vec<PathBuf>, path: &Path) {
    if !list.iter().any(|p| p.as_path() == path) {
        list.push(path.to_path_buf());
    }
}

/// Remove `path` from the open-editors list; a no-op if absent.
pub fn close(list: &mut Vec<PathBuf>, path: &Path) {
    list.retain(|p| p.as_path() != path);
}

/// Whether `path` is a markdown file (outline comes from the heading scanner
/// rather than LSP `documentSymbol`).
pub fn is_markdown(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
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
#[path = "explorer_model.test.rs"]
mod tests;
