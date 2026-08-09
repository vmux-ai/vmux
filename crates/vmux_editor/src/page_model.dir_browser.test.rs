use super::*;

fn entry(path: &str, is_dir: bool) -> FileDirEntry {
    FileDirEntry {
        name: path.rsplit('/').next().unwrap().to_string(),
        path: path.to_string(),
        is_dir,
    }
}

#[test]
fn classify_dir_and_image_and_text() {
    assert_eq!(classify("/a/b", true), ContentClass::Dir);
    assert_eq!(
        classify("/a/p.PNG", false),
        ContentClass::Image {
            mime: "image/png".into()
        }
    );
    assert_eq!(classify("/a/main.rs", false), ContentClass::Text);
    assert_eq!(classify("/a/blob", false), ContentClass::Other);
}

#[test]
fn clamp_selection_bounds() {
    assert_eq!(clamp_selection(5, 3), 2);
    assert_eq!(clamp_selection(0, 0), 0);
    assert_eq!(clamp_selection(1, 3), 1);
}

#[test]
fn dir_select_index_matches_came_from_by_basename() {
    let parent = vec![
        entry("/a/x", true),
        entry("/a/.worktrees", true),
        entry("/a/y", false),
    ];
    assert_eq!(dir_select_index(&parent, "/a/.worktrees"), 1);
    assert_eq!(dir_select_index(&parent, "a/.worktrees/"), 1);
    assert_eq!(dir_select_index(&parent, "~/proj/a/.worktrees"), 1);
    assert_eq!(dir_select_index(&parent, "/a/zzz"), 0);
    assert_eq!(dir_select_index(&parent, ""), 0);
}
