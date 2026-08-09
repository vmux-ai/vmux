use super::*;

#[test]
fn parses_file_url_stripping_scheme_fragment_and_encoding() {
    assert_eq!(
        path_from_file_url("file:///a/b.rs#L3:1-4"),
        Some(PathBuf::from("/a/b.rs"))
    );
    assert_eq!(
        path_from_file_url("file:///a/my%20file.rs"),
        Some(PathBuf::from("/a/my file.rs"))
    );
    assert_eq!(
        path_from_file_url("file:/rel#x"),
        Some(PathBuf::from("/rel"))
    );
    assert_eq!(path_from_file_url("https://x/y"), None);
    assert_eq!(path_from_file_url("file://"), None);
}

#[test]
fn decide_closable_below_threshold_is_empty() {
    let mut w = World::new();
    let ids: Vec<Entity> = (0..3).map(|_| w.spawn_empty().id()).collect();
    let stacks = vec![
        (ids[0], 10, false),
        (ids[1], 20, false),
        (ids[2], 30, false),
    ];
    assert!(decide_closable(&stacks, 5).is_empty());
}

#[test]
fn decide_closable_keeps_changed_and_active() {
    let mut w = World::new();
    let ids: Vec<Entity> = (0..6).map(|_| w.spawn_empty().id()).collect();
    // active = ids[5] (max ts); changed = ids[1], ids[3]; keep those, close the rest.
    let stacks = vec![
        (ids[0], 10, false),
        (ids[1], 20, true),
        (ids[2], 30, false),
        (ids[3], 40, true),
        (ids[4], 50, false),
        (ids[5], 60, false),
    ];
    let mut got = decide_closable(&stacks, 5);
    got.sort();
    let mut want = vec![ids[0], ids[2], ids[4]];
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn decide_closable_empty_when_all_changed() {
    let mut w = World::new();
    let ids: Vec<Entity> = (0..6).map(|_| w.spawn_empty().id()).collect();
    let stacks: Vec<(Entity, i64, bool)> = ids
        .iter()
        .enumerate()
        .map(|(i, &e)| (e, i as i64, true))
        .collect();
    assert!(decide_closable(&stacks, 5).is_empty());
}
