use super::*;

#[test]
fn creates_renames_and_deletes_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let file = create_entry(root, root, "a.txt", false).unwrap();
    assert!(file.is_file());
    let (renamed, is_dir) = rename_entry(root, &file, "b.txt").unwrap();
    assert!(!is_dir);
    assert!(renamed.is_file());
    delete_entry(root, &renamed).unwrap();
    assert!(!renamed.exists());

    let dir = create_entry(root, root, "src", true).unwrap();
    std::fs::write(dir.join("lib.rs"), "").unwrap();
    let (renamed, is_dir) = rename_entry(root, &dir, "source").unwrap();
    assert!(is_dir);
    delete_entry(root, &renamed).unwrap();
    assert!(!renamed.exists());
}

#[test]
fn rejects_nested_names_and_outside_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    assert!(create_entry(tmp.path(), tmp.path(), "a/b", false).is_err());
    assert!(create_entry(tmp.path(), outside.path(), "x", false).is_err());
}

#[test]
fn rejects_root_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(rename_entry(tmp.path(), tmp.path(), "other").is_err());
    assert!(delete_entry(tmp.path(), tmp.path()).is_err());
}
