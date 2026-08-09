use super::*;

#[test]
fn set_get_roundtrip_and_zero_removes() {
    let mut s = FoldStore::default();
    let p = Path::new("/tmp/vmux-fold-test.rs");
    s.set(p, &[3, 1]);
    assert_eq!(s.get(p), vec![1, 3]);
    s.set(p, &[]);
    assert!(s.get(p).is_empty());
}
