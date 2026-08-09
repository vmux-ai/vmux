use super::*;

const OUT: &str = "# branch.oid abc123\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n1 .M N... 100644 100644 100644 aaa bbb src/main.rs\n1 M. N... 100644 100644 100644 ccc ddd src/lib.rs\n? notes.txt\n";

#[test]
fn parses_branch_and_ahead_behind() {
    let p = parse_porcelain_v2(OUT, "src/main.rs");
    assert_eq!(p.branch, "main");
    assert_eq!(p.ahead, 2);
    assert_eq!(p.behind, 1);
    assert!(p.has_upstream);
}

#[test]
fn target_unstaged_modified() {
    assert_eq!(
        parse_porcelain_v2(OUT, "src/main.rs").file_status,
        FileStatus::Modified
    );
}

#[test]
fn target_staged() {
    assert_eq!(
        parse_porcelain_v2(OUT, "src/lib.rs").file_status,
        FileStatus::Staged
    );
}

#[test]
fn target_untracked() {
    assert_eq!(
        parse_porcelain_v2(OUT, "notes.txt").file_status,
        FileStatus::Untracked
    );
}

#[test]
fn target_clean_when_absent() {
    assert_eq!(
        parse_porcelain_v2(OUT, "README.md").file_status,
        FileStatus::Clean
    );
}

#[test]
fn staged_count_counts_staged_column() {
    assert_eq!(parse_porcelain_v2(OUT, "src/main.rs").staged_count, 1);
}

#[test]
fn no_upstream_header() {
    let out = "# branch.head feature\n";
    let p = parse_porcelain_v2(out, "x");
    assert!(!p.has_upstream);
    assert_eq!(p.ahead, 0);
    assert_eq!(p.behind, 0);
}

#[test]
fn changed_paths_collects_all_entry_kinds() {
    let out = "# branch.head main\n\
1 .M N... 100644 100644 100644 aaa bbb src/main.rs\n\
1 M. N... 100644 100644 100644 ccc ddd src/lib.rs\n\
2 R. N... 100644 100644 100644 eee fff R100 new.rs\told.rs\n\
u UU N... 100644 100644 100644 100644 ggg hhh iii conflict.rs\n\
? notes.txt\n";
    let set = changed_paths(out);
    assert!(set.contains("src/main.rs"));
    assert!(set.contains("src/lib.rs"));
    assert!(set.contains("new.rs"));
    assert!(!set.contains("old.rs"));
    assert!(set.contains("conflict.rs"));
    assert!(set.contains("notes.txt"));
    assert_eq!(set.len(), 5);
}
