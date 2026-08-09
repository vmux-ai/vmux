use super::*;

#[test]
fn creates_knowledge_folders() {
    let temp = tempfile::tempdir().unwrap();
    ensure_vault(temp.path()).unwrap();
    for directory in DIRECTORIES {
        assert!(temp.path().join(directory).is_dir());
    }
}

#[test]
fn initializes_knowledge_as_git_repository() {
    let temp = tempfile::tempdir().unwrap();
    ensure_vault(temp.path()).unwrap();
    std::fs::write(temp.path().join("projects/plan.md"), "# Plan\n").unwrap();
    ensure_vault_repository(temp.path()).unwrap();
    assert!(temp.path().join(".git").is_dir());
    assert!(
        vmux_git::runner::file_statuses(temp.path())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn tree_reports_added_modified_deleted_and_directory_status() {
    let temp = tempfile::tempdir().unwrap();
    ensure_vault(temp.path()).unwrap();
    let projects = temp.path().join("projects");
    std::fs::write(projects.join("modified.md"), "old\n").unwrap();
    std::fs::write(projects.join("deleted.md"), "delete\n").unwrap();
    ensure_vault_repository(temp.path()).unwrap();
    std::fs::write(projects.join("modified.md"), "new\n").unwrap();
    std::fs::remove_file(projects.join("deleted.md")).unwrap();
    std::fs::write(projects.join("added.md"), "add\n").unwrap();

    let tree = build_tree(temp.path()).unwrap();
    let status = |name: &str| {
        tree.entries
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.git_status)
            .unwrap()
    };
    assert_eq!(status("added.md"), KnowledgeGitStatus::Added);
    assert_eq!(status("modified.md"), KnowledgeGitStatus::Modified);
    assert_eq!(status("deleted.md"), KnowledgeGitStatus::Deleted);
    assert_eq!(status("projects"), KnowledgeGitStatus::Deleted);
}

#[test]
fn creates_markdown_notes_and_nested_folders_inside_knowledge() {
    let temp = tempfile::tempdir().unwrap();
    ensure_vault(temp.path()).unwrap();
    let folder = create_entry(temp.path(), temp.path(), "Ideas", true).unwrap();
    let note = create_entry(temp.path(), &folder, "Restaurant", false).unwrap();
    assert!(folder.is_dir());
    assert_eq!(note.file_name().unwrap(), "Restaurant.md");
    assert!(note.is_file());
}

#[test]
fn rejects_knowledge_creation_outside_root_and_nested_names() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    ensure_vault(temp.path()).unwrap();
    assert!(create_entry(temp.path(), outside.path(), "note", false).is_err());
    assert!(create_entry(temp.path(), temp.path(), "nested/note", false).is_err());
    assert!(create_entry(temp.path(), temp.path(), ".hidden", false).is_err());
    assert!(create_entry(temp.path(), temp.path(), ".hidden.md", false).is_err());
}

#[test]
fn removes_empty_legacy_folders_and_preserves_content() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("decisions")).unwrap();
    std::fs::create_dir_all(temp.path().join("runbooks")).unwrap();
    std::fs::write(temp.path().join("runbooks/keep.md"), "# Keep").unwrap();
    ensure_vault(temp.path()).unwrap();
    assert!(!temp.path().join("decisions").exists());
    assert!(temp.path().join("runbooks/keep.md").is_file());
}

#[test]
fn builds_sorted_markdown_tree() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Projects/Nested")).unwrap();
    std::fs::write(temp.path().join("z.md"), "---\ntitle: Zed\n---\n").unwrap();
    std::fs::write(temp.path().join("a.txt"), "ignored").unwrap();
    std::fs::write(temp.path().join("Projects/Nested/Brief.MDX"), "# Brief").unwrap();
    let tree = build_tree(temp.path()).unwrap();
    assert!(tree.entries.first().unwrap().is_directory);
    assert!(tree.entries.iter().any(|entry| entry.name == "z.md"));
    assert_eq!(
        tree.entries
            .iter()
            .find(|entry| entry.name == "z.md")
            .unwrap()
            .title,
        "Zed"
    );
    assert!(!tree.entries.iter().any(|entry| entry.name == "a.txt"));
    let projects = tree
        .entries
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case("projects"))
        .unwrap();
    let nested = tree
        .entries
        .iter()
        .find(|entry| entry.parent == projects.path && entry.name == "Nested")
        .unwrap();
    assert!(
        tree.entries
            .iter()
            .any(|entry| entry.parent == nested.path && entry.name == "Brief.MDX")
    );
}

#[cfg(unix)]
#[test]
fn skips_hidden_entries_and_symlinks() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("visible")).unwrap();
    std::fs::create_dir_all(temp.path().join(".hidden")).unwrap();
    std::fs::write(temp.path().join("visible/note.md"), "# Note").unwrap();
    std::os::unix::fs::symlink(
        temp.path().join("visible/note.md"),
        temp.path().join("linked.md"),
    )
    .unwrap();
    let tree = build_tree(temp.path()).unwrap();
    assert!(!tree.entries.iter().any(|entry| entry.name == ".hidden"));
    assert!(!tree.entries.iter().any(|entry| entry.name == "linked.md"));
}

#[cfg(unix)]
#[test]
fn vault_is_private() {
    let temp = tempfile::tempdir().unwrap();
    ensure_vault(temp.path()).unwrap();
    assert_eq!(
        std::fs::metadata(temp.path()).unwrap().mode() & 0o777,
        0o700
    );
}
