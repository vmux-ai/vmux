use super::*;

fn lines(n: u32) -> Vec<DiffLine> {
    (0..n)
        .map(|i| DiffLine {
            kind: DiffKind::Context,
            old_no: Some(i),
            new_no: Some(i),
            hunk: None,
            spans: vec![],
        })
        .collect()
}

#[test]
fn returns_total_and_slice() {
    let (total, win) = window(&lines(10), 2, 3);
    assert_eq!(total, 10);
    assert_eq!(win.len(), 3);
    assert_eq!(win[0].old_no, Some(2));
}

#[test]
fn clamps_at_bottom() {
    let (total, win) = window(&lines(10), 8, 5);
    assert_eq!(total, 10);
    assert_eq!(win.len(), 2);
}

#[test]
fn top_past_end_is_empty() {
    let (_, win) = window(&lines(3), 99, 5);
    assert!(win.is_empty());
}

#[test]
fn empty_input() {
    let (total, win) = window(&[], 0, 5);
    assert_eq!(total, 0);
    assert!(win.is_empty());
}
