use super::*;

#[test]
fn diffline_rkyv_roundtrips() {
    let line = DiffLine {
        kind: DiffKind::Add,
        old_no: None,
        new_no: Some(7),
        hunk: Some(2),
        spans: vec![StyledSpan {
            text: "x".into(),
            fg: [1, 2, 3],
            bold: false,
            italic: false,
        }],
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&line).unwrap();
    let back: DiffLine = rkyv::from_bytes::<DiffLine, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(back.new_no, Some(7));
    assert_eq!(back.hunk, Some(2));
    assert!(matches!(back.kind, DiffKind::Add));
}
