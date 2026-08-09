use super::*;
use crate::event::StyledSpan;

fn line(kind: DiffKind, no: u32) -> DiffLine {
    DiffLine {
        kind,
        old_no: Some(no),
        new_no: Some(no),
        hunk: None,
        spans: Vec::<StyledSpan>::new(),
    }
}

#[test]
fn collapses_context_outside_changed_hunks() {
    let mut lines = (1..=20)
        .map(|no| line(DiffKind::Context, no))
        .collect::<Vec<_>>();
    lines[9].kind = DiffKind::Add;

    let rows = diff_view_rows(&lines, &HashSet::new());

    assert_eq!(rows.first(), Some(&DiffViewRow::Gap { start: 0, end: 6 }));
    assert_eq!(rows.last(), Some(&DiffViewRow::Gap { start: 13, end: 20 }));
    assert!(rows.contains(&DiffViewRow::Line(9)));
}

#[test]
fn expands_selected_context_gap() {
    let mut lines = (1..=20)
        .map(|no| line(DiffKind::Context, no))
        .collect::<Vec<_>>();
    lines[9].kind = DiffKind::Add;
    let expanded = HashSet::from([(0, 6)]);

    let rows = diff_view_rows(&lines, &expanded);

    assert_eq!(rows.first(), Some(&DiffViewRow::Line(0)));
    assert!(!rows.contains(&DiffViewRow::Gap { start: 0, end: 6 }));
    assert!(rows.contains(&DiffViewRow::Gap { start: 13, end: 20 }));
}

#[test]
fn editor_markers_classify_modified_added_and_deleted_lines() {
    let lines = vec![
        DiffLine {
            kind: DiffKind::Remove,
            old_no: Some(2),
            new_no: None,
            hunk: None,
            spans: Vec::new(),
        },
        DiffLine {
            kind: DiffKind::Add,
            old_no: None,
            new_no: Some(2),
            hunk: None,
            spans: Vec::new(),
        },
        line(DiffKind::Context, 3),
        DiffLine {
            kind: DiffKind::Add,
            old_no: None,
            new_no: Some(8),
            hunk: Some(1),
            spans: Vec::new(),
        },
        DiffLine {
            kind: DiffKind::Remove,
            old_no: Some(12),
            new_no: None,
            hunk: Some(2),
            spans: Vec::new(),
        },
        line(DiffKind::Context, 12),
    ];

    let markers = editor_diff_markers(&lines);

    assert_eq!(markers.get(&2), Some(&EditorDiffMarker::Modified));
    assert_eq!(markers.get(&8), Some(&EditorDiffMarker::Added));
    assert_eq!(markers.get(&12), Some(&EditorDiffMarker::Deleted));
}
