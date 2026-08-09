use super::*;

#[test]
fn process_id_rkyv_roundtrip() {
    let original = ProcessId::new();
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
    let recovered =
        rkyv::from_bytes::<ProcessId, rkyv::rancor::Error>(&bytes).expect("deserialize");
    assert_eq!(original, recovered);
}

#[test]
fn process_id_display_and_parse_roundtrip() {
    let original = ProcessId::new();
    let s = original.to_string();
    let parsed: ProcessId = s.parse().expect("parse");
    assert_eq!(original, parsed);
}
