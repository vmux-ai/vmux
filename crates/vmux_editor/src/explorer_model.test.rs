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
    let rows = flatten_tree(&root, &expanded, &HashSet::new(), &children);
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
    let rows = flatten_tree(&root, &HashSet::new(), &HashSet::new(), &children);
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].expanded);
}

#[test]
fn missing_cache_yields_no_rows() {
    let rows = flatten_tree(
        &PathBuf::from("/r"),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
    );
    assert!(rows.is_empty());
}

#[test]
fn loading_dir_is_marked() {
    let root = PathBuf::from("/r");
    let src = PathBuf::from("/r/src");
    let children = HashMap::from([(root.clone(), vec![entry("src", "/r/src", true)])]);
    let rows = flatten_tree(
        &root,
        &HashSet::from([src.clone()]),
        &HashSet::from([src]),
        &children,
    );
    assert!(rows[0].expanded);
    assert!(rows[0].loading);
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
