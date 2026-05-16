pub const SETTINGS_WEBVIEW_URL: &str = "vmux://settings/";
pub const SETTINGS_LIST_EVENT: &str = "settings_list";
pub const SETTINGS_SCHEMA_EVENT: &str = "settings_schema";

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SettingsListEvent {
    pub json: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SettingsCommandEvent {
    pub path: String,
    pub value: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SettingsSchemaEvent {
    pub json: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_list_event_rkyv_roundtrip() {
        let original = SettingsListEvent {
            json: r#"{"auto_update":true}"#.to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let decoded =
            rkyv::from_bytes::<SettingsListEvent, rkyv::rancor::Error>(&bytes).expect("de");
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
        let decoded =
            rkyv::from_bytes::<SettingsSchemaEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded, original);
    }
}
