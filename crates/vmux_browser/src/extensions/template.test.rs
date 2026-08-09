use super::*;

#[test]
fn renders_each_placeholder_exactly_once() {
    assert_eq!(
        render("x=__VMUX_X__", &[("__VMUX_X__", "1".into())]).unwrap(),
        "x=1"
    );
    assert!(render("x", &[("__VMUX_X__", "1".into())]).is_err());
    assert!(render("__VMUX_X____VMUX_X__", &[("__VMUX_X__", "1".into())]).is_err());
    assert!(render("__VMUX_Y__", &[]).is_err());
}
