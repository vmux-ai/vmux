use super::*;

#[test]
fn update_ready_event_rkyv_round_trips() {
    let evt = UpdateReadyEvent {
        version: "v9.9.9".to_string(),
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&evt).unwrap();
    let back = rkyv::from_bytes::<UpdateReadyEvent, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(back.version, "v9.9.9");
}

#[test]
fn update_progress_event_rkyv_round_trips() {
    let evt = UpdateProgressEvent {
        version: "0.0.20".to_string(),
        downloaded: 42,
        total: 100,
        installing: false,
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&evt).unwrap();
    let back = rkyv::from_bytes::<UpdateProgressEvent, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(back.version, "0.0.20");
    assert_eq!(back.downloaded, 42);
    assert_eq!(back.total, 100);
    assert!(!back.installing);
}

#[test]
fn event_ids_are_stable() {
    assert_eq!(UPDATE_READY_EVENT, "update-ready");
    assert_eq!(UPDATE_CLEARED_EVENT, "update-cleared");
    assert_eq!(UPDATE_PROGRESS_EVENT, "update-progress");
}
