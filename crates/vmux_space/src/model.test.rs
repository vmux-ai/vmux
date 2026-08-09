use super::*;

#[test]
fn space_ids_are_slugged() {
    assert_eq!(normalize_space_id("Client A!"), "client-a");
    assert_eq!(normalize_space_id("  "), "space");
}

#[test]
fn normalize_keeps_slash_as_nested_separator() {
    assert_eq!(normalize_space_id("vmux-ai/vmux"), "vmux-ai/vmux");
    assert_eq!(normalize_space_id("Org Name/Repo!"), "org-name/repo");
    assert_eq!(normalize_space_id("a//b/"), "a/b");
}

#[test]
fn unique_space_id_skips_existing() {
    let existing: std::collections::HashSet<String> = ["work".to_string(), "work-2".to_string()]
        .into_iter()
        .collect();
    assert_eq!(unique_space_id_among(&existing, "Work"), "work-3");
}
