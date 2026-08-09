use super::*;

#[test]
fn classifies_core_four_kinds() {
    assert_eq!(page_kind_for_url("vmux://agent/vibe/abc"), PageKind::Agent);
    assert_eq!(page_kind_for_url("vmux://terminal/123"), PageKind::Terminal);
    assert_eq!(page_kind_for_url("file:///x.rs"), PageKind::File);
    assert_eq!(page_kind_for_url("https://example.com"), PageKind::Browser);
    assert_eq!(page_kind_for_url("vmux://services/"), PageKind::Browser);
    assert_eq!(page_kind_for_url("vmux://spaces/"), PageKind::Browser);
}

fn e(n: u64) -> Entity {
    Entity::from_bits(n)
}

fn leaf(pane: u64, kinds: &[PageKind], seq: u64, size: (f32, f32)) -> LeafInfo {
    LeafInfo {
        pane: e(pane),
        kinds: kinds.to_vec(),
        spawn_seq: seq,
        size: Vec2::new(size.0, size.1),
    }
}

#[test]
fn exact_url_reuse_wins() {
    let hit = ReuseHit {
        tab: e(1),
        stack: e(2),
    };
    let got = resolve_placement(
        "https://x.com",
        Some(hit),
        &[leaf(10, &[PageKind::Browser], 5, (800.0, 600.0))],
        e(10),
    );
    assert_eq!(
        got,
        Placement::Focus {
            tab: e(1),
            stack: e(2)
        }
    );
}

#[test]
fn same_type_adds_tab_no_split() {
    let got = resolve_placement(
        "https://b.com",
        None,
        &[leaf(10, &[PageKind::Browser], 5, (800.0, 600.0))],
        e(10),
    );
    assert_eq!(got, Placement::AddTab { pane: e(10) });
}

#[test]
fn same_type_uses_newest_matching_bucket() {
    let got = resolve_placement(
        "vmux://terminal/",
        None,
        &[
            leaf(10, &[PageKind::Terminal], 1, (800.0, 600.0)),
            leaf(20, &[PageKind::Terminal], 9, (800.0, 600.0)),
            leaf(30, &[PageKind::File], 12, (800.0, 600.0)),
        ],
        e(1),
    );
    assert_eq!(got, Placement::AddTab { pane: e(20) });
}

#[test]
fn same_type_prefers_pure_bucket_over_newer_mixed_bucket() {
    let got = resolve_placement(
        "file:///b.rs",
        None,
        &[
            leaf(10, &[PageKind::File], 1, (800.0, 600.0)),
            leaf(20, &[PageKind::File, PageKind::Terminal], 9, (800.0, 600.0)),
        ],
        e(1),
    );
    assert_eq!(got, Placement::AddTab { pane: e(10) });
}

#[test]
fn same_type_does_not_add_to_mixed_bucket_when_no_pure_bucket_exists() {
    let got = resolve_placement(
        "https://b.com",
        None,
        &[leaf(
            20,
            &[PageKind::File, PageKind::Browser],
            9,
            (900.0, 400.0),
        )],
        e(20),
    );
    assert_eq!(
        got,
        Placement::Spiral {
            anchor: e(20),
            axis: PaneSplitDirection::Row
        }
    );
}

#[test]
fn forced_split_uses_newest_nonagent_leaf() {
    let got = resolve_split_anchor(
        &[
            leaf(10, &[PageKind::Terminal], 9, (800.0, 600.0)),
            leaf(20, &[PageKind::Browser], 12, (800.0, 600.0)),
            leaf(30, &[PageKind::Agent], 50, (800.0, 600.0)),
        ],
        e(30),
    );

    assert_eq!(got, e(20));
}

#[test]
fn first_page_fills_empty_leaf() {
    let got = resolve_placement(
        "https://b.com",
        None,
        &[leaf(10, &[], 1, (800.0, 600.0))],
        e(10),
    );
    assert_eq!(got, Placement::AddTab { pane: e(10) });
}

#[test]
fn new_type_splits_newest_nonagent_leaf_along_longer_side() {
    let leaves = [
        leaf(1, &[PageKind::Agent], 1, (800.0, 900.0)),
        leaf(2, &[PageKind::File], 9, (900.0, 400.0)),
    ];
    let got = resolve_placement("https://b.com", None, &leaves, e(1));
    assert_eq!(
        got,
        Placement::Spiral {
            anchor: e(2),
            axis: PaneSplitDirection::Row
        }
    );
}

#[test]
fn first_file_splits_newest_terminal_when_no_file_bucket_exists() {
    let leaves = [
        leaf(1, &[PageKind::Agent], 1, (800.0, 900.0)),
        leaf(2, &[PageKind::Browser], 10, (900.0, 400.0)),
        leaf(3, &[PageKind::Terminal], 20, (900.0, 400.0)),
    ];
    let got = resolve_placement("file:///repo/README.md", None, &leaves, e(1));
    assert_eq!(
        got,
        Placement::Spiral {
            anchor: e(3),
            axis: PaneSplitDirection::Row
        }
    );
}

#[test]
fn first_terminal_splits_browser_into_top_and_bottom() {
    let leaves = [
        leaf(1, &[PageKind::Agent], 1, (800.0, 900.0)),
        leaf(2, &[PageKind::Browser], 10, (900.0, 400.0)),
    ];
    let got = resolve_placement("vmux://terminal/", None, &leaves, e(1));
    assert_eq!(
        got,
        Placement::Spiral {
            anchor: e(2),
            axis: PaneSplitDirection::Column
        }
    );
}

#[test]
fn new_type_splits_tall_leaf_into_column() {
    let leaves = [leaf(2, &[PageKind::File], 9, (400.0, 900.0))];
    let got = resolve_placement("https://b.com", None, &leaves, e(2));
    assert_eq!(
        got,
        Placement::Spiral {
            anchor: e(2),
            axis: PaneSplitDirection::Column
        }
    );
}

#[test]
fn agent_page_never_splits_when_agent_pane_exists() {
    let leaves = [
        leaf(1, &[PageKind::Agent], 1, (800.0, 900.0)),
        leaf(2, &[PageKind::Browser], 9, (900.0, 400.0)),
    ];
    let got = resolve_placement("vmux://agent/vibe/x", None, &leaves, e(2));
    assert_eq!(got, Placement::AddTab { pane: e(1) });
}

#[test]
fn nonagent_page_bootstraps_by_splitting_agent_when_only_leaf() {
    let leaves = [leaf(1, &[PageKind::Agent], 1, (1600.0, 900.0))];
    let got = resolve_placement("https://b.com", None, &leaves, e(1));
    assert_eq!(
        got,
        Placement::Spiral {
            anchor: e(1),
            axis: PaneSplitDirection::Row
        }
    );
}

#[test]
fn agent_page_bootstraps_by_splitting_newest_nonagent_when_no_agent_pane() {
    let leaves = [leaf(2, &[PageKind::Browser], 9, (400.0, 900.0))];
    let got = resolve_placement("vmux://agent/vibe/x", None, &leaves, e(2));
    assert_eq!(
        got,
        Placement::Spiral {
            anchor: e(2),
            axis: PaneSplitDirection::Column
        }
    );
}
