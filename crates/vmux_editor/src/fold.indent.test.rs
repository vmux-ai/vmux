use super::*;

#[test]
fn folds_indented_block() {
    let r = Rope::from_str("fn a() {\n    x;\n    y;\n}\nz;\n");
    let regs = indent_regions(&r);
    assert!(regs.contains(&FoldRegion { start: 0, end: 2 }));
}

#[test]
fn excludes_trailing_blanks() {
    let r = Rope::from_str("a:\n  b\n\n\nc\n");
    let regs = indent_regions(&r);
    assert_eq!(regs, vec![FoldRegion { start: 0, end: 1 }]);
}

#[test]
fn nests_deeper_blocks() {
    let r = Rope::from_str("a:\n  b:\n    c\n  d\ne\n");
    let regs = indent_regions(&r);
    assert!(regs.contains(&FoldRegion { start: 0, end: 3 }));
    assert!(regs.contains(&FoldRegion { start: 1, end: 2 }));
}
