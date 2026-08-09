use super::*;

#[test]
fn yank_populates_unnamed_and_zero() {
    let mut r = Registers::default();
    r.write_yank(None, RegisterValue::charwise("hi"));
    assert_eq!(r.read(None).unwrap().text, "hi");
    assert_eq!(r.read(Some('0')).unwrap().text, "hi");
}

#[test]
fn linewise_delete_shifts_the_numbered_ring() {
    let mut r = Registers::default();
    r.write_delete(None, RegisterValue::linewise("first\n"));
    r.write_delete(None, RegisterValue::linewise("second\n"));
    assert_eq!(r.read(Some('1')).unwrap().text, "second\n");
    assert_eq!(r.read(Some('2')).unwrap().text, "first\n");
    assert_eq!(r.read(None).unwrap().text, "second\n");
}

#[test]
fn small_delete_avoids_the_numbered_ring() {
    let mut r = Registers::default();
    r.write_delete(None, RegisterValue::charwise("ab"));
    assert_eq!(r.read(Some(SMALL_DELETE)).unwrap().text, "ab");
    assert!(r.read(Some('1')).is_none());
}

#[test]
fn blackhole_discards_without_touching_unnamed() {
    let mut r = Registers::default();
    r.write_yank(None, RegisterValue::charwise("keep"));
    r.write_delete(Some(BLACKHOLE), RegisterValue::charwise("gone"));
    assert_eq!(r.read(None).unwrap().text, "keep");
}

#[test]
fn uppercase_register_appends() {
    let mut r = Registers::default();
    r.write_yank(Some('a'), RegisterValue::charwise("one"));
    r.write_yank(Some('A'), RegisterValue::charwise("two"));
    assert_eq!(r.read(Some('a')).unwrap().text, "onetwo");
}

#[test]
fn yank_to_a_named_register_keeps_zero_untouched() {
    let mut r = Registers::default();
    r.write_yank(None, RegisterValue::charwise("zero"));
    r.write_yank(Some('b'), RegisterValue::charwise("named"));
    assert_eq!(r.read(Some('0')).unwrap().text, "zero");
    assert_eq!(r.read(Some('b')).unwrap().text, "named");
    assert_eq!(r.read(None).unwrap().text, "named");
}
