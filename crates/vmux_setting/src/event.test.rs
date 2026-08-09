use super::*;

#[test]
fn settings_list_event_rkyv_roundtrip() {
    let original = SettingsListEvent {
        json: r#"{"auto_update":true}"#.to_string(),
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
    let decoded = rkyv::from_bytes::<SettingsListEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(decoded, original);
}

#[test]
fn settings_command_event_rkyv_roundtrip() {
    let original = SettingsCommandEvent {
        path: "layout.pane.gap".to_string(),
        value: "12.0".to_string(),
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
    let decoded =
        rkyv::from_bytes::<SettingsCommandEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(decoded, original);
}

#[test]
fn settings_schema_event_rkyv_roundtrip() {
    let original = SettingsSchemaEvent {
        json: r#"{"sections":[]}"#.to_string(),
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
    let decoded = rkyv::from_bytes::<SettingsSchemaEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(decoded, original);
}

#[test]
fn check_for_updates_event_rkyv_roundtrip() {
    let original = CheckForUpdatesEvent;
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
    let decoded =
        rkyv::from_bytes::<CheckForUpdatesEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(decoded, original);
}

#[test]
fn update_check_status_event_rkyv_roundtrip() {
    let original = UpdateCheckStatusEvent {
        status: UpdateCheckStatus::Downloading {
            version: "1.2.3".to_string(),
        },
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
    let decoded =
        rkyv::from_bytes::<UpdateCheckStatusEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(decoded, original);
}
