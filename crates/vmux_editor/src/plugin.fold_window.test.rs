use crate::fold::{FoldState, indent_regions};
use ropey::Rope;

#[test]
fn collapsed_region_hidden_from_window() {
    let r = Rope::from_str("fn a() {\n    x;\n    y;\n}\nz;\n");
    let mut folds = FoldState::default();
    folds.set_regions(indent_regions(&r));
    folds.close(0);
    let view = folds.view(r.len_lines() as u32);
    let visible = view.lines_for_window(0, view.visible_count());
    assert!(visible.contains(&0));
    assert!(!visible.contains(&1) && !visible.contains(&2));
    assert!(visible.contains(&3));
}
