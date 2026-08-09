use super::*;

fn state() -> FoldState {
    let mut s = FoldState::default();
    s.set_regions(vec![
        FoldRegion { start: 1, end: 4 },
        FoldRegion { start: 6, end: 8 },
    ]);
    s
}

#[test]
fn gutter_marks_headers() {
    let mut s = state();
    assert_eq!(s.gutter(1), FoldGutter::Open);
    assert_eq!(s.gutter(2), FoldGutter::None);
    s.close(1);
    assert_eq!(s.gutter(1), FoldGutter::Collapsed);
}

#[test]
fn view_hides_body_only() {
    let mut s = state();
    s.close(1);
    let v = s.view(10);
    assert!(!v.is_hidden(1));
    assert!(v.is_hidden(2) && v.is_hidden(4));
    assert!(!v.is_hidden(5));
    assert_eq!(v.visible_count(), 10 - 3);
    assert_eq!(v.buffer_to_row(5), 5 - 3);
}

#[test]
fn step_rows_skips_hidden() {
    let mut s = state();
    s.close(1);
    let v = s.view(10);
    assert_eq!(v.step_rows(1, 1), 5);
    assert_eq!(v.step_rows(5, -1), 1);
}

#[test]
fn window_returns_visible_lines() {
    let mut s = state();
    s.close(1);
    let v = s.view(10);
    assert_eq!(v.lines_for_window(0, 4), vec![0, 1, 5, 6]);
}

#[test]
fn toggle_recursive_folds_nested() {
    let mut s = FoldState::default();
    s.set_regions(vec![
        FoldRegion { start: 0, end: 9 },
        FoldRegion { start: 2, end: 4 },
    ]);
    s.toggle_recursive(0);
    assert!(s.collapsed.contains(&0) && s.collapsed.contains(&2));
    s.toggle_recursive(0);
    assert!(s.collapsed.is_empty());
}

#[test]
fn reveal_opens_enclosing() {
    let mut s = state();
    s.close(1);
    s.reveal(3);
    assert!(!s.collapsed.contains(&1));
}

#[test]
fn hiding_header_returns_innermost() {
    let mut s = FoldState::default();
    s.set_regions(vec![
        FoldRegion { start: 0, end: 9 },
        FoldRegion { start: 2, end: 5 },
    ]);
    s.close(0);
    s.close(2);
    assert_eq!(s.hiding_header(3), Some(2));
    assert_eq!(s.hiding_header(7), Some(0));
    assert_eq!(s.hiding_header(0), None);
}

#[test]
fn shift_moves_collapsed_starts() {
    let mut s = state();
    s.close(6);
    s.shift(2, 3);
    assert!(s.collapsed.contains(&9));
}

#[test]
fn reconcile_drops_stale() {
    let mut s = state();
    s.close(6);
    s.set_regions(vec![FoldRegion { start: 1, end: 4 }]);
    assert!(!s.collapsed.contains(&6));
}
