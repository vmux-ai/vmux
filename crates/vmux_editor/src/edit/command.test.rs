use super::*;

#[test]
fn selection_range_normalizes_direction() {
    assert_eq!(Selection { anchor: 2, head: 5 }.range(), 2..5);
    assert_eq!(Selection { anchor: 5, head: 2 }.range(), 2..5);
}

#[test]
fn caret_is_empty() {
    assert!(Selection::caret(3).is_empty());
    assert!(!Selection { anchor: 1, head: 2 }.is_empty());
}

#[test]
fn mode_labels() {
    assert_eq!(EditMode::Normal.label(), "NORMAL");
    assert!(EditMode::VisualLine.is_visual());
    assert!(!EditMode::Insert.is_visual());
}
