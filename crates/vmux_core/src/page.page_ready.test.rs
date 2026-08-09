use super::*;

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
struct PageReadyPayloadProbe {}

#[test]
fn page_ready_cross_type_rkyv_compat() {
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&PageReadyPayloadProbe {}).expect("ser");
    println!("PageReady archive byte length: {}", bytes.len());
    println!("PageReady archive bytes: {:?}", &bytes[..]);
    let _decoded =
        rkyv::from_bytes::<PageReady, rkyv::rancor::Error>(&bytes).expect("cross-type decode");
}

#[test]
fn page_ready_self_rkyv_roundtrip() {
    let original = PageReady {};
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
    println!("PageReady self archive byte length: {}", bytes.len());
    let _decoded = rkyv::from_bytes::<PageReady, rkyv::rancor::Error>(&bytes).expect("self decode");
}
