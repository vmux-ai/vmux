use super::*;

#[test]
fn parses_simple_path() {
    assert_eq!(
        path_from_files_url("file:///Users/me/src/main.rs"),
        Some(PathBuf::from("/Users/me/src/main.rs"))
    );
}

#[test]
fn decodes_percent_escapes() {
    assert_eq!(
        path_from_files_url("file:///Users/me/a%20b.rs"),
        Some(PathBuf::from("/Users/me/a b.rs"))
    );
}

#[test]
fn rejects_non_files_scheme() {
    assert_eq!(path_from_files_url("vmux://terminal/"), None);
}

#[test]
fn empty_path_is_root() {
    assert_eq!(path_from_files_url("file:///"), Some(PathBuf::from("/")));
}
