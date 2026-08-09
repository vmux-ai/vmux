use super::*;

#[test]
fn vscode_wrap_modes_resolve_columns() {
    assert_eq!(resolve_columns(WordWrap::Off, 120, 80), 0);
    assert_eq!(resolve_columns(WordWrap::On, 120, 80), 120);
    assert_eq!(resolve_columns(WordWrap::WordWrapColumn, 120, 80), 80);
    assert_eq!(resolve_columns(WordWrap::Bounded, 120, 80), 80);
    assert_eq!(resolve_columns(WordWrap::Bounded, 60, 80), 60);
}

#[test]
fn wrapped_rows_drive_cursor_and_selection_geometry() {
    let rope = Rope::from_str("abcdefghijkl\nshort\n");
    let folds = crate::fold::FoldState::default().view(rope.len_lines() as u32);
    let view = WrapView::new(&rope, &folds, WordWrap::On, 5, 80);

    assert_eq!(view.total_rows(), 5);
    assert_eq!(view.position(0, 7), (1, 2));
    assert_eq!(
        view.selections([SelSpan {
            line: 0,
            row: 0,
            start: 3,
            end: 11,
        }]),
        vec![
            SelSpan {
                line: 0,
                row: 0,
                start: 3,
                end: u32::MAX,
            },
            SelSpan {
                line: 0,
                row: 1,
                start: 0,
                end: u32::MAX,
            },
            SelSpan {
                line: 0,
                row: 2,
                start: 0,
                end: 1,
            },
        ]
    );
}
