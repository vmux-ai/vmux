use super::*;
use std::fs;

#[test]
fn lists_dir_includes_dotfiles_dirs_first() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("zdir")).unwrap();
    fs::write(tmp.path().join("a.txt"), "x").unwrap();
    fs::write(tmp.path().join(".hidden"), "x").unwrap();
    let entries = list_dir(tmp.path());
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["zdir", ".hidden", "a.txt"]);
}

#[test]
fn project_root_walks_up_to_git_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join(".git")).unwrap();
    let sub = root.join("crates").join("x");
    fs::create_dir_all(&sub).unwrap();
    let file = sub.join("lib.rs");
    fs::write(&file, "x").unwrap();
    assert_eq!(project_root(&file), root);
    assert_eq!(project_root(&sub), root);
}

#[test]
fn project_root_falls_back_to_containing_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let sub = tmp.path().join("nogit");
    fs::create_dir(&sub).unwrap();
    let file = sub.join("a.txt");
    fs::write(&file, "x").unwrap();
    assert_eq!(project_root(&file), sub);
}

#[test]
fn project_root_uses_full_knowledge_vault() {
    let tmp = tempfile::tempdir().unwrap();
    let knowledge = tmp.path().join("knowledge");
    let projects = knowledge.join("projects");
    fs::create_dir_all(&projects).unwrap();
    let file = projects.join("note.md");
    fs::write(&file, "# Note").unwrap();
    assert_eq!(project_root_with_knowledge(&file, &knowledge), knowledge);
}

#[test]
fn parent_listing_of_nested_is_some_root_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let child = tmp.path().join("child");
    fs::create_dir(&child).unwrap();
    let (pp, pe) = parent_listing(&child);
    assert_eq!(pp, tmp.path().to_string_lossy());
    assert!(pe.iter().any(|e| e.name == "child"));

    let (rp, re) = parent_listing(Path::new("/"));
    assert!(rp.is_empty());
    assert!(re.is_empty());
}
